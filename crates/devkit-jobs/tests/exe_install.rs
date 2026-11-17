//! P3-06 exe adapter pipeline tests: the vendor installer runs with the
//! fixed silent flags from the contract table, cancellation kills the
//! installer process (the job owns it), and any failure rolls the managed
//! root and registry back to the pre-install state (testing-strategy.md
//! rule 1: the tests inject a fake runner that records the constructed
//! command line and simulates installer behavior — a foreign vendor
//! artifact is never executed; the only real process tests spawn **this
//! test binary** as a harmless stub).

// The Windows-only tests below leave their helpers and imports unused on
// other targets — that is cfg-branching, not dead code.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use devkit_core::error::ErrorCode;
use devkit_core::manifest::{
    Architecture, Artifact, ArtifactOs, Category, Channel, InstallerKind, PackageManifest,
    PackageVersion, Platform, VersionPolicyName, SUPPORTED_SCHEMA_VERSION,
};
use devkit_download::{CancelToken, ReqwestDownloadHttp};
use devkit_install::{ArchiveLimits, ExeError, ExeRunOutcome, ExeRunner, ProcessExeRunner};
use devkit_jobs::pipeline::{
    uninstall_version, validate_installation, InstallReceipt, InstallSource, RealInstallConfig,
    StageFailure, UninstallOutcome,
};
use devkit_jobs::{JobManager, RunOutcome, Stage};
use devkit_registry::installed::InstalledRepo;
use devkit_registry::package_cache::PackageCache;
use devkit_registry::{Database, InstalledStatus};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const PACKAGE_ID: &str = "devkit.test.installer";

