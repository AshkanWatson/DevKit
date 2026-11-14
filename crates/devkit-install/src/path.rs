//! Archive entry-path validation (security-model.md §5 "Path traversal
//! defense").
//!
//! Pure functions: no filesystem access. An entry name from any archive
//! format is either accepted as a *canonical relative path* — which, when
//! joined under the staging root, cannot escape it — or rejected with the
//! precise reason. `validate_entry_path` is format-agnostic (zip and tar
//! both hand over `/`-separated names; `\` is treated as a separator too,
//! since a Windows-written archive may use it).

use std::path::{Path, PathBuf};

use crate::error::{ArchiveError, UnsafePathReason};

/// Windows reserved device names, with or without an extension
/// (`CON`, `CON.txt` is equally dangerous).
const RESERVED_DEVICE_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validates an archive entry name and returns the *relative* target path
/// to join under the staging directory.
///
/// Rules (all rejections are `INTEGRITY_CHECK_FAILED` upstream):
/// - non-empty, no NUL/control characters;
/// - relative: no leading `/`/`\`, no drive letter, no UNC/`\\?\` prefix;
/// - canonical: components are non-empty and none is `.` or `..`;
/// - no Windows-reserved device-name component;
/// - no component ends with a dot or space (Windows strips it, so the
///   extracted path would silently differ from the receipt).
///
/// Directory entries (trailing `/`) are accepted by stripping the
/// trailing separator before validation; the returned path is the same
/// either way.
pub fn validate_entry_path(entry: &str) -> Result<PathBuf, ArchiveError> {
    let failure = |reason| ArchiveError::UnsafeEntryPath {
        entry: entry.to_owned(),
        reason,
    };

    if entry.is_empty() {
        return Err(failure(UnsafePathReason::Empty));
    }
    if entry.chars().any(|c| c.is_control()) {
        return Err(failure(UnsafePathReason::ControlCharacter));
    }

    // Strip one trailing separator so directory entries validate like
    // file entries; more than one means an empty component (rejected).
    let trimmed = entry.strip_suffix(['/', '\\']).unwrap_or(entry);
    if trimmed.is_empty() {
        return Err(failure(UnsafePathReason::Empty));
    }

    let mut relative = PathBuf::new();
    for (index, component) in trimmed.split(['/', '\\']).enumerate() {
        if component.is_empty() {
            // First empty = leading separator or UNC (`//server`,
            // `\\?\C:`); later empties = `a//b` (non-canonical).
            return Err(failure(if index == 0 {
                UnsafePathReason::Absolute
            } else {
                UnsafePathReason::NonCanonical
            }));
        }
        if component == ".." {
            return Err(failure(UnsafePathReason::ParentTraversal));
        }
        if component == "." {
            return Err(failure(UnsafePathReason::NonCanonical));
        }
        if is_drive_or_namespace(component) {
            return Err(failure(UnsafePathReason::Absolute));
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err(failure(UnsafePathReason::TrailingDotOrSpace));
        }
        let (stem, _) = component.split_once('.').unwrap_or((component, ""));
        if RESERVED_DEVICE_NAMES
            .iter()
            .any(|name| name.eq_ignore_ascii_case(stem))
        {
            return Err(failure(UnsafePathReason::ReservedDeviceName));
        }
        relative.push(component);
    }

    Ok(relative)
}

