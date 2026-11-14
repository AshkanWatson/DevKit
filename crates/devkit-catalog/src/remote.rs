//! Remote catalog client (P2-04).
//!
//! Fetches the public catalog API (docs/api/catalog-api.md) with
//! conditional requests and keeps every payload plus its validator in the
//! SQLite `catalog_cache` table (docs/architecture/local-database.md §2):
//!
//! - Every GET carries `If-None-Match` when a cached ETag exists; a `304`
//!   leaves both caches untouched, so unchanged catalogs are never
//!   re-downloaded (P2-04 acceptance criterion).
//! - Fresh responses (list pages, package/profile manifests, categories)
//!   are stored in `catalog_cache` and parsed into the structured caches
//!   (`packages` / `package_versions` / `profiles`), which is what all
//!   browsing queries read.
//! - When the network is down a sync degrades to a no-op per endpoint:
//!   browsing keeps serving the structured cache, so the catalog stays
//!   fully browsable offline after the first successful load (P2-04
//!   acceptance criterion). Network failures never fail a sync; a
//!   malformed API payload does (contract violation, not an offline
//!   condition).
//!
//! The HTTP transport is injected ([`CatalogHttp`]), so tests drive the
//! complete cache / revalidation / offline state machine without touching
//! the network (docs/testing/testing-strategy.md). The production
//! transport is [`ReqwestCatalogHttp`] (ADR 0002: reqwest + rustls,
//! HTTPS-only per docs/security/security-model.md §1).

use std::collections::HashSet;

use devkit_core::manifest::{PackageManifest, ProfileManifest};

use crate::error::CatalogError;
use crate::offline::OfflineCatalog;

/// API version prefix the client pins (catalog-api.md § API versioning).
const API_BASE: &str = "/api/v1";

/// Page size for list requests; large enough that fixture- and
/// dev-tool-scale catalogs paginate rarely.
const PAGE_SIZE: usize = 100;

/// Safety cap for `next`-link pagination loops.
const MAX_PAGES: usize = 50;

/// Result of one conditional GET.
#[derive(Debug, Clone)]
pub enum CatalogFetch {
    /// `304 Not Modified`: the cached payload is still current.
    NotModified,
    /// `200 OK` with a (possibly absent) ETag validator.
    Fresh { etag: Option<String>, body: String },
}

/// Transport-level failures.
#[derive(Debug)]
pub enum HttpError {
    /// The service could not be reached (DNS/connect/TLS/timeout) or
    /// answered 5xx. The cache fallback applies.
    Unavailable(String),
    /// The API rejected the request with a 4xx status (error envelope per
    /// catalog-api.md). The affected entry stays stale.
    Rejected { status: u16 },
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Unavailable(reason) => write!(f, "catalog service unreachable: {reason}"),
            HttpError::Rejected { status } => {
                write!(f, "catalog API rejected the request: HTTP {status}")
            }
        }
    }
}

/// HTTP transport for the catalog client. Async, injectable for tests.
/// The returned future is `Send` so the client can run on multi-threaded
/// executors (the bridge runs engine work off the UI thread).
pub trait CatalogHttp {
    fn get(
        &self,
        url: &str,
        if_none_match: Option<&str>,
    ) -> impl std::future::Future<Output = Result<CatalogFetch, HttpError>> + Send;
}

/// Production transport: reqwest over rustls with the HTTPS-only policy
/// enforced at the HTTP client level as well (ADR 0002,
/// docs/security/security-model.md §1).
pub struct ReqwestCatalogHttp {
    client: reqwest::Client,
}

impl ReqwestCatalogHttp {
    pub fn new() -> Result<Self, CatalogError> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .build()
            .map_err(|error| CatalogError::Remote(format!("HTTP client init failed: {error}")))?;
        Ok(Self { client })
    }
}

