//! P3-04 activation tests: planning, consent gating, shims, user-PATH
//! integration, and rollback (version-management.md §3, security-model.md
//! §7, DESIGN.md journey C).
//!
//! Testing rules honored (testing-strategy.md):
//! - The real Windows registry is NEVER touched: `FakePathEnvironment`
//!   records PATH reads/appends in memory and can be told to fail.
//! - No network: activation only reads already-registered receipt rows,
//!   so tests fabricate rows + files directly (no HTTP fixtures needed).
//! - Every consent/refusal/rollback contract has an explicit test.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use devkit_core::error::ErrorCode;
use devkit_core::manifest::{
    Architecture, Artifact, ArtifactOs, Category, Channel, InstallerKind, PackageManifest,
    PackageVersion, Platform, VersionPolicyName, SUPPORTED_SCHEMA_VERSION,
};
use devkit_jobs::environment::{
    activate, plan_activation, remove_shims_referencing, ActivationConsent, PathChange,
    PathEnvironment, SHIMS_DIR,
};
use devkit_jobs::pipeline::{uninstall_version, InstallReceipt, ReceiptFile, StageFailure};
use devkit_jobs::UninstallOutcome;
use devkit_registry::installed::InstalledRepo;
use devkit_registry::package_cache::PackageCache;
use devkit_registry::{Database, InstalledRecord, InstalledStatus};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const PACKAGE_ID: &str = "devkit.test.tool";
const SHIM_RELATIVE: &str = if cfg!(windows) {
    r"shims\tool.cmd"
} else {
    "shims/tool.cmd"
};

/// Platform-native separator form of a mixed-separator path string, used
/// to compare against pipeline-produced paths (receipts store
/// platform-native separators: `\` on Windows, `/` elsewhere).
fn native(path: &str) -> String {
    if cfg!(windows) {
        path.replace('/', "\\")
    } else {
        path.replace('\\', "/")
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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

    fn db(&self) -> Database {
        Database::open(&self.db_path).unwrap()
    }

    fn path(&self) -> &Path {
        self.managed_root.path()
    }
}

/// Recorder stand-in for the user-PATH environment: tests must never touch
/// the real registry (security-model.md §7). `fail_append` injects the
/// "PATH write failed after activation started" rollback scenario.
struct FakePathEnvironment {
    entries: Mutex<Vec<String>>,
    appended: Mutex<Vec<String>>,
    fail_append: AtomicBool,
}

impl FakePathEnvironment {
    fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            appended: Mutex::new(Vec::new()),
            fail_append: AtomicBool::new(false),
        }
    }

    fn with_entries(entries: Vec<String>) -> Self {
        Self {
            entries: Mutex::new(entries),
            appended: Mutex::new(Vec::new()),
            fail_append: AtomicBool::new(false),
        }
    }

    fn appended(&self) -> Vec<String> {
        self.appended.lock().unwrap().clone()
    }
}

impl PathEnvironment for FakePathEnvironment {
    fn user_path_entries(&self) -> Result<Vec<String>, StageFailure> {
        Ok(self.entries.lock().unwrap().clone())
    }

