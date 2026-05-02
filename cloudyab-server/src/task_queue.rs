//! Async task queue with webhook callbacks, retry logic, and concurrency control.
//!
//! Provides an HTTP API for submitting long-running browsing tasks that
//! execute in the background and optionally call a webhook on completion.
//! Features:
//! - Configurable concurrency limits
//! - Automatic retry with exponential backoff
//! - Task cancellation
//! - Webhook delivery with retry
//! - Task TTL and automatic cleanup

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{delete, get, post};
use axum::Router;
use cloudyab_core::config::CloudyAbConfig;
use cloudyab_core::orchestrator::Orchestrator;
use cloudyab_types::session::{Layer, SessionConfig};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, Semaphore};
use tracing::{info, warn};
use uuid::Uuid;

/// Task status values.
const STATUS_PENDING: &str = "pending";
const STATUS_RUNNING: &str = "running";
const STATUS_COMPLETED: &str = "completed";
const STATUS_FAILED: &str = "failed";
const STATUS_CANCELLED: &str = "cancelled";
const STATUS_RETRYING: &str = "retrying";

/// Default max concurrent tasks.
const DEFAULT_MAX_CONCURRENT: usize = 5;

/// Default max retries per task.
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Default webhook retry attempts.
const WEBHOOK_MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff (ms).
const BACKOFF_BASE_MS: u64 = 1000;

/// Task TTL before cleanup (seconds).
const TASK_TTL_SECS: u64 = 3600;

/// Shared application state for the HTTP API.
#[derive(Clone)]
pub struct AppState {
    orchestrator: Arc<RwLock<Orchestrator>>,
    tasks: Arc<RwLock<HashMap<String, TaskEntry>>>,
    config: Arc<CloudyAbConfig>,
    http_client: Client,
    semaphore: Arc<Semaphore>,
}

/// A task entry stored in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEntry {
    pub id: String,
    pub status: String,
    pub request: TaskRequest,
    pub result: Option<TaskResult>,
    pub error: Option<String>,
    pub attempts: u32,
    pub max_retries: u32,
    #[serde(with = "instant_serde")]
    pub created_at: Instant,
    #[serde(skip_serializing_if = "Option::is_none", with = "option_instant_serde")]
    pub completed_at: Option<Instant>,
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
    /// Maximum retries for this task (overrides default).
    pub max_retries: Option<u32>,
    /// Priority (lower = higher priority, default 5).
    pub priority: Option<u32>,
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
    pub duration_ms: u64,
}

/// Response when a task is created.
#[derive(Serialize)]
struct CreateTaskResponse {
    task_id: String,
    status: String,
    position: usize,
}

/// Query params for listing tasks.
#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
}

/// Response for listing tasks.
#[derive(Serialize)]
struct ListTasksResponse {
    tasks: Vec<TaskSummary>,
    total: usize,
}

/// Summary of a task for list endpoint.
#[derive(Serialize)]
struct TaskSummary {
    id: String,
    status: String,
    url: String,
    attempts: u32,
}

/// Queue stats response.
#[derive(Serialize)]
struct QueueStats {
    total_tasks: usize,
    pending: usize,
    running: usize,
    completed: usize,
    failed: usize,
    cancelled: usize,
    max_concurrent: usize,
}

/// Build the axum router for the task queue HTTP API.
pub fn build_router(
    orchestrator: Arc<RwLock<Orchestrator>>,
    config: Arc<CloudyAbConfig>,
) -> Router {
    let max_concurrent = DEFAULT_MAX_CONCURRENT;

    let state = AppState {
        orchestrator,
        tasks: Arc::new(RwLock::new(HashMap::new())),
        config,
        http_client: Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new()),
        semaphore: Arc::new(Semaphore::new(max_concurrent)),
    };

    // Spawn background cleanup task
    let cleanup_state = state.clone();
    tokio::spawn(cleanup_expired_tasks(cleanup_state));

    Router::new()
        .route("/tasks", post(create_task))
        .route("/tasks", get(list_tasks))
        .route("/tasks/{id}", get(get_task))
        .route("/tasks/{id}", delete(cancel_task))
        .route("/tasks/{id}/retry", post(retry_task))
        .route("/queue/stats", get(queue_stats))
        .route("/health", get(health_check))
        .with_state(state)
}

