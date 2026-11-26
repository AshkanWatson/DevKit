//! DevKit core domain types.
//!
//! Phase 0 scope: the typed foundation that later phases build on:
//! - [`version`]: version policies and selection semantics (HANDOFF.md §5).
//! - [`error`]: the stable error-code model shared with the FFI contract
//!   (docs/ffi/bridge-contract.md).
//! - [`manifest`]: package/profile manifest parsing and validation (P1-03).
//! - [`profile`]: profile resolution into a concrete install plan (P4-01).
//! - [`project`]: project-level version pin resolution (P4-05).
//!
//! This crate must stay free of network, filesystem, and process access.
//! Those capabilities live in dedicated crates in later phases so that
//! security-sensitive behavior is isolated and reviewable.
//!
//! Exception to the last rule: `manifest` unit tests read the committed
//! schema fixtures under `schemas/examples/` (test-only filesystem access).

pub mod error;
pub mod manifest;
pub mod profile;
pub mod project;
pub mod version;
