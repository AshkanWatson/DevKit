//! Registration of validated installations and the global active-version
//! mapping (installation-lifecycle.md §4, local-database.md §2,
//! version-management.md §2).
//!
//! One row per installed concrete version (`PRIMARY KEY
//! (package_id, version)`) makes side-by-side installation structural:
//! two versions of one package are two independent rows, two receipts,
//! two directories. `receipt` stores the `InstallReceipt` JSON produced
//! by the P3-03 pipeline; paths are managed-root-relative platform-native
//! strings and are containment-checked on read (§5 "validate containment
//! inside the managed root on read").
//!
//! Activation helpers here are plumbing only: the consent-gated
//! `set_active_version` operation is P3-04. Uninstall clears the active
//! mapping before deleting the row, because `active_versions` references
//! `installed_versions` (baseline DDL) — this matches version-management
//! §2 rule 3 ("uninstalling the active version ... clears the active
//! mapping").

use rusqlite::{params, Connection};

use crate::error::RegistryError;

/// Lifecycle status of an installed version. Wire labels are the CHECK
/// constraint values of `installed_versions.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstalledStatus {
    /// Receipt written, validation not finished yet.
    Installing,
    /// Validated and permanent until explicit uninstall.
    Installed,
    /// Validation found the installation damaged (repair path: uninstall
    /// + reinstall; never silently overwritten, version-management §2.1).
    Broken,
    /// Being removed by an uninstall operation.
    Removing,
}

impl InstalledStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            InstalledStatus::Installing => "installing",
            InstalledStatus::Installed => "installed",
            InstalledStatus::Broken => "broken",
            InstalledStatus::Removing => "removing",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "installing" => Some(InstalledStatus::Installing),
            "installed" => Some(InstalledStatus::Installed),
            "broken" => Some(InstalledStatus::Broken),
            "removing" => Some(InstalledStatus::Removing),
            _ => None,
        }
    }
}

/// One row of `installed_versions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledRecord {
    pub package_id: String,
    pub version: String,
    /// Managed-root-relative directory of the installation
    /// (e.g. `packages/devkit.lang.go/1.25.0`).
    pub install_path: String,
    pub status: InstalledStatus,
    /// `InstallReceipt` JSON (pipeline-produced, local-database.md §2).
    pub receipt_json: String,
    /// UTC ISO-8601.
    pub installed_at: String,
}

pub struct InstalledRepo<'a> {
    conn: &'a Connection,
}

impl<'a> InstalledRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Register a validated installation. A row for the same
    /// (package, version) must not exist: side-by-side versions are
    /// distinct rows, and a duplicate is a pipeline bug (the pipeline
    /// checks for an existing receipt at RESOLVING).
    pub fn register(&self, record: &InstalledRecord) -> Result<(), RegistryError> {
        self.conn.execute(
            "INSERT INTO installed_versions
                (package_id, version, install_path, status, receipt, installed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.package_id,
                record.version,
                record.install_path,
                record.status.as_str(),
                record.receipt_json,
                record.installed_at,
            ],
        )?;
        Ok(())
    }

    /// One receipt row.
    pub fn get(
        &self,
        package_id: &str,
        version: &str,
    ) -> Result<Option<InstalledRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT package_id, version, install_path, status, receipt, installed_at
             FROM installed_versions WHERE package_id = ?1 AND version = ?2",
        )?;
        let mut rows = stmt.query_map(params![package_id, version], |row| {
            let status_text = row.get::<_, String>(3)?;
            Ok(InstalledRecord {
                package_id: row.get(0)?,
                version: row.get(1)?,
                install_path: row.get(2)?,
                status: InstalledStatus::parse(&status_text).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        format!("unknown installed status {status_text}").into(),
                    )
                })?,
                receipt_json: row.get(4)?,
                installed_at: row.get(5)?,
            })
        })?;
        match rows.next() {
            None => Ok(None),
            Some(row) => Ok(Some(row?)),
        }
    }

    /// All receipt rows, ordered by package then version.
    pub fn list(&self) -> Result<Vec<InstalledRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT package_id, version, install_path, status, receipt, installed_at
             FROM installed_versions ORDER BY package_id, version",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(InstalledRecord {
                package_id: row.get(0)?,
                version: row.get(1)?,
                install_path: row.get(2)?,
                status: InstalledStatus::parse(&row.get::<_, String>(3)?)
                    .expect("status is CHECK-constrained"),
                receipt_json: row.get(4)?,
                installed_at: row.get(5)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// All receipt rows of one package (side-by-side view).
    pub fn list_for_package(
        &self,
        package_id: &str,
    ) -> Result<Vec<InstalledRecord>, RegistryError> {
        let all = self.list()?;
        Ok(all
            .into_iter()
            .filter(|record| record.package_id == package_id)
            .collect())
    }

    /// Move one receipt to a new status. Returns whether the row existed.
    pub fn update_status(
        &self,
        package_id: &str,
        version: &str,
        status: InstalledStatus,
    ) -> Result<bool, RegistryError> {
        let updated = self.conn.execute(
            "UPDATE installed_versions SET status = ?3
             WHERE package_id = ?1 AND version = ?2",
            params![package_id, version, status.as_str()],
        )?;
        Ok(updated > 0)
    }

    /// Remove one receipt row (exactly one version — version-management
    /// §2 rule 3). Callers must have removed the `active_versions` mapping
    /// first when that version was active (the FK forbids deleting a
    /// referenced row).
    pub fn delete(&self, package_id: &str, version: &str) -> Result<bool, RegistryError> {
        let deleted = self.conn.execute(
            "DELETE FROM installed_versions WHERE package_id = ?1 AND version = ?2",
            params![package_id, version],
        )?;
        Ok(deleted > 0)
    }

    /// The active version of a package, if one is recorded.
    pub fn active_version(&self, package_id: &str) -> Result<Option<String>, RegistryError> {
        let version = self.conn.query_row(
            "SELECT version FROM active_versions WHERE package_id = ?1",
            params![package_id],
            |row| row.get::<_, String>(0),
        );
        match version {
            Ok(version) => Ok(Some(version)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Record the active version (P3-04 wires consent around this; the
    /// row must reference an installed version — the FK enforces it).
    pub fn set_active(&self, package_id: &str, version: &str) -> Result<(), RegistryError> {
        self.conn.execute(
            "INSERT INTO active_versions (package_id, version, activated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(package_id) DO UPDATE SET
                version = excluded.version,
                activated_at = excluded.activated_at",
            params![package_id, version, crate::error::now_utc_rfc3339()?],
        )?;
        Ok(())
    }

    /// Clear the active mapping if — and only if — it points at
    /// `version`. Returns whether a mapping was cleared.
    pub fn clear_active_if_points_at(
        &self,
        package_id: &str,
        version: &str,
    ) -> Result<bool, RegistryError> {
        let cleared = self.conn.execute(
            "DELETE FROM active_versions WHERE package_id = ?1 AND version = ?2",
            params![package_id, version],
        )?;
        Ok(cleared > 0)
    }
}
