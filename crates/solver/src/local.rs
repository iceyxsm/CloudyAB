//! Local ONNX-based captcha solvers.
//!
//! Implements text OCR, image classification, and slider puzzle solving
//! using ONNX Runtime for inference and the `image` crate for preprocessing.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use tokio::sync::OnceCell;
use tracing::info;

use cloudyab_types::captcha::{CaptchaResult, CaptchaSolution, CaptchaType};

use crate::preprocess;
use crate::traits::{Solver, SolverError};

/// Default input height for text OCR models.
const TEXT_MODEL_HEIGHT: u32 = 32;

/// Default input width for text OCR models.
const TEXT_MODEL_WIDTH: u32 = 128;

/// Default input size for image classification models.
const CLASSIFIER_INPUT_SIZE: u32 = 224;

/// Default grid dimensions for image selection captchas.
const GRID_ROWS: u32 = 3;

/// Default grid columns for image selection captchas.
const GRID_COLS: u32 = 3;

/// Classification confidence threshold for selecting tiles.
const TILE_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// Fraction of image width to skip from left when finding slider slot.
const SLIDER_SKIP_LEFT: f32 = 0.15;

/// CTC blank token index (conventionally index 0).
const CTC_BLANK_INDEX: usize = 0;

/// Default character set for text captcha OCR (digits + lowercase + uppercase).
const DEFAULT_CHARSET: &str = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Number of intra-op threads for ONNX inference.
const INFERENCE_THREADS: usize = 4;

// ─── Text OCR Solver ────────────────────────────────────────────────────

/// Text captcha solver using a CNN-RNN-CTC ONNX model.
///
/// Expects a model with:
/// - Input: `[1, 1, 32, 128]` grayscale normalized image
/// - Output: `[1, seq_len, num_chars+1]` logits (index 0 = blank)
pub struct TextOcrSolver {
    model_path: PathBuf,
    session: OnceCell<Arc<Mutex<Session>>>,
    charset: Vec<char>,
}

impl TextOcrSolver {
    /// Create a new text OCR solver pointing to the model directory.
    pub fn new(models_dir: &std::path::Path) -> Self {
        Self {
            model_path: models_dir.join("text_captcha.onnx"),
            session: OnceCell::new(),
            charset: DEFAULT_CHARSET.chars().collect(),
        }
    }

    /// Lazily load the ONNX session on first use.
    async fn get_session(&self) -> Result<&Arc<Mutex<Session>>, SolverError> {
        self.session
            .get_or_try_init(|| async { load_session(&self.model_path) })
            .await
    }

    /// Decode CTC output logits (flat buffer) into text using greedy decoding.
    fn ctc_decode_raw(&self, data: &[f32], seq_len: usize, num_chars: usize) -> String {
        let mut result = String::new();
        let mut prev_idx = CTC_BLANK_INDEX;

        for t in 0..seq_len {
            let row_start = t * num_chars;
            let mut max_idx = 0;
            let mut max_val = f32::NEG_INFINITY;

            for c in 0..num_chars {
                let val = data[row_start + c];
                if val > max_val {
                    max_val = val;
                    max_idx = c;
                }
            }

            if max_idx != CTC_BLANK_INDEX && max_idx != prev_idx {
                if let Some(&ch) = self.charset.get(max_idx - 1) {
                    result.push(ch);
                }
            }
            prev_idx = max_idx;
        }

        result
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
        let session = self.get_session().await?.clone();

        let img = preprocess::decode_image(image)?;
        let tensor = preprocess::to_grayscale_tensor(&img, TEXT_MODEL_HEIGHT, TEXT_MODEL_WIDTH)?;

        let output_data: Vec<f32> = tokio::task::spawn_blocking(move || {
            let input_tensor = Tensor::from_array(tensor)
                .map_err(|e| SolverError::InferenceFailed(format!("Tensor creation failed: {e}")))?;
            let mut sess = session.lock().map_err(|e| {
                SolverError::InferenceFailed(format!("Session lock poisoned: {e}"))
            })?;
            let outputs = sess
                .run(ort::inputs!["input" => input_tensor])
                .map_err(|e| SolverError::InferenceFailed(format!("Inference failed: {e}")))?;
            let logits = outputs["output"]
                .try_extract_array::<f32>()
                .map_err(|e| SolverError::InferenceFailed(format!("Output extraction failed: {e}")))?;
            Ok::<Vec<f32>, SolverError>(logits.iter().copied().collect())
        })
        .await
        .map_err(|e| SolverError::InferenceFailed(format!("Task join failed: {e}")))??;

        let num_chars = self.charset.len() + 1;
        let seq_len = output_data.len() / num_chars;
        let text = self.ctc_decode_raw(&output_data, seq_len, num_chars);

        Ok(CaptchaResult {
            success: !text.is_empty(),
            solution: if text.is_empty() { None } else { Some(CaptchaSolution::Text(text)) },
            solver_used: self.name().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
        })
    }

