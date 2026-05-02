//! CloudyAB MCP Server
//!
//! Exposes the CloudyAB headless browser engine as an MCP (Model Context Protocol)
//! server over stdio, allowing AI agents to navigate, interact with, and extract
//! data from websites protected by Cloudflare, AWS WAF, and other anti-bot systems.

use std::sync::Arc;

use anyhow::Result;
use cloudyab_core::config::CloudyAbConfig;
use cloudyab_core::orchestrator::Orchestrator;
use cloudyab_stealth_http::engine::StealthEngine;
use cloudyab_types::fingerprint::{
    FingerprintProfile, NavigatorProfile, OsProfile, ScreenProfile, TlsProfile, WebGlProfile,
};
use rmcp::ServiceExt;
use tokio::sync::RwLock;
use tracing_subscriber::{fmt, EnvFilter};

mod tools;

use tools::CloudyAbServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging to stderr (stdout is for MCP protocol)
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .json()
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("CloudyAB MCP server starting");

    // Load configuration (defaults for now)
    let config = CloudyAbConfig::default();

    // Create the orchestrator
    let mut orchestrator = Orchestrator::new(config);

    // Create and register the stealth-HTTP engine with a default fingerprint
    let fingerprint = default_fingerprint();
    let stealth_engine = StealthEngine::new(fingerprint)
        .map_err(|e| anyhow::anyhow!("Failed to create stealth engine: {e}"))?;
    orchestrator.set_stealth_engine(Arc::new(stealth_engine));

    // Browser engine will be registered when Obscura integration is ready
    tracing::info!("Stealth-HTTP engine registered (browser engine pending)");

    // Wrap orchestrator for shared access
    let orchestrator = Arc::new(RwLock::new(orchestrator));

    // Create the MCP server
    let server = CloudyAbServer::new(orchestrator);

    // Run on stdio transport
    tracing::info!("Starting MCP server on stdio transport");
    let transport = rmcp::transport::io::stdio();
    let server_handle = server.serve(transport).await?;
    server_handle.waiting().await?;

    Ok(())
}

/// Default browser fingerprint (Chrome 125 on Windows 11).
fn default_fingerprint() -> FingerprintProfile {
    FingerprintProfile {
        name: "windows_chrome_125".into(),
        os: OsProfile {
            platform: "Windows".into(),
            os_cpu: "Windows NT 10.0; Win64; x64".into(),
            architecture: "x86_64".into(),
        },
        tls: TlsProfile {
            cipher_suites: vec![
                "TLS_AES_128_GCM_SHA256".into(),
                "TLS_AES_256_GCM_SHA384".into(),
                "TLS_CHACHA20_POLY1305_SHA256".into(),
                "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256".into(),
                "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256".into(),
            ],
            extensions: vec![
                "server_name".into(),
                "supported_groups".into(),
                "key_share".into(),
                "supported_versions".into(),
            ],
            elliptic_curves: vec!["X25519".into(), "P-256".into(), "P-384".into()],
            ec_point_formats: vec!["uncompressed".into()],
        },
        navigator: NavigatorProfile {
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36".into(),
            platform: "Win32".into(),
            language: "en-US".into(),
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 8,
            vendor: "Google Inc.".into(),
            max_touch_points: 0,
        },
        screen: ScreenProfile {
            width: 1920,
            height: 1080,
            avail_width: 1920,
            avail_height: 1040,
            color_depth: 24,
            pixel_ratio: 1.0,
        },
        webgl: WebGlProfile {
            vendor: "Google Inc. (NVIDIA)".into(),
            renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)".into(),
        },
    }
}
