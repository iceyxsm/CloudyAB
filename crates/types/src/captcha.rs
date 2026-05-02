//! Captcha-related types.

use serde::{Deserialize, Serialize};

/// The type of captcha detected on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptchaType {
    /// Simple text recognition (distorted characters)
    TextRecognition,
    /// Image grid selection ("select all traffic lights")
    ImageSelection { prompt: String },
    /// Slider puzzle (drag piece to correct position)
    SliderPuzzle,
    /// Cloudflare Turnstile challenge
    CloudflareTurnstile,
    /// AWS WAF CAPTCHA
    AwsWafCaptcha,
    /// hCaptcha challenge
    HCaptcha,
    /// Google reCAPTCHA v2
    RecaptchaV2,
    /// Unknown or custom captcha type
    Custom(String),
}

/// The solution produced by a captcha solver.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptchaSolution {
    /// Text answer (e.g., OCR result)
    Text(String),
    /// Click coordinates (for image selection)
    Coordinates(Vec<(i32, i32)>),
    /// Slider offset in pixels
    SliderOffset(i32),
    /// Token-based solution (for Turnstile, reCAPTCHA)
    Token(String),
}

/// Result of a captcha solving attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptchaResult {
    /// Whether the solve was successful
    pub success: bool,
    /// The solution if successful
    pub solution: Option<CaptchaSolution>,
    /// Which solver backend was used
    pub solver_used: String,
    /// Time taken in milliseconds
    pub duration_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
}
