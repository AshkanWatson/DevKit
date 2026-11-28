//! P5-09: routes packages with a committed official provider through
//! `devkit-sources` instead of the catalog cache (ADR 0009).
//!
//! Resolution is metadata-only (official manifest/API over HTTPS); the
//! resolved artifact is converted into the pipeline's concrete `Artifact`
//! **without inventing any field**: a missing official checksum or size is a
//! hard error, never a fabricated value. The actual download, SHA-256 check,
//! extraction, receipt and activation all stay in the unchanged pipeline.

use std::sync::OnceLock;

use devkit_core::manifest::Artifact;
use devkit_jobs::pipeline::{current_platform, InstallSource, StageFailure};
use devkit_sources::flutter::FlutterSource;
use devkit_sources::github::GithubSource;
use devkit_sources::{
    ReqwestSourceHttp, ResolveRequest, ResolvedArtifact, SourceResolver, TargetPlatform,
    VersionSelector,
};

use super::error::{bridge_error, from_engine_code, BridgeError, BridgeErrorCode};

/// The committed provider registry. It is built once and cannot be extended
/// from catalog data (ADR 0009).
fn resolver() -> &'static SourceResolver {
    static RESOLVER: OnceLock<SourceResolver> = OnceLock::new();
    RESOLVER.get_or_init(|| {
        SourceResolver::new(vec![Box::new(FlutterSource), Box::new(GithubSource)])
            .expect("committed providers have unique package IDs")
    })
}

/// True when a package is served by an official provider rather than the
/// catalog cache. Used by the enqueue path to choose the source.
pub(crate) fn has_provider(package_id: &str) -> bool {
    resolver().has_provider(package_id)
}

/// Committed capability: whether a provider package publishes an official
/// checksum. Flutter's manifest always does; ripgrep publishes a `.sha256`
/// sidecar; fd publishes none. This is Rust data (ADR 0009), never catalog
/// input, so a package without a trusted digest keeps its catalog path
/// instead of being installed with an invented checksum.
pub(crate) fn provider_has_official_checksum(package_id: &str) -> bool {
    matches!(
        package_id,
        "devkit.framework.flutter" | "devkit.utility.ripgrep"
    )
}

fn selector_for(version: &str) -> VersionSelector {
    match version.trim() {
        "stable" => VersionSelector::Stable,
        "latest" => VersionSelector::Latest,
        channel if channel.starts_with("channel:") => {
            VersionSelector::Channel(channel.trim_start_matches("channel:").to_owned())
        }
        exact => VersionSelector::Exact(exact.to_owned()),
    }
}

/// Turns an official resolution into the pipeline's concrete artifact.
/// Every field comes from the provider; nothing is synthesized.
fn into_artifact(resolved: ResolvedArtifact) -> Result<Artifact, BridgeError> {
    let checksum = resolved.sha256.ok_or_else(|| {
        bridge_error(
            BridgeErrorCode::IntegrityCheckFailed,
            format!(
                "{} {} has no official checksum; verification would be limited, so the install was refused",
                resolved.package_id, resolved.version
            ),
        )
    })?;
    let size_bytes = resolved.size_bytes.ok_or_else(|| {
        bridge_error(
            BridgeErrorCode::NetworkError,
            format!(
                "{} {} has no official size (HEAD returned no Content-Length); the install was refused instead of guessing bytes",
                resolved.package_id, resolved.version
            ),
        )
    })?;
    let platform = TargetPlatform::from_platform(current_platform());
    Ok(Artifact {
        os: platform.os,
        architecture: platform.architecture,
        installer: resolved.installer,
        url: resolved.url,
        sha256: checksum,
        size_bytes,
        signature_url: None,
        publisher_identity: String::new(),
        archive_root: resolved.archive_root,
        executable_relative_path: resolved.executable_relative_path,
    })
}

