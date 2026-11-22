//! The real installation pipeline (P3-03,
//! docs/architecture/installation-lifecycle.md §3, version-management.md §2).
//!
//! Walks a job through the real state machine with real work:
//!
//! ```text
//! RESOLVING   artifact resolution from the catalog (InstallSource)
//! PREPARING   staging directory under the managed root + disk space check
//! DOWNLOADING devkit-download engine: stream -> size cap -> SHA-256
//! VERIFYING   verification report (real hash + size from the engine)
//! EXTRACTING  devkit-install archive adapter (zip/tar/tar.gz, bomb/traversal/link defense)
//!             -- or, for `exe`/`msi` artifacts (Windows only) --
//! INSTALLING  devkit-install vendor adapters: technology/container
//!             detection from the verified bytes + fixed silent flags
//!             (never catalog data); the vendor installer process is
//!             owned by the job. `msi` receipts record the ProductCode
//!             (read-only Property-table read) so rollback and uninstall
//!             run `msiexec /x {ProductCode}` — no ghost registration.
//! CONFIGURING artifact cleanup + receipt row (status `installing`)
//! VALIDATING  validate_installation (existence, sizes, containment, entry point)
//! COMPLETED   receipt status becomes `installed`
//! ```
//!
//! Failure or cancellation at **any** stage triggers the documented
//! rollback chain: staging data (staging directory, artifact file, partial
//! `.part` file) is deleted and an unvalidated receipt row is removed, so
//! the managed root and registry return to their pre-install state. After
//! `VALIDATING` succeeds the installation is permanent until explicit
//! uninstall.
//!
//! Security invariants carried over from P3-01/P3-02 (security-model.md):
//! SHA-256 verification is unconditional and happens before extraction; a
//! mismatch never extracts; progress percentages are computed from real
//! byte counts (no fake percentages); the engine's HTTPS-only + redirect
//! policy applies to every artifact.
//!
//! No global state (PATH, env, registry) is touched here — activation is a
//! separate consent-gated operation (P3-04).

use std::cell::Cell;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use devkit_core::error::ErrorCode;
use devkit_core::manifest::{
    Architecture, Artifact, ArtifactOs, InstallerKind, PackageVersion, Platform,
};
use devkit_download::{
    download_and_verify_with_progress, CancelToken, DownloadError, DownloadHttp, DownloadRequest,
};
use devkit_install::{
    build_command, build_uninstall_command, detect_format, detect_technology_from_file,
    ensure_within, extract_file, is_msi_package_file, msi_build_command, msi_is_success_exit_code,
    msiexec_program, read_product_code, walk_installed_files, ArchiveFormat, ArchiveLimits,
    ExeRunner, ExeTechnology, MSI_EXIT_UNKNOWN_PRODUCT,
};
use devkit_registry::{
    package_cache::PackageCache, Database, InstalledRecord, InstalledRepo, InstalledStatus,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::JobsError;
use crate::runner::record_terminal_chain;
use crate::simulated::RunOutcome;
use crate::state::Stage;
use crate::store::JobStore;

/// A resolution/install failure with a stable error code, surfaced in the
/// job's event stream (`Finish::failed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StageFailure {
    pub code: ErrorCode,
    pub message: String,
}

impl StageFailure {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Resolves one concrete artifact from the catalog (installation-lifecycle
/// §3 RESOLVING). Production implementation: [`RegistrySource`] over the
/// cached catalog (populated by the Django catalog client). Tests plug in
/// fixture sources pointing at a loopback fixture server — never real
/// vendor hosts (testing-strategy.md rule 1).
pub trait InstallSource: Send + 'static {
    fn resolve(&self, package_id: &str, version: &str) -> Result<Artifact, StageFailure>;
}

/// One extracted file, as recorded in the receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptFile {
    /// Platform-native path relative to the installation directory.
    pub relative_path: String,
    pub size_bytes: u64,
    /// SHA-256 of the extracted bytes (P3-02 extraction journal).
    pub sha256_hex: String,
}

/// The downloaded artifact part of the receipt (archive adapter only;
/// `Option` since P3-06, when the exe adapter arrived).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveReceipt {
    /// `zip`, `tar`, or `tar.gz` (magic-byte detection).
    pub format: String,
    pub sha256_hex: String,
    pub size_bytes: u64,
}

/// The vendor-installer part of the receipt (`installer: exe`/`msi`,
/// P3-06/P3-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorReceiptPart {
    /// `inno-setup`, `nsis`, or `msi` — detected from the verified bytes,
    /// never from catalog data.
    pub technology: String,
    /// The installer's exit code (`Some(0)` = success; `None` when the
    /// process died without a code).
    pub exit_code: Option<i32>,
    /// `msi` installs only: the package's ProductCode, read from the
    /// verified MSI's Property table. It drives the `msiexec /x` uninstall
    /// and the rollback undo so no ghost registration is left behind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_code: Option<String>,
}

/// `InstallReceipt` (local-database.md §2: `installed_versions.receipt`).
/// Records everything `validate_installation` and `uninstall` need: paths,
/// per-file checksums, and the artifact identity. Serialized as JSON into
/// the receipt column. `receipt_version` lets the format evolve without
/// breaking old rows (v2, P3-06/07: `archive` became optional and the
/// `vendor` part was added for exe/msi installs; v1 rows still parse via
/// `#[serde(default)]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallReceipt {
    pub receipt_version: u32,
    pub package_id: String,
    pub version: String,
    /// `archive` (P3-02), `exe` (P3-06) or `msi` (P3-07); the remaining
    /// installer kinds are later phases.
    pub installer: String,
    /// The downloaded-artifact part (archive adapter only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<ArchiveReceipt>,
    /// The vendor-installer part (exe/msi adapters only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<VendorReceiptPart>,
    /// Managed-root-relative installation directory.
    pub install_path: String,
    pub files: Vec<ReceiptFile>,
    /// From the artifact declaration, when present (validated at
    /// VALIDATING).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_relative_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_root: Option<String>,
    pub job_id: String,
    /// UTC ISO-8601.
    pub installed_at: String,
}

/// Test-only validation-failure injection hook (P3-03 acceptance: "a
/// failure at any stage triggers the documented rollback chain"): called
/// with the receipt immediately before validation so a test can damage
/// the extracted files. Production callers pass `None`.
pub type ValidationHook = Box<dyn Fn(&InstallReceipt) + Send + Sync>;

