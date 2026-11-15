//! P3-03 pipeline tests: fixture HTTP server → download → verify →
//! extract → receipt → validate → registry (testing-strategy.md rule 1:
//! everything local — loopback server, temp managed root, committed
//! fixture archive; never a real vendor host).
//!
//! Covering the acceptance criteria:
//! - an end-to-end engine test through the `JobManager` passes and leaves
//!   the receipt + files exactly where documented;
//! - a failure at any stage (checksum mismatch, validation) triggers the
//!   rollback chain and leaves managed root + registry pre-install clean;
//! - mid-download cancellation rolls back cleanly;
//! - two versions of one package coexist as separate receipts; uninstall
//!   removes exactly one version (and clears the active mapping when it
//!   pointed at it);
//! - orphan recovery removes staging data a crashed job left behind.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use devkit_core::error::ErrorCode;
use devkit_core::manifest::{
    Architecture, Artifact, ArtifactOs, Category, Channel, InstallerKind, PackageManifest,
    PackageVersion, Platform, VersionPolicyName, SUPPORTED_SCHEMA_VERSION,
};
use devkit_download::ReqwestDownloadHttp;
use devkit_install::ArchiveLimits;
use devkit_jobs::pipeline::{
    uninstall_version, validate_installation, InstallReceipt, InstallSource, RealInstallConfig,
    ReceiptFile, RegistrySource, StageFailure, UninstallOutcome, ValidationHook,
};
use devkit_jobs::{JobKind, JobManager, JobStore, RunOutcome, Stage};
use devkit_registry::installed::InstalledRepo;
use devkit_registry::package_cache::PackageCache;
use devkit_registry::{Database, InstalledRecord, InstalledStatus};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const PACKAGE_ID: &str = "devkit.test.tool";

/// The committed P3-02 fixture archive, read by path relative to this
/// crate (sharing it proves the exact same bytes flow download →
/// extraction → receipt).
fn happy_zip() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../devkit-install/tests/fixtures/happy.zip");
    std::fs::read(path).expect("read committed happy.zip fixture")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Serves the given bytes with a Content-Length on an ephemeral loopback
/// port. Daemon thread; the test process terminates it.
fn spawn_serving(bytes: Vec<u8>) -> u16 {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind loopback fixture server");
    let port = match server.server_addr() {
        tiny_http::ListenAddr::IP(address) => address.port(),
        #[allow(unreachable_patterns)]
        other => panic!("unexpected listen address: {other:?}"),
    };
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let _ = request.respond(tiny_http::Response::from_data(bytes.clone()));
        }
    });
    port
}

/// Trickle server: small chunks with a pause between reads so a cancel
/// request reliably lands mid-download (the happy.zip fixture is only
/// ~1.5 KiB, so coarse chunks would finish before the cancel).
fn spawn_trickling(bytes: Vec<u8>) -> u16 {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind loopback fixture server");
    let port = match server.server_addr() {
        tiny_http::ListenAddr::IP(address) => address.port(),
        #[allow(unreachable_patterns)]
        other => panic!("unexpected listen address: {other:?}"),
    };
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let _ = request.respond(tiny_http::Response::new(
                tiny_http::StatusCode(200),
                Vec::new(),
                Trickle {
                    bytes: bytes.clone(),
                    position: 0,
                },
                None,
                None,
            ));
        }
    });
    port
}

struct Trickle {
    bytes: Vec<u8>,
    position: usize,
}

