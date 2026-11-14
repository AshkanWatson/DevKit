//! P3-01 transport policy tests: the reqwest transport's redirect policy
//! and error surface, exercised against the local fixture server (loopback
//! carve-out; no real hosts).

mod common;

use common::{payload, sha256_hex, spawn_fixture_server};
use devkit_download::{download_and_verify, CancelToken, DownloadError, DownloadRequest};
use tempfile::TempDir;

use tiny_http::{Header, Response};

fn transport() -> devkit_download::ReqwestDownloadHttp {
    devkit_download::ReqwestDownloadHttp::new().expect("build test transport")
}

fn location_header(target: &str) -> Header {
    Header::from_bytes(&b"Location"[..], target.as_bytes()).expect("valid header")
}

#[tokio::test]
async fn redirect_to_a_cross_host_is_blocked() {
    let port = spawn_fixture_server(move |request| {
        let response = Response::empty(tiny_http::StatusCode(302)).with_header(location_header(
            "http://evil.example.invalid/stolen-artifact.bin",
        ));
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let error = download_and_verify(
        &transport(),
        &DownloadRequest {
            url: format!("http://127.0.0.1:{port}/artifact.bin"),
            sha256_hex: sha256_hex(&payload(10)),
            size_bytes: 10,
        },
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("redirect"),
        "unexpected error: {error}"
    );
    assert!(!destination.exists());
}

#[tokio::test]
async fn redirect_to_another_scheme_is_blocked() {
    // On a loopback HTTP fixture there is no "downgrade" — the pure
    // decision function is unit-tested for https→http in src/http.rs; here
    // we verify that a scheme *switch* during a chain is blocked by the
    // same rule through the real transport.
    let port = spawn_fixture_server(move |request| {
        let response = Response::empty(tiny_http::StatusCode(302))
            .with_header(location_header("https://127.0.0.1:9/artifact.bin"));
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let error = download_and_verify(
        &transport(),
        &DownloadRequest {
            url: format!("http://127.0.0.1:{port}/artifact.bin"),
            sha256_hex: sha256_hex(&payload(10)),
            size_bytes: 10,
        },
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("redirect"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn same_host_same_scheme_redirect_is_followed() {
    let payload = payload(1000);
    let sha = sha256_hex(&payload);
    let port = spawn_fixture_server(move |request| {
        if request.url() == "/redirect" {
            // Relative Location resolves against the original URL, so the
            // redirect stays on the same host and scheme.
            let response = Response::empty(tiny_http::StatusCode(302))
                .with_header(location_header("/artifact.bin"));
            let _ = request.respond(response);
        } else {
            let response = tiny_http::Response::from_data(payload.clone());
            let _ = request.respond(response);
        }
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let downloaded = download_and_verify(
        &transport(),
        &DownloadRequest {
            url: format!("http://127.0.0.1:{port}/redirect"),
            sha256_hex: sha,
            size_bytes: 1000,
        },
        &destination,
        &CancelToken::new(),
    )
    .await
    .expect("same-host redirect must be followed");

    assert_eq!(downloaded.size_bytes, 1000);
}

#[tokio::test]
async fn too_many_redirects_is_blocked() {
    let port = spawn_fixture_server(move |request| {
        // Each /c<N> redirects to /c<N+1>; the chain runs past the cap.
        let url = request.url().to_owned();
        let next_index: usize = url
            .rsplit('c')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
            + 1;
        let response = Response::empty(tiny_http::StatusCode(302))
            .with_header(location_header(&format!("/c{next_index}")));
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let error = download_and_verify(
        &transport(),
        &DownloadRequest {
            url: format!("http://127.0.0.1:{port}/c0"),
            sha256_hex: sha256_hex(&payload(10)),
            size_bytes: 10,
        },
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("redirect"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn server_error_status_is_reported_without_leaking_the_url() {
    let port = spawn_fixture_server(move |request| {
        let response = Response::empty(tiny_http::StatusCode(404));
        let _ = request.respond(response);
    });
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("artifact.bin");

    let error = download_and_verify(
        &transport(),
        &DownloadRequest {
            url: format!("http://127.0.0.1:{port}/missing.bin?token=secret-value"),
            sha256_hex: sha256_hex(&payload(10)),
            size_bytes: 10,
        },
        &destination,
        &CancelToken::new(),
    )
    .await
    .unwrap_err();

    match error {
        DownloadError::Transport(message) => {
            // The query parameters are private (security model §8): the
            // message carries only the status.
            assert!(message.contains("HTTP 404"), "message: {message}");
            assert!(!message.contains("secret-value"));
        }
        other => panic!("expected transport error, got: {other}"),
    }
}
