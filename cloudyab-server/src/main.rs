//! CloudyAB MCP Server
//!
//! Exposes the CloudyAB headless browser engine as an MCP (Model Context Protocol)
//! server over stdio, allowing AI agents to navigate, interact with, and extract
//! data from websites protected by Cloudflare, AWS WAF, and other anti-bot systems.
//!
//! Configuration is loaded from `cloudyab.toml` (or `CLOUDYAB_CONFIG` env var).
//! Run with `--init` to generate a default config file.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use cloudyab_browser::config::BrowserConfig;
use cloudyab_browser::engine::BrowserEngine;
use cloudyab_core::config::CloudyAbConfig;
use cloudyab_core::orchestrator::Orchestrator;
use cloudyab_stealth_http::engine::StealthEngine;
use rmcp::ServiceExt;
use tokio::sync::RwLock;
use tracing_subscriber::{fmt, EnvFilter};

mod ai_browse;
mod cookie_adapter;
mod human_submitter;
#[allow(unused)]
mod interaction;
mod profiles;
mod solver_adapter;
mod task_queue;
mod task_store;
mod tools;

use tools::CloudyAbServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Handle --init flag to generate default config
    if std::env::args().any(|a| a == "--init") {
        let toml_str = CloudyAbConfig::generate_default_toml();
        std::fs::write("cloudyab.toml", &toml_str)?;
        println!("Generated cloudyab.toml with default configuration");
        return Ok(());
    }

    // Load configuration from file (or defaults)
    let config = CloudyAbConfig::load().map_err(|e| anyhow::anyhow!("Configuration error: {e}"))?;

    // Initialize structured logging to stderr (stdout is for MCP protocol)
    let log_level = config.engine.log_level.clone();
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level)),
        )
        .with_target(false)
        .json()
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("CloudyAB MCP server starting");

    // Create the orchestrator
    let mut orchestrator = Orchestrator::new(config.clone());

    // Load fingerprint profile (from profiles dir or built-in default)
    let profiles_dir = PathBuf::from("profiles");
    let fingerprint = profiles::load_random_profile(&profiles_dir);

    // Conditionally register the stealth-HTTP engine
    if config.stealth_http.enabled {
        let stealth_engine = StealthEngine::new(fingerprint.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create stealth engine: {e}"))?;
        orchestrator.set_stealth_engine(Arc::new(stealth_engine));
        tracing::info!("Stealth-HTTP engine registered");

        // Conditionally register the browser engine
        if config.browser.enabled {
            let browser_config = BrowserConfig {
                binary_path: config
                    .browser
                    .binary_path
                    .as_ref()
                    .map(PathBuf::from)
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

        // Register human-like solution submitter for natural interaction
        orchestrator.set_solution_submitter(Arc::new(human_submitter::HumanSubmitter));
        tracing::info!("Human-like solution submitter registered");
    }

    // Conditionally register cookie persistence
    if config.cookies.enabled {
        match cookie_adapter::CookieStoreAdapter::open(&config.cookies.db_path) {
            Ok(store) => {
                orchestrator.set_cookie_store(Arc::new(store));
                tracing::info!("Cookie persistence registered");
            }
            Err(e) => {
                tracing::warn!("Cookie persistence unavailable: {e}");
            }
        }
    }

    // Wrap orchestrator for shared access
    let orchestrator = Arc::new(RwLock::new(orchestrator));
    let config = Arc::new(config);

    // Start the HTTP task queue server in the background
    let orch_for_http = orchestrator.clone();
    let config_for_http = config.clone();
    tokio::spawn(task_queue::start_http_server(
        orch_for_http,
        config_for_http,
    ));

    // Create the MCP server
    let server = CloudyAbServer::new(orchestrator, config);

    // Run on stdio transport with graceful shutdown on Ctrl+C
    tracing::info!("Starting MCP server on stdio transport");
    let transport = rmcp::transport::io::stdio();
    let server_handle = server.serve(transport).await?;

    tokio::select! {
        result = server_handle.waiting() => {
            result?;
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received shutdown signal, exiting gracefully");
        }
    }

    Ok(())
}