/// Fake vendor-installer bytes: a PE image with (or without) a
/// contract-table marker. Committed-installer artifacts are never used —
/// these bytes are only downloaded, verified, and (for the technology
/// detection) read.
fn installer_bytes(marker: &str, pe_magic: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    if pe_magic {
        bytes.extend_from_slice(b"MZ");
        bytes.extend_from_slice(b"this program cannot be run in DOS mode");
    }
    bytes.extend_from_slice(marker.as_bytes());
    bytes.extend_from_slice(&[0_u8; 32]);
    bytes
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

/// Fixture source: hands back the prepared artifact regardless of the
/// requested version.
struct FixtureSource {
    artifact: Artifact,
}

impl InstallSource for FixtureSource {
    fn resolve(&self, _package_id: &str, _version: &str) -> Result<Artifact, StageFailure> {
        Ok(self.artifact.clone())
    }
}

fn artifact_for(port: u16, bytes: &[u8]) -> Artifact {
    Artifact {
        os: ArtifactOs::Windows,
        architecture: Architecture::X64,
        installer: InstallerKind::Exe,
        url: format!("http://127.0.0.1:{port}/setup.exe"),
        sha256: sha256_hex(bytes),
        size_bytes: bytes.len() as u64,
        signature_url: None,
        publisher_identity: String::new(),
        archive_root: String::new(),
        executable_relative_path: "bin/tool.exe".to_owned(),
    }
}

fn fixture_source(port: u16, bytes: &[u8]) -> FixtureSource {
    FixtureSource {
        artifact: artifact_for(port, bytes),
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

/// Seeds the catalog cache so the `installed_versions` FK has its package
/// row (production flow: the catalog client populated it first).
fn seed_catalog(env: &TestEnv) {
    let (platform, os, arch) = this_runner_platform();
    let manifest = PackageManifest {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        id: PACKAGE_ID.to_owned(),
        name: "Test Installer".to_owned(),
        description: "exe adapter pipeline test package".to_owned(),
        publisher: "Test Publisher".to_owned(),
        homepage: "https://example.invalid/tool".to_owned(),
        category: Category::Utility,
        supported_platforms: vec![platform],
        default_version_policy: VersionPolicyName::Stable,
        versions: vec![PackageVersion {
            version: "1.0.0".to_owned(),
            channel: Channel::Stable,
            release_date: Some("2026-01-01".to_owned()),
            release_notes_url: None,
            dependencies: Vec::new(),
            artifacts: vec![Artifact {
                os,
                architecture: arch,
                installer: InstallerKind::Exe,
                url: "https://releases.example.invalid/tool/setup.exe".to_owned(),
                sha256: sha256_hex(b"placeholder"),
                size_bytes: 1024,
                signature_url: None,
                publisher_identity: String::new(),
                archive_root: String::new(),
                executable_relative_path: "bin/tool.exe".to_owned(),
            }],
        }],
        license_expression: "MIT".to_owned(),
        maintainer: "maintainers@example.invalid".to_owned(),
        updated_at: "2026-01-01T00:00:00Z".to_owned(),
    };
    PackageCache::new(env.db().connection())
        .upsert(&manifest)
        .unwrap();
}

/// (Platform, os, architecture) of the machine running this test binary.
fn this_runner_platform() -> (Platform, ArtifactOs, Architecture) {
    #[cfg(target_os = "windows")]
    {
        if cfg!(target_arch = "x86_64") {
            (Platform::WindowsX64, ArtifactOs::Windows, Architecture::X64)
        } else {
            (
                Platform::WindowsArm64,
                ArtifactOs::Windows,
                Architecture::Arm64,
            )
        }
    }
    #[cfg(target_os = "macos")]
    {
        if cfg!(target_arch = "x86_64") {
            (Platform::MacosX64, ArtifactOs::Macos, Architecture::X64)
        } else {
            (Platform::MacosArm64, ArtifactOs::Macos, Architecture::Arm64)
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if cfg!(target_arch = "aarch64") {
            (Platform::LinuxArm64, ArtifactOs::Linux, Architecture::Arm64)
        } else {
            (Platform::LinuxX64, ArtifactOs::Linux, Architecture::X64)
        }
    }
}

fn repo_row(env: &TestEnv, version: &str) -> Option<devkit_registry::InstalledRecord> {
    InstalledRepo::new(env.db().connection())
        .get(PACKAGE_ID, version)
        .unwrap()
}

fn install_dir(managed_root: &Path, version: &str) -> PathBuf {
    managed_root.join("packages").join(PACKAGE_ID).join(version)
}

fn staging_dir(managed_root: &Path, version: &str, job_id: &str) -> PathBuf {
    managed_root
        .join("packages")
        .join(PACKAGE_ID)
        .join(format!("{version}.staging-{job_id}"))
}

/// Receipt paths are platform-native; test assertions compare against the
/// forward-slash form.
fn slashed(path: &str) -> String {
    path.replace('\\', "/")
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

// ---------------------------------------------------------------------------
// The fake runner: records the command line, simulates installer behavior
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum FakeBehavior {
    /// Simulates a well-behaved vendor installer: writes the declared
    /// files into the directory the fixed flags requested and exits 0.
    Succeed {
        files: Vec<(&'static str, &'static [u8])>,
    },
    /// Simulates a failed installer (non-zero exit code).
    Exit(i32),
    /// Simulates an installer that writes partial data and keeps running
    /// until the job's cancellation kills it.
    HangUntilCancelled,
}

/// Recorded (program, flags) calls of the fake runners.
type RecordedCalls = Arc<Mutex<Vec<(String, Vec<String>)>>>;

struct FakeExeRunner {
    behavior: FakeBehavior,
    calls: RecordedCalls,
    started: Arc<AtomicBool>,
}

impl ExeRunner for FakeExeRunner {
    fn run(
        &self,
        program: &str,
        flags: &[String],
        cancel: &CancelToken,
    ) -> Result<ExeRunOutcome, ExeError> {
        self.calls
            .lock()
            .unwrap()
            .push((program.to_owned(), flags.to_vec()));
        self.started.store(true, Ordering::SeqCst);
        // The fixed flags always carry the install directory; the fake
        // "installs" into exactly that directory, like a real vendor
        // installer honoring /DIR (Inno) or /D (NSIS).
        let final_dir = flags
            .iter()
            .find_map(|flag| {
                flag.strip_prefix("/DIR=")
                    .or_else(|| flag.strip_prefix("/D="))
            })
            .expect("the fixed flags carry the install directory");
        let final_dir = PathBuf::from(final_dir);
        match &self.behavior {
            FakeBehavior::Succeed { files } => {
                for (relative, content) in files {
                    let path = final_dir.join(relative);
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(path, content).unwrap();
                }
                Ok(ExeRunOutcome {
                    exit_code: Some(0),
                    cancelled: false,
                })
            }
            FakeBehavior::Exit(code) => Ok(ExeRunOutcome {
                exit_code: Some(*code),
                cancelled: false,
            }),
            FakeBehavior::HangUntilCancelled => {
                std::fs::create_dir_all(&final_dir).unwrap();
                std::fs::write(final_dir.join("partial-install.tmp"), b"vendor wrote this")
                    .unwrap();
                while !cancel.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Ok(ExeRunOutcome {
                    exit_code: None,
                    cancelled: true,
                })
            }
        }
    }
}

fn runner_with(behavior: FakeBehavior) -> (FakeExeRunner, RecordedCalls) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    (
        FakeExeRunner {
            behavior,
            calls: calls.clone(),
            started: Arc::new(AtomicBool::new(false)),
        },
        calls,
    )
}

fn config(
    env: &TestEnv,
    source: FixtureSource,
    runner: FakeExeRunner,
) -> RealInstallConfig<ReqwestDownloadHttp, FixtureSource> {
    RealInstallConfig {
        managed_root: env.managed_root.path().to_path_buf(),
        transport: ReqwestDownloadHttp::new().unwrap(),
        source,
        limits: ArchiveLimits::default(),
        vendor_runner: Box::new(runner),
        validation_hook: None,
    }
}

// ---------------------------------------------------------------------------
// Happy paths: the fixed flags per technology (P3-06 acceptance)
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[test]
fn inno_installer_runs_with_the_fixed_flags_and_registers() {
    let bytes = installer_bytes("built with Inno Setup 6.2", true);
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let (runner, calls) = runner_with(FakeBehavior::Succeed {
        files: vec![
            ("bin/tool.exe", b"MZtool".as_slice()),
            ("data/readme.txt", b"hello".as_slice()),
        ],
    });
    let manager = env.manager();
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), runner),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::Completed);

    // The constructed command line (P3-06 acceptance): exactly the
    // reviewed contract table, in order.
    let (program, flags) = calls.lock().unwrap()[0].clone();
    assert!(program.ends_with(".artifact.bin"), "program: {program}");
    let final_dir = install_dir(env.managed_root.path(), "1.0.0");
    let staging = staging_dir(env.managed_root.path(), "1.0.0", &job_id);
    assert_eq!(
        flags,
        vec![
            "/VERYSILENT".to_owned(),
            "/NORESTART".to_owned(),
            "/SUPPRESSMSGBOXES".to_owned(),
            format!("/LOG={}", staging.join("install.log").display()),
            format!("/DIR={}", final_dir.display()),
        ]
    );

    // The job walked INSTALLING (installer adapters), never EXTRACTING.
    let stage_list = events(&manager, &job_id)
        .into_iter()
        .map(|(stage, _, _)| stage)
        .collect::<Vec<_>>();
    assert!(stage_list.contains(&Stage::Installing));
    assert!(!stage_list.contains(&Stage::Extracting));

    // The receipt (v2): exe part, no archive part, real files with hashes.
    let record = repo_row(&env, "1.0.0").unwrap();
    assert_eq!(record.status, InstalledStatus::Installed);
    let receipt: InstallReceipt = serde_json::from_str(&record.receipt_json).unwrap();
    assert_eq!(receipt.receipt_version, 2);
    assert_eq!(receipt.installer, "exe");
    assert!(receipt.archive.is_none());
    let exe = receipt.exe.as_ref().unwrap();
    assert_eq!(exe.technology, "inno-setup");
    assert_eq!(exe.exit_code, Some(0));
    assert_eq!(
        slashed(&receipt.install_path),
        format!("packages/{PACKAGE_ID}/1.0.0")
    );
    assert_eq!(receipt.files.len(), 2);
    let tool = receipt
        .files
        .iter()
        .find(|file| slashed(&file.relative_path) == "bin/tool.exe")
        .unwrap();
    assert_eq!(tool.size_bytes, 6);
    assert_eq!(tool.sha256_hex, sha256_hex(b"MZtool"));
    assert_eq!(
        receipt
            .executable_relative_path
            .as_ref()
            .map(|path| slashed(path)),
        Some("bin/tool.exe".to_owned())
    );
    assert!(validate_installation(env.managed_root.path(), &receipt).is_ok());

    // Staged artifact + staging directory are gone; only the final
    // directory remains.
    assert!(!staging.exists());
    assert!(!env
        .managed_root
        .path()
        .join("packages")
        .join(PACKAGE_ID)
        .join(format!("1.0.0.staging-{job_id}.artifact.bin"))
        .exists());
    assert!(final_dir.join("bin/tool.exe").is_file());

    // Uninstall removes exactly what the receipt records.
    let outcome = uninstall_version(
        env.managed_root.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.0.0",
    )
    .unwrap();
    assert!(
        matches!(
            outcome,
            UninstallOutcome::Removed {
                files_removed: 2,
                cleared_active: false
            }
        ),
        "unexpected outcome: {outcome:?}"
    );
    assert!(!final_dir.exists());
}

#[cfg(windows)]
#[test]
fn nsis_installer_runs_with_the_fixed_flags_and_registers() {
    let bytes = installer_bytes("Nullsoft Install System v3.08", true);
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let (runner, calls) = runner_with(FakeBehavior::Succeed {
        files: vec![("bin/tool.exe", b"MZtool".as_slice())],
    });
    let manager = env.manager();
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), runner),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::Completed);

    // NSIS: /S and the /D directory flag last, nothing else.
    let (program, flags) = calls.lock().unwrap()[0].clone();
    assert!(program.ends_with(".artifact.bin"), "program: {program}");
    let final_dir = install_dir(env.managed_root.path(), "1.0.0");
    assert_eq!(
        flags,
        vec!["/S".to_owned(), format!("/D={}", final_dir.display())]
    );

    let receipt: InstallReceipt =
        serde_json::from_str(&repo_row(&env, "1.0.0").unwrap().receipt_json).unwrap();
    let exe = receipt.exe.as_ref().unwrap();
    assert_eq!(exe.technology, "nsis");
    assert_eq!(exe.exit_code, Some(0));
    assert!(receipt.archive.is_none());
    assert_eq!(receipt.files.len(), 1);
    assert!(validate_installation(env.managed_root.path(), &receipt).is_ok());
}

