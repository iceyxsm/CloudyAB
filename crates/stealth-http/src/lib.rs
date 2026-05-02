//! TLS fingerprint spoofing and Cloudflare JS challenge solver.
//!
//! This is Layer 1 — the lightweight HTTP-only approach that handles
//! sites with basic Cloudflare/WAF protection without launching a browser.
//! Inspired by cloudscraper's approach: custom TLS fingerprints + JS interpreter.

pub mod challenge;
pub mod client;
pub mod tls;