/// Start the HTTP task queue server on the configured port.
pub async fn start_http_server(
    orchestrator: Arc<RwLock<Orchestrator>>,
    config: Arc<CloudyAbConfig>,
) {
    let port = config.engine.http_port;
    if port == 0 {
        info!("HTTP task queue disabled (port = 0)");
        return;
    }

    let router = build_router(orchestrator, config);
    let addr = format!("0.0.0.0:{port}");
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
    let max_retries = request.max_retries.unwrap_or(DEFAULT_MAX_RETRIES);

    let entry = TaskEntry {
        id: task_id.clone(),
        status: STATUS_PENDING.to_string(),
        request: request.clone(),
        result: None,
        error: None,
        attempts: 0,
        max_retries,
        created_at: Instant::now(),
        completed_at: None,
    };

    let position = {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task_id.clone(), entry);
        tasks
            .values()
            .filter(|t| t.status == STATUS_PENDING || t.status == STATUS_RUNNING)
            .count()
    };

    // Spawn background execution with concurrency control
    let task_id_clone = task_id.clone();
    let state_clone = state.clone();
    tokio::spawn(async move {
        execute_with_retry(state_clone, task_id_clone, request, max_retries).await;
    });

    (
        StatusCode::ACCEPTED,
        Json(CreateTaskResponse {
            task_id,
            status: STATUS_PENDING.to_string(),
            position,
        }),
    )
}

/// GET /tasks — list all tasks with optional status filter.
async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<ListTasksQuery>,
) -> Json<ListTasksResponse> {
    let tasks = state.tasks.read().await;
    let limit = query.limit.unwrap_or(100);

    let filtered: Vec<TaskSummary> = tasks
        .values()
        .filter(|t| query.status.as_ref().map_or(true, |s| &t.status == s))
        .take(limit)
        .map(|t| TaskSummary {
            id: t.id.clone(),
            status: t.status.clone(),
            url: t.request.url.clone(),
            attempts: t.attempts,
        })
        .collect();

    let total = filtered.len();
    Json(ListTasksResponse {
        tasks: filtered,
        total,
    })
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

/// DELETE /tasks/:id — cancel a pending or running task.
async fn cancel_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TaskEntry>, StatusCode> {
    let mut tasks = state.tasks.write().await;
    match tasks.get_mut(&id) {
        Some(entry) if entry.status == STATUS_PENDING || entry.status == STATUS_RUNNING => {
            entry.status = STATUS_CANCELLED.to_string();
            entry.completed_at = Some(Instant::now());
            Ok(Json(entry.clone()))
        }
        Some(_) => Err(StatusCode::CONFLICT),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /tasks/:id/retry — manually retry a failed task.
async fn retry_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<TaskEntry>), StatusCode> {
    let request = {
        let mut tasks = state.tasks.write().await;
        match tasks.get_mut(&id) {
            Some(entry) if entry.status == STATUS_FAILED || entry.status == STATUS_CANCELLED => {
                entry.status = STATUS_PENDING.to_string();
                entry.error = None;
                entry.result = None;
                entry.completed_at = None;
                let req = entry.request.clone();
                let max_retries = entry.max_retries;
                (req, max_retries)
            }
            Some(_) => return Err(StatusCode::CONFLICT),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    let state_clone = state.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        execute_with_retry(state_clone, id_clone, request.0, request.1).await;
    });

    let tasks = state.tasks.read().await;
    let entry = tasks.get(&id).cloned().ok_or(StatusCode::NOT_FOUND)?;
    Ok((StatusCode::ACCEPTED, Json(entry)))
}

/// GET /queue/stats — get queue statistics.
async fn queue_stats(State(state): State<AppState>) -> Json<QueueStats> {
    let tasks = state.tasks.read().await;
    let total = tasks.len();
    let pending = tasks.values().filter(|t| t.status == STATUS_PENDING).count();
    let running = tasks.values().filter(|t| t.status == STATUS_RUNNING).count();
    let completed = tasks
        .values()
        .filter(|t| t.status == STATUS_COMPLETED)
        .count();
    let failed = tasks.values().filter(|t| t.status == STATUS_FAILED).count();
    let cancelled = tasks
        .values()
        .filter(|t| t.status == STATUS_CANCELLED)
        .count();

    Json(QueueStats {
        total_tasks: total,
        pending,
        running,
        completed,
        failed,
        cancelled,
        max_concurrent: DEFAULT_MAX_CONCURRENT,
    })
}

/// GET /health — simple health check.
async fn health_check() -> &'static str {
    "ok"
}

/// Execute a task with retry logic and concurrency control.
async fn execute_with_retry(
    state: AppState,
    task_id: String,
    request: TaskRequest,
    max_retries: u32,
) {
    // Acquire semaphore permit for concurrency control
    let _permit = state.semaphore.acquire().await;

    for attempt in 0..=max_retries {
        // Check if task was cancelled
        {
            let tasks = state.tasks.read().await;
            if let Some(entry) = tasks.get(&task_id) {
                if entry.status == STATUS_CANCELLED {
                    info!(task_id = %task_id, "Task cancelled, stopping execution");
                    return;
                }
            }
        }

        // Update status
        if attempt == 0 {
            update_task(&state, &task_id, |entry| {
                entry.status = STATUS_RUNNING.to_string();
                entry.attempts = 1;
            })
            .await;
        } else {
            update_task(&state, &task_id, |entry| {
                entry.status = STATUS_RETRYING.to_string();
                entry.attempts = attempt + 1;
            })
            .await;

            // Exponential backoff before retry
            let delay = Duration::from_millis(BACKOFF_BASE_MS * 2u64.pow(attempt - 1));
            info!(
                task_id = %task_id,
                attempt = attempt + 1,
                delay_ms = delay.as_millis() as u64,
                "Retrying task after backoff"
            );
            tokio::time::sleep(delay).await;
        }

        let start = Instant::now();
        let result = run_navigation(&state, &request).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(mut task_result) => {
                task_result.duration_ms = duration_ms;
                update_task(&state, &task_id, |entry| {
                    entry.status = STATUS_COMPLETED.to_string();
                    entry.result = Some(task_result);
                    entry.completed_at = Some(Instant::now());
                })
                .await;
                fire_webhook_with_retry(&state, &task_id, &request.webhook_url).await;
                return;
            }
            Err(error_msg) => {
                if attempt >= max_retries {
                    update_task(&state, &task_id, |entry| {
                        entry.status = STATUS_FAILED.to_string();
                        entry.error = Some(error_msg);
                        entry.completed_at = Some(Instant::now());
                    })
                    .await;
                    fire_webhook_with_retry(&state, &task_id, &request.webhook_url).await;
                    return;
                }
                warn!(
                    task_id = %task_id,
                    attempt = attempt + 1,
                    error = %error_msg,
                    "Task attempt failed, will retry"
                );
            }
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
        orchestrator.snapshot(&options).await.ok().map(|s| s.tree)
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
        duration_ms: 0,
    })
}

