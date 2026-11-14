//! Archive adapter errors with stable `ErrorCode` mapping.
//!
//! Extraction-safety violations (traversal, symlink, bomb limits,
//! malformed archive) map to `INTEGRITY_CHECK_FAILED`
//! (installation-lifecycle.md §5: the artifact is quarantined or deleted,
//! never partially installed). Filesystem failures map to
//! `INSTALL_FAILED`; cancellation to `CANCELLED`.

use devkit_core::error::ErrorCode;

/// Archive extraction failures (security-model.md §5,
/// installation-lifecycle.md §5).
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// An entry path was absolute, escaped the staging root via `..`,
    /// used a reserved device name, or was otherwise not a safe relative
    /// path.
    #[error("unsafe archive entry path `{entry}`: {reason}")]
    UnsafeEntryPath {
        entry: String,
        reason: UnsafePathReason,
    },

    /// The archive contains a symlink or hardlink. Phase 3 scope (Windows
    /// first release, installation-lifecycle.md §5) rejects all links in
    /// managed archives; the staging root is a fresh directory so links
    /// serve no legitimate purpose there.
    #[error("archive entry `{entry}` is a {kind}; managed archives must not contain links")]
    LinkEntry { entry: String, kind: &'static str },

    /// Pre-scan (zip) or streaming accumulation (tar) hit a bomb limit:
    /// total uncompressed size, file count, or compression ratio.
    #[error("archive bomb limit hit ({kind}): {detail}")]
    BombLimit { kind: &'static str, detail: String },

    /// The bytes are not a supported archive format, or the container is
    /// corrupt.
    #[error("unsupported or malformed archive: {0}")]
    Malformed(String),

    /// The artifact declares `installer: archive` but carries neither zip
    /// nor tar/tar.gz magic.
    #[error("artifact is not a supported archive format (zip, tar, tar.gz)")]
    UnsupportedFormat,

    /// Cancellation was requested between entries; partial output is
    /// removed.
    #[error("extraction cancelled")]
    Cancelled,

    /// Filesystem failure while extracting into the staging directory.
    #[error("extraction filesystem failure: {0}")]
    Io(#[from] std::io::Error),
}

/// Why an entry path failed validation (`path::validate_entry_path`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsafePathReason {
    /// Empty path or only separators.
    Empty,
    /// Starts with `/`, `\`, or a drive/UNC prefix.
    Absolute,
    /// Contains a `..` component (zip-slip).
    ParentTraversal,
    /// Contains a `.` component or empty component (non-canonical).
    NonCanonical,
    /// Component stem is a Windows reserved device name (CON, NUL, ...).
    ReservedDeviceName,
    /// Component ends with a dot or space (lossy on Windows).
    TrailingDotOrSpace,
    /// Contains a NUL or other control character.
    ControlCharacter,
}

impl UnsafePathReason {
    pub fn as_str(self) -> &'static str {
        match self {
            UnsafePathReason::Empty => "empty path",
            UnsafePathReason::Absolute => "absolute path",
            UnsafePathReason::ParentTraversal => "parent traversal (`..`)",
            UnsafePathReason::NonCanonical => "non-canonical path",
            UnsafePathReason::ReservedDeviceName => "reserved device name",
            UnsafePathReason::TrailingDotOrSpace => "trailing dot or space",
            UnsafePathReason::ControlCharacter => "control character",
        }
    }
}

impl std::fmt::Display for UnsafePathReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ArchiveError {
    /// The stable error code for the job event stream and FFI surface.
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::UnsafeEntryPath { .. }
            | Self::LinkEntry { .. }
            | Self::BombLimit { .. }
            | Self::Malformed(_)
            | Self::UnsupportedFormat => ErrorCode::IntegrityCheckFailed,
            Self::Io(_) => ErrorCode::InstallFailed,
            Self::Cancelled => ErrorCode::Cancelled,
        }
    }
}
