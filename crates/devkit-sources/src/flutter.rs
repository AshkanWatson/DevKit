//! Flutter SDK official source (ADR 0009, first provider).
//!
//! Resolves versions and artifacts from Flutter's official release manifest:
//! `https://storage.googleapis.com/flutter_infra_release/releases/releases_<os>.json`.
//! The manifest's `base_url` + `archive` is the official download URL and its
//! `sha256` is the official digest — no URL, version or checksum is invented.
//! All platform naming (windows/macos/linux, x64/arm64, zip/tar.xz) is derived
//! from the real manifest shape verified 2026-11-28.

use devkit_core::manifest::{Architecture, ArtifactOs, InstallerKind};
use serde::Deserialize;

use crate::{
    ArtifactSource, ResolveRequest, ResolvedArtifact, SourceError, SourceHttp, TargetPlatform,
    VersionSelector,
};

/// One entry in Flutter's official `releases` array.
#[derive(Debug, Clone, Deserialize)]
struct FlutterRelease {
    version: String,
    channel: String,
    #[serde(default)]
    dart_sdk_arch: String,
    archive: String,
    sha256: String,
}

/// The official release manifest shape.
#[derive(Debug, Deserialize)]
struct FlutterManifest {
    base_url: String,
    releases: Vec<FlutterRelease>,
}

/// The Flutter SDK provider. Holds no state; all data comes from `SourceHttp`.
pub struct FlutterSource;

impl FlutterSource {
    /// Official manifest URL for a target OS (verified real endpoints).
    fn manifest_url(platform: &TargetPlatform) -> Result<String, SourceError> {
        let os = match platform.os {
            ArtifactOs::Windows => "windows",
            ArtifactOs::Macos => "macos",
            ArtifactOs::Linux => "linux",
        };
        Ok(format!(
            "https://storage.googleapis.com/flutter_infra_release/releases/releases_{os}.json"
        ))
    }

    /// Maps the manifest's `dart_sdk_arch` to the requested architecture.
    /// Windows/Linux manifests only carry x64; macOS carries both, selected
    /// by the `dart_sdk_arch` field on separate release rows.
    fn arch_matches(platform: &TargetPlatform, release: &FlutterRelease) -> bool {
        match platform.os {
            ArtifactOs::Macos => match platform.architecture {
                Architecture::X64 => release.dart_sdk_arch == "x64",
                Architecture::Arm64 => release.dart_sdk_arch == "arm64",
            },
            // Windows and Linux official archives are x64-only in the
            // manifest; arm64 requests are reported unsupported rather than
            // silently handed an x64 build.
            _ => platform.architecture == Architecture::X64,
        }
    }

    fn select<'a>(
        releases: &'a [FlutterRelease],
        selector: &VersionSelector,
    ) -> Option<&'a FlutterRelease> {
        match selector {
            VersionSelector::Exact(version) => {
                releases.iter().find(|release| &release.version == version)
            }
            // Flutter's manifest lists releases newest first.
            VersionSelector::Stable => releases.iter().find(|release| release.channel == "stable"),
            VersionSelector::Latest => releases.first(),
            VersionSelector::Channel(channel) => {
                releases.iter().find(|release| &release.channel == channel)
            }
        }
    }
}

#[async_trait::async_trait]
impl ArtifactSource for FlutterSource {
    fn package_ids(&self) -> &[&str] {
        &["devkit.framework.flutter"]
    }

    async fn resolve(
        &self,
        request: &ResolveRequest,
        http: &dyn SourceHttp,
    ) -> Result<ResolvedArtifact, SourceError> {
        let url = Self::manifest_url(&request.platform)?;
        let body = http.get_text(&url).await?;
        let manifest: FlutterManifest = serde_json::from_str(&body)
            .map_err(|error| SourceError::InvalidMetadata(error.to_string()))?;

        let candidates: Vec<FlutterRelease> = manifest
            .releases
            .iter()
            .filter(|release| Self::arch_matches(&request.platform, release))
            .cloned()
            .collect();
        let release = Self::select(&candidates, &request.selector).ok_or_else(|| match &request
            .selector
        {
            VersionSelector::Exact(version) => SourceError::VersionNotFound(version.clone()),
            _ => SourceError::ArtifactNotFound,
        })?;

        // Official URL = manifest base_url + archive path (verified shape).
        let download_url = format!(
            "{}/{}",
            manifest.base_url.trim_end_matches('/'),
            release.archive.trim_start_matches('/')
        );
        // Size is not published by Flutter's manifest. Ask the official host
        // over HEAD (real server data) rather than inventing a number.
        let size_bytes = http.head_length(&download_url).await?;
        let entry_point = match request.platform.os {
            ArtifactOs::Windows => "flutter/bin/flutter.bat",
            ArtifactOs::Macos | ArtifactOs::Linux => "flutter/bin/flutter",
        };

        Ok(ResolvedArtifact {
            package_id: request.package_id.clone(),
            version: release.version.clone(),
            url: download_url,
            sha256: Some(release.sha256.clone()),
            size_bytes,
            installer: InstallerKind::Archive,
            executable_relative_path: entry_point.to_owned(),
            archive_root: String::new(),
            source: "flutter".to_owned(),
            verification_limited: false,
        })
    }
}
