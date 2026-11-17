//! Job operations (P3-05): the install/job center over the real
//! `devkit-jobs` engine (docs/ffi/bridge-contract.md). Jobs live in the
//! Rust job manager and persist to SQLite, so they never depend on a
//! Flutter widget being mounted; streams replay from the persisted
//! event log (`watch_job_events(job_id, last_seen_seq)`).
//!
//! `watch_jobs` additionally feeds the Downloads screen's list stream:
//! a poller emits the full snapshot (active jobs first, then history)
//! whenever anything changed in SQLite. Cancellation is cooperative;
//! terminal states reject it.

use std::time::Duration;

use devkit_download::ReqwestDownloadHttp;
use devkit_install::{ArchiveLimits, ProcessExeRunner};
use devkit_jobs::pipeline::{RealInstallConfig, RegistrySource};
use devkit_jobs::{CancelOutcome, Stage};
use devkit_registry::package_cache::PackageCache;

use crate::frb_generated::StreamSink;

use super::engine::{engine, managed_root, open_db};
use super::error::{bridge_error, from_jobs_error, BridgeError, BridgeErrorCode};

/// One entry of a job's structured event log (contract § Event model).
#[derive(Debug, Clone)]
pub struct JobEventDto {
    pub job_id: String,
    /// Monotonically increasing, 1-based, per job.
    pub seq: i64,
    /// UTC ISO-8601.
    pub timestamp: String,
    /// Stage wire form (`"DOWNLOADING"`, ...).
    pub stage: String,
    /// 0.0..1.0 where measurable; null = indeterminate (the UI must show
    /// an indeterminate stage, never a fake percentage).
    pub progress: Option<f64>,
    /// Safe for display and logs.
    pub message: String,
    /// Stable code string on failure events.
    pub error_code: Option<String>,
}

/// One job snapshot (list/detail form for the Downloads screen).
#[derive(Debug, Clone)]
pub struct JobDto {
    pub id: String,
    pub package_id: Option<String>,
    /// Display name resolved from the catalog cache (null → use the ID).
    pub package_name: Option<String>,
    pub version: Option<String>,
    /// Current stage wire form.
    pub state: String,
    /// Stable error code string on failure.
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    /// Event log tail (newest last).
    pub events: Vec<JobEventDto>,
    /// Last progress fraction of the current stage, when measurable.
    pub progress: Option<f64>,
}

/// Cancellation request result (contract § Cancellation semantics: this
/// confirms the request was accepted, not that the job has stopped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelJobResult {
    Accepted,
    AlreadyFinished,
    UnknownJob,
}

const EVENT_TAIL: usize = 40;

fn event_dto(event: devkit_jobs::JobEvent) -> JobEventDto {
    JobEventDto {
        job_id: event.job_id,
        seq: event.seq,
        timestamp: event.timestamp,
        stage: event.stage,
        progress: event.progress,
        message: event.message,
        error_code: event.error_code,
    }
}

fn job_dto(
    snapshot: devkit_jobs::JobSnapshot,
    events: Vec<devkit_jobs::JobEvent>,
    package_name: Option<String>,
) -> JobDto {
    let tail = if events.len() > EVENT_TAIL {
        events[events.len() - EVENT_TAIL..].to_vec()
    } else {
        events
    };
    let stage = snapshot.state.clone();
    let progress = tail
        .iter()
        .rev()
        .find(|event| event.stage == stage)
        .and_then(|event| event.progress);
    JobDto {
        id: snapshot.id,
        package_id: snapshot.package_id,
        package_name,
        version: snapshot.version,
        state: snapshot.state,
        error_code: snapshot.error_code,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
        finished_at: snapshot.finished_at,
        events: tail.into_iter().map(event_dto).collect(),
        progress,
    }
}

