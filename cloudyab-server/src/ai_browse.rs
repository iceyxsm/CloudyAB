//! AI-powered browsing agent.
//!
//! Uses an LLM (OpenAI-compatible API) to autonomously navigate pages,
//! extract information, and complete multi-step browsing tasks.

use std::sync::Arc;

use cloudyab_core::config::AiConfig;
use cloudyab_core::orchestrator::Orchestrator;
use cloudyab_types::session::{Layer, SessionConfig};
use cloudyab_types::page::SnapshotOptions;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Maximum characters from snapshot to include in LLM context.
const MAX_SNAPSHOT_CHARS: usize = 8000;

/// System prompt for the browsing agent.
const SYSTEM_PROMPT: &str = r#"You are a web browsing agent. You navigate pages and extract information.

You receive a page snapshot (accessibility tree with element refs like @e1, @e2).
Respond with ONE action in JSON format:

{"action": "click", "ref": "e5"}
{"action": "fill", "ref": "e3", "text": "search query"}
{"action": "navigate", "url": "https://example.com"}
{"action": "extract", "data": "the information you found"}
{"action": "done", "result": "final answer or extracted data"}

Rules:
- Use element refs from the snapshot (e.g., "e1", "e5")
- Only one action per response
- Use "done" when you have the answer or completed the task
- Use "extract" to report intermediate findings
- Be concise and direct"#;

/// Result of an AI browse session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseResult {
    /// Whether the task was completed successfully.
    pub success: bool,
    /// The extracted data or final answer.
    pub result: String,
    /// Number of steps taken.
    pub steps_taken: u32,
    /// URLs visited during the session.
    pub urls_visited: Vec<String>,
}

/// Run an AI-powered browsing session to accomplish a goal.
pub async fn ai_browse(
    orchestrator: &Arc<RwLock<Orchestrator>>,
    config: &AiConfig,
    goal: &str,
    start_url: Option<&str>,
) -> Result<BrowseResult, AiBrowseError> {
    let api_key = config.api_key.as_deref().ok_or(AiBrowseError::NoApiKey)?;
    let client = Client::new();
    let mut urls_visited = Vec::new();
    let mut messages = vec![
        ChatMessage { role: "system".into(), content: SYSTEM_PROMPT.into() },
        ChatMessage { role: "user".into(), content: format!("Goal: {goal}") },
    ];

    // Navigate to start URL if provided
    if let Some(url) = start_url {
        navigate_to(orchestrator, url).await?;
        urls_visited.push(url.to_string());
    }

    for step in 0..config.max_steps {
        let snapshot_text = get_snapshot_for_llm(orchestrator).await?;
        messages.push(ChatMessage {
            role: "user".into(),
            content: format!("Page snapshot:\n{snapshot_text}\n\nWhat action should I take?"),
        });

        let response = call_llm(&client, config, api_key, &messages).await?;
        debug!(step, response = %response, "LLM response");
        messages.push(ChatMessage { role: "assistant".into(), content: response.clone() });

        let action = parse_action(&response)?;
        match execute_action(orchestrator, action, &mut urls_visited, &mut messages).await? {
            StepOutcome::Continue => {}
            StepOutcome::Done(result) => {
                info!(steps = step + 1, "AI browse completed");
                return Ok(BrowseResult {
                    success: true,
                    result,
                    steps_taken: step + 1,
                    urls_visited,
                });
            }
        }
    }

    warn!("AI browse hit max steps limit");
    Ok(BrowseResult {
        success: false,
        result: "Reached maximum steps without completing the task".into(),
        steps_taken: config.max_steps,
        urls_visited,
    })
}

enum StepOutcome {
    Continue,
    Done(String),
}

/// Execute a parsed agent action against the orchestrator.
async fn execute_action(
    orchestrator: &Arc<RwLock<Orchestrator>>,
    action: AgentAction,
    urls_visited: &mut Vec<String>,
    messages: &mut Vec<ChatMessage>,
) -> Result<StepOutcome, AiBrowseError> {
    match action {
        AgentAction::Click { ref_id } => {
            info!(ref_id = %ref_id, "AI clicking");
            let orch = orchestrator.read().await;
            orch.click(&ref_id).await.map_err(|e| {
                AiBrowseError::ActionFailed(format!("Click {ref_id} failed: {e}"))
            })?;
            Ok(StepOutcome::Continue)
        }
        AgentAction::Fill { ref_id, text } => {
            info!(ref_id = %ref_id, "AI filling");
            let orch = orchestrator.read().await;
            orch.fill(&ref_id, &text).await.map_err(|e| {
                AiBrowseError::ActionFailed(format!("Fill {ref_id} failed: {e}"))
            })?;
            Ok(StepOutcome::Continue)
        }
        AgentAction::Navigate { url } => {
            info!(url = %url, "AI navigating");
            navigate_to(orchestrator, &url).await?;
            urls_visited.push(url);
            Ok(StepOutcome::Continue)
        }
        AgentAction::Done { result } => Ok(StepOutcome::Done(result)),
        AgentAction::Extract { data } => {
            info!("AI extracted intermediate data");
            messages.push(ChatMessage {
                role: "user".into(),
                content: format!("Noted. Continue with the task. Extracted so far: {data}"),
            });
            Ok(StepOutcome::Continue)
        }
    }
}

