//! Artifact validation before resolution is accepted (ADR 0009, spec §6).

use devkit_download::http::validate_url;

use crate::{ResolvedArtifact, SourceError};

/// Validates a resolved artifact's URL and digest shape. Returns a notice
/// when verification is limited (no official checksum published) so callers
/// can report it to the user instead of silently pretending full verification.
pub fn validate_artifact(artifact: &ResolvedArtifact) -> Result<Option<String>, SourceError> {
    validate_url(&artifact.url).map_err(|error| SourceError::UnsafeUrl(error.to_string()))?;

    if let Some(digest) = &artifact.sha256 {
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(SourceError::InvalidMetadata(format!(
                "official checksum for {} is not a 64-hex digest",
                artifact.version
            )));
        }
        Ok(None)
    } else {
        Ok(Some(format!(
            "no official checksum published for {} {}; verification is limited to HTTPS + size",
            artifact.package_id, artifact.version
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devkit_core::manifest::InstallerKind;

    fn artifact(url: &str, sha256: Option<&str>) -> ResolvedArtifact {
        ResolvedArtifact {
            package_id: "devkit.framework.flutter".to_owned(),
            version: "3.50.1".to_owned(),
            url: url.to_owned(),
            sha256: sha256.map(str::to_owned),
            size_bytes: None,
            installer: InstallerKind::Archive,
            executable_relative_path: String::new(),
            archive_root: String::new(),
            source: "flutter".to_owned(),
            verification_limited: sha256.is_none(),
        }
    }

    #[test]
    fn valid_official_url_and_digest_pass() {
        assert!(validate_artifact(&artifact(
            "https://storage.googleapis.com/flutter_infra_release/releases/stable/windows/flutter_windows_3.50.1-stable.zip",
            Some("33c1e3d4dd08f1136ddb9c7976cfd2b0ab20296f1565b78ba176a4cbb4e140eb")
        ))
        .unwrap()
        .is_none());
    }

    #[test]
    fn http_url_is_rejected() {
        assert!(validate_artifact(&artifact(
            "http://storage.googleapis.com/flutter.zip",
            Some("33c1e3d4dd08f1136ddb9c7976cfd2b0ab20296f1565b78ba176a4cbb4e140eb")
        ))
        .is_err());
    }

    #[test]
    fn missing_checksum_returns_a_limited_verification_notice() {
        let notice = validate_artifact(&artifact(
            "https://storage.googleapis.com/flutter.zip",
            None,
        ))
        .unwrap();
        assert!(notice.unwrap().contains("verification is limited"));
    }

    #[test]
    fn malformed_checksum_is_rejected() {
        assert!(validate_artifact(&artifact(
            "https://storage.googleapis.com/flutter.zip",
            Some("not-a-digest")
        ))
        .is_err());
    }
}