/// Starts a real install: the pipeline resolves the artifact from the
/// catalog cache, downloads it (HTTPS-only, redirect policy), verifies
/// SHA-256, extracts into staging, registers the receipt, validates,
/// and completes — with rollback on any failure. Returns the job ID.
pub fn start_install(package_id: String, version: String) -> Result<String, BridgeError> {
    if package_id.trim().is_empty() || version.trim().is_empty() {
        return Err(bridge_error(
            BridgeErrorCode::InvalidArgument,
            "package id and version are required",
        ));
    }
    let state = engine()?;
    let source = RegistrySource::new(open_db()?);
    let config = RealInstallConfig {
        managed_root: managed_root()?.to_path_buf(),
        transport: ReqwestDownloadHttp::new()
            .map_err(|error| bridge_error(BridgeErrorCode::NetworkError, error.to_string()))?,
        source,
        limits: ArchiveLimits::default(),
        vendor_runner: Box::new(ProcessExeRunner),
        validation_hook: None,
    };
    state
        .manager
        .enqueue_real_install(config, &package_id, &version)
        .map_err(from_jobs_error)
}

/// Requests cooperative cancellation (contract § Cancellation
/// semantics); the job stops at the next safe boundary and rolls back.
pub fn cancel_job(job_id: String) -> Result<CancelJobResult, BridgeError> {
    let state = engine()?;
    Ok(
        match state.manager.cancel_job(&job_id).map_err(from_jobs_error)? {
            CancelOutcome::Accepted => CancelJobResult::Accepted,
            CancelOutcome::AlreadyFinished => CancelJobResult::AlreadyFinished,
            CancelOutcome::UnknownJob => CancelJobResult::UnknownJob,
        },
    )
}

/// One job snapshot from the persisted store (null when unknown).
pub fn get_job(job_id: String) -> Result<Option<JobDto>, BridgeError> {
    let db = open_db()?;
    let snapshot = devkit_jobs::JobStore::new(db.connection())
        .get_job(&job_id)
        .map_err(from_jobs_error)?;
    Ok(snapshot.map(|snapshot| {
        let (events, name) = job_context(&snapshot);
        job_dto(snapshot, events, name)
    }))
}

/// All jobs, newest first (active jobs surface through `state`).
pub fn list_jobs() -> Result<Vec<JobDto>, BridgeError> {
    snapshot_all()
}

/// Events with `seq > last_seen_seq` — the persisted replay the
/// contract's `watch_job_events` re-attaches with after a UI restart.
pub fn get_job_events(job_id: String, last_seen_seq: i64) -> Result<Vec<JobEventDto>, BridgeError> {
    let db = open_db()?;
    let store = devkit_jobs::JobStore::new(db.connection());
    Ok(store
        .events_since(&job_id, last_seen_seq)
        .map_err(from_jobs_error)?
        .into_iter()
        .map(event_dto)
        .collect())
}

/// (events, catalog display name) for one job snapshot.
fn job_context(
    snapshot: &devkit_jobs::JobSnapshot,
) -> (Vec<devkit_jobs::JobEvent>, Option<String>) {
    let mut events = Vec::new();
    let mut package_name = None;
    if let Ok(db) = open_db() {
        let conn = db.connection();
        if let Ok(store_events) = devkit_jobs::JobStore::new(conn).events_since(&snapshot.id, 0) {
            events = store_events;
        }
        if let Some(package_id) = &snapshot.package_id {
            package_name = PackageCache::new(conn)
                .get(package_id)
                .ok()
                .flatten()
                .map(|cached| cached.manifest.name);
        }
    }
    (events, package_name)
}

