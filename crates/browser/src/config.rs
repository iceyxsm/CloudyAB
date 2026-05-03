//! Browser engine configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default browser binary name.
const DEFAULT_BROWSER_BINARY: &str = "obscura";

/// Default viewport width.
const DEFAULT_VIEWPORT_WIDTH: u32 = 1920;

/// Default viewport height.
const DEFAULT_VIEWPORT_HEIGHT: u32 = 1080;

/// Default page load timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default CDP port for Obscura serve mode.
const DEFAULT_CDP_PORT: u16 = 9223;

/// Configuration for the browser engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// Path to the Obscura binary.
    pub binary_path: PathBuf,
    /// Additional CLI arguments passed to the browser process.
    pub extra_args: Vec<String>,
    /// Viewport width in pixels.
    pub viewport_width: u32,
    /// Viewport height in pixels.
    pub viewport_height: u32,
    /// Whether to run headless (no visible window).
    pub headless: bool,
    /// Page load timeout in seconds.
    pub timeout_secs: u64,
    /// User data directory (None = temp dir per session).
    pub user_data_dir: Option<PathBuf>,
    /// Proxy URL (overrides system proxy).
    pub proxy_url: Option<String>,
    /// Whether to disable GPU acceleration.
    pub disable_gpu: bool,
    /// CDP port for Obscura serve mode.
    pub cdp_port: u16,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from(DEFAULT_BROWSER_BINARY),
            extra_args: Vec::new(),
            viewport_width: DEFAULT_VIEWPORT_WIDTH,
            viewport_height: DEFAULT_VIEWPORT_HEIGHT,
            headless: true,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            user_data_dir: None,
            proxy_url: None,
            disable_gpu: true,
            cdp_port: DEFAULT_CDP_PORT,
        }
    }
}

impl BrowserConfig {
    /// Resolve the binary path from environment or default.
    pub fn resolve_binary(&self) -> PathBuf {
        if let Ok(env_path) = std::env::var("CLOUDYAB_BROWSER_BIN") {
            return PathBuf::from(env_path);
        }
        self.binary_path.clone()
    }

    /// Build CLI arguments for Obscura serve mode.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            "serve".to_string(),
            "--port".to_string(),
            self.cdp_port.to_string(),
            "--stealth".to_string(),
        ];

        if let Some(ref proxy) = self.proxy_url {
            args.push("--proxy".to_string());
            args.push(proxy.clone());
        }

        args.extend(self.extra_args.clone());
        args
    }

    /// Get the CDP WebSocket endpoint URL.
    pub fn cdp_ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/devtools/browser", self.cdp_port)
    }
}