/// Configuration for a real install job. Built per job and moved onto the
/// job's worker thread; cheap to construct.
pub struct RealInstallConfig<T: DownloadHttp, S: InstallSource> {
    /// DevKit-managed root: `packages/<package-id>/<version>/` lives here
    /// (version-management.md §2). Chosen by the caller (bridge), never
    /// hard-coded.
    pub managed_root: PathBuf,
    pub transport: T,
    pub source: S,
    pub limits: ArchiveLimits,
    /// The vendor-installers' runner (exe/msi adapters). Production
    /// callers inject `devkit_install::ProcessExeRunner`; tests record the
    /// constructed command line and simulate installer behavior instead of
    /// running foreign vendor artifacts (testing-strategy.md rule 1).
    pub vendor_runner: Box<dyn ExeRunner>,
    /// Test-only hook (see [`ValidationHook`]).
    pub validation_hook: Option<ValidationHook>,
}

/// Production artifact source: the catalog cache in the local database
/// (`packages`/`package_versions`, populated by the Django catalog client
/// — Django is the catalog-metadata authority, HANDOFF.md §5). Resolves
/// the artifact matching the current platform/architecture.
pub struct RegistrySource {
    db: Database,
}

impl RegistrySource {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

impl InstallSource for RegistrySource {
    fn resolve(&self, package_id: &str, version: &str) -> Result<Artifact, StageFailure> {
        let cache = PackageCache::new(self.db.connection());
        let rows = cache
            .versions(package_id)
            .map_err(|error| StageFailure::new(ErrorCode::DatabaseError, error.to_string()))?;
        if rows.is_empty() {
            return Err(StageFailure::new(
                ErrorCode::PackageNotFound,
                format!("package {package_id} is not in the catalog cache"),
            ));
        }
        let Some(row) = rows.iter().find(|row| row.version == version) else {
            return Err(StageFailure::new(
                ErrorCode::VersionNotFound,
                format!("version {version} of {package_id} is not in the catalog cache"),
            ));
        };
        // Parse the cached version fragment. Schema-level validation
        // happened at catalog ingestion (devkit-core manifest validation);
        // the download engine re-enforces the URL policy for whatever is
        // declared here.
        let version_entry: PackageVersion =
            serde_json::from_str(&row.manifest_json).map_err(|_error| {
                StageFailure::new(
                    ErrorCode::IntegrityCheckFailed,
                    format!("cached version entry for {package_id} {version} is malformed"),
                )
            })?;
        let platform = current_platform();
        version_entry
            .artifacts
            .into_iter()
            .find(|artifact| {
                artifact.os == platform_os(platform) && artifact_arch(platform, artifact)
            })
            .ok_or_else(|| {
                StageFailure::new(
                    ErrorCode::UnsupportedPlatform,
                    format!(
                        "no artifact for {} in {package_id} {version}",
                        platform_label(platform)
                    ),
                )
            })
    }
}

/// Wire form matching `supported_platforms` labels ("windows-x64", ...).
pub fn platform_label(platform: Platform) -> &'static str {
    match platform {
        Platform::WindowsX64 => "windows-x64",
        Platform::WindowsArm64 => "windows-arm64",
        Platform::MacosX64 => "macos-x64",
        Platform::MacosArm64 => "macos-arm64",
        Platform::LinuxX64 => "linux-x64",
        Platform::LinuxArm64 => "linux-arm64",
    }
}

/// Compile-time platform detection (ADR 0004: six supported platforms).
pub fn current_platform() -> Platform {
    #[cfg(target_os = "windows")]
    {
        if cfg!(target_arch = "x86_64") {
            Platform::WindowsX64
        } else {
            Platform::WindowsArm64
        }
    }
    #[cfg(target_os = "macos")]
    {
        if cfg!(target_arch = "x86_64") {
            Platform::MacosX64
        } else {
            Platform::MacosArm64
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if cfg!(target_arch = "x86_64") {
            Platform::LinuxX64
        } else {
            Platform::LinuxArm64
        }
    }
}

#[allow(unused_variables)]
fn platform_os(platform: Platform) -> ArtifactOs {
    match platform {
        Platform::WindowsX64 | Platform::WindowsArm64 => ArtifactOs::Windows,
        Platform::MacosX64 | Platform::MacosArm64 => ArtifactOs::Macos,
        Platform::LinuxX64 | Platform::LinuxArm64 => ArtifactOs::Linux,
    }
}

#[allow(unused_variables)]
fn artifact_arch(platform: Platform, artifact: &Artifact) -> bool {
    let wanted = match platform {
        Platform::WindowsX64 | Platform::MacosX64 | Platform::LinuxX64 => Architecture::X64,
        Platform::WindowsArm64 | Platform::MacosArm64 | Platform::LinuxArm64 => Architecture::Arm64,
    };
    artifact.architecture == wanted
}

/// Package ids are stable ids (letters, digits, dots) and versions are
/// release tags; both become path components under the managed root, so
/// they are re-validated defensively here regardless of upstream checks.
pub(crate) fn is_safe_path_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
        && value != "."
        && value != ".."
        && !value.ends_with('.')
        && !value.ends_with(' ')
}

fn validate_install_identities(package_id: &str, version: &str) -> Result<(), StageFailure> {
    for (label, value) in [("package id", package_id), ("version", version)] {
        if !is_safe_path_component(value) {
            return Err(StageFailure::new(
                ErrorCode::InvalidArgument,
                format!("{label} is not a safe managed-root path component"),
            ));
        }
    }
    Ok(())
}

/// Staging layout (version-management.md §2 rule 2):
/// `packages/<package-id>/<version>.staging-<job-id>/`.
fn staging_relative(package_id: &str, version: &str, job_id: &str) -> PathBuf {
    Path::new("packages")
        .join(package_id)
        .join(format!("{version}.staging-{job_id}"))
}

/// Final layout: `packages/<package-id>/<version>/`.
fn final_relative(package_id: &str, version: &str) -> PathBuf {
    Path::new("packages").join(package_id).join(version)
}