impl CatalogHttp for ReqwestCatalogHttp {
    async fn get(&self, url: &str, if_none_match: Option<&str>) -> Result<CatalogFetch, HttpError> {
        let mut request = self.client.get(url);
        if let Some(etag) = if_none_match {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request
            .send()
            .await
            .map_err(|error| HttpError::Unavailable(error.to_string()))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(CatalogFetch::NotModified);
        }
        if !status.is_success() {
            return Err(HttpError::Rejected {
                status: status.as_u16(),
            });
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = response
            .text()
            .await
            .map_err(|error| HttpError::Unavailable(error.to_string()))?;
        Ok(CatalogFetch::Fresh { etag, body })
    }
}

/// One endpoint fetch resolved against the local cache. Internal to the
/// sync state machine; the distinction between offline (network) and
/// rejected (API) outcomes drives the sync report.
enum FetchOutcome {
    /// `200 OK`: a fresh payload was downloaded and cached (the ETag was
    /// persisted to `catalog_cache` at the same time).
    Fresh { body: String },
    /// `304 Not Modified`: the validator proved the cached payload is
    /// still current; the body is replayed from `catalog_cache` and
    /// nothing was re-downloaded.
    Revalidated { body: String },
    /// Network failure: cache-only browsing applies (P2-04 fallback).
    Offline,
    /// API rejection (4xx): keep the cached data, count the failure.
    Rejected,
}

/// Counters from one [`RemoteCatalog::sync`] run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RemoteSyncReport {
    /// Package manifests fetched fresh and written to the structured cache.
    pub packages_updated: usize,
    /// Profile manifests fetched fresh and written to the structured cache.
    pub profiles_updated: usize,
    /// Fetches answered with 304 — validators still valid, nothing
    /// re-downloaded (P2-04 ETag acceptance criterion).
    pub not_modified: usize,
    /// Fetches that fell back to the cache because the network was down.
    pub offline: usize,
    /// Fetches the API rejected (4xx); cached data is kept stale.
    pub failed: usize,
}

/// List fetch results that must not abort a sync (network/API level);
/// parse and database errors abort it instead.
enum ListFetch {
    Offline,
    Rejected,
    Catalog(CatalogError),
}

/// List-page envelope (catalog-api.md § Pagination shape), shared by the
/// packages and profiles endpoints — only the `next` link and identities
/// are consumed; all other fields — present and future — are ignored
/// defensively (catalog-api.md § API versioning).
#[derive(serde::Deserialize)]
struct SummaryPage {
    next: Option<String>,
    results: Vec<SummaryRef>,
}

#[derive(serde::Deserialize)]
struct SummaryRef {
    id: String,
}

/// The remote catalog: offline SQLite cache plus an HTTP client with ETag
/// revalidation. Browsing reads always come from the cache (same seam as
/// [`OfflineCatalog`]); sync refreshes it from the API.
pub struct RemoteCatalog<C: CatalogHttp> {
    catalog: OfflineCatalog,
    http: C,
    base_url: String,
}

