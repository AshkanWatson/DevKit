//! P2-04 acceptance tests: ETag revalidation avoids re-downloading
//! unchanged catalogs, and cached browsing works with the network down.
//!
//! All HTTP is a fake in-memory transport — no sockets, no real requests
//! (docs/testing/testing-strategy.md). The fake is shared with the client
//! behind `Arc` so tests can inspect recorded requests after syncing.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use devkit_catalog::{
    CatalogFetch, CatalogHttp, HttpError, OfflineCatalog, RemoteCatalog, RemoteSyncReport,
};
use serde_json::{json, Value};
use tempfile::TempDir;

const BASE: &str = "https://catalog.example";
const PYTHON_ID: &str = "devkit.lang.python";
const FLUTTER_ID: &str = "devkit.framework.flutter";
const PYTHON_PROFILE_ID: &str = "devkit.profile.python-developer";
const FLUTTER_PROFILE_ID: &str = "devkit.profile.flutter-developer";

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn package_fixture(id: &str) -> String {
    let file = if id == PYTHON_ID {
        "python-package.example.json"
    } else {
        "flutter-package.example.json"
    };
    fs::read_to_string(fixtures_dir().join(file)).unwrap()
}

fn profile_fixture(id: &str) -> String {
    let file = if id == PYTHON_PROFILE_ID {
        "python-developer-profile.example.json"
    } else {
        "flutter-developer-profile.example.json"
    };
    fs::read_to_string(fixtures_dir().join(file)).unwrap()
}

struct FakeResponse {
    etag: String,
    body: String,
}

/// In-memory transport: deterministic responses, recorded requests, and a
/// switch that takes the "network" down. No sockets involved.
struct FakeHttp {
    down: AtomicBool,
    responses: Mutex<HashMap<String, FakeResponse>>,
    /// (url, if-none-match) per request, in order.
    requests: Mutex<Vec<(String, Option<String>)>>,
    /// Number of 200 bodies actually delivered.
    bodies_served: AtomicUsize,
}

impl FakeHttp {
    fn new() -> Self {
        Self {
            down: AtomicBool::new(false),
            responses: Mutex::new(HashMap::new()),
            requests: Mutex::new(Vec::new()),
            bodies_served: AtomicUsize::new(0),
        }
    }

    fn respond(&self, url: String, etag: &str, body: String) {
        self.responses.lock().unwrap().insert(
            url,
            FakeResponse {
                etag: etag.to_owned(),
                body,
            },
        );
    }

    fn set_down(&self, down: bool) {
        self.down.store(down, Ordering::SeqCst);
    }

    fn requests(&self) -> Vec<(String, Option<String>)> {
        self.requests.lock().unwrap().clone()
    }

    fn bodies_served(&self) -> usize {
        self.bodies_served.load(Ordering::SeqCst)
    }
}

/// Shared handle for the fake transport. A newtype because the orphan
/// rule forbids implementing a foreign trait on `Arc` directly.
struct SharedFakeHttp(Arc<FakeHttp>);

impl CatalogHttp for SharedFakeHttp {
    async fn get(&self, url: &str, if_none_match: Option<&str>) -> Result<CatalogFetch, HttpError> {
        let inner = &self.0;
        inner
            .requests
            .lock()
            .unwrap()
            .push((url.to_owned(), if_none_match.map(str::to_owned)));
        if inner.down.load(Ordering::SeqCst) {
            return Err(HttpError::Unavailable("network down".into()));
        }
        // Own the decision inside a block so the mutex guard never
        // becomes part of the returned future's state.
        let decision = {
            let responses = inner.responses.lock().unwrap();
            match responses.get(url) {
                None => Err(HttpError::Rejected { status: 404 }),
                Some(response) => {
                    if if_none_match == Some(response.etag.as_str()) {
                        Ok(CatalogFetch::NotModified)
                    } else {
                        inner.bodies_served.fetch_add(1, Ordering::SeqCst);
                        Ok(CatalogFetch::Fresh {
                            etag: Some(response.etag.clone()),
                            body: response.body.clone(),
                        })
                    }
                }
            }
        };
        decision
    }
}