/// Maps download-engine failures to stable codes for the event stream.
/// Only NETWORK_ERROR and INTEGRITY_CHECK_FAILED carry data (P3-01 notes);
/// staging I/O failures are INSTALL_FAILED.
fn download_code(error: &DownloadError) -> ErrorCode {
    match error {
        DownloadError::Cancelled => ErrorCode::Cancelled,
        DownloadError::Transport(_) => ErrorCode::NetworkError,
        DownloadError::Io(_) | DownloadError::IoAtPath { .. } => ErrorCode::InstallFailed,
        // Policy/size/checksum violations are all integrity failures of the
        // declared artifact (security-model.md §5).
        DownloadError::InvalidUrl(_)
        | DownloadError::InsecureUrl(_)
        | DownloadError::CredentialsInUrl
        | DownloadError::Redirect(_)
        | DownloadError::SizeLimitExceeded { .. }
        | DownloadError::SizeMismatch { .. }
        | DownloadError::ChecksumMismatch { .. }
        | DownloadError::BadChecksumSpec(_) => ErrorCode::IntegrityCheckFailed,
    }
}

/// Everything a failure/cancel needs to restore the pre-install state.
/// Every field is `None` until that artifact of the installation exists —
/// cleanup never touches anything it did not create (in particular the
/// managed root itself is never a cleanup target).
#[derive(Default)]
struct RollbackPaths {
    staging_dir: Option<PathBuf>,
    artifact_path: Option<PathBuf>,
    final_dir: Option<PathBuf>,
}

impl RollbackPaths {
    fn none() -> Self {
        Self::default()
    }

    fn remove_all(&self) {
        for path in [
            self.staging_dir.as_deref(),
            self.artifact_path.as_deref(),
            self.final_dir.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if path.is_dir() {
                let _ = fs::remove_dir_all(path);
            } else if path.is_file() {
                let _ = fs::remove_file(path);
            }
        }
        // The download engine's own partial file (`destination.part`).
        if let Some(artifact_path) = &self.artifact_path {
            let _ = fs::remove_file(artifact_path.with_extension("part"));
        }
    }
}

/// Context for the terminal chain (`FAILED`/`CANCELLED` →
/// `ROLLING_BACK` → `ROLLED_BACK`): the filesystem/registry cleanup runs
/// **before** the events are recorded, so the recorded state is true.
/// For `msi` installs the vendor undo (`msiexec /x {ProductCode}`) also
/// runs here — a failed install that left a Windows Installer registration
/// behind must not ghost the system.
struct Finish<'a, 'conn> {
    store: &'a JobStore<'conn>,
    repo: InstalledRepo<'conn>,
    job_id: &'a str,
    package_id: &'a str,
    version: &'a str,
    /// The vendor runner (exe/msi adapters); `None` never happens in
    /// practice, `Option` keeps construction simple.
    vendor_runner: Option<&'a dyn ExeRunner>,
    /// Set when a vendor install that registers itself (msi) is about to
    /// run: the ProductCode for the best-effort `msiexec /x` undo.
    vendor_undo: Cell<Option<String>>,
}

impl Finish<'_, '_> {
    fn failed(
        &self,
        current: Stage,
        code: ErrorCode,
        message: &str,
        paths: &RollbackPaths,
        receipt_written: bool,
    ) -> Result<RunOutcome, JobsError> {
        paths.remove_all();
        self.run_vendor_undo();
        if receipt_written {
            self.repo.delete(self.package_id, self.version)?;
        }
        record_terminal_chain(
            self.store,
            self.job_id,
            current,
            Stage::Failed,
            Some(code),
            message,
            "Removing staging data and unregistered receipts",
            "Managed root and registry restored to pre-install state",
        )?;
        Ok(RunOutcome::RolledBack)
    }

    fn cancelled(
        &self,
        current: Stage,
        paths: &RollbackPaths,
        receipt_written: bool,
    ) -> Result<RunOutcome, JobsError> {
        paths.remove_all();
        self.run_vendor_undo();
        if receipt_written {
            self.repo.delete(self.package_id, self.version)?;
        }
        record_terminal_chain(
            self.store,
            self.job_id,
            current,
            Stage::Cancelled,
            None,
            "Cancelled at a safe boundary",
            "Removing staging data and unregistered receipts",
            "Managed root and registry restored to pre-install state",
        )?;
        Ok(RunOutcome::RolledBack)
    }

    /// Best-effort `msiexec /x {ProductCode}`: a failed/cancelled msi
    /// install may have registered the product with Windows Installer
    /// before failing; leaving that registration behind would ghost the
    /// system and break the next install. Failures of the undo itself are
    /// ignored — the rollback must never mask the original error, and a
    /// stubborn registration surfaces at the next install as an honest
    /// msiexec failure.
    fn run_vendor_undo(&self) {
        let Some(product_code) = self.vendor_undo.take() else {
            return;
        };
        let Some(runner) = self.vendor_runner else {
            return;
        };
        // The undo must not be cancellable by the very cancellation that
        // triggered it: it runs with a fresh, unset token.
        let undo_cancel = CancelToken::from_shared(Arc::new(AtomicBool::new(false)));
        let (program, flags) = build_uninstall_command(&msiexec_program(), &product_code);
        let _ = runner.run(&program, &flags, &undo_cancel);
    }
}

