//! Registry error model.
//!
//! Per the architecture contract, failures surface to the UI as stable
//! error codes (docs/ffi/bridge-contract.md § Error model). The bridge
//! layer (devkit-bridge) performs that mapping; this crate only refines
//! the cause for diagnostics.

use std::fmt;

#[derive(Debug)]
pub enum RegistryError {
    Sql(rusqlite::Error),
    Json(serde_json::Error),
    Io(std::io::Error),
    Time(time::error::Format),
    /// A misuse that is a bug in this crate rather than bad input.
    Internal(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryError::Sql(e) => write!(f, "sqlite error: {e}"),
            RegistryError::Json(e) => write!(f, "json error: {e}"),
            RegistryError::Io(e) => write!(f, "filesystem error: {e}"),
            RegistryError::Time(e) => write!(f, "timestamp formatting error: {e}"),
            RegistryError::Internal(message) => write!(f, "registry bug: {message}"),
        }
    }
}

impl std::error::Error for RegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RegistryError::Sql(e) => Some(e),
            RegistryError::Json(e) => Some(e),
            RegistryError::Io(e) => Some(e),
            RegistryError::Time(e) => Some(e),
            RegistryError::Internal(_) => None,
        }
    }
}

impl From<rusqlite::Error> for RegistryError {
    fn from(e: rusqlite::Error) -> Self {
        RegistryError::Sql(e)
    }
}

impl From<serde_json::Error> for RegistryError {
    fn from(e: serde_json::Error) -> Self {
        RegistryError::Json(e)
    }
}

impl From<std::io::Error> for RegistryError {
    fn from(e: std::io::Error) -> Self {
        RegistryError::Io(e)
    }
}

impl From<time::error::Format> for RegistryError {
    fn from(e: time::error::Format) -> Self {
        RegistryError::Time(e)
    }
}

/// Current UTC time as an ISO-8601/RFC 3339 string (schema convention:
/// `applied_at`/`cached_at`/... are "UTC ISO-8601").
pub fn now_utc_rfc3339() -> Result<String, RegistryError> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

/// Serialize an enum value to its manifest label (`stable`, `language`, ...).
/// All manifest enums serialize as JSON strings via `serde(rename_all)`.
pub fn enum_label<T: serde::Serialize>(value: &T) -> Result<String, RegistryError> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(label) => Ok(label),
        other => Err(RegistryError::Internal(format!(
            "expected enum to serialize as a JSON string, got: {other}"
        ))),
    }
}
