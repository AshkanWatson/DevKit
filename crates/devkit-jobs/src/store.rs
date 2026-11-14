//! Persistence of jobs and events (`installation_jobs`,
//! `installation_events`; docs/architecture/local-database.md §2).
//!
//! Every state transition writes the event row and the job row update in
//! one transaction ("all multi-row writes (job + events) are
//! transactional", §4). `seq` is derived from `MAX(seq)` inside the same
//! transaction; each job is written by a single thread, so sequences are
//! monotonic.

use rusqlite::{params, Connection};

use crate::error::JobsError;
use crate::events::JobEvent;
use crate::state::Stage;

/// Job kinds allowed by the `installation_jobs.kind` CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Install,
    Uninstall,
    Activate,
    ProfileApply,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JobKind::Install => "install",
            JobKind::Uninstall => "uninstall",
            JobKind::Activate => "activate",
            JobKind::ProfileApply => "profile_apply",
        }
    }
}

/// A job row snapshot (`get_job` per the bridge contract).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct JobSnapshot {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    /// Stage wire form.
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    /// Stable error code string on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

pub struct JobStore<'a> {
    conn: &'a Connection,
}

impl<'a> JobStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Create a job row in `QUEUED` and record its first event (seq 1).
    pub fn create_job(
        &self,
        job_id: &str,
        kind: JobKind,
        package_id: Option<&str>,
        version: Option<&str>,
        profile_id: Option<&str>,
    ) -> Result<(), JobsError> {
        let now = devkit_registry::error::now_utc_rfc3339()?;
        self.conn.execute(
            "INSERT INTO installation_jobs
                (id, kind, package_id, version, profile_id, state, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'QUEUED', ?6, ?6)",
            params![job_id, kind.as_str(), package_id, version, profile_id, now],
        )?;
        self.record_event(
            job_id,
            Stage::Queued,
            None,
            &format!("Job queued ({})", kind.as_str()),
            None,
        )?;
        Ok(())
    }

    /// Record one event and the resulting job state in one transaction.
    /// Terminal stages stamp `finished_at` (first one wins via COALESCE);
    /// `error_code` is set once and never overwritten.
    pub fn record_event(
        &self,
        job_id: &str,
        stage: Stage,
        progress: Option<f64>,
        message: &str,
        error_code: Option<&str>,
    ) -> Result<JobEvent, JobsError> {
        if let Some(progress) = progress {
            if !(0.0..=1.0).contains(&progress) {
                return Err(JobsError::Internal(format!(
                    "progress {progress} outside 0.0..=1.0"
                )));
            }
        }
        let timestamp = devkit_registry::error::now_utc_rfc3339()?;

        let tx = self.conn.unchecked_transaction()?;
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM installation_events WHERE job_id = ?1",
            params![job_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO installation_events
                (job_id, seq, timestamp, stage, progress, message, error_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                job_id,
                seq,
                timestamp,
                stage.as_str(),
                progress,
                message,
                error_code,
            ],
        )?;
        tx.execute(
            "UPDATE installation_jobs
             SET state = ?2,
                 updated_at = ?3,
                 finished_at = COALESCE(finished_at, ?4),
                 error_code = COALESCE(error_code, ?5)
             WHERE id = ?1",
            params![
                job_id,
                stage.as_str(),
                timestamp,
                if stage.is_terminal() {
                    Some(&timestamp)
                } else {
                    None
                },
                error_code,
            ],
        )?;
        tx.commit()?;

        Ok(JobEvent {
            job_id: job_id.to_string(),
            seq,
            timestamp,
            stage: stage.as_str().to_string(),
            progress,
            message: message.to_string(),
            error_code: error_code.map(|c| c.to_string()),
        })
    }

    /// Load a job snapshot.
    pub fn get_job(&self, job_id: &str) -> Result<Option<JobSnapshot>, JobsError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, package_id, version, profile_id, state, created_at,
                    updated_at, finished_at, error_code
             FROM installation_jobs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![job_id], |row| {
            Ok(JobSnapshot {
                id: row.get(0)?,
                kind: row.get(1)?,
                package_id: row.get(2)?,
                version: row.get(3)?,
                profile_id: row.get(4)?,
                state: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                finished_at: row.get(8)?,
                error_code: row.get(9)?,
            })
        })?;
        match rows.next() {
            None => Ok(None),
            Some(row) => Ok(Some(row?)),
        }
    }

    /// All jobs, newest first (for the Downloads screen).
    pub fn list_jobs(&self) -> Result<Vec<JobSnapshot>, JobsError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, package_id, version, profile_id, state, created_at,
                    updated_at, finished_at, error_code
             FROM installation_jobs ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(JobSnapshot {
                id: row.get(0)?,
                kind: row.get(1)?,
                package_id: row.get(2)?,
                version: row.get(3)?,
                profile_id: row.get(4)?,
                state: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                finished_at: row.get(8)?,
                error_code: row.get(9)?,
            })
        })?;
        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row?);
        }
        Ok(jobs)
    }

    /// Events of a job with `seq > last_seen_seq`, ordered — the persisted
    /// event log that `watch_job_events` replays after UI restart.
    pub fn events_since(
        &self,
        job_id: &str,
        last_seen_seq: i64,
    ) -> Result<Vec<JobEvent>, JobsError> {
        let mut stmt = self.conn.prepare(
            "SELECT job_id, seq, timestamp, stage, progress, message, error_code
             FROM installation_events
             WHERE job_id = ?1 AND seq > ?2
             ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![job_id, last_seen_seq], |row| {
            Ok(JobEvent {
                job_id: row.get(0)?,
                seq: row.get(1)?,
                timestamp: row.get(2)?,
                stage: row.get(3)?,
                progress: row.get(4)?,
                message: row.get(5)?,
                error_code: row.get(6)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Highest recorded `seq` for a job (0 when unknown).
    pub fn latest_seq(&self, job_id: &str) -> Result<i64, JobsError> {
        let seq = self.conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM installation_events WHERE job_id = ?1",
            params![job_id],
            |row| row.get(0),
        )?;
        Ok(seq)
    }

    /// All jobs still in a non-terminal state. After a hard process death
    /// these are orphans; the manager recovers them on startup
    /// (cooperative shutdown means live threads are normally joined first).
    pub fn non_terminal_jobs(&self) -> Result<Vec<JobSnapshot>, JobsError> {
        let jobs = self.list_jobs()?;
        Ok(jobs
            .into_iter()
            .filter(|job| {
                Stage::parse(&job.state)
                    .map(|stage| !stage.is_terminal())
                    .unwrap_or(true)
            })
            .collect())
    }
}
