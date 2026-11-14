//! Numbered, forward-only migrations (docs/architecture/local-database.md §3).
//!
//! - Each migration is an ordered `(version, name, SQL)` applied inside a
//!   single transaction and recorded in `schema_migrations`.
//! - No down-migrations.
//! - Applying an already-applied migration list is a clean no-op
//!   (idempotence rule, §3.5).

use rusqlite::{params, Connection};

use crate::error::RegistryError;

/// One forward-only schema migration.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

/// All migrations, in apply order. New tables ship with the feature that
/// uses them (§3.4); append a new entry rather than editing existing SQL.
///
/// Invariant: the version-1 migration creates `schema_migrations` itself
/// (the documented baseline DDL includes it), and every later migration
/// runs against a database where the table already exists. `apply` records
/// each applied version in that table after the migration SQL succeeds.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "baseline_v1",
    sql: include_str!("migrations/0001_baseline_v1.sql"),
}];

/// Apply `migrations` to `conn`, in order, skipping already-applied
/// versions. Returns the recorded migration versions after applying.
pub fn apply(conn: &Connection, migrations: &[Migration]) -> Result<Vec<i64>, RegistryError> {
    // The list must be strictly ascending so that "applied up to MAX(version)"
    // is a sound progress marker.
    for pair in migrations.windows(2) {
        if pair[0].version >= pair[1].version {
            return Err(RegistryError::Internal(format!(
                "migrations are not strictly ascending: {} then {}",
                pair[0].version, pair[1].version
            )));
        }
    }

    let current = current_version(conn)?;
    for migration in migrations {
        if migration.version <= current {
            continue;
        }
        let tx = conn.unchecked_transaction()?;
        let result = (|| -> Result<(), RegistryError> {
            tx.execute_batch(migration.sql)?;
            let applied_at = crate::error::now_utc_rfc3339()?;
            tx.execute(
                "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                params![migration.version, migration.name, applied_at],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => tx.commit()?,
            Err(e) => return Err(e), // Transaction rolls back on drop.
        }
    }
    applied_versions(conn)
}

/// Versions recorded in `schema_migrations`, ascending.
pub fn applied_versions(conn: &Connection) -> Result<Vec<i64>, RegistryError> {
    if !table_exists(conn, "schema_migrations")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut versions = Vec::new();
    for row in rows {
        versions.push(row?);
    }
    Ok(versions)
}

/// Highest applied version, or 0 when the database is fresh. Fresh
/// databases have no `schema_migrations` table yet (it is created by
/// migration 1, whose DDL is the documented baseline).
fn current_version(conn: &Connection) -> Result<i64, RegistryError> {
    if !table_exists(conn, "schema_migrations")? {
        return Ok(0);
    }
    let version = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, RegistryError> {
    let exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        params![name],
        |row| row.get(0),
    )?;
    Ok(exists)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_migrations_are_strictly_ascending() {
        let pairs = MIGRATIONS.windows(2);
        for pair in pairs {
            assert!(pair[0].version < pair[1].version);
        }
        assert!(!MIGRATIONS.is_empty());
        assert_eq!(MIGRATIONS[0].version, 1, "baseline must be version 1");
    }

    #[test]
    fn baseline_sql_matches_documented_tables() {
        // Every table the authoritative doc defines must be created by the
        // baseline migration SQL (guards against doc/code drift).
        let documented = [
            "schema_migrations",
            "packages",
            "package_versions",
            "installed_versions",
            "active_versions",
            "installation_jobs",
            "installation_events",
            "catalog_cache",
            "profiles",
            "profile_packages",
            "settings",
        ];
        for table in documented {
            assert!(
                MIGRATIONS[0].sql.contains(&format!("CREATE TABLE {table}")),
                "baseline SQL is missing table {table}"
            );
        }
    }
}
