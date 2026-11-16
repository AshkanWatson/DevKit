//! Installed registry operations (P3-05): the registry reads, the
//! P3-04 activation pair (`plan_activation` consent data + consent-gated
//! `set_active_version`), and exact-version uninstall
//! (docs/ffi/bridge-contract.md, ADR 0007).
//!
//! The listed surface shows DevKit-managed rows in settled states
//! (`installed`/`broken`); transient `installing`/`removing` rows belong
//! to a running job (the Downloads screen shows that job).

use devkit_jobs::environment::{
    default_path_environment, plan_activation, ActivationConsent, PathChange,
};
use devkit_jobs::pipeline::{uninstall_version, InstallReceipt, UninstallOutcome};
use devkit_registry::installed::{InstalledRepo, InstalledStatus};
use devkit_registry::package_cache::PackageCache;

use super::engine::{managed_root, open_db};
use super::error::{bridge_error, from_engine_code, from_jobs_error, BridgeError, BridgeErrorCode};

/// One installed version, flattened for the Installed screen (the
/// domain's `InstalledPackage`; ownership is always DevKit-managed
/// here — externally detected software never enters this registry).
#[derive(Debug, Clone)]
pub struct InstalledPackageDto {
    pub package_id: String,
    /// Catalog display name; the Dart adapter falls back to the ID.
    pub name: String,
    pub version: String,
    /// `"installed"` or `"broken"` (transient states are job territory).
    pub status: String,
    pub is_active: bool,
    /// Managed-root-relative installation directory.
    pub location_label: String,
    /// UTC ISO-8601.
    pub installed_at: String,
}

/// One installed version with its receipt facts (detail level).
#[derive(Debug, Clone)]
pub struct InstalledVersionDto {
    pub package_id: String,
    pub version: String,
    pub status: String,
    pub is_active: bool,
    pub install_path: String,
    pub installed_at: String,
    pub file_count: usize,
    pub total_bytes: u64,
    /// Declared entry point, when the package has one.
    pub entry_point: Option<String>,
}

/// One shim the activation would (re)write (ADR 0007).
#[derive(Debug, Clone)]
pub struct ShimPlanDto {
    pub shim_relative_path: String,
    pub target_relative_path: String,
}

/// The user-PATH part of an activation plan. Kind values:
/// `alreadyPresent` (no write needed), `append` (entry would be added
/// to the user PATH; `entry` set), `unavailable` (PATH integration not
/// supported on this platform; `reason` set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathChangeKind {
    AlreadyPresent,
    Append,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct PathChangeDto {
    pub kind: PathChangeKind,
    /// The entry that would be appended (kind = append).
    pub entry: Option<String>,
    /// Why PATH integration is unavailable (kind = unavailable).
    pub reason: Option<String>,
}

/// What activating a version would change — the consent dialog's data.
/// Nothing is written by `plan_activation`.
#[derive(Debug, Clone)]
pub struct ActivationPlanDto {
    pub package_id: String,
    pub version: String,
    pub previous_active_version: Option<String>,
    pub shims: Vec<ShimPlanDto>,
    pub shims_dir_relative: String,
    pub path_change: PathChangeDto,
}

/// Result of a granted activation.
#[derive(Debug, Clone)]
pub struct ActivationOutcomeDto {
    pub package_id: String,
    pub version: String,
    pub previous_active_version: Option<String>,
    pub shims_written: usize,
    pub path_change: PathChangeDto,
}

/// Exact-version uninstall result.
#[derive(Debug, Clone)]
pub struct RemovalResultDto {
    pub found: bool,
    pub files_removed: usize,
    /// Whether the active mapping pointed at this version and was cleared.
    pub cleared_active: bool,
}

fn path_change_dto(change: &PathChange) -> PathChangeDto {
    match change {
        PathChange::AlreadyPresent => PathChangeDto {
            kind: PathChangeKind::AlreadyPresent,
            entry: None,
            reason: None,
        },
        PathChange::Append(entry) => PathChangeDto {
            kind: PathChangeKind::Append,
            entry: Some(entry.clone()),
            reason: None,
        },
        PathChange::Unavailable(reason) => PathChangeDto {
            kind: PathChangeKind::Unavailable,
            entry: None,
            reason: Some(reason.clone()),
        },
    }
}

fn status_str(status: InstalledStatus) -> Option<&'static str> {
    match status {
        InstalledStatus::Installed => Some("installed"),
        InstalledStatus::Broken => Some("broken"),
        // Transient rows belong to the running job, not this list.
        InstalledStatus::Installing | InstalledStatus::Removing => None,
    }
}

fn package_names(ids: &[String]) -> std::collections::HashMap<String, String> {
    let mut names = std::collections::HashMap::new();
    if let Ok(db) = open_db() {
        let cache = PackageCache::new(db.connection());
        for id in ids {
            if let Ok(Some(cached)) = cache.get(id) {
                names.insert(id.clone(), cached.manifest.name);
            }
        }
    }
    names
}

