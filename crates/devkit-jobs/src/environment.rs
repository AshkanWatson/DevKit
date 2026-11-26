//! Activation: active-version switching, DevKit-managed shims, and the
//! consent-gated user-PATH integration (P3-04, version-management.md §3,
//! HANDOFF.md §6, security-model.md §7, DESIGN.md journey C).
//!
//! Activation is **separate** from installation (installation-lifecycle.md
//! §4): the install pipeline never touches global state. This module:
//!
//! 1. `plan_activation` — a pure read that computes exactly what would
//!    change: the shim file(s) (`<managed-root>/shims/<tool>.cmd`, ADR 0007)
//!    and the user-PATH entry (only when the shims directory is not on the
//!    user PATH yet). The UI shows this plan as the consent step
//!    (DESIGN.md §8 journey C step 4).
//! 2. `activate` — applies the plan **only with explicit consent**
//!    (`PERMISSION_REQUIRED` when declined, and nothing is written). The
//!    operation is reversible: the previous mapping and shim content are
//!    kept during the operation and restored if any step fails (DESIGN.md
//!    journey C step 6, version-management.md §3 "reversible").
//!
//! Security boundaries (security-model.md §7):
//! - Shims live under DevKit's own managed root — no system locations, no
//!   copied executables, no symlinks, no elevation.
//! - PATH changes are **user-scope only** (`HKCU\Environment\Path`,
//!   REG_EXPAND_SZ preserved, idempotent, `WM_SETTINGCHANGE` broadcast) and
//!   happen only after consent.
//! - The environment access is injectable ([`PathEnvironment`]): tests
//!   record changes in memory and never touch the real registry
//!   (testing-strategy.md).

use std::fs;
use std::path::{Path, PathBuf};

use devkit_core::error::ErrorCode;
use devkit_registry::installed::{InstalledRepo, InstalledStatus};
use rusqlite::Connection;
use serde::Serialize;

use crate::pipeline::{is_safe_path_component, InstallReceipt, StageFailure};

/// Shims directory at the managed root (HANDOFF.md §6 layout:
/// `DevKit/shims/`, sibling of `packages/`).
pub const SHIMS_DIR: &str = "shims";

/// One shim the activation would (re)write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShimPlan {
    /// Managed-root-relative shim file, e.g. `shims\tool.cmd`.
    pub shim_relative_path: String,
    /// Managed-root-relative executable it launches, e.g.
    /// `packages\devkit.test.tool\1.0.0\app\bin\tool.exe`.
    pub target_relative_path: String,
}

/// The user-PATH part of an activation plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathChange {
    /// The shims directory is already on the user PATH — no write needed.
    AlreadyPresent,
    /// This entry would be appended to the user PATH.
    Append(String),
    /// PATH integration is not available on this platform in this release
    /// (Windows-first scope, ADR 0007); shims still work when invoked by
    /// absolute path.
    Unavailable(String),
}

/// What an activation would change (the consent-dialog content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivationPlan {
    pub package_id: String,
    pub version: String,
    /// The currently active version, if any (shown in the dialog; kept for
    /// rollback while the operation runs).
    pub previous_active_version: Option<String>,
    pub shims: Vec<ShimPlan>,
    pub shims_dir_relative: String,
    pub path_change: PathChange,
}

/// The consent decision (DESIGN.md journey C step 5; bridge contract:
/// "Consent handled in UI before call").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationConsent {
    /// The user declined (or has not seen the plan): nothing may change.
    Declined,
    /// The user reviewed the plan and confirmed it.
    Granted,
}

/// Result of a completed activation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivationOutcome {
    pub package_id: String,
    pub version: String,
    pub previous_active_version: Option<String>,
    pub shims_written: usize,
    pub path_change: PathChange,
}

fn failure(code: ErrorCode, message: impl Into<String>) -> StageFailure {
    StageFailure::new(code, message)
}

/// Platform seam for the user PATH. Production: [`WindowsUserEnvironment`]
/// (Windows) / [`UnsupportedPlatformEnvironment`] (others). Tests inject a
/// recorder — the real registry is never touched in tests.
pub trait PathEnvironment: Send + Sync {
    /// Whether PATH integration exists on this platform in this release.
    fn path_integration_supported(&self) -> bool {
        true
    }

    /// The current user-PATH entries (split on the platform separator).
    fn user_path_entries(&self) -> Result<Vec<String>, StageFailure>;

