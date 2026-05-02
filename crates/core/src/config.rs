//! Global configuration for CloudyAB.
//!
//! Loaded from a TOML file (default: `cloudyab.toml` or `CLOUDYAB_CONFIG` env var).
//! Every subsystem can be enabled/disabled independently.

use cloudyab_types::ProxyConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

/// Environment variable for config file path override.
const CONFIG_ENV_VAR: &str = "CLOUDYAB_CONFIG";

/// Default config file name.
const DEFAULT_CONFIG_FILE: &str = "cloudyab.toml";

/// Default page load timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default RAM budget in MB.
const DEFAULT_MAX_RAM_MB: u32 = 500;

/// Default max captcha solve attempts.
const DEFAULT_MAX_CAPTCHA_ATTEMPTS: u32 = 3;

/// Default AI model for browsing agent.
const DEFAULT_AI_MODEL: &str = "gpt-4o-mini";

/// Default max AI browsing steps.
const DEFAULT_AI_MAX_STEPS: u32 = 20;

/// Default HTTP API port for the task queue.
const DEFAULT_HTTP_PORT: u16 = 9222;

/// Top-level configuration for the CloudyAB engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudyAbConfig {
    /// General engine settings.
    pub engine: EngineConfig,
    /// Stealth-HTTP layer configuration.
    pub stealth_http: StealthHttpConfig,
    /// Browser engine configuration.
    pub browser: BrowserLayerConfig,
    /// Captcha solver configuration.
    pub solver: SolverConfig,
    /// Cookie storage configuration.
    pub cookies: CookieConfig,
    /// Proxy configuration (global default).
    pub proxy: Option<ProxyConfig>,
    /// AI browsing agent configuration.
    pub ai: AiConfig,
}

/// General engine settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    /// Default timeout for page loads (seconds).
    pub timeout_secs: u64,
    /// Whether to auto-escalate from HTTP to browser layer on failure.
    pub auto_escalate: bool,
    /// Maximum RAM budget in MB (for monitoring).
    pub max_ram_mb: u32,
    /// Log level (trace, debug, info, warn, error).
    pub log_level: String,
    /// HTTP API port for the task queue (0 = disabled).
    pub http_port: u16,
}

/// Stealth-HTTP layer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StealthHttpConfig {
    /// Whether the stealth-HTTP layer is enabled.
    pub enabled: bool,
    /// User-agent string override (None = use fingerprint profile).
    pub user_agent: Option<String>,
    /// Maximum redirects to follow.
    pub max_redirects: u32,
}

/// Browser engine layer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserLayerConfig {
    /// Whether the browser engine is enabled.
    pub enabled: bool,
    /// Path to the browser binary (Obscura or stealth-patched Chromium).
    pub binary_path: Option<String>,
    /// Whether to run headless.
    pub headless: bool,
    /// Viewport width.
    pub viewport_width: u32,
    /// Viewport height.
    pub viewport_height: u32,
    /// Whether to disable GPU.
    pub disable_gpu: bool,
    /// Extra CLI arguments for the browser process.
    pub extra_args: Vec<String>,
}

/// Captcha solver configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SolverConfig {
    /// Whether captcha solving is enabled at all.
    pub enabled: bool,
    /// Whether to use local ONNX models.
    pub use_local: bool,
    /// Path to ONNX model directory.
    pub models_dir: PathBuf,
    /// Whether to fall back to cloud API if local fails.
    pub use_cloud_fallback: bool,
    /// Cloud solver API endpoint.
    pub cloud_api_url: Option<String>,
    /// Cloud solver API key.
    pub cloud_api_key: Option<String>,
    /// Maximum attempts per captcha.
    pub max_attempts: u32,
}

/// Cookie storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CookieConfig {
    /// Whether cookie persistence is enabled.
    pub enabled: bool,
    /// Path to SQLite cookie database.
    pub db_path: PathBuf,
}

/// AI browsing agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// Whether AI-powered browsing is enabled.
    pub enabled: bool,
    /// LLM provider: "openai", "anthropic", "local".
    pub provider: String,
    /// API key for the LLM provider.
    pub api_key: Option<String>,
    /// Model name (e.g., "gpt-4o-mini", "claude-sonnet-4-20250514").
    pub model: String,
    /// API base URL override (for local/custom endpoints).
    pub api_base_url: Option<String>,
    /// Maximum steps the AI agent can take per browse request.
    pub max_steps: u32,
}

// ─── Defaults ───────────────────────────────────────────────────────────

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            auto_escalate: true,
            max_ram_mb: DEFAULT_MAX_RAM_MB,
            log_level: "info".to_string(),
            http_port: DEFAULT_HTTP_PORT,
        }
    }
}

impl Default for StealthHttpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            user_agent: None,
            max_redirects: 10,
        }
    }
}

impl Default for BrowserLayerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            binary_path: None,
            headless: true,
            viewport_width: 1920,
            viewport_height: 1080,
            disable_gpu: true,
            extra_args: Vec::new(),
        }
    }
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            use_local: true,
            models_dir: PathBuf::from("models"),
            use_cloud_fallback: false,
            cloud_api_url: None,
            cloud_api_key: None,
            max_attempts: DEFAULT_MAX_CAPTCHA_ATTEMPTS,
        }
    }
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            db_path: PathBuf::from("data/cookies.db"),
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "openai".to_string(),
            api_key: None,
            model: DEFAULT_AI_MODEL.to_string(),
            api_base_url: None,
            max_steps: DEFAULT_AI_MAX_STEPS,
        }
    }
}

// ─── Loading ────────────────────────────────────────────────────────────

impl CloudyAbConfig {
    /// Load configuration from the default file path or CLOUDYAB_CONFIG env var.
    /// Falls back to defaults if no config file is found.
    pub fn load() -> Result<Self, ConfigError> {
        let path = resolve_config_path();
        if path.exists() {
            Self::load_from_file(&path)
        } else {
            info!("No config file found at {}, using defaults", path.display());
            Ok(Self::default())
        }
    }

    /// Load configuration from a specific file path.
    pub fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        info!(path = %path.display(), "Loading configuration");
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        let config: Self = toml::from_str(&content).map_err(|e| ConfigError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        Ok(config)
    }

    /// Generate a default config file as a TOML string (for `--init` flag).
    pub fn generate_default_toml() -> String {
        toml::to_string_pretty(&Self::default()).unwrap_or_default()
    }
}

/// Resolve the config file path from env var or default.
fn resolve_config_path() -> PathBuf {
    std::env::var(CONFIG_ENV_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_FILE))
}

/// Errors from configuration loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file '{path}': {message}")]
    Io { path: PathBuf, message: String },

    #[error("Failed to parse config file '{path}': {message}")]
    Parse { path: PathBuf, message: String },
}
