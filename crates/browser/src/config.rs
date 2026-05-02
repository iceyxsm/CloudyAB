//! Browser engine configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default browser binary name (stealth-patched Chromium).
const DEFAULT_BROWSER_BINARY: &str = "obscura";

/// Default viewport width.
const DEFAULT_VIEWPORT_WIDTH: u32 = 1920;

/// Default viewport height.
const DEFAULT_VIEWPORT_HEIGHT: u32 = 1080;

/// Default page load timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Configuration for the browser engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// Path to the browser binary (stealth-patched Chromium/Obscura).
    /// If not absolute, searched in PATH.
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

    /// Build the full set of CLI arguments for the browser process.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            // Core stealth flags
            "--disable-blink-features=AutomationControlled".to_string(),
            "--disable-features=IsolateOrigins,site-per-process".to_string(),
            "--disable-infobars".to_string(),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            // Performance
            "--disable-background-networking".to_string(),
            "--disable-sync".to_string(),
            "--disable-translate".to_string(),
            "--metrics-recording-only".to_string(),
            "--mute-audio".to_string(),
            // Viewport
            format!("--window-size={},{}", self.viewport_width, self.viewport_height),
        ];

        if self.disable_gpu {
            args.push("--disable-gpu".to_string());
            args.push("--disable-software-rasterizer".to_string());
        }

        if let Some(ref proxy) = self.proxy_url {
            args.push(format!("--proxy-server={proxy}"));
        }

        if let Some(ref data_dir) = self.user_data_dir {
            args.push(format!("--user-data-dir={}", data_dir.display()));
        }

        args.extend(self.extra_args.clone());
        args
    }
}
