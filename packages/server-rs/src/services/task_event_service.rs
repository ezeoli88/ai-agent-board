//! Task event persistence service.
//!
//! Provides functions to persist and retrieve SSE events. New events are stored
//! in `agent_events`, which can be keyed by either a task id or a spec id. Legacy
//! task events are still read from `task_logs` for backward compatibility.

use rusqlite::Connection;
use tracing::warn;

use crate::error::AppError;
use crate::utils::{SSEEvent, SSEEventType};

/// A single event to be persisted.
pub struct PersistEvent {
    pub task_id: String,
    pub event: SSEEvent,
    pub timestamp: String,
}

/// Batch-inserts events into the `agent_events` table.
///
/// For `log` events, extracts `level` and `message` from the event data for
/// readable exports. For all other event types, stores `level='info'` and
/// `message=<event_type>`.
pub fn insert_events(conn: &Connection, events: &[PersistEvent]) -> Result<(), AppError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut stmt = conn
        .prepare_cached(
            "INSERT INTO agent_events (id, run_id, timestamp, level, message, event_type, event_data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
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
            event.task_id,
            event.timestamp,
            level,
            message,
            event_type_str,
            event_data_json,
        ]) {
            warn!(task_id = %event.task_id, error = %e, "Failed to persist SSE event");
        }
    }

    Ok(())
}

/// Retrieves all persisted events for a run id, ordered by insertion order (ROWID ASC).
///
/// `run_id` is historically named `task_id` by callers, but specs use the same
/// function with their spec id. If no generic events exist, it falls back to
/// legacy `task_logs` rows.
pub fn get_events_for_task(conn: &Connection, task_id: &str) -> Result<Vec<SSEEvent>, AppError> {
    if table_exists(conn, "agent_events")? {
        let events = get_events_from_agent_events(conn, task_id)?;
        if !events.is_empty() {
            return Ok(events);
        }
    }

    get_events_from_task_logs(conn, task_id)
}

fn get_events_from_agent_events(
    conn: &Connection,
    run_id: &str,
) -> Result<Vec<SSEEvent>, AppError> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT level, message, event_type, event_data, timestamp
             FROM agent_events
             WHERE run_id = ?1
             ORDER BY ROWID ASC",
        )
        .map_err(AppError::Database)?;

    read_events_from_stmt(&mut stmt, run_id)
}

fn get_events_from_task_logs(conn: &Connection, task_id: &str) -> Result<Vec<SSEEvent>, AppError> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT level, message, event_type, event_data, timestamp
             FROM task_logs
             WHERE task_id = ?1
             ORDER BY ROWID ASC",
        )
        .map_err(AppError::Database)?;

    read_events_from_stmt(&mut stmt, task_id)
}

fn read_events_from_stmt(
    stmt: &mut rusqlite::CachedStatement<'_>,
    id: &str,
) -> Result<Vec<SSEEvent>, AppError> {
    let rows = stmt
        .query_map(rusqlite::params![id], |row| {
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
            let ts = timestamp.unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
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

fn table_exists(conn: &Connection, table_name: &str) -> Result<bool, AppError> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table_name],
            |row| row.get(0),
        )
        .map_err(AppError::Database)?;
    Ok(exists > 0)
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
