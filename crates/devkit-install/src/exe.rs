//! The `exe` adapter (P3-06): vendor installer executables with a fixed,
//! silent-install contract (installation-lifecycle.md §2.1,
//! security-model.md §6).
//!
//! The adapter runs a **verified** installer executable (SHA-256 checked by
//! `devkit-download` before anything here is called) with flags from a
//! hardcoded per-technology table — **never** from catalog-supplied
//! strings. Two technologies are in the contract table, chosen because both
//! honor an install-directory flag, which keeps the installation inside the
//! managed root:
//!
//! | Technology | Detection (case-sensitive, in the PE image) | Fixed flags |
//! |---|---|---|
//! | Inno Setup | `Inno Setup` / `InnoSetupVersion` | `/VERYSILENT /NORESTART /SUPPRESSMSGBOXES /LOG=<log> /DIR=<final dir>` |
//! | NSIS | `Nullsoft` | `/S /D=<final dir>` (last, unquoted) |
//!
//! A file that is not a PE image or carries no supported marker is rejected
//! with `INVALID_ARGUMENT` — it is not in the contract table, so it is
//! never executed. WiX Burn and other bootstrappers are excluded on
//! purpose: they do not honor an install-directory flag.
//!
//! The vendor installer runs un-elevated and is owned by the caller's job:
//! [`ExeRunner::run`] takes the job's cancellation token and kills the
//! process when it fires (installation-lifecycle.md §3 rule 3: no process
//! is killed unless the job owns it — a vendor installer it started).
//! Nothing here touches global state (PATH, registry, env vars); the exe
//! adapter is Windows-only at the pipeline gate (`UNSUPPORTED_PLATFORM`
//! elsewhere).

use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use devkit_core::error::ErrorCode;
use devkit_download::CancelToken;
use sha2::{Digest, Sha256};

use crate::archive::ExtractedFile;

/// Streaming scan window for technology detection (1 MiB, with an overlap
/// so a marker straddling a chunk boundary is still found).
const SCAN_CHUNK: usize = 1024 * 1024;
/// Longest marker byte length minus one: bytes carried between chunks.
const SCAN_OVERLAP: usize = 16 - 1;

/// PE images start with the DOS header magic.
const PE_MAGIC: [u8; 2] = *b"MZ";

/// A vendor installer technology from the fixed contract table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExeTechnology {
    /// Inno Setup (jrsoftware.org) silent install.
    InnoSetup,
    /// NSIS (Nullsoft Install System) silent install.
    Nsis,
}

impl ExeTechnology {
    /// Every technology in the contract table, in review order.
    pub const ALL: [ExeTechnology; 2] = [ExeTechnology::InnoSetup, ExeTechnology::Nsis];

    /// Stable label for receipts and events.
    pub fn label(self) -> &'static str {
        match self {
            ExeTechnology::InnoSetup => "inno-setup",
            ExeTechnology::Nsis => "nsis",
        }
    }

    /// Inverse of [`ExeTechnology::label`].
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "inno-setup" => Some(ExeTechnology::InnoSetup),
            "nsis" => Some(ExeTechnology::Nsis),
            _ => None,
        }
    }

    /// Case-sensitive byte markers the technology embeds in its installer
    /// image (reviewed in installation-lifecycle.md §2.1).
    fn markers(self) -> &'static [&'static str] {
        match self {
            ExeTechnology::InnoSetup => &["Inno Setup", "InnoSetupVersion"],
            ExeTechnology::Nsis => &["Nullsoft"],
        }
    }
}

/// `exe` adapter errors with a stable `ErrorCode` mapping. Cancellation is
/// not an error: the runner reports it as a successful outcome (see
/// [`ExeRunOutcome`]) because killing the child process **is** the correct
/// handling, not a failure.
#[derive(Debug, thiserror::Error)]
pub enum ExeError {
    /// The verified artifact is not a PE image or carries no marker from
    /// the contract table — it is never executed.
    #[error("vendor installer is not in the supported contract table: {0}")]
    UnsupportedTechnology(String),

