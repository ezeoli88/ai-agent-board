use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::db::Database;
use crate::services::task_event_service::{self, PersistEvent};

/// Channel capacity for per-task SSE broadcast channels.
const CHANNEL_CAPACITY: usize = 256;

/// Maximum number of events to keep in history per task to prevent memory leaks.
const MAX_HISTORY_PER_TASK: usize = 1500;

/// Types of SSE events that can be emitted to clients.
///
/// Each variant maps to the lowercase string used as the SSE `event:` field,
/// matching the TypeScript server's event names exactly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SSEEventType {
    Log,
    Status,
    TimeoutWarning,
    AwaitingReview,
    Complete,
    Error,
    PrComment,
    ChatMessage,
    ToolActivity,
}

impl SSEEventType {
    /// Returns the SSE event name string used in `event: <name>` lines.
    pub fn as_event_name(&self) -> &'static str {
        match self {
            SSEEventType::Log => "log",
            SSEEventType::Status => "status",
            SSEEventType::TimeoutWarning => "timeout_warning",
            SSEEventType::AwaitingReview => "awaiting_review",
            SSEEventType::Complete => "complete",
            SSEEventType::Error => "error",
            SSEEventType::PrComment => "pr_comment",
            SSEEventType::ChatMessage => "chat_message",
            SSEEventType::ToolActivity => "tool_activity",
        }
    }
}

/// An SSE event carrying a typed event and a JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSEEvent {
    pub event_type: SSEEventType,
    pub data: serde_json::Value,
}

/// Task-specific SSE broadcaster.
///
/// Maintains one `broadcast::Sender` per task ID so that multiple HTTP clients
/// can subscribe to the same task's event stream. Producers call `emit*` methods
/// which fan-out to every active subscriber on that task's channel.
///
/// This struct is cheaply cloneable (inner state is behind `Arc<RwLock<_>>`).
/// Message sent to the persistence background task.
struct PersistMsg {
    entity_id: String,
    entity_type: String,
    event: SSEEvent,
    timestamp: String,
}

#[derive(Clone)]
pub struct SSEEmitter {
    channels: Arc<RwLock<HashMap<String, broadcast::Sender<SSEEvent>>>>,
    /// Per-task event history for replay on new SSE connections.
    event_history: Arc<RwLock<HashMap<String, Vec<SSEEvent>>>>,
    /// Optional channel to the persistence background loop.
    persist_tx: Option<mpsc::UnboundedSender<PersistMsg>>,
}

