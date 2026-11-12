//! DevKit core domain types.
//!
//! Phase 0 scope: the typed foundation that later phases build on:
//! - [`version`]: version policies and selection semantics (HANDOFF.md §5).
//! - [`error`]: the stable error-code model shared with the FFI contract
//!   (docs/ffi/bridge-contract.md).
//!
//! This crate must stay free of network, filesystem, and process access.
//! Those capabilities live in dedicated crates in later phases so that
//! security-sensitive behavior is isolated and reviewable.

pub mod error;
pub mod version;