impl Read for Trickle {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::thread::sleep(Duration::from_millis(50));
        let remaining = self.bytes.len() - self.position;
        let count = buf.len().min(256).min(remaining);
        buf[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

/// Fixture source: hands back the prepared artifact regardless of the
/// requested version, or a prepared failure.
struct FixtureSource {
    artifact: Artifact,
    failure: Option<StageFailure>,
}

impl InstallSource for FixtureSource {
    fn resolve(&self, _package_id: &str, _version: &str) -> Result<Artifact, StageFailure> {
        match &self.failure {
            Some(failure) => Err(failure.clone()),
            None => Ok(self.artifact.clone()),
        }
    }
}

fn artifact_for(port: u16, bytes: &[u8], installer: InstallerKind) -> Artifact {
    Artifact {
        os: ArtifactOs::Windows,
        architecture: Architecture::X64,
        installer,
        url: format!("http://127.0.0.1:{port}/tool.zip"),
        sha256: sha256_hex(bytes),
        size_bytes: bytes.len() as u64,
        signature_url: None,
        publisher_identity: String::new(),
        archive_root: String::new(),
        executable_relative_path: "app/bin/tool.exe".to_owned(),
    }
}

struct TestEnv {
    _db_dir: TempDir,
    managed_root: TempDir,
    db_path: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let db_dir = TempDir::new().unwrap();
        let db_path = db_dir.path().join("state").join("devkit.sqlite");
        Self {
            _db_dir: db_dir,
            managed_root: TempDir::new().unwrap(),
            db_path,
        }
    }

    fn manager(&self) -> JobManager {
        JobManager::open_with(&self.db_path, Some(self.managed_root.path())).unwrap()
    }

    fn db(&self) -> Database {
        Database::open(&self.db_path).unwrap()
    }
}

fn config(
    env: &TestEnv,
    source: FixtureSource,
    hook: Option<ValidationHook>,
) -> RealInstallConfig<ReqwestDownloadHttp, FixtureSource> {
    RealInstallConfig {
        managed_root: env.managed_root.path().to_path_buf(),
        transport: ReqwestDownloadHttp::new().unwrap(),
        source,
        limits: ArchiveLimits::default(),
        validation_hook: hook,
    }
}

fn fixture_source(port: u16, bytes: &[u8]) -> FixtureSource {
    FixtureSource {
        artifact: artifact_for(port, bytes, InstallerKind::Archive),
        failure: None,
    }
}

/// Stage/progress/error triples of a job's event log.
fn events(manager: &JobManager, job_id: &str) -> Vec<(Stage, Option<f64>, Option<String>)> {
    manager
        .watch_job_events(job_id, 0)
        .unwrap()
        .into_iter()
        .map(|event| {
            (
                Stage::parse(&event.stage).unwrap(),
                event.progress,
                event.error_code,
            )
        })
        .collect()
}

fn stages(manager: &JobManager, job_id: &str) -> Vec<Stage> {
    events(manager, job_id)
        .into_iter()
        .map(|(stage, _, _)| stage)
        .collect()
}

fn install_dir(managed_root: &Path, version: &str) -> PathBuf {
    managed_root.join("packages").join(PACKAGE_ID).join(version)
}

/// Receipt paths are platform-native (`app\bin\tool.exe` on Windows);
/// test assertions compare against the manifest's forward-slash form.
fn slashed(path: &str) -> String {
    path.replace('\\', "/")
}

/// Seeds the catalog cache with the package row the
/// `installed_versions` FK points at (production flow: the cache is
/// populated by the catalog client before any install is requested).
fn seed_catalog(env: &TestEnv) {
    let db = env.db();
    PackageCache::new(db.connection())
        .upsert(&catalog_manifest(
            "https://releases.example.invalid/tool.zip".to_owned(),
        ))
        .unwrap();
}

fn repo_row(env: &TestEnv, version: &str) -> Option<InstalledRecord> {
    InstalledRepo::new(env.db().connection())
        .get(PACKAGE_ID, version)
        .unwrap()
}

// ---------------------------------------------------------------------------
// End-to-end happy path
// ---------------------------------------------------------------------------

#[test]
fn end_to_end_install_downloads_extracts_registers_and_validates() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let manager = env.manager();
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), None),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();

    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::Completed);

    // Event chain walks the documented machine with real download
    // progress fractions (no "(simulated)" anywhere).
    let chain = stages(&manager, &job_id);
    for stage in [
        Stage::Queued,
        Stage::Resolving,
        Stage::Preparing,
        Stage::Downloading,
        Stage::Verifying,
        Stage::Extracting,
        Stage::Configuring,
        Stage::Validating,
        Stage::Completed,
    ] {
        assert!(
            chain.contains(&stage),
            "missing stage {stage:?} in {chain:?}"
        );
    }
    let all_events = events(&manager, &job_id);
    assert!(
        all_events.iter().all(|(_, _, error)| error.is_none()),
        "no error codes on the happy path"
    );
    let download_progress: Vec<f64> = all_events
        .iter()
        .filter(|(stage, _, _)| *stage == Stage::Downloading)
        .filter_map(|(_, progress, _)| *progress)
        .collect();
    assert!(!download_progress.is_empty(), "no download progress events");
    assert!(
        download_progress
            .iter()
            .all(|fraction| (0.0..=1.0).contains(fraction)),
        "progress outside 0.0..=1.0: {download_progress:?}"
    );

    // Receipt row: one per (package, version), status installed, path
    // managed-root-relative.
    let db = env.db();
    let repo = InstalledRepo::new(db.connection());
    let record = repo.get(PACKAGE_ID, "1.0.0").unwrap().expect("receipt row");
    assert_eq!(record.status, InstalledStatus::Installed);
    let expected_relative = install_dir(env.managed_root.path(), "1.0.0")
        .strip_prefix(env.managed_root.path())
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(record.install_path, expected_relative);

    // Receipt content matches the fixture archive exactly.
    let receipt: InstallReceipt = serde_json::from_str(&record.receipt_json).unwrap();
    assert_eq!(receipt.installer, "archive");
    assert_eq!(receipt.archive.format, "zip");
    assert_eq!(receipt.archive.sha256_hex, sha256_hex(&bytes));
    assert_eq!(receipt.archive.size_bytes, bytes.len() as u64);
    assert_eq!(
        receipt.executable_relative_path.as_deref(),
        Some("app/bin/tool.exe")
    );
    let mut names: Vec<String> = receipt
        .files
        .iter()
        .map(|file| slashed(&file.relative_path))
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "app/README.md",
            "app/bin/readme.txt",
            "app/bin/tool.exe",
            "app/lib/core.dat"
        ]
    );

    // The extracted bytes match the receipt's per-file hashes.
    let install_root = install_dir(env.managed_root.path(), "1.0.0");
    for file in &receipt.files {
        let on_disk = sha256_hex(&std::fs::read(install_root.join(&file.relative_path)).unwrap());
        assert_eq!(on_disk, file.sha256_hex, "file {}", file.relative_path);
    }

    // Staging directory removed; exactly one version directory remains.
    let package_root = env.managed_root.path().join("packages").join(PACKAGE_ID);
    let mut entries: Vec<String> = std::fs::read_dir(&package_root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        vec!["1.0.0"],
        "only the final version dir may remain"
    );

    // validate_installation passes on the untouched installation.
    validate_installation(env.managed_root.path(), &receipt).unwrap();
}

