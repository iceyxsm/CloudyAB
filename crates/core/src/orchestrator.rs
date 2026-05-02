//! Session orchestrator — dispatches requests through the layer stack.
//!
//! The orchestrator owns the engine instances and routes requests based on
//! `LayerRouter` decisions. When the stealth-HTTP layer fails with a
//! protection/browser error and auto-escalation is enabled, it automatically
//! retries with the browser engine.

use std::sync::Arc;

use cloudyab_types::{
    CaptchaType, Cookie, CookieJar, Layer, NavigationResult, PageSnapshot, SessionConfig,
    SnapshotOptions,
};
use tracing::{info, warn};

use crate::config::CloudyAbConfig;
use crate::engine::{BrowsingEngine, CaptchaSolver, EngineError};
use crate::router::LayerRouter;

/// Maximum number of captcha solve attempts per navigation.
const MAX_CAPTCHA_RETRIES: u32 = 3;

/// The core orchestrator that manages engine lifecycle and request routing.
pub struct Orchestrator {
    router: LayerRouter,
    stealth_engine: Option<Arc<dyn BrowsingEngine>>,
    browser_engine: Option<Arc<dyn BrowsingEngine>>,
    solver: Option<Arc<dyn CaptchaSolver>>,
    config: CloudyAbConfig,
    /// Which engine is currently active (last used)
    active_layer: tokio::sync::RwLock<Option<Layer>>,
}

impl Orchestrator {
    /// Create a new orchestrator with the given configuration.
    ///
    /// Engines are registered separately via `set_stealth_engine` and
    /// `set_browser_engine` — this allows the MCP binary to wire them
    /// without core depending on Layer 2 crates.
    pub fn new(config: CloudyAbConfig) -> Self {
        let router = LayerRouter::new(config.engine.auto_escalate);
        Self {
            router,
            stealth_engine: None,
            browser_engine: None,
            solver: None,
            config,
            active_layer: tokio::sync::RwLock::new(None),
        }
    }

    /// Register the stealth-HTTP engine.
    pub fn set_stealth_engine(&mut self, engine: Arc<dyn BrowsingEngine>) {
        info!(engine = engine.name(), "Registered stealth-HTTP engine");
        self.stealth_engine = Some(engine);
    }

    /// Register the browser engine.
    pub fn set_browser_engine(&mut self, engine: Arc<dyn BrowsingEngine>) {
        info!(engine = engine.name(), "Registered browser engine");
        self.browser_engine = Some(engine);
    }

    /// Register the captcha solver.
    pub fn set_solver(&mut self, solver: Arc<dyn CaptchaSolver>) {
        info!(solver = solver.name(), "Registered captcha solver");
        self.solver = Some(solver);
    }

    /// Navigate to a URL, handling layer routing and auto-escalation.
    pub async fn navigate(&self, config: &SessionConfig) -> Result<NavigationResult, EngineError> {
        let target_layer = self.router.route(config);

        match target_layer {
            Layer::StealthHttp => self.navigate_with_escalation(config).await,
            Layer::Browser => self.navigate_browser(config).await,
        }
    }

    /// Get a snapshot from the currently active engine.
    pub async fn snapshot(&self, options: &SnapshotOptions) -> Result<PageSnapshot, EngineError> {
        let engine = self.active_engine().await?;
        engine.snapshot(options).await
    }

    /// Click an element on the current page.
    pub async fn click(&self, ref_id: &str) -> Result<(), EngineError> {
        let engine = self.active_engine().await?;
        match engine.click(ref_id).await {
            Ok(()) => Ok(()),
            Err(EngineError::BrowserError(_)) if self.can_escalate().await => {
                warn!("Click requires browser — escalating");
                self.escalate_to_browser_for_interaction().await?;
                let browser = self.get_browser_engine()?;
                browser.click(ref_id).await
            }
            Err(e) => Err(e),
        }
    }

    /// Fill a text input on the current page.
    pub async fn fill(&self, ref_id: &str, text: &str) -> Result<(), EngineError> {
        let engine = self.active_engine().await?;
        match engine.fill(ref_id, text).await {
            Ok(()) => Ok(()),
            Err(EngineError::BrowserError(_)) if self.can_escalate().await => {
                warn!("Fill requires browser — escalating");
                self.escalate_to_browser_for_interaction().await?;
                let browser = self.get_browser_engine()?;
                browser.fill(ref_id, text).await
            }
            Err(e) => Err(e),
        }
    }

