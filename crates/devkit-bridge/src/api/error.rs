//! Bridge error model (docs/ffi/bridge-contract.md § Error model).
//!
//! Every operation returns `Result<T, BridgeError>`; FRB raises the
//! mirrored object as a Dart exception. `code` is one of the stable
//! contract codes (Flutter maps codes to copy and must not parse
//! `message`), `message` is safe to display and log, and
//! `diagnostic_id` is an opaque correlation ID.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use devkit_core::error::ErrorCode;
use devkit_jobs::error::JobsError;
use devkit_registry::RegistryError;

/// Stable, machine-readable error categories (the contract's codes).
/// Mirrors `devkit_core::error::ErrorCode`; adding a variant is a
/// contract change (update bridge-contract.md and the Flutter mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BridgeErrorCode {
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

/// Structured bridge error carrying the stable code.
#[derive(Debug, Clone)]
pub struct BridgeError {
    pub code: BridgeErrorCode,
    pub message: String,
    /// Opaque correlation ID for support/triage; never user-identifying.
    pub diagnostic_id: String,
}

/// Opaque correlation ID derived deterministically from (code, message):
/// stable across restarts for the same failure, no user-identifying data.
fn diagnostic_id(code: BridgeErrorCode, message: &str) -> String {
    let mut hasher = DefaultHasher::new();
    code.hash(&mut hasher);
    message.hash(&mut hasher);
    format!("d-{:08x}", hasher.finish() as u32)
}

impl BridgeError {
    #[flutter_rust_bridge::frb(ignore)]
    pub fn code(&self) -> BridgeErrorCode {
        self.code
    }
}

/// Builds a [`BridgeError`]; the `From`/map_err path uses this everywhere.
pub(crate) fn bridge_error(code: BridgeErrorCode, message: impl Into<String>) -> BridgeError {
    let message = message.into();
    BridgeError {
        diagnostic_id: diagnostic_id(code, &message),
        code,
        message,
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{} {}] {}",
            self.code_str(),
            self.diagnostic_id,
            self.message
        )
    }
}

impl std::error::Error for BridgeError {}

impl BridgeErrorCode {
    /// Stable wire form (SCREAMING_SNAKE_CASE per the contract).
    #[flutter_rust_bridge::frb(ignore)]
    pub fn as_str(self) -> &'static str {
        match self {
            BridgeErrorCode::InvalidArgument => "INVALID_ARGUMENT",
            BridgeErrorCode::PackageNotFound => "PACKAGE_NOT_FOUND",
            BridgeErrorCode::VersionNotFound => "VERSION_NOT_FOUND",
            BridgeErrorCode::UnsupportedPlatform => "UNSUPPORTED_PLATFORM",
            BridgeErrorCode::DependencyConflict => "DEPENDENCY_CONFLICT",
            BridgeErrorCode::NetworkError => "NETWORK_ERROR",
            BridgeErrorCode::IntegrityCheckFailed => "INTEGRITY_CHECK_FAILED",
            BridgeErrorCode::InstallFailed => "INSTALL_FAILED",
            BridgeErrorCode::PermissionRequired => "PERMISSION_REQUIRED",
            BridgeErrorCode::Cancelled => "CANCELLED",
            BridgeErrorCode::DatabaseError => "DATABASE_ERROR",
        }
    }
}

/// Stable-code wire string for the Dart mapping (the Dart side maps the
/// mirrored enum, this is for logs and tests).
impl BridgeError {
    #[flutter_rust_bridge::frb(ignore)]
    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

pub(crate) fn from_engine_code(code: ErrorCode, message: impl Into<String>) -> BridgeError {
    let code = match code {
        ErrorCode::InvalidArgument => BridgeErrorCode::InvalidArgument,
        ErrorCode::PackageNotFound => BridgeErrorCode::PackageNotFound,
        ErrorCode::VersionNotFound => BridgeErrorCode::VersionNotFound,
        ErrorCode::UnsupportedPlatform => BridgeErrorCode::UnsupportedPlatform,
        ErrorCode::DependencyConflict => BridgeErrorCode::DependencyConflict,
        ErrorCode::NetworkError => BridgeErrorCode::NetworkError,
        ErrorCode::IntegrityCheckFailed => BridgeErrorCode::IntegrityCheckFailed,
        ErrorCode::InstallFailed => BridgeErrorCode::InstallFailed,
        ErrorCode::PermissionRequired => BridgeErrorCode::PermissionRequired,
        ErrorCode::Cancelled => BridgeErrorCode::Cancelled,
        ErrorCode::DatabaseError => BridgeErrorCode::DatabaseError,
    };
    bridge_error(code, message)
}

/// Registry failures are storage-layer: any unexpected repository error
/// is a `DATABASE_ERROR` (the pipeline maps its own producer codes).
pub(crate) fn from_registry_error(error: RegistryError) -> BridgeError {
    bridge_error(BridgeErrorCode::DatabaseError, error.to_string())
}

pub(crate) fn from_jobs_error(error: JobsError) -> BridgeError {
    bridge_error(BridgeErrorCode::DatabaseError, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_carries_stable_code_and_diagnostic_id() {
        let error = bridge_error(
            BridgeErrorCode::IntegrityCheckFailed,
            "The downloaded file did not match the expected checksum.",
        );
        assert_eq!(error.code_str(), "INTEGRITY_CHECK_FAILED");
        assert!(error.diagnostic_id.starts_with("d-"));
        // Deterministic: same failure -> same correlation ID.
        let again = bridge_error(
            BridgeErrorCode::IntegrityCheckFailed,
            "The downloaded file did not match the expected checksum.",
        );
        assert_eq!(error.diagnostic_id, again.diagnostic_id);
        assert!(!error.message.contains("https://"));
    }

    #[test]
    fn engine_codes_map_one_to_one() {
        assert_eq!(
            from_engine_code(ErrorCode::PermissionRequired, "declined").code_str(),
            "PERMISSION_REQUIRED"
        );
        assert_eq!(
            from_engine_code(ErrorCode::UnsupportedPlatform, "no artifact").code_str(),
            "UNSUPPORTED_PLATFORM"
        );
    }
}
