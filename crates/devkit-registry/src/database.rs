//! Database open/configure/migrate entry point
//! (docs/architecture/local-database.md §1, §3, §4).
//!
//! Startup flow (§3.3): open database → read `schema_migrations` → apply
//! pending migrations in order → verify `PRAGMA foreign_keys = ON` and
//! `journal_mode = WAL`.

use std::fs;
use std::path::Path;

use rusqlite::Connection;

use crate::error::RegistryError;
use crate::migrations;

/// An opened, migrated local database.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (creating parent directories as needed) and migrate a database
    /// file at `path`. The path convention (`<data-root>/state/devkit.sqlite`)
    /// is enforced by the caller; this crate never hard-codes paths.
    pub fn open(path: &Path) -> Result<Self, RegistryError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        Self::initialize(conn, true)
    }

    /// In-memory database for tests and ephemeral use. WAL does not apply
    /// to in-memory databases; `foreign_keys` is still enforced.
    pub fn open_in_memory() -> Result<Self, RegistryError> {
        Self::initialize(Connection::open_in_memory()?, false)
    }

    fn initialize(conn: Connection, verify_wal: bool) -> Result<Self, RegistryError> {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        Self::verify_pragmas(&conn, verify_wal)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn verify_pragmas(conn: &Connection, verify_wal: bool) -> Result<(), RegistryError> {
        let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        assert_eq!(foreign_keys, 1, "foreign_keys must be enforced");
        if verify_wal {
            let journal_mode: String =
                conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
            assert_eq!(
                journal_mode.to_ascii_lowercase(),
                "wal",
                "journal_mode must be WAL for file databases"
            );
        }
        Ok(())
    }

    /// Apply any pending migrations (forward-only, idempotent).
    pub fn migrate(&self) -> Result<(), RegistryError> {
        migrations::apply(&self.conn, migrations::MIGRATIONS)?;
        Ok(())
    }

    /// Applied migration versions, ascending.
    pub fn applied_migrations(&self) -> Result<Vec<i64>, RegistryError> {
        migrations::applied_versions(&self.conn)
    }

    /// Borrowed connection for repositories.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}
