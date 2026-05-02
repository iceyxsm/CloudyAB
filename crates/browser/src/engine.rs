//! Browser engine implementation via CDP (Chrome DevTools Protocol).
//!
//! Provides a high-level interface over a stealth-patched browser binary
//! (Obscura or compatible). Handles page lifecycle, stealth injection,
//! DOM interaction, and accessibility tree extraction.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig as CdpBrowserConfig};
use chromiumoxide::page::Page;
use cloudyab_core::engine::{BrowsingEngine, EngineError};
use cloudyab_types::cookie::{Cookie, CookieJar, SameSite};
use cloudyab_types::fingerprint::FingerprintProfile;
use cloudyab_types::page::{ElementRef, PageSnapshot, SnapshotOptions, CONTENT_ROLES, INTERACTIVE_ROLES};
use cloudyab_types::session::{Layer, NavigationResult, SessionConfig};
use futures::StreamExt;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::config::BrowserConfig;
use crate::stealth::build_stealth_script;

/// The full browser engine (Layer 2).
///
/// Uses a CDP-compatible browser binary for full page rendering with
/// JavaScript execution, stealth features, and DOM access.
pub struct BrowserEngine {
    browser: Browser,
    page: RwLock<Option<Arc<Page>>>,
    stealth_script: String,
    config: BrowserConfig,
}

