//! Common error types shared across CloudyAB crates.

use serde::{Deserialize, Serialize};

/// The layer that produced an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorLayer {
    /// HTTP stealth layer (TLS, challenge solving)
    StealthHttp,
    /// Full browser engine
    Browser,
    /// Captcha solver
    Solver,
    /// Cookie store
    Cookies,
    /// Human interaction simulation
    Human,
    /// Snapshot/accessibility tree
    Snapshot,
}

/// Severity of an error for reporting purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    /// Recoverable — try next layer or retry
    Recoverable,
    /// Fatal — cannot proceed
    Fatal,
}
