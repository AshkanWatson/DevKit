//! Profile resolution (Phase 4, P4-01).
//!
//! Turns a declarative Developer Profile (`manifest::ProfileManifest`,
//! `schemas/profile.schema.json`) into a concrete, reviewable install plan:
//! each package selection's [`VersionPolicyName`] plus its optional
//! `version`/`channel` is mapped to the runtime [`VersionPolicy`] and resolved
//! against that package's available releases (HANDOFF.md §5,
//! version-management.md §2/§5, DESIGN.md journey B "review the combined
//! installation plan").
//!
//! This module is pure — no network, filesystem, or process access. Available
//! releases are supplied by the caller (in production: the catalog cache via
//! the bridge) through [`ProfileReleaseSource`], so the engine is testable with
//! in-memory data and never touches a real catalog or host. The ordering of
//! releases is the caller's responsibility (newest first), exactly as for
//! [`version::resolve`] — version formats vary by package, so ordering does
//! not live in this layer.

use crate::error::ErrorCode;
use crate::manifest::{ProfileManifest, ProfilePackage, VersionPolicyName};
use crate::version::{resolve, Release, VersionPolicy};

/// Supplies the available releases for a package, in preference order
/// (newest first). Implemented by the catalog-cache adapter at the engine
/// boundary; tests use an in-memory fake. No network or filesystem access.
pub trait ProfileReleaseSource {
    /// Returns `package_id`'s releases, newest first. An empty slice means
    /// the package is unknown/unavailable in the catalog.
    fn releases(&self, package_id: &str) -> &[Release];
}

/// A resolved package selection ready for the combined install plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPackage {
    pub package_id: String,
    /// Whether this package is required (a failed required package fails the
    /// whole resolution; a failed optional package is only skipped).
    pub required: bool,
    /// The runtime policy that was applied (persisted with the selection so
    /// an exact pin is never silently upgraded — HANDOFF.md §5).
    pub policy: VersionPolicy,
    /// The concrete version the policy resolved to.
    pub version: String,
}

/// Why an optional package selection was skipped (it never fails the plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// No releases are available for the package at all.
    PackageUnavailable,
    /// Releases exist but none satisfy the selection's policy.
    NoMatchingVersion,
}

/// An optional package that was left out of the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedPackage {
    pub package_id: String,
    pub reason: SkipReason,
}

/// The reviewable result of resolving a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileResolution {
    pub profile_id: String,
    /// Required and resolvable-optional selections, in profile order.
    pub packages: Vec<ResolvedPackage>,
    /// Optional selections that could not be resolved, in profile order.
    pub skipped: Vec<SkippedPackage>,
}

/// A failure to resolve a profile: either a required package could not be
/// resolved, or a selection's declarative fields are internally inconsistent
/// (the latter cannot happen for a manifest that passed `validate()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileResolveError {
    /// A required package has no releases at all.
    RequiredPackageUnavailable { package_id: String },
    /// A required package has releases but none match its policy.
    RequiredVersionNotFound { package_id: String },
    /// Declarative fields disagree with the policy (e.g. `exact` with no
    /// `version`), so the selection cannot be mapped to a runtime policy.
    InvalidSelection { package_id: String, reason: String },
}

impl ProfileResolveError {
    /// Stable `ErrorCode` for the job event stream / FFI boundary.
    pub fn code(&self) -> ErrorCode {
        match self {
            ProfileResolveError::RequiredPackageUnavailable { .. } => ErrorCode::PackageNotFound,
            ProfileResolveError::RequiredVersionNotFound { .. } => ErrorCode::VersionNotFound,
            ProfileResolveError::InvalidSelection { .. } => ErrorCode::InvalidArgument,
        }
    }
}

impl std::fmt::Display for ProfileResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileResolveError::RequiredPackageUnavailable { package_id } => {
                write!(
                    f,
                    "required package {package_id} is unavailable in the catalog"
                )
            }
            ProfileResolveError::RequiredVersionNotFound { package_id } => {
                write!(
                    f,
                    "no version of required package {package_id} matches its policy"
                )
            }
            ProfileResolveError::InvalidSelection { package_id, reason } => {
                write!(f, "invalid selection for {package_id}: {reason}")
            }
        }
    }
}

impl std::error::Error for ProfileResolveError {}

