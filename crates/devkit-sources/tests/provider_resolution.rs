//! Provider resolution tests (ADR 0009, testing-strategy.md rule 1).
//!
//! All metadata comes from committed fixtures captured from official sources;
//! no test contacts a real host or runs a downloaded artifact. The fake HTTP
//! also verifies that providers only request the official endpoints we expect.

use std::collections::HashMap;
use std::sync::Mutex;

use devkit_core::manifest::{Architecture, ArtifactOs};
use devkit_sources::flutter::FlutterSource;
use devkit_sources::github::GithubSource;
use devkit_sources::{
    ArtifactSource, ResolveRequest, SourceError, SourceHttp, SourceResolver, TargetPlatform,
    VersionSelector,
};

/// Fake metadata client backed by committed fixtures. Records every URL so
/// tests can assert providers hit only official endpoints.
struct FakeHttp {
    texts: HashMap<String, String>,
    lengths: HashMap<String, Option<u64>>,
    requested: Mutex<Vec<String>>,
}

impl FakeHttp {
    fn new() -> Self {
        Self {
            texts: HashMap::new(),
            lengths: HashMap::new(),
            requested: Mutex::new(Vec::new()),
        }
    }

    fn with_text(mut self, url: &str, body: &str) -> Self {
        self.texts.insert(url.to_owned(), body.to_owned());
        self
    }

    fn with_length(mut self, url: &str, length: u64) -> Self {
        self.lengths.insert(url.to_owned(), Some(length));
        self
    }

