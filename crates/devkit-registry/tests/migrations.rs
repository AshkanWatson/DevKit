//! Migration tests (HANDOFF.md §15, docs/architecture/local-database.md §3.5):
//! fresh-apply, sequential-apply, and idempotence (applying twice must be a
//! clean no-op, not a corruption).

use devkit_registry::migrations::{self, Migration};
use devkit_registry::Database;
use tempfile::TempDir;

const EXPECTED_TABLES: &[&str] = &[
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

fn table_names(db: &Database) -> Vec<String> {
    let conn = db.connection();
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    let rows = stmt.query_map([], |row| row.get::<_, String>(0)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

#[test]
fn fresh_apply_creates_baseline_schema() {
    let dir = TempDir::new().unwrap();
    let db = Database::open(&dir.path().join("state/devkit.sqlite")).unwrap();

    assert_eq!(db.applied_migrations().unwrap(), vec![1]);
    let tables = table_names(&db);
    for expected in EXPECTED_TABLES {
        assert!(
            tables.iter().any(|t| t == expected),
            "missing table {expected}; found {tables:?}"
        );
    }
}

#[test]
fn fresh_database_gets_wal_and_foreign_keys() {
    let dir = TempDir::new().unwrap();
    let db = Database::open(&dir.path().join("devkit.sqlite")).unwrap();
    let conn = db.connection();

    let foreign_keys: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(foreign_keys, 1);
    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
}

#[test]
fn foreign_keys_are_enforced() {
    let dir = TempDir::new().unwrap();
    let db = Database::open(&dir.path().join("devkit.sqlite")).unwrap();
    let conn = db.connection();

    conn.execute(
        "INSERT INTO packages (id, name, description, publisher, homepage, category,
                               default_version_policy, manifest, schema_version,
                               updated_at, cached_at)
         VALUES ('devkit.lang.python', 'Python', '', 'PSF', 'https://www.python.org/',
                 'language', 'stable', '{}', 1, '2026-01-01T00:00:00Z',
                 '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    let result = conn.execute(
        "INSERT INTO package_versions (package_id, version, channel, manifest_json, cached_at)
         VALUES ('devkit.lang.nonexistent', '1.0.0', 'stable', '{}', '2026-01-01T00:00:00Z')",
        [],
    );
    assert!(result.is_err(), "version row without a parent must fail");
}

/// Synthetic two-migration list proving sequential-apply ordering: the
/// second migration's table references the first's, so order is observable.
#[test]
fn sequential_apply_applies_migrations_in_order() {
    // Mirrors the real baseline contract: the version-1 migration's SQL
    // creates `schema_migrations` itself (see 0001_baseline_v1.sql, whose
    // DDL is verbatim from docs/architecture/local-database.md §2).
    let v1 = Migration {
        version: 1,
        name: "test_v1",
        sql: "CREATE TABLE schema_migrations (
                  version INTEGER PRIMARY KEY,
                  name TEXT NOT NULL,
                  applied_at TEXT NOT NULL);
              CREATE TABLE t_a (id TEXT PRIMARY KEY);",
    };
    let v2 = Migration {
        version: 2,
        name: "test_v2",
        sql: "CREATE TABLE t_b (id TEXT PRIMARY KEY REFERENCES t_a(id) ON DELETE CASCADE);",
    };
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let applied = migrations::apply(&conn, &[v1, v2]).unwrap();
    assert_eq!(applied, vec![1, 2]);

    conn.execute("INSERT INTO t_a (id) VALUES ('a')", [])
        .unwrap();
    conn.execute("INSERT INTO t_b (id) VALUES ('a')", [])
        .unwrap();

    // A partially-migrated database at version 1 only applies v2.
    let conn2 = rusqlite::Connection::open_in_memory().unwrap();
    migrations::apply(&conn2, &[v1]).unwrap();
    let applied = migrations::apply(&conn2, &[v1, v2]).unwrap();
    assert_eq!(applied, vec![1, 2]);
    // t_a survived (v1 was correctly skipped the second time).
    conn2
        .execute("INSERT INTO t_a (id) VALUES ('b')", [])
        .unwrap();
    conn2
        .execute("INSERT INTO t_b (id) VALUES ('b')", [])
        .unwrap();
}

/// Idempotence: re-applying the committed migration list on an
/// already-migrated database is a clean no-op that preserves data.
#[test]
fn reopen_and_reapply_is_a_clean_noop() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("devkit.sqlite");

    let package_json = r##"{
        "schema_version": 1,
        "id": "devkit.lang.python",
        "name": "Python",
        "description": "round-trip marker",
        "publisher": "PSF",
        "homepage": "https://www.python.org/",
        "category": "language",
        "supported_platforms": ["windows-x64"],
        "default_version_policy": "stable",
        "versions": [
            {
                "version": "3.12.7",
                "channel": "stable",
                "artifacts": [
                    {
                        "os": "windows",
                        "architecture": "x64",
                        "installer": "exe",
                        "url": "https://example.invalid/python.exe",
                        "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "size_bytes": 1
                    }
                ]
            }
        ],
        "license_expression": "PSF-2.0",
        "maintainer": "Team",
        "updated_at": "2026-11-13T00:00:00Z"
    }"##;
    let manifest: devkit_core::manifest::PackageManifest =
        serde_json::from_str(package_json).unwrap();

    {
        let db = Database::open(&path).unwrap();
        devkit_registry::package_cache::PackageCache::new(db.connection())
            .upsert(&manifest)
            .unwrap();
    }
    {
        let db = Database::open(&path).unwrap(); // reopen
        db.migrate().unwrap(); // re-apply
        assert_eq!(db.applied_migrations().unwrap(), vec![1]);

        // No duplicate migration rows.
        let count: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // Data written before the reopen survives.
        let cached = devkit_registry::package_cache::PackageCache::new(db.connection())
            .get("devkit.lang.python")
            .unwrap()
            .expect("package survives reopen");
        assert_eq!(cached.manifest.id, "devkit.lang.python");
    }
}

#[test]
fn migration_rows_record_version_name_and_utc_time() {
    let dir = TempDir::new().unwrap();
    let db = Database::open(&dir.path().join("devkit.sqlite")).unwrap();
    let (version, name, applied_at): (i64, String, String) = db
        .connection()
        .query_row(
            "SELECT version, name, applied_at FROM schema_migrations",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(version, 1);
    assert_eq!(name, "baseline_v1");
    // UTC ISO-8601 (RFC 3339): ends with Z, contains a 'T' separator.
    assert!(applied_at.ends_with('Z'), "applied_at = {applied_at}");
    assert!(applied_at.contains('T'), "applied_at = {applied_at}");
}