/// All DevKit-managed installations in settled states, active marker
/// included. Ordered by package ID, then version.
pub fn list_installed_packages() -> Result<Vec<InstalledPackageDto>, BridgeError> {
    let db = open_db()?;
    let conn = db.connection();
    let repo = InstalledRepo::new(conn);
    let records = repo.list().map_err(super::error::from_registry_error)?;
    let names = package_names(
        &records
            .iter()
            .map(|record| record.package_id.clone())
            .collect::<Vec<_>>(),
    );
    let mut result: Vec<InstalledPackageDto> = Vec::new();
    for record in records {
        let Some(status) = status_str(record.status) else {
            continue;
        };
        let is_active = repo
            .active_version(&record.package_id)
            .map_err(super::error::from_registry_error)?
            .as_deref()
            == Some(record.version.as_str());
        result.push(InstalledPackageDto {
            name: names
                .get(&record.package_id)
                .cloned()
                .unwrap_or_else(|| record.package_id.clone()),
            package_id: record.package_id,
            version: record.version,
            status: status.to_owned(),
            is_active,
            location_label: record.install_path,
            installed_at: record.installed_at,
        });
    }
    result.sort_by(|a, b| {
        a.package_id
            .cmp(&b.package_id)
            .then(a.version.cmp(&b.version))
    });
    Ok(result)
}

/// One package's installed versions with receipt facts.
pub fn list_installed_versions(
    package_id: String,
) -> Result<Vec<InstalledVersionDto>, BridgeError> {
    let db = open_db()?;
    let conn = db.connection();
    let repo = InstalledRepo::new(conn);
    let active = repo
        .active_version(&package_id)
        .map_err(super::error::from_registry_error)?;
    let mut result = Vec::new();
    for record in repo
        .list_for_package(&package_id)
        .map_err(super::error::from_registry_error)?
    {
        let receipt: InstallReceipt = serde_json::from_str(&record.receipt_json).map_err(|_| {
            bridge_error(
                BridgeErrorCode::DatabaseError,
                format!("unreadable receipt for {package_id} {}", record.version),
            )
        })?;
        result.push(InstalledVersionDto {
            is_active: active.as_deref() == Some(record.version.as_str()),
            package_id: record.package_id,
            version: record.version,
            status: record.status.as_str().to_owned(),
            install_path: record.install_path,
            installed_at: record.installed_at,
            file_count: receipt.files.len(),
            total_bytes: receipt.files.iter().map(|file| file.size_bytes).sum(),
            entry_point: receipt.executable_relative_path,
        });
    }
    result.sort_by(|a, b| a.version.cmp(&b.version));
    Ok(result)
}

/// Computes what activating `version` would change — the consent step's
/// data. Pure read; nothing is written (P3-04 / ADR 0007).
pub fn plan_activation_for(
    package_id: String,
    version: String,
) -> Result<ActivationPlanDto, BridgeError> {
    let db = open_db()?;
    let plan = plan_activation(
        managed_root()?,
        db.connection(),
        &package_id,
        &version,
        &default_path_environment(),
    )
    .map_err(|failure| from_engine_code(failure.code, failure.message))?;
    Ok(ActivationPlanDto {
        package_id: plan.package_id,
        version: plan.version,
        previous_active_version: plan.previous_active_version,
        shims: plan
            .shims
            .into_iter()
            .map(|shim| ShimPlanDto {
                shim_relative_path: shim.shim_relative_path,
                target_relative_path: shim.target_relative_path,
            })
            .collect(),
        shims_dir_relative: plan.shims_dir_relative,
        path_change: path_change_dto(&plan.path_change),
    })
}

/// Activates one version. `consent` must be true — the UI shows the
/// plan first and passes the user's explicit decision; a false consent
/// is refused with `PERMISSION_REQUIRED` and writes nothing
/// (bridge-contract.md, P3-04 semantics).
pub fn set_active_version(
    package_id: String,
    version: String,
    consent: bool,
) -> Result<ActivationOutcomeDto, BridgeError> {
    let db = open_db()?;
    let outcome = devkit_jobs::environment::activate(
        managed_root()?,
        db.connection(),
        &package_id,
        &version,
        if consent {
            ActivationConsent::Granted
        } else {
            ActivationConsent::Declined
        },
        &default_path_environment(),
    )
    .map_err(|failure| from_engine_code(failure.code, failure.message))?;
    Ok(ActivationOutcomeDto {
        package_id: outcome.package_id,
        version: outcome.version,
        previous_active_version: outcome.previous_active_version,
        shims_written: outcome.shims_written,
        path_change: path_change_dto(&outcome.path_change),
    })
}

/// Uninstalls exactly one version: receipt-driven, containment-checked,
/// siblings untouched. When the active mapping pointed at the version it
/// is cleared and dangling shims removed (version-management.md §2).
pub fn remove_managed_version(
    package_id: String,
    version: String,
) -> Result<RemovalResultDto, BridgeError> {
    let db = open_db()?;
    match uninstall_version(managed_root()?, db.connection(), &package_id, &version) {
        Ok(outcome) => Ok(match outcome {
            UninstallOutcome::NotFound => RemovalResultDto {
                found: false,
                files_removed: 0,
                cleared_active: false,
            },
            UninstallOutcome::Removed {
                files_removed,
                cleared_active,
            } => RemovalResultDto {
                found: true,
                files_removed,
                cleared_active,
            },
        }),
        Err(error) => Err(from_jobs_error(error)),
    }
}
