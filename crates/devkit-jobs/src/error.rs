//! Job error model. The bridge layer maps these to stable wire codes
//! (docs/ffi/bridge-contract.md § Error model); this crate only refines
//! the cause. Job failures themselves carry stable [`devkit_core::ErrorCode`]s
//! in events.

use std::fmt;

#[derive(Debug)]
pub enum JobsError {
    Sql(rusqlite::Error),
    Registry(devkit_registry::RegistryError),
    /// A bug in this crate rather than bad input (e.g. an invalid state
    /// transition the state machine should have prevented).
    Internal(String),
}

impl fmt::Display for JobsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobsError::Sql(e) => write!(f, "sqlite error: {e}"),
            JobsError::Registry(e) => write!(f, "registry error: {e}"),
            JobsError::Internal(message) => write!(f, "jobs bug: {message}"),
        }
    }
}

impl std::error::Error for JobsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JobsError::Sql(e) => Some(e),
            JobsError::Registry(e) => Some(e),
            JobsError::Internal(_) => None,
        }
    }
}

impl From<rusqlite::Error> for JobsError {
    fn from(e: rusqlite::Error) -> Self {
        JobsError::Sql(e)
    }
}

impl From<devkit_registry::RegistryError> for JobsError {
    fn from(e: devkit_registry::RegistryError) -> Self {
        JobsError::Registry(e)
    }
}