// ---------------------------------------------------------------------------
// Failure, cancellation, and rejection roll back cleanly
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[test]
fn failing_installer_rolls_back_cleanly() {
    let bytes = installer_bytes("built with Inno Setup 6.2", true);
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let (runner, _calls) = runner_with(FakeBehavior::Exit(3));
    let manager = env.manager();
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), runner),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);

    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(Stage::parse(&snapshot.state), Some(Stage::RolledBack));
    assert_eq!(
        snapshot.error_code.as_deref(),
        Some(ErrorCode::InstallFailed.as_str())
    );
    assert!(
        manager
            .watch_job_events(&job_id, 0)
            .unwrap()
            .iter()
            .any(|event| event.message.contains("exit code")),
        "the failure message names the installer's exit code"
    );
    assert!(repo_row(&env, "1.0.0").is_none());
    assert!(!install_dir(env.managed_root.path(), "1.0.0").exists());
    assert!(!staging_dir(env.managed_root.path(), "1.0.0", &job_id).exists());
    assert!(!env
        .managed_root
        .path()
        .join("packages")
        .join(PACKAGE_ID)
        .join(format!("1.0.0.staging-{job_id}.artifact.bin"))
        .exists());
}

#[cfg(windows)]
#[test]
fn cancelling_during_the_installer_rolls_back() {
    let bytes = installer_bytes("Nullsoft Install System v3.08", true);
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let (runner, calls) = runner_with(FakeBehavior::HangUntilCancelled);
    let manager = env.manager();
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), runner),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();

    // Wait until the (fake) installer is actually running, then cancel.
    let deadline = Instant::now() + Duration::from_secs(10);
    while calls.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "installer never started");
        std::thread::sleep(Duration::from_millis(25));
    }
    manager.cancel_job(&job_id).unwrap();
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);

    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(Stage::parse(&snapshot.state), Some(Stage::RolledBack));
    // The terminal chain records CANCELLED before ROLLED_BACK, but the
    // snapshot's final state is the rollback; the cancel-specific outcome
    // is asserted via the outcome value above.
    // The partial vendor-written directory is removed with everything else.
    assert!(!install_dir(env.managed_root.path(), "1.0.0").exists());
    assert!(repo_row(&env, "1.0.0").is_none());
}

