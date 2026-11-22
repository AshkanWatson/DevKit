//! Maintainer-only end-to-end proof (P3-03/P3-04 tooling): installs a REAL
//! vendor artifact through the real pipeline — real HTTPS download with
//! byte-count progress, SHA-256 verification, archive extraction, atomic
//! rename, receipt registration, validation, and then uninstall.
//!
//! This is NOT a test (testing-strategy.md: tests never touch the network).
//! It is an opt-in example, run explicitly by a maintainer:
//!
//! ```text
//! DEVKIT_REAL_ARTIFACT=1 cargo run -p devkit-jobs --example real_install
//! ```
//!
//! The artifact facts mirror `services/catalog-api` `seed_real_catalog`
//! (Python 3.12.8 embeddable windows-x64, measured 2026-11-15). python.org
//! serves the file directly — GitHub release URLs redirect to a CDN host,
//! which the same-host redirect policy (security-model.md §3.3) declines.
//! Nothing is executed and nothing outside a temp directory is touched.

use std::path::{Path, PathBuf};

use devkit_core::error::ErrorCode;
use devkit_core::manifest::{
    Architecture, Artifact, ArtifactOs, Category, Channel, InstallerKind, PackageManifest,
    PackageVersion, Platform, VersionPolicyName, SUPPORTED_SCHEMA_VERSION,
};
use devkit_download::ReqwestDownloadHttp;
use devkit_install::{ArchiveLimits, ProcessExeRunner};
use devkit_jobs::pipeline::{InstallReceipt, InstallSource, RealInstallConfig, StageFailure};
use devkit_jobs::{JobManager, RunOutcome, Stage, UninstallOutcome};
use devkit_registry::installed::InstalledRepo;
use devkit_registry::package_cache::PackageCache;
use devkit_registry::{Database, InstalledStatus};
use tempfile::TempDir;

const PACKAGE_ID: &str = "devkit.lang.python";
const VERSION: &str = "3.12.8";

/// The real vendor release, as recorded in the catalog seed.
fn real_artifact() -> Artifact {
    Artifact {
        os: ArtifactOs::Windows,
        architecture: Architecture::X64,
        installer: InstallerKind::Archive,
        url: "https://www.python.org/ftp/python/3.12.8/python-3.12.8-embed-amd64.zip".to_owned(),
        sha256: "8d3f33be9eb810f23c102f08475af2854e50484b8e4e06275e937be61ce3d2fb".to_owned(),
        size_bytes: 11_094_114,
        signature_url: None,
        publisher_identity: String::new(),
        archive_root: String::new(),
        executable_relative_path: "python.exe".to_owned(),
    }
}

struct RealVendorSource {
    artifact: Artifact,
}

impl InstallSource for RealVendorSource {
    fn resolve(&self, package_id: &str, version: &str) -> Result<Artifact, StageFailure> {
        if package_id != PACKAGE_ID || version != VERSION {
            return Err(StageFailure::new(
                ErrorCode::VersionNotFound,
                "this example installs exactly Python 3.12.8",
            ));
        }
        Ok(self.artifact.clone())
    }
}

/// Seeds the catalog cache the way production does (the Django-populated
/// `catalog_cache`): `installed_versions.package_id` has an FK to
/// `packages(id)`, so the cached manifest must exist before an install.
fn seed_catalog_cache(db_path: &Path) {
    let db = Database::open(db_path).expect("open db");
    PackageCache::new(db.connection())
        .upsert(&PackageManifest {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            id: PACKAGE_ID.to_owned(),
            name: "Python".to_owned(),
            description: "The Python programming language (embeddable).".to_owned(),
            publisher: "Python Software Foundation".to_owned(),
            homepage: "https://www.python.org/".to_owned(),
            category: Category::Language,
            supported_platforms: vec![Platform::WindowsX64],
            default_version_policy: VersionPolicyName::Stable,
            versions: vec![PackageVersion {
                version: VERSION.to_owned(),
                channel: Channel::Stable,
                release_date: Some("2024-12-04".to_owned()),
                release_notes_url: None,
                dependencies: Vec::new(),
                artifacts: vec![real_artifact()],
            }],
            license_expression: "PSF-2.0".to_owned(),
            maintainer: "devkit-maintainers".to_owned(),
            updated_at: "2026-11-15T00:00:00Z".to_owned(),
        })
        .expect("seed catalog cache");
}

