//! CloudyAB MCP Server
//!
//! Exposes the CloudyAB headless browser engine as an MCP (Model Context Protocol)
//! server over stdio, allowing AI agents to navigate, interact with, and extract
//! data from websites protected by Cloudflare, AWS WAF, and other anti-bot systems.
//!
//! Configuration is loaded from `cloudyab.toml` (or `CLOUDYAB_CONFIG` env var).
//! Run with `--init` to generate a default config file.

use std::sync::Arc;

use anyhow::Result;
use cloudyab_browser::config::BrowserConfig;
use cloudyab_browser::engine::BrowserEngine;
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
mod ai_browse;
mod task_queue;
mod solver_adapter;

use tools::CloudyAbServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Handle --init flag to generate default config
    if std::env::args().any(|a| a == "--init") {
        let toml = CloudyAbConfig::generate_default_toml();
        std::fs::write("cloudyab.toml", &toml)?;
        println!("Generated cloudyab.toml with default configuration");
        return Ok(());
    }

    // Load configuration from file (or defaults)
    let config = CloudyAbConfig::load()
        .map_err(|e| anyhow::anyhow!("Configuration error: {e}"))?;

    // Initialize structured logging to stderr (stdout is for MCP protocol)
    let log_level = config.engine.log_level.clone();
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&log_level)),
        )
        .with_target(false)
        .json()
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("CloudyAB MCP server starting");

    // Create the orchestrator
    let mut orchestrator = Orchestrator::new(config.clone());

    // Conditionally register the stealth-HTTP engine
    if config.stealth_http.enabled {
        let fingerprint = default_fingerprint();
        let stealth_engine = StealthEngine::new(fingerprint.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create stealth engine: {e}"))?;
        orchestrator.set_stealth_engine(Arc::new(stealth_engine));
        tracing::info!("Stealth-HTTP engine registered");

        // Conditionally register the browser engine
        if config.browser.enabled {
            let browser_config = BrowserConfig {
                binary_path: config.browser.binary_path
                    .as_ref()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default(),
                headless: config.browser.headless,
                viewport_width: config.browser.viewport_width,
                viewport_height: config.browser.viewport_height,
                disable_gpu: config.browser.disable_gpu,
                extra_args: config.browser.extra_args.clone(),
                timeout_secs: config.engine.timeout_secs,
                user_data_dir: None,
                proxy_url: config.proxy.as_ref().map(|p| p.url.clone()),
            };

            match BrowserEngine::launch(browser_config, &fingerprint).await {
                Ok(browser_engine) => {
                    orchestrator.set_browser_engine(Arc::new(browser_engine));
                    tracing::info!("Browser engine registered");
                }
                Err(e) => {
                    tracing::warn!("Browser engine unavailable: {e}");
                }
            }
        } else {
            tracing::info!("Browser engine disabled by config");
        }
    } else {
        tracing::info!("Stealth-HTTP engine disabled by config");
    }

    // Conditionally register the captcha solver
    if config.solver.enabled {
        let adapter = solver_adapter::SolverRegistryAdapter::new(&config.solver.models_dir);
        orchestrator.set_solver(Arc::new(adapter));
        tracing::info!("Captcha solver registered");
    } else {
        tracing::info!("Captcha solver disabled by config");
    }

    // Wrap orchestrator for shared access
    let orchestrator = Arc::new(RwLock::new(orchestrator));
    let config = Arc::new(config);

    // Start the HTTP task queue server in the background
    let orch_for_http = orchestrator.clone();
    let config_for_http = config.clone();
    tokio::spawn(task_queue::start_http_server(orch_for_http, config_for_http));

    // Create the MCP server
    let server = CloudyAbServer::new(orchestrator, config);

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
