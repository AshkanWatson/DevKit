//! Errors of the download engine. Stable codes for the job layer arrive
//! with P3-03; these variants carry the structured data it will map.

use std::path::PathBuf;

/// Download and verification failures (security-model.md §3/§4).
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// URL did not parse or is missing a host.
    #[error("invalid download URL: {0}")]
    InvalidUrl(String),

    /// `http://` for a non-loopback host (security model §3.1).
    #[error("insecure download URL: only HTTPS is allowed (http://example.invalid)")]
    InsecureUrl(String),

    /// URL carries userinfo credentials (security model §3.2).
    #[error("download URL must not carry credentials (userinfo)")]
    CredentialsInUrl,

    /// Redirect violated the policy: cross-scheme, cross-host, or over the
    /// cap (security model §3.3).
    #[error("redirect policy violation: {0}")]
    Redirect(String),

    /// The HTTP transport failed (connection, DNS, TLS, timeout). The
    /// underlying error text never contains URL query parameters.
    #[error("download failed: {0}")]
    Transport(String),

    /// The body exceeded the declared size plus safety margin
    /// (security model §3.4); partial data is discarded.
    #[error("download exceeded the size limit: declared {declared} bytes, hard limit {limit}, got {actual}")]
    SizeLimitExceeded {
        declared: u64,
        limit: u64,
        actual: u64,
    },

    /// The body size did not match the declared `size_bytes`.
    #[error("downloaded size mismatch: declared {declared} bytes, got {actual}")]
    SizeMismatch { declared: u64, actual: u64 },

    /// SHA-256 mismatch (security model §4): artifact discarded, never
    /// extracted or executed.
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// The manifest's `sha256` field is not a 64-hex digest.
    #[error("manifest checksum field is not a SHA-256 digest: {0}")]
    BadChecksumSpec(String),

    /// Cancellation was requested; the partial file was removed.
    #[error("download cancelled")]
    Cancelled,

    /// Filesystem failure while staging the artifact.
    #[error("staging file failure: {0}")]
    Io(#[source] std::io::Error),

    /// Filesystem failure with the file context (staging path is safe to
    /// show: it is under the managed root, not a secret).
    #[error("staging file failure for {}: {source}", path.display())]
    IoAtPath {
        path: PathBuf,
        source: std::io::Error,
    },
}