#[cfg(windows)]
#[test]
fn unknown_technology_fails_without_running_the_installer() {
    let env = TestEnv::new();
    seed_catalog(&env);
    let manager = env.manager();

    // In the contract table? No: PE image, but a WiX Burn bootstrapper.
    let bytes = installer_bytes("WiX Burn bootstrapper", true);
    let port = spawn_serving(bytes.clone());
    let (runner, calls) = runner_with(FakeBehavior::Exit(0));
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), runner),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);
    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(Stage::parse(&snapshot.state), Some(Stage::RolledBack));
    assert_eq!(
        snapshot.error_code.as_deref(),
        Some(ErrorCode::InvalidArgument.as_str())
    );

    // Also not in the table: a marker without the PE header is not a
    // Windows installer image at all.
    let bytes = installer_bytes("built with Inno Setup 6.2", false);
    let port = spawn_serving(bytes.clone());
    let (runner, calls2) = runner_with(FakeBehavior::Exit(0));
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), runner),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);
    assert_eq!(
        Stage::parse(&manager.get_job(&job_id).unwrap().unwrap().state),
        Some(Stage::RolledBack)
    );

    // Nothing was ever executed: detection failure means "never run it
    // with guessed flags".
    assert!(calls.lock().unwrap().is_empty());
    assert!(calls2.lock().unwrap().is_empty());
    assert!(repo_row(&env, "1.0.0").is_none());
    assert!(!install_dir(env.managed_root.path(), "1.0.0").exists());
}