    /// Filesystem failure while reading the installer or spawning the
    /// process.
    #[error("cannot run the vendor installer: {0}")]
    Io(#[from] std::io::Error),

    /// The spawned process could not be polled (wait-side failure).
    #[error("vendor installer process failed: {0}")]
    Process(String),
}

impl ExeError {
    /// The stable error code for the job event stream.
    pub fn code(&self) -> ErrorCode {
        match self {
            ExeError::UnsupportedTechnology(_) => ErrorCode::InvalidArgument,
            ExeError::Io(_) | ExeError::Process(_) => ErrorCode::InstallFailed,
        }
    }
}

/// Outcome of running a vendor installer under job ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExeRunOutcome {
    /// The child's exit code, or `None` when it died without one (killed
    /// externally, access violation without a code, ...). The pipeline
    /// treats `Some(0)` as success and everything else as `INSTALL_FAILED`.
    pub exit_code: Option<i32>,
    /// `true` when the job's cancellation was honored by killing the child
    /// process. The pipeline then rolls back as for any cancellation.
    pub cancelled: bool,
}

/// Spawns `program` and owns it until it exits or the job cancels. The
/// pipeline injects this so tests can record the constructed command line
/// and simulate installer behavior without running foreign vendor
/// artifacts (testing-strategy.md rule 1).
pub trait ExeRunner: Send + Sync {
    /// Runs the vendor installer with the adapter's **fixed** flags. The
    /// `cancel` token belongs to the installation job: the runner must
    /// check it while the child runs and kill the child when it fires.
    fn run(
        &self,
        program: &str,
        flags: &[String],
        cancel: &CancelToken,
    ) -> Result<ExeRunOutcome, ExeError>;
}

/// Production [`ExeRunner`]: spawns the installer as an un-elevated child
/// process, polls the job's cancellation token every ~100 ms, and kills the
/// child on cancel. The child's output is discarded (its own UI/log capture
/// is the vendor's business; DevKit's audit trail is the receipt).
pub struct ProcessExeRunner;

impl ExeRunner for ProcessExeRunner {
    fn run(
        &self,
        program: &str,
        flags: &[String],
        cancel: &CancelToken,
    ) -> Result<ExeRunOutcome, ExeError> {
        if cancel.is_cancelled() {
            return Ok(ExeRunOutcome {
                exit_code: None,
                cancelled: true,
            });
        }
        let mut command = std::process::Command::new(program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: a vendor installer must never flash a
            // console window over the desktop app.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
            for flag in flags {
                if flag.starts_with("/D=") {
                    // NSIS contract: /D=<dir> must be the LAST argument and
                    // must not be quoted, even when the path contains
                    // spaces — pass it raw so the OS never quotes it.
                    command.raw_arg(flag);
                } else {
                    command.arg(flag);
                }
            }
        }
        #[cfg(not(windows))]
        {
            command.args(flags);
        }
        let mut child = command.spawn()?;
        loop {
            if cancel.is_cancelled() {
                // The job owns this process: kill it and report the
                // cancellation so the pipeline rolls back the partial
                // installation.
                let _ = child.kill();
                let _ = child.wait();
                return Ok(ExeRunOutcome {
                    exit_code: None,
                    cancelled: true,
                });
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Ok(ExeRunOutcome {
                        exit_code: status.code(),
                        cancelled: false,
                    })
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(error) => return Err(ExeError::Process(error.to_string())),
            }
        }
    }
}

/// Detects the technology from a byte slice. The bytes must be a PE image
/// **and** carry a case-sensitive marker; anything else returns `None`
/// (the caller rejects it — detection failure means "not in the contract
/// table", never "run it anyway with guessed flags").
pub fn detect_technology(bytes: &[u8]) -> Option<ExeTechnology> {
    if bytes.len() < 2 || bytes[..2] != PE_MAGIC {
        return None;
    }
    scan_for_markers(bytes)
}

