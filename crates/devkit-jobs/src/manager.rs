//! The job manager: owns running jobs, cooperative cancellation, and the
//! persisted event log (docs/ffi/bridge-contract.md § Job independence
//! from Flutter lifecycle).
//!
//! - Jobs run on their own threads, writing events to SQLite; closing a
//!   screen never affects them, and the UI re-attaches via
//!   [`JobManager::watch_job_events`].
//! - `cancel_job` is cooperative: the flag is polled at stage boundaries.
//! - Dropping the manager joins its threads (cooperative shutdown), and
//!   opening one recovers jobs orphaned by a hard process death.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use devkit_core::error::ErrorCode;

use crate::error::JobsError;
use crate::events::JobEvent;
use crate::simulated::{self, RunOutcome, SimulatedPlan};
use crate::state::Stage;
use crate::store::{JobKind, JobSnapshot, JobStore};

/// Result of `cancel_job` (bridge contract § Cancellation semantics: the
/// result confirms whether the request was accepted, not whether the job
/// has already stopped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// Cancellation was requested; the job stops at the next boundary.
    Accepted,
    /// The job is already terminal — reject with `INVALID_ARGUMENT` at the
    /// FFI layer.
    AlreadyFinished,
    /// No such job.
    UnknownJob,
}

pub struct JobManager {
    db_path: PathBuf,
    /// Cooperative cancel flags of running jobs.
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
    handles: Mutex<HashMap<String, std::thread::JoinHandle<()>>>,
}

impl JobManager {
    /// Open (and migrate) the database this manager persists to, and
    /// recover jobs orphaned by a hard process death.
    pub fn open(path: &Path) -> Result<Self, JobsError> {
        // Migrations run here as well; the per-thread connections created
        // later reuse the migrated schema.
        devkit_registry::Database::open(path)?;
        let manager = Self {
            db_path: path.to_path_buf(),
            running: Mutex::new(HashMap::new()),
            handles: Mutex::new(HashMap::new()),
        };
        manager.recover_orphaned_jobs()?;
        Ok(manager)
    }