    /// Idempotent append of one entry to the user PATH. Returns what was
    /// done; `AlreadyPresent` writes nothing.
    fn append_user_path_entry(&self, entry: &str) -> Result<PathChange, StageFailure>;
}

/// Production user-PATH environment for Windows (ADR 0007): reads and
/// appends to `HKCU\Environment\Path`, preserving the `REG_EXPAND_SZ` type
/// so `%VAR%` references survive, and broadcasts `WM_SETTINGCHANGE` so
/// running shells pick the change up. User scope only — never
/// `HKLM`, never the system PATH, no elevation.
#[cfg(windows)]
pub struct WindowsUserEnvironment;

#[cfg(windows)]
impl PathEnvironment for WindowsUserEnvironment {
    fn user_path_entries(&self) -> Result<Vec<String>, StageFailure> {
        let value = read_user_path_raw().map_err(|error| match error {
            PathAccess::Denied => failure(
                ErrorCode::PermissionRequired,
                "reading the user PATH was denied by the system",
            ),
            PathAccess::Error(message) => failure(
                ErrorCode::InstallFailed,
                format!("cannot read the user PATH: {message}"),
            ),
        })?;
        Ok(split_path_value(&value))
    }

    fn append_user_path_entry(&self, entry: &str) -> Result<PathChange, StageFailure> {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE};
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let environment = hkcu
            .open_subkey_with_flags("Environment", KEY_QUERY_VALUE | KEY_SET_VALUE)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::PermissionDenied => failure(
                    ErrorCode::PermissionRequired,
                    "writing the user PATH was denied by the system",
                ),
                _ => failure(
                    ErrorCode::InstallFailed,
                    format!("cannot open the user environment for writing: {}", error),
                ),
            })?;
        let existing = read_raw_value(&environment).unwrap_or_default();
        if split_path_value(&existing)
            .iter()
            .any(|item| path_equivalent(item, entry))
        {
            return Ok(PathChange::AlreadyPresent);
        }
        let joined = join_path_value(&existing, entry);
        let expanded = winreg::RegValue {
            vtype: winreg::enums::RegType::REG_EXPAND_SZ,
            bytes: utf16_le_bytes(&joined).into(),
        };
        environment
            .set_raw_value("Path", &expanded)
            .map_err(|error| {
                failure(
                    ErrorCode::InstallFailed,
                    format!("cannot write the user PATH: {error}"),
                )
            })?;
        broadcast_environment_change();
        Ok(PathChange::Append(entry.to_owned()))
    }
}

#[cfg(windows)]
enum PathAccess {
    Denied,
    Error(String),
}

#[cfg(windows)]
fn read_user_path_raw() -> Result<String, PathAccess> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let environment = hkcu
        .open_subkey_with_flags("Environment", KEY_QUERY_VALUE)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::PermissionDenied => PathAccess::Denied,
            _ => PathAccess::Error(error.to_string()),
        })?;
    read_raw_value(&environment).map_err(|error| match error.kind() {
        std::io::ErrorKind::PermissionDenied => PathAccess::Denied,
        _ => PathAccess::Error(error.to_string()),
    })
}

/// Reads `HKCU\Environment\Path` as text (`REG_EXPAND_SZ`/`REG_SZ` are
/// UTF-16); a missing value is an empty PATH, anything else is an error.
#[cfg(windows)]
fn read_raw_value(environment: &winreg::RegKey) -> std::io::Result<String> {
    use winreg::enums::RegType;

    let value = match environment.get_raw_value("Path") {
        Ok(value) => value,
        Err(ref error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error),
    };
    match value.vtype {
        RegType::REG_EXPAND_SZ | RegType::REG_SZ => Ok(decode_utf16_le(&value.bytes)),
        other => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unexpected registry type for Path: {other:?}"),
        )),
    }
}

