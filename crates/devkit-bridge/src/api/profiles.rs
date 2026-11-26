//! Profile resolution at the engine/bridge boundary (P4-02).
//!
//! Turns a cached profile (`CatalogProfileDto` data) into a **reviewable
//! install plan**: each package selection's version policy is resolved against
//! the cached catalog versions into a concrete version. Browsing reads only
//! ever touch the local SQLite cache — no network. The pure resolution engine
//! lives in `devkit_core::profile`; this module supplies its release source
//! from the cache and maps the result/errors onto the bridge contract.

use std::collections::HashMap;

use devkit_core::profile::{
    resolve_profile as core_resolve_profile, ProfileReleaseSource,
    ProfileResolution as CoreProfileResolution, ProfileResolveError, SkipReason,
};
use devkit_core::project::{resolve_project as core_resolve_project, ProjectManifest};
use devkit_core::version::{Release, VersionPolicy};
use devkit_registry::package_cache::PackageCache;
use devkit_registry::Database;

use super::error::{
    bridge_error, from_engine_code, from_registry_error, BridgeError, BridgeErrorCode,
};

/// A `ProfileReleaseSource` backed by the local catalog cache. Releases for
/// each referenced package are loaded once and supplied newest-first (the
/// ordering `devkit_core::version::resolve` requires of its caller):
/// `release_date` descending, then version string descending as a tiebreaker.
struct CacheReleaseSource {
    by_package: HashMap<String, Vec<Release>>,
}

