//! Local ONNX-based captcha solvers.

use crate::traits::{Solver, SolverError};
use async_trait::async_trait;
use cloudyab_types::captcha::{CaptchaResult, CaptchaSolution, CaptchaType};
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info};

/// Text captcha solver using a CNN-RNN-CTC ONNX model.
pub struct TextOcrSolver {
    model_path: PathBuf,
}

impl TextOcrSolver {
    /// Create a new text OCR solver.
    pub fn new(models_dir: &std::path::Path) -> Self {
        Self {
            model_path: models_dir.join("text_captcha.onnx"),
        }
    }
}

#[async_trait]
impl Solver for TextOcrSolver {
    async fn solve(
        &self,
        image: &[u8],
        _captcha_type: &CaptchaType,
        _context: &str,
    ) -> Result<CaptchaResult, SolverError> {
        let start = Instant::now();

        if !self.model_path.exists() {
            return Err(SolverError::ModelNotFound(
                self.model_path.display().to_string(),
            ));
        }

        info!(model = %self.model_path.display(), "Running text OCR inference");

        // TODO: Load ONNX model via `ort` and run inference
        // For now, return a placeholder that shows the structure
        let _ = image; // Will be preprocessed and fed to model

        debug!("Text OCR inference complete");

        Ok(CaptchaResult {
            success: false,
            solution: None,
            solver_used: self.name().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some("Model not yet loaded — implementation pending".into()),
        })
    }

    fn supports(&self, captcha_type: &CaptchaType) -> bool {
        matches!(captcha_type, CaptchaType::TextRecognition)
    }

    fn name(&self) -> &str {
        "local-text-ocr"
    }

    fn priority(&self) -> u32 {
        10 // High priority (tried first)
    }
}

/// Image classification solver for reCAPTCHA-style grid challenges.
pub struct ImageClassifierSolver {
    model_path: PathBuf,
}

impl ImageClassifierSolver {
    /// Create a new image classifier solver.
    pub fn new(models_dir: &std::path::Path) -> Self {
        Self {
            model_path: models_dir.join("image_classifier.onnx"),
        }
    }
}

#[async_trait]
impl Solver for ImageClassifierSolver {
    async fn solve(
        &self,
        image: &[u8],
        captcha_type: &CaptchaType,
        _context: &str,
    ) -> Result<CaptchaResult, SolverError> {
        let start = Instant::now();

        if !self.model_path.exists() {
            return Err(SolverError::ModelNotFound(
                self.model_path.display().to_string(),
            ));
        }

        let prompt = match captcha_type {
            CaptchaType::ImageSelection { prompt } => prompt.clone(),
            _ => return Err(SolverError::Unsupported("Not an image selection captcha".into())),
        };

        info!(model = %self.model_path.display(), prompt = %prompt, "Running image classification");

        let _ = image; // Will be split into tiles and classified

        Ok(CaptchaResult {
            success: false,
            solution: None,
            solver_used: self.name().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some("Model not yet loaded — implementation pending".into()),
        })
    }

    fn supports(&self, captcha_type: &CaptchaType) -> bool {
        matches!(captcha_type, CaptchaType::ImageSelection { .. } | CaptchaType::RecaptchaV2)
    }

    fn name(&self) -> &str {
        "local-image-classifier"
    }

    fn priority(&self) -> u32 {
        10
    }
}

/// Slider puzzle solver using edge detection.
pub struct SliderSolver;

#[async_trait]
impl Solver for SliderSolver {
    async fn solve(
        &self,
        image: &[u8],
        _captcha_type: &CaptchaType,
        _context: &str,
    ) -> Result<CaptchaResult, SolverError> {
        let start = Instant::now();

        // Slider puzzles can be solved with image processing (no ML needed):
        // 1. Detect the puzzle piece outline via edge detection
        // 2. Find the matching slot in the background
        // 3. Calculate the pixel offset

        let _ = image; // Will use `image` crate for processing

        Ok(CaptchaResult {
            success: false,
            solution: Some(CaptchaSolution::SliderOffset(0)),
            solver_used: self.name().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some("Edge detection not yet implemented".into()),
        })
    }

    fn supports(&self, captcha_type: &CaptchaType) -> bool {
        matches!(captcha_type, CaptchaType::SliderPuzzle)
    }

    fn name(&self) -> &str {
        "local-slider"
    }

    fn priority(&self) -> u32 {
        10
    }
}
