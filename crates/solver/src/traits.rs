//! Solver trait definitions — the pluggable interface.

use async_trait::async_trait;
use cloudyab_types::captcha::{CaptchaResult, CaptchaType};

/// A captcha solver backend.
///
/// Implement this trait to add a new solver (local AI, cloud API, etc.).
#[async_trait]
pub trait Solver: Send + Sync {
    /// Attempt to solve a captcha from its screenshot.
    ///
    /// - `image`: PNG screenshot bytes of the captcha element
    /// - `captcha_type`: The detected type of captcha
    /// - `context`: Additional context (page URL, prompt text, etc.)
    async fn solve(
        &self,
        image: &[u8],
        captcha_type: &CaptchaType,
        context: &str,
    ) -> Result<CaptchaResult, SolverError>;

    /// Check if this solver supports the given captcha type.
    fn supports(&self, captcha_type: &CaptchaType) -> bool;

    /// Human-readable name of this solver.
    fn name(&self) -> &str;

    /// Priority (lower = tried first). Default: 100.
    fn priority(&self) -> u32 {
        100
    }
}

/// Errors from solver operations.
#[derive(Debug, thiserror::Error)]
pub enum SolverError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("ONNX inference failed: {0}")]
    InferenceFailed(String),

    #[error("Image preprocessing failed: {0}")]
    ImageProcessing(String),

    #[error("Cloud API error: {status} - {message}")]
    CloudApi { status: u16, message: String },

    #[error("Unsupported captcha type: {0}")]
    Unsupported(String),

    #[error("Timeout: solver took too long")]
    Timeout,

    #[error("All attempts exhausted")]
    Exhausted,
}
