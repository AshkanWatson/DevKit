//! Simulated installation work (P1-05).
//!
//! Phase 1 replaces the Phase 3 installer adapters with a deterministic
//! plan of stage/progress steps. No downloads, no extraction, no
//! processes: the plan exercises the real state machine, event log,
//! persistence, cancellation, and rollback paths. Messages say
//! "(simulated)" so nobody mistakes them for real progress.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use devkit_core::error::ErrorCode;

use crate::error::JobsError;
use crate::runner::record_terminal_chain;
use crate::state::Stage;
use crate::store::JobStore;

/// One planned step: a stage event with optional progress fraction.
#[derive(Debug, Clone, PartialEq)]
pub struct SimulatedStep {
    pub stage: Stage,
    /// 0.0..=1.0, or `None` for an indeterminate stage (the UI must show
    /// an indeterminate indicator, never a fake percentage).
    pub progress: Option<f64>,
    pub message: String,
}

/// A deterministic simulated job plan.
#[derive(Debug, Clone)]
pub struct SimulatedPlan {
    steps: Vec<SimulatedStep>,
    /// When the runner reaches this stage, the job fails with the given
    /// stable error code (failure-injection for tests and demos).
    pub failure: Option<(Stage, ErrorCode)>,
    /// Pause between steps, pacing the simulation. `ZERO` runs as fast as
    /// possible (unit tests); the manager paces real jobs slightly so a
    /// cancellation request reliably lands mid-run.
    pub step_delay: Duration,
}

impl SimulatedPlan {
    /// A full install plan: RESOLVING..VALIDATING then COMPLETED, with
    /// measurable progress in DOWNLOADING and EXTRACTING.
    pub fn install(package_id: &str, version: &str) -> Self {
        let step = |stage: Stage, progress: Option<f64>, message: String| SimulatedStep {
            stage,
            progress,
            message,
        };
        let steps = vec![
            step(
                Stage::Resolving,
                None,
                format!("Resolving {package_id} {version} (simulated)"),
            ),
            step(
                Stage::Preparing,
                None,
                "Preparing staging directory (simulated)".to_string(),
            ),
            step(
                Stage::Downloading,
                Some(0.25),
                "Simulating download chunk 1/4".to_string(),
            ),
            step(
                Stage::Downloading,
                Some(0.5),
                "Simulating download chunk 2/4".to_string(),
            ),
            step(
                Stage::Downloading,
                Some(0.75),
                "Simulating download chunk 3/4".to_string(),
            ),
            step(
                Stage::Downloading,
                Some(1.0),
                "Simulating download chunk 4/4".to_string(),
            ),
            step(
                Stage::Verifying,
                None,
                "Verifying SHA-256 (simulated)".to_string(),
            ),
            step(
                Stage::Extracting,
                Some(0.33),
                "Simulating archive extraction 1/3".to_string(),
            ),
            step(
                Stage::Extracting,
                Some(0.67),
                "Simulating archive extraction 2/3".to_string(),
            ),
            step(
                Stage::Extracting,
                Some(1.0),
                "Simulating archive extraction 3/3".to_string(),
            ),
            step(
                Stage::Configuring,
                None,
                "Writing install receipt (simulated)".to_string(),
            ),
            step(
                Stage::Validating,
                None,
                "Validating installation (simulated)".to_string(),
            ),
            step(
                Stage::Completed,
                None,
                "Installation complete (simulated)".to_string(),
            ),
        ];
        Self {
            steps,
            failure: None,
            step_delay: Duration::ZERO,
        }
    }

    /// Inject a failure at the first event of `stage`.
    pub fn fail_at(mut self, stage: Stage, code: ErrorCode) -> Self {
        self.failure = Some((stage, code));
        self
    }

    /// Pace the simulation by sleeping this long before each step.
    pub fn with_step_delay(mut self, delay: Duration) -> Self {
        self.step_delay = delay;
        self
    }

