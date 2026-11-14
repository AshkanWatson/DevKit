//! Offline catalog browsing over the SQLite cache.
//!
//! `sync_fixture_dir` seeds the cache from fixtures (this stands in for the
//! first online load; P2-04 replaces the seed source with the remote API
//! using the same repositories). Every browsing query reads only from the
//! cache — no fixture or network access — which is the P1-04 acceptance
//! criterion "catalog browsing works offline after first load".

use std::path::Path;

use devkit_core::manifest::{PackageManifest, ProfileManifest};
use devkit_registry::catalog_http_cache::CatalogHttpCache;
use devkit_registry::package_cache::PackageCache;
use devkit_registry::profile_cache::ProfileCache;
use devkit_registry::Database;

use crate::error::CatalogError;
use crate::fixtures::FixtureCatalog;

/// The local catalog: SQLite cache plus the fixtures that seed it.
pub struct OfflineCatalog {
    db: Database,
}

/// Counts written by one fixture sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncReport {
    pub packages: usize,
    pub profiles: usize,
}

impl OfflineCatalog {
    /// Open (and migrate) a catalog database at `path`.
    pub fn open(path: &Path) -> Result<Self, CatalogError> {
        Ok(Self {
            db: Database::open(path)?,
        })
    }

    /// In-memory catalog for tests and ephemeral use.
    pub fn open_in_memory() -> Result<Self, CatalogError> {
        Ok(Self {
            db: Database::open_in_memory()?,
        })
    }

    /// Seed/refresh the cache from a fixture directory. Profiles are cached
    /// with `origin = 'catalog'` (docs/architecture/local-database.md §2).
    pub fn sync_fixture_dir(&self, dir: &Path) -> Result<SyncReport, CatalogError> {
        let fixtures = FixtureCatalog::load_dir(dir)?;
        let conn = self.db.connection();
        let package_cache = PackageCache::new(conn);
        for package in &fixtures.packages {
            package_cache.upsert(package)?;
        }
        let profile_cache = ProfileCache::new(conn);
        for profile in &fixtures.profiles {
            profile_cache.upsert(profile, "catalog")?;
        }
        Ok(SyncReport {
            packages: fixtures.packages.len(),
            profiles: fixtures.profiles.len(),
        })
    }

    /// All cached packages, ordered by stable ID (cache-only: offline).
    pub fn list_packages(&self) -> Result<Vec<PackageManifest>, CatalogError> {
        Ok(PackageCache::new(self.db.connection())
            .list()?
            .into_iter()
            .map(|cached| cached.manifest)
            .collect())
    }

    /// One cached package by stable ID (cache-only: offline).
    pub fn get_package(&self, id: &str) -> Result<Option<PackageManifest>, CatalogError> {
        Ok(PackageCache::new(self.db.connection())
            .get(id)?
            .map(|cached| cached.manifest))
    }

    /// All cached profiles, ordered by stable ID (cache-only: offline).
    pub fn list_profiles(&self) -> Result<Vec<ProfileManifest>, CatalogError> {
        Ok(ProfileCache::new(self.db.connection())
            .list()?
            .into_iter()
            .map(|cached| cached.manifest)
            .collect())
    }

    /// One cached profile by stable ID (cache-only: offline).
    pub fn get_profile(&self, id: &str) -> Result<Option<ProfileManifest>, CatalogError> {
        Ok(ProfileCache::new(self.db.connection())
            .get(id)?
            .map(|cached| cached.manifest))
    }

    /// Shared repositories for later phases (job/HTTP-cache integration).
    pub fn package_cache(&self) -> PackageCache<'_> {
        PackageCache::new(self.db.connection())
    }

    pub fn profile_cache(&self) -> ProfileCache<'_> {
        ProfileCache::new(self.db.connection())
    }

    pub fn http_cache(&self) -> CatalogHttpCache<'_> {
        CatalogHttpCache::new(self.db.connection())
    }
}