/// Streaming variant of [`detect_technology`] for the verified artifact on
/// disk: bounded memory (1 MiB chunks with overlap) so a multi-gigabyte
/// installer never loads into RAM.
pub fn detect_technology_from_file(path: &Path) -> std::io::Result<Option<ExeTechnology>> {
    let mut file = fs::File::open(path)?;
    let mut magic = [0_u8; 2];
    let mut magic_read = 0_usize;
    while magic_read < magic.len() {
        match file.read(&mut magic[magic_read..]) {
            Ok(0) => return Ok(None),
            Ok(count) => magic_read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    if magic != PE_MAGIC {
        return Ok(None);
    }

    let mut overlap: Vec<u8> = Vec::with_capacity(SCAN_OVERLAP);
    let mut buffer = vec![0_u8; SCAN_CHUNK];
    loop {
        let mut read = 0_usize;
        while read < buffer.len() {
            match file.read(&mut buffer[read..]) {
                Ok(0) => break,
                Ok(count) => read += count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        if read == 0 {
            return Ok(None);
        }
        let mut window: Vec<u8> = Vec::with_capacity(overlap.len() + read);
        window.extend_from_slice(&overlap);
        window.extend_from_slice(&buffer[..read]);
        if let Some(technology) = scan_for_markers(&window) {
            return Ok(Some(technology));
        }
        if read < buffer.len() {
            return Ok(None);
        }
        overlap.clear();
        overlap.extend_from_slice(&window[window.len() - SCAN_OVERLAP..]);
    }
}

/// Any supported technology marker present wins (the technologies are
/// mutually exclusive in practice; Inno's own strings contain "Inno Setup",
/// never "Nullsoft").
fn scan_for_markers(bytes: &[u8]) -> Option<ExeTechnology> {
    ExeTechnology::ALL.into_iter().find(|&technology| {
        technology
            .markers()
            .iter()
            .any(|marker| contains_slice(bytes, marker.as_bytes()))
    })
}

/// Substring search that fails fast on the first byte.
fn contains_slice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    let first = needle[0];
    for (position, byte) in haystack.iter().enumerate() {
        if *byte != first {
            continue;
        }
        if position + needle.len() > haystack.len() {
            return false;
        }
        if &haystack[position..position + needle.len()] == needle {
            return true;
        }
    }
    false
}

/// The fixed silent flags for a technology, from the reviewed contract
/// table — the **only** place flags are produced. `log_path` is an Inno
/// Setup concern (`/LOG` into the staging directory, deleted on rollback);
/// NSIS has no log flag and ignores it.
pub fn silent_flags(
    technology: ExeTechnology,
    final_dir: &Path,
    log_path: Option<&Path>,
) -> Vec<String> {
    match technology {
        ExeTechnology::InnoSetup => {
            let mut flags = vec![
                "/VERYSILENT".to_owned(),
                "/NORESTART".to_owned(),
                "/SUPPRESSMSGBOXES".to_owned(),
            ];
            if let Some(log) = log_path {
                flags.push(format!("/LOG={}", log.display()));
            }
            flags.push(format!("/DIR={}", final_dir.display()));
            flags
        }
        ExeTechnology::Nsis => vec!["/S".to_owned(), format!("/D={}", final_dir.display())],
    }
}

/// The complete command line for the vendor installer: `(program, flags)`.
/// The only free inputs are the verified installer path, the installation
/// directory, and the staging log path — catalog text (name, description,
/// URLs, hashes) has no path into the command line, which a test asserts.
pub fn build_command(
    program: &Path,
    technology: ExeTechnology,
    final_dir: &Path,
    log_path: Option<&Path>,
) -> (String, Vec<String>) {
    (
        program.to_string_lossy().into_owned(),
        silent_flags(technology, final_dir, log_path),
    )
}

/// Recursively walks `root` and records every regular file as an
/// [`ExtractedFile`]-shaped receipt entry (path relative to `root`, size,
/// SHA-256), sorted by path for a deterministic receipt. Symlinks are
/// skipped (the archive adapter rejects links; a vendor directory should
/// not contain them either). Used by the `exe` adapter: the vendor
/// installer wrote the files, so the receipt is built by observing the
/// result — and an empty result fails validation, catching a vendor that
/// ignored the install-directory flag.
pub fn walk_installed_files(root: &Path) -> std::io::Result<Vec<ExtractedFile>> {
    let mut files = Vec::new();
    walk_dir(root, root, &mut files)?;
    files.sort_by(|a, b| {
        a.relative_path
            .to_string_lossy()
            .cmp(&b.relative_path.to_string_lossy())
    });
    Ok(files)
}

fn walk_dir(root: &Path, dir: &Path, files: &mut Vec<ExtractedFile>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        // Never follow links out of the installation directory.
        if entry.file_type()?.is_symlink() {
            continue;
        }
        if path.is_dir() {
            walk_dir(root, &path, files)?;
        } else if path.is_file() {
            let (size_bytes, sha256_hex) = hash_file(&path)?;
            files.push(ExtractedFile {
                relative_path: path
                    .strip_prefix(root)
                    .expect("walk starts at root")
                    .to_path_buf(),
                size_bytes,
                sha256_hex,
            });
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> std::io::Result<(u64, String)> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        size += count as u64;
    }
    Ok((size, to_hex(&hasher.finalize())))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pe_with(markers: &[&str]) -> Vec<u8> {
        let mut bytes = PE_MAGIC.to_vec();
        bytes.extend_from_slice(b"this program cannot be run in DOS mode");
        for marker in markers {
            bytes.extend_from_slice(marker.as_bytes());
        }
        bytes.extend_from_slice(&[0_u8; 64]);
        bytes
    }

    // -----------------------------------------------------------------
    // Technology detection
    // -----------------------------------------------------------------

    #[test]
    fn detects_inno_and_nsis_markers() {
        assert_eq!(
            detect_technology(&pe_with(&["built with Inno Setup 6.2"])),
            Some(ExeTechnology::InnoSetup)
        );
        assert_eq!(
            detect_technology(&pe_with(&["InnoSetupVersion=6.2.1"])),
            Some(ExeTechnology::InnoSetup)
        );
        assert_eq!(
            detect_technology(&pe_with(&["Nullsoft Install System v3.08"])),
            Some(ExeTechnology::Nsis)
        );
    }

    #[test]
    fn markers_are_case_sensitive() {
        assert_eq!(detect_technology(&pe_with(&["inno setup 6"])), None);
        assert_eq!(detect_technology(&pe_with(&["nullsoft installer"])), None);
    }

    #[test]
    fn non_pe_bytes_are_never_detected() {
        // The marker without the MZ header: not a Windows installer image.
        assert_eq!(detect_technology(b"Inno Setup plain text"), None);
        assert_eq!(detect_technology(b"M"), None);
        assert_eq!(detect_technology(b""), None);
        // PE header but no supported marker: not in the contract table.
        assert_eq!(
            detect_technology(&pe_with(&["WiX Burn bootstrapper"])),
            None
        );
    }

    #[test]
    fn streaming_detection_matches_in_memory_detection() {
        let case = pe_with(&["Inno Setup", "Nullsoft"]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installer.exe");
        std::fs::write(&path, &case).unwrap();
        assert_eq!(
            detect_technology_from_file(&path).unwrap(),
            detect_technology(&case)
        );
    }

    #[test]
    fn streaming_detection_finds_markers_across_chunk_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installer.exe");
        // A marker straddling the 1 MiB chunk boundary: the overlap logic
        // must still find it ("Nullsoft" starts 4 bytes before the
        // boundary).
        let mut bytes = PE_MAGIC.to_vec();
        bytes.resize(SCAN_CHUNK - 4, b'.');
        bytes.extend_from_slice(b"Nullsoft Install System");
        bytes.extend_from_slice(&[0_u8; 16]);
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            detect_technology_from_file(&path).unwrap(),
            Some(ExeTechnology::Nsis)
        );

        // A marker in the final (short) chunk.
        let mut bytes = PE_MAGIC.to_vec();
        bytes.resize(SCAN_CHUNK + 300_000, b'.');
        bytes.extend_from_slice(b"Inno Setup");
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            detect_technology_from_file(&path).unwrap(),
            Some(ExeTechnology::InnoSetup)
        );
    }

    #[test]
    fn streaming_detection_rejects_non_pe_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installer.txt");
        std::fs::write(&path, b"Nullsoft marker, no MZ header").unwrap();
        assert_eq!(detect_technology_from_file(&path).unwrap(), None);
    }

    #[test]
    fn labels_round_trip() {
        for technology in ExeTechnology::ALL {
            assert_eq!(
                ExeTechnology::from_label(technology.label()),
                Some(technology)
            );
        }
        assert_eq!(ExeTechnology::from_label("wix-burn"), None);
    }

    // -----------------------------------------------------------------
    // The fixed command line (P3-06 acceptance)
    // -----------------------------------------------------------------

    #[test]
    fn inno_flags_come_from_the_contract_table() {
        let dir = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0");
        let log = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0.staging-job\install.log");
        assert_eq!(
            silent_flags(ExeTechnology::InnoSetup, &dir, Some(&log)),
            vec![
                "/VERYSILENT".to_owned(),
                "/NORESTART".to_owned(),
                "/SUPPRESSMSGBOXES".to_owned(),
                format!("/LOG={}", log.display()),
                format!("/DIR={}", dir.display()),
            ]
        );
    }

    #[test]
    fn nsis_flags_come_from_the_contract_table() {
        let dir = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0");
        // NSIS has no log flag: the log path is ignored.
        let log = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0.staging-job\install.log");
        assert_eq!(
            silent_flags(ExeTechnology::Nsis, &dir, Some(&log)),
            vec!["/S".to_owned(), format!("/D={}", dir.display())]
        );
    }

    #[test]
    fn nsis_dir_flag_is_last_and_unquoted() {
        // "C:\Dev K\packages" contains a space; /D=... must stay a single
        // argument without embedded quotes (the runner passes it raw).
        let dir = PathBuf::from(r"C:\Dev K\packages\tool\1.0.0");
        let flags = silent_flags(ExeTechnology::Nsis, &dir, None);
        assert_eq!(flags.len(), 2);
        assert_eq!(flags[0], "/S");
        assert_eq!(flags[1], format!("/D={}", dir.display()));
        assert!(!flags[1].contains('"'));
        assert_eq!(
            flags.last().unwrap().strip_prefix("/D="),
            Some(dir.to_str().unwrap())
        );
    }

    #[test]
    fn catalog_text_never_reaches_the_command_line() {
        // The command is built from the contract table plus exactly three
        // free inputs (program, final dir, log). Two resolutions of the
        // same artifact with entirely different catalog strings must
        // produce identical commands.
        let program = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0.staging-j\artifact.bin");
        let dir = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0");
        let command = |catalog_noise: &str| {
            let _ = catalog_noise; // name, description, URLs, hashes, ...
            build_command(&program, ExeTechnology::InnoSetup, &dir, None)
        };
        assert_eq!(
            command("tool-suite official description http://vendor.example/setup /S /D=C:\\evil"),
            command("another rendering of the same manifest")
        );
        // And the flags themselves are exactly the reviewed table.
        assert_eq!(
            command("x").1,
            vec![
                "/VERYSILENT".to_owned(),
                "/NORESTART".to_owned(),
                "/SUPPRESSMSGBOXES".to_owned(),
                format!("/DIR={}", dir.display()),
            ]
        );
    }

    // -----------------------------------------------------------------
    // Receipt walking
    // -----------------------------------------------------------------

    #[test]
    fn walk_records_files_with_sizes_and_hashes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(dir.path().join("bin/tool.exe"), b"MZtoolbytes").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"hello").unwrap();

        let files = walk_installed_files(dir.path()).unwrap();
        assert_eq!(files.len(), 2);
        let tool = files
            .iter()
            .find(|file| file.relative_path == *"bin/tool.exe")
            .unwrap();
        assert_eq!(tool.size_bytes, 11);
        assert_eq!(tool.sha256_hex.len(), 64);
        let readme = files
            .iter()
            .find(|file| file.relative_path == *"readme.txt")
            .unwrap();
        assert_eq!(readme.size_bytes, 5);
        // Sorted for a deterministic receipt.
        let mut sorted = files.clone();
        sorted.sort_by(|a, b| {
            a.relative_path
                .to_string_lossy()
                .cmp(&b.relative_path.to_string_lossy())
        });
        assert_eq!(files, sorted);
    }

    #[test]
    fn walk_of_empty_directory_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(walk_installed_files(dir.path()).unwrap().is_empty());
    }
}
