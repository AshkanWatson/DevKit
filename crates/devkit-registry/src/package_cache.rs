//! Cached package identity and metadata
//! (docs/architecture/local-database.md §2, tables `packages` and
//! `package_versions`).
//!
//! Writes are transactional: the package row and all of its version rows
//! are updated together so the cache never shows a package whose version
//! list disagrees with the stored manifest (§4).

use rusqlite::{params, Connection};

use crate::error::RegistryError;

/// A package manifest as stored in the cache, plus its raw JSON.
#[derive(Debug, Clone)]
pub struct CachedPackage {
    pub manifest: devkit_core::manifest::PackageManifest,
    pub manifest_json: String,
    pub cached_at: String,
}

/// One cached release of a package (row of `package_versions`).
#[derive(Debug, Clone)]
pub struct CachedVersion {
    pub package_id: String,
    pub version: String,
    pub channel: String,
    pub release_date: Option<String>,
    pub is_prerelease: bool,
    pub manifest_json: String,
    pub cached_at: String,
}

pub struct PackageCache<'a> {
    conn: &'a Connection,
}

impl<'a> PackageCache<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert or replace a package and its version rows in one transaction.
    /// `package_versions` rows are rewritten as a set so removed versions
    /// disappear from the cache with the manifest update.
    pub fn upsert(
        &self,
        manifest: &devkit_core::manifest::PackageManifest,
    ) -> Result<(), RegistryError> {
        let manifest_json = serde_json::to_string(manifest)?;
        let cached_at = crate::error::now_utc_rfc3339()?;
        let category = crate::error::enum_label(&manifest.category)?;
        let policy = crate::error::enum_label(&manifest.default_version_policy)?;

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO packages
                (id, name, description, publisher, homepage, category,
                 default_version_policy, manifest, schema_version, updated_at, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                publisher = excluded.publisher,
                homepage = excluded.homepage,
                category = excluded.category,
                default_version_policy = excluded.default_version_policy,
                manifest = excluded.manifest,
                schema_version = excluded.schema_version,
                updated_at = excluded.updated_at,
                cached_at = excluded.cached_at",
            params![
                manifest.id,
                manifest.name,
                manifest.description,
                manifest.publisher,
                manifest.homepage,
                category,
                policy,
                manifest_json,
                manifest.schema_version,
                manifest.updated_at,
                cached_at,
            ],
        )?;
        // Rewrite version rows as a set inside the same transaction.
        tx.execute(
            "DELETE FROM package_versions WHERE package_id = ?1",
            params![manifest.id],
        )?;
        for version in &manifest.versions {
            let channel = crate::error::enum_label(&version.channel)?;
            let version_json = serde_json::to_string(version)?;
            // Channels other than `stable` are prerelease-style
            // (crates/devkit-core/src/version.rs `Release::prerelease`).
            let is_prerelease = version.channel != devkit_core::manifest::Channel::Stable;
            tx.execute(
                "INSERT INTO package_versions
                    (package_id, version, channel, release_date, is_prerelease,
                     manifest_json, cached_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    manifest.id,
                    version.version,
                    channel,
                    version.release_date,
                    is_prerelease,
                    version_json,
                    cached_at,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Load one cached package by stable ID.
    pub fn get(&self, id: &str) -> Result<Option<CachedPackage>, RegistryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT manifest, cached_at FROM packages WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        match rows.next() {
            None => Ok(None),
            Some(row) => {
                let (manifest_json, cached_at) = row?;
                let manifest = serde_json::from_str(&manifest_json)?;
                Ok(Some(CachedPackage {
                    manifest,
                    manifest_json,
                    cached_at,
                }))
            }
        }
    }

    /// All cached packages, ordered by stable ID.
    pub fn list(&self) -> Result<Vec<CachedPackage>, RegistryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT manifest, cached_at FROM packages ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut packages = Vec::new();
        for row in rows {
            let (manifest_json, cached_at) = row?;
            let manifest = serde_json::from_str(&manifest_json)?;
            packages.push(CachedPackage {
                manifest,
                manifest_json,
                cached_at,
            });
        }
        Ok(packages)
    }

    /// Cached version rows for one package, ordered by version.
    pub fn versions(&self, package_id: &str) -> Result<Vec<CachedVersion>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT package_id, version, channel, release_date, is_prerelease,
                    manifest_json, cached_at
             FROM package_versions WHERE package_id = ?1 ORDER BY version",
        )?;
        let rows = stmt.query_map(params![package_id], |row| {
            Ok(CachedVersion {
                package_id: row.get(0)?,
                version: row.get(1)?,
                channel: row.get(2)?,
                release_date: row.get(3)?,
                is_prerelease: row.get::<_, i64>(4)? != 0,
                manifest_json: row.get(5)?,
                cached_at: row.get(6)?,
            })
        })?;
        let mut versions = Vec::new();
        for row in rows {
            versions.push(row?);
        }
        Ok(versions)
    }

    /// Remove a package (version rows and cache entries cascade).
    pub fn remove(&self, id: &str) -> Result<(), RegistryError> {
        self.conn
            .execute("DELETE FROM packages WHERE id = ?1", params![id])?;
        Ok(())
    }
}