    /// Type text with realistic timing on the current page.
    pub async fn type_text(&self, ref_id: &str, text: &str) -> Result<(), EngineError> {
        let engine = self.active_engine().await?;
        match engine.type_text(ref_id, text).await {
            Ok(()) => Ok(()),
            Err(EngineError::BrowserError(_)) if self.can_escalate().await => {
                warn!("Type requires browser — escalating");
                self.escalate_to_browser_for_interaction().await?;
                let browser = self.get_browser_engine()?;
                browser.type_text(ref_id, text).await
            }
            Err(e) => Err(e),
        }
    }

    /// Take a screenshot of the current page.
    pub async fn screenshot(&self) -> Result<Vec<u8>, EngineError> {
        let engine = self.active_engine().await?;
        match engine.screenshot().await {
            Ok(bytes) => Ok(bytes),
            Err(EngineError::ScreenshotFailed(_)) if self.can_escalate().await => {
                warn!("Screenshot requires browser — escalating");
                self.escalate_to_browser_for_interaction().await?;
                let browser = self.get_browser_engine()?;
                browser.screenshot().await
            }
            Err(e) => Err(e),
        }
    }

    /// Get cookies from the active engine.
    pub async fn get_cookies(&self) -> Result<CookieJar, EngineError> {
        let engine = self.active_engine().await?;
        engine.get_cookies().await
    }

    /// Set cookies on the active engine.
    pub async fn set_cookies(&self, cookies: &[Cookie]) -> Result<(), EngineError> {
        let engine = self.active_engine().await?;
        engine.set_cookies(cookies).await
    }

    /// Get the current URL from the active engine.
    pub async fn current_url(&self) -> Result<String, EngineError> {
        let engine = self.active_engine().await?;
        engine.current_url().await
    }

    // ─── Internal Methods ───────────────────────────────────────────────

    /// Navigate using stealth-HTTP first, escalate to browser on failure.
    async fn navigate_with_escalation(
        &self,
        config: &SessionConfig,
    ) -> Result<NavigationResult, EngineError> {
        let stealth = self.get_stealth_engine()?;

        match stealth.navigate(config).await {
            Ok(result) => {
                self.set_active_layer(Layer::StealthHttp).await;
                Ok(result)
            }
            Err(e) if self.should_escalate(&e) => {
                info!(
                    error = %e,
                    url = %config.target_url,
                    "Stealth-HTTP failed, escalating to browser"
                );
                self.navigate_browser(config).await
            }
            Err(e) => Err(e),
        }
    }

    /// Navigate directly with the browser engine.
    async fn navigate_browser(
        &self,
        config: &SessionConfig,
    ) -> Result<NavigationResult, EngineError> {
        let browser = self.get_browser_engine()?;
        let mut result = browser.navigate(config).await?;
        self.set_active_layer(Layer::Browser).await;

        // Attempt auto-captcha solving if solver is registered
        if let Some(ref solver) = self.solver {
            if let Some(captcha_type) = self.detect_captcha().await {
                info!(captcha = ?captcha_type, "Captcha detected, attempting auto-solve");
                match self.solve_captcha(solver, &captcha_type).await {
                    Ok(true) => {
                        result.captcha_solved = true;
                        info!("Captcha solved successfully");
                    }
                    Ok(false) => {
                        warn!("Captcha solve returned unsuccessful");
                    }
                    Err(e) => {
                        warn!(error = %e, "Captcha solve failed");
                    }
                }
            }
        }

        Ok(result)
    }

    /// Escalate to browser for interactive operations (click/fill/type/screenshot).
    /// Navigates the browser to the current URL so it has page context.
    async fn escalate_to_browser_for_interaction(&self) -> Result<(), EngineError> {
        let current_url = {
            let engine = self.active_engine().await?;
            engine.current_url().await?
        };

        let browser = self.get_browser_engine()?;
        let config = SessionConfig {
            target_url: current_url,
            fingerprint: None,
            proxy: self.config.proxy.clone(),
            persist_cookies: true,
            timeout_secs: self.config.engine.timeout_secs,
            preferred_layer: Some(Layer::Browser),
        };

        browser.navigate(&config).await?;
        self.set_active_layer(Layer::Browser).await;
        Ok(())
    }

