//! Layer routing logic - decides whether to use HTTP stealth or full browser.

use cloudyab_types::{Layer, SessionConfig};
use tracing::info;

/// Determines which layer should handle a request.
pub struct LayerRouter {
    /// Whether auto-escalation is enabled
    auto_escalate: bool,
}

impl LayerRouter {
    /// Create a new router with the given configuration.
    pub fn new(auto_escalate: bool) -> Self {
        Self { auto_escalate }
    }

    /// Decide which layer to use for the given session config.
    pub fn route(&self, config: &SessionConfig) -> Layer {
        if let Some(layer) = config.preferred_layer {
            info!(layer = ?layer, "Using explicitly requested layer");
            return layer;
        }

        let url = &config.target_url;

        if self.requires_browser(url) {
            info!(url, "Routing to browser layer (protection detected)");
            Layer::Browser
        } else {
            info!(url, "Routing to stealth HTTP layer (lightweight)");
            Layer::StealthHttp
        }
    }

    /// Check if a URL is known to require full browser rendering.
    fn requires_browser(&self, url: &str) -> bool {
        let browser_patterns = [
            "login",
            "signin",
            "sign-in",
            "auth",
            "captcha",
            "challenge",
            "verify",
        ];
        let url_lower = url.to_lowercase();
        browser_patterns.iter().any(|p| url_lower.contains(p))
    }

    /// Whether to auto-escalate if the HTTP layer fails.
    pub fn should_escalate(&self) -> bool {
        self.auto_escalate
    }
}
