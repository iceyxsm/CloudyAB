//! Global configuration for CloudyAB.

use cloudyab_types::ProxyConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level configuration for the CloudyAB engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudyAbConfig {
    /// Path to SQLite cookie database
    pub cookie_db_path: PathBuf,
    /// Path to ONNX model directory
    pub models_dir: PathBuf,
    /// Default timeout for page loads (seconds)
    pub default_timeout_secs: u64,
    /// Whether to auto-escalate from HTTP to browser layer
    pub auto_escalate: bool,
    /// Maximum RAM budget in MB (for monitoring)
    pub max_ram_mb: u32,
    /// Captcha solver configuration
    pub solver: SolverConfig,
    /// Proxy configuration (global default)
    pub proxy: Option<ProxyConfig>,
}

/// Configuration for the captcha solver subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverConfig {
    /// Whether to use local ONNX models
    pub use_local: bool,
    /// Cloud API endpoint (fallback)
    pub cloud_api_url: Option<String>,
    /// Cloud API key
    pub cloud_api_key: Option<String>,
    /// Maximum attempts per captcha
    pub max_attempts: u32,
}

impl Default for CloudyAbConfig {
    fn default() -> Self {
        Self {
            cookie_db_path: PathBuf::from("data/cookies.db"),
            models_dir: PathBuf::from("models"),
            default_timeout_secs: 30,
            auto_escalate: true,
            max_ram_mb: 500,
            solver: SolverConfig::default(),
            proxy: None,
        }
    }
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            use_local: true,
            cloud_api_url: None,
            cloud_api_key: None,
            max_attempts: 3,
        }
    }
}
