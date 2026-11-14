//! P1-04 acceptance tests: fixture catalog loads, SQLite cache round-trips,
//! and browsing works offline after the first load.

use std::fs;
use std::path::{Path, PathBuf};

use devkit_catalog::{OfflineCatalog, SyncReport};
use tempfile::TempDir;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn fixtures_load_and_validate() {
    let catalog = devkit_catalog::FixtureCatalog::load_dir(&fixtures_dir()).unwrap();
    assert!(
        catalog.packages.len() >= 2,
        "expected the committed package fixtures"
    );
    assert!(
        catalog.profiles.len() >= 2,
        "expected the committed profile fixtures"
    );
}

#[test]
fn sync_then_browse_round_trips() {
    let dir = TempDir::new().unwrap();
    let catalog = OfflineCatalog::open(&dir.path().join("state/devkit.sqlite")).unwrap();

    let report = catalog.sync_fixture_dir(&fixtures_dir()).unwrap();
    assert_eq!(
        report,
        SyncReport {
            packages: 2,
            profiles: 2,
        }
    );

    // Browsing reads from the cache.
    let packages = catalog.list_packages().unwrap();
    assert_eq!(packages.len(), 2);
    let python = catalog
        .get_package("devkit.lang.python")
        .unwrap()
        .expect("python fixture cached");
    assert_eq!(python.name, "Python");
    assert!(!python.versions.is_empty());

    // Cached version rows match the manifest.
    let versions = catalog
        .package_cache()
        .versions("devkit.lang.python")
        .unwrap();
    assert_eq!(versions.len(), python.versions.len());
    assert_eq!(versions[0].version, "3.12.7");
    assert_eq!(versions[0].channel, "stable");
    assert!(!versions[0].is_prerelease);

    // Profiles round-trip with membership rows.
    let profiles = catalog.list_profiles().unwrap();
    assert_eq!(profiles.len(), 2);
    let profile = catalog
        .get_profile("devkit.profile.python-developer")
        .unwrap()
        .expect("profile cached");
    assert_eq!(profile.packages.len(), 3);
    let membership = catalog
        .profile_cache()
        .get("devkit.profile.python-developer")
        .unwrap()
        .expect("profile row");
    assert_eq!(membership.origin, "catalog");
}

#[test]
fn browsing_is_offline_after_first_load() {
    // First load with fixtures present.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state/devkit.sqlite");
    {
        let catalog = OfflineCatalog::open(&path).unwrap();
        catalog.sync_fixture_dir(&fixtures_dir()).unwrap();
    }
    // The fixture directory is "gone" for this session (a fresh process
    // opening the same database file): browsing must still work, cache-only.
    let catalog = OfflineCatalog::open(&path).unwrap();
    let packages = catalog.list_packages().unwrap();
    assert_eq!(packages.len(), 2);
    assert!(catalog.get_package("devkit.lang.python").unwrap().is_some());
    assert_eq!(catalog.list_profiles().unwrap().len(), 2);
}