    pub fn steps(&self) -> &[SimulatedStep] {
        &self.steps
    }
}

/// Outcome of running a simulated plan: the final job stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    RolledBack,
}

/// Drive a plan against the store, honoring cooperative cancellation and
/// automatic rollback. Runs synchronously; the manager calls it from a
/// worker thread, tests call it directly.
///
/// Every transition is validated against the state machine; a violation
/// is an [`JobsError::Internal`] (a runner bug), never a silent skip.
pub fn run(
    plan: &SimulatedPlan,
    store: &JobStore,
    job_id: &str,
    cancel: &AtomicBool,
) -> Result<RunOutcome, JobsError> {
    let mut current = Stage::Queued;

    for step in plan.steps() {
        // Pace the simulation (ZERO = run as fast as possible).
        if plan.step_delay > Duration::ZERO {
            std::thread::sleep(plan.step_delay);
        }
        // Cooperative cancellation: polled at every step boundary.
        if cancel.load(Ordering::Relaxed) {
            return finish_cancelled(store, job_id, current);
        }
        // Failure injection hits at the first event of the configured stage.
        if let Some((stage, code)) = plan.failure {
            if stage == step.stage {
                return finish_failed(store, job_id, current, code);
            }
        }
        // Staying in the same stage is not a state-machine transition —
        // it is another progress event within that stage.
        if step.stage != current && !current.can_transition(step.stage) {
            return Err(JobsError::Internal(format!(
                "invalid state transition {current} -> {}",
                step.stage
            )));
        }
        store.record_event(job_id, step.stage, step.progress, &step.message, None)?;
        current = step.stage;
    }

    if current != Stage::Completed {
        return Err(JobsError::Internal(format!(
            "plan ended in non-terminal state {current}"
        )));
    }
    Ok(RunOutcome::Completed)
}

fn finish_cancelled(
    store: &JobStore,
    job_id: &str,
    current: Stage,
) -> Result<RunOutcome, JobsError> {
    record_terminal_chain(
        store,
        job_id,
        current,
        Stage::Cancelled,
        None,
        "Cancelled at a safe boundary (simulated)",
        "Removing partial artifacts from staging (simulated)",
        "Managed root restored to pre-install state (simulated)",
    )?;
    Ok(RunOutcome::RolledBack)
}

fn finish_failed(
    store: &JobStore,
    job_id: &str,
    current: Stage,
    code: ErrorCode,
) -> Result<RunOutcome, JobsError> {
    record_terminal_chain(
        store,
        job_id,
        current,
        Stage::Failed,
        Some(code),
        &format!("Simulated failure at stage boundary ({code})"),
        "Removing partial artifacts from staging (simulated)",
        "Managed root restored to pre-install state (simulated)",
    )?;
    Ok(RunOutcome::RolledBack)
}

/// Recover a job orphaned by a hard process death: mark it `FAILED` with
/// the given stable code and run the rollback chain. The job was not
/// running, so there is nothing to cancel — this only closes the state
/// machine and leaves the history for diagnosis (retry = new job).
pub fn recover_interrupted(
    store: &JobStore,
    job_id: &str,
    code: ErrorCode,
) -> Result<(), JobsError> {
    let snapshot = store
        .get_job(job_id)?
        .ok_or_else(|| JobsError::Internal(format!("job {job_id} vanished during recovery")))?;
    let current = Stage::parse(&snapshot.state).ok_or_else(|| {
        JobsError::Internal(format!("job {job_id} has unknown state {}", snapshot.state))
    })?;
    if current.is_terminal() {
        return Ok(());
    }
    record_terminal_chain(
        store,
        job_id,
        current,
        Stage::Failed,
        Some(code),
        "Job interrupted by process restart (simulated)",
        "Removing partial artifacts from staging (simulated)",
        "Managed root restored to pre-install state (simulated)",
    )?;
    Ok(())
}
