//! Installer adapter layer (HANDOFF.md §7 `devkit-install`).
//!
//! P3-02 scope: the managed **archive** adapter — the first and safest
//! adapter per installation-lifecycle.md §2 — plus the shared entry-path
//! validation it is built on. `exe`/`msi` adapters follow in P3-06/P3-07;
//! the install pipeline and receipts in P3-03.
//!
//! The adapter consumes **verified** artifacts only: the caller must have
//! downloaded and SHA-256-verified the bytes with `devkit-download`
//! (security-model.md §4: verify → extract → install, in that order).
//! Nothing here touches global state (PATH, registry, env vars) —
//! activation is a separate, consent-gated step (§4).

pub mod archive;
pub mod error;
pub mod path;

pub use archive::{
    extract_bytes, extract_file, ArchiveFormat, ArchiveLimits, ExtractResult, ExtractedFile,
};
pub use error::{ArchiveError, UnsafePathReason};
pub use path::{ensure_within, validate_entry_path};
