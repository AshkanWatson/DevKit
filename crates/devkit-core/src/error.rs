//! Stable error codes shared across the Rust core and the FFI boundary.
//!
//! The codes mirror docs/ffi/bridge-contract.md. Flutter maps these codes to
//! user-facing messages; it must never rely on the message text alone.

use std::fmt;

/// Stable, machine-readable error categories.
///
/// Adding a variant is a contract change: update docs/ffi/bridge-contract.md
/// and the Flutter error mapping in the same commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidArgument,
    PackageNotFound,
    VersionNotFound,
    UnsupportedPlatform,
    DependencyConflict,
    NetworkError,
    IntegrityCheckFailed,
    InstallFailed,
    PermissionRequired,
    Cancelled,
    DatabaseError,
}

impl ErrorCode {
    /// Stable wire identifier used by the FFI contract and tests.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidArgument => "INVALID_ARGUMENT",
            ErrorCode::PackageNotFound => "PACKAGE_NOT_FOUND",
            ErrorCode::VersionNotFound => "VERSION_NOT_FOUND",
            ErrorCode::UnsupportedPlatform => "UNSUPPORTED_PLATFORM",
            ErrorCode::DependencyConflict => "DEPENDENCY_CONFLICT",
            ErrorCode::NetworkError => "NETWORK_ERROR",
            ErrorCode::IntegrityCheckFailed => "INTEGRITY_CHECK_FAILED",
            ErrorCode::InstallFailed => "INSTALL_FAILED",
            ErrorCode::PermissionRequired => "PERMISSION_REQUIRED",
            ErrorCode::Cancelled => "CANCELLED",
            ErrorCode::DatabaseError => "DATABASE_ERROR",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A structured DevKit error.
///
/// `message` must be safe to show to users and to write to logs: no secrets,
/// no credentials, no private URL query parameters.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DevKitError {
    pub code: ErrorCode,
    pub message: String,
    /// Opaque diagnostic identifier for support/triage. Never contains
    /// user-identifying data.
    pub diagnostic_id: String,
}

impl DevKitError {
    pub fn new(
        code: ErrorCode,
        message: impl Into<String>,
        diagnostic_id: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            diagnostic_id: diagnostic_id.into(),
        }
    }
}

impl fmt::Display for DevKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{} {}] {}", self.code, self.diagnostic_id, self.message)
    }
}

impl std::error::Error for DevKitError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(ErrorCode::PackageNotFound.as_str(), "PACKAGE_NOT_FOUND");
        assert_eq!(
            ErrorCode::IntegrityCheckFailed.as_str(),
            "INTEGRITY_CHECK_FAILED"
        );
    }

    #[test]
    fn serializes_as_stable_code() {
        let err = DevKitError::new(ErrorCode::NetworkError, "download failed", "diag-1");
        let json = serde_json::to_value(&err).expect("serialize");
        assert_eq!(json["code"], "NETWORK_ERROR");
        assert_eq!(json["message"], "download failed");
        assert_eq!(json["diagnostic_id"], "diag-1");
    }

    #[test]
    fn display_includes_code_and_diagnostic_id() {
        let err = DevKitError::new(ErrorCode::InstallFailed, "adapter failed", "diag-42");
        let text = err.to_string();
        assert!(text.contains("INSTALL_FAILED"));
        assert!(text.contains("diag-42"));
        assert!(text.contains("adapter failed"));
    }
}