    fn append_user_path_entry(&self, entry: &str) -> Result<PathChange, StageFailure> {
        if self.fail_append.load(Ordering::SeqCst) {
            return Err(StageFailure::new(
                ErrorCode::InstallFailed,
                "injected user-PATH write failure",
            ));
        }
        self.appended.lock().unwrap().push(entry.to_owned());
        self.entries.lock().unwrap().push(entry.to_owned());
        Ok(PathChange::Append(entry.to_owned()))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Registers the catalog row the `installed_versions.package_id` FK points
/// at (production order: catalog client populates the cache first).
fn seed_catalog(env: &TestEnv, package_id: &str) {
    let manifest = PackageManifest {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        id: package_id.to_owned(),
        name: "Test Tool".to_owned(),
        description: "Fixture package for activation tests".to_owned(),
        publisher: "Test Publisher".to_owned(),
        homepage: "https://example.invalid/tool".to_owned(),
        category: Category::Utility,
        supported_platforms: vec![Platform::WindowsX64],
        default_version_policy: VersionPolicyName::Stable,
        versions: vec![PackageVersion {
            version: "1.0.0".to_owned(),
            channel: Channel::Stable,
            release_date: Some("2026-01-01".to_owned()),
            release_notes_url: None,
            dependencies: Vec::new(),
            artifacts: vec![Artifact {
                os: ArtifactOs::Windows,
                architecture: Architecture::X64,
                installer: InstallerKind::Archive,
                url: "https://releases.example.invalid/tool.zip".to_owned(),
                sha256: sha256_hex(b"unused"),
                size_bytes: 0,
                signature_url: None,
                publisher_identity: String::new(),
                archive_root: String::new(),
                executable_relative_path: "app/bin/tool.exe".to_owned(),
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

/// Fabricates an `installed` receipt row + the referenced files on disk —
/// exactly what a completed P3-03 install leaves behind, without running
/// one. `executable` mirrors the artifact declaration (`None` = library
/// package).
fn seed_installed(env: &TestEnv, package_id: &str, version: &str, executable: Option<&str>) {
    let relative_install_path = Path::new("packages").join(package_id).join(version);
    let install_root = env.path().join(&relative_install_path);

    // Two payload files, hashed from real bytes.
    let mut files = Vec::new();
    let versioned_tool = format!("tool {version} bytes");
    let payload: [(&str, &str); 2] = [
        ("app/bin/readme.txt", "readme\n"),
        ("app/bin/tool.exe", versioned_tool.as_str()),
    ];
    for (relative, content) in payload {
        let file_path = install_root.join(relative);
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, content).unwrap();
        files.push(ReceiptFile {
            relative_path: relative.to_owned(),
            size_bytes: content.len() as u64,
            sha256_hex: sha256_hex(content.as_bytes()),
        });
    }
    if executable.is_some_and(|name| !payload.iter().any(|(relative, _)| *relative == name)) {
        let file_path = install_root.join(executable.unwrap());
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "entry point\n").unwrap();
        files.push(ReceiptFile {
            relative_path: executable.unwrap().to_owned(),
            size_bytes: "entry point\n".len() as u64,
            sha256_hex: sha256_hex(b"entry point\n"),
        });
    }

    let install_path = relative_install_path.to_string_lossy().into_owned();
    let receipt = InstallReceipt {
        receipt_version: 2,
        package_id: package_id.to_owned(),
        version: version.to_owned(),
        installer: "archive".to_owned(),
        archive: Some(devkit_jobs::pipeline::ArchiveReceipt {
            format: "zip".to_owned(),
            sha256_hex: sha256_hex(b"unused"),
            size_bytes: 0,
        }),
        exe: None,
        install_path: install_path.clone(),
        files,
        executable_relative_path: executable.map(str::to_owned),
        archive_root: None,
        job_id: format!("job-{package_id}-{version}"),
        installed_at: "2026-01-01T00:00:00Z".to_owned(),
    };
    let record = InstalledRecord {
        package_id: package_id.to_owned(),
        version: version.to_owned(),
        install_path,
        status: InstalledStatus::Installed,
        receipt_json: serde_json::to_string(&receipt).unwrap(),
        installed_at: "2026-01-01T00:00:00Z".to_owned(),
    };
    InstalledRepo::new(env.db().connection())
        .register(&record)
        .unwrap();
}

fn seed_two_versions(env: &TestEnv) {
    seed_catalog(env, PACKAGE_ID);
    seed_installed(env, PACKAGE_ID, "1.0.0", Some("app/bin/tool.exe"));
    seed_installed(env, PACKAGE_ID, "1.1.0", Some("app/bin/tool.exe"));
}

fn activate_1_0(env: &TestEnv, path_env: &FakePathEnvironment) {
    activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.0.0",
        ActivationConsent::Granted,
        path_env,
    )
    .unwrap();
}

fn active_version(env: &TestEnv) -> Option<String> {
    InstalledRepo::new(env.db().connection())
        .active_version(PACKAGE_ID)
        .unwrap()
}

fn shim_path(env: &TestEnv) -> PathBuf {
    env.path().join(SHIMS_DIR).join("tool.cmd")
}

fn shim_content(env: &TestEnv) -> String {
    std::fs::read_to_string(shim_path(env)).unwrap()
}

/// Absolute path in shell-usable form (canonicalize's `\\?\` drive-letter
/// prefix stripped — mirrors the shim content the module writes).
fn absolute(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap();
    let text = canonical.to_string_lossy().into_owned();
    text.strip_prefix(r"\\?\")
        .map(str::to_owned)
        .unwrap_or(text)
}

// ---------------------------------------------------------------------------
// Planning (the consent step's data)
// ---------------------------------------------------------------------------

#[test]
fn plan_shows_shims_path_change_and_previous_active() {
    let env = TestEnv::new();
    seed_two_versions(&env);
    let path_env = FakePathEnvironment::new();

    let plan = plan_activation(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.1.0",
        &path_env,
    )
    .unwrap();

    assert_eq!(plan.package_id, PACKAGE_ID);
    assert_eq!(plan.version, "1.1.0");
    assert_eq!(plan.previous_active_version, None);
    assert_eq!(plan.shims_dir_relative, SHIMS_DIR);
    assert_eq!(plan.shims.len(), 1);
    assert_eq!(plan.shims[0].shim_relative_path, SHIM_RELATIVE);
    let expected_target = Path::new("packages")
        .join(PACKAGE_ID)
        .join("1.1.0")
        .join("app/bin/tool.exe");
    assert_eq!(
        plan.shims[0].target_relative_path,
        native(&expected_target.to_string_lossy())
    );
    // Nothing on the fake PATH yet → the plan proposes one append.
    match &plan.path_change {
        PathChange::Append(entry) => {
            let expected = env.path().join(SHIMS_DIR);
            // Compare in a common separator form: the real entry is
            // absolute + platform-native.
            assert!(
                Path::new(entry)
                    .to_string_lossy()
                    .replace('\\', "/")
                    .eq_ignore_ascii_case(&expected.to_string_lossy().replace('\\', "/")),
                "planned PATH entry should be the shims dir, got {entry}"
            );
        }
        other => panic!("expected Append, got {other:?}"),
    }
    // Planning wrote nothing.
    assert!(!shim_path(&env).exists());
    assert!(active_version(&env).is_none());
    assert!(path_env.appended().is_empty());

    // With 1.0.0 active, the plan reports it as the version to revert to.
    activate_1_0(&env, &path_env);
    let plan = plan_activation(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.1.0",
        &path_env,
    )
    .unwrap();
    assert_eq!(plan.previous_active_version.as_deref(), Some("1.0.0"));
}

// ---------------------------------------------------------------------------
// Consent gating
// ---------------------------------------------------------------------------

#[test]
fn declined_consent_changes_nothing() {
    let env = TestEnv::new();
    seed_two_versions(&env);
    let path_env = FakePathEnvironment::new();

    // 1.0.0 active; the user declines activation of 1.1.0.
    activate_1_0(&env, &path_env);
    let shim_before = shim_content(&env);

    let error = activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.1.0",
        ActivationConsent::Declined,
        &path_env,
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::PermissionRequired);
    assert_eq!(active_version(&env).as_deref(), Some("1.0.0"));
    assert_eq!(shim_content(&env), shim_before, "shim untouched");
    assert_eq!(path_env.appended().len(), 1, "no further PATH write");
}

#[test]
fn granted_activation_switches_mapping_shim_and_path_once() {
    let env = TestEnv::new();
    seed_two_versions(&env);
    let path_env = FakePathEnvironment::new();

    // First activation: mapping + shim + one PATH append.
    let outcome = activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.0.0",
        ActivationConsent::Granted,
        &path_env,
    )
    .unwrap();
    assert_eq!(outcome.package_id, PACKAGE_ID);
    assert_eq!(outcome.version, "1.0.0");
    assert_eq!(outcome.previous_active_version, None);
    assert_eq!(outcome.shims_written, 1);
    assert_eq!(active_version(&env).as_deref(), Some("1.0.0"));
    assert_eq!(path_env.appended().len(), 1);
    let target_1_0 = env
        .path()
        .join("packages")
        .join(PACKAGE_ID)
        .join("1.0.0")
        .join("app/bin/tool.exe");
    assert_eq!(
        shim_content(&env),
        format!("@echo off\r\n\"{}\" %*\r\n", absolute(&target_1_0))
    );
    assert!(target_1_0.is_file(), "shim target must exist");

    // Second activation of another version: shim rewritten, PATH not
    // appended again (the shims dir is already on the fake PATH).
    let outcome = activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.1.0",
        ActivationConsent::Granted,
        &path_env,
    )
    .unwrap();
    assert_eq!(outcome.path_change, PathChange::AlreadyPresent);
    assert_eq!(
        path_env.appended().len(),
        1,
        "idempotent: no duplicate entry"
    );
    assert_eq!(active_version(&env).as_deref(), Some("1.1.0"));
    let target_1_1 = env
        .path()
        .join("packages")
        .join(PACKAGE_ID)
        .join("1.1.0")
        .join("app/bin/tool.exe");
    assert_eq!(
        shim_content(&env),
        format!("@echo off\r\n\"{}\" %*\r\n", absolute(&target_1_1)),
        "shim must point at the newly active version"
    );
}

// ---------------------------------------------------------------------------
// Rollback
// ---------------------------------------------------------------------------

#[test]
fn failed_path_write_rolls_back_mapping_and_shim() {
    let env = TestEnv::new();
    seed_two_versions(&env);
    let path_env = FakePathEnvironment::with_entries(Vec::new());
    path_env.fail_append.store(true, Ordering::SeqCst);

    activate_1_0(&env, &FakePathEnvironment::new());
    let shim_1_0 = shim_content(&env);

    // PATH write fails after the mapping + shim were already applied.
    let error = activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.1.0",
        ActivationConsent::Granted,
        &path_env,
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::InstallFailed);
    assert_eq!(
        active_version(&env).as_deref(),
        Some("1.0.0"),
        "mapping restored"
    );
    assert_eq!(
        shim_content(&env),
        shim_1_0,
        "shim restored to the previous version"
    );
    assert!(
        path_env.appended().is_empty(),
        "the failed write is not recorded"
    );
}

// ---------------------------------------------------------------------------
// Refusals by state
// ---------------------------------------------------------------------------

#[test]
fn activation_refuses_unknown_install_and_broken_rows() {
    let env = TestEnv::new();
    seed_two_versions(&env);
    let path_env = FakePathEnvironment::new();
    activate_1_0(&env, &path_env);

    // Unknown version.
    let error = activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "9.9.9",
        ActivationConsent::Granted,
        &path_env,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::VersionNotFound);

    // A still-installing row (never validated) may not be activated.
    let db = env.db();
    let repo = InstalledRepo::new(db.connection());
    repo.update_status(PACKAGE_ID, "1.1.0", InstalledStatus::Installing)
        .unwrap();
    let error = activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.1.0",
        ActivationConsent::Granted,
        &path_env,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::InstallFailed);