/// Runs the real install pipeline. Called by the job manager on the job's
/// worker thread; tests call it within a tokio current-thread runtime.
#[allow(clippy::too_many_lines)]
pub async fn run<T, S>(
    config: &RealInstallConfig<T, S>,
    store: &JobStore<'_>,
    job_id: &str,
    package_id: &str,
    version: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<RunOutcome, JobsError>
where
    T: DownloadHttp,
    S: InstallSource,
{
    let token = CancelToken::from_shared(cancel.clone());
    let managed_root = config.managed_root.clone();
    let finish = Finish {
        store,
        repo: InstalledRepo::new(store.connection()),
        job_id,
        package_id,
        version,
        vendor_runner: Some(config.vendor_runner.as_ref()),
        vendor_undo: Cell::new(None),
    };
    let repo = &finish.repo;

    // ------------------------------------------------------------------
    // RESOLVING
    // ------------------------------------------------------------------
    store.record_event(
        job_id,
        Stage::Resolving,
        None,
        &format!("Resolving {package_id} {version} from the catalog"),
        None,
    )?;
    let mut current = Stage::Resolving;
    let none_paths = RollbackPaths::none();
    if cancel.load(Ordering::Relaxed) {
        return finish.cancelled(current, &none_paths, false);
    }

    // Side-by-side rules (version-management.md §2):
    // - installed + validated -> idempotent "already installed"
    // - 'installing' row (a crash left it; recovery normally cleans it)
    //   or broken -> refuse; the repair path is uninstall + reinstall.
    let existing = match repo.get(package_id, version) {
        Ok(existing) => existing,
        Err(error) => {
            return finish.failed(
                current,
                ErrorCode::DatabaseError,
                &format!("cannot read the install registry: {error}"),
                &none_paths,
                false,
            )
        }
    };
    if let Some(record) = existing {
        match record.status {
            InstalledStatus::Installed => return run_already_installed(store, job_id),
            InstalledStatus::Installing | InstalledStatus::Broken | InstalledStatus::Removing => {
                return finish.failed(
                    current,
                    ErrorCode::InstallFailed,
                    "A previous install of this version did not complete cleanly; \
                     uninstall it before reinstalling",
                    &none_paths,
                    false,
                );
            }
        }
    }

    let artifact = match config.source.resolve(package_id, version) {
        Ok(artifact) => artifact,
        Err(failure) => {
            return finish.failed(current, failure.code, &failure.message, &none_paths, false)
        }
    };
    // Installer-kind gate (installation-lifecycle.md §2): `archive` is the
    // P3-02 adapter; `exe` (P3-06) and `msi` (P3-07) run Windows vendor
    // installers only — a non-Windows build has neither adapter, so the
    // job fails with `UNSUPPORTED_PLATFORM` before anything is downloaded.
    // The remaining kinds are later phases.
    match artifact.installer {
        InstallerKind::Archive => {}
        InstallerKind::Exe | InstallerKind::Msi => {
            if !cfg!(windows) {
                return finish.failed(
                    current,
                    ErrorCode::UnsupportedPlatform,
                    "the exe/msi adapters install Windows vendor installers \
                     only; this build has no such adapter",
                    &none_paths,
                    false,
                );
            }
        }
        InstallerKind::Dmg
        | InstallerKind::Pkg
        | InstallerKind::Appimage
        | InstallerKind::ExternalPackageManager => {
            return finish.failed(
                current,
                ErrorCode::InvalidArgument,
                "Only archive, exe, and msi artifacts install in this \
                 phase; the remaining installer kinds (dmg, pkg, appimage, \
                 external_package_manager) are later phases",
                &none_paths,
                false,
            );
        }
    }
    if let Err(failure) = validate_install_identities(package_id, version) {
        return finish.failed(current, failure.code, &failure.message, &none_paths, false);
    }

    // ------------------------------------------------------------------
    // PREPARING
    // ------------------------------------------------------------------
    store.record_event(
        job_id,
        Stage::Preparing,
        None,
        "Preparing staging directory",
        None,
    )?;
    current = Stage::Preparing;
    if cancel.load(Ordering::Relaxed) {
        return finish.cancelled(current, &none_paths, false);
    }

    let staging_rel = staging_relative(package_id, version, job_id);
    let staging_dir = managed_root.join(&staging_rel);
    let artifact_path =
        managed_root.join(format!("{}.artifact.bin", staging_rel.to_string_lossy()));
    let mut paths = RollbackPaths {
        staging_dir: Some(staging_dir.clone()),
        artifact_path: Some(artifact_path.clone()),
        final_dir: None,
    };
    if let Err(source) = fs::create_dir_all(&staging_dir) {
        return finish.failed(
            current,
            ErrorCode::InstallFailed,
            &format!("cannot create the staging directory: {source}"),
            &paths,
            false,
        );
    }
    // Disk space check (installation-lifecycle.md §3 PREPARING): the
    // declared size plus the same safety margin the download engine
    // enforces.
    let needed = artifact
        .size_bytes
        .saturating_add((artifact.size_bytes / 10).max(64 * 1024));
    match fs4::available_space(&managed_root) {
        Ok(available) if available < needed => {
            return finish.failed(
                current,
                ErrorCode::InstallFailed,
                &format!("Not enough disk space: need about {needed} bytes, {available} available"),
                &paths,
                false,
            );
        }
        Ok(_) => {}
        Err(source) => {
            return finish.failed(
                current,
                ErrorCode::InstallFailed,
                &format!("cannot check free disk space: {source}"),
                &paths,
                false,
            );
        }
    }

    // ------------------------------------------------------------------
    // DOWNLOADING
    // ------------------------------------------------------------------
    store.record_event(
        job_id,
        Stage::Downloading,
        None,
        &format!("Downloading artifact ({} bytes)", artifact.size_bytes),
        None,
    )?;
    current = Stage::Downloading;

    let request = DownloadRequest {
        url: artifact.url.clone(),
        sha256_hex: artifact.sha256.clone(),
        size_bytes: artifact.size_bytes,
    };
    // Real progress: one event per 5% of the declared size. Percentages
    // are computed from real byte counts — never a fake percentage.
    let step = (artifact.size_bytes / 20).max(1);
    let last_recorded = Cell::new(0_u64);
    let progress_error = Cell::new(None::<JobsError>);
    let progress = |bytes: u64| {
        if bytes.saturating_sub(last_recorded.get()) >= step {
            last_recorded.set(bytes);
            let fraction = (bytes as f64 / artifact.size_bytes as f64).min(1.0);
            if let Err(error) = store.record_event(
                job_id,
                Stage::Downloading,
                Some(fraction),
                &format!("Downloaded {bytes} of {} bytes", artifact.size_bytes),
                None,
            ) {
                progress_error.set(Some(error));
            }
        }
    };

    let downloaded = match download_and_verify_with_progress(
        &config.transport,
        &request,
        &artifact_path,
        &token,
        progress,
    )
    .await
    {
        Ok(downloaded) => downloaded,
        Err(error) => {
            let code = download_code(&error);
            if code == ErrorCode::Cancelled {
                return finish.cancelled(current, &paths, false);
            }
            return finish.failed(current, code, &error.to_string(), &paths, false);
        }
    };
    if progress_error.take().is_some() {
        // Progress events could not be persisted; treat as a database
        // failure and roll back what the download staged.
        return finish.failed(
            current,
            ErrorCode::DatabaseError,
            "cannot record download progress",
            &paths,
            false,
        );
    }

    // ------------------------------------------------------------------
    // VERIFYING (the engine verified before renaming; report real values)
    // ------------------------------------------------------------------
    store.record_event(
        job_id,
        Stage::Verifying,
        None,
        &format!(
            "Verified SHA-256 {} ({} bytes)",
            downloaded.sha256_hex, downloaded.size_bytes
        ),
        None,
    )?;
    current = Stage::Verifying;
    if cancel.load(Ordering::Relaxed) {
        return finish.cancelled(current, &paths, false);
    }

    // ------------------------------------------------------------------
    // EXTRACTING (archive adapter) / INSTALLING (vendor adapters)
    // ------------------------------------------------------------------
    let is_vendor = matches!(artifact.installer, InstallerKind::Exe | InstallerKind::Msi);
    let executable = non_empty(&artifact.executable_relative_path).map(str::to_owned);
    let mut archive_root = non_empty(&artifact.archive_root).map(str::to_owned);

    let files: Vec<ReceiptFile>;
    let mut archive_receipt: Option<ArchiveReceipt> = None;
    let mut vendor_receipt: Option<VendorReceiptPart> = None;

    if is_vendor {
        // The vendor adapters (P3-06 exe, P3-07 msi) run the verified
        // vendor installer with flags from the fixed contract tables
        // (devkit-install::exe / devkit-install::msi,
        // installation-lifecycle.md §2.1/§2.2). The installer writes
        // directly into the final directory — no staging extraction, no
        // rename — and INSTALLING covers the installer adapters (state
        // machine: Verifying -> Installing).
        store.record_event(
            job_id,
            Stage::Installing,
            None,
            "Running the vendor installer (managed vendor adapter)",
            None,
        )?;
        current = Stage::Installing;

        // The vendor installer owns its layout: the final directory is
        // registered for rollback **before** the process starts, so a
        // failure/cancel removes whatever the installer already wrote.
        let final_rel = final_relative(package_id, version);
        let final_dir = managed_root.join(&final_rel);
        if final_dir.exists() {
            return finish.failed(
                current,
                ErrorCode::InstallFailed,
                "target version directory already exists; repair = uninstall + reinstall",
                &paths,
                false,
            );
        }
        paths.final_dir = Some(final_dir.clone());

        // Per-kind command construction (devkit-install contract tables;
        // catalog text has no path into either command line) and the run.
        let (technology_label, exit_code, product_code) = match artifact.installer {
            InstallerKind::Exe => {
                // Technology detection reads the **verified** bytes: a
                // file that is not a PE image or carries no contract-table
                // marker is never executed.
                let technology = match detect_technology_from_file(&artifact_path) {
                    Ok(Some(technology)) => technology,
                    Ok(None) => {
                        return finish.failed(
                            current,
                            ErrorCode::InvalidArgument,
                            "verified installer is not in the supported contract \
                             table (the exe adapter covers Inno Setup and NSIS)",
                            &paths,
                            false,
                        );
                    }
                    Err(source) => {
                        return finish.failed(
                            current,
                            ErrorCode::InstallFailed,
                            &format!("cannot read the verified installer: {source}"),
                            &paths,
                            false,
                        );
                    }
                };
                store.record_event(
                    job_id,
                    Stage::Installing,
                    None,
                    &format!(
                        "Vendor installer technology: {} (fixed silent flags)",
                        technology.label()
                    ),
                    None,
                )?;

                // /LOG lands in the staging directory (deleted on
                // rollback); NSIS has no log flag.
                let log_path = match technology {
                    ExeTechnology::InnoSetup => Some(staging_dir.join("install.log")),
                    ExeTechnology::Nsis => None,
                };
                let (program, flags) =
                    build_command(&artifact_path, technology, &final_dir, log_path.as_deref());
                if cancel.load(Ordering::Relaxed) {
                    return finish.cancelled(current, &paths, false);
                }
                // The runner owns the process on the job's behalf: it
                // polls the job's cancellation token (~100 ms) and kills
                // the child on cancel (installation-lifecycle.md §3 rule 3:
                // the job owns a process it started).
                let outcome = match config.vendor_runner.run(&program, &flags, &token) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        let code = error.code();
                        return finish.failed(current, code, &error.to_string(), &paths, false);
                    }
                };
                if outcome.cancelled {
                    return finish.cancelled(current, &paths, false);
                }
                if outcome.exit_code != Some(0) {
                    return finish.failed(
                        current,
                        ErrorCode::InstallFailed,
                        &format!(
                            "vendor installer did not succeed (exit code {:?}); the \
                             installation directory was removed",
                            outcome.exit_code
                        ),
                        &paths,
                        false,
                    );
                }
                store.record_event(
                    job_id,
                    Stage::Installing,
                    None,
                    "Vendor installer finished successfully",
                    None,
                )?;
                (technology.label().to_owned(), outcome.exit_code, None)
            }
            InstallerKind::Msi => {
                // Container check first: a file without the OLE compound
                // magic is not an MSI package and is never executed nor
                // opened (INVALID_ARGUMENT, like a wrong exe technology).
                match is_msi_package_file(&artifact_path) {
                    Ok(true) => {}
                    Ok(false) => {
                        return finish.failed(
                            current,
                            ErrorCode::InvalidArgument,
                            "verified artifact is not a Windows Installer \
                             package (msi)",
                            &paths,
                            false,
                        );
                    }
                    Err(source) => {
                        return finish.failed(
                            current,
                            ErrorCode::InstallFailed,
                            &format!("cannot read the verified package: {source}"),
                            &paths,
                            false,
                        );
                    }
                }
                store.record_event(
                    job_id,
                    Stage::Installing,
                    None,
                    "Windows Installer package (fixed silent flags)",
                    None,
                )?;

                // The ProductCode is read from the verified package's
                // Property table (read-only database open; the file is
                // never executed for this). It drives the rollback undo
                // and the receipt-driven uninstall, so a package without
                // it is refused, never run.
                let product_code = match read_product_code(&artifact_path) {
                    Ok(code) => code,
                    Err(error) => {
                        let code = error.code();
                        return finish.failed(
                            current,
                            code,
                            &format!("cannot read the MSI ProductCode: {error}"),
                            &paths,
                            false,
                        );
                    }
                };
                // From here on, a failed/cancelled install runs the
                // msiexec /x undo: the package may have registered itself
                // with Windows Installer before failing.
                finish.vendor_undo.set(Some(product_code.clone()));

                let (program, flags) =
                    msi_build_command(&msiexec_program(), &artifact_path, &final_dir);
                if cancel.load(Ordering::Relaxed) {
                    return finish.cancelled(current, &paths, false);
                }
                let outcome = match config.vendor_runner.run(&program, &flags, &token) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        let code = error.code();
                        return finish.failed(current, code, &error.to_string(), &paths, false);
                    }
                };
                if outcome.cancelled {
                    return finish.cancelled(current, &paths, false);
                }
                // 0 = success; 3010 = success with a recommended reboot
                // (suppressed by /norestart; the files are installed).
                if !msi_is_success_exit_code(outcome.exit_code) {
                    return finish.failed(
                        current,
                        ErrorCode::InstallFailed,
                        &format!(
                            "msiexec did not succeed (exit code {:?}); the \
                             installation directory was removed and the \
                             package registration undone (msiexec /x)",
                            outcome.exit_code
                        ),
                        &paths,
                        false,
                    );
                }
                store.record_event(
                    job_id,
                    Stage::Installing,
                    None,
                    "Vendor installer finished successfully",
                    None,
                )?;
                ("msi".to_owned(), outcome.exit_code, Some(product_code))
            }
            _ => unreachable!("is_vendor matched Exe/Msi only"),
        };

        // The receipt is built by observing what the installer produced. A
        // vendor that ignored the install-directory flag leaves the
        // receipt empty, which fails validation below.
        let walked = match walk_installed_files(&final_dir) {
            Ok(walked) => walked,
            Err(source) => {
                return finish.failed(
                    current,
                    ErrorCode::InstallFailed,
                    &format!("cannot inventory the installation directory: {source}"),
                    &paths,
                    false,
                );
            }
        };
        files = walked
            .iter()
            .map(|file| ReceiptFile {
                relative_path: file.relative_path.to_string_lossy().into_owned(),
                size_bytes: file.size_bytes,
                sha256_hex: file.sha256_hex.clone(),
            })
            .collect();
        vendor_receipt = Some(VendorReceiptPart {
            technology: technology_label,
            exit_code,
            product_code,
        });
        archive_root = None;
    } else {
        store.record_event(
            job_id,
            Stage::Extracting,
            None,
            "Extracting archive (managed archive adapter)",
            None,
        )?;
        current = Stage::Extracting;

        let format = read_archive_format(&artifact_path);
        let Some(format) = format else {
            return finish.failed(
                current,
                ErrorCode::IntegrityCheckFailed,
                "verified artifact is not a supported archive (zip, tar, tar.gz)",
                &paths,
                false,
            );
        };
        let extract = match extract_file(&artifact_path, &staging_dir, &config.limits, &token) {
            Ok(extract) => extract,
            Err(error) => {
                let code = error.code();
                if code == ErrorCode::Cancelled {
                    return finish.cancelled(current, &paths, false);
                }
                return finish.failed(current, code, &error.to_string(), &paths, false);
            }
        };
        store.record_event(
            job_id,
            Stage::Extracting,
            None,
            &format!(
                "Extracted {} files ({} bytes)",
                extract.files.len(),
                extract.total_bytes
            ),
            None,
        )?;
        files = extract
            .files
            .iter()
            .map(|file| ReceiptFile {
                relative_path: file.relative_path.to_string_lossy().into_owned(),
                size_bytes: file.size_bytes,
                sha256_hex: file.sha256_hex.clone(),
            })
            .collect();
        archive_receipt = Some(ArchiveReceipt {
            format: format_label(format),
            sha256_hex: downloaded.sha256_hex,
            size_bytes: downloaded.size_bytes,
        });
    }

    // ------------------------------------------------------------------
    // CONFIGURING (receipt row; archive: atomic rename first)
    // ------------------------------------------------------------------
    store.record_event(
        job_id,
        Stage::Configuring,
        None,
        "Writing install receipt",
        None,
    )?;
    current = Stage::Configuring;
    if let Err(source) = fs::remove_file(&artifact_path) {
        return finish.failed(
            current,
            ErrorCode::InstallFailed,
            &format!("cannot remove the staged artifact: {source}"),
            &paths,
            false,
        );
    }
    let final_rel = final_relative(package_id, version);
    let final_dir = managed_root.join(&final_rel);
    if is_vendor {
        // The staging directory holds at most the Inno install log and the
        // vendor installer already wrote the final directory: remove the
        // staging debris best-effort (the log may still be held open by a
        // dying installer; a leftover staging directory never blocks
        // validation and rollback would have removed it anyway).
        let _ = fs::remove_dir_all(&staging_dir);
    } else {
        // Never overwrite an existing version directory
        // (version-management.md §2 rule 1). (The vendor branch checked
        // this before running the installer.)
        if final_dir.exists() {
            return finish.failed(
                current,
                ErrorCode::InstallFailed,
                "target version directory already exists; repair = uninstall + reinstall",
                &paths,
                false,
            );
        }
        if let Err(source) = fs::rename(&staging_dir, &final_dir) {
            return finish.failed(
                current,
                ErrorCode::InstallFailed,
                &format!("cannot move the installation into place: {source}"),
                &paths,
                false,
            );
        }
    }
    paths.final_dir = Some(final_dir);

    let installed_at = match devkit_registry::error::now_utc_rfc3339() {
        Ok(value) => value,
        Err(error) => {
            return finish.failed(
                current,
                ErrorCode::DatabaseError,
                &format!("cannot timestamp the install receipt: {error}"),
                &paths,
                false,
            )
        }
    };
    let receipt = InstallReceipt {
        receipt_version: 2,
        package_id: package_id.to_owned(),
        version: version.to_owned(),
        installer: match artifact.installer {
            InstallerKind::Exe => "exe",
            InstallerKind::Msi => "msi",
            _ => "archive",
        }
        .to_owned(),
        archive: archive_receipt,
        vendor: vendor_receipt,
        install_path: final_rel.to_string_lossy().into_owned(),
        files,
        executable_relative_path: executable,
        archive_root,
        job_id: job_id.to_owned(),
        installed_at: installed_at.clone(),
    };
    let receipt_json = serde_json::to_string(&receipt)
        .map_err(|error| JobsError::Internal(format!("receipt serialization failed: {error}")))?;
    let record = InstalledRecord {
        package_id: package_id.to_owned(),
        version: version.to_owned(),
        install_path: receipt.install_path.clone(),
        status: InstalledStatus::Installing,
        receipt_json,
        installed_at,
    };
    if let Err(error) = repo.register(&record) {
        return finish.failed(
            current,
            ErrorCode::DatabaseError,
            &format!("cannot write the install receipt: {error}"),
            &paths,
            false,
        );
    }
    let receipt_written = true;

    // ------------------------------------------------------------------
    // VALIDATING
    // ------------------------------------------------------------------
    store.record_event(
        job_id,
        Stage::Validating,
        None,
        "Validating installation",
        None,
    )?;
    current = Stage::Validating;
    if let Some(hook) = &config.validation_hook {
        hook(&receipt);
    }
    if let Err(reason) = validate_installation(&managed_root, &receipt) {
        return finish.failed(
            current,
            ErrorCode::InstallFailed,
            &format!("post-install validation failed: {reason}"),
            &paths,
            receipt_written,
        );
    }
    if cancel.load(Ordering::Relaxed) {
        // Cancellation after validation completed: the registration is
        // completed first (bridge contract § Cancellation semantics),
        // then the job reports already-finished.
        if let Err(error) = repo.update_status(package_id, version, InstalledStatus::Installed) {
            return finish.failed(
                current,
                ErrorCode::DatabaseError,
                &format!("cannot mark the installation installed: {error}"),
                &paths,
                receipt_written,
            );
        }
        store.record_event(
            job_id,
            Stage::Completed,
            None,
            "Installation complete (cancellation arrived after validation)",
            None,
        )?;
        return Ok(RunOutcome::Completed);
    }
    if let Err(error) = repo.update_status(package_id, version, InstalledStatus::Installed) {
        return finish.failed(
            current,
            ErrorCode::DatabaseError,
            &format!("cannot mark the installation installed: {error}"),
            &paths,
            receipt_written,
        );
    }

    // ------------------------------------------------------------------
    // COMPLETED
    // ------------------------------------------------------------------
    store.record_event(
        job_id,
        Stage::Completed,
        None,
        &format!(
            "Installation complete: {} files, {} bytes",
            receipt.files.len(),
            receipt
                .files
                .iter()
                .map(|file| file.size_bytes)
                .sum::<u64>()
        ),
        None,
    )?;
    Ok(RunOutcome::Completed)
}

