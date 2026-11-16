//! P3-05 integration test: the full real-data install flow driven
//! through the bridge operations exactly as the Flutter adapter will
//! call them (testing-strategy.md rule 1: loopback fixture server,
//! temp managed root, committed fixture archive — never a real vendor
//! host, never the real user PATH or registry).
//!
//! Flow: init_engine → seed catalog cache → start_install (real
//! download → verify → extract → receipt → validate) → event replay →
//! installed registry reads → activation consent pair → uninstall.
//!
//! The engine state is process-global, so every test in this binary
//! shares one initialized engine (idempotent re-init with the same
//! paths is allowed by the contract).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use devkit_bridge::api::engine::get_engine_paths;
use devkit_bridge::api::installed::{
    list_installed_packages, list_installed_versions, plan_activation_for, remove_managed_version,
    set_active_version,
};
use devkit_bridge::api::jobs::{get_job, get_job_events, start_install};
use devkit_core::manifest::{
    Architecture, Artifact, ArtifactOs, Category, Channel, InstallerKind, PackageManifest,
    PackageVersion, Platform, VersionPolicyName, SUPPORTED_SCHEMA_VERSION,
};
use devkit_registry::package_cache::PackageCache;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const PACKAGE_ID: &str = "devkit.bridge.tool";
const VERSION: &str = "1.0.0";

/// The committed P3-02 fixture archive, shared with the devkit-jobs and
/// devkit-install suites (same bytes through download → extraction →
/// receipt).
fn happy_zip() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../devkit-install/tests/fixtures/happy.zip");
    std::fs::read(path).expect("read committed happy.zip fixture")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Serves the fixture bytes with a Content-Length on an ephemeral
/// loopback port. Daemon thread; the test process terminates it.
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

struct EnginePaths {
    _temp: TempDir,
    db_path: PathBuf,
    managed_root: PathBuf,
}

/// Initializes the process-global engine once with temp paths (leaked
/// so the paths outlive every test in this binary).
static PATHS: OnceLock<EnginePaths> = OnceLock::new();

fn engine_paths() -> &'static EnginePaths {
    PATHS.get_or_init(|| {
        let temp = TempDir::new().expect("temp managed root");
        let managed_root = temp.path().join("managed");
        let db_path = managed_root.join("state").join("devkit.sqlite");
        devkit_bridge::api::engine::init_engine(
            Some(db_path.to_string_lossy().into_owned()),
            Some(managed_root.to_string_lossy().into_owned()),
        )
        .expect("init engine");
        EnginePaths {
            _temp: temp,
            db_path,
            managed_root,
        }
    })
}

/// Seeds the catalog cache exactly as the production flow does after a
/// catalog sync: the version row's artifact points at the loopback
/// fixture server (plain HTTP is allowed to 127.0.0.1 by the download
/// engine's documented carve-out), plus a wrong-platform decoy the
/// resolver must filter out.
fn seed_catalog(url: String) {
    let paths = engine_paths();
    let (this_platform, this_os, this_arch) = this_runner_platform();
    let bytes = happy_zip();
    let this_artifact = Artifact {
        os: this_os,
        architecture: this_arch,
        installer: InstallerKind::Archive,
        url: url.clone(),
        sha256: sha256_hex(&bytes),
        size_bytes: bytes.len() as u64,
        signature_url: None,
        publisher_identity: String::new(),
        archive_root: String::new(),
        executable_relative_path: "app/bin/tool.exe".to_owned(),
    };
    let (other_os, other_arch) = if matches!(this_os, ArtifactOs::Windows) {
        (ArtifactOs::Macos, Architecture::Arm64)
    } else {
        (ArtifactOs::Windows, Architecture::X64)
    };
    let other_platform = if matches!(this_platform, Platform::WindowsX64) {
        Platform::MacosArm64
    } else {
        Platform::WindowsX64
    };
    let other_artifact = Artifact {
        os: other_os,
        architecture: other_arch,
        installer: InstallerKind::Archive,
        url: url.replace("tool.zip", "tool.other.zip"),
        sha256: sha256_hex(&bytes),
        size_bytes: bytes.len() as u64,
        signature_url: None,
        publisher_identity: String::new(),
        archive_root: "tool".to_owned(),
        executable_relative_path: String::new(),
    };
    let manifest = PackageManifest {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        id: PACKAGE_ID.to_owned(),
        name: "Bridge Test Tool".to_owned(),
        description: "A tool installed through the bridge integration test".to_owned(),
        publisher: "Test Publisher".to_owned(),
        homepage: "https://example.invalid/bridge-tool".to_owned(),
        category: Category::Utility,
        supported_platforms: vec![this_platform, other_platform],
        default_version_policy: VersionPolicyName::Stable,
        versions: vec![PackageVersion {
            version: VERSION.to_owned(),
            channel: Channel::Stable,
            release_date: Some("2026-01-01".to_owned()),
            release_notes_url: None,
            dependencies: Vec::new(),
            artifacts: vec![this_artifact, other_artifact],
        }],
        license_expression: "MIT".to_owned(),
        maintainer: "maintainers@example.invalid".to_owned(),
        updated_at: "2026-01-01T00:00:00Z".to_owned(),
    };
    let db = devkit_registry::Database::open(&paths.db_path).expect("open db");
    PackageCache::new(db.connection())
        .upsert(&manifest)
        .expect("seed catalog cache");
}

