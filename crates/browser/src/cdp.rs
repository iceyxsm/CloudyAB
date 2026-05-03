//! Raw CDP (Chrome DevTools Protocol) WebSocket client.
//!
//! Designed to tolerate Obscura's non-standard messages — unknown methods,
//! extra fields, and proprietary events are logged and discarded rather than
//! causing connection failures.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{debug, trace};

use cloudyab_core::engine::EngineError;

/// Timeout for individual CDP method calls.
const CDP_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Channel buffer size for outgoing messages.
const SEND_BUFFER: usize = 64;

type WsSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;
type WsStream = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;
type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, CdpError>>>>>;

/// Errors specific to the CDP transport layer.
#[derive(Debug, thiserror::Error)]
pub enum CdpError {
    #[error("WebSocket connection failed: {0}")]
    Connection(String),

    #[error("CDP call timed out after {0:?}")]
    Timeout(Duration),

    #[error("CDP error response (code {code}): {message}")]
    Protocol { code: i64, message: String },

    #[error("WebSocket closed unexpectedly")]
    Closed,

    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<CdpError> for EngineError {
    fn from(e: CdpError) -> Self {
        EngineError::BrowserError(e.to_string())
    }
}

/// A raw CDP WebSocket client that tolerates non-standard messages.
///
/// Sends JSON-RPC style CDP commands and routes responses by `id`.
/// Unknown events and malformed messages from Obscura are logged at
/// trace level and discarded — they never crash the connection.
pub struct CdpClient {
    sender: mpsc::Sender<Message>,
    pending: PendingMap,
    next_id: AtomicU64,
    /// Channel for receiving CDP events (method notifications without an id).
    event_rx: Mutex<mpsc::Receiver<CdpEvent>>,
}

/// A CDP event (server-initiated notification).
#[derive(Debug, Clone)]
pub struct CdpEvent {
    pub method: String,
    pub params: Value,
}

impl CdpClient {
    /// Connect to a CDP WebSocket endpoint.
    ///
    /// Spawns background tasks for reading and writing. The client remains
    /// functional even when Obscura sends non-standard messages.
    pub async fn connect(ws_url: &str) -> Result<Self, CdpError> {
        let (ws_stream, _response) = connect_async(ws_url)
            .await
            .map_err(|e| CdpError::Connection(format!("{e}")))?;

        let (sink, stream) = ws_stream.split();

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (send_tx, send_rx) = mpsc::channel::<Message>(SEND_BUFFER);
        let (event_tx, event_rx) = mpsc::channel::<CdpEvent>(256);

        // Spawn writer task
        tokio::spawn(Self::writer_loop(sink, send_rx));

        // Spawn reader task
        tokio::spawn(Self::reader_loop(stream, pending.clone(), event_tx));

        Ok(Self {
            sender: send_tx,
            pending,
            next_id: AtomicU64::new(1),
            event_rx: Mutex::new(event_rx),
        })
    }

    /// Send a CDP method call and await the response.
    ///
    /// Returns the `result` field from the CDP response, or an error if the
    /// call times out or the server returns an error object.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, CdpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let msg = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });

        let text = serde_json::to_string(&msg)
            .map_err(|e| CdpError::Serialization(e.to_string()))?;

        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        self.sender
            .send(Message::Text(text))
            .await
            .map_err(|_| CdpError::Closed)?;

        let result = tokio::time::timeout(CDP_CALL_TIMEOUT, rx)
            .await
            .map_err(|_| {
                // Clean up the pending entry on timeout
                let pending = self.pending.clone();
                let id_copy = id;
                tokio::spawn(async move {
                    pending.lock().await.remove(&id_copy);
                });
                CdpError::Timeout(CDP_CALL_TIMEOUT)
            })?
            .map_err(|_| CdpError::Closed)?;

        result
    }

    /// Send a CDP method call without waiting for a response (fire-and-forget).
    pub async fn fire(&self, method: &str, params: Value) -> Result<(), CdpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let msg = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });

        let text = serde_json::to_string(&msg)
            .map_err(|e| CdpError::Serialization(e.to_string()))?;

        self.sender
            .send(Message::Text(text))
            .await
            .map_err(|_| CdpError::Closed)?;

        Ok(())
    }

    /// Receive the next CDP event, if available.
    pub async fn next_event(&self) -> Option<CdpEvent> {
        let mut rx = self.event_rx.lock().await;
        rx.recv().await
    }

    /// Background task: writes outgoing messages to the WebSocket.
    async fn writer_loop(mut sink: WsSink, mut rx: mpsc::Receiver<Message>) {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = sink.send(msg).await {
                debug!(error = %e, "CDP WebSocket write error, closing writer");
                break;
            }
        }
        let _ = sink.close().await;
    }

    /// Background task: reads incoming messages and routes them.
    ///
    /// Responses (messages with `id`) are routed to pending callers.
    /// Events (messages with `method` but no `id`) are sent to the event channel.
    /// Malformed or non-standard messages are logged and discarded.
    async fn reader_loop(
        mut stream: WsStream,
        pending: PendingMap,
        event_tx: mpsc::Sender<CdpEvent>,
    ) {
        while let Some(msg_result) = stream.next().await {
            let msg = match msg_result {
                Ok(m) => m,
                Err(e) => {
                    debug!(error = %e, "CDP WebSocket read error");
                    break;
                }
            };

            let text = match msg {
                Message::Text(t) => t,
                Message::Binary(b) => {
                    trace!(len = b.len(), "Ignoring binary CDP message");
                    continue;
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => {
                    debug!("CDP WebSocket closed by server");
                    break;
                }
                Message::Frame(_) => continue,
            };

            let parsed: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    trace!(
                        error = %e,
                        preview = &text[..text.len().min(200)],
                        "Ignoring non-JSON CDP message (Obscura non-standard)"
                    );
                    continue;
                }
            };

            // Route by presence of `id` field (response) vs `method` field (event)
            if let Some(id) = parsed.get("id").and_then(|v| v.as_u64()) {
                Self::handle_response(&pending, id, &parsed).await;
            } else if let Some(method) = parsed.get("method").and_then(|v| v.as_str()) {
                let params = parsed.get("params").cloned().unwrap_or(Value::Null);
                let event = CdpEvent {
                    method: method.to_string(),
                    params,
                };
                // Non-blocking send — drop event if channel is full
                if event_tx.try_send(event).is_err() {
                    trace!("CDP event channel full, dropping event");
                }
            } else {
                // Non-standard message from Obscura — log and discard
                trace!(
                    preview = &text[..text.len().min(200)],
                    "Ignoring unrecognized CDP message (Obscura proprietary)"
                );
            }
        }

        // Connection closed — fail all pending calls
        let mut pending = pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err(CdpError::Closed));
        }
    }

    /// Route a CDP response to the pending caller.
    async fn handle_response(pending: &PendingMap, id: u64, parsed: &Value) {
        let mut map = pending.lock().await;
        if let Some(tx) = map.remove(&id) {
            let result = if let Some(error) = parsed.get("error") {
                let code = error.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
                let message = error
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown CDP error")
                    .to_string();
                Err(CdpError::Protocol { code, message })
            } else {
                Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
            };
            let _ = tx.send(result);
        } else {
            trace!(id, "Received response for unknown/expired CDP call");
        }
    }
}
