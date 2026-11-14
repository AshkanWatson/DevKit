//! The managed archive adapter (P3-02, installation-lifecycle.md §2 #1).
//!
//! Extraction of **verified** artifacts (zip, tar, tar.gz) into the job's
//! staging directory. Safety rules (security-model.md §5):
//!
//! - **Path traversal defense:** every entry name is validated by
//!   [`crate::path::validate_entry_path`] before anything is written, and
//!   the joined target is containment-checked against the staging root.
//! - **Symlink defense:** symlinks and hardlinks are rejected outright —
//!   the staging root is a fresh directory, so links serve no legitimate
//!   purpose in Phase 3 scope (Windows-first).
//! - **Bomb defense:** zip central-directory pre-scan enforces the total
//!   uncompressed size, file count, and per-entry compression ratio
//!   *before* extraction; tar (streaming, no central directory) enforces
//!   cumulative size and count entry by entry.
//! - **Quarantine:** on any violation or cancellation, everything
//!   extracted so far is deleted — a failed extraction leaves the staging
//!   directory empty (installation-lifecycle.md §5: "quarantined or
//!   deleted").
//!
//! Every extracted file's SHA-256 is recorded for the receipt
//! (installation-lifecycle.md §1.3: "checksums of produced files where
//! feasible"). Cancellation is cooperative between entries via
//! [`devkit_download::CancelToken`]. The API is synchronous; P3-03 runs
//! it on the job runner's blocking context.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::ArchiveError;
use crate::path::{ensure_within, validate_entry_path};

/// Extraction limits (security-model.md §5 "Zip bomb defense").
#[derive(Debug, Clone)]
pub struct ArchiveLimits {
    /// Maximum sum of uncompressed entry sizes.
    pub max_total_uncompressed: u64,
    /// Maximum number of entries (files + directories).
    pub max_file_count: usize,
    /// Maximum per-entry uncompressed/compressed ratio (zip only — tar
    /// entries carry no compressed size; tar.gz is covered by the total
    /// size limit).
    pub max_compression_ratio: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_total_uncompressed: 2 * 1024 * 1024 * 1024, // 2 GiB
            max_file_count: 50_000,
            max_compression_ratio: 100,
        }
    }
}

/// One extracted file, for the install receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedFile {
    /// Path relative to the staging directory (validated, canonical).
    pub relative_path: PathBuf,
    pub size_bytes: u64,
    /// SHA-256 of the extracted bytes.
    pub sha256_hex: String,
}

/// Successful extraction outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractResult {
    /// Extracted files in archive order. Directories are not listed.
    pub files: Vec<ExtractedFile>,
    /// Sum of the extracted file sizes.
    pub total_bytes: u64,
}

/// Archive container formats the adapter understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    /// Plain (uncompressed) tar with the ustar magic.
    Tar,
    /// gzip-compressed tar.
    TarGz,
}

/// Detects the container format from the first bytes of an artifact.
/// `prefix` should be at least 512 bytes for tar detection (ustar magic
/// lives at offset 257); shorter prefixes still detect zip and gzip.
pub fn detect_format(prefix: &[u8]) -> Option<ArchiveFormat> {
    if prefix.starts_with(b"PK\x03\x04") || prefix.starts_with(b"PK\x05\x06") {
        Some(ArchiveFormat::Zip)
    } else if prefix.starts_with(&[0x1f, 0x8b]) {
        Some(ArchiveFormat::TarGz)
    } else if prefix.len() >= 262 && &prefix[257..262] == b"ustar" {
        Some(ArchiveFormat::Tar)
    } else {
        None
    }
}

/// Extracts a verified artifact from memory into `staging_dir`.
pub fn extract_bytes(
    archive: &[u8],
    staging_dir: &Path,
    limits: &ArchiveLimits,
    cancel: &devkit_download::CancelToken,
) -> Result<ExtractResult, ArchiveError> {
    extract_from(std::io::Cursor::new(archive), staging_dir, limits, cancel)
}

/// Extracts a verified artifact file into `staging_dir`. The caller must
/// have verified the artifact (SHA-256) beforehand; this adapter refuses
/// nothing about the bytes it was handed except unsafe archive content.
pub fn extract_file(
    artifact: &Path,
    staging_dir: &Path,
    limits: &ArchiveLimits,
    cancel: &devkit_download::CancelToken,
) -> Result<ExtractResult, ArchiveError> {
    let file = fs::File::open(artifact)?;
    extract_from(file, staging_dir, limits, cancel)
}

fn extract_from<R: Read + Seek>(
    mut reader: R,
    staging_dir: &Path,
    limits: &ArchiveLimits,
    cancel: &devkit_download::CancelToken,
) -> Result<ExtractResult, ArchiveError> {
    if cancel.is_cancelled() {
        return Err(ArchiveError::Cancelled);
    }
    let mut prefix = [0u8; 512];
    read_prefix(&mut reader, &mut prefix)?;
    match detect_format(&prefix) {
        Some(ArchiveFormat::Zip) => extract_zip(reader, staging_dir, limits, cancel),
        Some(ArchiveFormat::Tar) => extract_tar(reader, staging_dir, limits, cancel),
        Some(ArchiveFormat::TarGz) => {
            reader.seek(SeekFrom::Start(0))?;
            let gz = flate2::read::GzDecoder::new(reader);
            extract_tar(gz, staging_dir, limits, cancel)
        }
        None => Err(ArchiveError::UnsupportedFormat),
    }
}

