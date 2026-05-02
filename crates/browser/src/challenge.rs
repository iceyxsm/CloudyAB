//! Challenge detection in the browser layer.
//!
//! Uses DOM inspection via CDP to identify captcha types present on a page.
//! Goes beyond simple text matching by checking:
//! - Known iframe sources (Turnstile, hCaptcha, reCAPTCHA)
//! - Challenge page patterns (full-page interstitials)
//! - DOM element signatures (specific class names, IDs, data attributes)
//! - Script sources that indicate challenge frameworks

use cloudyab_types::captcha::CaptchaType;
use serde::Deserialize;
use tracing::{debug, info};

/// Result of challenge detection on a page.
#[derive(Debug, Clone)]
pub struct ChallengeDetection {
    /// The type of captcha/challenge detected.
    pub captcha_type: CaptchaType,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// CSS selector for the challenge container element.
    pub container_selector: Option<String>,
    /// Whether this is a full-page interstitial (vs embedded widget).
    pub is_interstitial: bool,
}

/// Raw detection result from the browser JS execution.
#[derive(Debug, Deserialize)]
struct RawDetection {
    captcha_type: String,
    confidence: f32,
    container: Option<String>,
    is_interstitial: bool,
}

/// JavaScript that performs deep DOM inspection for challenge detection.
/// Checks iframes, scripts, known element patterns, and page structure.
const DETECTION_JS: &str = r#"(() => {
    const detections = [];

    function exists(selector) {
        return document.querySelector(selector) !== null;
    }

    function checkIframes() {
        const iframes = document.querySelectorAll('iframe');
        for (const iframe of iframes) {
            const src = (iframe.src || '').toLowerCase();
            if (src.includes('challenges.cloudflare.com') || src.includes('turnstile')) {
                detections.push({
                    captcha_type: 'cloudflare_turnstile',
                    confidence: 0.95,
                    container: getSelector(iframe.parentElement),
                    is_interstitial: false
                });
            }
            if (src.includes('hcaptcha.com') || src.includes('newassets.hcaptcha.com')) {
                detections.push({
                    captcha_type: 'hcaptcha',
                    confidence: 0.95,
                    container: getSelector(iframe.parentElement),
                    is_interstitial: false
                });
            }
            if (src.includes('google.com/recaptcha') || src.includes('recaptcha/api')) {
                detections.push({
                    captcha_type: 'recaptcha_v2',
                    confidence: 0.95,
                    container: getSelector(iframe.parentElement),
                    is_interstitial: false
                });
            }
        }
    }

    function checkScripts() {
        const scripts = document.querySelectorAll('script[src]');
        for (const script of scripts) {
            const src = (script.src || '').toLowerCase();
            if (src.includes('challenges.cloudflare.com/turnstile')) {
                detections.push({
                    captcha_type: 'cloudflare_turnstile',
                    confidence: 0.85,
                    container: null,
                    is_interstitial: false
                });
            }
            if (src.includes('js.hcaptcha.com')) {
                detections.push({
                    captcha_type: 'hcaptcha',
                    confidence: 0.85,
                    container: null,
                    is_interstitial: false
                });
            }
            if (src.includes('google.com/recaptcha') || src.includes('gstatic.com/recaptcha')) {
                detections.push({
                    captcha_type: 'recaptcha_v2',
                    confidence: 0.85,
                    container: null,
                    is_interstitial: false
                });
            }
        }
    }

    function checkDomPatterns() {
        if (exists('.cf-turnstile') || exists('[data-sitekey][data-callback]')) {
            detections.push({
                captcha_type: 'cloudflare_turnstile',
                confidence: 0.9,
                container: '.cf-turnstile',
                is_interstitial: false
            });
        }

        if (exists('#challenge-form') || exists('#cf-challenge-running')) {
            detections.push({
                captcha_type: 'cloudflare_turnstile',
                confidence: 0.95,
                container: '#challenge-form',
                is_interstitial: true
            });
        }

        if (exists('.h-captcha') || exists('[data-hcaptcha-widget-id]')) {
            detections.push({
                captcha_type: 'hcaptcha',
                confidence: 0.9,
                container: '.h-captcha',
                is_interstitial: false
            });
        }

        if (exists('.g-recaptcha') || exists('#recaptcha')) {
            detections.push({
                captcha_type: 'recaptcha_v2',
                confidence: 0.9,
                container: '.g-recaptcha',
                is_interstitial: false
            });
        }

        if (exists('#captcha-container') && document.title.includes('Request Blocked')) {
            detections.push({
                captcha_type: 'aws_waf_captcha',
                confidence: 0.9,
                container: '#captcha-container',
                is_interstitial: true
            });
        }
        if (exists('form[action*="awswaf"]') || exists('[data-awswaf]')) {
            detections.push({
                captcha_type: 'aws_waf_captcha',
                confidence: 0.85,
                container: 'form[action*="awswaf"]',
                is_interstitial: true
            });
        }

        if (exists('.slider-captcha') || exists('.slide-verify') || exists('[data-slider-captcha]')) {
            detections.push({
                captcha_type: 'slider_puzzle',
                confidence: 0.85,
                container: '.slider-captcha, .slide-verify, [data-slider-captcha]',
                is_interstitial: false
            });
        }

        if (exists('img[alt*="captcha" i]') || exists('img[src*="captcha" i]')) {
            const hasInput = exists('input[name*="captcha" i]') || exists('input[placeholder*="code" i]');
            if (hasInput) {
                detections.push({
                    captcha_type: 'text_recognition',
                    confidence: 0.75,
                    container: null,
                    is_interstitial: false
                });
            }
        }
    }

    function checkInterstitialSignals() {
        const title = document.title.toLowerCase();
        const bodyText = (document.body?.innerText || '').toLowerCase().substring(0, 2000);

        if (title.includes('just a moment') || title.includes('attention required')) {
            detections.push({
                captcha_type: 'cloudflare_turnstile',
                confidence: 0.8,
                container: null,
                is_interstitial: true
            });
        }

        if (bodyText.includes('verify you are human') || bodyText.includes('prove you are not a robot')) {
            if (detections.length === 0) {
                detections.push({
                    captcha_type: 'text_recognition',
                    confidence: 0.5,
                    container: null,
                    is_interstitial: true
                });
            }
        }
    }

    function getSelector(el) {
        if (!el) return null;
        if (el.id) return '#' + el.id;
        if (el.className && typeof el.className === 'string') {
            const cls = el.className.trim().split(/\s+/)[0];
            if (cls) return '.' + cls;
        }
        return el.tagName.toLowerCase();
    }

    checkIframes();
    checkScripts();
    checkDomPatterns();
    checkInterstitialSignals();

    const best = {};
    for (const d of detections) {
        if (!best[d.captcha_type] || d.confidence > best[d.captcha_type].confidence) {
            best[d.captcha_type] = d;
        }
    }

    return Object.values(best);
})()"#;

