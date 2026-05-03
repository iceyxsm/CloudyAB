//! Browser engine implementation via raw CDP WebSocket.
//!
//! Provides a high-level interface over a stealth-patched browser binary
//! (Obscura or compatible). Uses our tolerant CDP client that gracefully
//! handles Obscura's non-standard messages.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cloudyab_core::engine::{BrowsingEngine, DetectedChallenge, EngineError};
use cloudyab_types::captcha::CaptchaSolution;
use cloudyab_types::cookie::{Cookie, CookieJar, SameSite};
use cloudyab_types::fingerprint::FingerprintProfile;
use cloudyab_types::page::{
    ElementRef, PageSnapshot, SnapshotOptions, CONTENT_ROLES, INTERACTIVE_ROLES,
};
use cloudyab_types::session::{Layer, NavigationResult, SessionConfig};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::cdp::CdpClient;
use crate::challenge;
use crate::config::BrowserConfig;
use crate::stealth::build_stealth_script;

/// Poll interval when waiting for CDP server to become ready (ms).
const CDP_POLL_INTERVAL_MS: u64 = 200;

/// The full browser engine (Layer 2).
///
/// Uses a raw CDP WebSocket client connected to Obscura. Tolerates
/// non-standard messages that would crash chromiumoxide.
pub struct BrowserEngine {
    cdp: Arc<CdpClient>,
    current_target: RwLock<Option<String>>,
    stealth_script: String,
    config: BrowserConfig,
    _child: Arc<tokio::process::Child>,
}