impl CacheReleaseSource {
    fn load(db: &Database, package_ids: &[&str]) -> Result<Self, devkit_registry::RegistryError> {
        let cache = PackageCache::new(db.connection());
        let mut by_package = HashMap::new();
        for package_id in package_ids {
            let mut cached = cache.versions(package_id)?;
            // Newest first: ISO-8601 release_date desc (None last), then
            // version string desc for a stable total order.
            cached.sort_by(|a, b| match (&b.release_date, &a.release_date) {
                (Some(x), Some(y)) => x.cmp(y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b.version.cmp(&a.version),
            });
            by_package.insert(
                (*package_id).to_owned(),
                cached
                    .into_iter()
                    .map(|v| Release {
                        version: v.version,
                        channel: v.channel,
                        prerelease: v.is_prerelease,
                    })
                    .collect(),
            );
        }
        Ok(Self { by_package })
    }
}

impl ProfileReleaseSource for CacheReleaseSource {
    fn releases(&self, package_id: &str) -> &[Release] {
        self.by_package
            .get(package_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// One resolved package in the reviewable plan.
#[derive(Debug, Clone)]
pub struct ResolvedProfilePackageDto {
    pub package_id: String,
    pub required: bool,
    /// The applied policy's wire label (`exact`/`stable`/`latest`/`channel`).
    pub policy: String,
    /// The concrete version it resolved to.
    pub version: String,
}

/// Why a package was left out of an applied profile plan.
#[derive(Debug, Clone, Copy)]
pub enum ProfileSkipReasonDto {
    PackageUnavailable,
    NoMatchingVersion,
    /// An optional package the user deselected in the review step.
    Deselected,
}

/// An optional package that could not be resolved.
#[derive(Debug, Clone)]
pub struct SkippedProfilePackageDto {
    pub package_id: String,
    pub reason: ProfileSkipReasonDto,
}

/// A reviewable resolution plan for one profile.
#[derive(Debug, Clone)]
pub struct ProfileResolutionDto {
    pub profile_id: String,
    pub packages: Vec<ResolvedProfilePackageDto>,
    pub skipped: Vec<SkippedProfilePackageDto>,
}

/// One install job enqueued while applying a profile.
#[derive(Debug, Clone)]
pub struct ProfileApplyJobDto {
    pub package_id: String,
    pub version: String,
    /// The real install job ID to watch for per-package progress.
    pub job_id: String,
}

/// Summary returned when a reviewed profile is applied: the jobs to track
/// and everything that was skipped (unresolvable or deselected optionals).
#[derive(Debug, Clone)]
pub struct ProfileApplyResultDto {
    pub profile_id: String,
    pub jobs: Vec<ProfileApplyJobDto>,
    pub skipped: Vec<SkippedProfilePackageDto>,
}

/// Turns a reviewed plan + the user's selection into the list of
/// `(package_id, version)` jobs to enqueue and the skipped list.
///
/// - Required packages are always applied (deselection cannot drop them).
/// - Optional packages are applied only when their ID is in `selected`.
/// - Optional packages not selected are recorded as `Deselected`; packages
///   the resolver already skipped are carried over unchanged.
///
/// Pure and injectable-free so the selection policy is testable with a
/// seeded plan and no engine/network.
pub(crate) fn plan_profile_jobs(
    plan: &ProfileResolutionDto,
    selected: &[String],
) -> (
    Vec<ResolvedProfilePackageDto>,
    Vec<SkippedProfilePackageDto>,
) {
    let mut jobs = Vec::new();
    let mut skipped = plan.skipped.clone();
    for package in &plan.packages {
        if package.required || selected.contains(&package.package_id) {
            jobs.push(package.clone());
        } else {
            skipped.push(SkippedProfilePackageDto {
                package_id: package.package_id.clone(),
                reason: ProfileSkipReasonDto::Deselected,
            });
        }
    }
    (jobs, skipped)
}

fn policy_label(policy: &VersionPolicy) -> &'static str {
    match policy {
        VersionPolicy::Exact { .. } => "exact",
        VersionPolicy::Stable => "stable",
        VersionPolicy::Latest { .. } => "latest",
        VersionPolicy::Channel { .. } => "channel",
    }
}

fn to_dto(resolution: CoreProfileResolution) -> ProfileResolutionDto {
    ProfileResolutionDto {
        profile_id: resolution.profile_id,
        packages: resolution
            .packages
            .into_iter()
            .map(|p| ResolvedProfilePackageDto {
                package_id: p.package_id,
                required: p.required,
                policy: policy_label(&p.policy).to_owned(),
                version: p.version,
            })
            .collect(),
        skipped: resolution
            .skipped
            .into_iter()
            .map(|s| SkippedProfilePackageDto {
                package_id: s.package_id,
                reason: match s.reason {
                    SkipReason::PackageUnavailable => ProfileSkipReasonDto::PackageUnavailable,
                    SkipReason::NoMatchingVersion => ProfileSkipReasonDto::NoMatchingVersion,
                },
            })
            .collect(),
    }
}

fn map_resolve_error(error: ProfileResolveError) -> BridgeError {
    from_engine_code(error.code(), error.to_string())
}

/// Resolves a cached profile into a reviewable plan against the given
/// connection. Testable with a seeded temp database (no process-global engine,
/// no network); the public [`resolve_profile`] op delegates here with the
/// engine's database connection.
pub(crate) fn resolve_profile_in(
    db: &Database,
    profile_id: &str,
) -> Result<ProfileResolutionDto, BridgeError> {
    let conn = db.connection();
    let cached = devkit_registry::profile_cache::ProfileCache::new(conn)
        .get(profile_id)
        .map_err(from_registry_error)?
        .ok_or_else(|| {
            bridge_error(
                BridgeErrorCode::PackageNotFound,
                format!("profile {profile_id} is not in the catalog cache"),
            )
        })?;
    let manifest = cached.manifest;
    let ids: Vec<&str> = manifest
        .packages
        .iter()
        .map(|p| p.package_id.as_str())
        .collect();
    let source = CacheReleaseSource::load(db, &ids).map_err(from_registry_error)?;
    core_resolve_profile(&manifest, &source)
        .map(to_dto)
        .map_err(map_resolve_error)
}

/// Resolves a cached profile into a concrete, reviewable install plan.
/// Reads only the local catalog cache (offline-safe). Unknown profile →
/// `PACKAGE_NOT_FOUND`; unresolved required package → `VERSION_NOT_FOUND` /
/// `PACKAGE_NOT_FOUND`; optional failures appear in `skipped`, never as errors.
pub fn resolve_profile(profile_id: String) -> Result<ProfileResolutionDto, BridgeError> {
    let db = super::engine::open_db()?;
    resolve_profile_in(&db, &profile_id)
}

/// Applies a reviewed plan: enqueues one real install job per selected
/// package and returns a summary for the UI to track (per-package job IDs).
/// `enqueue` is injected so the selection/enqueue mapping is testable without
/// the process-global engine or network. Required packages are always
/// enqueued; deselected/unresolvable optionals are reported in `skipped`.
pub(crate) fn apply_resolved<F>(
    plan: &ProfileResolutionDto,
    selected: &[String],
    mut enqueue: F,
) -> Result<ProfileApplyResultDto, BridgeError>
where
    F: FnMut(&str, &str) -> Result<String, BridgeError>,
{
    let (jobs, skipped) = plan_profile_jobs(plan, selected);
    let mut enqueued = Vec::new();
    for package in jobs {
        let job_id = enqueue(&package.package_id, &package.version)?;
        enqueued.push(ProfileApplyJobDto {
            package_id: package.package_id,
            version: package.version,
            job_id,
        });
    }
    Ok(ProfileApplyResultDto {
        profile_id: plan.profile_id.clone(),
        jobs: enqueued,
        skipped,
    })
}

/// Applies a profile from its reviewed plan: resolves it again against the
/// cache (concrete versions are engine-authoritative, never UI-supplied),
/// enqueues a real install job per selected package, and returns the job list
/// to watch plus everything skipped. Installing does **not** activate a
/// version or touch PATH — those remain the separate consent-gated P3 ops.
pub fn apply_profile(
    profile_id: String,
    selected_package_ids: Vec<String>,
) -> Result<ProfileApplyResultDto, BridgeError> {
    let db = super::engine::open_db()?;
    let plan = resolve_profile_in(&db, &profile_id)?;
    apply_resolved(&plan, &selected_package_ids, |package_id, version| {
        super::jobs::enqueue_real_install(package_id, version)
    })
}

/// Lists user-created/user-edited personal profiles (`origin = 'user'`),
/// ordered by stable ID. Catalog profiles are re-derived on sync and are not
/// editable — they are managed by `list_catalog_profiles`/`sync_catalog`.
pub fn list_personal_profiles() -> Result<Vec<super::catalog::CatalogProfileDto>, BridgeError> {
    let db = super::engine::open_db()?;
    let cache = devkit_registry::profile_cache::ProfileCache::new(db.connection());
    let profiles = cache.list_user().map_err(from_registry_error)?;
    Ok(profiles
        .into_iter()
        .map(|cached| super::catalog::profile_dto(cached.manifest))
        .collect())
}

/// Saves a personal profile from a manifest JSON document. The manifest is
/// parsed and validated in Rust (stable ID, unique packages, policy/version/
/// channel consistency — schemas/profile.schema.json); it is stored with
/// `origin = 'user'` and survives catalog re-sync. A catalog profile with the
/// same ID is never overwritten (`INVALID_ARGUMENT`). Returns the saved DTO.
pub fn save_personal_profile(
    manifest_json: String,
) -> Result<super::catalog::CatalogProfileDto, BridgeError> {
    use devkit_core::manifest::ProfileManifest;
    let manifest = ProfileManifest::from_json(&manifest_json)
        .map_err(|error| bridge_error(BridgeErrorCode::InvalidArgument, error.to_string()))?;
    let db = super::engine::open_db()?;
    let cache = devkit_registry::profile_cache::ProfileCache::new(db.connection());
    // Never let a user profile replace a catalog-origin profile.
    if let Some(existing) = cache.get(&manifest.id).map_err(from_registry_error)? {
        if existing.origin != "user" {
            return Err(bridge_error(
                BridgeErrorCode::InvalidArgument,
                "a catalog profile with this id cannot be overwritten",
            ));
        }
    }
    cache
        .upsert(&manifest, "user")
        .map_err(from_registry_error)?;
    Ok(super::catalog::profile_dto(manifest))
}

/// Deletes a personal profile by ID. Returns `true` when a user-owned profile
/// was removed, `false` when the ID is unknown or is a catalog profile (which
/// can never be deleted through the editor).
pub fn delete_personal_profile(id: String) -> Result<bool, BridgeError> {
    let db = super::engine::open_db()?;
    devkit_registry::profile_cache::ProfileCache::new(db.connection())
        .remove_user(&id)
        .map_err(from_registry_error)
}

/// One suggested pin in a project plan (P4-05).
#[derive(Debug, Clone)]
pub struct ProjectPinDto {
    pub package_id: String,
    pub version: String,
    /// `project` / `profile` / `global` — where the version came from.
    pub source: String,
    pub current_active: Option<String>,
    /// True when matching the project needs an install/activate change.
    pub change_needed: bool,
}

/// A reviewable project plan. Nothing is applied by planning.
#[derive(Debug, Clone)]
pub struct ProjectPlanDto {
    pub project_name: String,
    pub pins: Vec<ProjectPinDto>,
    pub unresolved: Vec<String>,
}

/// Plans a project's version pins against the catalog cache, an optional
/// selected profile, and the global active versions (P4-05). Pure read —
/// **never** applies anything; applying a suggestion goes through the existing
/// consent-gated activation/install ops. Resolution precedence: project pin >
/// profile policy > global active version; exact project pins always win.
/// `project_json` is parsed/validated in Rust (schemas/project.schema.json).
pub fn plan_project(
    project_json: String,
    profile_id: Option<String>,
) -> Result<ProjectPlanDto, BridgeError> {
    let db = super::engine::open_db()?;
    plan_project_in(&db, &project_json, profile_id.as_deref())
}

/// Testable core of [`plan_project`] against a supplied database (no global
/// engine, no network). Parses/validates the manifest, reads the optional
/// profile, global active versions, and cached releases, then resolves.
pub(crate) fn plan_project_in(
    db: &Database,
    project_json: &str,
    profile_id: Option<&str>,
) -> Result<ProjectPlanDto, BridgeError> {
    let project = ProjectManifest::from_json(project_json)
        .map_err(|error| bridge_error(BridgeErrorCode::InvalidArgument, error.to_string()))?;

    // Optional selected profile from the cache.
    let profile = match profile_id {
        Some(id) => devkit_registry::profile_cache::ProfileCache::new(db.connection())
            .get(id)
            .map_err(from_registry_error)?
            .map(|cached| cached.manifest),
        None => None,
    };

    // Global active versions from the installed registry.
    let repo = devkit_registry::installed::InstalledRepo::new(db.connection());
    let mut global_active = HashMap::new();
    for requirement in &project.packages {
        if let Some(version) = repo
            .active_version(&requirement.package_id)
            .map_err(from_registry_error)?
        {
            global_active.insert(requirement.package_id.clone(), version);
        }
    }

    // A single cache-backed release source serves both project and profile.
    let source = CacheReleaseSource::load(
        db,
        &project
            .packages
            .iter()
            .map(|p| p.package_id.as_str())
            .collect::<Vec<_>>(),
    )
    .map_err(from_registry_error)?;

    let plan = core_resolve_project(&project, profile.as_ref(), &source, &source, &global_active)
        .map_err(|error| from_engine_code(error.code(), error.to_string()))?;
    Ok(ProjectPlanDto {
        project_name: plan.project_name,
        pins: plan
            .pins
            .into_iter()
            .map(|pin| ProjectPinDto {
                package_id: pin.package_id,
                version: pin.version,
                source: pin.source.as_str().to_owned(),
                current_active: pin.current_active,
                change_needed: pin.change_needed,
            })
            .collect(),
        unresolved: plan.unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use devkit_core::manifest::{
        Category, Channel, PackageManifest, PackageVersion, Platform, ProfileManifest,
        ProfilePackage, VersionPolicyName, SUPPORTED_SCHEMA_VERSION,
    };
    use devkit_registry::package_cache::PackageCache;
    use devkit_registry::profile_cache::ProfileCache;
    use devkit_registry::Database;
    use tempfile::TempDir;

    fn test_db() -> (TempDir, Database) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state.sqlite");
        let db = devkit_registry::Database::open(&path).unwrap();
        (dir, db)
    }

    fn package(id: &str, versions: &[(&str, Channel, &str)]) -> PackageManifest {
        PackageManifest {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            id: id.to_owned(),
            name: id.to_owned(),
            description: String::new(),
            publisher: String::new(),
            homepage: String::new(),
            license_expression: String::new(),
            maintainer: String::new(),
            category: Category::Other,
            supported_platforms: vec![Platform::WindowsX64],
            default_version_policy: VersionPolicyName::Stable,
            versions: versions
                .iter()
                .map(|(version, channel, date)| PackageVersion {
                    version: (*version).to_owned(),
                    channel: *channel,
                    release_date: Some((*date).to_owned()),
                    release_notes_url: None,
                    dependencies: Vec::new(),
                    artifacts: Vec::new(),
                })
                .collect(),
            updated_at: String::new(),
        }
    }

    fn profile(id: &str, selections: Vec<ProfilePackage>) -> ProfileManifest {
        ProfileManifest {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            id: id.to_owned(),
            name: id.to_owned(),
            description: String::new(),
            maintainer: String::new(),
            supported_platforms: Vec::new(),
            packages: selections,
        }
    }

    fn selection(
        id: &str,
        required: bool,
        policy: VersionPolicyName,
        version: Option<&str>,
    ) -> ProfilePackage {
        ProfilePackage {
            package_id: id.to_owned(),
            required,
            version_policy: policy,
            version: version.map(str::to_owned),
            channel: None,
            reason: String::new(),
        }
    }

    #[test]
    fn resolves_a_seeded_profile_from_the_cache_into_a_reviewable_plan() {
        let (_dir, db) = test_db();
        let conn = db.connection();
        // Seed versions newest-first by release_date; store them deliberately
        // out of date order to prove the adapter re-sorts newest-first.
        let packages = vec![
            package(
                "devkit.lang.python",
                &[
                    ("3.12.6", Channel::Stable, "2024-08-01"),
                    ("3.13.0rc1", Channel::Beta, "2024-11-01"),
                    ("3.12.7", Channel::Stable, "2024-10-01"),
                ],
            ),
            package(
                "devkit.utility.git",
                &[("2.48.0", Channel::Stable, "2024-09-01")],
            ),
        ];
        for manifest in &packages {
            PackageCache::new(conn).upsert(manifest).unwrap();
        }
        ProfileCache::new(conn)
            .upsert(
                &profile(
                    "devkit.profile.test",
                    vec![
                        selection("devkit.lang.python", true, VersionPolicyName::Stable, None),
                        selection(
                            "devkit.utility.git",
                            true,
                            VersionPolicyName::Exact,
                            Some("2.48.0"),
                        ),
                    ],
                ),
                "catalog",
            )
            .unwrap();

        let plan = resolve_profile_in(&db, "devkit.profile.test").unwrap();
        assert_eq!(plan.profile_id, "devkit.profile.test");
        assert_eq!(plan.packages.len(), 2);
        assert_eq!(plan.packages[0].package_id, "devkit.lang.python");
        assert_eq!(plan.packages[0].version, "3.12.7"); // newest stable (rc1 skipped)
        assert_eq!(plan.packages[0].policy, "stable");
        assert_eq!(plan.packages[1].version, "2.48.0");
        assert_eq!(plan.packages[1].policy, "exact");
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn optional_unresolvable_package_is_skipped_in_the_plan() {
        let (_dir, db) = test_db();
        let conn = db.connection();
        PackageCache::new(conn)
            .upsert(&package(
                "devkit.lang.python",
                &[("3.12.7", Channel::Stable, "2024-10-01")],
            ))
            .unwrap();
        ProfileCache::new(conn)
            .upsert(
                &profile(
                    "devkit.profile.optional",
                    vec![
                        selection("devkit.lang.python", true, VersionPolicyName::Stable, None),
                        selection(
                            "devkit.unknown.package",
                            false,
                            VersionPolicyName::Stable,
                            None,
                        ),
                    ],
                ),
                "catalog",
            )
            .unwrap();

        let plan = resolve_profile_in(&db, "devkit.profile.optional").unwrap();
        assert_eq!(plan.packages.len(), 1);
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].package_id, "devkit.unknown.package");
        assert_eq!(
            format!("{:?}", plan.skipped[0].reason),
            "PackageUnavailable"
        );
    }

    #[test]
    fn required_unresolvable_package_fails_with_stable_code() {
        let (_dir, db) = test_db();
        let conn = db.connection();
        PackageCache::new(conn)
            .upsert(&package(
                "devkit.lang.python",
                &[("3.12.7", Channel::Stable, "2024-10-01")],
            ))
            .unwrap();
        ProfileCache::new(conn)
            .upsert(
                &profile(
                    "devkit.profile.required",
                    vec![selection(
                        "devkit.lang.python",
                        true,
                        VersionPolicyName::Exact,
                        Some("9.9.9"),
                    )],
                ),
                "catalog",
            )
            .unwrap();

        let error = resolve_profile_in(&db, "devkit.profile.required").unwrap_err();
        assert_eq!(error.code_str(), "VERSION_NOT_FOUND");
    }

    #[test]
    fn unknown_profile_fails_with_package_not_found() {
        let (_dir, db) = test_db();
        let error = resolve_profile_in(&db, "devkit.profile.missing").unwrap_err();
        assert_eq!(error.code_str(), "PACKAGE_NOT_FOUND");
    }

    fn test_plan() -> ProfileResolutionDto {
        ProfileResolutionDto {
            profile_id: "devkit.profile.apply".to_owned(),
            packages: vec![
                ResolvedProfilePackageDto {
                    package_id: "devkit.lang.python".to_owned(),
                    required: true,
                    policy: "exact".to_owned(),
                    version: "3.12.7".to_owned(),
                },
                ResolvedProfilePackageDto {
                    package_id: "devkit.ide.vscode".to_owned(),
                    required: false,
                    policy: "latest".to_owned(),
                    version: "1.95.0".to_owned(),
                },
            ],
            skipped: vec![SkippedProfilePackageDto {
                package_id: "devkit.unknown".to_owned(),
                reason: ProfileSkipReasonDto::PackageUnavailable,
            }],
        }
    }

    #[test]
    fn plan_jobs_always_includes_required_and_only_selected_optionals() {
        let plan = test_plan();
        // Nothing selected → only the required package is planned; the
        // optional is skipped as Deselected and the resolver skip is kept.
        let (jobs, skipped) = plan_profile_jobs(&plan, &[]);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].package_id, "devkit.lang.python");
        assert_eq!(skipped.len(), 2);
        assert!(skipped.iter().any(|s| s.package_id == "devkit.ide.vscode"
            && matches!(s.reason, ProfileSkipReasonDto::Deselected)));
        assert!(skipped.iter().any(|s| s.package_id == "devkit.unknown"));
    }