    // A broken row needs repair, not activation.
    repo.update_status(PACKAGE_ID, "1.1.0", InstalledStatus::Broken)
        .unwrap();
    let error = activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.1.0",
        ActivationConsent::Granted,
        &path_env,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::InstallFailed);
    assert_eq!(
        active_version(&env).as_deref(),
        Some("1.0.0"),
        "nothing changed"
    );
}

// ---------------------------------------------------------------------------
// Idempotence and shim lifecycle
// ---------------------------------------------------------------------------

#[test]
fn already_present_path_entry_is_not_appended_again() {
    let env = TestEnv::new();
    seed_two_versions(&env);
    let entry = env.path().join(SHIMS_DIR).to_string_lossy().into_owned();
    let path_env = FakePathEnvironment::with_entries(vec![entry]);

    let plan = plan_activation(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.0.0",
        &path_env,
    )
    .unwrap();
    assert_eq!(plan.path_change, PathChange::AlreadyPresent);

    let outcome = activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.0.0",
        ActivationConsent::Granted,
        &path_env,
    )
    .unwrap();
    assert_eq!(outcome.path_change, PathChange::AlreadyPresent);
    assert!(
        path_env.appended().is_empty(),
        "no write when already present"
    );
    assert_eq!(active_version(&env).as_deref(), Some("1.0.0"));
}

