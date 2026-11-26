//! Manifest models and validation (P1-03).
//!
//! The structs mirror `schemas/package.schema.json` and
//! `schemas/profile.schema.json`. `deny_unknown_fields` enforces the
//! schemas' `additionalProperties: false`; enum-typed fields enforce the
//! schemas' `enum` values. Semantic rules that the schemas express as
//! descriptions or constraints (HTTPS-only artifact URLs, checksum format,
//! unique versions/package IDs, policy-specific required fields) are
//! enforced by [`validate`](PackageManifest::validate).
//!
//! Security notes (docs/security/security-model.md §2/§4):
//! - Artifact URLs must be HTTPS. Plain HTTP or other schemes are rejected
//!   before any download decision can be made.
//! - No network or filesystem access lives here: this module only parses and
//!   validates documents. (Unit tests read committed fixture files, which is
//!   test-only.)

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;

/// Schema version accepted by this loader (schemas pin `const 1`).
pub const SUPPORTED_SCHEMA_VERSION: u16 = 1;

// ---------------------------------------------------------------------------
// Shared enums
// ---------------------------------------------------------------------------

/// Six supported platform/architecture combinations (ADR 0004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    #[serde(rename = "windows-x64")]
    WindowsX64,
    #[serde(rename = "windows-arm64")]
    WindowsArm64,
    #[serde(rename = "macos-x64")]
    MacosX64,
    #[serde(rename = "macos-arm64")]
    MacosArm64,
    #[serde(rename = "linux-x64")]
    LinuxX64,
    #[serde(rename = "linux-arm64")]
    LinuxArm64,
}

/// Package categories (schemas/package.schema.json `category`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Language,
    Framework,
    Sdk,
    Ide,
    #[serde(rename = "build_tool")]
    BuildTool,
    Database,
    Utility,
    #[serde(rename = "ai_tool")]
    AiTool,
    Other,
}

/// Manifest-level version policy name (schema enum). The runtime selection
/// semantics live in [`crate::version::VersionPolicy`]; this is the
/// declarative name a manifest carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionPolicyName {
    Stable,
    Latest,
    Exact,
    Channel,
}

/// Release channel of a version entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Stable,
    Beta,
    Dev,
    Nightly,
}

/// Target OS of an artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactOs {
    Windows,
    Macos,
    Linux,
}

/// Target CPU architecture of an artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X64,
    Arm64,
}

/// How the artifact is installed on the target machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallerKind {
    Archive,
    Exe,
    Msi,
    Dmg,
    Pkg,
    Appimage,
    #[serde(rename = "external_package_manager")]
    ExternalPackageManager,
}

// ---------------------------------------------------------------------------
// Package manifest
// ---------------------------------------------------------------------------

/// A published package manifest (`schemas/package.schema.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    pub description: String,
    pub publisher: String,
    pub homepage: String,
    pub category: Category,
    pub supported_platforms: Vec<Platform>,
    pub default_version_policy: VersionPolicyName,
    pub versions: Vec<PackageVersion>,
    pub license_expression: String,
    pub maintainer: String,
    pub updated_at: String,
}

/// One release of a package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageVersion {
    pub version: String,
    pub channel: Channel,
    /// `format: date` (YYYY-MM-DD). Treated opaquely here; publication-time
    /// schema checks own the format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,
    pub artifacts: Vec<Artifact>,
}

/// A declared dependency of a version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub package_id: String,
    pub required: bool,
    pub version_policy: VersionPolicyName,
    /// Required when `version_policy` is `Exact` (schema description).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Required when `version_policy` is `Channel` (schema description).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

/// A platform-specific downloadable artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub os: ArtifactOs,
    pub architecture: Architecture,
    pub installer: InstallerKind,
    /// Must be HTTPS (security model §2/§4); enforced by validation.
    pub url: String,
    /// 64 hex characters, upper or lower case.
    pub sha256: String,
    /// At least 1 (schema `minimum`).
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_url: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub publisher_identity: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub archive_root: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub executable_relative_path: String,
}