/// Ids list page as a summary envelope.
fn list_page(ids: &[&str], next: Option<&str>) -> String {
    let results: Vec<Value> = ids
        .iter()
        .map(|id| json!({ "id": id, "name": id }))
        .collect();
    json!({
        "count": ids.len(),
        "next": next,
        "previous": Value::Null,
        "results": results,
    })
    .to_string()
}

/// Seeds the full fake API: single-page lists, manifest details, and
/// categories.
fn seed_fake_api(http: &FakeHttp, package_ids: &[&str], profile_ids: &[&str]) {
    let list_url = format!("{BASE}/api/v1/catalog/packages?page=1&page_size=100");
    http.respond(list_url, "pkg-list-v1", list_page(package_ids, None));
    for id in package_ids {
        let detail = format!("{BASE}/api/v1/catalog/packages/{id}");
        http.respond(detail, &format!("pkg-{id}-v1"), package_fixture(id));
    }
    seed_profiles_and_categories(http, profile_ids);
}

/// Profiles list/details and categories (used standalone when a test
/// customizes the packages list, e.g. for pagination).
fn seed_profiles_and_categories(http: &FakeHttp, profile_ids: &[&str]) {
    let profiles_url = format!("{BASE}/api/v1/catalog/profiles?page=1&page_size=100");
    http.respond(
        profiles_url,
        "profile-list-v1",
        list_page(profile_ids, None),
    );
    for id in profile_ids {
        let detail = format!("{BASE}/api/v1/catalog/profiles/{id}");
        http.respond(detail, &format!("profile-{id}-v1"), profile_fixture(id));
    }

    http.respond(
        format!("{BASE}/api/v1/catalog/categories"),
        "cat-v1",
        json!({ "results": [{ "id": "language", "package_count": 1 }] }).to_string(),
    );
}

fn remote_catalog(http: SharedFakeHttp) -> RemoteCatalog<SharedFakeHttp> {
    RemoteCatalog::new(OfflineCatalog::open_in_memory().unwrap(), http, BASE).unwrap()
}

/// Fetch counts per full sync of the default fake API: 1 packages list +
/// 2 package details + 1 profiles list + 2 profile details + 1 categories.
const FULL_SYNC_FETCHES: usize = 7;

#[tokio::test]
async fn first_sync_populates_structured_and_response_cache() {
    let fake = Arc::new(FakeHttp::new());
    seed_fake_api(
        &fake,
        &[PYTHON_ID, FLUTTER_ID],
        &[PYTHON_PROFILE_ID, FLUTTER_PROFILE_ID],
    );
    let remote = remote_catalog(SharedFakeHttp(fake.clone()));

    let report = remote.sync().await.unwrap();
    assert_eq!(
        report,
        RemoteSyncReport {
            packages_updated: 2,
            profiles_updated: 2,
            not_modified: 0,
            offline: 0,
            failed: 0,
        }
    );

    // The structured cache serves browsing without any network.
    assert_eq!(remote.list_packages().unwrap().len(), 2);
    let python = remote.get_package(PYTHON_ID).unwrap().unwrap();
    assert_eq!(python.name, "Python");
    assert!(!python.versions.is_empty());
    assert_eq!(remote.list_profiles().unwrap().len(), 2);

    // The response cache holds every fetched URL with its validator.
    let http_cache = remote.catalog().http_cache();
    let list = http_cache
        .get(&format!(
            "{BASE}/api/v1/catalog/packages?page=1&page_size=100"
        ))
        .unwrap()
        .expect("list page cached");
    assert_eq!(list.etag.as_deref(), Some("pkg-list-v1"));
    let detail = http_cache
        .get(&format!("{BASE}/api/v1/catalog/packages/{PYTHON_ID}"))
        .unwrap()
        .expect("package manifest cached");
    assert_eq!(
        detail.etag.as_deref(),
        Some(format!("pkg-{PYTHON_ID}-v1").as_str())
    );
}