/// Navigate to a URL via the orchestrator.
async fn navigate_to(
    orchestrator: &Arc<RwLock<Orchestrator>>,
    url: &str,
) -> Result<(), AiBrowseError> {
    let orch = orchestrator.read().await;
    let nav_config = SessionConfig {
        target_url: url.to_string(),
        fingerprint: None,
        proxy: None,
        persist_cookies: true,
        timeout_secs: 30,
        preferred_layer: Some(Layer::Browser),
    };
    orch.navigate(&nav_config).await.map_err(|e| {
        AiBrowseError::Navigation(format!("Failed to navigate to {url}: {e}"))
    })?;
    Ok(())
}

/// Get a truncated snapshot suitable for LLM context.
async fn get_snapshot_for_llm(
    orchestrator: &Arc<RwLock<Orchestrator>>,
) -> Result<String, AiBrowseError> {
    let orch = orchestrator.read().await;
    let options = SnapshotOptions {
        interactive_only: false,
        compact: true,
        max_depth: 0,
        selector: None,
    };

    let snapshot = orch.snapshot(&options).await.map_err(|e| {
        AiBrowseError::ActionFailed(format!("Snapshot failed: {e}"))
    })?;

    let mut text = format!("URL: {}\nTitle: {}\n\n", snapshot.url, snapshot.title);
    let tree = if snapshot.tree.len() > MAX_SNAPSHOT_CHARS {
        &snapshot.tree[..MAX_SNAPSHOT_CHARS]
    } else {
        &snapshot.tree
    };
    text.push_str(tree);
    Ok(text)
}

/// Call the LLM API (OpenAI-compatible chat completions endpoint).
async fn call_llm(
    client: &Client,
    config: &AiConfig,
    api_key: &str,
    messages: &[ChatMessage],
) -> Result<String, AiBrowseError> {
    let base_url = config.api_base_url.as_deref().unwrap_or(resolve_base_url(&config.provider));
    let url = format!("{base_url}/chat/completions");

    let body = serde_json::json!({
        "model": config.model,
        "messages": messages,
        "temperature": 0.1,
        "max_tokens": 256
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AiBrowseError::LlmApi(format!("Request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        return Err(AiBrowseError::LlmApi(format!("API returned {status}: {text}")));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| {
        AiBrowseError::LlmApi(format!("Failed to parse response: {e}"))
    })?;

    json["choices"][0]["message"]["content"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| AiBrowseError::LlmApi("No content in response".into()))
}

/// Resolve the default API base URL for a provider.
fn resolve_base_url(provider: &str) -> &str {
    match provider {
        "anthropic" => "https://api.anthropic.com/v1",
        "local" => "http://localhost:11434/v1",
        _ => "https://api.openai.com/v1",
    }
}

/// Parse an LLM response into a structured action.
fn parse_action(response: &str) -> Result<AgentAction, AiBrowseError> {
    let json_str = extract_json(response);
    let val: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        AiBrowseError::ParseFailed(format!("Invalid action JSON: {e}\nRaw: {response}"))
    })?;

    let action = val["action"].as_str().unwrap_or("");
    match action {
        "click" => Ok(AgentAction::Click {
            ref_id: val["ref"].as_str().unwrap_or("").to_string(),
        }),
        "fill" => Ok(AgentAction::Fill {
            ref_id: val["ref"].as_str().unwrap_or("").to_string(),
            text: val["text"].as_str().unwrap_or("").to_string(),
        }),
        "navigate" => Ok(AgentAction::Navigate {
            url: val["url"].as_str().unwrap_or("").to_string(),
        }),
        "done" => Ok(AgentAction::Done {
            result: val["result"].as_str().unwrap_or("").to_string(),
        }),
        "extract" => Ok(AgentAction::Extract {
            data: val["data"].as_str().unwrap_or("").to_string(),
        }),
        _ => Err(AiBrowseError::ParseFailed(format!("Unknown action: {action}"))),
    }
}

/// Extract JSON object from a response that might contain markdown fences.
fn extract_json(text: &str) -> &str {
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return &text[start..=end];
        }
    }
    text
}

// ─── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

enum AgentAction {
    Click { ref_id: String },
    Fill { ref_id: String, text: String },
    Navigate { url: String },
    Done { result: String },
    Extract { data: String },
}

/// Errors from AI browsing operations.
#[derive(Debug, thiserror::Error)]
pub enum AiBrowseError {
    #[error("AI browsing not configured: no API key set in [ai] config section")]
    NoApiKey,

    #[error("LLM API error: {0}")]
    LlmApi(String),

    #[error("Navigation failed: {0}")]
    Navigation(String),

    #[error("Action execution failed: {0}")]
    ActionFailed(String),

    #[error("Failed to parse LLM response: {0}")]
    ParseFailed(String),
}