impl PackageManifest {
    /// Parse and validate a package manifest document.
    pub fn from_json(json: &str) -> Result<Self, ManifestError> {
        let manifest: Self =
            serde_json::from_str(json).map_err(|e| ManifestError::Schema(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Semantic validation beyond what serde enforces.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchemaVersion(self.schema_version));
        }
        validate_stable_id(&self.id)?;
        if self.name.is_empty() || self.publisher.is_empty() {
            return Err(ManifestError::MissingRequiredField);
        }
        validate_unique_platforms(&self.supported_platforms)?;
        if self.versions.is_empty() {
            return Err(ManifestError::EmptyVersions);
        }

        let mut seen_versions = Vec::new();
        for version in &self.versions {
            if seen_versions.contains(&version.version) {
                return Err(ManifestError::DuplicateVersion(version.version.clone()));
            }
            seen_versions.push(version.version.clone());
            if version.artifacts.is_empty() {
                return Err(ManifestError::EmptyArtifacts(version.version.clone()));
            }
            for artifact in &version.artifacts {
                validate_artifact(artifact)?;
            }
            for dependency in &version.dependencies {
                validate_stable_id(&dependency.package_id)?;
                match (
                    dependency.version_policy,
                    &dependency.version,
                    &dependency.channel,
                ) {
                    (VersionPolicyName::Exact, None, _) => {
                        return Err(ManifestError::PolicyMismatch {
                            package_id: dependency.package_id.clone(),
                            reason: "exact policy requires a version".to_string(),
                        });
                    }
                    (VersionPolicyName::Channel, _, None) => {
                        return Err(ManifestError::PolicyMismatch {
                            package_id: dependency.package_id.clone(),
                            reason: "channel policy requires a channel".to_string(),
                        });
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

fn validate_artifact(artifact: &Artifact) -> Result<(), ManifestError> {
    if !artifact.url.starts_with("https://") {
        return Err(ManifestError::InsecureUrl(artifact.url.clone()));
    }
    // Host part must not be empty: "https://" alone is not a URL.
    let host = artifact.url["https://".len()..]
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    if host.is_empty() {
        return Err(ManifestError::InsecureUrl(artifact.url.clone()));
    }
    validate_sha256(&artifact.sha256)?;
    if artifact.size_bytes == 0 {
        return Err(ManifestError::NonPositiveSize);
    }
    Ok(())
}

/// Stable package/profile ID pattern: `^[a-z0-9]+(\.[a-z0-9-]+)+$`.
/// Hand-rolled so the validator needs no regex dependency.
fn validate_stable_id(id: &str) -> Result<(), ManifestError> {
    let valid_segment = |s: &str, allow_hyphen: bool| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || (allow_hyphen && c == '-'))
    };
    let mut segments = id.split('.');
    let first = segments.next().unwrap_or("");
    let rest: Vec<_> = segments.collect();
    if first.is_empty() || rest.is_empty() || !valid_segment(first, false) {
        return Err(ManifestError::InvalidId(id.to_string()));
    }
    for segment in rest {
        if !valid_segment(segment, true) {
            return Err(ManifestError::InvalidId(id.to_string()));
        }
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), ManifestError> {
    if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ManifestError::BadChecksum(value.to_string()))
    }
}

fn validate_unique_platforms(platforms: &[Platform]) -> Result<(), ManifestError> {
    for (i, platform) in platforms.iter().enumerate() {
        if platforms[..i].contains(platform) {
            return Err(ManifestError::DuplicatePlatform(format!("{platform:?}")));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Profile manifest
// ---------------------------------------------------------------------------

/// A Developer Profile manifest (`schemas/profile.schema.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileManifest {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub maintainer: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_platforms: Vec<Platform>,
    pub packages: Vec<ProfilePackage>,
}

/// A package selection inside a profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfilePackage {
    pub package_id: String,
    pub required: bool,
    pub version_policy: VersionPolicyName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

impl ProfileManifest {
    /// Parse and validate a profile manifest document.
    pub fn from_json(json: &str) -> Result<Self, ManifestError> {
        let profile: Self =
            serde_json::from_str(json).map_err(|e| ManifestError::Schema(e.to_string()))?;
        profile.validate()?;
        Ok(profile)
    }

    /// Semantic validation beyond what serde enforces.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchemaVersion(self.schema_version));
        }
        validate_stable_id(&self.id)?;
        if self.name.is_empty() {
            return Err(ManifestError::MissingRequiredField);
        }
        validate_unique_platforms(&self.supported_platforms)?;
        if self.packages.is_empty() {
            return Err(ManifestError::EmptyProfilePackages);
        }
        let mut seen_ids = Vec::new();
        for package in &self.packages {
            validate_stable_id(&package.package_id)?;
            if seen_ids.contains(&package.package_id) {
                return Err(ManifestError::DuplicateProfilePackage(
                    package.package_id.clone(),
                ));
            }
            seen_ids.push(package.package_id.clone());
            match (package.version_policy, &package.version, &package.channel) {
                (VersionPolicyName::Exact, None, _) => {
                    return Err(ManifestError::PolicyMismatch {
                        package_id: package.package_id.clone(),
                        reason: "exact policy requires a version".to_string(),
                    });
                }
                (VersionPolicyName::Channel, _, None) => {
                    return Err(ManifestError::PolicyMismatch {
                        package_id: package.package_id.clone(),
                        reason: "channel policy requires a channel".to_string(),
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Error model
// ---------------------------------------------------------------------------

/// Manifest validation failure. Always maps to the stable
/// [`ErrorCode::InvalidArgument`] wire code (docs/ffi/bridge-contract.md);
/// the variant only refines the message for UI/diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub enum ManifestError {
    /// The document does not conform to the JSON schema (unknown field,
    /// wrong enum value, missing required field, bad JSON, ...). Holds the
    /// serde error message (`serde_json::Error` itself is neither `Clone`
    /// nor `PartialEq`).
    Schema(String),
    UnsupportedSchemaVersion(u16),
    InvalidId(String),
    MissingRequiredField,
    EmptyVersions,
    EmptyArtifacts(String),
    EmptyProfilePackages,
    DuplicateVersion(String),
    DuplicateProfilePackage(String),
    DuplicatePlatform(String),
    InsecureUrl(String),
    BadChecksum(String),
    NonPositiveSize,
    PolicyMismatch {
        package_id: String,
        reason: String,
    },
}

impl ManifestError {
    /// Stable wire code for this failure (docs/ffi/bridge-contract.md §
    /// Error model).
    pub fn code(&self) -> ErrorCode {
        ErrorCode::InvalidArgument
    }
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Schema(message) => {
                write!(f, "manifest does not match its schema: {message}")
            }
            ManifestError::UnsupportedSchemaVersion(v) => {
                write!(f, "unsupported manifest schema_version {v}")
            }
            ManifestError::InvalidId(id) => write!(f, "invalid stable ID '{id}'"),
            ManifestError::MissingRequiredField => {
                write!(f, "manifest is missing required metadata")
            }
            ManifestError::EmptyVersions => write!(f, "package manifest has no versions"),
            ManifestError::EmptyArtifacts(version) => {
                write!(f, "version {version} has no artifacts")
            }
            ManifestError::EmptyProfilePackages => write!(f, "profile has no package selections"),
            ManifestError::DuplicateVersion(v) => write!(f, "duplicate version {v}"),
            ManifestError::DuplicateProfilePackage(id) => {
                write!(f, "profile lists package {id} more than once")
            }
            ManifestError::DuplicatePlatform(p) => write!(f, "duplicate supported platform {p}"),
            ManifestError::InsecureUrl(url) => {
                write!(f, "artifact URL must use HTTPS: '{url}'")
            }
            ManifestError::BadChecksum(value) => {
                write!(f, "sha256 must be 64 hex characters: '{}'", value)
            }
            ManifestError::NonPositiveSize => write!(f, "artifact size_bytes must be positive"),
            ManifestError::PolicyMismatch { package_id, reason } => {
                write!(f, "invalid version policy for {package_id}: {reason}")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Path to the committed schema examples, via CARGO_MANIFEST_DIR so the
    /// tests run from any working directory (cargo test, IDE, CI).
    fn examples_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/examples")
    }

    fn read(path: &std::path::Path) -> String {
        std::fs::read_to_string(path).expect("fixture file readable")
    }

    #[test]
    fn all_positive_examples_parse_and_validate() {
        let dir = examples_dir();
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("examples dir exists") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let json = read(&path);
            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            if file_name.contains("package") {
                PackageManifest::from_json(&json)
                    .unwrap_or_else(|e| panic!("{file_name} must parse: {e}"));
            } else if file_name.contains("project") {
                crate::project::ProjectManifest::from_json(&json)
                    .unwrap_or_else(|e| panic!("{file_name} must parse: {e}"));
            } else {
                ProfileManifest::from_json(&json)
                    .unwrap_or_else(|e| panic!("{file_name} must parse: {e}"));
            }
            checked += 1;
        }
        assert!(checked >= 5, "expected the committed examples");
    }

    #[test]
    fn all_invalid_fixtures_are_rejected() {
        let dir = examples_dir().join("invalid");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("invalid dir exists") {
            let path = entry.expect("dir entry").path();
            let json = read(&path);
            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let result = if file_name.contains("package") {
                PackageManifest::from_json(&json).map(|_| ())
            } else if file_name.contains("project") {
                crate::project::ProjectManifest::from_json(&json)
                    .map(|_| ())
                    .map_err(|e| ManifestError::Schema(e.to_string()))
            } else {
                ProfileManifest::from_json(&json).map(|_| ())
            };
            assert!(result.is_err(), "{file_name} must be rejected");
            assert_eq!(
                result.unwrap_err().code(),
                ErrorCode::InvalidArgument,
                "{file_name} must map to a stable error code"
            );
            checked += 1;
        }
        assert!(checked >= 3, "expected the committed invalid fixtures");
    }

    fn valid_package_json() -> String {
        read(&examples_dir().join("python-package.example.json"))
    }

    fn valid_profile_json() -> String {
        read(&examples_dir().join("python-developer-profile.example.json"))
    }

    #[test]
    fn duplicate_versions_are_rejected() {
        let mut manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
        let mut duplicate = manifest.versions[0].clone();
        duplicate.version = "3.12.7".to_string();
        duplicate.channel = Channel::Beta;
        duplicate.artifacts[0].url = "https://example.invalid/other.exe".to_string();
        manifest.versions.push(duplicate);
        assert_eq!(
            manifest.validate().unwrap_err(),
            ManifestError::DuplicateVersion("3.12.7".to_string())
        );
    }

    #[test]
    fn plain_http_artifact_url_is_rejected() {
        let mut manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
        manifest.versions[0].artifacts[0].url = "http://example.invalid/python.exe".to_string();
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::InsecureUrl(_))
        ));
    }

    #[test]
    fn non_hex_or_short_checksum_is_rejected() {
        for bad in ["deadbeef", "", &"g".repeat(64)] {
            let mut manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
            manifest.versions[0].artifacts[0].sha256 = bad.to_string();
            assert!(matches!(
                manifest.validate(),
                Err(ManifestError::BadChecksum(_))
            ));
        }
    }

    #[test]
    fn uppercase_checksum_is_accepted() {
        let mut manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
        manifest.versions[0].artifacts[0].sha256 = "A".repeat(64);
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn zero_size_bytes_is_rejected() {
        let mut manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
        manifest.versions[0].artifacts[0].size_bytes = 0;
        assert_eq!(
            manifest.validate().unwrap_err(),
            ManifestError::NonPositiveSize
        );
    }

    #[test]
    fn empty_artifact_list_is_rejected() {
        let mut manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
        manifest.versions[0].artifacts.clear();
        assert_eq!(
            manifest.validate().unwrap_err(),
            ManifestError::EmptyArtifacts("3.12.7".to_string())
        );
    }

    #[test]
    fn wrong_schema_version_is_rejected() {
        let mut manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
        manifest.schema_version = 2;
        assert_eq!(
            manifest.validate().unwrap_err(),
            ManifestError::UnsupportedSchemaVersion(2)
        );
    }

    #[test]
    fn invalid_stable_ids_are_rejected() {
        // Note: the schema pattern allows trailing hyphens inside segments
        // (`[a-z0-9-]+`), so "devkit.lang.123-456-" is VALID and must parse.
        for bad in ["python", "devkit..python", "Devkit.lang", "devkit.lang."] {
            assert!(validate_stable_id(bad).is_err(), "'{bad}' must be rejected");
        }
        for good in [
            "devkit.lang.python",
            "a.b",
            "devkit.lang.python-3",
            "devkit.lang.123-456-",
        ] {
            assert!(
                validate_stable_id(good).is_ok(),
                "'{good}' must be accepted"
            );
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut json = valid_package_json();
        // Inject an unknown top-level field before the first value ends.
        let injected = json.replace(
            "\"schema_version\": 1,",
            "\"schema_version\": 1, \"vendor_extra\": true,",
        );
        assert!(injected != json, "injection point must exist");
        json = injected;
        assert!(PackageManifest::from_json(&json).is_err());
    }

    #[test]
    fn exact_dependency_without_version_is_rejected() {
        let mut manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
        manifest.versions[0].dependencies.push(Dependency {
            package_id: "devkit.util.tool".to_string(),
            required: true,
            version_policy: VersionPolicyName::Exact,
            version: None,
            channel: None,
        });
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::PolicyMismatch { .. })
        ));
    }

    #[test]
    fn duplicate_profile_package_ids_are_rejected() {
        let mut profile = ProfileManifest::from_json(&valid_profile_json()).unwrap();
        let clone = profile.packages[0].clone();
        profile.packages.push(clone);
        assert_eq!(
            profile.validate().unwrap_err(),
            ManifestError::DuplicateProfilePackage(profile.packages[0].package_id.clone())
        );
    }

    #[test]
    fn profile_exact_without_version_is_rejected() {
        let mut profile = ProfileManifest::from_json(&valid_profile_json()).unwrap();
        profile.packages[0].version = None;
        assert!(matches!(
            profile.validate(),
            Err(ManifestError::PolicyMismatch { .. })
        ));
    }

    #[test]
    fn duplicate_platforms_are_rejected() {
        let mut manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
        manifest.supported_platforms.push(Platform::WindowsX64);
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::DuplicatePlatform(_))
        ));
    }

    // Property-style tests (dependency-free): deterministic pseudo-random
    // LCG drives round-trip and corruption invariants. Not a full property
    // framework, but covers systematic per-field corruption.

    #[test]
    fn round_trip_preserves_manifests() {
        let manifest = PackageManifest::from_json(&valid_package_json()).unwrap();
        let json = serde_json::to_string(&manifest).expect("serialize");
        let parsed = PackageManifest::from_json(&json).expect("reparse");
        assert_eq!(parsed, manifest);

        let profile = ProfileManifest::from_json(&valid_profile_json()).unwrap();
        let json = serde_json::to_string(&profile).expect("serialize");
        let parsed = ProfileManifest::from_json(&json).expect("reparse");
        assert_eq!(parsed, profile);
    }

    #[test]
    fn corrupting_the_document_never_panics() {
        // Byte-level fuzz: deterministic LCG mutates the example JSON at
        // pseudo-random offsets (replace/insert/remove a byte). The property
        // under test is robustness: parsing a corrupted document must never
        // panic. Semantic rejection of malformed documents is covered by the
        // targeted unit tests above (a byte mutation inside a string value,
        // for example, can legitimately produce a valid-but-different
        // document, so "Ok implies unchanged" is not a sound invariant here).
        let original = valid_package_json();
        let mut state = 0x12345678u32;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let baseline = PackageManifest::from_json(&original).unwrap();
        let mut parsed_ok = 0;
        for _ in 0..200 {
            let mut bytes = original.clone().into_bytes();
            let pos = (next() as usize) % bytes.len();
            let action = next() % 3;
            match action {
                0 => bytes[pos] = b'!',
                1 => bytes.insert(pos, b'"'),
                _ => {
                    bytes.remove(pos);
                }
            }
            let corrupted = String::from_utf8_lossy(&bytes).into_owned();
            if let Ok(parsed) = PackageManifest::from_json(&corrupted) {
                parsed_ok += 1;
                // A structurally intact corruption must still be a coherent
                // manifest object; it may differ from the baseline only in
                // string contents (same invariants hold).
                assert_eq!(parsed.schema_version, baseline.schema_version);
            }
        }
        // Sanity: the fuzzer must actually be able to produce valid parses,
        // otherwise it is not exercising the happy path at all.
        assert!(parsed_ok > 0, "fuzzer produced no parseable documents");
    }
}
