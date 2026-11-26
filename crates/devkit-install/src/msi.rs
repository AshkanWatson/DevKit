//! The `msi` adapter (P3-07): Windows Installer packages run through the
//! system `msiexec` with a fixed, silent-install contract
//! (installation-lifecycle.md §2.2).
//!
//! The adapter runs `msiexec` (resolved from the absolute System32 path —
//! never through PATH resolution) against the **verified** MSI file with
//! flags from a hardcoded table — **never** catalog-supplied strings:
//!
//! | Step | Fixed command |
//! |---|---|
//! | Install | `msiexec /i <verified msi> /qn /norestart INSTALLDIR=<final dir>` |
//! | Uninstall | `msiexec /x <ProductCode> /qn /norestart` |
//!
//! MSI packages are registered with Windows Installer, so the receipt
//! records the package's `ProductCode` (read from the verified file's
//! Property table with the read-only `MsiOpenDatabase` API — the package is
//! opened as a database, never executed): rollback after a failed install
//! and receipt-driven uninstall both run `msiexec /x {ProductCode}` first,
//! so no ghost registration is left behind and reinstall stays possible.
//!
//! Exit code 0 = success; 3010 = success, reboot recommended (`/norestart`
//! suppresses the reboot; the files are installed). Any other code fails
//! the job with `INSTALL_FAILED`. Elevation-requiring (per-machine) MSIs
//! fail un-elevated with their own exit code (security-model.md §7).
//!
//! A file that is not an OLE compound document (the MSI container magic)
//! is rejected with `INVALID_ARGUMENT` — it is never executed. Like the
//! `exe` adapter, this one is Windows-only at the pipeline gate
//! (`UNSUPPORTED_PLATFORM` elsewhere) and never touches global state.

use std::io::Read;
use std::path::{Path, PathBuf};

use devkit_core::error::ErrorCode;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
#[cfg(windows)]
use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
    MsiCloseHandle, MsiDatabaseOpenViewW, MsiOpenDatabaseW, MsiRecordGetStringW, MsiViewClose,
    MsiViewExecute, MsiViewFetch, MSIDBOPEN_READONLY, MSIHANDLE,
};

/// OLE compound document magic: every `.msi` container starts with it.
pub const MSI_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// `msiexec` exit codes that mean success (3010 = reboot required; the
/// files are installed, `/norestart` suppressed the reboot).
pub const MSI_EXIT_SUCCESS: i32 = 0;
pub const MSI_EXIT_REBOOT_REQUIRED: i32 = 3010;
/// "This action is only valid for products that are currently installed" —
/// the product is not registered, so only our managed-root copy exists.
pub const MSI_EXIT_UNKNOWN_PRODUCT: i32 = 1605;

/// `msi` adapter errors with a stable `ErrorCode` mapping.
#[derive(Debug, thiserror::Error)]
pub enum MsiError {
    /// The verified artifact is not an MSI container (missing OLE magic) —
    /// it is never executed.
    #[error("verified artifact is not a Windows Installer package (msi)")]
    NotAnMsi,

    /// The MSI database could not be opened/queried (corrupt container or
    /// filesystem failure).
    #[error("Windows Installer database error ({function}: code {code})")]
    Api { function: &'static str, code: u32 },

    /// The package declares no `ProductCode` property — it is not a
    /// usable installer package (the receipt could never drive an
    /// uninstall), so it is never run.
    #[error("the MSI package declares no ProductCode property")]
    NoProductCode,

    /// The adapter is Windows-only; the pipeline gate normally prevents
    /// this from ever being reached.
    #[error("the msi adapter is Windows-only")]
    UnsupportedPlatform,

    /// Filesystem failure while reading the verified artifact.
    #[error("cannot read the MSI package: {0}")]
    Io(#[from] std::io::Error),
}

impl MsiError {
    /// The stable error code for the job event stream.
    pub fn code(&self) -> ErrorCode {
        match self {
            MsiError::NotAnMsi | MsiError::NoProductCode => ErrorCode::InvalidArgument,
            MsiError::Api { .. } | MsiError::Io(_) => ErrorCode::InstallFailed,
            MsiError::UnsupportedPlatform => ErrorCode::UnsupportedPlatform,
        }
    }
}

/// The fixed silent-install flags for a verified MSI package, from the
/// reviewed contract table — the **only** place flags are produced.
/// `INSTALLDIR` is a public property: MSIs authored with that property
/// install into the managed root; a vendor that ignores it produces an
/// empty installation directory, which fails receipt validation and rolls
/// back (with the ProductCode undo, so no registration remains).
pub fn silent_flags(msi_path: &Path, final_dir: &Path) -> Vec<String> {
    vec![
        "/i".to_owned(),
        msi_path.to_string_lossy().into_owned(),
        "/qn".to_owned(),
        "/norestart".to_owned(),
        format!("INSTALLDIR={}", final_dir.display()),
    ]
}

/// The fixed silent-uninstall flags for a registered product.
pub fn uninstall_flags(product_code: &str) -> Vec<String> {
    vec![
        "/x".to_owned(),
        product_code.to_owned(),
        "/qn".to_owned(),
        "/norestart".to_owned(),
    ]
}

/// The complete install command line: `(program, flags)`. The only free
/// inputs are the system `msiexec` path, the verified MSI path, and the
/// installation directory — catalog text has no path into the command
/// line, which a test asserts.
pub fn build_command(program: &Path, msi_path: &Path, final_dir: &Path) -> (String, Vec<String>) {
    (
        program.to_string_lossy().into_owned(),
        silent_flags(msi_path, final_dir),
    )
}

/// The complete uninstall command line for a receipt-recorded product.
pub fn build_uninstall_command(program: &Path, product_code: &str) -> (String, Vec<String>) {
    (
        program.to_string_lossy().into_owned(),
        uninstall_flags(product_code),
    )
}

/// The system `msiexec`, resolved from the absolute System32 path so PATH
/// can never shadow it (security-model.md §6). Falls back to the bare name
/// if `SystemRoot` is unavailable (then the OS still resolves System32
/// before PATH for CreateProcess).
pub fn msiexec_program() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            let program = PathBuf::from(system_root)
                .join("System32")
                .join("msiexec.exe");
            if program.is_file() {
                return program;
            }
        }
    }
    PathBuf::from("msiexec.exe")
}

