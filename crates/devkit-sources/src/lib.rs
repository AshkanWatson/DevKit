//! Provider-based official artifact source resolution (ADR 0009).
//!
//! Resolves a package + version policy + platform into a concrete artifact
//! declared by the package's **official** provider (Flutter release manifest,
//! GitHub Releases API, ...). Downloads still happen in `devkit-download` with
//! the documented HTTPS/redirect/SHA-256 policy — this crate only answers
//! "which official URL, which official checksum".
//!
//! Everything is injectable: [`SourceHttp`] is the metadata HTTP seam, so
//! tests use a fake and never contact a real host (testing-strategy.md rule 1).
//! The allowlist for cross-host redirects lives in `devkit-download` (ADR 0008)
//! and is not data this crate can extend.

use devkit_core::error::ErrorCode;
use devkit_core::manifest::{Architecture, ArtifactOs, InstallerKind, Platform};

pub mod flutter;
pub mod github;
pub mod http;
pub mod resolver;
pub mod validate;

pub use http::{ReqwestSourceHttp, SourceHttp};
pub use resolver::SourceResolver;

/// How a version was requested. Maps to `VersionPolicyName` at the boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSelector {
    /// A concrete version string; never resolves to a different version.
    Exact(String),
    /// Newest non-prerelease release.
    Stable,
    /// Newest release (prereleases included).
    Latest,
    /// Newest release within a named channel.
    Channel(String),
}

/// The platform for artifact selection (ADR 0004 six supported platforms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetPlatform {
    pub os: ArtifactOs,
    pub architecture: Architecture,
}

impl TargetPlatform {
    /// Maps a devkit-core `Platform` to its OS + architecture parts.
    pub fn from_platform(platform: Platform) -> Self {
        match platform {
            Platform::WindowsX64 => Self {
                os: ArtifactOs::Windows,
                architecture: Architecture::X64,
            },
            Platform::WindowsArm64 => Self {
                os: ArtifactOs::Windows,
                architecture: Architecture::Arm64,
            },
            Platform::MacosX64 => Self {
                os: ArtifactOs::Macos,
                architecture: Architecture::X64,
            },
            Platform::MacosArm64 => Self {
                os: ArtifactOs::Macos,
                architecture: Architecture::Arm64,
            },
            Platform::LinuxX64 => Self {
                os: ArtifactOs::Linux,
                architecture: Architecture::X64,
            },
            Platform::LinuxArm64 => Self {
                os: ArtifactOs::Linux,
                architecture: Architecture::Arm64,
            },
        }
    }
}

/// One resolution request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveRequest {
    pub package_id: String,
    pub selector: VersionSelector,
    pub platform: TargetPlatform,
}

/// A concrete artifact resolved from an official provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArtifact {
    pub package_id: String,
    pub version: String,
    /// Official HTTPS download URL (validated by `validate_url`).
    pub url: String,
    /// Official SHA-256 digest, when the provider publishes one.
    pub sha256: Option<String>,
    /// Official size in bytes, when the provider publishes one.
    pub size_bytes: Option<u64>,
    pub installer: InstallerKind,
    /// Path of the executable inside the extracted archive, when known.
    pub executable_relative_path: String,
    /// Subdirectory to strip from the archive, when known.
    pub archive_root: String,
    /// Provenance label (`flutter`, `github`, ...) for the review UI.
    pub source: String,
    /// True when no official checksum was published — verification is then
    /// limited to size/HTTPS and must be reported to the user (spec §6).
    pub verification_limited: bool,
}

#[async_trait::async_trait]
pub trait ArtifactSource: Send + Sync {
    /// Package IDs this provider serves.
    fn package_ids(&self) -> &[&str];

    /// Resolve `request` into a concrete artifact using official metadata.
    async fn resolve(
        &self,
        request: &ResolveRequest,
        http: &dyn SourceHttp,
    ) -> Result<ResolvedArtifact, SourceError>;
}

/// Resolution failures with stable wire codes.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("no provider is registered for package {0}")]
    NoProvider(String),
    #[error("package {0} is not supported by provider {1}")]
    UnsupportedPackage(String, String),
    #[error("unsupported platform {0:?}")]
    UnsupportedPlatform(TargetPlatform),
    #[error("version {0} not found in official metadata")]
    VersionNotFound(String),
    #[error("no official artifact for this platform/version")]
    ArtifactNotFound,
    #[error("official metadata is invalid: {0}")]
    InvalidMetadata(String),
    #[error("metadata HTTP failure: {0}")]
    Http(String),
    #[error("unsafe official URL rejected: {0}")]
    UnsafeUrl(String),
}

impl SourceError {
    pub fn code(&self) -> ErrorCode {
        match self {
            SourceError::NoProvider(_) | SourceError::UnsupportedPackage(_, _) => {
                ErrorCode::PackageNotFound
            }
            SourceError::UnsupportedPlatform(_) => ErrorCode::UnsupportedPlatform,
            SourceError::VersionNotFound(_) | SourceError::ArtifactNotFound => {
                ErrorCode::VersionNotFound
            }
            SourceError::InvalidMetadata(_) | SourceError::UnsafeUrl(_) => {
                ErrorCode::IntegrityCheckFailed
            }
            SourceError::Http(_) => ErrorCode::NetworkError,
        }
    }
}