// ---------------------------------------------------------------------------
// Rollback chains
// ---------------------------------------------------------------------------

#[test]
fn checksum_mismatch_fails_with_integrity_code_and_rolls_back_clean() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let manager = env.manager();
    let mut artifact = artifact_for(port, &bytes, InstallerKind::Archive);
    // Declare a checksum that cannot match what the server serves.
    artifact.sha256 = "0".repeat(64);
    let source = FixtureSource {
        artifact,
        failure: None,
    };
    let job_id = manager
        .enqueue_real_install(config(&env, source, None), PACKAGE_ID, "1.0.0")
        .unwrap();

    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);
    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(
        snapshot.error_code.as_deref(),
        Some(ErrorCode::IntegrityCheckFailed.as_str())
    );
    assert_eq!(Stage::parse(&snapshot.state), Some(Stage::RolledBack));

    // Managed root and registry are pre-install clean.
    assert!(repo_row(&env, "1.0.0").is_none());
    assert!(!install_dir(env.managed_root.path(), "1.0.0").exists());
    let package_root = env.managed_root.path().join("packages").join(PACKAGE_ID);
    if package_root.exists() {
        assert!(std::fs::read_dir(&package_root).unwrap().next().is_none());
    }
}

#[test]
fn cancellation_mid_download_stops_and_rolls_back_clean() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_trickling(bytes.clone());
    let manager = env.manager();
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), None),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();

    // Land the cancel mid-download (the trickle paces the transfer).
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(
        manager.cancel_job(&job_id).unwrap(),
        devkit_jobs::CancelOutcome::Accepted
    );
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);

    let chain = stages(&manager, &job_id);
    assert!(chain.contains(&Stage::Cancelled), "chain: {chain:?}");
    assert!(chain.contains(&Stage::RolledBack), "chain: {chain:?}");
    // Cancellation is not an error: the job snapshot carries no error
    // code (the terminal chain's `Cancelled` stage says it all).
    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(snapshot.error_code, None);
    assert!(repo_row(&env, "1.0.0").is_none());
    assert!(!install_dir(env.managed_root.path(), "1.0.0").exists());
}

