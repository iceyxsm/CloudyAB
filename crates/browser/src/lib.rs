//! Headless browser engine wrapper.
//!
//! Wraps the Obscura engine (or compatible CDP-based engine) with stealth
//! features, human-like interaction, and accessibility tree extraction.
//! This is Layer 2 — used when the HTTP stealth layer can't handle the site.

pub mod engine;