    fn supports(&self, captcha_type: &CaptchaType) -> bool {
        matches!(captcha_type, CaptchaType::TextRecognition)
    }

    fn name(&self) -> &str {
        "local-text-ocr"
    }

    fn priority(&self) -> u32 {
        10
    }
}

// ─── Image Classifier Solver ────────────────────────────────────────────

/// Image classification solver for reCAPTCHA-style grid challenges.
///
/// Expects a model with:
/// - Input: `[1, 3, 224, 224]` RGB ImageNet-normalized image
/// - Output: `[1, num_classes]` class probabilities
pub struct ImageClassifierSolver {
    model_path: PathBuf,
    session: OnceCell<Arc<Mutex<Session>>>,
}

impl ImageClassifierSolver {
    /// Create a new image classifier solver.
    pub fn new(models_dir: &std::path::Path) -> Self {
        Self {
            model_path: models_dir.join("image_classifier.onnx"),
            session: OnceCell::new(),
        }
    }

    /// Lazily load the ONNX session.
    async fn get_session(&self) -> Result<&Arc<Mutex<Session>>, SolverError> {
        self.session
            .get_or_try_init(|| async { load_session(&self.model_path) })
            .await
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

        let prompt = match captcha_type {
            CaptchaType::ImageSelection { prompt } => prompt.clone(),
            _ => return Err(SolverError::Unsupported("Not an image selection captcha".into())),
        };

        let session = self.get_session().await?.clone();
        let img = preprocess::decode_image(image)?;
        let tiles = preprocess::split_into_tiles(
            &img, GRID_ROWS, GRID_COLS, CLASSIFIER_INPUT_SIZE, CLASSIFIER_INPUT_SIZE,
        )?;

        info!(prompt = %prompt, tiles = tiles.len(), "Classifying grid tiles");

        let mut matching_coords: Vec<(i32, i32)> = Vec::new();

        for (idx, tile_tensor) in tiles.into_iter().enumerate() {
            let sess = session.clone();
            let scores: Vec<f32> = tokio::task::spawn_blocking(move || {
                let input_tensor = Tensor::from_array(tile_tensor)
                    .map_err(|e| SolverError::InferenceFailed(format!("Tensor failed: {e}")))?;
                let mut guard = sess.lock().map_err(|e| {
                    SolverError::InferenceFailed(format!("Session lock poisoned: {e}"))
                })?;
                let outputs = guard
                    .run(ort::inputs!["input" => input_tensor])
                    .map_err(|e| SolverError::InferenceFailed(format!("Inference failed: {e}")))?;
                let probs = outputs["output"]
                    .try_extract_array::<f32>()
                    .map_err(|e| SolverError::InferenceFailed(format!("Extract failed: {e}")))?;
                Ok::<Vec<f32>, SolverError>(probs.iter().copied().collect())
            })
            .await
            .map_err(|e| SolverError::InferenceFailed(format!("Task join failed: {e}")))??;

            let max_score = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            if max_score >= TILE_CONFIDENCE_THRESHOLD {
                let row = (idx as u32 / GRID_COLS) as i32;
                let col = (idx as u32 % GRID_COLS) as i32;
                matching_coords.push((col, row));
            }
        }

        let success = !matching_coords.is_empty();
        Ok(CaptchaResult {
            success,
            solution: if success { Some(CaptchaSolution::Coordinates(matching_coords)) } else { None },
            solver_used: self.name().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
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

// ─── Slider Puzzle Solver ───────────────────────────────────────────────

/// Slider puzzle solver using edge detection (no ML model needed).
///
/// Detects the puzzle slot position by analyzing vertical edge energy
/// in the background image and finding the peak.
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

        let img = preprocess::decode_image(image)?;
        let profile = preprocess::horizontal_edge_profile(&img);
        let offset = preprocess::find_slot_offset(&profile, SLIDER_SKIP_LEFT);

        info!(offset, "Slider slot detected");

        Ok(CaptchaResult {
            success: offset > 0,
            solution: Some(CaptchaSolution::SliderOffset(offset)),
            solver_used: self.name().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
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

// ─── Shared Helpers ─────────────────────────────────────────────────────

/// Load an ONNX session from a file path with optimized settings.
fn load_session(path: &std::path::Path) -> Result<Arc<Mutex<Session>>, SolverError> {
    if !path.exists() {
        return Err(SolverError::ModelNotFound(path.display().to_string()));
    }

    info!(model = %path.display(), "Loading ONNX model");

    let session = Session::builder()
        .map_err(|e| SolverError::InferenceFailed(format!("Session builder failed: {e}")))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| SolverError::InferenceFailed(format!("Optimization config failed: {e}")))?
        .with_intra_threads(INFERENCE_THREADS)
        .map_err(|e| SolverError::InferenceFailed(format!("Thread config failed: {e}")))?
        .commit_from_file(path)
        .map_err(|e| SolverError::InferenceFailed(format!("Model load failed: {e}")))?;

    Ok(Arc::new(Mutex::new(session)))
}
