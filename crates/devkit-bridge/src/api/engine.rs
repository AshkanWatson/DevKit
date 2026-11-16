//! Engine lifecycle (P3-05): paths and the process-global job manager.
//!
//! `init_engine` runs once before any other operation (the Dart adapter
//! calls it lazily on first use). Paths follow HANDOFF.md §6: the
//! managed root lives under the OS-standard user-local data directory
//! (never hard-coded), the database sits in `<managed-root>/state/`.
//! Tests and embedders pass explicit paths.
//!
//! The [`JobManager`] is process-global on purpose: `cancel_job` only
//! accepts jobs owned by the same manager instance, and installation
//! jobs must outlive Flutter screens (bridge-contract.md § Job
//! independence from Flutter lifecycle).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use devkit_jobs::JobManager;

use super::error::{bridge_error, BridgeError, BridgeErrorCode};

pub(crate) struct EngineState {
    pub(crate) db_path: PathBuf,
    pub(crate) managed_root: PathBuf,
    pub(crate) manager: JobManager,
}

static ENGINE: OnceLock<EngineState> = OnceLock::new();

pub(crate) fn engine() -> Result<&'static EngineState, BridgeError> {
    ENGINE.get().ok_or_else(|| {
        bridge_error(
            BridgeErrorCode::InvalidArgument,
            "the engine is not initialized; call init_engine before any other operation",
        )
    })
}

/// Opens the shared SQLite database (registry + catalog cache + jobs).
pub(crate) fn open_db() -> Result<devkit_registry::Database, BridgeError> {
    devkit_registry::Database::open(&engine()?.db_path)
        .map_err(|error| bridge_error(BridgeErrorCode::DatabaseError, error.to_string()))
}

pub(crate) fn managed_root() -> Result<&'static Path, BridgeError> {
    Ok(engine()?.managed_root.as_path())
}

/// Default data root (HANDOFF.md §6): OS-standard user-local data
/// directory + `DevKit` (e.g. `%LOCALAPPDATA%\DevKit` on Windows).
fn default_managed_root() -> Result<PathBuf, BridgeError> {
    dirs::data_local_dir()
        .map(|dir| dir.join("DevKit"))
        .ok_or_else(|| {
            bridge_error(
                BridgeErrorCode::InvalidArgument,
                "cannot determine the OS user-local data directory",
            )
        })
}

/// Initializes the engine (idempotent). Production passes nothing and
/// gets the OS-standard layout; tests pass temp paths.
///
/// Also runs schema migrations and orphan recovery: jobs interrupted by
/// a process death are marked `FAILED` and their staging data removed
/// before the UI ever sees them (installation-lifecycle.md §5).
pub fn init_engine(
    db_path: Option<String>,
    managed_root: Option<String>,
) -> Result<(), BridgeError> {
    let default_root = default_managed_root()?;
    let managed_root: PathBuf = managed_root
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or(default_root);
    let db_path: PathBuf = db_path
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| managed_root.join("state").join("devkit.sqlite"));

    if let Some(existing) = ENGINE.get() {
        if existing.db_path == db_path && existing.managed_root == managed_root {
            return Ok(()); // Idempotent re-init (hot restart etc.).
        }
        return Err(bridge_error(
            BridgeErrorCode::InvalidArgument,
            "the engine is already initialized with different paths",
        ));
    }

    std::fs::create_dir_all(&managed_root).map_err(|error| {
        bridge_error(
            BridgeErrorCode::InstallFailed,
            format!("cannot create the managed root directory: {error}"),
        )
    })?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            bridge_error(
                BridgeErrorCode::InstallFailed,
                format!("cannot create the state directory: {error}"),
            )
        })?;
    }

    // Migrations + orphan recovery happen inside `open_with`.
    let manager = JobManager::open_with(&db_path, Some(&managed_root))
        .map_err(|error| bridge_error(BridgeErrorCode::DatabaseError, error.to_string()))?;

    let state = EngineState {
        db_path: db_path.clone(),
        managed_root: managed_root.clone(),
        manager,
    };
    match ENGINE.set(state) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Another thread won the race; consistent paths are fine.
            let existing = ENGINE.get().expect("just set by another thread");
            if existing.db_path == db_path && existing.managed_root == managed_root {
                Ok(())
            } else {
                Err(bridge_error(
                    BridgeErrorCode::InvalidArgument,
                    "the engine is already initialized with different paths",
                ))
            }
        }
    }
}

/// Data-root facts for the Settings/Storage screen and diagnostics.
#[derive(Debug, Clone)]
pub struct EnginePathsInfo {
    /// Managed root directory (packages/, shims/, state/, ...).
    pub managed_root: String,
    /// SQLite database file (`<managed-root>/state/devkit.sqlite`).
    pub db_path: String,
}

/// Where the engine stores its data (explicit init paths or defaults).
pub fn get_engine_paths() -> Result<EnginePathsInfo, BridgeError> {
    let state = engine()?;
    Ok(EnginePathsInfo {
        managed_root: state.managed_root.to_string_lossy().into_owned(),
        db_path: state.db_path.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent_and_rejects_conflicting_paths() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let first = dir.path().join("one");
        init_engine(None, Some(first.to_string_lossy().into_owned())).expect("first init");
        init_engine(None, Some(first.to_string_lossy().into_owned())).expect("idempotent re-init");
        let other = dir.path().join("two");
        let conflict = init_engine(None, Some(other.to_string_lossy().into_owned()))
            .expect_err("conflict detected");
        assert_eq!(conflict.code_str(), "INVALID_ARGUMENT");
    }
}