impl BrowserEngine {
    /// Launch a new browser engine instance with the given configuration and fingerprint.
    /// Starts Obscura in serve mode and connects via raw CDP WebSocket.
    pub async fn launch(
        config: BrowserConfig,
        fingerprint: &FingerprintProfile,
    ) -> Result<Self, EngineError> {
        info!("Launching Obscura browser engine (raw CDP)");

        let binary_path = config.resolve_binary();
        let args = config.build_args();

        let mut cmd = tokio::process::Command::new(&binary_path);
        for arg in &args {
            cmd.arg(arg);
        }
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            EngineError::BrowserError(format!(
                "Failed to start Obscura at {}: {e}",
                binary_path.display()
            ))
        })?;

        let child_handle = Arc::new(child);

        let ws_url = config.cdp_ws_url();
        let ready = wait_for_cdp_ready(&ws_url, config.timeout_secs).await;
        if !ready {
            return Err(EngineError::BrowserError(format!(
                "Obscura CDP server did not become ready at {ws_url} within {}s",
                config.timeout_secs
            )));
        }

        info!(ws_url = %ws_url, "Connecting to Obscura via raw CDP WebSocket");

        let cdp = CdpClient::connect(&ws_url)
            .await
            .map_err(|e| EngineError::BrowserError(format!("CDP connect failed: {e}")))?;

        let stealth_script = build_stealth_script(fingerprint);

        Ok(Self {
            cdp: Arc::new(cdp),
            current_target: RwLock::new(None),
            stealth_script,
            config,
            _child: child_handle,
        })
    }

    /// Create a new target (page) and navigate to the URL.
    /// Injects stealth scripts before navigation completes.
    async fn create_page(&self, url: &str) -> Result<String, EngineError> {
        let result = self
            .cdp
            .call("Target.createTarget", json!({ "url": "about:blank" }))
            .await
            .map_err(EngineError::from)?;

        let target_id = result
            .get("targetId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EngineError::BrowserError("No targetId in response".into()))?
            .to_string();

        // Attach to the target to get a session
        let attach_result = self
            .cdp
            .call(
                "Target.attachToTarget",
                json!({ "targetId": &target_id, "flatten": true }),
            )
            .await
            .map_err(EngineError::from)?;

        let _session_id = attach_result
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Inject stealth script before navigation (non-fatal if unsupported)
        if !self.stealth_script.is_empty() {
            let inject_result = self
                .cdp
                .call(
                    "Page.addScriptToEvaluateOnNewDocument",
                    json!({ "source": &self.stealth_script }),
                )
                .await;

            if let Err(e) = inject_result {
                debug!("Stealth injection skipped (Obscura handles natively): {e}");
            }
        }

        // Enable page events
        let _ = self.cdp.call("Page.enable", json!({})).await;
        let _ = self.cdp.call("Network.enable", json!({})).await;

        // Navigate to the actual URL
        self.cdp
            .call("Page.navigate", json!({ "url": url }))
            .await
            .map_err(|e| EngineError::Navigation(format!("Navigate failed: {e}")))?;

        // Wait for load
        self.wait_for_load().await?;

        // Store as current target
        let mut guard = self.current_target.write().await;
        *guard = Some(target_id.clone());

        Ok(target_id)
    }

    /// Wait for the page to finish loading (poll document.readyState).
    async fn wait_for_load(&self) -> Result<(), EngineError> {
        let timeout = Duration::from_secs(self.config.timeout_secs);
        let start = tokio::time::Instant::now();
        let poll_interval = Duration::from_millis(CDP_POLL_INTERVAL_MS);

        loop {
            if start.elapsed() > timeout {
                return Err(EngineError::Timeout(self.config.timeout_secs));
            }

            let result = self
                .cdp
                .call(
                    "Runtime.evaluate",
                    json!({ "expression": "document.readyState" }),
                )
                .await;

            if let Ok(val) = result {
                let state = val
                    .get("result")
                    .and_then(|r| r.get("value"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if state == "complete" || state == "interactive" {
                    return Ok(());
                }
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Evaluate JavaScript on the current page and return the result.
    async fn evaluate(&self, expression: &str) -> Result<Value, EngineError> {
        let result = self
            .cdp
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
            )
            .await
            .map_err(|e| EngineError::BrowserError(format!("JS eval failed: {e}")))?;

        // Check for exception
        if let Some(exception) = result.get("exceptionDetails") {
            let text = exception
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown JS exception");
            return Err(EngineError::BrowserError(format!("JS exception: {text}")));
        }

        let value = result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null);

        Ok(value)
    }

    /// Get the current page URL via CDP.
    async fn get_url(&self) -> Result<String, EngineError> {
        let result = self
            .cdp
            .call(
                "Runtime.evaluate",
                json!({ "expression": "window.location.href", "returnByValue": true }),
            )
            .await
            .map_err(|e| EngineError::BrowserError(format!("Failed to get URL: {e}")))?;

        let url = result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(url)
    }

    /// Fallback screenshot: get page HTML via CDP and render to PNG using hyper-render.
    /// Used when Page.captureScreenshot is unavailable (Obscura has no visual renderer).
    async fn screenshot_via_html_render(&self) -> Result<Vec<u8>, EngineError> {
        // Get the document's outer HTML via DOM.getOuterHTML
        let doc_result = self
            .cdp
            .call("DOM.getDocument", json!({ "depth": 0 }))
            .await
            .map_err(|e| EngineError::ScreenshotFailed(format!("DOM.getDocument failed: {e}")))?;

        let node_id = doc_result
            .get("root")
            .and_then(|r| r.get("nodeId"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                EngineError::ScreenshotFailed("No root nodeId in DOM response".into())
            })?;

        let html_result = self
            .cdp
            .call("DOM.getOuterHTML", json!({ "nodeId": node_id }))
            .await
            .map_err(|e| EngineError::ScreenshotFailed(format!("DOM.getOuterHTML failed: {e}")))?;

        let html = html_result
            .get("outerHTML")
            .and_then(|v| v.as_str())
            .unwrap_or("<html><body>Empty page</body></html>");

        // Render HTML to PNG using hyper-render (pure Rust, no browser needed).
        // Offload to blocking thread since rendering is CPU-intensive.
        let html_owned = html.to_string();
        let width = self.config.viewport_width;
        let height = self.config.viewport_height;

        let png_bytes = tokio::task::spawn_blocking(move || {
            use hyper_render::{render_to_png, Config as RenderConfig};

            let render_config = RenderConfig::new().size(width, height);

            render_to_png(&html_owned, render_config)
                .map_err(|e| EngineError::ScreenshotFailed(format!("HTML render failed: {e}")))
        })
        .await
        .map_err(|e| EngineError::ScreenshotFailed(format!("Render task panicked: {e}")))?;

        png_bytes
    }
}

#[async_trait]
impl BrowsingEngine for BrowserEngine {
    async fn navigate(&self, config: &SessionConfig) -> Result<NavigationResult, EngineError> {
        info!(url = %config.target_url, "Browser navigating (raw CDP)");

        self.create_page(&config.target_url).await?;

        let final_url = self.get_url().await.unwrap_or(config.target_url.clone());

        Ok(NavigationResult {
            final_url,
            status_code: 200,
            layer_used: Layer::Browser,
            captcha_solved: false,
            protection_bypassed: true,
        })
    }

    async fn snapshot(&self, options: &SnapshotOptions) -> Result<PageSnapshot, EngineError> {
        let title: String = self
            .evaluate("document.title")
            .await?
            .as_str()
            .unwrap_or("")
            .to_string();

        let url = self.get_url().await?;

        let scope_selector = options.selector.as_deref().unwrap_or("document.body");
        let js = build_snapshot_js(scope_selector, options);

        let raw = self.evaluate(&js).await?;
        parse_snapshot_result(&url, &title, &raw)
    }

    async fn click(&self, ref_id: &str) -> Result<(), EngineError> {
        info!(ref_id, "Clicking element");

        let js = format!(
            r#"(() => {{
                const el = document.querySelector('[data-cloudyab-ref="{ref_id}"]');
                if (!el) return false;
                el.click();
                return true;
            }})()"#
        );

        let result = self.evaluate(&js).await?;
        if result.as_bool() != Some(true) {
            return Err(EngineError::ElementNotFound(ref_id.to_string()));
        }
        Ok(())
    }

    async fn fill(&self, ref_id: &str, text: &str) -> Result<(), EngineError> {
        info!(ref_id, text_len = text.len(), "Filling element");

        let escaped = text.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!(
            r#"(() => {{
                const el = document.querySelector('[data-cloudyab-ref="{ref_id}"]');
                if (!el) return false;
                el.focus();
                el.value = '';
                el.value = '{escaped}';
                el.dispatchEvent(new Event('input', {{bubbles: true}}));
                el.dispatchEvent(new Event('change', {{bubbles: true}}));
                return true;
            }})()"#
        );

        let result = self.evaluate(&js).await?;
        if result.as_bool() != Some(true) {
            return Err(EngineError::ElementNotFound(ref_id.to_string()));
        }
        Ok(())
    }

    async fn type_text(&self, ref_id: &str, text: &str) -> Result<(), EngineError> {
        info!(ref_id, text_len = text.len(), "Typing into element");

        let escaped = text.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!(
            r#"(() => {{
                const el = document.querySelector('[data-cloudyab-ref="{ref_id}"]');
                if (!el) return false;
                el.focus();
                for (const ch of '{escaped}') {{
                    el.dispatchEvent(new KeyboardEvent('keydown', {{key: ch, bubbles: true}}));
                    el.value += ch;
                    el.dispatchEvent(new Event('input', {{bubbles: true}}));
                    el.dispatchEvent(new KeyboardEvent('keyup', {{key: ch, bubbles: true}}));
                }}
                return true;
            }})()"#
        );

        let result = self.evaluate(&js).await?;
        if result.as_bool() != Some(true) {
            return Err(EngineError::ElementNotFound(ref_id.to_string()));
        }
        Ok(())
    }

    async fn screenshot(&self) -> Result<Vec<u8>, EngineError> {
        // Try CDP Page.captureScreenshot first (works with Chromium-based backends).
        // Obscura is a DOM-only engine without a visual renderer, so this will
        // return a protocol error — fall back to server-side HTML rendering.
        let cdp_result = self
            .cdp
            .call("Page.captureScreenshot", json!({ "format": "png" }))
            .await;

        match cdp_result {
            Ok(result) => {
                if let Some(data_b64) = result.get("data").and_then(|v| v.as_str()) {
                    use base64::Engine as _;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data_b64)
                        .map_err(|e| {
                            EngineError::ScreenshotFailed(format!("Base64 decode: {e}"))
                        })?;
                    return Ok(bytes);
                }
                // No data field — fall through to HTML rendering
                debug!("Page.captureScreenshot returned no data, falling back to HTML render");
            }
            Err(e) => {
                debug!("Page.captureScreenshot unsupported ({e}), falling back to HTML render");
            }
        }

        // Fallback: get the page HTML via DOM.getOuterHTML and render with hyper-render.
        self.screenshot_via_html_render().await
    }

    async fn get_cookies(&self) -> Result<CookieJar, EngineError> {
        let url = self.get_url().await?;

        let result = self
            .cdp
            .call("Network.getCookies", json!({}))
            .await
            .map_err(|e| EngineError::CookieError(format!("Failed to get cookies: {e}")))?;

        let cookies_arr = result
            .get("cookies")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let cookies: Vec<Cookie> = cookies_arr
            .iter()
            .map(|c| Cookie {
                name: c
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                value: c
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                domain: c
                    .get("domain")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                path: c
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("/")
                    .to_string(),
                expires: None,
                secure: c.get("secure").and_then(|v| v.as_bool()).unwrap_or(false),
                http_only: c.get("httpOnly").and_then(|v| v.as_bool()).unwrap_or(false),
                same_site: match c.get("sameSite").and_then(|v| v.as_str()) {
                    Some("Strict") => SameSite::Strict,
                    Some("Lax") => SameSite::Lax,
                    _ => SameSite::None,
                },
            })
            .collect();

        Ok(CookieJar {
            source_url: url,
            user_agent: String::new(),
            captured_at: chrono::Utc::now(),
            cookies,
        })
    }

    async fn set_cookies(&self, cookies: &[Cookie]) -> Result<(), EngineError> {
        for cookie in cookies {
            self.cdp
                .call(
                    "Network.setCookie",
                    json!({
                        "name": cookie.name,
                        "value": cookie.value,
                        "domain": cookie.domain,
                        "path": cookie.path,
                        "secure": cookie.secure,
                        "httpOnly": cookie.http_only,
                    }),
                )
                .await
                .map_err(|e| {
                    EngineError::CookieError(format!("Failed to set cookie '{}': {e}", cookie.name))
                })?;
        }
        Ok(())
    }

    async fn current_url(&self) -> Result<String, EngineError> {
        self.get_url().await
    }

    async fn can_handle(&self, _url: &str) -> bool {
        true
    }

    async fn detect_challenges(&self) -> Vec<DetectedChallenge> {
        let raw = match self.evaluate(challenge::detection_script()).await {
            Ok(v) => v,
            Err(e) => {
                debug!("Challenge detection JS failed: {e}");
                return Vec::new();
            }
        };

        challenge::parse_detection_results(&raw)
            .into_iter()
            .map(|d| DetectedChallenge {
                captcha_type: d.captcha_type,
                confidence: d.confidence,
                container_selector: d.container_selector,
                is_interstitial: d.is_interstitial,
            })
            .collect()
    }

    async fn submit_solution(
        &self,
        solution: &CaptchaSolution,
        container_selector: Option<&str>,
    ) -> Result<(), EngineError> {
        let js = build_submit_solution_js(solution, container_selector);
        self.evaluate(&js).await?;
        Ok(())
    }

    async fn evaluate_js(&self, js: &str) -> Result<Value, EngineError> {
        self.evaluate(js).await
    }

    fn name(&self) -> &str {
        "obscura-browser"
    }
}

