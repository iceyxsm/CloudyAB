//! StealthEngine — implements BrowsingEngine for the HTTP-only stealth layer.
//!
//! Flow: navigate → detect CF/WAF challenge → solve JS challenge → resubmit → return.
//! If the challenge requires a full browser (Turnstile, hCaptcha), signals escalation.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use cloudyab_core::engine::{BrowsingEngine, EngineError};
use cloudyab_types::fingerprint::FingerprintProfile;
use cloudyab_types::page::ElementRef;
use cloudyab_types::session::SessionConfig;
use cloudyab_types::{Cookie, CookieJar, Layer, NavigationResult, PageSnapshot, SnapshotOptions};
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::challenge::ChallengeSolver;
use crate::client::{StealthClient, StealthHttpError, StealthResponse};

/// Maximum number of challenge-solve retries before giving up.
const MAX_CHALLENGE_RETRIES: u32 = 3;

/// Stealth HTTP engine that handles navigation without a browser.
///
/// Capabilities:
/// - GET requests with browser-like TLS fingerprint and headers
/// - Cloudflare JS challenge solving (classic challenges)
/// - Cookie extraction
/// - Basic HTML → accessibility tree parsing
///
/// Limitations (will signal escalation needed):
/// - No JavaScript rendering beyond challenge solving
/// - No interactive elements (click, fill, type)
/// - No screenshots
/// - Cannot solve Turnstile/hCaptcha/reCAPTCHA (needs browser)
pub struct StealthEngine {
    client: StealthClient,
    fingerprint: FingerprintProfile,
    /// Current page state after last navigation
    current_page: tokio::sync::RwLock<Option<PageState>>,
    /// URLs that have failed and need browser escalation
    failed_urls: tokio::sync::RwLock<Vec<String>>,
}

/// Internal state of the last navigated page.
struct PageState {
    url: String,
    body: String,
    headers: reqwest::header::HeaderMap,
}

impl StealthEngine {
    /// Create a new stealth engine with the given fingerprint.
    pub fn new(fingerprint: FingerprintProfile) -> Result<Self, EngineError> {
        let client = StealthClient::new(fingerprint.clone())
            .map_err(|e| EngineError::Network(e.to_string()))?;

        Ok(Self {
            client,
            fingerprint,
            current_page: tokio::sync::RwLock::new(None),
            failed_urls: tokio::sync::RwLock::new(Vec::new()),
        })
    }

    /// Execute the full navigation flow with challenge solving.
    async fn navigate_with_challenges(
        &self,
        url: &str,
        timeout_secs: u64,
    ) -> Result<(StealthResponse, bool), EngineError> {
        let deadline = Duration::from_secs(timeout_secs);

        let result = timeout(deadline, async {
            match self.client.get(url).await {
                Ok(response) => Ok((response, false)),
                Err(StealthHttpError::CloudflareChallenge) => {
                    info!(url, "Cloudflare challenge detected, attempting JS solve");
                    self.solve_challenge_flow(url).await
                }
                Err(StealthHttpError::AwsWafChallenge) => {
                    warn!(url, "AWS WAF challenge detected, needs browser escalation");
                    let mut failed = self.failed_urls.write().await;
                    failed.push(url.to_string());
                    Err(EngineError::ProtectionBlocked(
                        "AWS WAF challenge requires browser engine".into(),
                    ))
                }
                Err(e) => Err(EngineError::Network(e.to_string())),
            }
        })
        .await
        .map_err(|_| EngineError::Timeout(timeout_secs))?;

        result
    }

