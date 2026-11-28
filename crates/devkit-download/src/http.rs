//! Injectable HTTP transport and the production reqwest implementation.
//!
//! Same seam as the P2-04 catalog client: the engine is generic over
//! [`DownloadHttp`], tests plug in a fixture-server or in-memory transport,
//! and only [`ReqwestDownloadHttp`] touches real sockets.

use std::future::Future;
use std::time::Duration;

use bytes::Bytes;
use reqwest::redirect::Attempt;
use reqwest::Url;

use crate::error::DownloadError;

/// One streamed response body.
pub struct Response<S> {
    /// `Content-Length` from the server, when present. Cross-checked
    /// against the manifest size by the engine (security model §3.4).
    pub content_length: Option<u64>,
    pub body: S,
}

/// A transport that can fetch a URL and stream its body in chunks.
///
/// Implementations enforce their own URL/redirect policies
/// (defense in depth: the engine validates too). Futures are `Send` so the
/// engine can run on the bridge's multi-threaded executor.
pub trait DownloadHttp: Send + Sync {
    type Stream: ChunkStream;

    fn get(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<Response<Self::Stream>, DownloadError>> + Send;
}

/// One chunk of a response body; `None` at the end.
pub trait ChunkStream {
    fn next_chunk(&mut self) -> impl Future<Output = Result<Option<Bytes>, DownloadError>> + Send;
}

/// Is this host the loopback carve-out (security model §3.1 as clarified
/// for fixture-server tests)? Only these three are allowed: `127.0.0.1`,
/// `::1` (bracketed or bare), `localhost`.
pub fn is_loopback_host(host: &str) -> bool {
    matches!(
        host.trim_matches(['[', ']']),
        "127.0.0.1" | "::1" | "localhost"
    )
}

/// Validates a download URL before anything is fetched (security model
/// §3.1/§3.2). HTTPS for every host; the loopback carve-out additionally
/// accepts plain `http://` so local fixture servers work in tests.
/// Userinfo credentials are always rejected.
pub fn validate_url(url: &str) -> Result<Url, DownloadError> {
    let parsed = Url::parse(url).map_err(|_| DownloadError::InvalidUrl(url.to_owned()))?;
    let host = parsed.host_str().unwrap_or_default().to_owned();
    if host.is_empty() {
        return Err(DownloadError::InvalidUrl(url.to_owned()));
    }
    match parsed.scheme() {
        "https" => {}
        "http" if is_loopback_host(&host) => {}
        _ => return Err(DownloadError::InsecureUrl(url.to_owned())),
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(DownloadError::CredentialsInUrl);
    }
    Ok(parsed)
}

/// GitHub's official release hosts. A redirect from one of these to the
/// asset-delivery allowlist below is a legitimate CDN hop, not a policy bypass
/// (ADR 0008). These are compile-time constants — catalog data can never add a
/// trusted destination.
const GITHUB_RELEASE_HOSTS: &[&str] = &["github.com"];
const GITHUB_ASSET_HOSTS: &[&str] = &[
    "objects.githubusercontent.com",
    "github-releases.githubusercontent.com",
];

/// The redirect decision (security model §3.3, ADR 0008), pure so it can be
/// unit-tested without any HTTP.
///
/// Allowed:
/// - same host + HTTPS (the original rule), and
/// - HTTPS original on a GitHub release host → HTTPS target on GitHub's
///   asset-delivery allowlist (the legitimate `github.com` → CDN hop).
///
/// Rejected: HTTPS→HTTP downgrades, cross-host redirects to any other host,
/// userinfo credentials (rejected upstream by `validate_url`), and chains
/// longer than 5. The loopback carve-out keeps fixture-server tests working.
pub fn redirect_allowed(original: &Url, next: &Url, previous_count: usize) -> Result<(), String> {
    if previous_count > 5 {
        return Err("too many redirects".to_owned());
    }
    // Credentials are never forwarded across a redirect hop.
    if !next.username().is_empty() || next.password().is_some() {
        return Err("credentials in redirect target".to_owned());
    }
    let original_host = original.host_str().unwrap_or_default().to_ascii_lowercase();
    let next_host = next.host_str().unwrap_or_default().to_ascii_lowercase();
    let same_host = next_host == original_host;
    let same_scheme = next.scheme() == original.scheme();
    // No HTTPS→HTTP downgrade: target must be HTTPS or the loopback carve-out.
    let scheme_allowed =
        next.scheme() == "https" || is_loopback_host(next.host_str().unwrap_or_default());
    if same_host && same_scheme && scheme_allowed {
        return Ok(());
    }
    // Trusted cross-host hop (ADR 0008): GitHub release host → GitHub asset
    // CDN, both HTTPS, never carrying URL credentials.
    if next.scheme() == "https"
        && GITHUB_RELEASE_HOSTS.contains(&original_host.as_str())
        && GITHUB_ASSET_HOSTS.contains(&next_host.as_str())
    {
        return Ok(());
    }
    Err("redirect leaves the approved host or scheme".to_owned())
}

/// Production transport: reqwest over rustls (ADR 0002), HTTPS-only with
/// the loopback carve-out, connect/read timeouts, and a redirect policy
/// that stays same-scheme + same-host under a hard cap (security model
/// §3.3). No certificate bypass exists (§3.5).
pub struct ReqwestDownloadHttp {
    client: reqwest::Client,
}

impl ReqwestDownloadHttp {
    /// Builds the client with conservative timeouts.
    pub fn new() -> Result<Self, DownloadError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            // Redirect policy: HTTPS→HTTPS (or loopback→loopback), same
            // host, at most 5 redirects. The original request URL is
            // `previous().first()`.
            .redirect(reqwest::redirect::Policy::custom(|attempt: Attempt| {
                let Some(original) = attempt.previous().first() else {
                    return attempt.stop(); // not a redirect; reqwest decides
                };
                match redirect_allowed(original, attempt.url(), attempt.previous().len()) {
                    Ok(()) => attempt.follow(),
                    Err(reason) => attempt.error(reason),
                }
            }))
            .build()
            .map_err(|error| DownloadError::Transport(error.to_string()))?;
        Ok(Self { client })
    }