/// Wait for the Obscura CDP WebSocket server to become ready.
async fn wait_for_cdp_ready(ws_url: &str, timeout_secs: u64) -> bool {
    use std::net::TcpStream;

    let addr = ws_url
        .trim_start_matches("ws://")
        .split('/')
        .next()
        .unwrap_or("127.0.0.1:9223");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    while tokio::time::Instant::now() < deadline {
        if TcpStream::connect(addr).is_ok() {
            tokio::time::sleep(Duration::from_millis(CDP_POLL_INTERVAL_MS)).await;
            return true;
        }
        tokio::time::sleep(Duration::from_millis(CDP_POLL_INTERVAL_MS)).await;
    }

    false
}

/// Build the JavaScript that extracts the accessibility tree from the DOM.
fn build_snapshot_js(scope_selector: &str, options: &SnapshotOptions) -> String {
    let interactive_only = options.interactive_only;
    let compact = options.compact;
    let max_depth = options.max_depth;

    format!(
        r#"(() => {{
const INTERACTIVE_ROLES = {interactive_roles};
const CONTENT_ROLES = {content_roles};
const interactiveOnly = {interactive_only};
const compact = {compact};
const maxDepth = {max_depth};
let refCounter = 0;
const refs = {{}};
const lines = [];

function getRole(el) {{
    if (el.getAttribute('role')) return el.getAttribute('role');
    const tag = el.tagName.toLowerCase();
    const roleMap = {{
        'a': 'link', 'button': 'button', 'input': getInputRole(el),
        'select': 'combobox', 'textarea': 'textbox', 'h1': 'heading',
        'h2': 'heading', 'h3': 'heading', 'h4': 'heading', 'h5': 'heading',
        'h6': 'heading', 'nav': 'navigation', 'main': 'main',
        'article': 'article', 'li': 'listitem', 'img': 'img'
    }};
    return roleMap[tag] || '';
}}

function getInputRole(el) {{
    const type = (el.getAttribute('type') || 'text').toLowerCase();
    const map = {{
        'text': 'textbox', 'email': 'textbox', 'password': 'textbox',
        'search': 'searchbox', 'checkbox': 'checkbox', 'radio': 'radio',
        'range': 'slider', 'number': 'spinbutton', 'submit': 'button',
        'button': 'button', 'reset': 'button'
    }};
    return map[type] || 'textbox';
}}

function getName(el) {{
    return el.getAttribute('aria-label')
        || el.getAttribute('alt')
        || el.getAttribute('title')
        || el.getAttribute('placeholder')
        || el.innerText?.trim().substring(0, 80)
        || '';
}}

function walk(el, depth) {{
    if (maxDepth > 0 && depth > maxDepth) return;
    if (!el || el.nodeType !== 1) return;
    if (el.getAttribute('aria-hidden') === 'true') return;
    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden') return;

    const role = getRole(el);
    const name = getName(el);
    const isInteractive = INTERACTIVE_ROLES.includes(role);
    const isContent = CONTENT_ROLES.includes(role) && name.length > 0;

    if (interactiveOnly && !isInteractive) {{
        for (const child of el.children) walk(child, depth);
        return;
    }}

    let refId = null;
    if (isInteractive || isContent) {{
        refCounter++;
        refId = 'e' + refCounter;
        el.setAttribute('data-cloudyab-ref', refId);
        const selector = '[data-cloudyab-ref="' + refId + '"]';
        refs[refId] = {{selector, role, name}};
    }}

    if (role || name) {{
        const indent = '  '.repeat(depth);
        const refStr = refId ? ' [ref=' + refId + ']' : '';
        const nameStr = name ? ' "' + name.replace(/"/g, '\\"') + '"' : '';
        const line = indent + '- ' + role + nameStr + refStr;
        if (!compact || name || refId) lines.push(line);
    }}

    for (const child of el.children) walk(child, depth + 1);
}}

const root = '{scope_selector}' === 'document.body' ? document.body : document.querySelector('{scope_selector}');
if (root) walk(root, 0);
return {{refs, tree: lines.join('\\n')}};
}})()"#,
        interactive_roles =
            serde_json::to_string(INTERACTIVE_ROLES).unwrap_or_else(|_| "[]".into()),
        content_roles = serde_json::to_string(CONTENT_ROLES).unwrap_or_else(|_| "[]".into()),
        interactive_only = interactive_only,
        compact = compact,
        max_depth = max_depth,
        scope_selector = scope_selector,
    )
}