/// Maps a single declarative selection to the runtime policy that
/// [`resolve`] understands. `exact` needs `version`, `channel` needs
/// `channel`; `stable`/`latest` need neither. A `latest` policy never opts
/// into prereleases from a profile (profiles have no prerelease flag).
pub fn selection_policy(selection: &ProfilePackage) -> Result<VersionPolicy, ProfileResolveError> {
    let invalid = |reason: &str| ProfileResolveError::InvalidSelection {
        package_id: selection.package_id.clone(),
        reason: reason.to_owned(),
    };
    match selection.version_policy {
        VersionPolicyName::Exact => {
            let version = selection
                .version
                .as_deref()
                .ok_or_else(|| invalid("exact policy requires a version"))?;
            Ok(VersionPolicy::Exact {
                version: version.to_owned(),
            })
        }
        VersionPolicyName::Stable => Ok(VersionPolicy::Stable),
        VersionPolicyName::Latest => Ok(VersionPolicy::Latest {
            allow_prerelease: false,
        }),
        VersionPolicyName::Channel => {
            let channel = selection
                .channel
                .as_deref()
                .ok_or_else(|| invalid("channel policy requires a channel"))?;
            Ok(VersionPolicy::Channel {
                channel: channel.to_owned(),
            })
        }
    }
}

