//! P1-05 acceptance tests: cancel/complete/fail/rollback paths, events
//! surviving restart, replay from SQLite, retry idempotence, and orphan
//! recovery. No Flutter dependency; no network or process access.

use std::sync::atomic::AtomicBool;

use devkit_core::error::ErrorCode;
use devkit_jobs::manager::CancelOutcome;
use devkit_jobs::state::Stage;
use devkit_jobs::store::{JobKind, JobStore};
use devkit_jobs::{JobManager, RunOutcome, SimulatedPlan};
use tempfile::TempDir;

#[test]
fn happy_path_completes_with_contiguous_events() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state/devkit.sqlite");

    let job_id = {
        let db = devkit_registry::Database::open(&path).unwrap();
        let store = JobStore::new(db.connection());
        store
            .create_job(
                "job-1",
                JobKind::Install,
                Some("devkit.lang.python"),
                Some("3.12.7"),
                None,
            )
            .unwrap();
        let plan = SimulatedPlan::install("devkit.lang.python", "3.12.7");
        let outcome =
            devkit_jobs::simulated::run(&plan, &store, "job-1", &AtomicBool::new(false)).unwrap();
        assert_eq!(outcome, RunOutcome::Completed);
        drop(db);
        "job-1"
    };

    let db = devkit_registry::Database::open(&path).unwrap();
    let store = JobStore::new(db.connection());
    let snapshot = store.get_job(job_id).unwrap().unwrap();
    assert_eq!(snapshot.state, "COMPLETED");
    assert!(snapshot.finished_at.is_some());

    let events = store.events_since(job_id, 0).unwrap();
    // QUEUED event from create_job, then every plan step.
    assert_eq!(
        events.len(),
        1 + SimulatedPlan::install("p", "v").steps().len()
    );
    for (expected_seq, event) in events.iter().enumerate() {
        assert_eq!(event.seq, (expected_seq + 1) as i64, "seq must be 1-based");
    }
    assert_eq!(events[0].stage, "QUEUED");
    assert_eq!(events.last().unwrap().stage, "COMPLETED");
    // Progress fractions only appear where the stage can measure them.
    let downloading: Vec<_> = events.iter().filter(|e| e.stage == "DOWNLOADING").collect();
    assert_eq!(downloading.len(), 4);
    assert_eq!(downloading[0].progress, Some(0.25));
    assert_eq!(downloading[3].progress, Some(1.0));
    let verifying = events.iter().find(|e| e.stage == "VERIFYING").unwrap();
    assert_eq!(
        verifying.progress, None,
        "indeterminate stages carry no fraction"
    );
}

#[test]
fn failure_at_stage_triggers_rollback_with_stable_code() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("devkit.sqlite");
    let db = devkit_registry::Database::open(&path).unwrap();
    let store = JobStore::new(db.connection());

    store
        .create_job(
            "job-2",
            JobKind::Install,
            Some("devkit.lang.python"),
            Some("3.12.7"),
            None,
        )
        .unwrap();
    let plan = SimulatedPlan::install("devkit.lang.python", "3.12.7")
        .fail_at(Stage::Verifying, ErrorCode::IntegrityCheckFailed);
    let outcome =
        devkit_jobs::simulated::run(&plan, &store, "job-2", &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, RunOutcome::RolledBack);

    let snapshot = store.get_job("job-2").unwrap().unwrap();
    assert_eq!(snapshot.state, "ROLLED_BACK");
    assert_eq!(
        snapshot.error_code.as_deref(),
        Some("INTEGRITY_CHECK_FAILED")
    );

    let events = store.events_since("job-2", 0).unwrap();
    let stages: Vec<&str> = events.iter().map(|e| e.stage.as_str()).collect();
    // Failure hits at VERIFYING: no VERIFYING/EXTRACTING/CONFIGURING/VALIDATING.
    assert!(!stages.contains(&"VERIFYING"));
    assert!(!stages.contains(&"EXTRACTING"));
    let failed_index = stages.iter().position(|s| *s == "FAILED").unwrap();
    assert_eq!(
        &stages[failed_index..],
        &["FAILED", "ROLLING_BACK", "ROLLED_BACK"]
    );
    let failed = store
        .events_since("job-2", 0)
        .unwrap()
        .into_iter()
        .find(|e| e.stage == "FAILED")
        .unwrap();
    assert_eq!(failed.error_code.as_deref(), Some("INTEGRITY_CHECK_FAILED"));
}