/// Full list snapshot: active jobs first, then history (the Downloads
/// screen's ordering, mirroring the Phase 1 fake).
fn snapshot_all() -> Result<Vec<JobDto>, BridgeError> {
    let db = open_db()?;
    let conn = db.connection();
    let store = devkit_jobs::JobStore::new(conn);
    let snapshots = store.list_jobs().map_err(from_jobs_error)?;
    let cache = PackageCache::new(conn);
    let mut jobs: Vec<JobDto> = snapshots
        .into_iter()
        .map(|snapshot| {
            let events = store.events_since(&snapshot.id, 0).unwrap_or_default();
            let package_name = snapshot
                .package_id
                .as_deref()
                .and_then(|id| cache.get(id).ok().flatten())
                .map(|cached| cached.manifest.name);
            job_dto(snapshot, events, package_name)
        })
        .collect();
    jobs.sort_by_key(|job| {
        let terminal = Stage::parse(&job.state)
            .map(Stage::is_terminal)
            .unwrap_or(true);
        (terminal, std::cmp::Reverse(job.created_at.clone()))
    });
    Ok(jobs)
}

/// Live list stream for the Downloads screen: emits the full snapshot
/// whenever anything changed in the persisted store, and stays alive
/// (new installs appear without re-subscribing). The thread exits when
/// the Dart side stops listening.
pub fn watch_jobs(sink: StreamSink<Vec<JobDto>>) -> Result<(), BridgeError> {
    loop {
        match snapshot_all() {
            Ok(jobs) => {
                if sink.add(jobs).is_err() {
                    return Ok(()); // Dart side stopped listening.
                }
            }
            Err(_) => return Ok(()), // Transient DB issue: retry next tick.
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Live event stream for one job (contract op `watch_job_events`):
/// replays everything after `last_seen_seq` from the persisted log and
/// follows the job until its terminal stage has been delivered.
pub fn watch_job_events(
    job_id: String,
    last_seen_seq: i64,
    sink: StreamSink<JobEventDto>,
) -> Result<(), BridgeError> {
    let mut sent = last_seen_seq;
    loop {
        let (events, terminal) = match open_db() {
            Ok(db) => {
                let conn = db.connection();
                let store = devkit_jobs::JobStore::new(conn);
                match (store.events_since(&job_id, sent), store.get_job(&job_id)) {
                    (Ok(events), snapshot) => {
                        let terminal = snapshot
                            .ok()
                            .flatten()
                            .and_then(|job| Stage::parse(&job.state))
                            .map(Stage::is_terminal)
                            .unwrap_or(false);
                        (events, terminal)
                    }
                    _ => (Vec::new(), false),
                }
            }
            Err(_) => (Vec::new(), false),
        };
        for event in events {
            sent = sent.max(event.seq);
            if sink.add(event_dto(event)).is_err() {
                return Ok(()); // Dart side stopped listening.
            }
        }
        if terminal {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_snapshot_progress_is_last_event_of_current_stage() {
        let dto = job_dto(
            devkit_jobs::JobSnapshot {
                id: "job".to_owned(),
                kind: "install".to_owned(),
                package_id: Some("p".to_owned()),
                version: Some("1".to_owned()),
                profile_id: None,
                state: "DOWNLOADING".to_owned(),
                created_at: "t".to_owned(),
                updated_at: "t".to_owned(),
                finished_at: None,
                error_code: None,
            },
            vec![
                devkit_jobs::JobEvent {
                    job_id: "job".to_owned(),
                    seq: 1,
                    timestamp: "t".to_owned(),
                    stage: "PREPARING".to_owned(),
                    progress: None,
                    message: "Preparing staging directory".to_owned(),
                    error_code: None,
                },
                devkit_jobs::JobEvent {
                    job_id: "job".to_owned(),
                    seq: 2,
                    timestamp: "t".to_owned(),
                    stage: "DOWNLOADING".to_owned(),
                    progress: Some(0.42),
                    message: "Downloading".to_owned(),
                    error_code: None,
                },
            ],
            Some("P".to_owned()),
        );
        assert_eq!(dto.progress, Some(0.42));
        assert_eq!(dto.package_name.as_deref(), Some("P"));
        assert_eq!(dto.events.len(), 2);
    }
}
