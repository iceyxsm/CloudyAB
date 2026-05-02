//! MCP tool definitions for CloudyAB.
//!
//! Each tool corresponds to an action that AI agents can invoke via MCP.
//! Tools are wired to the core `Orchestrator` which handles layer routing
//! and auto-escalation.

use std::sync::Arc;

use cloudyab_core::config::CloudyAbConfig;
use cloudyab_core::engine::EngineError;
use cloudyab_core::orchestrator::Orchestrator;
use cloudyab_types::{Layer, SessionConfig, SnapshotOptions};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::ai_browse;

/// The CloudyAB MCP server that exposes browser automation tools.
#[derive(Clone)]
pub struct CloudyAbServer {
    tool_router: ToolRouter<Self>,
    orchestrator: Arc<RwLock<Orchestrator>>,
    config: Arc<CloudyAbConfig>,
}

/// Input for the `navigate` tool.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NavigateParams {
    /// Target URL to navigate to
    pub url: String,
    /// Preferred layer: "http" for stealth HTTP or "browser" for full browser. Omit for auto.
    pub layer: Option<String>,
    /// Proxy URL (http://, https://, socks5://)
    pub proxy: Option<String>,
    /// Timeout in seconds for page load (default: 30)
    pub timeout: Option<u64>,
}

/// Input for the `click` tool.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ClickParams {
    /// Element reference ID from the snapshot (e.g., "e1", "e5")
    pub r#ref: String,
}

/// Input for the `fill` tool.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FillParams {
    /// Element reference ID for the input field
    pub r#ref: String,
    /// Text to fill into the input field
    pub text: String,
}

/// Input for the `type_text` tool.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TypeParams {
    /// Element reference ID for the input field
    pub r#ref: String,
    /// Text to type with realistic keystroke timing
    pub text: String,
}

/// Input for the `snapshot` tool.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SnapshotParams {
    /// Only include interactive elements (buttons, links, inputs)
    pub interactive: Option<bool>,
    /// Remove empty structural elements for a compact tree
    pub compact: Option<bool>,
    /// Maximum tree depth (0 = unlimited)
    pub max_depth: Option<u32>,
    /// CSS selector to scope the snapshot to a specific region
    pub selector: Option<String>,
}

/// Input for the `get_cookies` tool.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetCookiesParams {
    /// Domain to filter cookies by (returns all if omitted)
    pub domain: Option<String>,
}

/// Input for the `ai_browse` tool.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AiBrowseParams {
    /// Natural language goal (e.g., "Find the price of iPhone 16 on Amazon")
    pub goal: String,
    /// Starting URL to navigate to before the AI takes over (optional)
    pub url: Option<String>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CloudyAbServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info
    }
}