#[cfg(windows)]
fn decode_utf16_le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(windows)]
fn utf16_le_bytes(value: &str) -> Vec<u8> {
    value
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

/// Splits a `PATH`-shaped string into trimmed, non-empty entries.
#[cfg(windows)]
fn split_path_value(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Appends one entry, preserving existing content (never rewrites or
/// reorders; an empty existing value becomes just the entry).
#[cfg(windows)]
fn join_path_value(existing: &str, entry: &str) -> String {
    let trimmed = existing.trim().trim_end_matches(';').to_owned();
    if trimmed.is_empty() {
        entry.to_owned()
    } else {
        format!("{trimmed};{entry}")
    }
}

/// Equality up to trailing separator and case (Windows paths are
/// case-insensitive; `C:\devkit\shims\` and `c:\devkit\shims` are the same
/// entry).
#[cfg(windows)]
fn path_equivalent(left: &str, right: &str) -> bool {
    let normalize = |value: &str| value.trim_end_matches(['\\', '/']).to_lowercase();
    normalize(left) == normalize(right)
}

/// Running processes do not re-read the registry; the documented
/// `WM_SETTINGCHANGE` broadcast makes Explorer (and so newly launched
/// shells) refresh the environment (ADR 0007).
#[cfg(windows)]
fn broadcast_environment_change() {
    use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    let parameter: Vec<u16> = "Environment"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0 as WPARAM,
            parameter.as_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            2_000,
            std::ptr::null_mut(),
        );
    }
}

/// Non-Windows production environment: activation (mapping + shims) still
/// works, PATH integration reports unavailable (Windows-first scope,
/// ADR 0007). A platform module with real per-OS integration is the
/// Phase-5 scope (`devkit-platform`).
#[cfg(not(windows))]
pub struct UnsupportedPlatformEnvironment;

#[cfg(not(windows))]
impl PathEnvironment for UnsupportedPlatformEnvironment {
    fn path_integration_supported(&self) -> bool {
        false
    }

    fn user_path_entries(&self) -> Result<Vec<String>, StageFailure> {
        Err(failure(
            ErrorCode::UnsupportedPlatform,
            "user PATH integration is Windows-only in this release",
        ))
    }

    fn append_user_path_entry(&self, _entry: &str) -> Result<PathChange, StageFailure> {
        Err(failure(
            ErrorCode::UnsupportedPlatform,
            "user PATH integration is Windows-only in this release",
        ))
    }
}

/// The production environment for this build (ADR 0007).
pub fn default_path_environment() -> impl PathEnvironment {
    #[cfg(windows)]
    {
        WindowsUserEnvironment
    }
    #[cfg(not(windows))]
    {
        UnsupportedPlatformEnvironment
    }
}

/// Computes what activating `package_id` `version` would change — **no
/// writes happen here** (the consent step's data, DESIGN.md journey C
/// step 4).
pub fn plan_activation<E: PathEnvironment>(
    managed_root: &Path,
    conn: &Connection,
    package_id: &str,
    version: &str,
    env: &E,
) -> Result<ActivationPlan, StageFailure> {
    let repo = InstalledRepo::new(conn);
    let record = load_installed_record(&repo, package_id, version)?;
    let receipt = parse_receipt(&record)?;
    let previous_active_version = repo
        .active_version(package_id)
        .map_err(|error| failure(ErrorCode::DatabaseError, error.to_string()))?;
    let shims = shim_plans(&receipt)?;
    let path_change = if !env.path_integration_supported() {
        PathChange::Unavailable(
            "user PATH integration is Windows-only in this release; shims work \
             when invoked by absolute path"
                .to_owned(),
        )
    } else {
        let entry = shims_dir_entry(managed_root);
        let present = env
            .user_path_entries()
            .map_err(|error| failure(ErrorCode::DatabaseError, error.message.clone()))?
            .iter()
            .any(|item| path_entry_matches(item, &entry));
        if present {
            PathChange::AlreadyPresent
        } else {
            PathChange::Append(entry)
        }
    };
    Ok(ActivationPlan {
        package_id: package_id.to_owned(),
        version: version.to_owned(),
        previous_active_version,
        shims,
        shims_dir_relative: SHIMS_DIR.to_owned(),
        path_change,
    })
}

/// Activates one exact version per the given plan: records the active
/// mapping, (re)writes the shims, and — only when the plan says a change is
/// needed — appends the shims directory to the user PATH. Requires
/// [`ActivationConsent::Granted`]; anything else is refused with
/// `PERMISSION_REQUIRED` and **nothing is written**.
///
/// Reversibility (version-management.md §3): the previous mapping and the
/// previous shim contents are kept while the operation runs; if any step
/// fails, they are restored before the error is returned.
pub fn activate<E: PathEnvironment>(
    managed_root: &Path,
    conn: &Connection,
    package_id: &str,
    version: &str,
    consent: ActivationConsent,
    env: &E,
) -> Result<ActivationOutcome, StageFailure> {
    let plan = plan_activation(managed_root, conn, package_id, version, env)?;
    if consent != ActivationConsent::Granted {
        return Err(failure(
            ErrorCode::PermissionRequired,
            "activation requires explicit user consent after reviewing the changes; \
             nothing was modified",
        ));
    }

    let repo = InstalledRepo::new(conn);
    let previous_active_version = plan.previous_active_version.clone();
    repo.set_active(package_id, version)
        .map_err(|error| failure(ErrorCode::DatabaseError, error.to_string()))?;

    // Shims are replaced atomically (temporary file + rename), and the
    // previous bytes are remembered so a failed step restores them.
    let mut previous_shims: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::new();
    for shim in &plan.shims {
        let shim_path = managed_root.join(&shim.shim_relative_path);
        let Some(parent) = shim_path.parent() else {
            rollback(
                &repo,
                package_id,
                version,
                &previous_active_version,
                &previous_shims,
            );
            return Err(failure(
                ErrorCode::InstallFailed,
                format!("invalid shim path: {}", shim.shim_relative_path),
            ));
        };
        if let Err(source) = fs::create_dir_all(parent) {
            rollback(
                &repo,
                package_id,
                version,
                &previous_active_version,
                &previous_shims,
            );
            return Err(failure(
                ErrorCode::InstallFailed,
                format!("cannot create the shims directory: {source}"),
            ));
        }
        let previous_bytes = fs::read(&shim_path).ok();
        let content = shim_cmd_content(managed_root, &shim.target_relative_path);
        let temporary = shim_path.with_extension("cmd.devkit-tmp");
        let written =
            fs::write(&temporary, &content).and_then(|()| fs::rename(&temporary, &shim_path));
        if let Err(source) = written {
            let _ = fs::remove_file(&temporary);
            rollback(
                &repo,
                package_id,
                version,
                &previous_active_version,
                &previous_shims,
            );
            return Err(failure(
                ErrorCode::InstallFailed,
                format!(
                    "cannot write the shim {}: {source}",
                    shim.shim_relative_path
                ),
            ));
        }
        previous_shims.push((shim_path, previous_bytes));
    }

    // PATH only when the plan says a change is needed. A failure here
    // reverts the mapping and the shims (the consented change must not
    // half-apply — DESIGN.md journey C step 6).
    let path_change = match &plan.path_change {
        PathChange::AlreadyPresent => PathChange::AlreadyPresent,
        PathChange::Unavailable(message) => PathChange::Unavailable(message.clone()),
        PathChange::Append(entry) => match env.append_user_path_entry(entry) {
            Ok(change) => change,
            Err(error) => {
                rollback(
                    &repo,
                    package_id,
                    version,
                    &previous_active_version,
                    &previous_shims,
                );
                return Err(error);
            }
        },
    };

    Ok(ActivationOutcome {
        package_id: package_id.to_owned(),
        version: version.to_owned(),
        previous_active_version,
        shims_written: plan.shims.len(),
        path_change,
    })
}

/// Restores the pre-activation state: previous shim bytes (or no shim at
/// all) and the previous active mapping (or no mapping at all).
fn rollback(
    repo: &InstalledRepo<'_>,
    package_id: &str,
    version: &str,
    previous_active_version: &Option<String>,
    previous_shims: &[(PathBuf, Option<Vec<u8>>)],
) {
    for (shim_path, previous_bytes) in previous_shims {
        match previous_bytes {
            Some(bytes) => {
                let _ = fs::write(shim_path, bytes);
            }
            None => {
                let _ = fs::remove_file(shim_path);
            }
        }
    }
    match previous_active_version {
        Some(previous) => {
            let _ = repo.set_active(package_id, previous);
        }
        None => {
            let _ = repo.clear_active_if_points_at(package_id, version);
        }
    }
}

/// Loads the receipt row that activation operates on, with the stable
/// codes: unknown version → `VERSION_NOT_FOUND`, anything but a validated
/// `installed` row → `INSTALL_FAILED` (repair = uninstall + reinstall).
fn load_installed_record(
    repo: &InstalledRepo<'_>,
    package_id: &str,
    version: &str,
) -> Result<devkit_registry::installed::InstalledRecord, StageFailure> {
    let record = repo
        .get(package_id, version)
        .map_err(|error| failure(ErrorCode::DatabaseError, error.to_string()))?
        .ok_or_else(|| {
            failure(
                ErrorCode::VersionNotFound,
                format!("version {version} of {package_id} is not installed"),
            )
        })?;
    match record.status {
        InstalledStatus::Installed => Ok(record),
        InstalledStatus::Installing | InstalledStatus::Removing => Err(failure(
            ErrorCode::InstallFailed,
            "that version is still being installed or removed; try again later",
        )),
        InstalledStatus::Broken => Err(failure(
            ErrorCode::InstallFailed,
            "the installed version failed validation; repair = uninstall + reinstall",
        )),
    }
}

fn parse_receipt(
    record: &devkit_registry::installed::InstalledRecord,
) -> Result<InstallReceipt, StageFailure> {
    let receipt: InstallReceipt = serde_json::from_str(&record.receipt_json).map_err(|error| {
        failure(
            ErrorCode::DatabaseError,
            format!("unreadable install receipt: {error}"),
        )
    })?;
    if record.install_path != receipt.install_path {
        return Err(failure(
            ErrorCode::DatabaseError,
            "registry row and receipt disagree about the install path",
        ));
    }
    Ok(receipt)
}

/// One shim per declared entry point (ADR 0007): `shims/<stem>.cmd`
/// launching `packages/<pkg>/<version>/<executable>`.
fn shim_plans(receipt: &InstallReceipt) -> Result<Vec<ShimPlan>, StageFailure> {
    let Some(executable) = &receipt.executable_relative_path else {
        return Ok(Vec::new()); // library-style packages activate without a shim
    };
    let stem = Path::new(executable)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| is_safe_path_component(stem))
        .ok_or_else(|| {
            failure(
                ErrorCode::InvalidArgument,
                format!("cannot derive a safe shim name from the entry point: {executable}"),
            )
        })?;
    let target = Path::new(&receipt.install_path)
        .join(executable)
        // Component round-trip normalizes the mixed separators a
        // forward-slash executable declaration produces on Windows.
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned();
    Ok(vec![ShimPlan {
        shim_relative_path: Path::new(SHIMS_DIR)
            .join(format!("{stem}.cmd"))
            .to_string_lossy()
            .into_owned(),
        target_relative_path: target,
    }])
}

