//! Version selection policies (HANDOFF.md §5).
//!
//! Semantics required by the product contract:
//! - `Exact`: install precisely the requested version; never silently resolve
//!   to a different version.
//! - `Stable`: newest release on the stable channel that is not a prerelease.
//! - `Latest`: newest release allowed by the policy; prereleases only when
//!   explicitly requested.
//! - `Channel`: newest release within a named channel (stable/beta/dev/...).

use serde::{Deserialize, Serialize};

/// A named update channel as declared by a package manifest.
pub const STABLE_CHANNEL: &str = "stable";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub channel: String,
    /// True for prerelease-style versions (beta/dev/nightly channels).
    pub prerelease: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VersionPolicy {
    /// Install exactly this version string.
    Exact { version: String },
    /// Newest non-prerelease stable release.
    Stable,
    /// Newest release; prereleases included only when `allow_prerelease`.
    Latest { allow_prerelease: bool },
    /// Newest release within the named channel.
    Channel { channel: String },
}

/// A resolved selection: the user-visible policy plus the concrete version.
///
/// Both must be persisted (HANDOFF.md §5): never silently upgrade an exact
/// pin, and never lose the policy the user asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedVersion {
    pub policy: VersionPolicy,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// No release matches the policy. `code` carries the stable error code.
    NoMatch { code: crate::error::ErrorCode },
}

/// Resolve `policy` against the available releases.
///
/// Ordering is the caller's responsibility: releases must be supplied in
/// preference order (newest first). This keeps version ordering logic out of
/// the policy layer, since version formats vary by package (e.g. Flutter's
/// `3.x.y`, Rust's `1.85.0`, calendar versions).
pub fn resolve(
    releases: &[Release],
    policy: &VersionPolicy,
) -> Result<ResolvedVersion, ResolveError> {
    let matched = match policy {
        VersionPolicy::Exact { version } => releases
            .iter()
            .find(|r| r.version == *version)
            .ok_or(ResolveError::NoMatch {
                code: crate::error::ErrorCode::VersionNotFound,
            })?
            .version
            .clone(),
        VersionPolicy::Stable => releases
            .iter()
            .find(|r| !r.prerelease && r.channel == STABLE_CHANNEL)
            .ok_or(ResolveError::NoMatch {
                code: crate::error::ErrorCode::VersionNotFound,
            })?
            .version
            .clone(),
        VersionPolicy::Latest { allow_prerelease } => releases
            .iter()
            .find(|r| *allow_prerelease || !r.prerelease)
            .ok_or(ResolveError::NoMatch {
                code: crate::error::ErrorCode::VersionNotFound,
            })?
            .version
            .clone(),
        VersionPolicy::Channel { channel } => releases
            .iter()
            .find(|r| r.channel == *channel)
            .ok_or(ResolveError::NoMatch {
                code: crate::error::ErrorCode::VersionNotFound,
            })?
            .version
            .clone(),
    };
    Ok(ResolvedVersion {
        policy: policy.clone(),
        version: matched,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn releases() -> Vec<Release> {
        vec![
            Release {
                version: "3.14.0rc1".into(),
                channel: "beta".into(),
                prerelease: true,
            },
            Release {
                version: "3.13.1".into(),
                channel: "stable".into(),
                prerelease: false,
            },
            Release {
                version: "3.13.0".into(),
                channel: "stable".into(),
                prerelease: false,
            },
            Release {
                version: "3.12.7".into(),
                channel: "stable".into(),
                prerelease: false,
            },
        ]
    }

    #[test]
    fn stable_picks_newest_non_prerelease() {
        let r = resolve(&releases(), &VersionPolicy::Stable).unwrap();
        assert_eq!(r.version, "3.13.1");
    }

    #[test]
    fn latest_excludes_prerelease_by_default() {
        let r = resolve(
            &releases(),
            &VersionPolicy::Latest {
                allow_prerelease: false,
            },
        )
        .unwrap();
        assert_eq!(r.version, "3.13.1");
    }

    #[test]
    fn latest_can_include_prerelease_when_requested() {
        let r = resolve(
            &releases(),
            &VersionPolicy::Latest {
                allow_prerelease: true,
            },
        )
        .unwrap();
        assert_eq!(r.version, "3.14.0rc1");
    }

    #[test]
    fn exact_matches_only_the_requested_version() {
        let r = resolve(
            &releases(),
            &VersionPolicy::Exact {
                version: "3.12.7".into(),
            },
        )
        .unwrap();
        assert_eq!(r.version, "3.12.7");
        assert_eq!(
            r.policy,
            VersionPolicy::Exact {
                version: "3.12.7".into()
            }
        );
    }

    #[test]
    fn exact_never_silently_resolves_to_another_version() {
        let err = resolve(
            &releases(),
            &VersionPolicy::Exact {
                version: "9.9.9".into(),
            },
        )
        .unwrap_err();
        assert_eq!(
            err,
            ResolveError::NoMatch {
                code: crate::error::ErrorCode::VersionNotFound
            }
        );
    }

    #[test]
    fn channel_picks_newest_in_channel() {
        let r = resolve(
            &releases(),
            &VersionPolicy::Channel {
                channel: "beta".into(),
            },
        )
        .unwrap();
        assert_eq!(r.version, "3.14.0rc1");
    }

    #[test]
    fn empty_release_list_is_version_not_found() {
        let err = resolve(&[], &VersionPolicy::Stable).unwrap_err();
        assert_eq!(
            err,
            ResolveError::NoMatch {
                code: crate::error::ErrorCode::VersionNotFound
            }
        );
    }
}