#[tool_router]
impl CloudyAbServer {
    /// Create a new CloudyAB MCP server.
    pub fn new(orchestrator: Arc<RwLock<Orchestrator>>, config: Arc<CloudyAbConfig>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            orchestrator,
            config,
        }
    }

    /// Navigate to a URL with stealth protection bypass.
    #[tool(
        description = "Navigate to a URL. Automatically bypasses Cloudflare, AWS WAF, and other protections. Returns navigation result with status and protection bypass info."
    )]
    async fn navigate(
        &self,
        params: Parameters<NavigateParams>,
    ) -> Result<CallToolResult, McpError> {
        let params = params.0;
        let preferred_layer = params.layer.as_deref().map(|l| match l {
            "http" | "stealth" => Layer::StealthHttp,
            _ => Layer::Browser,
        });

        let config = SessionConfig {
            target_url: params.url,
            fingerprint: None,
            proxy: params.proxy.map(|url| cloudyab_types::ProxyConfig {
                url,
                username: None,
                password: None,
            }),
            persist_cookies: true,
            timeout_secs: params.timeout.unwrap_or(30),
            preferred_layer,
        };

        let orchestrator = self.orchestrator.read().await;
        let result = orchestrator
            .navigate(&config)
            .await
            .map_err(engine_to_mcp)?;

        let content = Content::json(&result)
            .map_err(|e| McpError::internal_error(format!("Serialization failed: {e}"), None))?;
        Ok(CallToolResult::success(vec![content]))
    }

    /// Get the accessibility tree snapshot of the current page.
    #[tool(
        description = "Get the current page's accessibility tree as a structured snapshot with element references (@eN). Use these refs with click, fill, and type tools."
    )]
    async fn snapshot(
        &self,
        params: Parameters<SnapshotParams>,
    ) -> Result<CallToolResult, McpError> {
        let params = params.0;
        let options = SnapshotOptions {
            interactive_only: params.interactive.unwrap_or(false),
            compact: params.compact.unwrap_or(false),
            max_depth: params.max_depth.unwrap_or(0),
            selector: params.selector,
        };

        let orchestrator = self.orchestrator.read().await;
        let snapshot = orchestrator
            .snapshot(&options)
            .await
            .map_err(engine_to_mcp)?;

        let mut text = format!("Page: {} ({})\n\n", snapshot.title, snapshot.url);
        text.push_str(&snapshot.tree);

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Click an element by its reference ID.
    #[tool(
        description = "Click an element on the page by its @eN reference from the snapshot. Automatically escalates to browser if needed."
    )]
    async fn click(&self, params: Parameters<ClickParams>) -> Result<CallToolResult, McpError> {
        let params = params.0;
        let orchestrator = self.orchestrator.read().await;
        orchestrator
            .click(&params.r#ref)
            .await
            .map_err(engine_to_mcp)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Clicked element @{}",
            params.r#ref
        ))]))
    }

    /// Fill a text input by its reference ID.
    #[tool(
        description = "Fill text into an input field by its @eN reference. Clears existing content first. Automatically escalates to browser if needed."
    )]
    async fn fill(&self, params: Parameters<FillParams>) -> Result<CallToolResult, McpError> {
        let params = params.0;
        let orchestrator = self.orchestrator.read().await;
        orchestrator
            .fill(&params.r#ref, &params.text)
            .await
            .map_err(engine_to_mcp)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Filled @{} with text",
            params.r#ref
        ))]))
    }

    /// Type text with realistic keystroke timing.
    #[tool(
        description = "Type text character-by-character with realistic human-like timing into an input field by its @eN reference."
    )]
    async fn type_text(&self, params: Parameters<TypeParams>) -> Result<CallToolResult, McpError> {
        let params = params.0;
        let orchestrator = self.orchestrator.read().await;
        orchestrator
            .type_text(&params.r#ref, &params.text)
            .await
            .map_err(engine_to_mcp)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Typed into @{}",
            params.r#ref
        ))]))
    }

    /// Take a screenshot of the current page.
    #[tool(
        description = "Take a PNG screenshot of the current page. Returns base64-encoded PNG data. Requires browser engine (auto-escalates from HTTP layer if needed)."
    )]
    async fn screenshot(&self) -> Result<CallToolResult, McpError> {
        let orchestrator = self.orchestrator.read().await;
        let png_bytes = orchestrator.screenshot().await.map_err(engine_to_mcp)?;

        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{b64}");

        Ok(CallToolResult::success(vec![Content::text(data_uri)]))
    }

    /// Get cookies from the current session.
    #[tool(
        description = "Get all cookies from the current browsing session. Returns cookies as JSON with name, value, domain, path, and flags."
    )]
    async fn get_cookies(
        &self,
        params: Parameters<GetCookiesParams>,
    ) -> Result<CallToolResult, McpError> {
        let params = params.0;
        let orchestrator = self.orchestrator.read().await;
        let jar = orchestrator.get_cookies().await.map_err(engine_to_mcp)?;

        let cookies = if let Some(domain) = &params.domain {
            jar.cookies
                .into_iter()
                .filter(|c| c.domain.contains(domain.as_str()))
                .collect::<Vec<_>>()
        } else {
            jar.cookies
        };

        let content = Content::json(&cookies)
            .map_err(|e| McpError::internal_error(format!("Serialization failed: {e}"), None))?;
        Ok(CallToolResult::success(vec![content]))
    }

    /// AI-powered autonomous browsing to accomplish a goal.
    #[tool(
        description = "Use AI to autonomously browse the web and extract information. Provide a natural language goal and optionally a starting URL. The AI agent will navigate, click, fill forms, and extract data to accomplish the goal. Requires [ai] section configured in cloudyab.toml with an API key."
    )]
    async fn ai_browse_tool(
        &self,
        params: Parameters<AiBrowseParams>,
    ) -> Result<CallToolResult, McpError> {
        let params = params.0;

        if !self.config.ai.enabled {
            return Err(McpError::internal_error(
                "AI browsing is disabled. Enable it in [ai] section of cloudyab.toml".to_string(),
                None,
            ));
        }

        let result = ai_browse::ai_browse(
            &self.orchestrator,
            &self.config.ai,
            &params.goal,
            params.url.as_deref(),
        )
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = Content::json(&result)
            .map_err(|e| McpError::internal_error(format!("Serialization failed: {e}"), None))?;
        Ok(CallToolResult::success(vec![content]))
    }
}

/// Convert an EngineError into an MCP error response.
fn engine_to_mcp(err: EngineError) -> McpError {
    McpError::internal_error(err.to_string(), None)
}
