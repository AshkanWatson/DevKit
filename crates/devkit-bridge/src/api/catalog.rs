//! Catalog read + sync operations (P3-05; the P1-04/P1-05 ops from
//! docs/ffi/bridge-contract.md). Browsing reads only ever touch the
//! local SQLite cache — offline browsing works after the first sync
//! (local-database.md §2). `sync_catalog` refreshes the cache from the
//! Django API over HTTPS (the client boundary never crosses plain
//! HTTP); network failure degrades to cache-only, never an error.

use devkit_catalog::offline::OfflineCatalog;
use devkit_catalog::remote::{RemoteCatalog, RemoteSyncReport, ReqwestCatalogHttp};
use devkit_core::manifest::{PackageManifest, ProfileManifest};
use devkit_registry::package_cache::PackageCache;
use devkit_registry::profile_cache::ProfileCache;

use super::engine::open_db;
use super::error::{bridge_error, BridgeError, BridgeErrorCode};

/// Wire helper: stringifies manifest enums through their serde renames
/// (the schema wire forms: "windows-x64", "build_tool", "stable", ...).
fn wire<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => text,
        _ => String::new(), // Unit-variant enums always serialize to strings.
    }
}

/// One downloadable artifact of a version.
#[derive(Debug, Clone)]
pub struct CatalogArtifactDto {
    pub os: String,
    pub architecture: String,
    pub installer: String,
    /// The official vendor URL. The Dart adapter shows only the source
    /// domain; URLs never appear in UI copy or logs (security-model.md).
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub archive_root: String,
    pub executable_relative_path: String,
}

/// A declared dependency of a version.
#[derive(Debug, Clone)]
pub struct CatalogDependencyDto {
    pub package_id: String,
    pub required: bool,
    pub policy: String,
    pub version: Option<String>,
    pub channel: Option<String>,
}

/// One release of a package.
#[derive(Debug, Clone)]
pub struct CatalogVersionDto {
    pub version: String,
    pub channel: String,
    pub release_date: Option<String>,
    pub release_notes_url: Option<String>,
    pub dependencies: Vec<CatalogDependencyDto>,
    pub artifacts: Vec<CatalogArtifactDto>,
}

/// A catalog package (schemas/package.schema.json).
#[derive(Debug, Clone)]
pub struct CatalogPackageDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub publisher: String,
    pub homepage: String,
    pub category: String,
    pub platforms: Vec<String>,
    pub default_policy: String,
    pub versions: Vec<CatalogVersionDto>,
    pub license: String,
    pub maintainer: String,
    pub updated_at: String,
}

/// A package selection inside a profile.
#[derive(Debug, Clone)]
pub struct CatalogProfilePackageDto {
    pub package_id: String,
    pub required: bool,
    pub policy: String,
    pub version: Option<String>,
    pub channel: Option<String>,
    pub reason: String,
}

/// A developer profile (schemas/profile.schema.json).
#[derive(Debug, Clone)]
pub struct CatalogProfileDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub maintainer: String,
    pub platforms: Vec<String>,
    pub packages: Vec<CatalogProfilePackageDto>,
}

fn package_dto(manifest: PackageManifest) -> CatalogPackageDto {
    CatalogPackageDto {
        id: manifest.id,
        name: manifest.name,
        description: manifest.description,
        publisher: manifest.publisher,
        homepage: manifest.homepage,
        category: wire(&manifest.category),
        platforms: manifest.supported_platforms.iter().map(wire).collect(),
        default_policy: wire(&manifest.default_version_policy),
        versions: manifest
            .versions
            .into_iter()
            .map(|version| CatalogVersionDto {
                version: version.version,
                channel: wire(&version.channel),
                release_date: version.release_date,
                release_notes_url: version.release_notes_url,
                dependencies: version
                    .dependencies
                    .into_iter()
                    .map(|dependency| CatalogDependencyDto {
                        package_id: dependency.package_id,
                        required: dependency.required,
                        policy: wire(&dependency.version_policy),
                        version: dependency.version,
                        channel: dependency.channel,
                    })
                    .collect(),
                artifacts: version
                    .artifacts
                    .into_iter()
                    .map(|artifact| CatalogArtifactDto {
                        os: wire(&artifact.os),
                        architecture: wire(&artifact.architecture),
                        installer: wire(&artifact.installer),
                        url: artifact.url,
                        sha256: artifact.sha256,
                        size_bytes: artifact.size_bytes,
                        archive_root: artifact.archive_root,
                        executable_relative_path: artifact.executable_relative_path,
                    })
                    .collect(),
            })
            .collect(),
        license: manifest.license_expression,
        maintainer: manifest.maintainer,
        updated_at: manifest.updated_at,
    }
}