#[test]
fn validation_failure_rolls_back_the_registered_receipt_and_files() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let manager = env.manager();
    // Test-only hook: damage one extracted file after CONFIGURING so the
    // VALIDATING stage fails the size check.
    let victim = env
        .managed_root
        .path()
        .join("packages")
        .join(PACKAGE_ID)
        .join("1.0.0")
        .join("app/bin/readme.txt");
    let hook: ValidationHook = Box::new(move |_| std::fs::write(&victim, b"damaged").unwrap());
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), Some(hook)),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();

    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);
    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(
        snapshot.error_code.as_deref(),
        Some(ErrorCode::InstallFailed.as_str())
    );
    // Post-install validation failed → installation is NOT registered
    // (installation-lifecycle.md §5) and staging is cleaned.
    assert!(repo_row(&env, "1.0.0").is_none());
    assert!(!install_dir(env.managed_root.path(), "1.0.0").exists());
}

#[test]
fn version_not_found_fails_with_the_stable_code_and_rolls_back() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes);
    let manager = env.manager();
    let source = FixtureSource {
        artifact: artifact_for(port, &[], InstallerKind::Archive),
        failure: Some(StageFailure::new(
            ErrorCode::VersionNotFound,
            "version 9.9.9 of the test tool is not in the catalog cache",
        )),
    };
    let job_id = manager
        .enqueue_real_install(config(&env, source, None), PACKAGE_ID, "9.9.9")
        .unwrap();

    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);
    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(snapshot.error_code.as_deref(), Some("VERSION_NOT_FOUND"));
    assert!(repo_row(&env, "9.9.9").is_none());
}

#[test]
fn non_archive_installer_kind_is_rejected_before_any_network_work() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let manager = env.manager();
    // The exe adapter is P3-06 scope: resolution must refuse before any
    // download happens.
    let source = FixtureSource {
        artifact: artifact_for(port, &bytes, InstallerKind::Exe),
        failure: None,
    };
    let job_id = manager
        .enqueue_real_install(config(&env, source, None), PACKAGE_ID, "1.0.0")
        .unwrap();

    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);
    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(snapshot.error_code.as_deref(), Some("INVALID_ARGUMENT"));
    assert!(repo_row(&env, "1.0.0").is_none());
}

// ---------------------------------------------------------------------------
// Side-by-side versions, uninstall, idempotence
// ---------------------------------------------------------------------------

#[test]
fn side_by_side_versions_coexist_as_separate_receipts_and_uninstall_is_exact() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let manager = env.manager();
    let first = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), None),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    assert_eq!(manager.wait(&first).unwrap(), RunOutcome::Completed);
    // Serialized deliberately: two concurrent SQLite writers would race
    // busy timeouts; concurrency is not what this test is about.
    let second = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), None),
            PACKAGE_ID,
            "1.1.0",
        )
        .unwrap();
    assert_eq!(manager.wait(&second).unwrap(), RunOutcome::Completed);

    // Two independent receipts and directories.
    let db = env.db();
    let repo = InstalledRepo::new(db.connection());
    let versions = repo.list_for_package(PACKAGE_ID).unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|record| record.version.as_str())
            .collect::<Vec<_>>(),
        vec!["1.0.0", "1.1.0"]
    );
    assert!(install_dir(env.managed_root.path(), "1.0.0")
        .join("app/README.md")
        .exists());
    assert!(install_dir(env.managed_root.path(), "1.1.0")
        .join("app/README.md")
        .exists());

    // Uninstall exactly one version; the sibling is untouched.
    let outcome = uninstall_version(
        env.managed_root.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.0.0",
    )
    .unwrap();
    assert_eq!(
        outcome,
        UninstallOutcome::Removed {
            files_removed: 4,
            cleared_active: false
        }
    );
    assert!(!install_dir(env.managed_root.path(), "1.0.0").exists());
    assert!(install_dir(env.managed_root.path(), "1.1.0")
        .join("app/README.md")
        .exists());
    assert!(repo.get(PACKAGE_ID, "1.0.0").unwrap().is_none());
    assert!(repo.get(PACKAGE_ID, "1.1.0").unwrap().is_some());

    // Uninstalling again reports NotFound instead of deleting anything.
    assert_eq!(
        uninstall_version(
            env.managed_root.path(),
            env.db().connection(),
            PACKAGE_ID,
            "1.0.0"
        )
        .unwrap(),
        UninstallOutcome::NotFound
    );
}