/// Resolves `profile` against `source` into a reviewable plan.
///
/// - Required selections always resolve; a required package with no releases
///   fails with `PACKAGE_NOT_FOUND`, and one with no matching version fails
///   with `VERSION_NOT_FOUND`.
/// - Optional selections that cannot resolve are recorded in
///   [`ProfileResolution::skipped`] with a reason; they never fail the plan.
/// - Resolved packages keep the applied [`VersionPolicy`] so the exact pin is
///   preserved (never silently upgraded).
pub fn resolve_profile(
    profile: &ProfileManifest,
    source: &impl ProfileReleaseSource,
) -> Result<ProfileResolution, ProfileResolveError> {
    let mut packages = Vec::new();
    let mut skipped = Vec::new();

    for selection in &profile.packages {
        let policy = selection_policy(selection)?;
        let releases = source.releases(&selection.package_id);
        match resolve(releases, &policy) {
            Ok(resolved) => packages.push(ResolvedPackage {
                package_id: selection.package_id.clone(),
                required: selection.required,
                policy,
                version: resolved.version,
            }),
            Err(_) => {
                if selection.required {
                    return Err(if releases.is_empty() {
                        ProfileResolveError::RequiredPackageUnavailable {
                            package_id: selection.package_id.clone(),
                        }
                    } else {
                        ProfileResolveError::RequiredVersionNotFound {
                            package_id: selection.package_id.clone(),
                        }
                    });
                }
                skipped.push(SkippedPackage {
                    package_id: selection.package_id.clone(),
                    reason: if releases.is_empty() {
                        SkipReason::PackageUnavailable
                    } else {
                        SkipReason::NoMatchingVersion
                    },
                });
            }
        }
    }

    Ok(ProfileResolution {
        profile_id: profile.id.clone(),
        packages,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ProfilePackage, SUPPORTED_SCHEMA_VERSION};

    struct FakeReleases {
        by_package: std::collections::HashMap<String, Vec<Release>>,
    }

    impl ProfileReleaseSource for FakeReleases {
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

    fn source() -> FakeReleases {
        let mut by_package = std::collections::HashMap::new();
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
        FakeReleases { by_package }
    }

    fn selection(
        id: &str,
        required: bool,
        policy: VersionPolicyName,
        version: Option<&str>,
        channel: Option<&str>,
    ) -> ProfilePackage {
        ProfilePackage {
            package_id: id.to_owned(),
            required,
            version_policy: policy,
            version: version.map(str::to_owned),
            channel: channel.map(str::to_owned),
            reason: String::new(),
        }
    }

    fn profile(packages: Vec<ProfilePackage>) -> ProfileManifest {
        ProfileManifest {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            id: "devkit.profile.test".to_owned(),
            name: "Test".to_owned(),
            description: String::new(),
            maintainer: String::new(),
            supported_platforms: Vec::new(),
            packages,
        }
    }

    #[test]
    fn selection_policy_maps_all_four_policy_names() {
        assert_eq!(
            selection_policy(&selection(
                "p",
                true,
                VersionPolicyName::Exact,
                Some("1.2.3"),
                None
            ))
            .unwrap(),
            VersionPolicy::Exact {
                version: "1.2.3".to_owned()
            }
        );
        assert_eq!(
            selection_policy(&selection("p", true, VersionPolicyName::Stable, None, None)).unwrap(),
            VersionPolicy::Stable
        );
        assert_eq!(
            selection_policy(&selection("p", true, VersionPolicyName::Latest, None, None)).unwrap(),
            VersionPolicy::Latest {
                allow_prerelease: false
            }
        );
        assert_eq!(
            selection_policy(&selection(
                "p",
                true,
                VersionPolicyName::Channel,
                None,
                Some("beta")
            ))
            .unwrap(),
            VersionPolicy::Channel {
                channel: "beta".to_owned()
            }
        );
    }

    #[test]
    fn selection_policy_rejects_missing_exact_version_and_channel() {
        let error = selection_policy(&selection("p", true, VersionPolicyName::Exact, None, None))
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidArgument);
        assert!(matches!(
            error,
            ProfileResolveError::InvalidSelection { .. }
        ));

        let error = selection_policy(&selection(
            "p",
            true,
            VersionPolicyName::Channel,
            None,
            None,
        ))
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidArgument);
    }

    #[test]
    fn resolves_all_policies_into_a_reviewable_plan() {
        let prof = profile(vec![
            selection(
                "devkit.lang.python",
                true,
                VersionPolicyName::Exact,
                Some("3.12.6"),
                None,
            ),
            selection(
                "devkit.utility.git",
                true,
                VersionPolicyName::Stable,
                None,
                None,
            ),
        ]);
        let result = resolve_profile(&prof, &source()).unwrap();
        assert_eq!(result.profile_id, "devkit.profile.test");
        assert_eq!(result.packages.len(), 2);
        assert_eq!(result.packages[0].version, "3.12.6");
        assert_eq!(
            result.packages[0].policy,
            VersionPolicy::Exact {
                version: "3.12.6".to_owned()
            }
        );
        assert_eq!(result.packages[1].version, "2.48.0");
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn latest_never_selects_a_prerelease_from_a_profile() {
        let prof = profile(vec![selection(
            "devkit.lang.python",
            true,
            VersionPolicyName::Latest,
            None,
            None,
        )]);
        let result = resolve_profile(&prof, &source()).unwrap();
        // 3.13.0rc1 is first (newest) but prerelease; Latest from a profile
        // must skip it and pick 3.12.7.
        assert_eq!(result.packages[0].version, "3.12.7");
    }

    #[test]
    fn optional_package_with_no_matching_version_is_skipped_not_failed() {
        let prof = profile(vec![
            selection(
                "devkit.lang.python",
                true,
                VersionPolicyName::Stable,
                None,
                None,
            ),
            selection(
                "devkit.utility.git",
                false,
                VersionPolicyName::Exact,
                Some("9.9.9"),
                None,
            ),
        ]);
        let result = resolve_profile(&prof, &source()).unwrap();
        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].package_id, "devkit.utility.git");
        assert_eq!(result.skipped[0].reason, SkipReason::NoMatchingVersion);
    }

    #[test]
    fn optional_unknown_package_is_skipped_as_unavailable() {
        let prof = profile(vec![selection(
            "devkit.unknown.package",
            false,
            VersionPolicyName::Stable,
            None,
            None,
        )]);
        let result = resolve_profile(&prof, &source()).unwrap();
        assert!(result.packages.is_empty());
        assert_eq!(result.skipped[0].reason, SkipReason::PackageUnavailable);
    }

    #[test]
    fn required_unknown_package_fails_with_package_not_found() {
        let prof = profile(vec![selection(
            "devkit.unknown.package",
            true,
            VersionPolicyName::Stable,
            None,
            None,
        )]);
        let error = resolve_profile(&prof, &source()).unwrap_err();
        assert_eq!(error.code(), ErrorCode::PackageNotFound);
        assert!(matches!(
            error,
            ProfileResolveError::RequiredPackageUnavailable { .. }
        ));
    }

    #[test]
    fn required_package_with_no_matching_version_fails_with_version_not_found() {
        let prof = profile(vec![selection(
            "devkit.lang.python",
            true,
            VersionPolicyName::Exact,
            Some("0.0.1"),
            None,
        )]);
        let error = resolve_profile(&prof, &source()).unwrap_err();
        assert_eq!(error.code(), ErrorCode::VersionNotFound);
        assert!(matches!(
            error,
            ProfileResolveError::RequiredVersionNotFound { .. }
        ));
    }

    #[test]
    fn empty_profile_resolves_to_an_empty_plan() {
        let result = resolve_profile(&profile(Vec::new()), &source()).unwrap();
        assert!(result.packages.is_empty());
        assert!(result.skipped.is_empty());
    }
}