    /// Create and start a simulated install job. Returns the job ID.
    /// The job keeps running independently of any caller lifecycle.
    pub fn enqueue_install(&self, package_id: &str, version: &str) -> Result<String, JobsError> {
        let job_id = uuid::Uuid::new_v4().to_string();
        let plan = SimulatedPlan::install(package_id, version)
            // Pace steps so a cancellation request lands mid-run instead of
            // racing the (very fast) simulated work.
            .with_step_delay(std::time::Duration::from_millis(10));

        {
            let db = devkit_registry::Database::open(&self.db_path)?;
            JobStore::new(db.connection()).create_job(
                &job_id,
                JobKind::Install,
                Some(package_id),
                Some(version),
                None,
            )?;
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.running
            .lock()
            .expect("running map lock")
            .insert(job_id.clone(), cancel.clone());

        let db_path = self.db_path.clone();
        let thread_job_id = job_id.clone();
        let handle = std::thread::spawn(move || {
            // Each job owns its connection; WAL allows concurrent readers.
            let db = match devkit_registry::Database::open(&db_path) {
                Ok(db) => db,
                Err(_) => return, // Recovery on next open marks it FAILED.
            };
            let store = JobStore::new(db.connection());
            let _ = simulated::run(&plan, &store, &thread_job_id, &cancel);
        });
        self.handles
            .lock()
            .expect("handles map lock")
            .insert(job_id.clone(), handle);
        Ok(job_id)
    }

    /// Request cooperative cancellation. Accepted jobs stop at the next
    /// stage boundary and roll back. Terminal jobs reject cancellation.
    pub fn cancel_job(&self, job_id: &str) -> Result<CancelOutcome, JobsError> {
        if let Some(flag) = self.running.lock().expect("running map lock").get(job_id) {
            flag.store(true, Ordering::Relaxed);
            return Ok(CancelOutcome::Accepted);
        }
        let db = devkit_registry::Database::open(&self.db_path)?;
        let snapshot = JobStore::new(db.connection()).get_job(job_id)?;
        match snapshot {
            None => Ok(CancelOutcome::UnknownJob),
            Some(job) => {
                let terminal = Stage::parse(&job.state)
                    .map(|stage| stage.is_terminal())
                    .unwrap_or(false);
                if terminal {
                    Ok(CancelOutcome::AlreadyFinished)
                } else {
                    // Non-terminal but not owned by this manager instance:
                    // only possible if the process died and recovery has
                    // not run; treat like a request that cannot be honored.
                    Ok(CancelOutcome::AlreadyFinished)
                }
            }
        }
    }

    /// Wait for a job's thread to finish (used by tests and graceful
    /// shutdown; the UI instead polls the event log).
    pub fn wait(&self, job_id: &str) -> Result<RunOutcome, JobsError> {
        let handle = self
            .handles
            .lock()
            .expect("handles map lock")
            .remove(job_id);
        self.running
            .lock()
            .expect("running map lock")
            .remove(job_id);
        match handle {
            None => Err(JobsError::Internal(format!(
                "job {job_id} is not owned by this manager"
            ))),
            Some(handle) => {
                handle.join().map_err(|_| {
                    JobsError::Internal(format!("job thread for {job_id} panicked"))
                })?;
                let db = devkit_registry::Database::open(&self.db_path)?;
                let snapshot = JobStore::new(db.connection())
                    .get_job(job_id)?
                    .ok_or_else(|| JobsError::Internal(format!("job {job_id} vanished")))?;
                match Stage::parse(&snapshot.state) {
                    Some(Stage::Completed) => Ok(RunOutcome::Completed),
                    _ => Ok(RunOutcome::RolledBack),
                }
            }
        }
    }

    /// Job snapshot from the persisted store (`get_job` in the contract).
    pub fn get_job(&self, job_id: &str) -> Result<Option<JobSnapshot>, JobsError> {
        let db = devkit_registry::Database::open(&self.db_path)?;
        JobStore::new(db.connection()).get_job(job_id)
    }

    /// Events with `seq > last_seen_seq` from the persisted log
    /// (`watch_job_events` in the contract; re-attachable after UI restart).
    pub fn watch_job_events(
        &self,
        job_id: &str,
        last_seen_seq: i64,
    ) -> Result<Vec<JobEvent>, JobsError> {
        let db = devkit_registry::Database::open(&self.db_path)?;
        JobStore::new(db.connection()).events_since(job_id, last_seen_seq)
    }

    /// Recover jobs orphaned by a hard process death: non-terminal jobs
    /// are marked `FAILED` (`INSTALL_FAILED`) and rolled back, so the
    /// manager never leaves a job stuck in `DOWNLOADING` forever
    /// (installation-lifecycle.md §5, idempotent retry rule §3.5).
    fn recover_orphaned_jobs(&self) -> Result<(), JobsError> {
        let db = devkit_registry::Database::open(&self.db_path)?;
        let store = JobStore::new(db.connection());
        for job in store.non_terminal_jobs()? {
            simulated::recover_interrupted(&store, &job.id, ErrorCode::InstallFailed)?;
        }
        Ok(())
    }
}

impl Drop for JobManager {
    fn drop(&mut self) {
        // Cooperative shutdown: ask every running job to cancel, then wait
        // for the threads to record their final (rolled-back) state.
        let flags: Vec<Arc<AtomicBool>> = self
            .running
            .lock()
            .expect("running map lock")
            .values()
            .cloned()
            .collect();
        for flag in flags {
            flag.store(true, Ordering::Relaxed);
        }
        let handles: Vec<std::thread::JoinHandle<()>> = self
            .handles
            .lock()
            .expect("handles map lock")
            .drain()
            .map(|(_, handle)| handle)
            .collect();
        for handle in handles {
            let _ = handle.join();
        }
    }
}
