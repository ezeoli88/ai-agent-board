use serde_json::Value;
use tracing::debug;

use crate::utils::{SSEEmitter, SSEEvent, SSEEventType};

/// Parses a single line of Gemini CLI `stream-json` output.
///
/// Gemini emits JSON lines with a `type` field:
/// - `init`: session metadata (model, session_id)
/// - `tool_use`: tool invocation with name and parameters
/// - `tool_result`: tool output with status and content
/// - `result`: final result with status and optional stats/error
/// - Plain text (non-JSON): agent messages and summaries
pub async fn parse(line: &str, emitter: &SSEEmitter, task_id: &str) {
    let parsed: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            // Not JSON -- plain text agent message
            emit_log(
                emitter,
                task_id,
                "info",
                &format!("Agent: {}", truncate(line, 1000)),
            )
            .await;
            emit_chat_message(emitter, task_id, "assistant", line).await;
            return;
        }
    };

    let event_type = parsed.get("type").and_then(Value::as_str).unwrap_or("");
    let timestamp = parsed
        .get("timestamp")
        .and_then(Value::as_str)
        .map(String::from);

    match event_type {
        "init" => {
            // Session initialization -- extract model and session info
            let model = parsed.get("model").and_then(Value::as_str);
            let session_id = parsed.get("session_id").and_then(Value::as_str);

            let mut parts = Vec::new();
            if let Some(m) = model {
                parts.push(format!("model={m}"));
            }
            if let Some(s) = session_id {
                parts.push(format!("session={}", truncate(s, 8)));
            }

            let info_msg = if parts.is_empty() {
                "Gemini initialized".to_string()
            } else {
                format!("Gemini initialized: {}", parts.join(", "))
            };

            emit_log(emitter, task_id, "info", &info_msg).await;
            emit_chat_message_with_ts(emitter, task_id, "system", &info_msg, timestamp.as_deref())
                .await;
        }

        "tool_use" => {
            // Tool invocation -- emit tool activity with "running" status
            let tool_name = parsed
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let tool_id = parsed.get("tool_id").and_then(Value::as_str).unwrap_or("");
            let parameters = parsed.get("parameters");

            let summary = extract_tool_summary(parameters);

            let log_detail = if summary.is_empty() {
                String::new()
            } else {
                format!(": {summary}")
            };
            emit_log(
                emitter,
                task_id,
                "info",
                &format!("Tool: {tool_name}{log_detail}"),
            )
            .await;
            emit_tool_activity_with_ts(
                emitter,
                task_id,
                tool_id,
                tool_name,
                &summary,
                "running",
                timestamp.as_deref(),
            )
            .await;
        }

        "tool_result" => {
            // Tool result -- emit tool activity with completed/error status
            let tool_id = parsed.get("tool_id").and_then(Value::as_str).unwrap_or("");
            let status_str = parsed.get("status").and_then(Value::as_str).unwrap_or("");
            let status = if status_str == "success" {
                "completed"
            } else {
                "error"
            };
            let output = parsed.get("output").and_then(Value::as_str);

            let summary_text = if status == "error" {
                output
                    .map(|o| truncate(o, 200).to_string())
                    .unwrap_or_else(|| "error".to_string())
            } else {
                "done".to_string()
            };

            if status == "error" {
                if let Some(out) = output {
                    emit_log(
                        emitter,
                        task_id,
                        "warn",
                        &format!("Tool error: {}", truncate(out, 500)),
                    )
                    .await;
                }
            } else {
                debug!(
                    task_id,
                    "Tool result ({} chars)",
                    output.map(|o| o.len()).unwrap_or(0)
                );
            }

            emit_tool_activity_with_ts(
                emitter,
                task_id,
                tool_id,
                "",
                &summary_text,
                status,
                timestamp.as_deref(),
            )
            .await;
        }

        "result" => {
            // Final result -- completion or error
            let result_status = parsed.get("status").and_then(Value::as_str);
            let error = parsed.get("error");
            let stats = parsed.get("stats");

            let mut meta_parts = Vec::new();
            if let Some(s) = stats {
                if let Some(tokens_in) = s.get("input_tokens").and_then(Value::as_i64) {
                    meta_parts.push(format!("in={tokens_in}"));
                }
                if let Some(tokens_out) = s.get("output_tokens").and_then(Value::as_i64) {
                    meta_parts.push(format!("out={tokens_out}"));
                }
            }

            let msg = if result_status == Some("error") {
                let error_msg = error
                    .and_then(|e| {
                        e.get("message")
                            .or_else(|| e.get("error"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("unknown error");
                let m = format!("Result: error -- {error_msg}");
                emit_log(emitter, task_id, "error", &m).await;
                m
            } else {
                let base = format!("Result: {}", result_status.unwrap_or("completed"));
                let m = if meta_parts.is_empty() {
                    base
                } else {
                    format!("{base} ({})", meta_parts.join(", "))
                };
                emit_log(emitter, task_id, "info", &m).await;
                m
            };

            emit_chat_message_with_ts(emitter, task_id, "system", &msg, timestamp.as_deref()).await;
        }

        _ => {
            // Unknown JSON event type -- check for message/content fields
            let message_text = parsed
                .get("message")
                .or_else(|| parsed.get("content"))
                .or_else(|| parsed.get("text"))
                .and_then(Value::as_str);

            if let Some(msg) = message_text {
                emit_log(
                    emitter,
                    task_id,
                    "info",
                    &format!("Agent: {}", truncate(msg, 1000)),
                )
                .await;
                emit_chat_message_with_ts(emitter, task_id, "assistant", msg, timestamp.as_deref())
                    .await;
            } else {
                debug!(
                    task_id,
                    "Gemini event ({event_type}): {}",
                    truncate(line, 200)
                );
            }
        }
    }
}

/// Extracts a human-readable summary from tool parameters.
fn extract_tool_summary(parameters: Option<&Value>) -> String {
    let Some(params) = parameters else {
        return String::new();
    };

    for key in &[
        "command",
        "file_path",
        "path",
        "pattern",
        "query",
        "file",
        "url",
    ] {
        if let Some(val) = params.get(key).and_then(Value::as_str) {
            return truncate(val, 200).to_string();
        }
    }

    String::new()
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

async fn emit_log(emitter: &SSEEmitter, task_id: &str, level: &str, message: &str) {
    emitter.emit_log(task_id, level, message, None).await;
}

async fn emit_chat_message(emitter: &SSEEmitter, task_id: &str, role: &str, content: &str) {
    emit_chat_message_with_ts(emitter, task_id, role, content, None).await;
}

async fn emit_chat_message_with_ts(
    emitter: &SSEEmitter,
    task_id: &str,
    role: &str,
    content: &str,
    timestamp: Option<&str>,
) {
    let ts = timestamp
        .map(String::from)
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let data = serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "role": role,
        "content": content,
        "timestamp": ts,
    });
    emitter
        .emit(
            task_id,
            SSEEvent {
                event_type: SSEEventType::ChatMessage,
                data,
            },
        )
        .await;
}

async fn emit_tool_activity_with_ts(
    emitter: &SSEEmitter,
    task_id: &str,
    id: &str,
    name: &str,
    summary: &str,
    status: &str,
    timestamp: Option<&str>,
) {
    let tool_id = if id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        id.to_string()
    };
    let ts = timestamp
        .map(String::from)
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let data = serde_json::json!({
        "id": tool_id,
        "name": name,
        "summary": summary,
        "status": status,
        "timestamp": ts,
    });
    emitter
        .emit(
            task_id,
            SSEEvent {
                event_type: SSEEventType::ToolActivity,
                data,
            },
        )
        .await;
}
