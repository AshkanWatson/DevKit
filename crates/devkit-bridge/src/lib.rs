//! DevKit FFI boundary (flutter_rust_bridge v2).
//!
//! `src/api` is the generated surface: every `pub` function in this module
//! becomes a typed Dart call. See docs/ffi/bridge-contract.md for the
//! operation and error models. Never expose raw command execution here.

mod frb_generated; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */

pub mod api;
