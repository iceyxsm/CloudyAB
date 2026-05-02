//! Session and configuration types.

use serde::{Deserialize, Serialize};

use crate::fingerprint::FingerprintProfile;

/// Configuration for a CloudyAB browsing session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Target URL to navigate to
    pub target_url: String,
    /// Fingerprint profile to use (None = auto-select)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<FingerprintProfile>,
    /// Proxy configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<ProxyConfig>,
    /// Whether to persist cookies after session
    #[serde(default = "default_true")]
    pub persist_cookies: bool,
    /// Maximum time to wait for page load (seconds)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Which layer to prefer (None = auto-escalate)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_layer: Option<Layer>,
}

/// Proxy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Proxy URL (http://, https://, socks5://)
    pub url: String,
    /// Username for proxy auth
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Password for proxy auth
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

/// Which processing layer to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// HTTP-only with TLS stealth (fast, no browser)
    StealthHttp,
    /// Full headless browser engine
    Browser,
}

/// Result of a navigation/session operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationResult {
    /// Final URL after redirects
    pub final_url: String,
    /// HTTP status code
    pub status_code: u16,
    /// Which layer handled the request
    pub layer_used: Layer,
    /// Whether a captcha was encountered and solved
    pub captcha_solved: bool,
    /// Whether Cloudflare/WAF was bypassed
    pub protection_bypassed: bool,
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}
