//! The download-and-verify engine (security model §3/§4).
//!
//! Order matters and is fixed: stream to a `.part` staging file → check
//! declared size → verify SHA-256 → rename into place. Only verified bytes
//! ever reach the destination; any failure removes the partial file and
//! the caller retries with a fresh staging directory.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::cancel::CancelToken;
use crate::error::DownloadError;
use crate::http::{validate_url, ChunkStream, DownloadHttp};

/// A manifest artifact download declaration (schemas/package.schema.json
/// `artifacts[]`). `sha256` must be a 64-hex digest; `size_bytes` is the
/// declared size.
pub struct DownloadRequest {
    pub url: String,
    pub sha256_hex: String,
    pub size_bytes: u64,
}

/// A verified artifact. `path` is the caller-provided destination.
#[derive(Debug)]
pub struct Downloaded {
    pub path: PathBuf,
    pub sha256_hex: String,
    pub size_bytes: u64,
}

/// Hard size cap: declared size plus the larger of a 10% safety margin
/// and 64 KiB of headroom (security model §3.4).
fn hard_limit(declared: u64) -> u64 {
    declared.saturating_add((declared / 10).max(64 * 1024))
}

/// Parses and validates a 64-hex SHA-256 declaration.
fn parse_digest(sha256_hex: &str) -> Result<[u8; 32], DownloadError> {
    let hex = sha256_hex.trim();
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(DownloadError::BadChecksumSpec(hex.to_owned()));
    }
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| DownloadError::BadChecksumSpec(hex.to_owned()))?;
    }
    Ok(digest)
}

/// Downloads `request` into `destination` (staging directory) and verifies
/// it. The partial file is `destination.part` and is renamed only after
/// verification succeeds. Any failure removes the partial file.
pub async fn download_and_verify<T: DownloadHttp>(
    transport: &T,
    request: &DownloadRequest,
    destination: &Path,
    cancel: &CancelToken,
) -> Result<Downloaded, DownloadError> {
    if cancel.is_cancelled() {
        return Err(DownloadError::Cancelled);
    }
    // URL policy is enforced here for every transport, not only reqwest.
    validate_url(&request.url)?;
    let expected = parse_digest(&request.sha256_hex)?;

    let partial = destination.with_extension("part");
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| DownloadError::IoAtPath {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let result = download_inner(transport, request, destination, &partial, cancel, expected).await;
    match result {
        Ok(downloaded) => Ok(downloaded),
        Err(error) => {
            // Discard partial bytes on every failure: a checksum mismatch
            // is never extracted or executed (security model §4), and a
            // cancelled download leaves nothing behind.
            let _ = fs::remove_file(&partial);
            Err(error)
        }
    }
}

async fn download_inner<T: DownloadHttp>(
    transport: &T,
    request: &DownloadRequest,
    destination: &Path,
    partial: &Path,
    cancel: &CancelToken,
    expected: [u8; 32],
) -> Result<Downloaded, DownloadError> {
    let response = transport.get(&request.url).await?;

    // Content-Length cross-check when the server declares one.
    if let Some(content_length) = response.content_length {
        if content_length != request.size_bytes {
            return Err(DownloadError::SizeMismatch {
                declared: request.size_bytes,
                actual: content_length,
            });
        }
    }

    let limit = hard_limit(request.size_bytes);
    let mut hasher = Sha256::new();
    let mut body = response.body;
    let mut size: u64 = 0;

    let mut file = fs::File::create(partial).map_err(|source| DownloadError::IoAtPath {
        path: partial.to_path_buf(),
        source,
    })?;

    while let Some(chunk) = body.next_chunk().await? {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        size = size.saturating_add(chunk.len() as u64);
        if size > limit {
            return Err(DownloadError::SizeLimitExceeded {
                declared: request.size_bytes,
                limit,
                actual: size,
            });
        }
        hasher.update(&chunk);
        use std::io::Write;
        file.write_all(&chunk)
            .map_err(|source| DownloadError::IoAtPath {
                path: partial.to_path_buf(),
                source,
            })?;
    }

    if size != request.size_bytes {
        return Err(DownloadError::SizeMismatch {
            declared: request.size_bytes,
            actual: size,
        });
    }

    let actual = hasher.finalize();
    if actual.as_slice() != expected {
        return Err(DownloadError::ChecksumMismatch {
            expected: hex_of(&expected),
            actual: hex_of(&actual),
        });
    }

    drop(file);
    fs::rename(partial, destination).map_err(|source| DownloadError::IoAtPath {
        path: destination.to_path_buf(),
        source,
    })?;

    Ok(Downloaded {
        path: destination.to_path_buf(),
        sha256_hex: hex_of(&actual),
        size_bytes: size,
    })
}

fn hex_of(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_limit_gives_ten_percent_margin_and_minimum_headroom() {
        assert_eq!(hard_limit(0), 64 * 1024);
        // Small declared sizes still get the 64 KiB minimum headroom.
        assert_eq!(hard_limit(1000), 1000 + 64 * 1024);
        assert_eq!(hard_limit(1024), 1024 + 64 * 1024);
        assert_eq!(hard_limit(1_000_000), 1_100_000);
    }

    #[test]
    fn parse_digest_accepts_64_hex_and_rejects_other_shapes() {
        let digest = parse_digest(&"a".repeat(64)).unwrap();
        assert_eq!(digest, [0xaa; 32]);
        assert!(matches!(
            parse_digest("aa"),
            Err(DownloadError::BadChecksumSpec(_))
        ));
        assert!(matches!(
            parse_digest(&"g".repeat(64)),
            Err(DownloadError::BadChecksumSpec(_))
        ));
    }
}