    /// Validates the URL at the transport level as well (the engine
    /// validates independently).
    fn checked_url(&self, url: &str) -> Result<Url, DownloadError> {
        validate_url(url)
    }
}

/// Builds a transport error message from a reqwest failure. The top-level
/// reqwest message embeds the request URL (which can carry private query
/// parameters — security model §8), so only the cause chain is reported.
fn transport_message(error: &reqwest::Error) -> String {
    let mut causes = Vec::new();
    let mut current: Option<&dyn std::error::Error> = Some(error);
    while let Some(cause) = current.and_then(std::error::Error::source) {
        causes.push(cause.to_string());
        current = Some(cause);
    }
    if causes.is_empty() {
        "request failed".to_owned()
    } else {
        causes.join(": ")
    }
}

pub struct ReqwestChunkStream {
    inner: reqwest::Response,
}

impl ChunkStream for ReqwestChunkStream {
    // Explicit RPITIT instead of `async fn` syntax: the `+ Send` bound must
    // stay explicit so the engine can run on the bridge's multi-threaded
    // executor (`async fn` in a trait would lose that guarantee).
    #[allow(clippy::manual_async_fn)]
    fn next_chunk(&mut self) -> impl Future<Output = Result<Option<Bytes>, DownloadError>> + Send {
        async {
            self.inner
                .chunk()
                .await
                .map_err(|error| DownloadError::Transport(transport_message(&error)))
        }
    }
}

impl DownloadHttp for ReqwestDownloadHttp {
    type Stream = ReqwestChunkStream;

