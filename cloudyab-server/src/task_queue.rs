//! Async task queue with webhook callbacks.
//!
//! Provides an HTTP API for submitting long-running browsing tasks that
//! execute in the background and optionally call a webhook on completion.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use cloudyab_core::config::CloudyAbConfig;
use cloudyab_core::orchestrator::Orchestrator;
use cloudyab_types::session::{Layer, SessionConfig};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// Default HTTP port for the task queue API.
const DEFAULT_HTTP_PORT: u16 = 9222;

/// Task status values.
const STATUS_PENDING: &str = "pending";
const STATUS_RUNNING: &str = "running";
const STATUS_COMPLETED: &str = "completed";
const STATUS_FAILED: &str = "failed";

/// Shared application state for the HTTP API.
#[derive(Clone)]
pub struct AppState {
    orchestrator: Arc<RwLock<Orchestrator>>,
    tasks: Arc<RwLock<HashMap<String, TaskEntry>>>,
    config: Arc<CloudyAbConfig>,
    http_client: Client,
}

/// A task entry stored in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEntry {
    pub id: String,
    pub status: String,
    pub request: TaskRequest,
    pub result: Option<TaskResult>,
    pub error: Option<String>,
}

/// Request body for creating a new task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    /// URL to navigate to.
    pub url: String,
    /// Preferred layer: "http", "browser", or null for auto.
    pub layer: Option<String>,
    /// Whether to return a page snapshot in the result.
    pub snapshot: Option<bool>,
    /// Whether to return cookies in the result.
    pub cookies: Option<bool>,
    /// Webhook URL to POST the result to on completion.
    pub webhook_url: Option<String>,
}

/// Result of a completed task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub final_url: String,
    pub status_code: u16,
    pub layer_used: String,
    pub captcha_solved: bool,
    pub snapshot: Option<String>,
    pub cookies: Option<serde_json::Value>,
}

/// Response when a task is created.
#[derive(Serialize)]
struct CreateTaskResponse {
    task_id: String,
    status: String,
}

/// Build the axum router for the task queue HTTP API.
pub fn build_router(
    orchestrator: Arc<RwLock<Orchestrator>>,
    config: Arc<CloudyAbConfig>,
) -> Router {
    let state = AppState {
        orchestrator,
        tasks: Arc::new(RwLock::new(HashMap::new())),
        config,
        http_client: Client::new(),
    };

    Router::new()
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .route("/health", get(health_check))
        .with_state(state)
}

/// Start the HTTP task queue server on the configured port.
pub async fn start_http_server(
    orchestrator: Arc<RwLock<Orchestrator>>,
    config: Arc<CloudyAbConfig>,
) {
    let router = build_router(orchestrator, config);
    let addr = format!("0.0.0.0:{DEFAULT_HTTP_PORT}");
    info!(addr = %addr, "Starting HTTP task queue server");

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            warn!(error = %e, "Failed to bind HTTP server, task queue disabled");
            return;
        }
    };

    if let Err(e) = axum::serve(listener, router).await {
        warn!(error = %e, "HTTP server stopped");
    }
}

/// POST /tasks — create a new background navigation task.
async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<TaskRequest>,
) -> (StatusCode, Json<CreateTaskResponse>) {
    let task_id = Uuid::new_v4().to_string();

    let entry = TaskEntry {
        id: task_id.clone(),
        status: STATUS_PENDING.to_string(),
        request: request.clone(),
        result: None,
        error: None,
    };

    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task_id.clone(), entry);
    }

    // Spawn background execution
    let task_id_clone = task_id.clone();
    tokio::spawn(execute_task(state, task_id_clone, request));

    (
        StatusCode::ACCEPTED,
        Json(CreateTaskResponse {
            task_id,
            status: STATUS_PENDING.to_string(),
        }),
    )
}

/// GET /tasks/:id — get the status and result of a task.
async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TaskEntry>, StatusCode> {
    let tasks = state.tasks.read().await;
    tasks
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// GET /health — simple health check.
async fn health_check() -> &'static str {
    "ok"
}

/// Execute a task in the background and update its status.
async fn execute_task(state: AppState, task_id: String, request: TaskRequest) {
    update_status(&state, &task_id, STATUS_RUNNING).await;

    let result = run_navigation(&state, &request).await;

    match result {
        Ok(task_result) => {
            let mut tasks = state.tasks.write().await;
            if let Some(entry) = tasks.get_mut(&task_id) {
                entry.status = STATUS_COMPLETED.to_string();
                entry.result = Some(task_result);
            }
            drop(tasks);
            fire_webhook(&state, &task_id, &request.webhook_url).await;
        }
        Err(error_msg) => {
            let mut tasks = state.tasks.write().await;
            if let Some(entry) = tasks.get_mut(&task_id) {
                entry.status = STATUS_FAILED.to_string();
                entry.error = Some(error_msg);
            }
            drop(tasks);
            fire_webhook(&state, &task_id, &request.webhook_url).await;
        }
    }
}

/// Run the actual navigation for a task.
async fn run_navigation(state: &AppState, request: &TaskRequest) -> Result<TaskResult, String> {
    let preferred_layer = request.layer.as_deref().map(|l| match l {
        "http" | "stealth" => Layer::StealthHttp,
        _ => Layer::Browser,
    });

    let nav_config = SessionConfig {
        target_url: request.url.clone(),
        fingerprint: None,
        proxy: state.config.proxy.clone(),
        persist_cookies: true,
        timeout_secs: state.config.engine.timeout_secs,
        preferred_layer,
    };

    let orchestrator = state.orchestrator.read().await;
    let nav_result = orchestrator
        .navigate(&nav_config)
        .await
        .map_err(|e| format!("Navigation failed: {e}"))?;

    let snapshot = if request.snapshot.unwrap_or(false) {
        let options = cloudyab_types::page::SnapshotOptions::default();
        orchestrator
            .snapshot(&options)
            .await
            .ok()
            .map(|s| s.tree)
    } else {
        None
    };

    let cookies = if request.cookies.unwrap_or(false) {
        orchestrator
            .get_cookies()
            .await
            .ok()
            .and_then(|jar| serde_json::to_value(&jar.cookies).ok())
    } else {
        None
    };

    let layer_str = match nav_result.layer_used {
        Layer::StealthHttp => "stealth_http",
        Layer::Browser => "browser",
    };

    Ok(TaskResult {
        final_url: nav_result.final_url,
        status_code: nav_result.status_code,
        layer_used: layer_str.to_string(),
        captcha_solved: nav_result.captcha_solved,
        snapshot,
        cookies,
    })
}

/// Update a task's status in the store.
async fn update_status(state: &AppState, task_id: &str, status: &str) {
    let mut tasks = state.tasks.write().await;
    if let Some(entry) = tasks.get_mut(task_id) {
        entry.status = status.to_string();
    }
}

/// Fire a webhook callback if configured.
async fn fire_webhook(state: &AppState, task_id: &str, webhook_url: &Option<String>) {
    let url = match webhook_url {
        Some(u) => u,
        None => return,
    };

    let tasks = state.tasks.read().await;
    let entry = match tasks.get(task_id) {
        Some(e) => e.clone(),
        None => return,
    };
    drop(tasks);

    info!(task_id, url = %url, "Firing webhook callback");
    let resp = state.http_client.post(url).json(&entry).send().await;
    if let Err(e) = resp {
        warn!(task_id, error = %e, "Webhook callback failed");
    }
}