    fn requested(&self) -> Vec<String> {
        self.requested.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl SourceHttp for FakeHttp {
    async fn get_text(&self, url: &str) -> Result<String, SourceError> {
        self.requested.lock().unwrap().push(url.to_owned());
        self.texts
            .get(url)
            .cloned()
            .ok_or_else(|| SourceError::Http(format!("unexpected URL: {url}")))
    }

    async fn head_length(&self, url: &str) -> Result<Option<u64>, SourceError> {
        self.requested.lock().unwrap().push(url.to_owned());
        Ok(self.lengths.get(url).copied().flatten())
    }
}

const WINDOWS_MANIFEST: &str = include_str!("fixtures/flutter_releases_windows.json");
const MACOS_MANIFEST: &str = include_str!("fixtures/flutter_releases_macos.json");
const LINUX_MANIFEST: &str = include_str!("fixtures/flutter_releases_linux.json");

fn flutter_http() -> FakeHttp {
    FakeHttp::new()
        .with_text(
            "https://storage.googleapis.com/flutter_infra_release/releases/releases_windows.json",
            WINDOWS_MANIFEST,
        )
        .with_text(
            "https://storage.googleapis.com/flutter_infra_release/releases/releases_macos.json",
            MACOS_MANIFEST,
        )
        .with_text(
            "https://storage.googleapis.com/flutter_infra_release/releases/releases_linux.json",
            LINUX_MANIFEST,
        )
        .with_length(
            "https://storage.googleapis.com/flutter_infra_release/releases/stable/windows/flutter_windows_3.50.1-stable.zip",
            1_000_000,
        )
        .with_length(
            "https://storage.googleapis.com/flutter_infra_release/releases/stable/macos/flutter_macos_3.50.1-stable.zip",
            1_100_000,
        )
        .with_length(
            "https://storage.googleapis.com/flutter_infra_release/releases/stable/macos/flutter_macos_arm64_3.50.1-stable.zip",
            1_200_000,
        )
        .with_length(
            "https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/flutter_linux_3.50.1-stable.tar.xz",
            900_000,
        )
}

fn platform(os: ArtifactOs, architecture: Architecture) -> TargetPlatform {
    TargetPlatform { os, architecture }
}

#[tokio::test]
async fn flutter_resolves_exact_windows_artifact() {
    let source = FlutterSource;
    let http = flutter_http();
    let artifact = source
        .resolve(
            &ResolveRequest {
                package_id: "devkit.framework.flutter".to_owned(),
                selector: VersionSelector::Exact("3.50.1".to_owned()),
                platform: platform(ArtifactOs::Windows, Architecture::X64),
            },
            &http,
        )
        .await
        .unwrap();
    assert_eq!(artifact.package_id, "devkit.framework.flutter");
    assert_eq!(artifact.version, "3.50.1");
    assert_eq!(artifact.source, "flutter");
    assert_eq!(
        artifact.url,
        "https://storage.googleapis.com/flutter_infra_release/releases/stable/windows/flutter_windows_3.50.1-stable.zip"
    );
    assert_eq!(
        artifact.sha256.as_deref(),
        Some("33c1e3d4dd08f1136ddb9c7976cfd2b0ab20296f1565b78ba176a4cbb4e140eb")
    );
    assert_eq!(artifact.size_bytes, Some(1_000_000));
    assert_eq!(artifact.executable_relative_path, "flutter/bin/flutter.bat");
    assert!(!artifact.verification_limited);
    assert!(http
        .requested()
        .iter()
        .all(|url| url.starts_with("https://storage.googleapis.com/")));
}

#[tokio::test]
async fn flutter_resolves_macos_arm64_by_arch() {
    let source = FlutterSource;
    let http = flutter_http();
    let artifact = source
        .resolve(
            &ResolveRequest {
                package_id: "devkit.framework.flutter".to_owned(),
                selector: VersionSelector::Exact("3.50.1".to_owned()),
                platform: platform(ArtifactOs::Macos, Architecture::Arm64),
            },
            &http,
        )
        .await
        .unwrap();
    assert!(artifact
        .url
        .contains("flutter_macos_arm64_3.50.1-stable.zip"));
    assert_eq!(
        artifact.sha256.as_deref(),
        Some("872883d49965e0cc1d8e477d2d87f2f45930d13f94b34889e53783586f04c715")
    );
    assert_eq!(artifact.size_bytes, Some(1_200_000));
}

#[tokio::test]
async fn flutter_stable_selects_first_stable_release() {
    let source = FlutterSource;
    let http = flutter_http();
    let artifact = source
        .resolve(
            &ResolveRequest {
                package_id: "devkit.framework.flutter".to_owned(),
                selector: VersionSelector::Stable,
                platform: platform(ArtifactOs::Windows, Architecture::X64),
            },
            &http,
        )
        .await
        .unwrap();
    assert_eq!(artifact.version, "3.50.1");
    assert_eq!(artifact.source, "flutter");
    assert!(!artifact.verification_limited);
}

#[tokio::test]
async fn flutter_channel_beta_resolves_the_beta_archive() {
    let source = FlutterSource;
    let http = flutter_http();
    let artifact = source
        .resolve(
            &ResolveRequest {
                package_id: "devkit.framework.flutter".to_owned(),
                selector: VersionSelector::Channel("beta".to_owned()),
                platform: platform(ArtifactOs::Windows, Architecture::X64),
            },
            &http,
        )
        .await
        .unwrap();
    assert_eq!(artifact.version, "3.51.0-0.2.pre");
    assert!(artifact
        .url
        .contains("beta/windows/flutter_windows_3.51.0-0.2.pre-beta.zip"));
}

#[tokio::test]
async fn flutter_latest_uses_the_newest_manifest_entry() {
    let source = FlutterSource;
    let http = flutter_http();
    let artifact = source
        .resolve(
            &ResolveRequest {
                package_id: "devkit.framework.flutter".to_owned(),
                selector: VersionSelector::Latest,
                platform: platform(ArtifactOs::Windows, Architecture::X64),
            },
            &http,
        )
        .await
        .unwrap();
    // The manifest lists newest first; in this fixture that is 3.50.1.
    assert_eq!(artifact.version, "3.50.1");
}

#[tokio::test]
async fn flutter_resolves_linux_x64_artifact() {
    let source = FlutterSource;
    let http = flutter_http();
    let artifact = source
        .resolve(
            &ResolveRequest {
                package_id: "devkit.framework.flutter".to_owned(),
                selector: VersionSelector::Exact("3.50.1".to_owned()),
                platform: platform(ArtifactOs::Linux, Architecture::X64),
            },
            &http,
        )
        .await
        .unwrap();
    assert!(artifact.url.ends_with("flutter_linux_3.50.1-stable.tar.xz"));
    assert_eq!(
        artifact.sha256.as_deref(),
        Some("4ad17fc8462fbcef2ba19ff4a78c0a16b7147fde1add8578e267ea62cdb92678")
    );
}

#[tokio::test]
async fn flutter_reports_missing_version_without_inventing_one() {
    let source = FlutterSource;
    let http = flutter_http();
    let error = source
        .resolve(
            &ResolveRequest {
                package_id: "devkit.framework.flutter".to_owned(),
                selector: VersionSelector::Exact("9.99.99".to_owned()),
                platform: platform(ArtifactOs::Windows, Architecture::X64),
            },
            &http,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::VersionNotFound(_)));
}

#[tokio::test]
async fn flutter_rejects_windows_arm64_as_unsupported() {
    // The official manifest only publishes x64 for Windows. We must not hand
    // back an x64 build for an arm64 request.
    let source = FlutterSource;
    let http = flutter_http();
    let error = source
        .resolve(
            &ResolveRequest {
                package_id: "devkit.framework.flutter".to_owned(),
                selector: VersionSelector::Stable,
                platform: platform(ArtifactOs::Windows, Architecture::Arm64),
            },
            &http,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        SourceError::VersionNotFound(_) | SourceError::ArtifactNotFound
    ));
}

