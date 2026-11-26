//! Project-level version pinning (Phase 4, P4-05).
//!
//! Implements the documented extension in version-management.md §5: a
//! project manifest (`schemas/project.schema.json`) declares package version
//! requirements; DevKit **suggests** environment changes when a project is
//! opened but never applies them automatically (the consent flow is the same
//! consent-gated activation/install ops from Phase 3).
//!
//! Resolution precedence (highest first):
//! 1. **Project pin** — an exact project pin always wins.
//! 2. **Profile policy** — a selected profile's policy for the package.
//! 3. **Global active version** — the version currently active (no change).
//!
//! The module is pure: no network, filesystem, or process access. Available
//! releases, the optional profile, and the global active versions are supplied
//! by the caller (in production the bridge reads the catalog cache and the
//! installed registry), so the engine is testable with in-memory data only.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;
use crate::manifest::{ProfileManifest, VersionPolicyName};
use crate::profile::ProfileReleaseSource;
use crate::version::{resolve, VersionPolicy};

impl ProjectManifest {
    /// Parse and validate a project manifest document.
    pub fn from_json(json: &str) -> Result<Self, ProjectResolveError> {
        let project: Self =
            serde_json::from_str(json).map_err(|error| ProjectResolveError::InvalidPin {
                package_id: String::new(),
                reason: format!("invalid project manifest: {error}"),
            })?;
        project.validate()?;
        Ok(project)
    }

    /// Semantic validation beyond what serde enforces.
    pub fn validate(&self) -> Result<(), ProjectResolveError> {
        if self.schema_version != crate::manifest::SUPPORTED_SCHEMA_VERSION {
            return Err(ProjectResolveError::InvalidPin {
                package_id: String::new(),
                reason: format!("unsupported schema version {}", self.schema_version),
            });
        }
        if self.name.trim().is_empty() {
            return Err(ProjectResolveError::InvalidPin {
                package_id: String::new(),
                reason: "project name is required".to_owned(),
            });
        }
        if self.packages.is_empty() {
            return Err(ProjectResolveError::InvalidPin {
                package_id: String::new(),
                reason: "project has no package pins".to_owned(),
            });
        }
        let mut seen = std::collections::HashSet::new();
        for package in &self.packages {
            if !seen.insert(&package.package_id) {
                return Err(ProjectResolveError::InvalidPin {
                    package_id: package.package_id.clone(),
                    reason: "duplicate package pin".to_owned(),
                });
            }
            // Policy/version/channel consistency is checked again during
            // resolution via `selection_policy`, but validate eagerly so a
            // malformed manifest is rejected before any cache access.
            selection_policy(
                &package.package_id,
                package.version_policy,
                package.version.as_deref(),
                package.channel.as_deref(),
            )?;
        }
        Ok(())
    }
}

/// A project package requirement (schemas/project.schema.json).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPackage {
    pub package_id: String,
    /// A required pin that cannot resolve fails the plan; an optional one is
    /// reported as unresolved and skipped.
    pub required: bool,
    pub version_policy: VersionPolicyName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

/// A project manifest (schemas/project.schema.json).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub schema_version: u16,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub packages: Vec<ProjectPackage>,
}

/// Why a suggested version was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinSource {
    /// An exact (or any) pin declared in the project — highest precedence.
    ProjectPin,
    /// A policy inherited from the selected profile.
    ProfilePolicy,
    /// The currently active version — no change needed.
    GlobalActive,
}

impl PinSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PinSource::ProjectPin => "project",
            PinSource::ProfilePolicy => "profile",
            PinSource::GlobalActive => "global",
        }
    }
}

/// One resolved package requirement in a project plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPin {
    pub package_id: String,
    /// The concrete suggested version.
    pub version: String,
    /// Where the version came from.
    pub source: PinSource,
    /// The currently active version, when any.
    pub current_active: Option<String>,
    /// True when the suggestion differs from the global active version and
    /// the user would need to install/activate to match the project.
    pub change_needed: bool,
}