/// Reads up to `prefix.len()` bytes (archives may be shorter), then
/// rewinds for the real reader.
fn read_prefix<R: Read + Seek>(reader: &mut R, prefix: &mut [u8]) -> Result<(), ArchiveError> {
    let mut filled = 0;
    while filled < prefix.len() {
        match reader.read(&mut prefix[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(ArchiveError::Io(error)),
        }
    }
    prefix[filled..].fill(0);
    reader.seek(SeekFrom::Start(0))?;
    Ok(())
}

/// Writer wrapper: streams to disk while hashing and counting.
struct HashWriter<W: Write> {
    inner: W,
    hasher: Sha256,
    written: u64,
}

impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let count = self.inner.write(buf)?;
        self.hasher.update(&buf[..count]);
        self.written += count as u64;
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write> HashWriter<W> {
    fn hex(self) -> String {
        self.hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// Bookkeeping for quarantine: everything created during extraction, so
/// any failure can undo it completely.
#[derive(Default)]
struct Journal {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}

impl Journal {
    /// Removes everything recorded (files first, then directories in
    /// reverse creation order). Best effort: a failed cleanup still
    /// returns the original error.
    fn quarantine(self, staging_dir: &Path) {
        for path in &self.files {
            let _ = fs::remove_file(path);
        }
        for dir in self.dirs.iter().rev() {
            let _ = fs::remove_dir(dir);
            let _ = fs::remove_dir_all(dir);
        }
        // Defense in depth: nothing must remain in staging.
        let _ = fs::remove_dir_all(staging_dir);
        let _ = fs::create_dir_all(staging_dir);
    }

    fn create_parents(&mut self, staging_dir: &Path, relative: &Path) -> Result<(), ArchiveError> {
        let mut current = staging_dir.to_path_buf();
        for component in relative
            .parent()
            .into_iter()
            .flat_map(|parent| parent.components())
        {
            current.push(component);
            if !current.is_dir() {
                fs::create_dir(&current).map_err(ArchiveError::Io)?;
                self.dirs.push(current.clone());
            }
        }
        Ok(())
    }
}

/// Zip extraction: central-directory pre-scan (limits, links, names)
/// before any byte is written, then a second pass that extracts.
fn extract_zip<R: Read + Seek>(
    reader: R,
    staging_dir: &Path,
    limits: &ArchiveLimits,
    cancel: &devkit_download::CancelToken,
) -> Result<ExtractResult, ArchiveError> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|error| ArchiveError::Malformed(error.to_string()))?;

    // ---- Pre-scan pass (security-model.md §5: "pre-scan declared sizes").
    if archive.len() > limits.max_file_count {
        return Err(ArchiveError::BombLimit {
            kind: "file_count",
            detail: format!("{} entries, limit {}", archive.len(), limits.max_file_count),
        });
    }
    let mut declared_total: u64 = 0;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| ArchiveError::Malformed(error.to_string()))?;
        if entry.is_symlink() {
            return Err(ArchiveError::LinkEntry {
                entry: entry.name().to_owned(),
                kind: "symlink",
            });
        }
        let relative = validate_entry_path(entry.name())?;
        ensure_within(staging_dir, &staging_dir.join(&relative))?;
        let size = entry.size();
        declared_total = declared_total.saturating_add(size);
        if declared_total > limits.max_total_uncompressed {
            return Err(ArchiveError::BombLimit {
                kind: "total_size",
                detail: format!(
                    "declared total {declared_total} exceeds limit {}",
                    limits.max_total_uncompressed
                ),
            });
        }
        let compressed = entry.compressed_size();
        if compressed > 0 && size / compressed > limits.max_compression_ratio {
            return Err(ArchiveError::BombLimit {
                kind: "compression_ratio",
                detail: format!(
                    "entry `{}` expands {} from {} (limit {})",
                    relative.display(),
                    size,
                    compressed,
                    limits.max_compression_ratio
                ),
            });
        }
    }

    // ---- Extraction pass.
    let mut journal = Journal::default();
    let mut files = Vec::new();
    let result = (|| {
        for index in 0..archive.len() {
            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }
            let mut entry = archive
                .by_index(index)
                .map_err(|error| ArchiveError::Malformed(error.to_string()))?;
            let relative = validate_entry_path(entry.name())?;
            if entry.is_dir() {
                let target = staging_dir.join(&relative);
                ensure_within(staging_dir, &target)?;
                journal.create_parents(staging_dir, &relative)?;
                fs::create_dir_all(&target).map_err(ArchiveError::Io)?;
                continue;
            }
            journal.create_parents(staging_dir, &relative)?;
            let target = staging_dir.join(&relative);
            ensure_within(staging_dir, &target)?;
            let file = fs::File::create(&target).map_err(ArchiveError::Io)?;
            let mut sink = HashWriter {
                inner: file,
                hasher: Sha256::new(),
                written: 0,
            };
            std::io::copy(&mut entry, &mut sink).map_err(ArchiveError::Io)?;
            if sink.written != entry.size() {
                return Err(ArchiveError::Malformed(format!(
                    "entry `{}` declared {} bytes but produced {}",
                    relative.display(),
                    entry.size(),
                    sink.written
                )));
            }
            let written = sink.written;
            let sha256_hex = sink.hex();
            journal.files.push(target);
            files.push(ExtractedFile {
                relative_path: relative,
                size_bytes: written,
                sha256_hex,
            });
        }
        Ok(())
    })();

    match result {
        Ok(()) => Ok(ExtractResult {
            total_bytes: files.iter().map(|file| file.size_bytes).sum(),
            files,
        }),
        Err(error) => {
            journal.quarantine(staging_dir);
            Err(error)
        }
    }
}

