//! Stealth HTTP client with browser-like headers and TLS fingerprinting.

use cloudyab_types::fingerprint::FingerprintProfile;
use reqwest::Client;
use thiserror::Error;
use tracing::debug;

/// Errors from the stealth HTTP layer.
#[derive(Debug, Error)]
pub enum StealthHttpError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Cloudflare challenge detected, needs browser escalation")]
    CloudflareChallenge,

    #[error("AWS WAF challenge detected, needs browser escalation")]
    AwsWafChallenge,

    #[error("JS challenge solving failed: {0}")]
    ChallengeFailed(String),

    #[error("TLS configuration error: {0}")]
    TlsConfig(String),
}

/// A stealth HTTP client that mimics real browser traffic.
pub struct StealthClient {
    client: Client,
    fingerprint: FingerprintProfile,
}

impl StealthClient {
    /// Create a new stealth client with the given fingerprint profile.
    pub fn new(fingerprint: FingerprintProfile) -> Result<Self, StealthHttpError> {
        let client = Client::builder()
            .user_agent(&fingerprint.navigator.user_agent)
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .build()
            .map_err(StealthHttpError::Request)?;

        debug!(
            ua = %fingerprint.navigator.user_agent,
            "Created stealth HTTP client"
        );

        Ok(Self {
            client,
            fingerprint,
        })
    }

    /// Make a GET request with browser-like headers.
    pub async fn get(&self, url: &str) -> Result<StealthResponse, StealthHttpError> {
        let response = self
            .client
            .get(url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
            .header("Accept-Language", &self.fingerprint.navigator.language)
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("Cache-Control", "max-age=0")
            .header("Sec-Ch-Ua-Mobile", "?0")
            .header("Sec-Ch-Ua-Platform", format!("\"{}\"", self.fingerprint.os.platform))
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Sec-Fetch-User", "?1")
            .header("Upgrade-Insecure-Requests", "1")
            .send()
            .await
            .map_err(StealthHttpError::Request)?;

        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.text().await.map_err(StealthHttpError::Request)?;

        // Detect Cloudflare challenge pages
        if status == 403 || status == 503 {
            if body.contains("cf-browser-verification")
                || body.contains("cf_chl_opt")
                || body.contains("jschl_vc")
                || body.contains("__cf_chl_f_tk")
            {
                return Err(StealthHttpError::CloudflareChallenge);
            }

            if body.contains("awswaf") || body.contains("aws-waf-token") {
                return Err(StealthHttpError::AwsWafChallenge);
            }
        }

        Ok(StealthResponse {
            status,
            headers,
            body,
        })
    }

    /// Get the fingerprint profile being used.
    pub fn fingerprint(&self) -> &FingerprintProfile {
        &self.fingerprint
    }
}

impl StealthClient {
    /// Make a GET request that does NOT check for challenges (used during challenge flow).
    pub async fn get_raw(&self, url: &str) -> Result<StealthResponse, StealthHttpError> {
        let response = self
            .client
            .get(url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
            .header("Accept-Language", &self.fingerprint.navigator.language)
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("Cache-Control", "max-age=0")
            .header("Sec-Ch-Ua-Mobile", "?0")
            .header("Sec-Ch-Ua-Platform", format!("\"{}\"", self.fingerprint.os.platform))
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Sec-Fetch-User", "?1")
            .header("Upgrade-Insecure-Requests", "1")
            .send()
            .await
            .map_err(StealthHttpError::Request)?;

        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.text().await.map_err(StealthHttpError::Request)?;

        Ok(StealthResponse {
            status,
            headers,
            body,
        })
    }

    /// Submit a Cloudflare challenge answer via POST with form data.
    pub async fn post_challenge_answer(
        &self,
        original_url: &str,
        submit_path: &str,
        form_params: &[(String, String)],
        answer: &str,
    ) -> Result<StealthResponse, StealthHttpError> {
        let base = url::Url::parse(original_url).map_err(|e| {
            StealthHttpError::ChallengeFailed(format!("Invalid URL: {e}"))
        })?;
        let submit_url = base.join(submit_path).map_err(|e| {
            StealthHttpError::ChallengeFailed(format!("Invalid submit path: {e}"))
        })?;

        let mut form = form_params.to_vec();
        form.push(("jschl_answer".into(), answer.into()));

        debug!(url = %submit_url, "Submitting challenge answer");

        let response = self
            .client
            .post(submit_url.as_str())
            .header("Referer", original_url)
            .header("Origin", format!("{}://{}", base.scheme(), base.host_str().unwrap_or("")))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", &self.fingerprint.navigator.language)
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "same-origin")
            .header("Upgrade-Insecure-Requests", "1")
            .form(&form)
            .send()
            .await
            .map_err(StealthHttpError::Request)?;

        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.text().await.map_err(StealthHttpError::Request)?;

        Ok(StealthResponse {
            status,
            headers,
            body,
        })
    }
}

/// Response from a stealth HTTP request.
pub struct StealthResponse {
    /// HTTP status code
    pub status: u16,
    /// Response headers
    pub headers: reqwest::header::HeaderMap,
    /// Response body as text
    pub body: String,
}