/// Returns `Some` for non-empty manifest strings, `None` otherwise.
fn non_empty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn read_archive_format(artifact_path: &Path) -> Option<ArchiveFormat> {
    let mut file = fs::File::open(artifact_path).ok()?;
    let mut prefix = [0_u8; 512];
    let mut read = 0_usize;
    while read < prefix.len() {
        match file.read(&mut prefix[read..]) {
            Ok(0) => break,
            Ok(count) => read += count,
            Err(_) => return None,
        }
    }
    detect_format(&prefix[..read])
}

fn format_label(format: ArchiveFormat) -> String {
    match format {
        ArchiveFormat::Zip => "zip".to_owned(),
        ArchiveFormat::Tar => "tar".to_owned(),
        ArchiveFormat::TarGz => "tar.gz".to_owned(),
    }
}

/// The already-installed path (version-management.md §2 rule 1: "the
/// install job completes as 'already installed'"): walk the full stage
/// chain with honest no-op messages, touching nothing.
fn run_already_installed(store: &JobStore, job_id: &str) -> Result<RunOutcome, JobsError> {
    let chain = [
        Stage::Resolving,
        Stage::Preparing,
        Stage::Downloading,
        Stage::Verifying,
        Stage::Extracting,
        Stage::Configuring,
        Stage::Validating,
        Stage::Completed,
    ];
    let mut current = Stage::Queued;
    for stage in chain {
        if stage != Stage::Resolving && !current.can_transition(stage) {
            return Err(JobsError::Internal(format!(
                "invalid state transition {current} -> {stage}"
            )));
        }
        let message = if stage == Stage::Completed {
            "Already installed; nothing to do (idempotent re-install)"
        } else {
            "Version already installed; stage skipped (idempotent re-install)"
        };
        store.record_event(job_id, stage, None, message, None)?;
        current = stage;
    }
    Ok(RunOutcome::Completed)
}