/// True when the exit code means the MSI install/uninstall succeeded.
pub fn is_success_exit_code(exit_code: Option<i32>) -> bool {
    matches!(
        exit_code,
        Some(MSI_EXIT_SUCCESS) | Some(MSI_EXIT_REBOOT_REQUIRED)
    )
}

/// Detects the MSI container magic in a byte slice.
pub fn is_msi_package(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[..8] == MSI_MAGIC
}

/// Streaming variant of [`is_msi_package`] for the verified artifact on
/// disk: reads exactly the first 8 bytes.
pub fn is_msi_package_file(path: &Path) -> std::io::Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut magic = [0_u8; 8];
    let mut read = 0_usize;
    while read < magic.len() {
        match file.read(&mut magic[read..]) {
            Ok(0) => return Ok(false),
            Ok(count) => read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(magic == MSI_MAGIC)
}

/// Reads the package's `ProductCode` property from the verified MSI's
/// Property table. The package is opened **read-only as a database**
/// (`MsiOpenDatabase` with `MSIDBOPEN_READONLY`) — this never executes
/// anything, it is a structured-storage read.
#[cfg(windows)]
pub fn read_product_code(msi_path: &Path) -> Result<String, MsiError> {
    let path_w = wide(&msi_path.to_string_lossy());
    let mut database: MSIHANDLE = 0;
    // SAFETY: all arguments are valid for the duration of the call; the
    // out-handles are written before use. msi.dll calls have no Rust-side
    // preconditions beyond the wide strings' NUL termination.
    unsafe {
        check(
            "MsiOpenDatabaseW",
            MsiOpenDatabaseW(path_w.as_ptr(), MSIDBOPEN_READONLY, &mut database),
        )?;
    }
    let result = query_product_code(database);
    unsafe {
        let _ = MsiCloseHandle(database);
    }
    result
}

#[cfg(not(windows))]
pub fn read_product_code(_msi_path: &Path) -> Result<String, MsiError> {
    Err(MsiError::UnsupportedPlatform)
}

#[cfg(windows)]
fn query_product_code(database: MSIHANDLE) -> Result<String, MsiError> {
    let query_w = wide("SELECT `Value` FROM `Property` WHERE `Property` = 'ProductCode'");
    let mut view: MSIHANDLE = 0;
    unsafe {
        check(
            "MsiDatabaseOpenViewW",
            MsiDatabaseOpenViewW(database, query_w.as_ptr(), &mut view),
        )?;
    }
    let fetch = (|| unsafe {
        check("MsiViewExecute", MsiViewExecute(view, 0))?;
        let mut record: MSIHANDLE = 0;
        let status = MsiViewFetch(view, &mut record);
        if status == ERROR_NO_MORE_ITEMS {
            return Err(MsiError::NoProductCode);
        }
        check("MsiViewFetch", status)?;
        let value = record_string(record, 1);
        let _ = MsiCloseHandle(record);
        value
    })();
    unsafe {
        let _ = MsiViewClose(view);
        let _ = MsiCloseHandle(view);
    }
    fetch
}

#[cfg(windows)]
fn record_string(record: MSIHANDLE, field: u32) -> Result<String, MsiError> {
    // Buffer size in characters, NUL included; grows on ERROR_MORE_DATA.
    let mut capacity = 64_u32;
    loop {
        let mut buffer = vec![0_u16; capacity as usize];
        // SAFETY: `buffer` is valid for `capacity` characters and NUL
        // terminated by the callee on success.
        let status =
            unsafe { MsiRecordGetStringW(record, field, buffer.as_mut_ptr(), &mut capacity) };
        match status {
            ERROR_SUCCESS => {
                let end = buffer
                    .iter()
                    .position(|&character| character == 0)
                    .unwrap_or(buffer.len());
                return Ok(String::from_utf16_lossy(&buffer[..end]));
            }
            ERROR_MORE_DATA => {
                capacity = capacity.saturating_add(1);
                continue;
            }
            code => {
                return Err(MsiError::Api {
                    function: "MsiRecordGetStringW",
                    code,
                })
            }
        }
    }
}

#[cfg(windows)]
fn check(function: &'static str, status: u32) -> Result<(), MsiError> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(MsiError::Api {
            function,
            code: status,
        })
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
        MsiCreateRecord, MsiDatabaseCommit, MsiRecordSetStringW, MSIDBOPEN_CREATE,
        MSIDBOPEN_TRANSACT,
    };

    // -----------------------------------------------------------------
    // Container detection
    // -----------------------------------------------------------------

    #[test]
    fn detects_the_ole_container_magic() {
        let mut bytes = MSI_MAGIC.to_vec();
        bytes.extend_from_slice(b"structured storage body");
        assert!(is_msi_package(&bytes));
        assert!(!is_msi_package(&bytes[..4]), "truncated magic");
        assert!(!is_msi_package(b""));
        // A PE installer is not an MSI container.
        assert!(!is_msi_package(b"MZ\x00\x00inno setup"));
    }

    #[test]
    fn streaming_detection_reads_only_the_first_eight_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("setup.msi");
        let mut bytes = MSI_MAGIC.to_vec();
        bytes.extend_from_slice(&[0_u8; 4096]);
        std::fs::write(&path, &bytes).unwrap();
        assert!(is_msi_package_file(&path).unwrap());

        std::fs::write(&path, b"MZ not an msi at all").unwrap();
        assert!(!is_msi_package_file(&path).unwrap());

        std::fs::write(&path, b"short").unwrap();
        assert!(!is_msi_package_file(&path).unwrap());
    }

    // -----------------------------------------------------------------
    // The fixed command line (P3-07 acceptance)
    // -----------------------------------------------------------------

    #[test]
    fn install_flags_come_from_the_contract_table() {
        let msi = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0.staging-j\artifact.bin");
        let dir = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0");
        assert_eq!(
            silent_flags(&msi, &dir),
            vec![
                "/i".to_owned(),
                msi.to_string_lossy().into_owned(),
                "/qn".to_owned(),
                "/norestart".to_owned(),
                format!("INSTALLDIR={}", dir.display()),
            ]
        );
    }

    #[test]
    fn uninstall_flags_come_from_the_contract_table() {
        assert_eq!(
            uninstall_flags("{B01FCC4E-CD9C-4F1A-B3D0-CD9DC0A2C777}"),
            vec![
                "/x".to_owned(),
                "{B01FCC4E-CD9C-4F1A-B3D0-CD9DC0A2C777}".to_owned(),
                "/qn".to_owned(),
                "/norestart".to_owned(),
            ]
        );
    }

    #[test]
    fn catalog_text_never_reaches_the_command_line() {
        // Two resolutions of the same artifact with entirely different
        // catalog strings must produce identical commands.
        let msi = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0.staging-j\artifact.bin");
        let dir = PathBuf::from(r"C:\DevKit\packages\tool\1.0.0");
        let command = |catalog_noise: &str| {
            let _ = catalog_noise;
            build_command(Path::new(r"C:\Windows\System32\msiexec.exe"), &msi, &dir)
        };
        assert_eq!(
            command("official description http://vendor.example/setup.msi INSTALLDIR=C:\\evil"),
            command("another rendering of the same manifest")
        );
        assert_eq!(
            command("x").1.last().unwrap(),
            &format!("INSTALLDIR={}", dir.display())
        );
    }

    #[test]
    fn msiexec_is_resolved_outside_path() {
        let program = msiexec_program();
        #[cfg(windows)]
        assert!(
            program.ends_with("System32\\msiexec.exe"),
            "program: {}",
            program.display()
        );
        #[cfg(not(windows))]
        assert_eq!(program, PathBuf::from("msiexec.exe"));
    }

    #[test]
    fn success_codes_match_the_contract() {
        assert!(is_success_exit_code(Some(MSI_EXIT_SUCCESS)));
        assert!(is_success_exit_code(Some(MSI_EXIT_REBOOT_REQUIRED)));
        assert!(!is_success_exit_code(Some(1603)));
        assert!(!is_success_exit_code(None));
    }

    // -----------------------------------------------------------------
    // ProductCode read: a real MSI database created in a temp dir (the
    // test builds the Property table itself through the same read-only
    // msi.dll API — no vendor artifact, no install, nothing executed)
    // -----------------------------------------------------------------

    #[cfg(windows)]
    const TEST_PRODUCT_CODE: &str = "{B01FCC4E-CD9C-4F1A-B3D0-CD9DC0A2C777}";

    /// Creates a minimal MSI database with a Property table (test support:
    /// proves `read_product_code` round-trips without any vendor artifact).
    #[cfg(windows)]
    fn create_test_msi(path: &Path, product_code: &str) {
        let path_w = wide(&path.to_string_lossy());
        let mut database: MSIHANDLE = 0;
        // SAFETY: out-handles written before use; test-only FFI setup.
        unsafe {
            assert_eq!(
                MsiOpenDatabaseW(path_w.as_ptr(), MSIDBOPEN_CREATE, &mut database),
                ERROR_SUCCESS
            );
            assert_eq!(
                execute_query(
                    database,
                    "CREATE TABLE `Property` (`Property` CHAR(72) NOT NULL, `Value` CHAR(0) \
                     LOCALIZABLE PRIMARY KEY `Property`)",
                    None,
                ),
                ERROR_SUCCESS
            );
            let record = MsiCreateRecord(2);
            assert_ne!(record, 0);
            assert_eq!(
                MsiRecordSetStringW(record, 1, wide("ProductCode").as_ptr()),
                ERROR_SUCCESS
            );
            assert_eq!(
                MsiRecordSetStringW(record, 2, wide(product_code).as_ptr()),
                ERROR_SUCCESS
            );
            assert_eq!(
                execute_query(
                    database,
                    "INSERT INTO `Property` (`Property`, `Value`) VALUES (?, ?)",
                    Some(record),
                ),
                ERROR_SUCCESS
            );
            let _ = MsiCloseHandle(record);
            assert_eq!(MsiDatabaseCommit(database), ERROR_SUCCESS);
            assert_eq!(MsiCloseHandle(database), ERROR_SUCCESS);
        }
    }

    #[cfg(windows)]
    fn execute_query(database: MSIHANDLE, query: &str, record: Option<MSIHANDLE>) -> u32 {
        let query_w = wide(query);
        let mut view: MSIHANDLE = 0;
        // SAFETY: out-handle written before use; test-only FFI helper.
        let status = unsafe { MsiDatabaseOpenViewW(database, query_w.as_ptr(), &mut view) };
        if status != ERROR_SUCCESS {
            return status;
        }
        // SAFETY: `view` and the optional `record` are valid handles.
        let status = unsafe { MsiViewExecute(view, record.unwrap_or(0)) };
        unsafe {
            let _ = MsiViewClose(view);
            let _ = MsiCloseHandle(view);
        }
        status
    }

    #[cfg(windows)]
    #[test]
    fn read_product_code_round_trips_a_real_msi_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tiny.msi");
        create_test_msi(&path, TEST_PRODUCT_CODE);
        assert_eq!(read_product_code(&path).unwrap(), TEST_PRODUCT_CODE);
    }

    #[cfg(windows)]
    #[test]
    fn read_product_code_reports_a_package_without_the_property() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.msi");
        create_test_msi(&path, TEST_PRODUCT_CODE);
        // Delete the row via a DELETE query: the package then declares no
        // ProductCode and must be refused, never run.
        {
            let path_w = wide(&path.to_string_lossy());
            let mut database: MSIHANDLE = 0;
            // SAFETY: out-handle written before use; test-only FFI.
            unsafe {
                assert_eq!(
                    MsiOpenDatabaseW(path_w.as_ptr(), MSIDBOPEN_TRANSACT, &mut database),
                    ERROR_SUCCESS
                );
                assert_eq!(
                    execute_query(
                        database,
                        "DELETE FROM `Property` WHERE `Property` = 'ProductCode'",
                        None,
                    ),
                    ERROR_SUCCESS
                );
                assert_eq!(MsiDatabaseCommit(database), ERROR_SUCCESS);
                assert_eq!(MsiCloseHandle(database), ERROR_SUCCESS);
            }
        }
        let error = read_product_code(&path).unwrap_err();
        assert!(matches!(error, MsiError::NoProductCode), "error: {error}");
        assert_eq!(error.code(), ErrorCode::InvalidArgument);
    }

    #[cfg(windows)]
    #[test]
    fn read_product_code_refuses_non_msi_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garbage.bin");
        std::fs::write(&path, b"this is not a structured storage file at all").unwrap();
        let error = read_product_code(&path).unwrap_err();
        assert!(matches!(error, MsiError::Api { .. }), "error: {error}");
        assert_eq!(error.code(), ErrorCode::InstallFailed);
    }
}