    /// Attempt to solve a Cloudflare JS challenge and resubmit.
    async fn solve_challenge_flow(
        &self,
        url: &str,
    ) -> Result<(StealthResponse, bool), EngineError> {
        let domain = extract_domain(url);

        for attempt in 1..=MAX_CHALLENGE_RETRIES {
            debug!(attempt, domain = %domain, "Challenge solve attempt");

            // Fetch the challenge page again to get fresh challenge data
            let challenge_page = self.client.get_raw(url).await.map_err(|e| {
                EngineError::Network(format!("Failed to fetch challenge page: {e}"))
            })?;

            // If the page loads fine now (CF cleared after delay), return it
            if challenge_page.status == 200 {
                info!("Challenge cleared on retry — CF accepted previous session");
                return Ok((challenge_page, true));
            }

            // Solve the JS challenge
            let answer = ChallengeSolver::solve_cf_challenge(&challenge_page.body, &domain)
                .map_err(|e| {
                    EngineError::ProtectionBlocked(format!("Challenge parse/solve failed: {e}"))
                })?;

            // CF requires a delay before submitting (typically 4-5 seconds)
            debug!("Waiting 4s before challenge submission (CF timing requirement)");
            tokio::time::sleep(Duration::from_secs(4)).await;

            // Submit the challenge answer
            let submit_response = self
                .client
                .post_challenge_answer(url, &answer.submit_url, &answer.form_params, &answer.answer)
                .await
                .map_err(|e| EngineError::Network(format!("Challenge submission failed: {e}")))?;

            if submit_response.status == 200 {
                info!("Cloudflare challenge solved successfully");
                return Ok((submit_response, true));
            }

            // Handle redirects (CF often 302s to the real page after solving)
            if submit_response.status == 301 || submit_response.status == 302 {
                if let Some(location) = submit_response.headers.get("location") {
                    let redirect_url = location.to_str().unwrap_or(url);
                    let final_response = self.client.get(redirect_url).await.map_err(|e| {
                        EngineError::Network(format!("Post-challenge redirect failed: {e}"))
                    })?;
                    info!("Cloudflare challenge solved (followed redirect)");
                    return Ok((final_response, true));
                }
            }

            warn!(
                attempt,
                status = submit_response.status,
                "Challenge submission returned non-success, retrying"
            );
        }

        // All retries exhausted — mark URL as needing browser
        let mut failed = self.failed_urls.write().await;
        failed.push(url.to_string());

        Err(EngineError::ProtectionBlocked(format!(
            "Failed to solve Cloudflare challenge after {MAX_CHALLENGE_RETRIES} attempts"
        )))
    }

    /// Parse HTML into a basic accessibility tree (best-effort without JS).
    fn parse_html_to_snapshot(url: &str, html: &str) -> PageSnapshot {
        let title = extract_title(html).unwrap_or_default();
        let mut refs = HashMap::new();
        let mut tree = String::new();
        let mut ref_counter = 0u32;

        // Extract links
        for (href, text) in extract_links(html) {
            ref_counter += 1;
            let ref_id = format!("e{ref_counter}");
            tree.push_str(&format!("[@{ref_id}] link \"{text}\"\n"));
            refs.insert(
                ref_id.clone(),
                ElementRef {
                    selector: format!("a[href=\"{href}\"]"),
                    role: "link".into(),
                    name: text,
                    nth: None,
                    attributes: HashMap::from([("href".into(), href)]),
                },
            );
        }

        // Extract form inputs
        for (input_type, name, placeholder) in extract_inputs(html) {
            ref_counter += 1;
            let ref_id = format!("e{ref_counter}");
            let role = match input_type.as_str() {
                "submit" => "button",
                "checkbox" => "checkbox",
                "radio" => "radio",
                _ => "textbox",
            };
            let display_name = if !placeholder.is_empty() {
                placeholder.clone()
            } else {
                name.clone()
            };
            tree.push_str(&format!("[@{ref_id}] {role} \"{display_name}\"\n"));
            refs.insert(
                ref_id.clone(),
                ElementRef {
                    selector: format!("input[name=\"{name}\"]"),
                    role: role.into(),
                    name: display_name,
                    nth: None,
                    attributes: HashMap::from([("type".into(), input_type), ("name".into(), name)]),
                },
            );
        }

        // Extract buttons
        for button_text in extract_buttons(html) {
            ref_counter += 1;
            let ref_id = format!("e{ref_counter}");
            tree.push_str(&format!("[@{ref_id}] button \"{button_text}\"\n"));
            refs.insert(
                ref_id.clone(),
                ElementRef {
                    selector: format!("button:has-text(\"{button_text}\")"),
                    role: "button".into(),
                    name: button_text,
                    nth: None,
                    attributes: HashMap::new(),
                },
            );
        }

        PageSnapshot {
            url: url.to_string(),
            title,
            tree,
            refs,
        }
    }
}