impl<C: CatalogHttp> RemoteCatalog<C> {
    /// Creates a client for `base_url` (e.g. `https://catalog.example`).
    ///
    /// Non-HTTPS base URLs are rejected outright: the Django API ↔ client
    /// boundary is a trust boundary that only ever crosses TLS
    /// (docs/security/security-model.md §1). Local API development is
    /// exercised through the injected transport in tests, not by relaxing
    /// this check.
    pub fn new(
        catalog: OfflineCatalog,
        http: C,
        base_url: impl Into<String>,
    ) -> Result<Self, CatalogError> {
        let base_url = base_url.into();
        if !base_url.starts_with("https://") {
            return Err(CatalogError::Remote(format!(
                "catalog base URL must use HTTPS (security-model.md §1): {base_url}"
            )));
        }
        Ok(Self {
            catalog,
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }

    /// The underlying offline catalog for direct cache access in tests
    /// and diagnostics.
    pub fn catalog(&self) -> &OfflineCatalog {
        &self.catalog
    }

    // Cache-only browsing: identical contract to `OfflineCatalog` — all
    // reads come from the SQLite cache, never from the network.

    pub fn list_packages(&self) -> Result<Vec<PackageManifest>, CatalogError> {
        self.catalog.list_packages()
    }

    pub fn get_package(&self, id: &str) -> Result<Option<PackageManifest>, CatalogError> {
        self.catalog.get_package(id)
    }

    pub fn list_profiles(&self) -> Result<Vec<ProfileManifest>, CatalogError> {
        self.catalog.list_profiles()
    }

    pub fn get_profile(&self, id: &str) -> Result<Option<ProfileManifest>, CatalogError> {
        self.catalog.get_profile(id)
    }

    /// Revalidate the whole catalog against the API (P2-04).
    ///
    /// Order: packages list → per-package manifests, profiles list →
    /// per-profile manifests, categories (response cache only). Every
    /// fetch is independent: a network failure only makes that endpoint
    /// keep serving from cache. A list is trusted for cache
    /// reconciliation only when *every* page loaded, so a partial fetch
    /// can never erase published packages.
    pub async fn sync(&self) -> Result<RemoteSyncReport, CatalogError> {
        let mut report = RemoteSyncReport::default();

        match self.fetch_package_ids().await {
            Ok((ids, revalidated_pages)) => {
                report.not_modified += revalidated_pages;
                self.reconcile_packages(&ids)?;
                for id in ids {
                    match self.fetch_cached(&self.package_detail_url(&id)).await? {
                        FetchOutcome::Fresh { body, .. } => {
                            let manifest: PackageManifest = serde_json::from_str(&body)?;
                            self.catalog.package_cache().upsert(&manifest)?;
                            report.packages_updated += 1;
                        }
                        FetchOutcome::Revalidated { .. } => report.not_modified += 1,
                        FetchOutcome::Offline => report.offline += 1,
                        FetchOutcome::Rejected => report.failed += 1,
                    }
                }
            }
            Err(ListFetch::Offline) => report.offline += 1,
            Err(ListFetch::Rejected) => report.failed += 1,
            Err(ListFetch::Catalog(error)) => return Err(error),
        }

        match self.fetch_profile_ids().await {
            Ok((ids, revalidated_pages)) => {
                report.not_modified += revalidated_pages;
                self.reconcile_profiles(&ids)?;
                for id in ids {
                    match self.fetch_cached(&self.profile_detail_url(&id)).await? {
                        FetchOutcome::Fresh { body, .. } => {
                            let manifest: ProfileManifest = serde_json::from_str(&body)?;
                            self.catalog.profile_cache().upsert(&manifest, "catalog")?;
                            report.profiles_updated += 1;
                        }
                        FetchOutcome::Revalidated { .. } => report.not_modified += 1,
                        FetchOutcome::Offline => report.offline += 1,
                        FetchOutcome::Rejected => report.failed += 1,
                    }
                }
            }
            Err(ListFetch::Offline) => report.offline += 1,
            Err(ListFetch::Rejected) => report.failed += 1,
            Err(ListFetch::Catalog(error)) => return Err(error),
        }

        // Categories have no structured table yet (no browsing surface);
        // the response is cached for later phases.
        match self.fetch_cached(&self.categories_url()).await? {
            FetchOutcome::Fresh { .. } => {}
            FetchOutcome::Revalidated { .. } => report.not_modified += 1,
            FetchOutcome::Offline => report.offline += 1,
            FetchOutcome::Rejected => report.failed += 1,
        }

        Ok(report)
    }

    /// After a complete list refresh the remote list is authoritative:
    /// packages (and profiles) no longer in it are unpublished and
    /// disappear from the structured cache.
    fn reconcile_packages(&self, ids: &[String]) -> Result<(), CatalogError> {
        let present: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let cache = self.catalog.package_cache();
        for cached in cache.list()? {
            if !present.contains(cached.manifest.id.as_str()) {
                cache.remove(&cached.manifest.id)?;
            }
        }
        Ok(())
    }

    fn reconcile_profiles(&self, ids: &[String]) -> Result<(), CatalogError> {
        let present: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let cache = self.catalog.profile_cache();
        for cached in cache.list()? {
            if !present.contains(cached.manifest.id.as_str()) {
                cache.remove(&cached.manifest.id)?;
            }
        }
        Ok(())
    }

    /// One conditional GET: revalidates with the stored ETag and writes
    /// fresh payloads + validators into `catalog_cache`. A `304` resolves
    /// to the cached payload ([`FetchOutcome::Revalidated`]) so callers
    /// always get the current body while the sync report can still tell
    /// revalidations from downloads.
    async fn fetch_cached(&self, url: &str) -> Result<FetchOutcome, CatalogError> {
        let validator = self
            .catalog
            .http_cache()
            .get(url)?
            .and_then(|cached| cached.etag);
        match self.http.get(url, validator.as_deref()).await {
            Ok(CatalogFetch::Fresh { etag, body }) => {
                self.catalog
                    .http_cache()
                    .put(url, etag.as_deref(), None, &body)?;
                Ok(FetchOutcome::Fresh { body })
            }
            Ok(CatalogFetch::NotModified) => match self.catalog.http_cache().get(url)? {
                Some(cached) => Ok(FetchOutcome::Revalidated {
                    body: cached.response_json,
                }),
                // A 304 for a URL we never stored is a server contract
                // violation; treat it like an outage (cache-only), not a
                // hard failure.
                None => Ok(FetchOutcome::Offline),
            },
            Err(HttpError::Unavailable(_)) => Ok(FetchOutcome::Offline),
            Err(HttpError::Rejected { status: _ }) => Ok(FetchOutcome::Rejected),
        }
    }

    async fn fetch_package_ids(&self) -> Result<(Vec<String>, usize), ListFetch> {
        self.fetch_list_ids(&self.packages_list_url()).await
    }

    async fn fetch_profile_ids(&self) -> Result<(Vec<String>, usize), ListFetch> {
        self.fetch_list_ids(&self.profiles_list_url()).await
    }

    /// All identities from a paginated list endpoint, following `next`
    /// links, plus the number of pages that arrived as 304 revalidations.
    /// The list is only returned when every page loaded: a partial list
    /// must never drive cache reconciliation.
    async fn fetch_list_ids(&self, start_url: &str) -> Result<(Vec<String>, usize), ListFetch> {
        let mut ids = Vec::new();
        let mut revalidated_pages = 0usize;
        let mut url = start_url.to_owned();
        let mut pages = 0usize;
        while pages < MAX_PAGES {
            let body = match self.fetch_cached(&url).await.map_err(ListFetch::Catalog)? {
                FetchOutcome::Fresh { body, .. } => body,
                FetchOutcome::Revalidated { body } => {
                    revalidated_pages += 1;
                    body
                }
                FetchOutcome::Offline => return Err(ListFetch::Offline),
                FetchOutcome::Rejected => return Err(ListFetch::Rejected),
            };
            let page: SummaryPage = serde_json::from_str(&body)
                .map_err(|error| ListFetch::Catalog(CatalogError::Json(error)))?;
            ids.extend(page.results.into_iter().map(|summary| summary.id));
            pages += 1;
            match page.next {
                Some(next) => {
                    url = if next.starts_with('/') {
                        format!("{}{next}", self.base_url)
                    } else {
                        next
                    };
                }
                None => return Ok((ids, revalidated_pages)),
            }
        }
        Err(ListFetch::Catalog(CatalogError::Remote(format!(
            "catalog list pagination exceeded {MAX_PAGES} pages"
        ))))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{API_BASE}{path}", self.base_url)
    }

    fn packages_list_url(&self) -> String {
        self.url(&format!("/catalog/packages?page=1&page_size={PAGE_SIZE}"))
    }

    fn package_detail_url(&self, id: &str) -> String {
        self.url(&format!("/catalog/packages/{id}"))
    }

    fn profiles_list_url(&self) -> String {
        self.url(&format!("/catalog/profiles?page=1&page_size={PAGE_SIZE}"))
    }

    fn profile_detail_url(&self, id: &str) -> String {
        self.url(&format!("/catalog/profiles/{id}"))
    }

    fn categories_url(&self) -> String {
        self.url("/catalog/categories")
    }
}