/// Parse the raw JSON result from the snapshot JavaScript into a PageSnapshot.
fn parse_snapshot_result(url: &str, title: &str, raw: &Value) -> Result<PageSnapshot, EngineError> {
    let tree = raw
        .get("tree")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let refs_raw = raw.get("refs").and_then(|v| v.as_object());

    let mut refs = HashMap::new();
    if let Some(obj) = refs_raw {
        for (key, val) in obj {
            let selector = val
                .get("selector")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let role = val
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = val
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            refs.insert(
                key.clone(),
                ElementRef {
                    selector,
                    role,
                    name,
                    nth: None,
                    attributes: HashMap::new(),
                },
            );
        }
    }

    Ok(PageSnapshot {
        url: url.to_string(),
        title: title.to_string(),
        tree,
        refs,
    })
}

/// Build JavaScript to submit a captcha solution to the page.
fn build_submit_solution_js(solution: &CaptchaSolution, container: Option<&str>) -> String {
    let container_js = container
        .map(|s| format!("'{s}'"))
        .unwrap_or_else(|| "null".to_string());

    match solution {
        CaptchaSolution::Token(token) => build_token_submit_js(&container_js, token),
        CaptchaSolution::Text(text) => build_text_submit_js(&container_js, text),
        CaptchaSolution::SliderOffset(offset) => build_slider_submit_js(&container_js, *offset),
        CaptchaSolution::Coordinates(coords) => build_coords_submit_js(&container_js, coords),
    }
}