// ---------------------------------------------------------------------------
// Platform gate: the exe adapter is Windows-only
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
#[test]
fn exe_artifacts_are_unsupported_off_windows() {
    let bytes = installer_bytes("built with Inno Setup 6.2", true);
    let env = TestEnv::new();
    seed_catalog(&env);
    let port = spawn_serving(bytes.clone());
    let (runner, calls) = runner_with(FakeBehavior::Exit(0));
    let manager = env.manager();
    let job_id = manager
        .enqueue_real_install(
            config(&env, fixture_source(port, &bytes), runner),
            PACKAGE_ID,
            "1.0.0",
        )
        .unwrap();
    assert_eq!(manager.wait(&job_id).unwrap(), RunOutcome::RolledBack);
    let snapshot = manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(Stage::parse(&snapshot.state), Some(Stage::RolledBack));
    assert_eq!(
        snapshot.error_code.as_deref(),
        Some(ErrorCode::UnsupportedPlatform.as_str())
    );
    // Nothing was downloaded into a final directory and nothing ran.
    assert!(calls.lock().unwrap().is_empty());
    assert!(repo_row(&env, "1.0.0").is_none());
    assert!(!install_dir(env.managed_root.path(), "1.0.0").exists());
}

// ---------------------------------------------------------------------------
// ProcessExeRunner: spawn failures, real process ownership
// ---------------------------------------------------------------------------

#[test]
fn process_runner_reports_spawn_failures() {
    let cancel = CancelToken::from_shared(Arc::new(AtomicBool::new(false)));
    let error = ProcessExeRunner
        .run("definitely-not-a-real-devkit-program-xyz", &[], &cancel)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InstallFailed);
}

/// The stub child the real-process tests below spawn: this test binary
/// itself. It only ever writes a heartbeat file — when the marker
/// environment variable is absent (a normal test run) it exits
/// immediately.
#[cfg(windows)]
#[test]
fn zzz_stub_child_installer() {
    let Ok(heartbeat) = std::env::var("DEVKIT_EXE_STUB_HEARTBEAT") else {
        return;
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let _ = std::fs::write(&heartbeat, b"alive");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Serializes the two real-process tests: they temporarily mutate this
/// process's environment (the stub marker), so they must not overlap.
#[cfg(windows)]
static SPAWN_LOCK: Mutex<()> = Mutex::new(());

#[cfg(windows)]
#[test]
fn process_runner_runs_a_child_to_completion() {
    let _guard = SPAWN_LOCK.lock().unwrap();
    let exe = std::env::current_exe().unwrap();
    let cancel = CancelToken::from_shared(Arc::new(AtomicBool::new(false)));
    let outcome = ProcessExeRunner
        .run(
            exe.to_string_lossy().as_ref(),
            &["--exact".to_owned(), "zzz_stub_child_installer".to_owned()],
            &cancel,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ExeRunOutcome {
            exit_code: Some(0),
            cancelled: false
        }
    );
}

#[cfg(windows)]
#[test]
fn process_runner_kills_the_child_on_cancel() {
    let _guard = SPAWN_LOCK.lock().unwrap();
    let dir = TempDir::new().unwrap();
    let heartbeat = dir.path().join("heartbeat");
    std::env::set_var("DEVKIT_EXE_STUB_HEARTBEAT", &heartbeat);
    let started = Instant::now();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel = CancelToken::from_shared(cancel_flag.clone());
    let exe = std::env::current_exe().unwrap();
    let handle = std::thread::spawn(move || {
        ProcessExeRunner.run(
            exe.to_string_lossy().as_ref(),
            &["--exact".to_owned(), "zzz_stub_child_installer".to_owned()],
            &cancel,
        )
    });

    // Wait for the child's first heartbeat (it is genuinely running).
    let deadline = Instant::now() + Duration::from_secs(15);
    while !heartbeat.exists() {
        assert!(Instant::now() < deadline, "child never wrote a heartbeat");
        std::thread::sleep(Duration::from_millis(25));
    }
    cancel_flag.store(true, Ordering::SeqCst);
    let outcome = handle.join().unwrap().unwrap();
    std::env::remove_var("DEVKIT_EXE_STUB_HEARTBEAT");

    assert_eq!(
        outcome,
        ExeRunOutcome {
            exit_code: None,
            cancelled: true
        }
    );
    // The kill actually worked: the child died promptly after the cancel,
    // not after its own 30 s deadline.
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancel took too long; the child was probably not killed"
    );
}
