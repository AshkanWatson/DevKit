//! SQLite local state for DevKit: migrations and repositories.
//!
//! Authoritative design: `docs/architecture/local-database.md`.
//! - [`migrations`]: numbered, forward-only migrations (§3).
//! - [`database`]: open/configure/migrate entry point (§1, §3, §4).
//! - [`package_cache`]: cached package/version manifests from the catalog.
//! - [`catalog_http_cache`]: ETag-keyed HTTP response cache for catalog URLs.
//! - [`profile_cache`]: cached and user-editable developer profiles.
//!
//! Tables ship with the feature that uses them; migration v1 carries the
//! full documented baseline so later migrations stay small and reviewable.

pub mod catalog_http_cache;
pub mod database;
pub mod error;
pub mod migrations;
pub mod package_cache;
pub mod profile_cache;

pub use database::Database;
pub use error::RegistryError;