const RIPGREP_RELEASES: &str = include_str!("fixtures/github_ripgrep_releases.json");
const RIPGREP_SIDECAR: &str = include_str!("fixtures/github_ripgrep_sha256.txt");

fn github_http() -> FakeHttp {
    FakeHttp::new()
        .with_text(
            "https://api.github.com/repos/BurntSushi/ripgrep/releases",
            RIPGREP_RELEASES,
        )
        .with_text(
            "https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-pc-windows-msvc.zip.sha256",
            RIPGREP_SIDECAR,
        )
}

#[tokio::test]
async fn github_resolves_ripgrep_with_sidecar_checksum() {
    let source = GithubSource;
    let http = github_http();
    let artifact = source
        .resolve(
            &ResolveRequest {
                package_id: "devkit.utility.ripgrep".to_owned(),
                selector: VersionSelector::Stable,
                platform: platform(ArtifactOs::Windows, Architecture::X64),
            },
            &http,
        )
        .await
        .unwrap();
    assert_eq!(artifact.source, "github");
    assert_eq!(artifact.version, "14.1.1");
    assert!(artifact
        .url
        .ends_with("ripgrep-14.1.1-x86_64-pc-windows-msvc.zip"));
    assert_eq!(artifact.size_bytes, Some(2_058_893));
    assert_eq!(
        artifact.sha256.as_deref(),
        Some("d0f534024c42afd6cb4d38907c25cd2b249b79bbe6cc1dbee8e3e37c2b6e25a1")
    );
    assert_eq!(
        artifact.executable_relative_path,
        "ripgrep-14.1.1-x86_64-pc-windows-msvc/rg.exe"
    );
    assert!(!artifact.verification_limited);
}

#[tokio::test]
async fn resolver_dispatches_to_the_registered_provider() {
    let resolver =
        SourceResolver::new(vec![Box::new(FlutterSource), Box::new(GithubSource)]).unwrap();
    assert!(resolver.has_provider("devkit.framework.flutter"));
    assert!(resolver.has_provider("devkit.utility.ripgrep"));
    assert!(!resolver.has_provider("devkit.unknown"));

    let http = flutter_http();
    let artifact = resolver
        .resolve(
            &ResolveRequest {
                package_id: "devkit.framework.flutter".to_owned(),
                selector: VersionSelector::Stable,
                platform: platform(ArtifactOs::Windows, Architecture::X64),
            },
            &http,
        )
        .await
        .unwrap();
    assert_eq!(artifact.source, "flutter");
}

#[tokio::test]
async fn resolver_rejects_ambiguous_provider_registration() {
    // Avoid `unwrap_err()` because SourceResolver is intentionally opaque.
    let error = match SourceResolver::new(vec![Box::new(FlutterSource), Box::new(FlutterSource)]) {
        Ok(_) => panic!("duplicate registration must be rejected"),
        Err(error) => error,
    };
    assert!(matches!(error, SourceError::InvalidMetadata(_)));
}
