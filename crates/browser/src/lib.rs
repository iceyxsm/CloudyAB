//! Headless browser engine wrapper.
//!
//! Wraps a CDP-compatible browser binary (Obscura or stealth-patched Chromium)
//! with anti-detection script injection, human-like interaction dispatch,
//! and accessibility tree extraction.
//! This is Layer 2 — used when the HTTP stealth layer can't handle the site.

pub mod config;
pub mod engine;
pub mod stealth;
