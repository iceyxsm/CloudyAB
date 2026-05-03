//! Core engine traits that each layer must implement.

use async_trait::async_trait;
use cloudyab_types::{
    CaptchaResult, CaptchaSolution, CaptchaType, Cookie, CookieJar, NavigationResult, PageSnapshot,
    SessionConfig, SnapshotOptions,
};

/// Detected challenge information returned by engines.
#[derive(Debug, Clone)]
pub struct DetectedChallenge {
    /// The type of captcha/challenge detected.
    pub captcha_type: CaptchaType,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// CSS selector for the challenge container element.
    pub container_selector: Option<String>,
    /// Whether this is a full-page interstitial (vs embedded widget).
    pub is_interstitial: bool,
}

/// Trait for a browsing engine (HTTP stealth or full browser).
#[async_trait]
pub trait BrowsingEngine: Send + Sync {
    /// Navigate to a URL and return the result.
    async fn navigate(&self, config: &SessionConfig) -> Result<NavigationResult, EngineError>;

    /// Get the current page's accessibility tree snapshot.
    async fn snapshot(&self, options: &SnapshotOptions) -> Result<PageSnapshot, EngineError>;

    /// Click an element by its @eN ref.
    async fn click(&self, ref_id: &str) -> Result<(), EngineError>;

    /// Fill a text input by its @eN ref.
    async fn fill(&self, ref_id: &str, text: &str) -> Result<(), EngineError>;

    /// Type text with realistic keystroke timing.
    async fn type_text(&self, ref_id: &str, text: &str) -> Result<(), EngineError>;

    /// Take a screenshot and return PNG bytes.
    async fn screenshot(&self) -> Result<Vec<u8>, EngineError>;

    /// Get all cookies from the current session.
    async fn get_cookies(&self) -> Result<CookieJar, EngineError>;

    /// Set cookies for the current session.
    async fn set_cookies(&self, cookies: &[Cookie]) -> Result<(), EngineError>;

    /// Get the current page URL.
    async fn current_url(&self) -> Result<String, EngineError>;

    /// Check if the engine can handle this URL without escalation.
    async fn can_handle(&self, url: &str) -> bool;

    /// Detect challenges/captchas on the current page using DOM inspection.
    /// Returns detected challenges sorted by confidence (highest first).
    /// Default implementation returns empty (no detection capability).
    async fn detect_challenges(&self) -> Vec<DetectedChallenge> {
        Vec::new()
    }

    /// Submit a captcha solution to the page.
    /// Handles different solution types (token injection, text input, slider drag, coordinates).
    /// Default implementation returns an error (not supported by this engine).
    async fn submit_solution(
        &self,
        _solution: &CaptchaSolution,
        _container_selector: Option<&str>,
    ) -> Result<(), EngineError> {
        Err(EngineError::Internal(
            "Solution submission not supported by this engine".into(),
        ))
    }

    /// Execute arbitrary JavaScript on the current page and return the result.
    /// Used by the server to inject human-like interaction sequences.
    /// Default implementation returns an error (not supported by this engine).
    async fn evaluate_js(&self, _js: &str) -> Result<serde_json::Value, EngineError> {
        Err(EngineError::Internal(
            "JS evaluation not supported by this engine".into(),
        ))
    }

    /// Name of this engine for logging.
    fn name(&self) -> &str;
}

/// Trait for captcha solving backends.
#[async_trait]
pub trait CaptchaSolver: Send + Sync {
    /// Attempt to solve a captcha from a screenshot.
    async fn solve(
        &self,
        image: &[u8],
        captcha_type: &CaptchaType,
        context: &str,
    ) -> Result<CaptchaResult, EngineError>;

    /// Check if this solver supports the given captcha type.
    fn supports(&self, captcha_type: &CaptchaType) -> bool;

    /// Name of this solver backend.
    fn name(&self) -> &str;
}

/// Trait for persistent cookie storage.
pub trait CookiePersistence: Send + Sync {
    /// Persist a cookie jar to storage.
    fn persist(&self, jar: &CookieJar) -> Result<(), EngineError>;

    /// Load cookies for a given domain from storage.
    fn load_for_domain(&self, domain: &str) -> Result<Vec<Cookie>, EngineError>;
}

/// Errors produced by engine operations.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Navigation failed: {0}")]
    Navigation(String),

    #[error("Element not found: ref={0}")]
    ElementNotFound(String),

    #[error("Timeout after {0}s")]
    Timeout(u64),

    #[error("Captcha detected but solver failed: {0}")]
    CaptchaFailed(String),

    #[error("Protection detected and could not be bypassed: {0}")]
    ProtectionBlocked(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Browser engine error: {0}")]
    BrowserError(String),

    #[error("Cookie operation failed: {0}")]
    CookieError(String),

    #[error("Screenshot failed: {0}")]
    ScreenshotFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}