#[async_trait]
impl BrowsingEngine for StealthEngine {
    async fn navigate(&self, config: &SessionConfig) -> Result<NavigationResult, EngineError> {
        let (response, protection_bypassed) = self
            .navigate_with_challenges(&config.target_url, config.timeout_secs)
            .await?;

        // Store page state for subsequent snapshot/cookie calls
        {
            let mut page = self.current_page.write().await;
            *page = Some(PageState {
                url: config.target_url.clone(),
                body: response.body,
                headers: response.headers,
            });
        }

        Ok(NavigationResult {
            final_url: config.target_url.clone(),
            status_code: response.status,
            layer_used: Layer::StealthHttp,
            captcha_solved: false,
            protection_bypassed,
        })
    }

    async fn snapshot(&self, _options: &SnapshotOptions) -> Result<PageSnapshot, EngineError> {
        let page = self.current_page.read().await;
        let page = page
            .as_ref()
            .ok_or_else(|| EngineError::Navigation("No page loaded yet".into()))?;

        Ok(Self::parse_html_to_snapshot(&page.url, &page.body))
    }

    async fn click(&self, _ref_id: &str) -> Result<(), EngineError> {
        Err(EngineError::BrowserError(
            "Click requires browser engine — stealth HTTP cannot interact with elements".into(),
        ))
    }

    async fn fill(&self, _ref_id: &str, _text: &str) -> Result<(), EngineError> {
        Err(EngineError::BrowserError(
            "Fill requires browser engine — stealth HTTP cannot interact with elements".into(),
        ))
    }

    async fn type_text(&self, _ref_id: &str, _text: &str) -> Result<(), EngineError> {
        Err(EngineError::BrowserError(
            "Type requires browser engine — stealth HTTP cannot interact with elements".into(),
        ))
    }

    async fn screenshot(&self) -> Result<Vec<u8>, EngineError> {
        Err(EngineError::ScreenshotFailed(
            "Screenshots require browser engine — stealth HTTP has no renderer".into(),
        ))
    }

    async fn get_cookies(&self) -> Result<CookieJar, EngineError> {
        let page = self.current_page.read().await;
        let page = page
            .as_ref()
            .ok_or_else(|| EngineError::CookieError("No page loaded yet".into()))?;

        let cookies: Vec<Cookie> = page
            .headers
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .filter_map(|s| parse_set_cookie(s, &page.url))
            .collect();

        Ok(CookieJar {
            source_url: page.url.clone(),
            user_agent: self.fingerprint.navigator.user_agent.clone(),
            captured_at: Utc::now(),
            cookies,
        })
    }

    async fn set_cookies(&self, _cookies: &[Cookie]) -> Result<(), EngineError> {
        // reqwest's internal cookie store handles persistence across requests.
        // Explicit injection would require rebuilding the client — not supported here.
        warn!("set_cookies on stealth HTTP layer is a no-op (managed by reqwest cookie store)");
        Ok(())
    }

    async fn current_url(&self) -> Result<String, EngineError> {
        let page = self.current_page.read().await;
        page.as_ref()
            .map(|p| p.url.clone())
            .ok_or_else(|| EngineError::Navigation("No page loaded yet".into()))
    }

    async fn can_handle(&self, url: &str) -> bool {
        let failed = self.failed_urls.read().await;
        !failed.iter().any(|u| u == url)
    }

    fn name(&self) -> &str {
        "stealth-http"
    }
}

// ─── HTML Parsing Helpers ───────────────────────────────────────────────────