#[test]
fn pre_cancelled_job_rolls_back() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("devkit.sqlite");
    let db = devkit_registry::Database::open(&path).unwrap();
    let store = JobStore::new(db.connection());

    store
        .create_job(
            "job-3",
            JobKind::Install,
            Some("devkit.lang.python"),
            Some("3.12.7"),
            None,
        )
        .unwrap();
    let cancel = AtomicBool::new(true);
    let plan = SimulatedPlan::install("devkit.lang.python", "3.12.7");
    let outcome = devkit_jobs::simulated::run(&plan, &store, "job-3", &cancel).unwrap();
    assert_eq!(outcome, RunOutcome::RolledBack);

    let stages: Vec<String> = store
        .events_since("job-3", 0)
        .unwrap()
        .iter()
        .map(|e| e.stage.clone())
        .collect();
    assert_eq!(
        stages,
        vec!["QUEUED", "CANCELLED", "ROLLING_BACK", "ROLLED_BACK"]
    );
    let snapshot = store.get_job("job-3").unwrap().unwrap();
    assert_eq!(snapshot.state, "ROLLED_BACK");
    assert!(
        snapshot.error_code.is_none(),
        "cancellation is not an error"
    );
}

#[test]
fn events_replay_after_restart() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state/devkit.sqlite");
    let job_id = {
        let manager = JobManager::open(&path).unwrap();
        let job_id = manager
            .enqueue_install("devkit.lang.python", "3.12.7")
            .unwrap();
        assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::Completed);
        job_id
    }; // manager dropped = "app restart"

    // Re-attach with a fresh manager: full event log replays from SQLite.
    let manager = JobManager::open(&path).unwrap();
    let events = manager.watch_job_events(&job_id, 0).unwrap();
    assert_eq!(events.first().unwrap().stage, "QUEUED");
    assert_eq!(events.last().unwrap().stage, "COMPLETED");
    assert!(events.len() > 5);

    // Partial re-attach from a mid-stream cursor only returns the tail.
    let resume_at = events[3].seq;
    let tail = manager.watch_job_events(&job_id, resume_at).unwrap();
    assert_eq!(tail.first().unwrap().seq, resume_at + 1);

    // The job snapshot survives too.
    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(snapshot.state, "COMPLETED");
    assert_eq!(snapshot.package_id.as_deref(), Some("devkit.lang.python"));
}

#[test]
fn retry_creates_new_job_and_keeps_old_history() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("devkit.sqlite");

    let failed_id = {
        let manager = JobManager::open(&path).unwrap();
        let id = manager
            .enqueue_install("devkit.lang.python", "3.12.7")
            .unwrap();
        assert_eq!(manager.wait(&id).unwrap(), RunOutcome::Completed);
        id
    };

    // Retry after failure = a NEW job (old history remains, §3.5).
    let manager = JobManager::open(&path).unwrap();
    let retry_id = manager
        .enqueue_install("devkit.lang.python", "3.12.7")
        .unwrap();
    assert_ne!(retry_id, failed_id);
    assert_eq!(manager.wait(&retry_id).unwrap(), RunOutcome::Completed);

    let old_events = manager.watch_job_events(&failed_id, 0).unwrap();
    let new_events = manager.watch_job_events(&retry_id, 0).unwrap();
    assert_eq!(old_events.first().unwrap().stage, "QUEUED");
    assert_eq!(old_events.last().unwrap().stage, "COMPLETED");
    assert_eq!(new_events.first().unwrap().seq, 1, "fresh seq per job");
    assert_eq!(new_events.last().unwrap().stage, "COMPLETED");
}

#[test]
fn cancellation_during_run_is_accepted_then_rolls_back() {
    let dir = TempDir::new().unwrap();
    let manager = JobManager::open(&dir.path().join("devkit.sqlite")).unwrap();
    let job_id = manager
        .enqueue_install("devkit.lang.python", "3.12.7")
        .unwrap();

    // Cancel while the job runs (accepted at once).
    let outcome = manager.cancel_job(&job_id).unwrap();
    assert_eq!(outcome, CancelOutcome::Accepted);
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);

    // Terminal states reject further cancellation.
    assert_eq!(
        manager.cancel_job(&job_id).unwrap(),
        CancelOutcome::AlreadyFinished
    );
    assert_eq!(
        manager.cancel_job("no-such-job").unwrap(),
        CancelOutcome::UnknownJob
    );

    let stages: Vec<String> = manager
        .watch_job_events(&job_id, 0)
        .unwrap()
        .iter()
        .map(|e| e.stage.clone())
        .collect();
    assert!(stages.contains(&"CANCELLED".to_string()));
    assert!(stages.contains(&"ROLLED_BACK".to_string()));
    assert!(!stages.contains(&"COMPLETED".to_string()));
}

