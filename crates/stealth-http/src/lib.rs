//! TLS fingerprint spoofing and Cloudflare JS challenge solver.
//!
//! This is Layer 2 — the lightweight HTTP-only approach that handles
//! sites with basic Cloudflare/WAF protection without launching a browser.
//! Inspired by cloudscraper's approach: custom TLS fingerprints + JS interpreter.
//!
//! The [`engine::StealthEngine`] implements `BrowsingEngine` and orchestrates:
//! request → challenge detection → JS solve → resubmit → return cookies/body.

pub mod challenge;
pub mod client;
pub mod engine;
pub mod tls;
