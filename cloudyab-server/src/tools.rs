//! MCP tool definitions for CloudyAB.
//!
//! Each tool corresponds to an action that AI agents can invoke via MCP.

use serde::{Deserialize, Serialize};

/// Input for the `navigate` tool.
#[derive(Debug, Deserialize)]
pub struct NavigateInput {
    /// URL to navigate to
    pub url: String,
    /// Preferred layer: "http" or "browser" (optional, auto-selects if omitted)
    pub layer: Option<String>,
    /// Proxy URL (optional)
    pub proxy: Option<String>,
    /// Timeout in seconds (default: 30)
    pub timeout: Option<u64>,
}

/// Input for the `click` tool.
#[derive(Debug, Deserialize)]
pub struct ClickInput {
    /// Element ref (e.g., "@e1", "@e5")
    pub r#ref: String,
}

/// Input for the `fill` tool.
#[derive(Debug, Deserialize)]
pub struct FillInput {
    /// Element ref (e.g., "@e3")
    pub r#ref: String,
    /// Text to fill
    pub text: String,
    /// Whether to use realistic typing (default: true)
    pub realistic: Option<bool>,
}

/// Input for the `type_text` tool.
#[derive(Debug, Deserialize)]
pub struct TypeInput {
    /// Element ref
    pub r#ref: String,
    /// Text to type character by character
    pub text: String,
    /// Words per minute (default: 40)
    pub wpm: Option<u32>,
}

/// Input for the `snapshot` tool.
#[derive(Debug, Deserialize)]
pub struct SnapshotInput {
    /// Only include interactive elements
    pub interactive: Option<bool>,
    /// Remove empty structural elements
    pub compact: Option<bool>,
    /// Maximum tree depth (0 = unlimited)
    pub max_depth: Option<u32>,
    /// CSS selector to scope the snapshot
    pub selector: Option<String>,
}

/// Input for the `get_cookies` tool.
#[derive(Debug, Deserialize)]
pub struct GetCookiesInput {
    /// Domain to filter cookies (optional, returns all if omitted)
    pub domain: Option<String>,
}

/// Input for the `set_cookies` tool.
#[derive(Debug, Deserialize)]
pub struct SetCookiesInput {
    /// Cookies to set (JSON array)
    pub cookies: Vec<CookieInput>,
}

/// A single cookie to set.
#[derive(Debug, Deserialize)]
pub struct CookieInput {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: Option<String>,
    pub secure: Option<bool>,
    pub http_only: Option<bool>,
}

/// Input for the `solve_captcha` tool.
#[derive(Debug, Deserialize)]
pub struct SolveCaptchaInput {
    /// Type of captcha (optional, auto-detects if omitted)
    pub captcha_type: Option<String>,
    /// Additional context/prompt for the solver
    pub context: Option<String>,
}

/// Input for the `mouse_move` tool.
#[derive(Debug, Deserialize)]
pub struct MouseMoveInput {
    /// Target X coordinate
    pub x: f64,
    /// Target Y coordinate
    pub y: f64,
}

/// Input for the `scroll` tool.
#[derive(Debug, Deserialize)]
pub struct ScrollInput {
    /// Direction: "up" or "down"
    pub direction: String,
    /// Pixels to scroll (default: 300)
    pub pixels: Option<i32>,
}

/// Output format for tool results.
#[derive(Debug, Serialize)]
pub struct ToolOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