/// Update a task entry with a closure.
async fn update_task<F>(state: &AppState, task_id: &str, f: F)
where
    F: FnOnce(&mut TaskEntry),
{
    let mut tasks = state.tasks.write().await;
    if let Some(entry) = tasks.get_mut(task_id) {
        f(entry);
    }
}

/// Fire a webhook callback with exponential backoff retry.
async fn fire_webhook_with_retry(state: &AppState, task_id: &str, webhook_url: &Option<String>) {
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

    for attempt in 0..WEBHOOK_MAX_RETRIES {
        info!(
            task_id,
            url = %url,
            attempt = attempt + 1,
            "Firing webhook callback"
        );

        let resp = state
            .http_client
            .post(url)
            .json(&entry)
            .header("X-CloudyAB-Task-Id", task_id)
            .header("X-CloudyAB-Attempt", (attempt + 1).to_string())
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                info!(task_id, "Webhook delivered successfully");
                return;
            }
            Ok(r) => {
                warn!(
                    task_id,
                    status = r.status().as_u16(),
                    "Webhook returned non-success status"
                );
            }
            Err(e) => {
                warn!(task_id, error = %e, "Webhook delivery failed");
            }
        }

        if attempt < WEBHOOK_MAX_RETRIES - 1 {
            let delay = Duration::from_millis(BACKOFF_BASE_MS * 2u64.pow(attempt));
            tokio::time::sleep(delay).await;
        }
    }

    warn!(task_id, url = %url, "Webhook delivery exhausted all retries");
}

/// Background task that cleans up expired task entries.
async fn cleanup_expired_tasks(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        let mut tasks = state.tasks.write().await;
        let now = Instant::now();
        let before = tasks.len();

        tasks.retain(|_, entry| {
            if entry.status == STATUS_PENDING
                || entry.status == STATUS_RUNNING
                || entry.status == STATUS_RETRYING
            {
                return true;
            }
            now.duration_since(entry.created_at).as_secs() < TASK_TTL_SECS
        });

        let removed = before - tasks.len();
        if removed > 0 {
            info!(removed, "Cleaned up expired tasks");
        }
    }
}

/// Serde support for Instant (serialized as elapsed seconds since creation).
mod instant_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Instant;

    pub fn serialize<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let elapsed = instant.elapsed().as_secs();
        elapsed.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let _secs = u64::deserialize(deserializer)?;
        Ok(Instant::now())
    }
}

mod option_instant_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Instant;

    pub fn serialize<S>(instant: &Option<Instant>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match instant {
            Some(i) => {
                let elapsed = i.elapsed().as_secs();
                Some(elapsed).serialize(serializer)
            }
            None => None::<u64>.serialize(serializer),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Instant>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<u64>::deserialize(deserializer)?;
        Ok(opt.map(|_| Instant::now()))
    }
}