fn main() {
    if std::env::var("DEVKIT_REAL_ARTIFACT").as_deref() != Ok("1") {
        eprintln!(
            "refusing to touch the network without DEVKIT_REAL_ARTIFACT=1.\n\
             run:  DEVKIT_REAL_ARTIFACT=1 cargo run -p devkit-jobs --example real_install"
        );
        std::process::exit(2);
    }

    let db_dir = TempDir::new().expect("temp db dir");
    let managed_root = TempDir::new().expect("temp managed root");
    let db_path: PathBuf = db_dir.path().join("state").join("devkit.sqlite");
    seed_catalog_cache(&db_path);

    let manager =
        JobManager::open_with(&db_path, Some(managed_root.path())).expect("open job manager");
    let job_id = manager
        .enqueue_real_install(
            RealInstallConfig {
                managed_root: managed_root.path().to_path_buf(),
                transport: ReqwestDownloadHttp::new().expect("HTTPS transport"),
                source: RealVendorSource {
                    artifact: real_artifact(),
                },
                limits: ArchiveLimits::default(),
                vendor_runner: Box::new(devkit_install::ProcessExeRunner),
                validation_hook: None,
            },
            PACKAGE_ID,
            VERSION,
        )
        .expect("enqueue real install");

    println!("job {job_id}: downloading the real Python 3.12.8 embeddable release…");
    let outcome = manager.wait(&job_id).expect("job finishes");
    if outcome != RunOutcome::Completed {
        for event in manager.watch_job_events(&job_id, 0).expect("event replay") {
            eprintln!(
                "  event {} {:?} progress={:?} code={:?} msg={}",
                event.seq, event.stage, event.progress, event.error_code, event.message
            );
        }
        panic!("the real install must complete, got {outcome:?}");
    }

    // The receipt: real artifact facts, real extracted files.
    let db = Database::open(&db_path).expect("open db");
    let record = InstalledRepo::new(db.connection())
        .get(PACKAGE_ID, VERSION)
        .expect("registry read")
        .expect("receipt row registered");
    assert_eq!(record.status, InstalledStatus::Installed);
    let receipt: InstallReceipt = serde_json::from_str(&record.receipt_json).expect("receipt JSON");
    println!(
        "receipt: installer={} archive={}",
        receipt.installer,
        receipt
            .archive
            .as_ref()
            .map(|part| part.format.clone())
            .unwrap_or_default()
    );
    if let Some(archive) = &receipt.archive {
        println!("  sha256 = {}", archive.sha256_hex);
        println!("  size   = {} bytes", archive.size_bytes);
    }
    println!("  files  = {}", receipt.files.len());
    let entry = receipt
        .executable_relative_path
        .as_deref()
        .expect("entry point recorded");
    let executable = managed_root.path().join(&receipt.install_path).join(entry);
    assert!(
        executable.is_file(),
        "the real executable must be installed"
    );
    println!("  entry  = {}", executable.display());

    // Stage chain was real (download progress, verify, extract, complete).
    let stages: Vec<Stage> = manager
        .watch_job_events(&job_id, 0)
        .expect("event replay")
        .into_iter()
        .filter_map(|event| Stage::parse(&event.stage))
        .collect();
    for stage in [
        Stage::Downloading,
        Stage::Verifying,
        Stage::Extracting,
        Stage::Validating,
        Stage::Completed,
    ] {
        assert!(stages.contains(&stage), "missing {stage:?}");
    }

    // Uninstall removes exactly this version again (reversibility).
    let removed = devkit_jobs::uninstall_version(
        managed_root.path(),
        db.connection(),
        PACKAGE_ID,
        VERSION,
        &ProcessExeRunner,
    )
    .expect("uninstall");
    let UninstallOutcome::Removed { files_removed, .. } = removed else {
        panic!("expected Removed, got {removed:?}");
    };
    assert!(files_removed > 0, "receipt files removed");
    let install_dir = managed_root.path().join("packages").join(PACKAGE_ID);
    assert!(!install_dir.join(VERSION).exists(), "version dir removed");
    println!("uninstall: {files_removed} files removed, directory gone");
    println!("OK: the real vendor artifact installed, validated, and uninstalled cleanly");
}