#[test]
fn resync_replaces_cached_versions() {
    // Copy fixtures to a writable dir, sync, then change the manifest and
    // sync again: the new version set must fully replace the old one.
    let dir = TempDir::new().unwrap();
    let fixture_dir = dir.path().join("fixtures");
    fs::create_dir_all(&fixture_dir).unwrap();
    for entry in fs::read_dir(fixtures_dir()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_file() {
            fs::copy(entry.path(), fixture_dir.join(entry.file_name())).unwrap();
        }
    }

    let db_path = dir.path().join("state/devkit.sqlite");
    let catalog = OfflineCatalog::open(&db_path).unwrap();
    catalog.sync_fixture_dir(&fixture_dir).unwrap();

    let json_path = fixture_dir.join("python-package.example.json");
    // Normalize CRLF (Windows) so the splice anchor is platform-independent.
    let mut json = fs::read_to_string(&json_path)
        .unwrap()
        .replace("\r\n", "\n");
    // Splice a second (beta) version into the versions array. Note: raw
    // strings do not process \n escapes, so the anchor is built with a
    // normal string literal.
    let anchor = "    }\n  ],";
    assert!(json.contains(anchor), "anchor not found in fixture copy");
    let extra_version = r#",
    {
      "version": "3.13.1",
      "channel": "beta",
      "artifacts": [
        {
          "os": "windows",
          "architecture": "x64",
          "installer": "exe",
          "url": "https://example.invalid/python-3.13.1-amd64.exe",
          "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          "size_bytes": 31000000
        }
      ]
    }"#;
    json = json.replace(anchor, &format!("    }}{extra_version}\n  ],"));
    fs::write(&json_path, json).unwrap();

    catalog.sync_fixture_dir(&fixture_dir).unwrap();
    let python = catalog
        .get_package("devkit.lang.python")
        .unwrap()
        .expect("still cached");
    assert_eq!(python.versions.len(), 2);
    let versions = catalog
        .package_cache()
        .versions("devkit.lang.python")
        .unwrap();
    assert_eq!(versions.len(), 2);
    let beta = versions.iter().find(|v| v.version == "3.13.1").unwrap();
    assert!(beta.is_prerelease, "beta channel is prerelease-style");
}

#[test]
fn invalid_fixture_is_rejected_not_cached() {
    let dir = TempDir::new().unwrap();
    let fixture_dir = dir.path().join("fixtures");
    fs::create_dir_all(&fixture_dir).unwrap();
    // Insecure (HTTP) artifact URL: semantically invalid per the security
    // model even though the JSON parses.
    fs::write(
        fixture_dir.join("bad-package.json"),
        r##"{
          "schema_version": 1,
          "id": "devkit.util.bad",
          "name": "Bad",
          "description": "",
          "publisher": "X",
          "homepage": "https://example.invalid/",
          "category": "utility",
          "supported_platforms": ["windows-x64"],
          "default_version_policy": "stable",
          "versions": [
            {
              "version": "1.0.0",
              "channel": "stable",
              "artifacts": [
                {
                  "os": "windows",
                  "architecture": "x64",
                  "installer": "exe",
                  "url": "http://example.invalid/bad.exe",
                  "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                  "size_bytes": 1
                }
              ]
            }
          ],
          "license_expression": "MIT",
          "maintainer": "X",
          "updated_at": "2026-11-13T00:00:00Z"
        }"##,
    )
    .unwrap();

    let catalog = OfflineCatalog::open_in_memory().unwrap();
    let result = catalog.sync_fixture_dir(&fixture_dir);
    assert!(result.is_err(), "invalid fixture must fail the sync");
    assert!(
        catalog.list_packages().unwrap().is_empty(),
        "nothing may be cached from a failed sync"
    );
}

#[test]
fn http_cache_round_trips() {
    let catalog = OfflineCatalog::open_in_memory().unwrap();
    let cache = catalog.http_cache();
    assert!(cache
        .get("https://example.invalid/packages")
        .unwrap()
        .is_none());

    cache
        .put(
            "https://example.invalid/packages",
            Some("\"etag-1\""),
            None,
            r#"{"items": []}"#,
        )
        .unwrap();
    let entry = cache
        .get("https://example.invalid/packages")
        .unwrap()
        .expect("cached");
    assert_eq!(entry.etag.as_deref(), Some("\"etag-1\""));
    assert!(entry.fetched_at.ends_with('Z'));

    // Re-put replaces the entry (validators included).
    cache
        .put(
            "https://example.invalid/packages",
            Some("\"etag-2\""),
            Some("Wed, 13 Nov 2026 00:00:00 GMT"),
            r#"{"items": ["new"]}"#,
        )
        .unwrap();
    let entry = cache
        .get("https://example.invalid/packages")
        .unwrap()
        .unwrap();
    assert_eq!(entry.etag.as_deref(), Some("\"etag-2\""));
    assert_eq!(entry.response_json, r#"{"items": ["new"]}"#);

    cache.remove("https://example.invalid/packages").unwrap();
    assert!(cache
        .get("https://example.invalid/packages")
        .unwrap()
        .is_none());
}