/// Extract the domain from a URL.
fn extract_domain(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.host_str().unwrap_or("unknown").to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Extract `<title>` content from HTML.
fn extract_title(html: &str) -> Option<String> {
    let start = html.find("<title")?;
    let rest = &html[start + 6..];
    let content_start = rest.find('>')? + 1;
    let content_end = rest.find("</title>")?;
    Some(rest[content_start..content_end].trim().to_string())
}

/// Extract links as (href, visible_text) pairs from HTML.
fn extract_links(html: &str) -> Vec<(String, String)> {
    let mut links = Vec::new();
    let mut search_from = 0;

    while let Some(pos) = html[search_from..].find("<a ") {
        let abs_pos = search_from + pos;
        let tag_end = match html[abs_pos..].find('>') {
            Some(e) => abs_pos + e,
            None => break,
        };

        if let Some(href) = extract_attr(&html[abs_pos..tag_end + 1], "href") {
            let close_tag = html[tag_end..].find("</a>").unwrap_or(0) + tag_end;
            let text = strip_tags(&html[tag_end + 1..close_tag]).trim().to_string();
            if !text.is_empty() && !href.starts_with('#') && !href.starts_with("javascript:") {
                links.push((href, text));
            }
        }

        search_from = tag_end + 1;
    }

    links
}

/// Extract visible form inputs as (type, name, placeholder) tuples.
fn extract_inputs(html: &str) -> Vec<(String, String, String)> {
    let mut inputs = Vec::new();
    let mut search_from = 0;

    while let Some(pos) = html[search_from..].find("<input") {
        let abs_pos = search_from + pos;
        let tag_end = match html[abs_pos..].find('>') {
            Some(e) => abs_pos + e,
            None => break,
        };

        let tag = &html[abs_pos..tag_end + 1];
        let input_type = extract_attr(tag, "type").unwrap_or_else(|| "text".into());
        let name = extract_attr(tag, "name").unwrap_or_default();
        let placeholder = extract_attr(tag, "placeholder").unwrap_or_default();

        if input_type != "hidden" {
            inputs.push((input_type, name, placeholder));
        }

        search_from = tag_end + 1;
    }

    inputs
}

/// Extract button visible text from HTML.
fn extract_buttons(html: &str) -> Vec<String> {
    let mut buttons = Vec::new();
    let mut search_from = 0;

    while let Some(pos) = html[search_from..].find("<button") {
        let abs_pos = search_from + pos;
        let tag_end = match html[abs_pos..].find('>') {
            Some(e) => abs_pos + e,
            None => break,
        };

        let close_tag = html[tag_end..].find("</button>").unwrap_or(0) + tag_end;
        let text = strip_tags(&html[tag_end + 1..close_tag]).trim().to_string();
        if !text.is_empty() {
            buttons.push(text);
        }

        search_from = tag_end + 1;
    }

    buttons
}

/// Extract an attribute value from an HTML tag string.
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("{attr}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

/// Strip HTML tags from a string.
fn strip_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

/// Parse a Set-Cookie header value into our Cookie type.
fn parse_set_cookie(header: &str, url: &str) -> Option<Cookie> {
    let parts: Vec<&str> = header.split(';').collect();
    let name_value = parts.first()?;
    let (name, value) = name_value.split_once('=')?;

    let domain = extract_domain(url);
    let mut cookie = Cookie {
        name: name.trim().to_string(),
        value: value.trim().to_string(),
        domain: domain.clone(),
        path: "/".to_string(),
        expires: None,
        secure: false,
        http_only: false,
        same_site: cloudyab_types::SameSite::Lax,
    };

    for part in parts.iter().skip(1) {
        let part = part.trim().to_lowercase();
        if part == "secure" {
            cookie.secure = true;
        } else if part == "httponly" {
            cookie.http_only = true;
        } else if let Some(path) = part.strip_prefix("path=") {
            cookie.path = path.to_string();
        } else if let Some(d) = part.strip_prefix("domain=") {
            cookie.domain = d.trim_start_matches('.').to_string();
        } else if let Some(val) = part.strip_prefix("samesite=") {
            cookie.same_site = match val {
                "strict" => cloudyab_types::SameSite::Strict,
                "none" => cloudyab_types::SameSite::None,
                _ => cloudyab_types::SameSite::Lax,
            };
        }
    }

    Some(cookie)
}