/// True for drive-letter components (`C:`, `C:something`). UNC and
/// `\\?\` device paths are caught earlier: after separator splitting
/// they produce an empty first component.
fn is_drive_or_namespace(component: &str) -> bool {
    let bytes = component.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Belt-and-suspenders containment check for a path that was produced by
/// joining a validated entry under the staging root. Lexical only — the
/// staging root must be a fresh, canonical directory (which it is: the
/// job creates it before extraction, installation-lifecycle.md §3
/// PREPARING).
pub fn ensure_within(staging_root: &Path, joined: &Path) -> Result<(), ArchiveError> {
    if joined.starts_with(staging_root) {
        Ok(())
    } else {
        Err(ArchiveError::UnsafeEntryPath {
            entry: joined.to_string_lossy().into_owned(),
            reason: UnsafePathReason::Absolute,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_error(entry: &str) -> UnsafePathReason {
        match validate_entry_path(entry) {
            Err(ArchiveError::UnsafeEntryPath { reason, .. }) => reason,
            other => panic!("expected rejection for {entry:?}, got {other:?}"),
        }
    }

    #[test]
    fn accepts_canonical_relative_paths() {
        let path = validate_entry_path("app/bin/tool.exe").unwrap();
        assert_eq!(path, PathBuf::from("app").join("bin").join("tool.exe"));
        // Backslash separators (Windows-written archives) normalize.
        assert_eq!(
            validate_entry_path("app\\README.md").unwrap(),
            PathBuf::from("app").join("README.md")
        );
        // Directory entry (trailing slash) validates.
        assert_eq!(
            validate_entry_path("app/bin/").unwrap(),
            PathBuf::from("app").join("bin")
        );
        // Unicode component names are fine.
        assert!(validate_entry_path("app/構成/設定.json").is_ok());
    }

    #[test]
    fn rejects_traversal_and_absolute_entries() {
        assert_eq!(
            entry_error("../evil.txt"),
            UnsafePathReason::ParentTraversal
        );
        assert_eq!(
            entry_error("a/../../evil.txt"),
            UnsafePathReason::ParentTraversal
        );
        assert_eq!(
            entry_error("..\\evil.txt"),
            UnsafePathReason::ParentTraversal
        );
        assert_eq!(entry_error("/etc/passwd"), UnsafePathReason::Absolute);
        assert_eq!(entry_error("\\Windows\\evil"), UnsafePathReason::Absolute);
        assert_eq!(entry_error("C:/evil.txt"), UnsafePathReason::Absolute);
        assert_eq!(entry_error("C:\\evil.txt"), UnsafePathReason::Absolute);
        // UNC / device namespace forms.
        assert_eq!(entry_error("\\\\?\\C:\\evil"), UnsafePathReason::Absolute);
        assert_eq!(
            entry_error("\\\\server\\share\\x"),
            UnsafePathReason::Absolute
        );
    }

    #[test]
    fn rejects_reserved_names_and_lossy_components() {
        assert_eq!(entry_error("CON"), UnsafePathReason::ReservedDeviceName);
        assert_eq!(entry_error("nul.txt"), UnsafePathReason::ReservedDeviceName);
        assert_eq!(
            entry_error("a/COM1.exe"),
            UnsafePathReason::ReservedDeviceName
        );
        // Trailing dot/space would be stripped by Windows.
        assert_eq!(entry_error("evil. "), UnsafePathReason::TrailingDotOrSpace);
        assert_eq!(entry_error("evil."), UnsafePathReason::TrailingDotOrSpace);
        // "dir /file" splits to `dir ` (trailing space) → lossy name.
        assert_eq!(
            entry_error("dir /file"),
            UnsafePathReason::TrailingDotOrSpace
        );
    }

    #[test]
    fn rejects_empty_and_non_canonical() {
        assert_eq!(entry_error(""), UnsafePathReason::Empty);
        assert_eq!(entry_error("/"), UnsafePathReason::Empty);
        // Mid-path empty component (`a//b`) is non-canonical; a leading
        // empty (UNC/absolute) is covered by the traversal test above.
        assert_eq!(entry_error("a//b"), UnsafePathReason::NonCanonical);
        assert_eq!(entry_error("a/./b"), UnsafePathReason::NonCanonical);
        assert_eq!(entry_error("./evil"), UnsafePathReason::NonCanonical);
    }

    #[test]
    fn containment_check_rejects_escaping_joins() {
        let root = Path::new("C:\\staging\\pkg");
        assert!(ensure_within(root, &root.join("app\\bin")).is_ok());
        assert!(ensure_within(root, Path::new("C:\\staging\\other")).is_err());
    }
}