/// Post-install validation (installation-lifecycle.md §1 `validate`):
/// every receipt file exists with its declared size inside the
/// installation directory, nothing escaped the managed root, and the
/// declared entry point is present. Re-hashing every file is a separate
/// deep check (backlog) — extraction already hashed each file.
pub fn validate_installation(managed_root: &Path, receipt: &InstallReceipt) -> Result<(), String> {
    let install_root = managed_root.join(&receipt.install_path);
    if let Err(error) = ensure_within(managed_root, &install_root) {
        return Err(format!("install path escapes the managed root: {error}"));
    }
    if !install_root.is_dir() {
        return Err("installation directory is missing".to_owned());
    }
    if receipt.files.is_empty() {
        return Err("receipt records no files".to_owned());
    }
    for file in &receipt.files {
        let path = install_root.join(&file.relative_path);
        if let Err(error) = ensure_within(&install_root, &path) {
            return Err(format!("receipt path escapes the installation: {error}"));
        }
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => return Err(format!("receipt file is missing: {}", file.relative_path)),
        };
        if !metadata.is_file() {
            return Err(format!(
                "receipt path is not a file: {}",
                file.relative_path
            ));
        }
        if metadata.len() != file.size_bytes {
            return Err(format!(
                "receipt file has an unexpected size: {} ({} expected, {} found)",
                file.relative_path,
                file.size_bytes,
                metadata.len()
            ));
        }
    }
    if let Some(executable) = &receipt.executable_relative_path {
        let path = install_root.join(executable);
        if let Err(error) = ensure_within(&install_root, &path) {
            return Err(format!("entry point escapes the installation: {error}"));
        }
        if !path.is_file() {
            return Err(format!("declared entry point is missing: {executable}"));
        }
    }
    Ok(())
}