#[test]
fn uninstalling_the_active_version_clears_the_active_mapping() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let manager = env.manager();
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), None),
            PACKAGE_ID,
            "2.0.0",
        )
        .unwrap();
    manager.wait(&job_id).unwrap();

    {
        let db = env.db();
        let repo = InstalledRepo::new(db.connection());
        repo.set_active(PACKAGE_ID, "2.0.0").unwrap();
    }
    let outcome = uninstall_version(
        env.managed_root.path(),
        env.db().connection(),
        PACKAGE_ID,
        "2.0.0",
    )
    .unwrap();
    assert_eq!(
        outcome,
        UninstallOutcome::Removed {
            files_removed: 4,
            cleared_active: true
        }
    );
    {
        let db = env.db();
        let repo = InstalledRepo::new(db.connection());
        assert_eq!(repo.active_version(PACKAGE_ID).unwrap(), None);
        assert!(repo.get(PACKAGE_ID, "2.0.0").unwrap().is_none());
    }
}

#[test]
fn reinstalling_an_installed_version_is_idempotent() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let manager = env.manager();
    let first = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), None),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    manager.wait(&first).unwrap();
    let second = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), None),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    assert_eq!(manager.wait(&second).unwrap(), RunOutcome::Completed);

    // The idempotent walk reports itself and touches nothing.
    let messages: Vec<String> = manager
        .watch_job_events(&second, 0)
        .unwrap()
        .into_iter()
        .map(|event| event.message)
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Already installed")),
        "messages: {messages:?}"
    );
    let idempotent_events = events(&manager, &second);
    assert!(
        idempotent_events
            .iter()
            .all(|(_, progress, _)| progress.is_none()),
        "no download progress may run on an idempotent re-install"
    );
    assert_eq!(
        InstalledRepo::new(env.db().connection())
            .list_for_package(PACKAGE_ID)
            .unwrap()
            .len(),
        1
    );
}

// ---------------------------------------------------------------------------
// Registry source (the real catalog-data path)
// ---------------------------------------------------------------------------

fn catalog_manifest(url: String) -> PackageManifest {
    let windows_artifact = Artifact {
        os: ArtifactOs::Windows,
        architecture: Architecture::X64,
        installer: InstallerKind::Archive,
        url: url.clone(),
        sha256: sha256_hex(&happy_zip()),
        size_bytes: happy_zip().len() as u64,
        signature_url: None,
        publisher_identity: String::new(),
        archive_root: String::new(),
        executable_relative_path: "app/bin/tool.exe".to_owned(),
    };
    let macos_artifact = Artifact {
        os: ArtifactOs::Macos,
        architecture: Architecture::Arm64,
        installer: InstallerKind::Archive,
        url: url.replace("tool.zip", "tool.tar.gz"),
        sha256: sha256_hex(&happy_zip()),
        size_bytes: happy_zip().len() as u64,
        signature_url: None,
        publisher_identity: String::new(),
        archive_root: "tool".to_owned(),
        executable_relative_path: String::new(),
    };
    PackageManifest {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        id: PACKAGE_ID.to_owned(),
        name: "Test Tool".to_owned(),
        description: "A tool only for the catalog-resolution test".to_owned(),
        publisher: "Test Publisher".to_owned(),
        homepage: "https://example.invalid/tool".to_owned(),
        category: Category::Utility,
        supported_platforms: vec![Platform::WindowsX64, Platform::MacosArm64],
        default_version_policy: VersionPolicyName::Stable,
        versions: vec![PackageVersion {
            version: "1.0.0".to_owned(),
            channel: Channel::Stable,
            release_date: Some("2026-01-01".to_owned()),
            release_notes_url: None,
            dependencies: Vec::new(),
            artifacts: vec![windows_artifact, macos_artifact],
        }],
        license_expression: "MIT".to_owned(),
        maintainer: "maintainers@example.invalid".to_owned(),
        updated_at: "2026-01-01T00:00:00Z".to_owned(),
    }
}

#[test]
fn registry_source_resolves_the_current_platform_artifact_from_the_catalog_cache() {
    let env = TestEnv::new();
    seed_catalog(&env);
    let db = env.db();
    let url = "https://releases.example.invalid/devkit.test.tool/tool.zip".to_owned();
    PackageCache::new(db.connection())
        .upsert(&catalog_manifest(url))
        .unwrap();

    let resolved = RegistrySource::new(Database::open(&env.db_path).unwrap())
        .resolve(PACKAGE_ID, "1.0.0")
        .unwrap();
    assert_eq!(resolved.os, ArtifactOs::Windows);
    assert_eq!(resolved.architecture, Architecture::X64);
    assert!(resolved
        .url
        .starts_with("https://releases.example.invalid/"));

    // Unknown version/package failures carry the stable codes.
    let source = RegistrySource::new(Database::open(&env.db_path).unwrap());
    assert_eq!(
        source.resolve(PACKAGE_ID, "9.9.9").unwrap_err().code,
        ErrorCode::VersionNotFound
    );
    assert_eq!(
        source
            .resolve("devkit.missing.thing", "1.0.0")
            .unwrap_err()
            .code,
        ErrorCode::PackageNotFound
    );
}

