//! CloudyAB MCP Server
//!
//! Exposes the CloudyAB headless browser engine as an MCP (Model Context Protocol)
//! server over stdio, allowing AI agents to navigate, interact with, and extract
//! data from websites protected by Cloudflare, AWS WAF, and other anti-bot systems.

use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};

mod tools;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .json()
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("CloudyAB MCP server starting");

    // TODO: Initialize MCP server with rmcp
    // 1. Create server with tools
    // 2. Register all tool handlers (navigate, click, fill, snapshot, etc.)
    // 3. Run on stdio transport

    // Placeholder: print server info to stderr
    eprintln!("CloudyAB v{}", env!("CARGO_PKG_VERSION"));
    eprintln!("MCP server ready on stdio");

    // TODO: Replace with actual rmcp server loop
    // let server = ServerBuilder::new("cloudyab", env!("CARGO_PKG_VERSION"))
    //     .with_tool(tools::navigate_tool())
    //     .with_tool(tools::click_tool())
    //     .with_tool(tools::fill_tool())
    //     .with_tool(tools::snapshot_tool())
    //     .with_tool(tools::get_cookies_tool())
    //     .with_tool(tools::set_cookies_tool())
    //     .with_tool(tools::screenshot_tool())
    //     .with_tool(tools::solve_captcha_tool())
    //     .with_tool(tools::mouse_move_tool())
    //     .with_tool(tools::scroll_tool())
    //     .build();
    // server.run_stdio().await?;

    Ok(())
}