/// The absolute, normal shims-directory path (the user-PATH entry).
///
/// Built **lexically** from the (already absolute) managed root — never via
/// `canonicalize()`. Canonicalizing here is unstable: on macOS `/var` is a
/// symlink to `/private/var`, and `canonicalize()` only resolves it once the
/// shims directory exists. The first activation (dir absent) would store
/// `/var/folders/.../shims` while the second (dir present) computes
/// `/private/var/folders/.../shims`, so idempotence detection fails and the
/// entry is appended twice. A lexical path is stable across both states.
fn shims_dir_entry(managed_root: &Path) -> String {
    let entry = managed_root.join(SHIMS_DIR);
    let text = entry.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        // Strip a drive-letter verbatim prefix if one was supplied.
        let bytes = rest.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' {
            return rest.to_owned();
        }
    }
    text.into_owned()
}

fn normalize_absolute(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = canonical.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        // Only strip the drive-letter verbatim form; UNC forms stay.
        let bytes = rest.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' {
            return PathBuf::from(rest);
        }
    }
    canonical
}

/// The `.cmd` launcher content (ADR 0007): forward the command line to the
/// activated version's executable. Quoted target; `%*` passes arguments.
fn shim_cmd_content(managed_root: &Path, target_relative_path: &str) -> String {
    let target = normalize_absolute(&managed_root.join(target_relative_path));
    format!("@echo off\r\n\"{}\" %*\r\n", target.display())
}

/// Case-insensitive, trailing-separator-insensitive entry comparison used
/// for both the plan and the idempotent append.
fn path_entry_matches(left: &str, right: &str) -> bool {
    let normalize = |value: &str| value.trim_end_matches(['\\', '/']).to_lowercase();
    normalize(left) == normalize(right)
}

/// Removes shim files whose launcher targets `install_path` (managed-root
/// relative). Best-effort cleanup used by uninstall when the removed
/// version was the active one; the count is informational.
pub fn remove_shims_referencing(managed_root: &Path, install_path: &str) -> usize {
    let shims_dir = managed_root.join(SHIMS_DIR);
    let Ok(entries) = fs::read_dir(&shims_dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("cmd") {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_default();
        if content.contains(install_path) && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}
