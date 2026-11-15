//! Download and verify engine (P3-01).
//!
//! Downloads a declared artifact over HTTPS into a staging file, verifies
//! its SHA-256 and size against the manifest declaration, and only then
//! hands the verified file to the caller. Implements the download rules of
//! `docs/security/security-model.md` §3/§4:
//!
//! - **HTTPS only** (§3.1): `http://` is rejected for every host except
//!   the loopback carve-out (127.0.0.1, ::1, localhost) that exists solely
//!   so the integration tests demanded by `docs/testing/testing-strategy.md`
//!   can run a local fixture HTTP server. Real vendor hosts are never
//!   exempt, and manifest validation keeps rejecting `http://` outright.
//! - **No credentials in URLs** (§3.2): userinfo is rejected.
//! - **Redirect policy** (§3.3): HTTPS-to-HTTPS (or loopback-to-loopback)
//!   only, same host, capped count.
//! - **Size limits and timeouts** (§3.4): the declared `size_bytes` plus a
//!   safety margin is a hard cap; connect/read timeouts are enforced by the
//!   production transport.
//! - **SHA-256 always** (§4): verified before the file is renamed into
//!   place — nothing downstream ever sees unverified bytes. A checksum
//!   mismatch discards the partial artifact; it is never extracted or
//!   executed.
//!
//! Cancellation is cooperative: the engine checks a [`CancelToken`] at
//! every chunk boundary and stops promptly, cleaning up the partial file.
//!
//! The HTTP transport is injectable (same pattern as the P2-04 catalog
//! client) so tests run against a local fixture server or an in-memory
//! fake — never real network hosts.

pub mod cancel;
pub mod error;
pub mod http;
pub mod verify;

pub use cancel::CancelToken;
pub use error::DownloadError;
pub use http::{ChunkStream, DownloadHttp, ReqwestDownloadHttp, Response};
pub use verify::{
    download_and_verify, download_and_verify_with_progress, DownloadRequest, Downloaded,
};