#[test]
fn uninstalling_the_active_version_removes_its_shim() {
    let env = TestEnv::new();
    seed_two_versions(&env);
    let path_env = FakePathEnvironment::new();

    // Uninstall the active version → its shim is dangling, remove it.
    activate_1_0(&env, &path_env);
    let outcome =
        uninstall_version(env.path(), env.db().connection(), PACKAGE_ID, "1.0.0").unwrap();
    assert!(matches!(
        outcome,
        UninstallOutcome::Removed {
            cleared_active: true,
            ..
        }
    ));
    assert!(!shim_path(&env).exists(), "dangling shim removed");
    assert_eq!(active_version(&env), None);

    // Uninstall an inactive version → the active shim is kept.
    activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.1.0",
        ActivationConsent::Granted,
        &path_env,
    )
    .unwrap();
    seed_installed(&env, PACKAGE_ID, "1.0.0", Some("app/bin/tool.exe"));
    let outcome =
        uninstall_version(env.path(), env.db().connection(), PACKAGE_ID, "1.0.0").unwrap();
    assert!(matches!(
        outcome,
        UninstallOutcome::Removed {
            cleared_active: false,
            ..
        }
    ));
    assert!(shim_path(&env).exists(), "active shim kept");
    assert_eq!(active_version(&env).as_deref(), Some("1.1.0"));
}