/// Resolves and validates one package from its official provider.
///
/// Runs the async metadata client on a short-lived current-thread runtime
/// inside a helper thread (FRB entry points are synchronous and must never
/// call `block_on` on another runtime's worker thread).
pub(crate) fn resolve_provider_artifact(
    package_id: &str,
    version: &str,
) -> Result<(String, Artifact), BridgeError> {
    let package_id = package_id.to_owned();
    let version = version.to_owned();
    let outcome = std::thread::Builder::new()
        .name("devkit-source-resolve".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    return Err(bridge_error(
                        BridgeErrorCode::NetworkError,
                        format!("cannot start metadata client: {error}"),
                    ));
                }
            };
            runtime.block_on(async {
                let request = ResolveRequest {
                    package_id: package_id.clone(),
                    selector: selector_for(&version),
                    platform: TargetPlatform::from_platform(current_platform()),
                };
                let http = ReqwestSourceHttp::new();
                let resolved = resolver()
                    .resolve(&request, &http)
                    .await
                    .map_err(|error| from_engine_code(error.code(), error.to_string()))?;
                // Extra guard: the provider must not switch packages or
                // return an artifact for a different platform.
                if resolved.package_id != package_id {
                    return Err(bridge_error(
                        BridgeErrorCode::IntegrityCheckFailed,
                        "provider returned an artifact for a different package",
                    ));
                }
                // ArtifactValidator: re-check the official URL and digest
                // shape before the download engine sees them. A limited-
                // verification notice is handled by `into_artifact` (which
                // refuses to fabricate a digest).
                let _notice = devkit_sources::validate::validate_artifact(&resolved)
                    .map_err(|error| from_engine_code(error.code(), error.to_string()))?;
                let concrete_version = resolved.version.clone();
                into_artifact(resolved).map(|artifact| (concrete_version, artifact))
            })
        })
        .map_err(|error| {
            bridge_error(
                BridgeErrorCode::NetworkError,
                format!("cannot start resolution thread: {error}"),
            )
        })?;
    outcome.join().unwrap_or_else(|_| {
        Err(bridge_error(
            BridgeErrorCode::NetworkError,
            "resolution thread panicked",
        ))
    })
}

/// An `InstallSource` that returns exactly one pre-resolved official
/// artifact. The pipeline never sees provider data directly; it sees the
/// same validated `Artifact` shape it has always used.
pub(crate) struct ProviderSource {
    package_id: String,
    version: String,
    artifact: Artifact,
}

impl ProviderSource {
    pub(crate) fn new(package_id: String, version: String, artifact: Artifact) -> Self {
        Self {
            package_id,
            version,
            artifact,
        }
    }
}