/// Build JS for token-based solution submission (Turnstile, reCAPTCHA, hCaptcha).
fn build_token_submit_js(container_js: &str, token: &str) -> String {
    let escaped_token = token.replace('\\', "\\\\").replace('\'', "\\'");
    format!(
        r#"(() => {{
    const container = {container_js} ? document.querySelector({container_js}) : document;
    const turnstile = container.querySelector('[data-callback]');
    if (turnstile) {{
        const cb = turnstile.getAttribute('data-callback');
        if (window[cb]) {{ window[cb]('{escaped_token}'); return true; }}
    }}
    const textarea = container.querySelector('textarea[name*="response"], #g-recaptcha-response, #h-captcha-response');
    if (textarea) {{
        textarea.value = '{escaped_token}';
        textarea.dispatchEvent(new Event('input', {{bubbles: true}}));
        const form = textarea.closest('form');
        if (form) form.submit();
        return true;
    }}
    if (window.___grecaptcha_cfg) {{
        const clients = window.___grecaptcha_cfg.clients;
        for (const key in clients) {{
            const client = clients[key];
            if (client && client.callback) {{ client.callback('{escaped_token}'); return true; }}
        }}
    }}
    return false;
}})()"#
    )
}

/// Build JS for text-based solution submission (OCR captchas).
fn build_text_submit_js(container_js: &str, text: &str) -> String {
    let escaped_text = text.replace('\\', "\\\\").replace('\'', "\\'");
    format!(
        r#"(() => {{
    const container = {container_js} ? document.querySelector({container_js}) : document;
    const input = container.querySelector('input[name*="captcha" i], input[placeholder*="code" i], input[placeholder*="captcha" i], input[type="text"]:not([name=""])');
    if (!input) return false;
    input.focus();
    input.value = '{escaped_text}';
    input.dispatchEvent(new Event('input', {{bubbles: true}}));
    input.dispatchEvent(new Event('change', {{bubbles: true}}));
    const form = input.closest('form');
    const btn = form ? form.querySelector('button[type="submit"], input[type="submit"], button:not([type])') : null;
    if (btn) btn.click();
    else if (form) form.submit();
    return true;
}})()"#
    )
}

