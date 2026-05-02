//! AI-powered captcha solver with pluggable backends.
//!
//! Uses local ONNX models for lightweight inference (text OCR, image classification)
//! with optional cloud API fallback for complex captchas.

pub mod local;
pub mod registry;
pub mod traits;