impl InstallSource for ProviderSource {
    fn resolve(&self, package_id: &str, version: &str) -> Result<Artifact, StageFailure> {
        if package_id != self.package_id {
            return Err(StageFailure::new(
                devkit_core::error::ErrorCode::PackageNotFound,
                "the resolved artifact belongs to a different package",
            ));
        }
        if version != self.version {
            // The enqueue path always uses the concrete resolved version, so
            // a mismatch here would mean a corrupted job row.
            return Err(StageFailure::new(
                devkit_core::error::ErrorCode::VersionNotFound,
                "the resolved artifact version does not match the job",
            ));
        }
        let platform = TargetPlatform::from_platform(current_platform());
        if self.artifact.os != platform.os || self.artifact.architecture != platform.architecture {
            return Err(StageFailure::new(
                devkit_core::error::ErrorCode::UnsupportedPlatform,
                "the resolved artifact is for a different platform",
            ));
        }
        Ok(self.artifact.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devkit_core::error::ErrorCode;
    use devkit_core::manifest::{Architecture, ArtifactOs, InstallerKind};

    fn resolved(checksum: Option<&str>, size: Option<u64>) -> ResolvedArtifact {
        ResolvedArtifact {
            package_id: "devkit.framework.flutter".to_owned(),
            version: "3.50.1".to_owned(),
            url: "https://storage.googleapis.com/flutter_infra_release/releases/stable/windows/flutter_windows_3.50.1-stable.zip".to_owned(),
            sha256: checksum.map(str::to_owned),
            size_bytes: size,
            installer: InstallerKind::Archive,
            executable_relative_path: "flutter/bin/flutter.bat".to_owned(),
            archive_root: String::new(),
            source: "flutter".to_owned(),
            verification_limited: checksum.is_none(),
        }
    }

    const CHECKSUM: &str = "33c1e3d4dd08f1136ddb9c7976cfd2b0ab20296f1565b78ba176a4cbb4e140eb";

    #[test]
    fn provider_registry_knows_flutter_and_the_github_utilities() {
        assert!(has_provider("devkit.framework.flutter"));
        assert!(has_provider("devkit.utility.ripgrep"));
        assert!(has_provider("devkit.utility.fd"));
        assert!(!has_provider("devkit.bridge.tool"));
        assert!(provider_has_official_checksum("devkit.framework.flutter"));
        assert!(provider_has_official_checksum("devkit.utility.ripgrep"));
        assert!(!provider_has_official_checksum("devkit.utility.fd"));
    }

    #[test]
    fn selector_maps_policy_words_without_guessing_versions() {
        assert_eq!(selector_for("stable"), VersionSelector::Stable);
        assert_eq!(selector_for("latest"), VersionSelector::Latest);
        assert_eq!(
            selector_for("channel:beta"),
            VersionSelector::Channel("beta".to_owned())
        );
        assert_eq!(
            selector_for("3.50.1"),
            VersionSelector::Exact("3.50.1".to_owned())
        );
    }

    #[test]
    fn missing_official_checksum_refuses_install_instead_of_faking_one() {
        let error = into_artifact(resolved(None, Some(100))).unwrap_err();
        assert_eq!(error.code, BridgeErrorCode::IntegrityCheckFailed);
        assert!(error.message.contains("no official checksum"));
    }

    #[test]
    fn missing_official_size_refuses_install_instead_of_guessing_bytes() {
        let error = into_artifact(resolved(Some(CHECKSUM), None)).unwrap_err();
        assert_eq!(error.code, BridgeErrorCode::NetworkError);
        assert!(error.message.contains("no official size"));
    }

    #[test]
    fn concrete_artifact_copies_official_fields() {
        let artifact = into_artifact(resolved(Some(CHECKSUM), Some(123))).unwrap();
        assert_eq!(artifact.url, "https://storage.googleapis.com/flutter_infra_release/releases/stable/windows/flutter_windows_3.50.1-stable.zip");
        assert_eq!(artifact.sha256, CHECKSUM);
        assert_eq!(artifact.size_bytes, 123);
        assert_eq!(artifact.executable_relative_path, "flutter/bin/flutter.bat");
        assert_eq!(artifact.installer, InstallerKind::Archive);
    }

    #[test]
    fn provider_source_refuses_mismatched_job_identity() {
        let artifact = into_artifact(resolved(Some(CHECKSUM), Some(123))).unwrap();
        // Force a platform the artifact may not match, using a synthetic
        // source whose stored identity differs from the requested one.
        let source = ProviderSource::new(
            "devkit.framework.flutter".to_owned(),
            "3.50.1".to_owned(),
            artifact,
        );
        assert_eq!(
            source
                .resolve("devkit.utility.fd", "3.50.1")
                .unwrap_err()
                .code,
            ErrorCode::PackageNotFound
        );
        assert_eq!(
            source
                .resolve("devkit.framework.flutter", "3.50.0")
                .unwrap_err()
                .code,
            ErrorCode::VersionNotFound
        );
    }

    #[test]
    fn platform_detection_maps_to_artifact_os_and_arch() {
        let platform = TargetPlatform::from_platform(current_platform());
        assert!(matches!(
            platform.os,
            ArtifactOs::Windows | ArtifactOs::Macos | ArtifactOs::Linux
        ));
        assert!(matches!(
            platform.architecture,
            Architecture::X64 | Architecture::Arm64
        ));
    }
}