// ---------------------------------------------------------------------------
// Orphan recovery: crashed jobs leave no staging data behind
// ---------------------------------------------------------------------------

#[test]
fn recovery_removes_abandoned_staging_data_and_installing_rows() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    {
        let db = Database::open(&env.db_path).unwrap();
        let store = JobStore::new(db.connection());
        // A job that "died" mid-download: staging dir + artifact file + a
        // half-written receipt row.
        let job_id = "00000000-0000-0000-0000-000000000001";
        store
            .create_job(
                job_id,
                JobKind::Install,
                Some(PACKAGE_ID),
                Some("1.0.0"),
                None,
            )
            .unwrap();
        let staging = env
            .managed_root
            .path()
            .join("packages")
            .join(PACKAGE_ID)
            .join(format!("1.0.0.staging-{job_id}"));
        std::fs::create_dir_all(staging.join("app")).unwrap();
        std::fs::write(staging.join("app/loose.bin"), b"partial").unwrap();
        std::fs::write(
            env.managed_root
                .path()
                .join("packages")
                .join(PACKAGE_ID)
                .join(format!("1.0.0.staging-{job_id}.artifact.bin")),
            &bytes,
        )
        .unwrap();
        let repo = InstalledRepo::new(db.connection());
        repo.register(&InstalledRecord {
            package_id: PACKAGE_ID.to_owned(),
            version: "1.0.0".to_owned(),
            install_path: String::new(),
            status: InstalledStatus::Installing,
            receipt_json: "{}".to_owned(),
            installed_at: "2026-11-15T00:00:00Z".to_owned(),
        })
        .unwrap();
    }

    // Reopening the manager runs recovery.
    let _manager = env.manager();

    let db = Database::open(&env.db_path).unwrap();
    let snapshot = JobStore::new(db.connection())
        .get_job("00000000-0000-0000-0000-000000000001")
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.state, "ROLLED_BACK");
    let repo = InstalledRepo::new(db.connection());
    assert!(
        repo.get(PACKAGE_ID, "1.0.0").unwrap().is_none(),
        "installing row cleaned"
    );
    let package_root = env.managed_root.path().join("packages").join(PACKAGE_ID);
    assert!(
        std::fs::read_dir(&package_root).unwrap().next().is_none(),
        "staging cleaned"
    );
}

// ---------------------------------------------------------------------------
// validate_installation direct checks
// ---------------------------------------------------------------------------

#[test]
fn validate_installation_rejects_tampered_files_and_missing_entry_points() {
    let bytes = happy_zip();
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let manager = env.manager();
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), None),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    manager.wait(&job_id).unwrap();

    let record = repo_row(&env, "1.0.0").unwrap();
    let receipt: InstallReceipt = serde_json::from_str(&record.receipt_json).unwrap();

    // Tamper: append a byte to one file → size check fails.
    let target = install_dir(env.managed_root.path(), "1.0.0").join("app/README.md");
    let original = std::fs::read(&target).unwrap();
    let mut content = original.clone();
    content.push(b'!');
    std::fs::write(&target, &content).unwrap();
    assert!(validate_installation(env.managed_root.path(), &receipt).is_err());

    // Tamper: remove the declared entry point → entry-point check fails.
    // (tool.exe is also a receipt file, so use a receipt copy without it —
    // otherwise the missing-file check masks the entry-point check.)
    std::fs::write(&target, &original).unwrap();
    let executable = install_dir(env.managed_root.path(), "1.0.0").join("app/bin/tool.exe");
    std::fs::remove_file(&executable).unwrap();
    let mut entry_point_only = receipt.clone();
    entry_point_only
        .files
        .retain(|file| slashed(&file.relative_path) != "app/bin/tool.exe");
    let error = validate_installation(env.managed_root.path(), &entry_point_only).unwrap_err();
    assert!(error.contains("entry point"), "unexpected reason: {error}");

    // A receipt pointing outside the managed root is rejected outright.
    let mut escaped = receipt.clone();
    escaped.install_path = "../../somewhere".to_owned();
    assert!(validate_installation(env.managed_root.path(), &escaped).is_err());
}

// Keep ReceiptFile in the import set used by the receipt assertions.
#[allow(dead_code)]
fn assert_receipt_file_shape(_file: &ReceiptFile) {}