/// Build JS for slider-based solution submission (drag puzzles).
fn build_slider_submit_js(container_js: &str, offset: i32) -> String {
    format!(
        r#"(() => {{
    const container = {container_js} ? document.querySelector({container_js}) : document;
    const slider = container.querySelector('.slider-handle, .slide-btn, [data-slider-handle], .handler');
    if (!slider) return false;
    const rect = slider.getBoundingClientRect();
    const startX = rect.left + rect.width / 2;
    const startY = rect.top + rect.height / 2;
    const endX = startX + {offset};
    slider.dispatchEvent(new MouseEvent('mousedown', {{clientX: startX, clientY: startY, bubbles: true}}));
    document.dispatchEvent(new MouseEvent('mousemove', {{clientX: endX, clientY: startY, bubbles: true}}));
    document.dispatchEvent(new MouseEvent('mouseup', {{clientX: endX, clientY: startY, bubbles: true}}));
    return true;
}})()"#
    )
}

/// Build JS for coordinate-based solution submission (image grid selection).
fn build_coords_submit_js(container_js: &str, coords: &[(i32, i32)]) -> String {
    let coords_json = serde_json::to_string(coords).unwrap_or_else(|_| "[]".into());
    format!(
        r#"(() => {{
    const container = {container_js} ? document.querySelector({container_js}) : document;
    const target = container.querySelector('img, canvas, .captcha-image, [data-captcha-image]');
    if (!target) return false;
    const rect = target.getBoundingClientRect();
    const coords = {coords_json};
    for (const [x, y] of coords) {{
        const clientX = rect.left + x;
        const clientY = rect.top + y;
        target.dispatchEvent(new MouseEvent('click', {{clientX, clientY, bubbles: true}}));
    }}
    const btn = container.querySelector('button[type="submit"], .verify-btn, [data-action="verify"]');
    if (btn) btn.click();
    return true;
}})()"#
    )
}