impl SSEEmitter {
    /// Creates a new, empty emitter with no active channels and no persistence.
    pub fn new() -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            event_history: Arc::new(RwLock::new(HashMap::new())),
            persist_tx: None,
        }
    }

    /// Creates a new emitter backed by SQLite persistence.
    ///
    /// Returns the emitter and a `JoinHandle` for the background persist loop.
    /// Await the handle after the server stops to flush remaining events.
    pub fn with_persistence(db: Database) -> (Self, JoinHandle<()>) {
        let (tx, rx) = mpsc::unbounded_channel::<PersistMsg>();
        let handle = tokio::spawn(persist_loop(db, rx));
        let emitter = Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            event_history: Arc::new(RwLock::new(HashMap::new())),
            persist_tx: Some(tx),
        };
        (emitter, handle)
    }

    /// Subscribes to the event stream for a given task.
    ///
    /// If no channel exists for the task yet, one is created automatically.
    /// The returned `Receiver` will receive all events emitted after the
    /// subscription point.
    pub async fn subscribe(&self, task_id: &str) -> broadcast::Receiver<SSEEvent> {
        let mut channels = self.channels.write().await;
        let sender = channels
            .entry(task_id.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
                debug!(task_id, "Created new SSE channel");
                tx
            });
        sender.subscribe()
    }

    /// Stores an event in the history for replay on future SSE connections,
    /// but does NOT broadcast it to live subscribers.
    ///
    /// Use this for events that the frontend already handles optimistically
    /// (e.g., user chat messages added by the frontend before calling the API).
    /// Broadcasting these would cause duplicate entries in the UI.
    pub async fn store_event(&self, task_id: &str, event: SSEEvent) {
        self.store_event_for_entity(task_id, "task", event).await;
    }

    /// Stores an event in the history for replay on new SSE connections for any entity type.
    pub async fn store_event_for_entity(&self, entity_id: &str, entity_type: &str, event: SSEEvent) {
        // Send to persistence channel
        self.send_to_persist(entity_id, entity_type, &event);

        let mut history = self.event_history.write().await;
        let entity_history = history.entry(entity_id.to_string()).or_default();
        if entity_history.len() < MAX_HISTORY_PER_TASK {
            entity_history.push(event);
        }
    }

    /// Stores an event in the in-memory history only, without persisting to DB.
    ///
    /// Used when warming the cache from DB on first SSE connection after restart.
    pub async fn store_event_no_persist(&self, task_id: &str, event: SSEEvent) {
        let mut history = self.event_history.write().await;
        let task_history = history.entry(task_id.to_string()).or_default();
        if task_history.len() < MAX_HISTORY_PER_TASK {
            task_history.push(event);
        }
    }

    /// Emits an arbitrary SSE event to all subscribers of a task.
    ///
    /// If no channel exists for the task (i.e., nobody has subscribed),
    /// the event is silently dropped.
    pub async fn emit(&self, task_id: &str, event: SSEEvent) {
        self.emit_for_entity(task_id, "task", event).await;
    }

    /// Emits an SSE event for any entity type (task or spec).
    pub async fn emit_for_entity(&self, entity_id: &str, entity_type: &str, event: SSEEvent) {
        // Send to persistence channel
        self.send_to_persist(entity_id, entity_type, &event);

        // Store in history for replay on new connections
        {
            let mut history = self.event_history.write().await;
            let entity_history = history.entry(entity_id.to_string()).or_default();
            if entity_history.len() < MAX_HISTORY_PER_TASK {
                entity_history.push(event.clone());
            }
        }

        // Broadcast to live subscribers
        let channels = self.channels.read().await;
        if let Some(sender) = channels.get(entity_id) {
            let receiver_count = sender.receiver_count();
            if receiver_count == 0 {
                debug!(
                    entity_id,
                    event_type = %event.event_type.as_event_name(),
                    "SSE broadcast skipped: 0 receivers"
                );
                return;
            }
            match sender.send(event) {
                Ok(_) => {
                    debug!(
                        entity_id,
                        event_type = ?&"emit",
                        receivers = receiver_count,
                        "SSE event broadcast"
                    );
                }
                Err(_) => {
                    warn!(entity_id, "SSE send failed: no active receivers");
                }
            }
        } else {
            debug!(
                entity_id,
                event_type = %event.event_type.as_event_name(),
                "SSE broadcast skipped: no channel exists for entity"
            );
        }
    }

    /// Emits a log event for a task.
    pub async fn emit_log(
        &self,
        task_id: &str,
        level: &str,
        message: &str,
        data: Option<serde_json::Value>,
    ) {
        let mut log_data = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "level": level,
            "message": message,
        });
        if let Some(extra) = data {
            log_data
                .as_object_mut()
                .unwrap()
                .insert("data".to_string(), extra);
        }
        self.emit(
            task_id,
            SSEEvent {
                event_type: SSEEventType::Log,
                data: log_data,
            },
        )
        .await;
    }

    /// Emits a status change event for a task.
    pub async fn emit_status(&self, task_id: &str, status: &str) {
        self.emit(
            task_id,
            SSEEvent {
                event_type: SSEEventType::Status,
                data: serde_json::json!({ "status": status }),
            },
        )
        .await;
    }

    /// Emits a completion event for a task.
    pub async fn emit_complete(
        &self,
        task_id: &str,
        pr_url: Option<&str>,
        summary: Option<&str>,
    ) {
        let mut obj = serde_json::Map::new();
        if let Some(url) = pr_url {
            obj.insert("pr_url".to_string(), serde_json::Value::String(url.to_string()));
        }
        if let Some(s) = summary {
            obj.insert("summary".to_string(), serde_json::Value::String(s.to_string()));
        }
        self.emit(
            task_id,
            SSEEvent {
                event_type: SSEEventType::Complete,
                data: serde_json::Value::Object(obj),
            },
        )
        .await;
    }

    /// Emits a chat message event for a task.
    ///
    /// Matches the `ChatMessageEvent` schema: `{ id, role, content, timestamp }`.
    pub async fn emit_chat_message(&self, task_id: &str, role: &str, content: &str) {
        self.emit(
            task_id,
            SSEEvent {
                event_type: SSEEventType::ChatMessage,
                data: serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "role": role,
                    "content": content,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }),
            },
        )
        .await;
    }

    /// Emits a tool activity event for a task.
    ///
    /// Matches the `ToolActivityEvent` schema: `{ id, name, summary, status, timestamp }`.
    /// `status` should be one of: `"running"`, `"completed"`, `"error"`.
    pub async fn emit_tool_activity(
        &self,
        task_id: &str,
        tool_id: &str,
        name: &str,
        summary: &str,
        status: &str,
    ) {
        let id = if tool_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            tool_id.to_string()
        };
        self.emit(
            task_id,
            SSEEvent {
                event_type: SSEEventType::ToolActivity,
                data: serde_json::json!({
                    "id": id,
                    "name": name,
                    "summary": summary,
                    "status": status,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }),
            },
        )
        .await;
    }

    /// Emits an awaiting_review event for a task.
    ///
    /// This is a non-terminal event — the SSE connection stays open so the user
    /// can send feedback and the agent can resume on the same channel.
    pub async fn emit_awaiting_review(&self, task_id: &str, message: &str) {
        self.emit(
            task_id,
            SSEEvent {
                event_type: SSEEventType::AwaitingReview,
                data: serde_json::json!({ "message": message }),
            },
        )
        .await;
    }

    /// Emits an error event for a task.
    pub async fn emit_error(&self, task_id: &str, message: &str) {
        self.emit(
            task_id,
            SSEEvent {
                event_type: SSEEventType::Error,
                data: serde_json::json!({ "message": message }),
            },
        )
        .await;
    }

    /// Returns the event history for a task (for replay on new SSE connections).
    pub async fn get_history(&self, task_id: &str) -> Vec<SSEEvent> {
        let history = self.event_history.read().await;
        history.get(task_id).cloned().unwrap_or_default()
    }

    /// Removes the broadcast channel for a task, causing all receivers to see
    /// the channel as closed. Subsequent subscribes will create a fresh channel.
    /// Note: event history is NOT cleared — it persists for replay until
    /// explicitly cleaned up via [`clear_history`].
    pub async fn close_task(&self, task_id: &str) {
        let mut channels = self.channels.write().await;
        if channels.remove(task_id).is_some() {
            debug!(task_id, "SSE channel closed and removed");
        }
    }

    /// Clears the event history for a task (e.g. when task is deleted or archived).
    pub async fn clear_history(&self, task_id: &str) {
        let mut history = self.event_history.write().await;
        history.remove(task_id);
    }

    /// Returns the number of active receivers (connected SSE clients) for a task.
    /// Returns 0 if no channel exists for the task.
    pub async fn get_client_count(&self, task_id: &str) -> usize {
        let channels = self.channels.read().await;
        channels
            .get(task_id)
            .map(|tx| tx.receiver_count())
            .unwrap_or(0)
    }

    /// Sends an event to the persistence background loop (if enabled).
    fn send_to_persist(&self, entity_id: &str, entity_type: &str, event: &SSEEvent) {
        if let Some(ref tx) = self.persist_tx {
            let _ = tx.send(PersistMsg {
                entity_id: entity_id.to_string(),
                entity_type: entity_type.to_string(),
                event: event.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
        }
    }
}

/// Background loop that batches incoming events and flushes them to SQLite.
///
/// Collects events for up to 500ms or 50 events (whichever comes first),
/// then batch-inserts via `db.call()`. On channel close (shutdown), flushes
/// any remaining buffered events.
async fn persist_loop(db: Database, mut rx: mpsc::UnboundedReceiver<PersistMsg>) {
    let mut buffer: Vec<PersistEvent> = Vec::new();
    const BATCH_SIZE: usize = 50;
    const FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

    loop {
        // Wait for first event or channel close
        let msg = tokio::select! {
            msg = rx.recv() => msg,
        };

        match msg {
            Some(msg) => {
                buffer.push(PersistEvent {
                    entity_id: msg.entity_id,
                    entity_type: msg.entity_type,
                    event: msg.event,
                    timestamp: msg.timestamp,
                });

                // Drain more events up to BATCH_SIZE or FLUSH_INTERVAL
                let deadline = tokio::time::Instant::now() + FLUSH_INTERVAL;
                while buffer.len() < BATCH_SIZE {
                    let timeout = tokio::time::timeout_at(deadline, rx.recv());
                    match timeout.await {
                        Ok(Some(msg)) => {
                            buffer.push(PersistEvent {
                                entity_id: msg.entity_id,
                                entity_type: msg.entity_type,
                                event: msg.event,
                                timestamp: msg.timestamp,
                            });
                        }
                        Ok(None) => {
                            // Channel closed — flush and exit
                            flush_buffer(&db, &mut buffer).await;
                            info!("SSE persist loop: channel closed, flushed remaining events");
                            return;
                        }
                        Err(_) => break, // Timeout — flush what we have
                    }
                }

                flush_buffer(&db, &mut buffer).await;
            }
            None => {
                // Channel closed
                flush_buffer(&db, &mut buffer).await;
                info!("SSE persist loop: shutdown complete");
                return;
            }
        }
    }
}

/// Flushes the buffer to the database.
async fn flush_buffer(db: &Database, buffer: &mut Vec<PersistEvent>) {
    if buffer.is_empty() {
        return;
    }

    let events: Vec<PersistEvent> = buffer.drain(..).collect();
    let count = events.len();

    if let Err(e) = db
        .call(move |conn| task_event_service::insert_events(conn, &events))
        .await
    {
        warn!(error = %e, count, "Failed to persist SSE events batch");
    } else {
        debug!(count, "Persisted SSE events batch");
    }
}

impl Default for SSEEmitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_serializes_to_snake_case() {
        let json = serde_json::to_string(&SSEEventType::TimeoutWarning).unwrap();
        assert_eq!(json, r#""timeout_warning""#);

        let json = serde_json::to_string(&SSEEventType::ChatMessage).unwrap();
        assert_eq!(json, r#""chat_message""#);

        let json = serde_json::to_string(&SSEEventType::PrComment).unwrap();
        assert_eq!(json, r#""pr_comment""#);
    }

    #[test]
    fn event_type_as_event_name() {
        assert_eq!(SSEEventType::Log.as_event_name(), "log");
        assert_eq!(SSEEventType::ToolActivity.as_event_name(), "tool_activity");
    }

    #[tokio::test]
    async fn subscribe_creates_channel_and_receives_events() {
        let emitter = SSEEmitter::new();
        let mut rx = emitter.subscribe("task-1").await;

        emitter
            .emit(
                "task-1",
                SSEEvent {
                    event_type: SSEEventType::Status,
                    data: serde_json::json!({"status": "running"}),
                },
            )
            .await;

        let event = rx.recv().await.unwrap();
        assert_eq!(event.event_type, SSEEventType::Status);
        assert_eq!(event.data["status"], "running");
    }

    #[tokio::test]
    async fn emit_to_nonexistent_task_is_silent() {
        let emitter = SSEEmitter::new();
        // Should not panic
        emitter
            .emit(
                "no-such-task",
                SSEEvent {
                    event_type: SSEEventType::Error,
                    data: serde_json::json!({"message": "boom"}),
                },
            )
            .await;
    }

    #[tokio::test]
    async fn close_task_removes_channel() {
        let emitter = SSEEmitter::new();
        let _rx = emitter.subscribe("task-2").await;
        assert_eq!(emitter.get_client_count("task-2").await, 1);

        emitter.close_task("task-2").await;
        assert_eq!(emitter.get_client_count("task-2").await, 0);
    }

    #[tokio::test]
    async fn get_client_count_reflects_subscribers() {
        let emitter = SSEEmitter::new();
        assert_eq!(emitter.get_client_count("t").await, 0);

        let _rx1 = emitter.subscribe("t").await;
        assert_eq!(emitter.get_client_count("t").await, 1);

        let _rx2 = emitter.subscribe("t").await;
        assert_eq!(emitter.get_client_count("t").await, 2);

        drop(_rx1);
        // broadcast::Sender::receiver_count decrements when receivers drop
        assert_eq!(emitter.get_client_count("t").await, 1);
    }
}