#[tokio::test]
async fn second_sync_revalidates_and_never_redownloads_unchanged_catalogs() {
    let fake = Arc::new(FakeHttp::new());
    seed_fake_api(
        &fake,
        &[PYTHON_ID, FLUTTER_ID],
        &[PYTHON_PROFILE_ID, FLUTTER_PROFILE_ID],
    );
    let remote = remote_catalog(SharedFakeHttp(fake.clone()));
    remote.sync().await.unwrap();
    let served_before = fake.bodies_served();
    let requests_before = fake.requests().len();

    let report = remote.sync().await.unwrap();
    assert_eq!(
        report,
        RemoteSyncReport {
            packages_updated: 0,
            profiles_updated: 0,
            not_modified: FULL_SYNC_FETCHES,
            offline: 0,
            failed: 0,
        }
    );

    // Every revalidation request carried the stored validator.
    for (url, if_none_match) in &fake.requests()[requests_before..] {
        assert!(if_none_match.is_some(), "missing If-None-Match on {url}");
    }
    // And not a single body was re-downloaded (P2-04 acceptance).
    assert_eq!(fake.bodies_served(), served_before);
}

#[tokio::test]
async fn cached_browsing_works_with_the_network_down() {
    let fake = Arc::new(FakeHttp::new());
    seed_fake_api(
        &fake,
        &[PYTHON_ID, FLUTTER_ID],
        &[PYTHON_PROFILE_ID, FLUTTER_PROFILE_ID],
    );
    let remote = remote_catalog(SharedFakeHttp(fake.clone()));
    remote.sync().await.unwrap();

    // The network goes away before the next sync.
    fake.set_down(true);
    let report = remote.sync().await.unwrap();

    // The three list-level fetches (packages, profiles, categories) fall
    // back to cache; per-package details are never attempted because the
    // list is untrusted.
    assert_eq!(
        report,
        RemoteSyncReport {
            packages_updated: 0,
            profiles_updated: 0,
            not_modified: 0,
            offline: 3,
            failed: 0,
        }
    );

    // Browsing still serves the previously synced catalog in full.
    assert_eq!(remote.list_packages().unwrap().len(), 2);
    let python = remote.get_package(PYTHON_ID).unwrap().unwrap();
    assert_eq!(python.name, "Python");
    assert_eq!(remote.list_profiles().unwrap().len(), 2);
    assert!(remote.get_profile(PYTHON_PROFILE_ID).unwrap().is_some());
}

#[tokio::test]
async fn empty_cache_with_network_down_stays_browsable_but_empty() {
    let fake = Arc::new(FakeHttp::new());
    fake.set_down(true);
    let remote = remote_catalog(SharedFakeHttp(fake));

    let report = remote.sync().await.unwrap();
    assert_eq!(report.offline, 3);
    assert_eq!(remote.list_packages().unwrap(), Vec::new());
    assert!(remote.get_package(PYTHON_ID).unwrap().is_none());
}

#[tokio::test]
async fn changed_catalog_updates_cache_and_drops_unpublished_packages() {
    let fake = Arc::new(FakeHttp::new());
    seed_fake_api(
        &fake,
        &[PYTHON_ID, FLUTTER_ID],
        &[PYTHON_PROFILE_ID, FLUTTER_PROFILE_ID],
    );
    let remote = remote_catalog(SharedFakeHttp(fake.clone()));
    remote.sync().await.unwrap();

    // New remote state: Flutter unpublished (absent from the list), Python
    // re-published with changed metadata and a bumped ETag.
    let list_url = format!("{BASE}/api/v1/catalog/packages?page=1&page_size=100");
    fake.respond(list_url, "pkg-list-v2", list_page(&[PYTHON_ID], None));

    let mut python: Value = serde_json::from_str(&package_fixture(PYTHON_ID)).unwrap();
    python["description"] = json!("Updated description from the changed catalog.");
    fake.respond(
        format!("{BASE}/api/v1/catalog/packages/{PYTHON_ID}"),
        "pkg-python-v2",
        python.to_string(),
    );

    let report = remote.sync().await.unwrap();
    assert_eq!(report.packages_updated, 1);
    // Profiles (list + 2 details) and categories revalidated unchanged.
    assert_eq!(report.not_modified, 4);

    // Flutter is gone from the cache (unpublished), Python shows the
    // updated manifest.
    assert!(remote.get_package(FLUTTER_ID).unwrap().is_none());
    let python = remote.get_package(PYTHON_ID).unwrap().unwrap();
    assert_eq!(
        python.description,
        "Updated description from the changed catalog."
    );

    // The response cache records the new validators.
    let detail = remote
        .catalog()
        .http_cache()
        .get(&format!("{BASE}/api/v1/catalog/packages/{PYTHON_ID}"))
        .unwrap()
        .expect("python detail still cached");
    assert_eq!(detail.etag.as_deref(), Some("pkg-python-v2"));
    assert!(
        remote
            .catalog()
            .http_cache()
            .get(&format!("{BASE}/api/v1/catalog/packages/{FLUTTER_ID}"))
            .unwrap()
            .is_some(),
        "the response cache keeps previously seen URLs (validators for replays)"
    );
}