/// Outcome of an exact-version uninstall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallOutcome {
    /// No receipt row for this (package, version).
    NotFound,
    /// Removed exactly this version; `cleared_active` reports whether the
    /// global active mapping pointed at it (version-management.md §2
    /// rule 3: clearing requires user confirmation — the caller's job).
    Removed {
        files_removed: usize,
        cleared_active: bool,
    },
}

/// Uninstall exactly one version (P3-03 acceptance): reads the receipt,
/// verifies containment inside the managed root, clears the active
/// mapping when it points at this version, deletes the version directory,
/// and removes the receipt row. Sibling versions are untouched. The
/// consent dialog for uninstalling the *active* version is a UI
/// responsibility (DESIGN.md §8); this function only reports what it
/// cleared.
///
/// P3-07: for `msi` receipts the receipt-driven vendor uninstall runs
/// first — `msiexec /x {ProductCode} /qn /norestart` with the injected
/// runner — so the Windows Installer registration goes away together
/// with the directory (no ghost ARP entry). Exit 0/3010 = uninstalled;
/// 1605 ("product not registered") falls through to the directory
/// removal only; any other code fails the uninstall with the row's
/// status restored. The vendor uninstall is not cancellable (fresh
/// token): it is a local, silent, quick operation, and aborting it
/// halfway would leave a worse state than finishing it.
pub fn uninstall_version(
    managed_root: &Path,
    conn: &Connection,
    package_id: &str,
    version: &str,
    vendor_runner: &dyn ExeRunner,
) -> Result<UninstallOutcome, JobsError> {
    let repo = InstalledRepo::new(conn);
    let Some(record) = repo.get(package_id, version)? else {
        return Ok(UninstallOutcome::NotFound);
    };
    let receipt: InstallReceipt = serde_json::from_str(&record.receipt_json)
        .map_err(|error| JobsError::Internal(format!("unreadable receipt: {error}")))?;
    if record.install_path != receipt.install_path {
        return Err(JobsError::Internal(
            "registry row and receipt disagree about the install path".to_owned(),
        ));
    }
    let install_root = managed_root.join(&record.install_path);
    ensure_within(managed_root, &install_root).map_err(|error| {
        JobsError::Internal(format!("install path escapes the managed root: {error}"))
    })?;

    // `removing` while files are deleted (schema status values).
    repo.update_status(package_id, version, InstalledStatus::Removing)?;

    // Receipt-driven vendor uninstall (msi receipts only).
    if receipt.installer == "msi" {
        let Some(product_code) = receipt
            .vendor
            .as_ref()
            .and_then(|part| part.product_code.as_deref())
        else {
            repo.update_status(package_id, version, record.status)?;
            return Err(JobsError::Internal(
                "msi receipt has no ProductCode to uninstall with".to_owned(),
            ));
        };
        let undo_cancel = CancelToken::from_shared(Arc::new(AtomicBool::new(false)));
        let (program, flags) = build_uninstall_command(&msiexec_program(), product_code);
        let uninstall_result = match vendor_runner.run(&program, &flags, &undo_cancel) {
            Ok(outcome) if outcome.cancelled => Err("msiexec /x was cancelled".to_owned()),
            Ok(outcome) if outcome.exit_code == Some(MSI_EXIT_UNKNOWN_PRODUCT) => {
                // Not registered (e.g. our rollback already undid it, or
                // a foreign cleanup): only the managed directory remains.
                Ok(())
            }
            Ok(outcome) if msi_is_success_exit_code(outcome.exit_code) => Ok(()),
            Ok(outcome) => Err(format!(
                "msiexec /x did not succeed (exit code {:?})",
                outcome.exit_code
            )),
            Err(error) => Err(error.to_string()),
        };
        if let Err(message) = uninstall_result {
            // Restore the row's status; the user can retry.
            let _ = repo.update_status(package_id, version, record.status);
            return Err(JobsError::Vendor(message));
        }
    }

    let cleared_active = repo.clear_active_if_points_at(package_id, version)?;
    if cleared_active {
        // The shim (if any) pointed at this version; best-effort cleanup —
        // a leftover shim is derived state a later activation rewrites, but
        // removing it here keeps the shims directory honest.
        crate::environment::remove_shims_referencing(managed_root, &record.install_path);
    }
    if install_root.exists() {
        if let Err(source) = fs::remove_dir_all(&install_root) {
            // Restore the row's status; the user can retry.
            let _ = repo.update_status(package_id, version, record.status);
            return Err(JobsError::Io(source));
        }
    }
    if !repo.delete(package_id, version)? {
        let _ = repo.update_status(package_id, version, record.status);
        return Err(JobsError::Internal(
            "receipt row vanished during uninstall".to_owned(),
        ));
    }
    Ok(UninstallOutcome::Removed {
        files_removed: receipt.files.len(),
        cleared_active,
    })
}