fn terminal(job_id: &str) -> Option<String> {
    get_job(job_id.to_owned())
        .expect("get job")
        .map(|job| job.state)
        .filter(|state| {
            matches!(
                state.as_str(),
                "COMPLETED" | "FAILED" | "CANCELLED" | "ROLLED_BACK"
            )
        })
}

/// Waits for a terminal stage through the bridge snapshot op (the same
/// re-attach path the UI uses after a restart).
fn wait_terminal(job_id: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(state) = terminal(job_id) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "job {job_id} did not reach a terminal stage in time; last snapshot: {:?}",
            get_job(job_id.to_owned()).expect("get job")
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn full_install_flow_through_bridge_operations() {
    let paths = engine_paths();

    // Engine path facts (Settings/Storage screen data).
    let info = get_engine_paths().expect("engine paths");
    assert_eq!(
        Path::new(&info.managed_root),
        paths.managed_root.as_path(),
        "managed root reported as initialized"
    );
    assert!(info.db_path.ends_with("devkit.sqlite"));

    // Real install through the bridge (loopback fixture server).
    let bytes = happy_zip();
    let port = spawn_serving(bytes.clone());
    seed_catalog(format!("http://127.0.0.1:{port}/tool.zip"));

    // Catalog reads see the seeded cache (offline browsing source).
    let packages = devkit_bridge::api::catalog::search_catalog(Some("bridge".to_owned()), None)
        .expect("search catalog");
    assert_eq!(packages.len(), 1, "seeded package is searchable");
    assert_eq!(packages[0].id, PACKAGE_ID);
    assert_eq!(packages[0].name, "Bridge Test Tool");
    let details =
        devkit_bridge::api::catalog::get_package_details(PACKAGE_ID.to_owned()).expect("details");
    assert!(details.is_some());
    assert_eq!(
        details.as_ref().unwrap().versions[0].version,
        VERSION,
        "version listing comes from the cache"
    );

    let job_id = start_install(PACKAGE_ID.to_owned(), VERSION.to_owned()).expect("start install");
    assert!(!job_id.is_empty());

    // The job reaches COMPLETED (real download → verify → extract →
    // receipt → validate pipeline behind the single bridge call).
    assert_eq!(wait_terminal(&job_id), "COMPLETED");

    // Persisted event log replays from seq 0 (UI restart semantics).
    let replay = get_job_events(job_id.clone(), 0).expect("event replay");
    assert!(!replay.is_empty());
    assert_eq!(replay[0].seq, 1, "event log is 1-based and ordered");
    let stages: Vec<&str> = replay.iter().map(|event| event.stage.as_str()).collect();
    for stage in ["DOWNLOADING", "VERIFYING", "EXTRACTING", "COMPLETED"] {
        assert!(
            stages.contains(&stage),
            "missing stage {stage} in replay {stages:?}"
        );
    }
    // Incremental replay honors last_seen_seq.
    let last = replay.last().unwrap().seq;
    let incremental = get_job_events(job_id.clone(), last).expect("incremental replay");
    assert!(incremental.is_empty(), "nothing after the last seen event");
    // No error codes on the happy path.
    assert!(replay.iter().all(|event| event.error_code.is_none()));

    // The installed registry now shows the managed installation.
    let installed = list_installed_packages().expect("list installed");
    let row = installed
        .iter()
        .find(|row| row.package_id == PACKAGE_ID)
        .expect("installed row appears after a completed install");
    assert_eq!(row.version, VERSION);
    assert_eq!(row.status, "installed");
    assert!(!row.is_active, "nothing active before activation");
    assert_eq!(row.name, "Bridge Test Tool", "display name from the cache");

    // Receipt facts surface through the versions op.
    let versions = list_installed_versions(PACKAGE_ID.to_owned()).expect("versions");
    assert_eq!(versions.len(), 1);
    let version = &versions[0];
    assert!(version.file_count > 0, "receipt lists extracted files");
    assert!(version.total_bytes > 0);
    assert_eq!(
        version.entry_point.as_deref(),
        Some("app/bin/tool.exe"),
        "declared entry point from the receipt"
    );

    // Activation plan: pure read, nothing written yet.
    let plan =
        plan_activation_for(PACKAGE_ID.to_owned(), VERSION.to_owned()).expect("activation plan");
    assert_eq!(plan.package_id, PACKAGE_ID);
    assert!(plan.previous_active_version.is_none());
    assert!(
        !plan.shims.is_empty(),
        "the fixture declares an entry point, so shims are planned"
    );
    #[cfg(target_os = "windows")]
    {
        assert!(
            matches!(
                plan.path_change.kind,
                devkit_bridge::api::installed::PathChangeKind::Append
            ),
            "fresh temp root: shims dir is not on PATH yet"
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        assert!(
            matches!(
                plan.path_change.kind,
                devkit_bridge::api::installed::PathChangeKind::Unavailable
            ),
            "PATH integration is Windows-only in this release"
        );
    }

    // Declined consent writes nothing and refuses with the stable code.
    let declined = set_active_version(PACKAGE_ID.to_owned(), VERSION.to_owned(), false);
    let error = declined.expect_err("declined consent is refused");
    assert_eq!(error.code_str(), "PERMISSION_REQUIRED");
    let still_inactive = list_installed_packages().expect("list after declined consent");
    assert!(
        !still_inactive[0].is_active,
        "no writes on declined consent"
    );

    // Granted consent activates: active marker + shim writes.
    let outcome = set_active_version(PACKAGE_ID.to_owned(), VERSION.to_owned(), true)
        .expect("granted activation");
    assert_eq!(outcome.version, VERSION);
    assert!(
        outcome.shims_written > 0,
        "shims written under the managed root"
    );
    #[cfg(target_os = "windows")]
    assert!(matches!(
        outcome.path_change.kind,
        devkit_bridge::api::installed::PathChangeKind::Append
    ));
    #[cfg(not(target_os = "windows"))]
    assert!(matches!(
        outcome.path_change.kind,
        devkit_bridge::api::installed::PathChangeKind::Unavailable
    ));
    let active = list_installed_packages().expect("list after activation");
    let active_row = active
        .iter()
        .find(|row| row.package_id == PACKAGE_ID)
        .expect("row still present");
    assert!(active_row.is_active, "active marker set");

    // Shim file exists under the managed root (ADR 0007 layout).
    let shims_dir = paths.managed_root.join("shims");
    let shim_count = std::fs::read_dir(&shims_dir)
        .expect("shims dir exists after activation")
        .count();
    assert!(shim_count > 0, "at least one shim file was written");

    // Uninstall removes exactly this version, clears the active mapping,
    // and leaves the package absent from the registry afterwards.
    let removal =
        remove_managed_version(PACKAGE_ID.to_owned(), VERSION.to_owned()).expect("uninstall");
    assert!(removal.found);
    assert!(removal.files_removed > 0, "receipt-driven file removal");
    assert!(removal.cleared_active, "active mapping pointed here");
    let after = list_installed_packages().expect("list after uninstall");
    assert!(
        !after.iter().any(|row| row.package_id == PACKAGE_ID),
        "removed version leaves the registry"
    );
    // Removing an unknown version reports found=false (idempotent).
    let missing = remove_managed_version(PACKAGE_ID.to_owned(), VERSION.to_owned())
        .expect("repeat uninstall");
    assert!(!missing.found);
}

#[test]
fn start_install_rejects_bad_input_without_touching_jobs() {
    // engine_paths() ensures init happened for this binary too.
    engine_paths();
    let error =
        start_install("   ".to_owned(), VERSION.to_owned()).expect_err("empty package id refused");
    assert_eq!(error.code_str(), "INVALID_ARGUMENT");
    let error =
        start_install(PACKAGE_ID.to_owned(), "  ".to_owned()).expect_err("empty version refused");
    assert_eq!(error.code_str(), "INVALID_ARGUMENT");
    // Unknown package → the resolver maps it to PACKAGE_NOT_FOUND at the
    // RESOLVING stage and the job fails, rolls back, and rests in the
    // terminal ROLLED_BACK state (installation-lifecycle.md §3.2).
    let job_id = start_install("devkit.bridge.missing".to_owned(), VERSION.to_owned())
        .expect("job enqueued for unknown package");
    assert_eq!(wait_terminal(&job_id), "ROLLED_BACK");
    let job = get_job(job_id.clone())
        .expect("failed job snapshot")
        .unwrap();
    assert_eq!(job.error_code.as_deref(), Some("PACKAGE_NOT_FOUND"));
    let events = get_job_events(job_id, 0).expect("failure events");
    assert!(
        events
            .iter()
            .any(|event| event.error_code.as_deref() == Some("PACKAGE_NOT_FOUND")),
        "failure carries the stable code in the event log"
    );
}