    /// Determine if an error warrants escalation to the browser layer.
    fn should_escalate(&self, error: &EngineError) -> bool {
        if !self.router.should_escalate() {
            return false;
        }
        matches!(
            error,
            EngineError::ProtectionBlocked(_) | EngineError::BrowserError(_)
        )
    }

    /// Check if escalation is possible (browser engine registered + auto-escalate on).
    async fn can_escalate(&self) -> bool {
        self.router.should_escalate() && self.browser_engine.is_some()
    }

    /// Get the currently active engine.
    async fn active_engine(&self) -> Result<Arc<dyn BrowsingEngine>, EngineError> {
        let layer = self.active_layer.read().await;
        match *layer {
            Some(Layer::StealthHttp) => self.get_stealth_engine().map(Arc::clone),
            Some(Layer::Browser) => self.get_browser_engine().map(Arc::clone),
            None => Err(EngineError::Internal(
                "No active engine — call navigate() first".into(),
            )),
        }
    }

    fn get_stealth_engine(&self) -> Result<&Arc<dyn BrowsingEngine>, EngineError> {
        self.stealth_engine.as_ref().ok_or_else(|| {
            EngineError::Internal("Stealth-HTTP engine not registered".into())
        })
    }

    fn get_browser_engine(&self) -> Result<&Arc<dyn BrowsingEngine>, EngineError> {
        self.browser_engine.as_ref().ok_or_else(|| {
            EngineError::Internal("Browser engine not registered".into())
        })
    }

    async fn set_active_layer(&self, layer: Layer) {
        let mut active = self.active_layer.write().await;
        *active = Some(layer);
    }

    /// Detect if the current page contains a captcha by checking the snapshot.
    /// Returns the detected captcha type, or None if no captcha is present.
    async fn detect_captcha(&self) -> Option<CaptchaType> {
        let engine = self.active_engine().await.ok()?;
        let options = SnapshotOptions {
            interactive_only: false,
            compact: true,
            max_depth: 0,
            selector: None,
        };
        let snapshot = engine.snapshot(&options).await.ok()?;
        detect_captcha_in_snapshot(&snapshot.tree)
    }

    /// Attempt to solve a captcha: screenshot → solve → submit answer.
    /// Returns Ok(true) if solved successfully, Ok(false) if solver returned failure.
    async fn solve_captcha(
        &self,
        solver: &Arc<dyn CaptchaSolver>,
        captcha_type: &CaptchaType,
    ) -> Result<bool, EngineError> {
        let engine = self.active_engine().await?;
        let url = engine.current_url().await.unwrap_or_default();

        for attempt in 0..MAX_CAPTCHA_RETRIES {
            info!(attempt = attempt + 1, "Captcha solve attempt");
            let screenshot = engine.screenshot().await?;
            let result = solver.solve(&screenshot, captcha_type, &url).await?;

            if result.success {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

/// Detect captcha type from a page snapshot's accessibility tree text.
/// Looks for known captcha indicators in element names and roles.
fn detect_captcha_in_snapshot(tree: &str) -> Option<CaptchaType> {
    let lower = tree.to_lowercase();

    if lower.contains("cf-turnstile") || lower.contains("cloudflare") && lower.contains("challenge") {
        return Some(CaptchaType::CloudflareTurnstile);
    }
    if lower.contains("h-captcha") || lower.contains("hcaptcha") {
        return Some(CaptchaType::HCaptcha);
    }
    if lower.contains("recaptcha") || lower.contains("g-recaptcha") {
        return Some(CaptchaType::RecaptchaV2);
    }
    if lower.contains("aws-waf") || lower.contains("awswaf") {
        return Some(CaptchaType::AwsWafCaptcha);
    }
    if lower.contains("slider") && lower.contains("puzzle") {
        return Some(CaptchaType::SliderPuzzle);
    }
    if lower.contains("captcha") && lower.contains("type the") {
        return Some(CaptchaType::TextRecognition);
    }

    None
}