/// Removes what a dead job may have left behind (staging directory,
/// artifact file, `.part` file, an `installing`-status receipt row).
/// Called by the manager's orphan recovery when it knows the managed
/// root. `Ok(false)` = nothing was left behind.
pub fn cleanup_abandoned(
    managed_root: &Path,
    conn: &Connection,
    package_id: &str,
    version: &str,
    job_id: &str,
) -> Result<bool, JobsError> {
    let staging_rel = staging_relative(package_id, version, job_id);
    let staging_dir = managed_root.join(&staging_rel);
    let artifact_path =
        managed_root.join(format!("{}.artifact.bin", staging_rel.to_string_lossy()));
    let mut cleaned = false;
    for path in [
        &staging_dir,
        &artifact_path,
        &artifact_path.with_extension("part"),
    ] {
        let removed = if path.is_dir() {
            fs::remove_dir_all(path).is_ok()
        } else if path.is_file() {
            fs::remove_file(path).is_ok()
        } else {
            false
        };
        cleaned |= removed;
    }
    let repo = InstalledRepo::new(conn);
    if let Some(record) = repo.get(package_id, version)? {
        if record.status == InstalledStatus::Installing {
            repo.delete(package_id, version)?;
            cleaned = true;
        }
    }
    Ok(cleaned)
}