    #[test]
    fn plan_jobs_includes_an_optional_when_selected() {
        let plan = test_plan();
        let (jobs, skipped) = plan_profile_jobs(&plan, &["devkit.ide.vscode".to_owned()]);
        assert_eq!(jobs.len(), 2);
        assert_eq!(skipped.len(), 1); // only the resolver skip remains
        assert_eq!(skipped[0].package_id, "devkit.unknown");
    }

    #[test]
    fn apply_resolved_enqueues_one_job_per_planned_package() {
        let plan = test_plan();
        let mut calls = Vec::new();
        let result = apply_resolved(&plan, &["devkit.ide.vscode".to_owned()], |id, version| {
            calls.push((id.to_owned(), version.to_owned()));
            Ok(format!("job-{}", calls.len()))
        })
        .unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0],
            ("devkit.lang.python".to_owned(), "3.12.7".to_owned())
        );
        assert_eq!(result.jobs.len(), 2);
        assert_eq!(result.jobs[0].job_id, "job-1");
        assert_eq!(result.jobs[1].job_id, "job-2");
        assert_eq!(result.profile_id, "devkit.profile.apply");
        assert_eq!(result.skipped.len(), 1);
    }

    #[test]
    fn apply_resolved_propagates_enqueue_failure() {
        let plan = test_plan();
        let error = apply_resolved(&plan, &[], |_id, _version| {
            Err(bridge_error(
                BridgeErrorCode::InstallFailed,
                "enqueue failed",
            ))
        })
        .unwrap_err();
        assert_eq!(error.code_str(), "INSTALL_FAILED");
    }

    fn project_json(id: &str, policy: &str, version: Option<&str>) -> String {
        let mut pin = serde_json::json!({
            "package_id": id,
            "required": true,
            "version_policy": policy,
        });
        if let Some(version) = version {
            pin["version"] = serde_json::Value::String(version.to_owned());
        }
        serde_json::json!({
            "schema_version": 1,
            "name": "Test Project",
            "packages": [pin],
        })
        .to_string()
    }

    #[test]
    fn plan_project_reads_active_version_and_flags_change() {
        let (_dir, db) = test_db();
        let conn = db.connection();
        PackageCache::new(conn)
            .upsert(&package(
                "devkit.lang.python",
                &[
                    ("3.12.6", Channel::Stable, "2024-08-01"),
                    ("3.12.7", Channel::Stable, "2024-10-01"),
                ],
            ))
            .unwrap();
        // Seed an installed row + active mapping at an older version so the
        // project's stable pin suggests a change.
        conn.execute(
            "INSERT INTO installed_versions (package_id, version, install_path, status, receipt, installed_at) \
             VALUES ('devkit.lang.python', '3.12.6', 'packages/devkit.lang.python/3.12.6', 'installed', '{}', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        devkit_registry::installed::InstalledRepo::new(conn)
            .set_active("devkit.lang.python", "3.12.6")
            .unwrap();

        let plan = plan_project_in(
            &db,
            &project_json("devkit.lang.python", "stable", None),
            None,
        )
        .unwrap();
        assert_eq!(plan.project_name, "Test Project");
        assert_eq!(plan.pins.len(), 1);
        assert_eq!(plan.pins[0].package_id, "devkit.lang.python");
        assert_eq!(plan.pins[0].version, "3.12.7"); // newest stable
        assert_eq!(plan.pins[0].source, "project");
        assert_eq!(plan.pins[0].current_active.as_deref(), Some("3.12.6"));
        assert!(plan.pins[0].change_needed);
    }

    #[test]
    fn plan_project_rejects_invalid_manifest() {
        let (_dir, db) = test_db();
        // exact policy without a version is invalid.
        let bad = r#"{"schema_version":1,"name":"Bad","packages":[{"package_id":"devkit.lang.python","required":true,"version_policy":"exact"}]}"#;
        let error = plan_project_in(&db, bad, None).unwrap_err();
        assert_eq!(error.code_str(), "INVALID_ARGUMENT");
    }
}