impl BrowserEngine {
    /// Launch a new browser engine instance with the given configuration and fingerprint.
    pub async fn launch(
        config: BrowserConfig,
        fingerprint: &FingerprintProfile,
    ) -> Result<Self, EngineError> {
        info!("Launching browser engine");

        let binary_path = config.resolve_binary();
        let args = config.build_args();

        let mut builder = CdpBrowserConfig::builder()
            .chrome_executable(binary_path)
            .viewport(None);

        if config.headless {
            builder = builder.arg("--headless=new");
        }

        for arg in &args {
            builder = builder.arg(arg);
        }

        let browser_config = builder.build().map_err(|e| {
            EngineError::BrowserError(format!("Failed to build browser config: {e}"))
        })?;

        let (browser, mut handler) =
            Browser::launch(browser_config).await.map_err(|e| {
                EngineError::BrowserError(format!("Failed to launch browser: {e}"))
            })?;

        // Spawn the CDP event handler in the background
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                debug!(?event, "CDP event");
            }
        });

        let stealth_script = build_stealth_script(fingerprint);

        Ok(Self {
            browser,
            page: RwLock::new(None),
            stealth_script,
            config,
        })
    }

    /// Get the active page, returning an error if none exists.
    async fn active_page(&self) -> Result<Arc<Page>, EngineError> {
        let guard = self.page.read().await;
        guard
            .clone()
            .ok_or_else(|| EngineError::Internal("No active page — call navigate() first".into()))
    }

    /// Create a new page with stealth scripts injected before any content loads.
    async fn create_stealth_page(&self, url: &str) -> Result<Arc<Page>, EngineError> {
        let page = self.browser.new_page(url).await.map_err(|e| {
            EngineError::Navigation(format!("Failed to create page: {e}"))
        })?;

        // Inject stealth scripts via CDP's Page.addScriptToEvaluateOnNewDocument
        page.execute(
            chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams::new(
                &self.stealth_script,
            ),
        )
        .await
        .map_err(|e| {
            EngineError::BrowserError(format!("Failed to inject stealth script: {e}"))
        })?;

        Ok(Arc::new(page))
    }

    /// Wait for the page to reach a stable state (network idle + DOM loaded).
    async fn wait_for_stable(&self, page: &Page) -> Result<(), EngineError> {
        let timeout = Duration::from_secs(self.config.timeout_secs);
        tokio::time::timeout(timeout, page.wait_for_navigation())
            .await
            .map_err(|_| EngineError::Timeout(self.config.timeout_secs))?
            .map_err(|e| EngineError::Navigation(format!("Navigation wait failed: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl BrowsingEngine for BrowserEngine {
    async fn navigate(&self, config: &SessionConfig) -> Result<NavigationResult, EngineError> {
        info!(url = %config.target_url, "Browser navigating");

        let page = self.create_stealth_page(&config.target_url).await?;
        self.wait_for_stable(&page).await?;

        let final_url = page.url().await.map_err(|e| {
            EngineError::Navigation(format!("Failed to get URL: {e}"))
        })?.unwrap_or_else(|| config.target_url.clone());

        // Store as active page
        let mut guard = self.page.write().await;
        *guard = Some(page);

        Ok(NavigationResult {
            final_url,
            status_code: 200,
            layer_used: Layer::Browser,
            captcha_solved: false,
            protection_bypassed: true,
        })
    }

    async fn snapshot(&self, options: &SnapshotOptions) -> Result<PageSnapshot, EngineError> {
        let page = self.active_page().await?;

        let title = page
            .evaluate("document.title")
            .await
            .map_err(|e| EngineError::BrowserError(format!("Failed to get title: {e}")))?
            .into_value::<String>()
            .unwrap_or_default();

        let url = page.url().await.map_err(|e| {
            EngineError::BrowserError(format!("Failed to get URL: {e}"))
        })?.unwrap_or_default();

        // Extract accessibility tree via JavaScript
        let scope_selector = options
            .selector
            .as_deref()
            .unwrap_or("document.body");

        let js = build_snapshot_js(scope_selector, options);
        let raw: serde_json::Value = page
            .evaluate(js)
            .await
            .map_err(|e| EngineError::BrowserError(format!("Snapshot JS failed: {e}")))?
            .into_value()
            .map_err(|e| EngineError::BrowserError(format!("Snapshot parse failed: {e}")))?;

        parse_snapshot_result(&url, &title, &raw)
    }

    async fn click(&self, ref_id: &str) -> Result<(), EngineError> {
        let page = self.active_page().await?;
        info!(ref_id, "Clicking element");

        let selector = resolve_ref_selector(&page, ref_id).await?;
        page.find_element(&selector)
            .await
            .map_err(|e| EngineError::ElementNotFound(format!("{ref_id}: {e}")))?
            .click()
            .await
            .map_err(|e| EngineError::BrowserError(format!("Click failed: {e}")))?;

        Ok(())
    }

    async fn fill(&self, ref_id: &str, text: &str) -> Result<(), EngineError> {
        let page = self.active_page().await?;
        info!(ref_id, text_len = text.len(), "Filling element");

        let selector = resolve_ref_selector(&page, ref_id).await?;
        let element = page
            .find_element(&selector)
            .await
            .map_err(|e| EngineError::ElementNotFound(format!("{ref_id}: {e}")))?;

        // Clear existing value then type
        element.click().await.map_err(|e| {
            EngineError::BrowserError(format!("Focus failed: {e}"))
        })?;

        page.evaluate(format!(
            "document.querySelector('{selector}').value = ''"
        ))
        .await
        .map_err(|e| EngineError::BrowserError(format!("Clear failed: {e}")))?;

        element.type_str(text).await.map_err(|e| {
            EngineError::BrowserError(format!("Type failed: {e}"))
        })?;

        Ok(())
    }

    async fn type_text(&self, ref_id: &str, text: &str) -> Result<(), EngineError> {
        let page = self.active_page().await?;
        info!(ref_id, text_len = text.len(), "Typing into element");

        let selector = resolve_ref_selector(&page, ref_id).await?;
        let element = page
            .find_element(&selector)
            .await
            .map_err(|e| EngineError::ElementNotFound(format!("{ref_id}: {e}")))?;

        element.click().await.map_err(|e| {
            EngineError::BrowserError(format!("Focus failed: {e}"))
        })?;

        element.type_str(text).await.map_err(|e| {
            EngineError::BrowserError(format!("Type failed: {e}"))
        })?;

        Ok(())
    }

    async fn screenshot(&self) -> Result<Vec<u8>, EngineError> {
        let page = self.active_page().await?;
        let bytes = page.screenshot(
            chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams::builder()
                .format(chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png)
                .build(),
        )
        .await
        .map_err(|e| EngineError::ScreenshotFailed(format!("{e}")))?;

        Ok(bytes)
    }

    async fn get_cookies(&self) -> Result<CookieJar, EngineError> {
        let page = self.active_page().await?;

        let url = page.url().await.map_err(|e| {
            EngineError::CookieError(format!("Failed to get URL: {e}"))
        })?.unwrap_or_default();

        let cdp_cookies = page
            .execute(chromiumoxide::cdp::browser_protocol::network::GetCookiesParams::default())
            .await
            .map_err(|e| EngineError::CookieError(format!("Failed to get cookies: {e}")))?;

        let cookies: Vec<Cookie> = cdp_cookies
            .result
            .cookies
            .iter()
            .map(|c| Cookie {
                name: c.name.clone(),
                value: c.value.clone(),
                domain: c.domain.clone(),
                path: c.path.clone(),
                expires: None,
                secure: c.secure,
                http_only: c.http_only,
                same_site: match c.same_site.as_ref() {
                    Some(s) => match s.as_ref() {
                        "Strict" => SameSite::Strict,
                        "Lax" => SameSite::Lax,
                        _ => SameSite::None,
                    },
                    None => SameSite::None,
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
        let page = self.active_page().await?;

        for cookie in cookies {
            let params = chromiumoxide::cdp::browser_protocol::network::SetCookieParams::builder()
                .name(&cookie.name)
                .value(&cookie.value)
                .domain(&cookie.domain)
                .path(&cookie.path)
                .secure(cookie.secure)
                .http_only(cookie.http_only)
                .build()
                .map_err(|e| {
                    EngineError::CookieError(format!("Invalid cookie params for '{}': {e}", cookie.name))
                })?;

            page.execute(params).await.map_err(|e| {
                EngineError::CookieError(format!("Failed to set cookie '{}': {e}", cookie.name))
            })?;
        }

        Ok(())
    }

    async fn current_url(&self) -> Result<String, EngineError> {
        let page = self.active_page().await?;
        page.url()
            .await
            .map_err(|e| EngineError::BrowserError(format!("Failed to get URL: {e}")))?
            .ok_or_else(|| EngineError::BrowserError("No URL available".into()))
    }

    async fn can_handle(&self, _url: &str) -> bool {
        true
    }

    fn name(&self) -> &str {
        "obscura-browser"
    }
}

/// Resolve a @eN ref ID to a CSS selector by querying the page's stored ref map.
async fn resolve_ref_selector(page: &Page, ref_id: &str) -> Result<String, EngineError> {
    let js = format!(
        r#"(() => {{
            const el = document.querySelector('[data-cloudyab-ref="{ref_id}"]');
            if (!el) return null;
            return '[data-cloudyab-ref="{ref_id}"]';
        }})()"#,
    );

    let result: Option<String> = page
        .evaluate(js)
        .await
        .map_err(|e| EngineError::BrowserError(format!("Ref resolution failed: {e}")))?
        .into_value()
        .unwrap_or(None);

    result.ok_or_else(|| EngineError::ElementNotFound(ref_id.to_string()))
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
        interactive_roles = serde_json::to_string(INTERACTIVE_ROLES).unwrap_or_else(|_| "[]".into()),
        content_roles = serde_json::to_string(CONTENT_ROLES).unwrap_or_else(|_| "[]".into()),
        interactive_only = interactive_only,
        compact = compact,
        max_depth = max_depth,
        scope_selector = scope_selector,
    )
}

/// Parse the raw JSON result from the snapshot JavaScript into a PageSnapshot.
fn parse_snapshot_result(
    url: &str,
    title: &str,
    raw: &serde_json::Value,
) -> Result<PageSnapshot, EngineError> {
    let tree = raw
        .get("tree")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let refs_raw = raw.get("refs").and_then(|v| v.as_object());

    let mut refs = HashMap::new();
    if let Some(obj) = refs_raw {
        for (key, val) in obj {
            let selector = val.get("selector").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let role = val.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();

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