    fn get(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<Response<Self::Stream>, DownloadError>> + Send {
        let url = self.checked_url(url);
        async move {
            let url = url?;
            let response = self
                .client
                .get(url.clone())
                .send()
                .await
                .map_err(|error| DownloadError::Transport(transport_message(&error)))?;
            let status = response.status();
            if !status.is_success() {
                // Never leaks URL query parameters — only the status.
                return Err(DownloadError::Transport(format!(
                    "server answered HTTP {status}"
                )));
            }
            let content_length = response
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            Ok(Response {
                content_length,
                body: ReqwestChunkStream { inner: response },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn redirects_must_stay_same_host_and_same_scheme() {
        let original = url("https://releases.example.invalid/a.bin");

        // Same host and scheme: allowed.
        assert!(redirect_allowed(
            &original,
            &url("https://releases.example.invalid/mirror/a.bin"),
            1
        )
        .is_ok());

        // HTTPS→HTTP downgrade: blocked even on the same host.
        assert_eq!(
            redirect_allowed(&original, &url("http://releases.example.invalid/a.bin"), 1)
                .unwrap_err(),
            "redirect leaves the approved host or scheme"
        );

        // HTTP→HTTPS scheme switch: blocked too (same rule, no exceptions
        // once a redirect chain has started on a different scheme).
        let loopback_original = url("http://127.0.0.1:9000/a.bin");
        assert!(
            redirect_allowed(&loopback_original, &url("https://127.0.0.1:9000/a.bin"), 1).is_err()
        );

        // Cross-host: blocked, even over HTTPS.
        assert!(
            redirect_allowed(&original, &url("https://evil.example.invalid/a.bin"), 1).is_err()
        );

        // The loopback carve-out only allows plain HTTP to loopback.
        assert!(redirect_allowed(
            &loopback_original,
            &url("http://releases.example.invalid/a.bin"),
            1
        )
        .is_err());
    }

    #[test]
    fn github_release_host_may_redirect_to_asset_cdn() {
        // ADR 0008: the legitimate github.com → objects.githubusercontent.com
        // hop must be allowed; the artifact is still SHA-256 verified later.
        let original = url("https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/rg.zip");
        assert!(redirect_allowed(
            &original,
            &url("https://objects.githubusercontent.com/asset/rg.zip"),
            1
        )
        .is_ok());
        assert!(redirect_allowed(
            &original,
            &url("https://github-releases.githubusercontent.com/asset/rg.zip"),
            1
        )
        .is_ok());
    }

    #[test]
    fn cross_host_redirect_to_untrusted_host_is_still_blocked() {
        // The allowlist is narrow: a github.com original may NOT redirect to
        // arbitrary hosts, and non-GitHub originals may not cross hosts at all.
        let original = url("https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/rg.zip");
        assert!(
            redirect_allowed(&original, &url("https://evil.example.invalid/rg.zip"), 1).is_err()
        );
        let other = url("https://releases.example.invalid/a.bin");
        assert!(redirect_allowed(
            &other,
            &url("https://objects.githubusercontent.com/a.bin"),
            1
        )
        .is_err());
    }

    #[test]
    fn github_cdn_redirect_rejects_downgrade_and_credentials() {
        let original = url("https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/rg.zip");
        // HTTPS→HTTP downgrade to the CDN is rejected.
        assert!(redirect_allowed(
            &original,
            &url("http://objects.githubusercontent.com/rg.zip"),
            1
        )
        .is_err());
        // Credentials in the redirect target are never forwarded.
        assert!(redirect_allowed(
            &original,
            &url("https://user:pass@objects.githubusercontent.com/rg.zip"),
            1
        )
        .is_err());
    }

    #[test]
    fn redirect_chain_is_capped() {
        let original = url("https://releases.example.invalid/a.bin");
        let next = url("https://releases.example.invalid/a.bin");
        assert!(redirect_allowed(&original, &next, 5).is_ok());
        assert_eq!(
            redirect_allowed(&original, &next, 6).unwrap_err(),
            "too many redirects"
        );
    }

    #[test]
    fn loopback_host_detection() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("[::1]"));
        assert!(!is_loopback_host("releases.example.invalid"));
        assert!(!is_loopback_host("0.0.0.0"));
    }
}