#[test]
fn library_package_activates_without_shim() {
    let env = TestEnv::new();
    seed_catalog(&env, PACKAGE_ID);
    seed_installed(&env, PACKAGE_ID, "1.0.0", None);
    let path_env = FakePathEnvironment::new();

    let plan = plan_activation(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.0.0",
        &path_env,
    )
    .unwrap();
    assert!(plan.shims.is_empty(), "no entry point → no shim");

    let outcome = activate(
        env.path(),
        env.db().connection(),
        PACKAGE_ID,
        "1.0.0",
        ActivationConsent::Granted,
        &path_env,
    )
    .unwrap();
    assert_eq!(outcome.shims_written, 0);
    assert!(!shim_path(&env).exists());
    assert_eq!(active_version(&env).as_deref(), Some("1.0.0"));
    // The shims directory is still offered/added on PATH (it will host
    // future tools' shims).
    assert_eq!(path_env.appended().len(), 1);
}

#[test]
fn remove_shims_referencing_deletes_only_matching_shims() {
    let env = TestEnv::new();
    seed_two_versions(&env);
    let path_env = FakePathEnvironment::new();

    activate_1_0(&env, &path_env);
    // A shim for another tool that must survive.
    let other = env.path().join(SHIMS_DIR).join("other.cmd");
    std::fs::write(&other, "@echo off\r\n\"C:\\somewhere\\else.exe\" %*\r\n").unwrap();

    let removed = remove_shims_referencing(
        env.path(),
        &Path::new("packages")
            .join(PACKAGE_ID)
            .join("1.0.0")
            .to_string_lossy(),
    );
    assert_eq!(removed, 1);
    assert!(!shim_path(&env).exists());
    assert!(other.exists(), "unrelated shim untouched");
}