/// Detect challenges on the current page using deep DOM inspection.
///
/// Executes JavaScript in the browser context to check iframes, scripts,
/// DOM patterns, and page-level signals for known captcha/challenge types.
///
/// Returns detections sorted by confidence (highest first).
pub fn parse_detection_results(raw_json: &serde_json::Value) -> Vec<ChallengeDetection> {
    let raw_detections: Vec<RawDetection> = match serde_json::from_value(raw_json.clone()) {
        Ok(d) => d,
        Err(e) => {
            debug!(error = %e, "Failed to parse challenge detection results");
            return Vec::new();
        }
    };

    let mut detections: Vec<ChallengeDetection> = raw_detections
        .into_iter()
        .filter_map(|raw| {
            let captcha_type = map_captcha_type(&raw.captcha_type);
            Some(ChallengeDetection {
                captcha_type,
                confidence: raw.confidence,
                container_selector: raw.container,
                is_interstitial: raw.is_interstitial,
            })
        })
        .collect();

    detections.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if !detections.is_empty() {
        info!(
            count = detections.len(),
            top_type = ?detections[0].captcha_type,
            top_confidence = detections[0].confidence,
            "Challenge detection complete"
        );
    }

    detections
}

/// Map a raw captcha type string to the CaptchaType enum.
fn map_captcha_type(raw: &str) -> CaptchaType {
    match raw {
        "cloudflare_turnstile" => CaptchaType::CloudflareTurnstile,
        "hcaptcha" => CaptchaType::HCaptcha,
        "recaptcha_v2" => CaptchaType::RecaptchaV2,
        "aws_waf_captcha" => CaptchaType::AwsWafCaptcha,
        "slider_puzzle" => CaptchaType::SliderPuzzle,
        "text_recognition" => CaptchaType::TextRecognition,
        other => CaptchaType::Custom(other.to_string()),
    }
}

/// Get the JavaScript code for challenge detection.
/// This is injected into the browser page via CDP evaluate.
pub fn detection_script() -> &'static str {
    DETECTION_JS
}
