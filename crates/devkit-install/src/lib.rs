//! Installer adapter layer (HANDOFF.md §7 `devkit-install`).
//!
//! P3-02 scope: the managed **archive** adapter - the first and safest
//! adapter per installation-lifecycle.md §2 - plus the shared entry-path
//! validation it is built on. P3-06 adds the **exe** adapter (vendor
//! installers with a fixed silent-install contract, Windows only) and
//! P3-07 the **msi** adapter (msiexec with a fixed silent contract +
//! ProductCode-driven uninstall); `dmg`/`pkg`/`appimage` follow later.
//! The install pipeline and receipts live in P3-03's `devkit-jobs`.
//!
//! The adapter consumes **verified** artifacts only: the caller must have
//! downloaded and SHA-256-verified the bytes with `devkit-download`
//! (security-model.md §4: verify → extract → install, in that order).
//! Nothing here touches global state (PATH, registry, env vars) —
//! activation is a separate, consent-gated step (§4).

pub mod archive;
pub mod error;
pub mod exe;
pub mod msi;
pub mod path;

pub use archive::{
    detect_format, extract_bytes, extract_file, ArchiveFormat, ArchiveLimits, ExtractResult,
    ExtractedFile,
};
pub use error::{ArchiveError, UnsafePathReason};
pub use exe::{
    build_command, detect_technology, detect_technology_from_file, silent_flags,
    walk_installed_files, ExeError, ExeRunOutcome, ExeRunner, ExeTechnology, ProcessExeRunner,
};
pub use msi::{
    build_command as msi_build_command, build_uninstall_command, is_msi_package,
    is_msi_package_file, is_success_exit_code as msi_is_success_exit_code, msiexec_program,
    read_product_code, MsiError, MSI_EXIT_REBOOT_REQUIRED, MSI_EXIT_SUCCESS,
    MSI_EXIT_UNKNOWN_PRODUCT,
};
pub use path::{ensure_within, validate_entry_path};
