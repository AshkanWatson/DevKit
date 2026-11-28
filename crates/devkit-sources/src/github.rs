//! GitHub Releases provider (ADR 0009): validates the ADR 0008 asset-delivery
//! redirect with ripgrep and fd. Uses the official `api.github.com` Releases
//! API and the published `.sha256` sidecars — no invented URLs or checksums.

use devkit_core::manifest::{Architecture, ArtifactOs, InstallerKind};
use serde::Deserialize;

use crate::{
    ArtifactSource, ResolveRequest, ResolvedArtifact, SourceError, SourceHttp, TargetPlatform,
    VersionSelector,
};

/// Committed provider definition: package ID → official GitHub repository.
/// Every entry was verified against the live API on 2026-11-28.
const PROVIDERS: &[(&str, &str, &str)] = &[
    ("devkit.utility.ripgrep", "BurntSushi", "ripgrep"),
    ("devkit.utility.fd", "sharkdp", "fd"),
];

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

/// Selects the asset name suffix for a platform from verified naming
/// conventions (real asset lists verified 2026-11-28). `None` means the
/// provider publishes no artifact for that target.
fn asset_suffix(package_id: &str, platform: &TargetPlatform) -> Option<&'static str> {
    let is_fd = package_id == "devkit.utility.fd";
    match (platform.os, platform.architecture) {
        (ArtifactOs::Windows, Architecture::X64) => Some("x86_64-pc-windows-msvc.zip"),
        (ArtifactOs::Macos, Architecture::X64) => Some("x86_64-apple-darwin.tar.gz"),
        (ArtifactOs::Macos, Architecture::Arm64) => Some("aarch64-apple-darwin.tar.gz"),
        (ArtifactOs::Linux, Architecture::X64) => Some(if is_fd {
            "x86_64-unknown-linux-gnu.tar.gz"
        } else {
            "x86_64-unknown-linux-musl.tar.gz"
        }),
        (ArtifactOs::Linux, Architecture::Arm64) => Some("aarch64-unknown-linux-gnu.tar.gz"),
        // No verified Windows-arm64 asset for either package.
        (ArtifactOs::Windows, Architecture::Arm64) => None,
    }
}

/// Derives the executable path relative to the extracted install root.
/// Real GitHub archives (verified against the catalog) wrap the binary in a
/// directory named after the asset without its extension, e.g.
/// `ripgrep-14.1.1-x86_64-pc-windows-msvc/rg.exe`.
fn entry_point(package_id: &str, platform: &TargetPlatform, asset_name: &str) -> String {
    let binary = if package_id == "devkit.utility.ripgrep" {
        "rg"
    } else {
        "fd"
    };
    let binary = match platform.os {
        ArtifactOs::Windows => format!("{binary}.exe"),
        _ => binary.to_owned(),
    };
    let archive_root = asset_name
        .strip_suffix(".tar.gz")
        .or_else(|| asset_name.strip_suffix(".zip"))
        .unwrap_or(asset_name);
    format!("{archive_root}/{binary}")
}

/// GitHub Releases provider.
pub struct GithubSource;

impl GithubSource {
    fn repo(package_id: &str) -> Result<(&'static str, &'static str), SourceError> {
        PROVIDERS
            .iter()
            .find(|(id, _, _)| *id == package_id)
            .map(|(_, owner, repo)| (*owner, *repo))
            .ok_or_else(|| SourceError::UnsupportedPackage(package_id.to_owned(), "github".into()))
    }

    fn select_release<'a>(
        releases: &'a [GithubRelease],
        selector: &VersionSelector,
    ) -> Option<&'a GithubRelease> {
        let usable: Vec<&GithubRelease> = releases
            .iter()
            .filter(|release| !release.draft && !release.prerelease)
            .collect();
        match selector {
            VersionSelector::Exact(version) => usable.into_iter().find(|release| {
                &release.tag_name == version
                    || release.tag_name.trim_start_matches('v') == version.as_str()
            }),
            VersionSelector::Stable => usable.first().copied(),
            // GitHub's API lists releases newest first. "Latest" includes
            // prereleases but never drafts (drafts have no published asset).
            VersionSelector::Latest => releases.iter().find(|release| !release.draft),
            VersionSelector::Channel(channel) => releases
                .iter()
                .find(|release| !release.draft && release.tag_name.contains(channel)),
        }
    }
}

#[async_trait::async_trait]
impl ArtifactSource for GithubSource {
    fn package_ids(&self) -> &[&str] {
        // Cannot return a slice of a const's IDs directly; use a static list.
        &["devkit.utility.ripgrep", "devkit.utility.fd"]
    }

    async fn resolve(
        &self,
        request: &ResolveRequest,
        http: &dyn SourceHttp,
    ) -> Result<ResolvedArtifact, SourceError> {
        let (owner, repo) = Self::repo(&request.package_id)?;
        let url = format!("https://api.github.com/repos/{owner}/{repo}/releases");
        let body = http.get_text(&url).await?;
        let releases: Vec<GithubRelease> = serde_json::from_str(&body)
            .map_err(|error| SourceError::InvalidMetadata(error.to_string()))?;
        let release = Self::select_release(&releases, &request.selector).ok_or_else(|| {
            match &request.selector {
                VersionSelector::Exact(version) => SourceError::VersionNotFound(version.clone()),
                _ => SourceError::ArtifactNotFound,
            }
        })?;

        let suffix = asset_suffix(&request.package_id, &request.platform)
            .ok_or(SourceError::UnsupportedPlatform(request.platform))?;
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name.ends_with(suffix))
            .ok_or(SourceError::ArtifactNotFound)?;

        // Official checksum: published `.sha256` sidecar, when present.
        let sidecar_name = format!("{}.sha256", asset.name);
        let sidecar_url = asset
            .browser_download_url
            .rsplit_once('/')
            .map(|(prefix, _)| format!("{prefix}/{sidecar_name}"))
            .ok_or_else(|| SourceError::InvalidMetadata("asset URL has no path".into()))?;
        let sidecar = http.get_text(&sidecar_url).await.ok();
        // Real GitHub `.sha256` sidecars (verified for ripgrep 14.1.1) carry a
        // human-readable header/footer around the digest, e.g.
        //   SHA256 hash of <asset>:
        //   <64 hex>
        //   CertUtil: -hashfile command completed successfully.
        // Scan for the 64-hex token; never take a header word as the digest.
        let sha256 = sidecar.and_then(|text| {
            text.split_whitespace()
                .find(|value| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
                .map(str::to_owned)
        });
        let verification_limited = sha256.is_none();

        Ok(ResolvedArtifact {
            package_id: request.package_id.clone(),
            version: release.tag_name.clone(),
            url: asset.browser_download_url.clone(),
            sha256,
            size_bytes: Some(asset.size),
            installer: InstallerKind::Archive,
            executable_relative_path: entry_point(
                &request.package_id,
                &request.platform,
                &asset.name,
            ),
            archive_root: String::new(),
            source: "github".to_owned(),
            verification_limited,
        })
    }
}
