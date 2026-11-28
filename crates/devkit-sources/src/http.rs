//! Injectable metadata HTTP seam (ADR 0009).
//!
//! Only official metadata endpoints are fetched here — never artifacts. The
//! same HTTPS/redirect policy as the download engine is enforced (ADR 0008),
//! and tests inject a fake so no test contacts a real host.

use std::time::Duration;

use devkit_download::http::{redirect_allowed, validate_url};
use reqwest::redirect::Attempt;

use crate::SourceError;

/// Reads official metadata (JSON/text) and optionally the declared size of an
/// artifact via HEAD. Injected so tests never touch the network.
#[async_trait::async_trait]
pub trait SourceHttp: Send + Sync {
    async fn get_text(&self, url: &str) -> Result<String, SourceError>;

    async fn head_length(&self, url: &str) -> Result<Option<u64>, SourceError>;
}

/// Production metadata client: reqwest over rustls (ADR 0002), HTTPS-only,
/// with the same narrow redirect allowlist as the download engine (ADR 0008).
pub struct ReqwestSourceHttp {
    client: reqwest::Client,
}

impl Default for ReqwestSourceHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestSourceHttp {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::custom(|attempt: Attempt| {
                let Some(original) = attempt.previous().first() else {
                    return attempt.stop();
                };
                match redirect_allowed(original, attempt.url(), attempt.previous().len()) {
                    Ok(()) => attempt.follow(),
                    Err(reason) => attempt.error(reason),
                }
            }))
            .build()
            .expect("reqwest client with rustls must build");
        Self { client }
    }
}

#[async_trait::async_trait]
impl SourceHttp for ReqwestSourceHttp {
    async fn get_text(&self, url: &str) -> Result<String, SourceError> {
        validate_url(url).map_err(|error| SourceError::Http(error.to_string()))?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| SourceError::Http(error.to_string()))?;
        response
            .error_for_status()
            .map_err(|error| SourceError::Http(error.to_string()))?
            .text()
            .await
            .map_err(|error| SourceError::Http(error.to_string()))
    }

    async fn head_length(&self, url: &str) -> Result<Option<u64>, SourceError> {
        validate_url(url).map_err(|error| SourceError::Http(error.to_string()))?;
        let response = self
            .client
            .head(url)
            .send()
            .await
            .map_err(|error| SourceError::Http(error.to_string()))?;
        let response = response
            .error_for_status()
            .map_err(|error| SourceError::Http(error.to_string()))?;
        Ok(response.content_length())
    }
}
