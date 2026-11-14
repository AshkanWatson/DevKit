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
        let mut stmt = self
            .conn
            .prepare("SELECT manifest, origin, updated_at FROM profiles ORDER BY id")?;
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

    /// Remove a profile (membership rows cascade).
    pub fn remove(&self, id: &str) -> Result<(), RegistryError> {
        self.conn
            .execute("DELETE FROM profiles WHERE id = ?1", params![id])?;
        Ok(())
    }
}