/// Tar (and tar.gz) extraction. Streaming: there is no central directory,
/// so limits are enforced cumulatively entry by entry; links are rejected
/// and the journal quarantines partial output on any failure.
fn extract_tar<R: Read>(
    reader: R,
    staging_dir: &Path,
    limits: &ArchiveLimits,
    cancel: &devkit_download::CancelToken,
) -> Result<ExtractResult, ArchiveError> {
    let mut archive = tar::Archive::new(reader);
    // Do not apply archive-declared permissions/owners; files are created
    // with default user permissions.
    archive.set_preserve_permissions(false);

    let mut journal = Journal::default();
    let mut files = Vec::new();
    let mut total: u64 = 0;
    let mut count: usize = 0;

    let result = (|| {
        for entry in archive
            .entries()
            .map_err(|error| ArchiveError::Malformed(error.to_string()))?
        {
            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }
            let mut entry = entry.map_err(|error| ArchiveError::Malformed(error.to_string()))?;
            let header = entry.header();
            match header.entry_type() {
                tar::EntryType::Regular | tar::EntryType::Directory => {}
                tar::EntryType::Symlink => {
                    return Err(ArchiveError::LinkEntry {
                        entry: name_of(&entry),
                        kind: "symlink",
                    });
                }
                tar::EntryType::Link => {
                    return Err(ArchiveError::LinkEntry {
                        entry: name_of(&entry),
                        kind: "hardlink",
                    });
                }
                // Metadata pseudo-entries the tar crate resolves
                // transparently (pax, GNU long names).
                tar::EntryType::XHeader | tar::EntryType::XGlobalHeader => continue,
                tar::EntryType::GNULongName | tar::EntryType::GNULongLink => continue,
                other => {
                    return Err(ArchiveError::Malformed(format!(
                        "unsupported tar entry type {other:?}"
                    )));
                }
            }

            let name = name_of(&entry);
            let relative = validate_entry_path(&name)?;
            let target = staging_dir.join(&relative);
            ensure_within(staging_dir, &target)?;

            let declared = header.size().unwrap_or(0);
            total = total.saturating_add(declared);
            if total > limits.max_total_uncompressed {
                return Err(ArchiveError::BombLimit {
                    kind: "total_size",
                    detail: format!(
                        "streamed total {total} exceeds limit {}",
                        limits.max_total_uncompressed
                    ),
                });
            }
            count += 1;
            if count > limits.max_file_count {
                return Err(ArchiveError::BombLimit {
                    kind: "file_count",
                    detail: format!("more than {} entries", limits.max_file_count),
                });
            }

            if header.entry_type() == tar::EntryType::Directory {
                journal.create_parents(staging_dir, &relative)?;
                fs::create_dir_all(&target).map_err(ArchiveError::Io)?;
                continue;
            }

            journal.create_parents(staging_dir, &relative)?;
            let file = fs::File::create(&target).map_err(ArchiveError::Io)?;
            let mut sink = HashWriter {
                inner: file,
                hasher: Sha256::new(),
                written: 0,
            };
            std::io::copy(&mut entry, &mut sink).map_err(ArchiveError::Io)?;
            if sink.written != declared {
                return Err(ArchiveError::Malformed(format!(
                    "tar entry `{}` declared {declared} bytes but produced {}",
                    relative.display(),
                    sink.written
                )));
            }
            let written = sink.written;
            let sha256_hex = sink.hex();
            journal.files.push(target);
            files.push(ExtractedFile {
                relative_path: relative,
                size_bytes: written,
                sha256_hex,
            });
        }
        Ok(())
    })();

    match result {
        Ok(()) => Ok(ExtractResult {
            total_bytes: files.iter().map(|file| file.size_bytes).sum(),
            files,
        }),
        Err(error) => {
            journal.quarantine(staging_dir);
            Err(error)
        }
    }
}

/// Entry name as stored (separator-normalized is the validator's job).
fn name_of<R: Read>(entry: &tar::Entry<'_, R>) -> String {
    entry
        .path()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}