/// The reviewable result of resolving a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPlan {
    pub project_name: String,
    pub pins: Vec<ProjectPin>,
    /// Optional requirements that could not be resolved (informational).
    pub unresolved: Vec<String>,
}

/// A failure to resolve a required pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectResolveError {
    /// The declarative fields disagree (e.g. exact with no version).
    InvalidPin { package_id: String, reason: String },
    /// A required package has no releases or no matching version.
    RequiredUnresolved { package_id: String },
}

impl ProjectResolveError {
    pub fn code(&self) -> ErrorCode {
        ErrorCode::InvalidArgument
    }
}

impl std::fmt::Display for ProjectResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectResolveError::InvalidPin { package_id, reason } => {
                write!(f, "invalid pin for {package_id}: {reason}")
            }
            ProjectResolveError::RequiredUnresolved { package_id } => {
                write!(f, "required project pin {package_id} cannot be resolved")
            }
        }
    }
}

impl std::error::Error for ProjectResolveError {}

/// Maps a project selection's declarative fields to a runtime `VersionPolicy`.
fn selection_policy(
    package_id: &str,
    policy: VersionPolicyName,
    version: Option<&str>,
    channel: Option<&str>,
) -> Result<VersionPolicy, ProjectResolveError> {
    let invalid = |reason: &str| ProjectResolveError::InvalidPin {
        package_id: package_id.to_owned(),
        reason: reason.to_owned(),
    };
    match policy {
        VersionPolicyName::Exact => {
            let version = version.ok_or_else(|| invalid("exact pin requires a version"))?;
            Ok(VersionPolicy::Exact {
                version: version.to_owned(),
            })
        }
        VersionPolicyName::Stable => Ok(VersionPolicy::Stable),
        VersionPolicyName::Latest => Ok(VersionPolicy::Latest {
            allow_prerelease: false,
        }),
        VersionPolicyName::Channel => {
            let channel = channel.ok_or_else(|| invalid("channel pin requires a channel"))?;
            Ok(VersionPolicy::Channel {
                channel: channel.to_owned(),
            })
        }
    }
}

/// Finds a profile selection for `package_id`, returning its policy parts.
fn profile_selection<'a>(
    profile: Option<&'a ProfileManifest>,
    package_id: &str,
) -> Option<(VersionPolicyName, Option<&'a str>, Option<&'a str>)> {
    let profile = profile?;
    let selection = profile
        .packages
        .iter()
        .find(|p| p.package_id == package_id)?;
    Some((
        selection.version_policy,
        selection.version.as_deref(),
        selection.channel.as_deref(),
    ))
}

