//! Event persistence service.
//!
//! Provides functions to persist and retrieve SSE events from the `event_logs` table.
//! Events are stored with their full JSON payload in `event_data`, while `level`/`message`
//! are populated for backward compatibility with the export format.

use rusqlite::Connection;
use tracing::warn;

use crate::error::AppError;
use crate::utils::{SSEEvent, SSEEventType};

/// A single event to be persisted.
pub struct PersistEvent {
    pub entity_id: String,
    pub entity_type: String,
    pub event: SSEEvent,
    pub timestamp: String,
}

/// Batch-inserts events into the `event_logs` table.
///
/// For `log` events, extracts `level` and `message` from the event data for
/// backward compatibility. For all other event types, stores `level='info'`
/// and `message=<event_type>`.
pub fn insert_events(conn: &Connection, events: &[PersistEvent]) -> Result<(), AppError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut stmt = conn
        .prepare_cached(
            "INSERT INTO event_logs (id, entity_id, entity_type, timestamp, level, message, event_type, event_data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .map_err(AppError::Database)?;

    for event in events {
        let id = uuid::Uuid::new_v4().to_string();
        let event_type_str = event.event.event_type.as_event_name();
        let event_data_json = serde_json::to_string(&event.event.data).unwrap_or_default();

        // Extract level/message for backward compat
        let (level, message) = if event.event.event_type == SSEEventType::Log {
            let level = event.event.data["level"]
                .as_str()
                .unwrap_or("info")
                .to_string();
            let message = event.event.data["message"]
                .as_str()
                .unwrap_or("")
                .to_string();
            (level, message)
        } else {
            ("info".to_string(), event_type_str.to_string())
        };

        if let Err(e) = stmt.execute(rusqlite::params![
            id,
            event.entity_id,
            event.entity_type,
            event.timestamp,
            level,
            message,
            event_type_str,
            event_data_json,
        ]) {
            warn!(entity_id = %event.entity_id, error = %e, "Failed to persist SSE event");
        }
    }

    Ok(())
}

/// Retrieves all persisted events for an entity, ordered by insertion order (ROWID ASC).
///
/// Handles two cases:
/// - **New rows** (have `event_data`): deserialize from the stored JSON payload.
/// - **Legacy rows** (no `event_data`): reconstruct a log SSEEvent from `level`/`message`.
pub fn get_events_for_entity(conn: &Connection, entity_id: &str) -> Result<Vec<SSEEvent>, AppError> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT level, message, event_type, event_data, timestamp
             FROM event_logs
             WHERE entity_id = ?1
             ORDER BY ROWID ASC",
        )
        .map_err(AppError::Database)?;

    let rows = stmt
        .query_map(rusqlite::params![entity_id], |row| {
            let level: String = row.get(0)?;
            let message: String = row.get(1)?;
            let event_type: Option<String> = row.get(2)?;
            let event_data: Option<String> = row.get(3)?;
            let timestamp: Option<String> = row.get(4)?;
            Ok((level, message, event_type, event_data, timestamp))
        })
        .map_err(AppError::Database)?;

    let mut events = Vec::new();
    for row in rows {
        let (level, message, event_type, event_data, timestamp) =
            row.map_err(AppError::Database)?;

        let event = if let Some(ref data_json) = event_data {
            // New-style row: parse event_type and event_data
            let etype = event_type
                .as_deref()
                .and_then(parse_event_type)
                .unwrap_or(SSEEventType::Log);
            let data: serde_json::Value =
                serde_json::from_str(data_json).unwrap_or(serde_json::json!({}));
            SSEEvent {
                event_type: etype,
                data,
            }
        } else {
            // Legacy row: reconstruct a log event from level/message
            let ts = timestamp
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
            SSEEvent {
                event_type: SSEEventType::Log,
                data: serde_json::json!({
                    "timestamp": ts,
                    "level": level,
                    "message": message,
                }),
            }
        };

        events.push(event);
    }

    Ok(events)
}

/// Parses an event type string back into an SSEEventType enum.
fn parse_event_type(s: &str) -> Option<SSEEventType> {
    match s {
        "log" => Some(SSEEventType::Log),
        "status" => Some(SSEEventType::Status),
        "timeout_warning" => Some(SSEEventType::TimeoutWarning),
        "awaiting_review" => Some(SSEEventType::AwaitingReview),
        "complete" => Some(SSEEventType::Complete),
        "error" => Some(SSEEventType::Error),
        "pr_comment" => Some(SSEEventType::PrComment),
        "chat_message" => Some(SSEEventType::ChatMessage),
        "tool_activity" => Some(SSEEventType::ToolActivity),
        _ => None,
    }
}
