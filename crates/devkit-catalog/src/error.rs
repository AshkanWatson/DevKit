//! Catalog error model. The bridge layer maps these to stable wire codes
//! (docs/ffi/bridge-contract.md § Error model); this crate only refines
//! the cause.

use std::fmt;

#[derive(Debug)]
pub enum CatalogError {
    Fixture(std::io::Error),
    Json(serde_json::Error),
    Manifest(devkit_core::manifest::ManifestError),
    Registry(devkit_registry::RegistryError),
    /// The remote catalog API broke the contract or the client is
    /// misconfigured (non-HTTPS base URL, malformed API payload).
    /// Network unavailability is *not* this: it degrades to cache-only
    /// browsing (P2-04), it never fails a sync.
    Remote(String),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogError::Fixture(e) => write!(f, "fixture error: {e}"),
            CatalogError::Json(e) => write!(f, "json error: {e}"),
            CatalogError::Manifest(e) => write!(f, "manifest error: {e}"),
            CatalogError::Registry(e) => write!(f, "registry error: {e}"),
            CatalogError::Remote(e) => write!(f, "remote catalog error: {e}"),
        }
    }
}

impl std::error::Error for CatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CatalogError::Fixture(e) => Some(e),
            CatalogError::Json(e) => Some(e),
            CatalogError::Manifest(e) => Some(e),
            CatalogError::Registry(e) => Some(e),
            CatalogError::Remote(_) => None,
        }
    }
}

impl From<std::io::Error> for CatalogError {
    fn from(e: std::io::Error) -> Self {
        CatalogError::Fixture(e)
    }
}

impl From<serde_json::Error> for CatalogError {
    fn from(e: serde_json::Error) -> Self {
        CatalogError::Json(e)
    }
}

impl From<devkit_core::manifest::ManifestError> for CatalogError {
    fn from(e: devkit_core::manifest::ManifestError) -> Self {
        CatalogError::Manifest(e)
    }
}

impl From<devkit_registry::RegistryError> for CatalogError {
    fn from(e: devkit_registry::RegistryError) -> Self {
        CatalogError::Registry(e)
    }
}