/// Resolves a project into a reviewable plan.
///
/// `project_releases` supplies releases for packages the project pins;
/// `profile_releases` (usually the same source) for profile-inherited
/// policies. `global_active` maps package IDs to their active version.
pub fn resolve_project(
    project: &ProjectManifest,
    profile: Option<&ProfileManifest>,
    project_releases: &impl ProfileReleaseSource,
    profile_releases: &impl ProfileReleaseSource,
    global_active: &HashMap<String, String>,
) -> Result<ProjectPlan, ProjectResolveError> {
    let mut pins = Vec::new();
    let mut unresolved = Vec::new();

    for requirement in &project.packages {
        let current = global_active.get(&requirement.package_id).cloned();
        // Precedence: project pin > profile policy > global active.
        let policy = selection_policy(
            &requirement.package_id,
            requirement.version_policy,
            requirement.version.as_deref(),
            requirement.channel.as_deref(),
        )?;

        // Precedence: project pin > profile policy > global active.
        // Resolve returns (suggested version, its source).
        let (suggested, source): (Option<String>, PinSource) =
            match resolve(project_releases.releases(&requirement.package_id), &policy) {
                Ok(resolved) => (Some(resolved.version), PinSource::ProjectPin),
                Err(_) => {
                    // Project pin could not resolve with these releases. Try
                    // the profile's policy before falling back to global.
                    if let Some((p_policy, p_version, p_channel)) =
                        profile_selection(profile, &requirement.package_id)
                    {
                        let p_runtime = selection_policy(
                            &requirement.package_id,
                            p_policy,
                            p_version,
                            p_channel,
                        )?;
                        match resolve(
                            profile_releases.releases(&requirement.package_id),
                            &p_runtime,
                        ) {
                            Ok(resolved) => (Some(resolved.version), PinSource::ProfilePolicy),
                            Err(_) => (None, PinSource::GlobalActive),
                        }
                    } else {
                        (None, PinSource::GlobalActive)
                    }
                }
            };

        let version = match suggested {
            Some(version) => version,
            None => {
                if let Some(active) = current.clone() {
                    // Fall back to the global active version (no change).
                    pins.push(ProjectPin {
                        package_id: requirement.package_id.clone(),
                        version: active.clone(),
                        source: PinSource::GlobalActive,
                        current_active: Some(active),
                        change_needed: false,
                    });
                    continue;
                }
                if requirement.required {
                    return Err(ProjectResolveError::RequiredUnresolved {
                        package_id: requirement.package_id.clone(),
                    });
                }
                unresolved.push(requirement.package_id.clone());
                continue;
            }
        };

        let change_needed = current.as_deref() != Some(version.as_str());
        pins.push(ProjectPin {
            package_id: requirement.package_id.clone(),
            version,
            source,
            current_active: current,
            change_needed,
        });
    }

    Ok(ProjectPlan {
        project_name: project.name.clone(),
        pins,
        unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Platform, ProfilePackage, SUPPORTED_SCHEMA_VERSION};
    use crate::version::Release;

    struct Releases {
        by_package: HashMap<String, Vec<Release>>,
    }

    impl ProfileReleaseSource for Releases {
        fn releases(&self, package_id: &str) -> &[Release] {
            self.by_package
                .get(package_id)
                .map(Vec::as_slice)
                .unwrap_or(&[])
        }
    }

    fn release(version: &str, channel: &str, prerelease: bool) -> Release {
        Release {
            version: version.to_owned(),
            channel: channel.to_owned(),
            prerelease,
        }
    }

    fn sources() -> (Releases, Releases) {
        let mut by_package = HashMap::new();
        by_package.insert(
            "devkit.lang.python".to_owned(),
            vec![
                release("3.13.0rc1", "beta", true),
                release("3.12.7", "stable", false),
                release("3.12.6", "stable", false),
            ],
        );
        by_package.insert(
            "devkit.utility.git".to_owned(),
            vec![release("2.48.0", "stable", false)],
        );
        let source = Releases {
            by_package: by_package.clone(),
        };
        (source, Releases { by_package })
    }

    fn requirement(
        id: &str,
        required: bool,
        policy: VersionPolicyName,
        version: Option<&str>,
    ) -> ProjectPackage {
        ProjectPackage {
            package_id: id.to_owned(),
            required,
            version_policy: policy,
            version: version.map(str::to_owned),
            channel: None,
        }
    }

    fn project(packages: Vec<ProjectPackage>) -> ProjectManifest {
        ProjectManifest {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            name: "Test Project".to_owned(),
            description: String::new(),
            packages,
        }
    }

    fn profile(packages: Vec<ProfilePackage>) -> ProfileManifest {
        ProfileManifest {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            id: "devkit.profile.test".to_owned(),
            name: "Test".to_owned(),
            description: String::new(),
            maintainer: String::new(),
            supported_platforms: Vec::<Platform>::new(),
            packages,
        }
    }

    #[test]
    fn exact_project_pin_wins_over_profile_and_global() {
        let (project_src, profile_src) = sources();
        let mut active = HashMap::new();
        active.insert("devkit.lang.python".to_owned(), "3.11.0".to_owned());
        let prof = profile(vec![ProfilePackage {
            package_id: "devkit.lang.python".to_owned(),
            required: true,
            version_policy: VersionPolicyName::Stable,
            version: None,
            channel: None,
            reason: String::new(),
        }]);
        let plan = resolve_project(
            &project(vec![requirement(
                "devkit.lang.python",
                true,
                VersionPolicyName::Exact,
                Some("3.12.6"),
            )]),
            Some(&prof),
            &project_src,
            &profile_src,
            &active,
        )
        .unwrap();
        assert_eq!(plan.pins[0].version, "3.12.6");
        assert_eq!(plan.pins[0].source, PinSource::ProjectPin);
        assert_eq!(plan.pins[0].current_active.as_deref(), Some("3.11.0"));
        assert!(plan.pins[0].change_needed);
    }

    #[test]
    fn global_active_used_when_project_pin_cannot_resolve() {
        let (project_src, profile_src) = sources();
        let mut active = HashMap::new();
        active.insert("devkit.lang.python".to_owned(), "3.12.6".to_owned());
        // Exact version 9.9.9 has no release; no profile → fall back to global.
        let plan = resolve_project(
            &project(vec![requirement(
                "devkit.lang.python",
                true,
                VersionPolicyName::Exact,
                Some("9.9.9"),
            )]),
            None,
            &project_src,
            &profile_src,
            &active,
        )
        .unwrap();
        assert_eq!(plan.pins[0].version, "3.12.6");
        assert_eq!(plan.pins[0].source, PinSource::GlobalActive);
        assert!(!plan.pins[0].change_needed);
    }

    #[test]
    fn profile_policy_used_when_project_pin_unresolvable() {
        let (project_src, profile_src) = sources();
        let active = HashMap::new();
        let prof = profile(vec![ProfilePackage {
            package_id: "devkit.lang.python".to_owned(),
            required: true,
            version_policy: VersionPolicyName::Stable,
            version: None,
            channel: None,
            reason: String::new(),
        }]);
        let plan = resolve_project(
            &project(vec![requirement(
                "devkit.lang.python",
                true,
                VersionPolicyName::Exact,
                Some("9.9.9"),
            )]),
            Some(&prof),
            &project_src,
            &profile_src,
            &active,
        )
        .unwrap();
        assert_eq!(plan.pins[0].version, "3.12.7"); // newest stable via profile policy
        assert_eq!(plan.pins[0].source, PinSource::ProfilePolicy);
    }

    #[test]
    fn required_unresolvable_pin_fails() {
        let (project_src, profile_src) = sources();
        let error = resolve_project(
            &project(vec![requirement(
                "devkit.utility.git",
                true,
                VersionPolicyName::Exact,
                Some("9.9.9"),
            )]),
            None,
            &project_src,
            &profile_src,
            &HashMap::new(),
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidArgument);
        assert!(matches!(
            error,
            ProjectResolveError::RequiredUnresolved { .. }
        ));
    }

    #[test]
    fn optional_unresolvable_pin_is_reported_not_fatal() {
        let (project_src, profile_src) = sources();
        let plan = resolve_project(
            &project(vec![requirement(
                "devkit.unknown",
                false,
                VersionPolicyName::Stable,
                None,
            )]),
            None,
            &project_src,
            &profile_src,
            &HashMap::new(),
        )
        .unwrap();
        assert!(plan.pins.is_empty());
        assert_eq!(plan.unresolved, vec!["devkit.unknown".to_owned()]);
    }

    #[test]
    fn exact_without_version_is_invalid() {
        let (project_src, profile_src) = sources();
        let error = resolve_project(
            &project(vec![requirement(
                "devkit.lang.python",
                true,
                VersionPolicyName::Exact,
                None,
            )]),
            None,
            &project_src,
            &profile_src,
            &HashMap::new(),
        )
        .unwrap_err();
        assert!(matches!(error, ProjectResolveError::InvalidPin { .. }));
        assert_eq!(error.code(), ErrorCode::InvalidArgument);
    }

    #[test]
    fn change_needed_false_when_project_matches_global() {
        let (project_src, profile_src) = sources();
        let mut active = HashMap::new();
        active.insert("devkit.lang.python".to_owned(), "3.12.7".to_owned());
        let plan = resolve_project(
            &project(vec![requirement(
                "devkit.lang.python",
                true,
                VersionPolicyName::Stable,
                None,
            )]),
            None,
            &project_src,
            &profile_src,
            &active,
        )
        .unwrap();
        assert_eq!(plan.pins[0].version, "3.12.7");
        assert!(!plan.pins[0].change_needed);
    }
}