fn profile_dto(manifest: ProfileManifest) -> CatalogProfileDto {
    CatalogProfileDto {
        id: manifest.id,
        name: manifest.name,
        description: manifest.description,
        maintainer: manifest.maintainer,
        platforms: manifest.supported_platforms.iter().map(wire).collect(),
        packages: manifest
            .packages
            .into_iter()
            .map(|package| CatalogProfilePackageDto {
                package_id: package.package_id,
                required: package.required,
                policy: wire(&package.version_policy),
                version: package.version,
                channel: package.channel,
                reason: package.reason,
            })
            .collect(),
    }
}

/// Cached packages, optionally filtered. `query` matches id, name,
/// description, or publisher case-insensitively; `category` is the wire
/// form ("language", "build_tool", ...). Ordering is stable by ID.
pub fn search_catalog(
    query: Option<String>,
    category: Option<String>,
) -> Result<Vec<CatalogPackageDto>, BridgeError> {
    let db = open_db()?;
    let mut packages = PackageCache::new(db.connection())
        .list()
        .map_err(super::error::from_registry_error)?
        .into_iter()
        .map(|cached| cached.manifest)
        .collect::<Vec<PackageManifest>>();
    packages.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(packages
        .into_iter()
        .filter(|manifest| {
            if let Some(category) = category.as_deref() {
                if wire(&manifest.category) != category {
                    return false;
                }
            }
            if let Some(query) = query.as_deref() {
                let query = query.to_ascii_lowercase();
                let haystack = format!(
                    "{} {} {} {}",
                    manifest.id, manifest.name, manifest.description, manifest.publisher
                )
                .to_ascii_lowercase();
                if !haystack.contains(&query) {
                    return false;
                }
            }
            true
        })
        .map(package_dto)
        .collect())
}

/// One cached package by stable ID (null when unknown).
pub fn get_package_details(package_id: String) -> Result<Option<CatalogPackageDto>, BridgeError> {
    let db = open_db()?;
    Ok(PackageCache::new(db.connection())
        .get(&package_id)
        .map_err(super::error::from_registry_error)?
        .map(|cached| cached.manifest)
        .map(package_dto))
}

/// All cached profiles.
pub fn list_catalog_profiles() -> Result<Vec<CatalogProfileDto>, BridgeError> {
    let db = open_db()?;
    let mut profiles = ProfileCache::new(db.connection())
        .list()
        .map_err(super::error::from_registry_error)?
        .into_iter()
        .map(|cached| cached.manifest)
        .collect::<Vec<ProfileManifest>>();
    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(profiles.into_iter().map(profile_dto).collect())
}

/// Counters from one catalog sync (cache-only browsing on failure).
#[derive(Debug, Clone)]
pub struct SyncCatalogStats {
    /// Package manifests fetched fresh and cached.
    pub packages_updated: usize,
    /// Profile manifests fetched fresh and cached.
    pub profiles_updated: usize,
    /// Fetches answered with 304 (nothing re-downloaded).
    pub not_modified: usize,
    /// Fetches that fell back to the cache (network unavailable).
    pub offline: usize,
    /// Fetches the API rejected (4xx); cached data kept.
    pub failed: usize,
}

/// Refreshes the local catalog cache from the Django API (P2-04
/// semantics: ETag revalidation, offline degradation, defensive
/// reconciliation). The base URL must be HTTPS.
pub fn sync_catalog(base_url: String) -> Result<SyncCatalogStats, BridgeError> {
    let db_path = super::engine::engine()?.db_path.clone();
    let catalog = OfflineCatalog::open(&db_path)
        .map_err(|error| bridge_error(BridgeErrorCode::DatabaseError, error.to_string()))?;
    let http = ReqwestCatalogHttp::new()
        .map_err(|error| bridge_error(BridgeErrorCode::NetworkError, error.to_string()))?;
    let remote = RemoteCatalog::new(catalog, http, base_url)
        .map_err(|error| bridge_error(BridgeErrorCode::InvalidArgument, error.to_string()))?;
    // The sync is async (single-flight HTTP); a dedicated thread keeps
    // the bridge thread pool free.
    let report: Result<RemoteSyncReport, devkit_catalog::CatalogError> =
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| devkit_catalog::CatalogError::Remote(error.to_string()))?;
            runtime.block_on(remote.sync())
        })
        .join()
        .map_err(|_| {
            bridge_error(
                BridgeErrorCode::DatabaseError,
                "catalog sync thread panicked",
            )
        })?;
    let report =
        report.map_err(|error| bridge_error(BridgeErrorCode::DatabaseError, error.to_string()))?;
    Ok(SyncCatalogStats {
        packages_updated: report.packages_updated,
        profiles_updated: report.profiles_updated,
        not_modified: report.not_modified,
        offline: report.offline,
        failed: report.failed,
    })
}
