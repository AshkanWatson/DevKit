//! Developer profiles (docs/architecture/local-database.md §2, tables
//! `profiles` and `profile_packages`).
//!
//! Catalog profiles are cached with `origin = 'catalog'`; user-created and
//! user-edited profiles use `origin = 'user'` and are the only
//! non-reconstructible local data besides install receipts (§3.2).

use rusqlite::{params, Connection};

use crate::error::RegistryError;

/// A profile manifest as stored, plus its raw JSON and origin.
#[derive(Debug, Clone)]
pub struct CachedProfile {
    pub manifest: devkit_core::manifest::ProfileManifest,
    pub origin: String,
    pub manifest_json: String,
    pub updated_at: String,
}

pub struct ProfileCache<'a> {
    conn: &'a Connection,
}

impl<'a> ProfileCache<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert or replace a profile and its membership rows in one
    /// transaction. Membership rows are rewritten as a set, ordered by the
    /// manifest's package order (`position`).
    pub fn upsert(
        &self,
        manifest: &devkit_core::manifest::ProfileManifest,
        origin: &str,
    ) -> Result<(), RegistryError> {
        let manifest_json = serde_json::to_string(manifest)?;
        let updated_at = crate::error::now_utc_rfc3339()?;

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO profiles (id, name, description, origin, manifest, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                origin = excluded.origin,
                manifest = excluded.manifest,
                updated_at = excluded.updated_at",
            params![
                manifest.id,
                manifest.name,
                manifest.description,
                origin,
                manifest_json,
                updated_at,
            ],
        )?;
        tx.execute(
            "DELETE FROM profile_packages WHERE profile_id = ?1",
            params![manifest.id],
        )?;
        for (position, package) in manifest.packages.iter().enumerate() {
            let policy = crate::error::enum_label(&package.version_policy)?;
            tx.execute(
                "INSERT INTO profile_packages
                    (profile_id, package_id, required, version_policy, version,
                     channel, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    manifest.id,
                    package.package_id,
                    package.required,
                    policy,
                    package.version,
                    package.channel,
                    position as i64,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Load one profile by stable ID.
    pub fn get(&self, id: &str) -> Result<Option<CachedProfile>, RegistryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT manifest, origin, updated_at FROM profiles WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        match rows.next() {
            None => Ok(None),
            Some(row) => {
                let (manifest_json, origin, updated_at) = row?;
                let manifest = serde_json::from_str(&manifest_json)?;
                Ok(Some(CachedProfile {
                    manifest,
                    origin,
                    manifest_json,
                    updated_at,
                }))
            }
        }
    }

    /// All profiles, ordered by stable ID.
    pub fn list(&self) -> Result<Vec<CachedProfile>, RegistryError> {
        self.list_where("1 = 1")
    }

    /// User-created/user-edited profiles only (the non-reconstructible local
    /// data). Ordered by stable ID. Catalog profiles are always re-derived on
    /// sync, so editors must only offer this set for save/delete.
    pub fn list_user(&self) -> Result<Vec<CachedProfile>, RegistryError> {
        self.list_where("origin = 'user'")
    }

    fn list_where(&self, predicate: &str) -> Result<Vec<CachedProfile>, RegistryError> {
        let sql = format!(
            "SELECT manifest, origin, updated_at FROM profiles WHERE {predicate} ORDER BY id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut profiles = Vec::new();
        for row in rows {
            let (manifest_json, origin, updated_at) = row?;
            let manifest = serde_json::from_str(&manifest_json)?;
            profiles.push(CachedProfile {
                manifest,
                origin,
                manifest_json,
                updated_at,
            });
        }
        Ok(profiles)
    }

    /// Remove a profile only when it is user-owned. Returns `false` when the
    /// ID is unknown or is a catalog profile (which is re-derived on sync and
    /// must never be deletable by the editor).
    pub fn remove_user(&self, id: &str) -> Result<bool, RegistryError> {
        let deleted = self.conn.execute(
            "DELETE FROM profiles WHERE id = ?1 AND origin = 'user'",
            params![id],
        )?;
        Ok(deleted > 0)
    }

    /// Remove a profile (membership rows cascade).
    pub fn remove(&self, id: &str) -> Result<(), RegistryError> {
        self.conn
            .execute("DELETE FROM profiles WHERE id = ?1", params![id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devkit_core::manifest::{
        ProfileManifest, ProfilePackage, VersionPolicyName, SUPPORTED_SCHEMA_VERSION,
    };
    use tempfile::TempDir;

    fn profile(id: &str) -> ProfileManifest {
        ProfileManifest {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            id: id.to_owned(),
            name: id.to_owned(),
            description: String::new(),
            maintainer: String::new(),
            supported_platforms: Vec::new(),
            packages: vec![ProfilePackage {
                package_id: "devkit.lang.python".to_owned(),
                required: true,
                version_policy: VersionPolicyName::Stable,
                version: None,
                channel: None,
                reason: String::new(),
            }],
        }
    }

    fn db() -> (TempDir, crate::Database) {
        let dir = TempDir::new().unwrap();
        let database = crate::Database::open(&dir.path().join("state.sqlite")).unwrap();
        (dir, database)
    }

    #[test]
    fn list_user_separates_personal_from_catalog_profiles() {
        let (_dir, database) = db();
        let cache = ProfileCache::new(database.connection());
        cache
            .upsert(&profile("devkit.profile.catalog"), "catalog")
            .unwrap();
        cache
            .upsert(&profile("devkit.profile.personal"), "user")
            .unwrap();
        let user: Vec<_> = cache
            .list_user()
            .unwrap()
            .into_iter()
            .map(|c| c.manifest.id)
            .collect();
        assert_eq!(user, vec!["devkit.profile.personal".to_owned()]);
        assert_eq!(cache.list().unwrap().len(), 2);
    }

    #[test]
    fn remove_user_never_deletes_a_catalog_profile() {
        let (_dir, database) = db();
        let cache = ProfileCache::new(database.connection());
        cache
            .upsert(&profile("devkit.profile.catalog"), "catalog")
            .unwrap();
        cache
            .upsert(&profile("devkit.profile.personal"), "user")
            .unwrap();
        assert!(!cache.remove_user("devkit.profile.catalog").unwrap());
        assert!(cache.get("devkit.profile.catalog").unwrap().is_some());
        assert!(cache.remove_user("devkit.profile.personal").unwrap());
        assert!(cache.get("devkit.profile.personal").unwrap().is_none());
        assert!(!cache.remove_user("devkit.profile.unknown").unwrap());
    }
}