#[tokio::test]
async fn paginated_list_follows_next_links() {
    let fake = Arc::new(FakeHttp::new());
    // Two list pages: page 1 has Python and a `next` link, page 2 has
    // Flutter and none.
    let page1_url = format!("{BASE}/api/v1/catalog/packages?page=1&page_size=100");
    let page2_path = "/api/v1/catalog/packages?page=2&page_size=100";
    fake.respond(
        page1_url,
        "pkg-list-p1",
        list_page(&[PYTHON_ID], Some(page2_path)),
    );
    fake.respond(
        format!("{BASE}{page2_path}"),
        "pkg-list-p2",
        list_page(&[FLUTTER_ID], None),
    );
    for id in [PYTHON_ID, FLUTTER_ID] {
        fake.respond(
            format!("{BASE}/api/v1/catalog/packages/{id}"),
            &format!("pkg-{id}-v1"),
            package_fixture(id),
        );
    }
    seed_profiles_and_categories(&fake, &[PYTHON_PROFILE_ID, FLUTTER_PROFILE_ID]);

    let remote = remote_catalog(SharedFakeHttp(fake));
    let report = remote.sync().await.unwrap();
    assert_eq!(report.packages_updated, 2);
    assert_eq!(remote.list_packages().unwrap().len(), 2);
    assert!(remote.get_package(FLUTTER_ID).unwrap().is_some());
}

#[test]
fn non_https_base_url_is_rejected() {
    let error = RemoteCatalog::new(
        OfflineCatalog::open_in_memory().unwrap(),
        SharedFakeHttp(Arc::new(FakeHttp::new())),
        "http://catalog.example",
    )
    .err()
    .expect("expected the non-HTTPS base URL to be rejected");
    assert!(
        error.to_string().contains("HTTPS"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn rejected_detail_keeps_the_stale_cache() {
    let fake = Arc::new(FakeHttp::new());
    seed_fake_api(
        &fake,
        &[PYTHON_ID, FLUTTER_ID],
        &[PYTHON_PROFILE_ID, FLUTTER_PROFILE_ID],
    );
    let remote = remote_catalog(SharedFakeHttp(fake.clone()));
    remote.sync().await.unwrap();

    // Next sync: the Python detail endpoint is broken. The fake answers
    // 404 (Rejected) for any URL without a seeded response, so remove its
    // response. The stale manifest must survive.
    fake.responses
        .lock()
        .unwrap()
        .remove(&format!("{BASE}/api/v1/catalog/packages/{PYTHON_ID}"));

    let report = remote.sync().await.unwrap();
    assert_eq!(report.failed, 1);
    assert!(remote.get_package(PYTHON_ID).unwrap().is_some());
}

#[tokio::test]
async fn sync_reconciles_offline_database_too() {
    // Reconciliation also works on a database that already has fixture
    // content: the remote list is authoritative after a full fetch.
    let dir = TempDir::new().unwrap();
    let catalog = OfflineCatalog::open(&dir.path().join("state/devkit.sqlite")).unwrap();
    catalog.sync_fixture_dir(&fixtures_dir()).unwrap();
    assert_eq!(catalog.list_packages().unwrap().len(), 2);

    let fake = Arc::new(FakeHttp::new());
    seed_fake_api(&fake, &[PYTHON_ID], &[PYTHON_PROFILE_ID]);
    let remote = RemoteCatalog::new(catalog, SharedFakeHttp(fake), BASE).unwrap();
    let report = remote.sync().await.unwrap();
    assert_eq!(report.packages_updated, 1);
    assert_eq!(report.profiles_updated, 1);

    let ids: Vec<String> = remote
        .list_packages()
        .unwrap()
        .into_iter()
        .map(|package| package.id)
        .collect();
    assert_eq!(ids, vec![PYTHON_ID.to_owned()]);
}
