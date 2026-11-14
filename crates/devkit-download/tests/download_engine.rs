//! P3-01 engine tests: download → verify against a **local fixture HTTP
//! server** on loopback (testing-strategy.md rule 1 — no real vendor
//! hosts). The loopback carve-out in the URL policy exists for exactly
//! this; production hosts are HTTPS-only.

mod common;

use std::time::Duration;

use common::{payload, sha256_hex, spawn_fixture_server, TrickleReader};
use devkit_download::{download_and_verify, CancelToken, DownloadError, DownloadRequest};
use tempfile::TempDir;

fn transport() -> devkit_download::ReqwestDownloadHttp {
    devkit_download::ReqwestDownloadHttp::new().expect("build test transport")
}

fn request(url: String, sha256_hex: String, size_bytes: u64) -> DownloadRequest {
    DownloadRequest {
        url,
        sha256_hex,
        size_bytes,
    }
}

#[tokio::test]
async fn happy_path_downloads_verifies_and_renames_into_place() {
    let payload = payload(100_000);
    let sha = sha256_hex(&payload);
    let size = payload.len() as u64;
    let expected = payload.clone();
    let port = spawn_fixture_server(move |request| {
        let response = tiny_http::Response::from_data(payload.clone());
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let downloaded = download_and_verify(
        &transport(),
        &request(
            format!("http://127.0.0.1:{port}/artifact.bin"),
            sha.clone(),
            size,
        ),
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(downloaded.size_bytes, 100_000);
    assert_eq!(downloaded.sha256_hex, sha);
    assert_eq!(std::fs::read(&destination).unwrap(), expected);
    // No partial file remains next to the verified artifact.
    assert!(!destination.with_extension("part").exists());
}

#[tokio::test]
async fn checksum_mismatch_discards_the_artifact_and_never_writes_the_destination() {
    let payload = payload(50_000);
    let size = payload.len() as u64;
    let port = spawn_fixture_server(move |request| {
        let response = tiny_http::Response::from_data(payload.clone());
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let error = download_and_verify(
        &transport(),
        &request(
            format!("http://127.0.0.1:{port}/artifact.bin"),
            "0".repeat(64),
            size,
        ),
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    match error {
        DownloadError::ChecksumMismatch { expected, .. } => {
            assert_eq!(expected, "0".repeat(64));
        }
        other => panic!("expected checksum mismatch, got: {other}"),
    }
    // The destination never materializes; partial bytes are gone.
    assert!(!destination.exists());
    assert!(!destination.with_extension("part").exists());
}

#[tokio::test]
async fn oversized_body_hits_the_hard_limit_and_is_discarded() {
    // Server sends more than declared; no Content-Length is declared so
    // the stream itself must be cut off by the hard limit.
    let payload = payload(300_000);
    let declared = 100_000u64; // declared far below the streamed size
    let sha = sha256_hex(&payload);
    let port = spawn_fixture_server(move |request| {
        // Unknown length → tiny_http streams without Content-Length.
        let response = tiny_http::Response::new(
            tiny_http::StatusCode(200),
            Vec::new(),
            payload.as_slice(),
            None,
            None,
        );
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let error = download_and_verify(
        &transport(),
        &request(
            format!("http://127.0.0.1:{port}/artifact.bin"),
            sha,
            declared, // declared far below the streamed size
        ),
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, DownloadError::SizeLimitExceeded { .. }));
    assert!(!destination.exists());
    assert!(!destination.with_extension("part").exists());
}

#[tokio::test]
async fn truncated_body_fails_the_declared_size_check() {
    // No Content-Length (chunked stream); the body ends early, so the
    // byte-count check (not the Content-Length check) must fire.
    let payload = payload(40_000);
    let sha = sha256_hex(&payload);
    let port = spawn_fixture_server(move |request| {
        let response = tiny_http::Response::new(
            tiny_http::StatusCode(200),
            Vec::new(),
            payload.as_slice(),
            None,
            None,
        );
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let error = download_and_verify(
        &transport(),
        &request(
            format!("http://127.0.0.1:{port}/artifact.bin"),
            sha,
            41_000, // declared larger than streamed
        ),
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, DownloadError::SizeMismatch { .. }));
    assert!(!destination.exists());
}

#[tokio::test]
async fn content_length_that_disagrees_with_the_declaration_fails_early() {
    // tiny_http sets Content-Length from the data; declaring a different
    // size makes the early Content-Length cross-check fire.
    let payload = payload(100);
    let sha = sha256_hex(&payload);
    let port = spawn_fixture_server(move |request| {
        let response = tiny_http::Response::from_data(payload.clone());
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let error = download_and_verify(
        &transport(),
        &request(
            format!("http://127.0.0.1:{port}/artifact.bin"),
            sha,
            50, // declared ≠ the server's Content-Length (100)
        ),
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, DownloadError::SizeMismatch { .. }));
}

#[tokio::test]
async fn cancellation_mid_download_stops_promptly_and_cleans_up() {
    let port = spawn_fixture_server(move |request| {
        let response = tiny_http::Response::new(
            tiny_http::StatusCode(200),
            Vec::new(),
            TrickleReader::new(64),
            None,
            None,
        );
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");
    let cancel = CancelToken::new();
    let download = request(
        format!("http://127.0.0.1:{port}/artifact.bin"),
        "0".repeat(64),
        1024 * 1024, // declared large; we cancel long before that
    );
    let staging = destination.clone();
    let inner_cancel = cancel.clone();

    let task = tokio::spawn(async move {
        let transport = devkit_download::ReqwestDownloadHttp::new().unwrap();
        download_and_verify(&transport, &download, &staging, &inner_cancel).await
    });

    // Give the download a few chunks, then cancel between them.
    tokio::time::sleep(Duration::from_millis(250)).await;
    cancel.cancel();

    let result = task.await.unwrap();
    assert!(matches!(result, Err(DownloadError::Cancelled)));
    assert!(!destination.exists());
    assert!(!destination.with_extension("part").exists());
}

#[tokio::test]
async fn cancelling_before_the_start_fails_without_any_fetch() {
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");
    let cancel = CancelToken::new();
    cancel.cancel();

    let error = download_and_verify(
        &transport(),
        &request(
            "http://127.0.0.1:1/artifact.bin".to_owned(),
            "0".repeat(64),
            1,
        ),
        &destination,
        &cancel,
    )
    .await
    .unwrap_err();

    assert!(matches!(error, DownloadError::Cancelled));
}

#[tokio::test]
async fn plain_http_to_a_non_loopback_host_is_rejected() {
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let error = download_and_verify(
        &transport(),
        &request(
            "http://releases.example.invalid/artifact.bin".to_owned(),
            "0".repeat(64),
            1,
        ),
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, DownloadError::InsecureUrl(_)));
    assert!(!destination.exists());
}

#[test]
fn url_policy_rejects_credentials_and_foreign_schemes() {
    use devkit_download::http::validate_url;

    assert!(matches!(
        validate_url("ftp://releases.example.invalid/a.bin"),
        Err(DownloadError::InsecureUrl(_))
    ));
    assert!(matches!(
        validate_url("https://user:pass@releases.example.invalid/a.bin"),
        Err(DownloadError::CredentialsInUrl)
    ));
    assert!(validate_url("https://releases.example.invalid/a.bin").is_ok());
    // The loopback carve-out accepts plain HTTP for fixture servers only.
    assert!(validate_url("http://127.0.0.1:1234/a.bin").is_ok());
    assert!(validate_url("http://localhost:1234/a.bin").is_ok());
    assert!(validate_url("http://[::1]:1234/a.bin").is_ok());
}
