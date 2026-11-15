//! Shared runner plumbing for stage-walking job runners (the P1-05
//! simulated plan and the P3-03 real pipeline): the terminal chain
//!
//! ```text
//! FAILED/CANCELLED -> ROLLING_BACK -> ROLLED_BACK
//! ```
//!
//! with validated transitions and one persisted event per hop. Both
//! runners also do their *filesystem/registry* cleanup before claiming
//! `ROLLED_BACK`; this module only records the event chain.

use devkit_core::error::ErrorCode;

use crate::error::JobsError;
use crate::state::Stage;
use crate::store::JobStore;

/// `current -> terminal -> ROLLING_BACK -> ROLLED_BACK`, each with a
/// validated transition and a persisted event. Returns after the final
/// `ROLLED_BACK` event is committed. The three messages name the
/// rollback story per runner (simulated vs. real); parallel parameters
/// keep the two call sites one-liners.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_terminal_chain(
    store: &JobStore,
    job_id: &str,
    current: Stage,
    terminal: Stage,
    error_code: Option<ErrorCode>,
    terminal_message: &str,
    rollback_message: &str,
    rolled_back_message: &str,
) -> Result<(), JobsError> {
    if !current.can_transition(terminal) {
        return Err(JobsError::Internal(format!(
            "invalid state transition {current} -> {terminal}"
        )));
    }
    store.record_event(
        job_id,
        terminal,
        None,
        terminal_message,
        error_code.map(|code| code.as_str()),
    )?;

    if !terminal.can_transition(Stage::RollingBack) {
        return Err(JobsError::Internal(format!(
            "invalid state transition {terminal} -> ROLLING_BACK"
        )));
    }
    store.record_event(job_id, Stage::RollingBack, None, rollback_message, None)?;

    if !Stage::RollingBack.can_transition(Stage::RolledBack) {
        return Err(JobsError::Internal(
            "invalid state transition ROLLING_BACK -> ROLLED_BACK".to_string(),
        ));
    }
    store.record_event(job_id, Stage::RolledBack, None, rolled_back_message, None)?;
    Ok(())
}