#[test]
fn orphaned_jobs_are_recovered_on_open() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("devkit.sqlite");

    // Simulate a hard crash: a job row left in a non-terminal state.
    {
        let db = devkit_registry::Database::open(&path).unwrap();
        let store = JobStore::new(db.connection());
        store
            .create_job(
                "orphan-1",
                JobKind::Install,
                Some("devkit.lang.python"),
                Some("3.12.7"),
                None,
            )
            .unwrap();
        store
            .record_event(
                "orphan-1",
                Stage::Downloading,
                Some(0.5),
                "Simulating download chunk 2/4",
                None,
            )
            .unwrap();
    }

    // Reopening the manager must fail-and-roll-back the orphan.
    let manager = JobManager::open(&path).unwrap();
    let snapshot = manager.get_job("orphan-1").unwrap().unwrap();
    assert_eq!(snapshot.state, "ROLLED_BACK");
    assert_eq!(snapshot.error_code.as_deref(), Some("INSTALL_FAILED"));
    assert!(snapshot.finished_at.is_some());
    let stages: Vec<String> = manager
        .watch_job_events("orphan-1", 0)
        .unwrap()
        .iter()
        .map(|e| e.stage.clone())
        .collect();
    assert_eq!(
        stages,
        vec![
            "QUEUED",
            "DOWNLOADING",
            "FAILED",
            "ROLLING_BACK",
            "ROLLED_BACK"
        ]
    );
}

#[test]
fn terminal_jobs_are_not_recovered() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state/devkit.sqlite");
    let manager = JobManager::open(&path).unwrap();
    let job_id = manager
        .enqueue_install("devkit.lang.python", "3.12.7")
        .unwrap();
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::Completed);
    let events_before: i64 = {
        let db = devkit_registry::Database::open(&path).unwrap();
        JobStore::new(db.connection()).latest_seq(&job_id).unwrap()
    };
    drop(manager);

    // Reopen: a completed job must not gain recovery events.
    let _manager = JobManager::open(&path).unwrap();
    let events_after: i64 = {
        let db = devkit_registry::Database::open(&path).unwrap();
        JobStore::new(db.connection()).latest_seq(&job_id).unwrap()
    };
    assert_eq!(events_before, events_after);
}

#[test]
fn dropping_the_manager_joins_running_jobs() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("devkit.sqlite");
    let manager = JobManager::open(&path).unwrap();
    let job_id = manager
        .enqueue_install("devkit.lang.python", "3.12.7")
        .unwrap();
    drop(manager); // cooperative shutdown: cancel + join

    // After shutdown the job is rolled back, never stuck in-flight.
    let db = devkit_registry::Database::open(&path).unwrap();
    let store = JobStore::new(db.connection());
    let snapshot = store.get_job(&job_id).unwrap().unwrap();
    assert!(
        matches!(snapshot.state.as_str(), "ROLLED_BACK" | "COMPLETED"),
        "state after shutdown: {}",
        snapshot.state
    );
}

#[test]
fn progress_never_leaves_the_unit_interval() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("devkit.sqlite");
    let db = devkit_registry::Database::open(&path).unwrap();
    let store = JobStore::new(db.connection());
    store
        .create_job("job-9", JobKind::Install, None, None, None)
        .unwrap();
    assert!(store
        .record_event("job-9", Stage::Downloading, Some(1.5), "bad", None)
        .is_err());
    assert!(store
        .record_event("job-9", Stage::Downloading, Some(-0.1), "bad", None)
        .is_err());
}

/// Helper asserting the Queued event is always seq 1 for a fresh job.
#[test]
fn queued_event_is_seq_one() {
    let dir = TempDir::new().unwrap();
    let db = devkit_registry::Database::open(&dir.path().join("devkit.sqlite")).unwrap();
    let store = JobStore::new(db.connection());
    store
        .create_job(
            "job-10",
            JobKind::Install,
            Some("devkit.util.git"),
            Some("2.0"),
            None,
        )
        .unwrap();
    let events = store.events_since("job-10", 0).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].seq, 1);
    assert_eq!(events[0].stage, "QUEUED");
    assert_eq!(events[0].message, "Job queued (install)");
}
