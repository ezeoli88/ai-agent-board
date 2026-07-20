use serde_json::Value;
use tracing::debug;

use crate::utils::{SSEEmitter, SSEEvent, SSEEventType};

/// Parses a single line of Claude Code `stream-json` output.
///
/// Claude Code emits JSON lines with a `type` field. Message types wrap
/// an API-style `message` object with `role` and `content` array:
///
/// - `{ "type": "assistant", "message": { "content": [{ "type": "text", "text": "..." }, { "type": "tool_use", "name": "Read", "input": {...} }] } }`
/// - `{ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "...", "content": "...", "is_error": false }] } }`
/// - `{ "type": "system", "model": "...", "session_id": "...", "tools": [...] }`
/// - `{ "type": "result", "result": "...", "cost_usd": 0.05, "duration_ms": 12000 }`
pub async fn parse(line: &str, emitter: &SSEEmitter, task_id: &str) {
    let parsed: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            // Not JSON -- log as raw output
            emit_log(
                emitter,
                task_id,
                "info",
                &format!("CLI: {}", truncate(line, 1000)),
            )
            .await;
            return;
        }
    };

    let event_type = parsed.get("type").and_then(Value::as_str).unwrap_or("");

    // Content blocks live inside message.content (stream-json format)
    let message = parsed.get("message");
    let content_blocks = message
        .and_then(|m| m.get("content"))
        .or_else(|| parsed.get("content"));

    match event_type {
        "assistant" => {
            // Assistant turn -- extract text and tool_use from content blocks
            if let Some(blocks) = content_blocks.and_then(Value::as_array) {
                for block in blocks {
                    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                    match block_type {
                        "text" => {
                            if let Some(text) = block.get("text").and_then(Value::as_str) {
                                emit_log(
                                    emitter,
                                    task_id,
                                    "info",
                                    &format!("Agent: {}", truncate(text, 1000)),
                                )
                                .await;
                                emit_chat_message(emitter, task_id, "assistant", text).await;
                            }
                        }
                        "tool_use" => {
                            let tool_name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown");
                            let tool_id = block.get("id").and_then(Value::as_str).unwrap_or("");
                            let summary = extract_tool_summary(block.get("input"));

                            emit_log(
                                emitter,
                                task_id,
                                "info",
                                &format!(
                                    "Tool: {tool_name}{}",
                                    if summary.is_empty() {
                                        String::new()
                                    } else {
                                        format!(": {summary}")
                                    }
                                ),
                            )
                            .await;
                            emit_tool_activity(
                                emitter, task_id, tool_id, tool_name, &summary, "running",
                            )
                            .await;
                        }
                        _ => {}
                    }
                }
            } else if let Some(text) = content_blocks.and_then(Value::as_str) {
                // Fallback for plain string content
                emit_log(
                    emitter,
                    task_id,
                    "info",
                    &format!("Agent: {}", truncate(text, 1000)),
                )
                .await;
                emit_chat_message(emitter, task_id, "assistant", text).await;
            }
        }

        "user" => {
            // User turn -- tool_result blocks. Only log errors to reduce noise.
            if let Some(blocks) = content_blocks.and_then(Value::as_array) {
                for block in blocks {
                    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                    if block_type == "tool_result" {
                        let tool_use_id = block
                            .get("tool_use_id")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let is_error = block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);

                        let (summary, status) = if is_error {
                            let err_content = block
                                .get("content")
                                .and_then(Value::as_str)
                                .map(|s| truncate(s, 200).to_string())
                                .unwrap_or_else(|| "error".to_string());
                            (err_content, "error")
                        } else {
                            ("done".to_string(), "completed")
                        };

                        emit_tool_activity(emitter, task_id, tool_use_id, "", &summary, status)
                            .await;

                        if is_error {
                            let err_content = block
                                .get("content")
                                .and_then(Value::as_str)
                                .map(|s| truncate(s, 500))
                                .unwrap_or("tool execution failed");
                            emit_log(
                                emitter,
                                task_id,
                                "warn",
                                &format!("Tool error: {err_content}"),
                            )
                            .await;
                        }
                    }
                }
            }
        }

        "system" => {
            // System init -- extract useful metadata
            let model = parsed.get("model").and_then(Value::as_str);
            let session_id = parsed.get("session_id").and_then(Value::as_str);
            let tools_count = parsed
                .get("tools")
                .and_then(Value::as_array)
                .map(|a| a.len());

            let mut parts = Vec::new();
            if let Some(m) = model {
                parts.push(format!("model={m}"));
            }
            if let Some(s) = session_id {
                parts.push(format!("session={}", truncate(s, 8)));
            }
            if let Some(count) = tools_count {
                parts.push(format!("tools={count}"));
            }

            if !parts.is_empty() {
                let info_msg = format!("System: {}", parts.join(", "));
                emit_log(emitter, task_id, "info", &info_msg).await;
                emit_chat_message(
                    emitter,
                    task_id,
                    "system",
                    &format!("System initialized with {}", parts.join(", ")),
                )
                .await;
            }
        }

        "tool_use" => {
            // Standalone tool_use event (alternative format)
            let tool_name = parsed
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let summary = extract_tool_summary(parsed.get("input"));
            emit_log(
                emitter,
                task_id,
                "info",
                &format!(
                    "Tool: {tool_name}{}",
                    if summary.is_empty() {
                        String::new()
                    } else {
                        format!(": {summary}")
                    }
                ),
            )
            .await;
        }

        "tool_result" => {
            // Standalone tool_result event
            if let Some(output) = parsed.get("output").and_then(Value::as_str) {
                debug!(task_id, "Tool result: {}", truncate(output, 500));
            }
        }

        "result" => {
            // Final result with optional cost/duration info
            let result_text = parsed.get("result").and_then(Value::as_str);
            let cost_usd = parsed.get("cost_usd").and_then(Value::as_f64);
            let duration_ms = parsed.get("duration_ms").and_then(Value::as_f64);

            let base_msg = result_text
                .map(|r| format!("Result: {}", truncate(r, 1000)))
                .unwrap_or_else(|| "Result: completed".to_string());

            let mut meta_parts = Vec::new();
            if let Some(cost) = cost_usd {
                meta_parts.push(format!("${cost:.4}"));
            }
            if let Some(duration) = duration_ms {
                meta_parts.push(format!("{:.1}s", duration / 1000.0));
            }

            let msg = if meta_parts.is_empty() {
                base_msg
            } else {
                format!("{base_msg} ({})", meta_parts.join(", "))
            };

            emit_log(emitter, task_id, "info", &msg).await;

            let chat_content = if meta_parts.is_empty() {
                "Completed".to_string()
            } else {
                format!("Completed ({})", meta_parts.join(", "))
            };
            emit_chat_message(emitter, task_id, "system", &chat_content).await;
        }

        "error" => {
            let error_msg = parsed
                .get("error")
                .or_else(|| parsed.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            emit_log(
                emitter,
                task_id,
                "error",
                &format!("CLI error: {error_msg}"),
            )
            .await;
        }

        _ => {
            // Unknown event types -- debug level to reduce noise
            debug!(
                task_id,
                event_type,
                "Unknown Claude Code event: {}",
                truncate(line, 200)
            );
        }
    }
}

/// Extracts a human-readable summary from a tool's input parameters.
fn extract_tool_summary(input: Option<&Value>) -> String {
    let Some(input) = input else {
        return String::new();
    };

    for key in &["file_path", "command", "pattern", "query", "path"] {
        if let Some(val) = input.get(key).and_then(Value::as_str) {
            return truncate(val, 200).to_string();
        }
    }

    String::new()
}

/// Truncates a string to at most `max_len` characters.
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find a valid UTF-8 boundary
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

/// Emits a log SSE event.
async fn emit_log(emitter: &SSEEmitter, task_id: &str, level: &str, message: &str) {
    emitter.emit_log(task_id, level, message, None).await;
}

/// Emits a chat message SSE event.
async fn emit_chat_message(emitter: &SSEEmitter, task_id: &str, role: &str, content: &str) {
    let data = serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "role": role,
        "content": content,
        "timestamp": chrono::Utc::now().to_rfc3339(),
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

/// Emits a tool activity SSE event.
async fn emit_tool_activity(
    emitter: &SSEEmitter,
    task_id: &str,
    id: &str,
    name: &str,
    summary: &str,
    status: &str,
) {
    let tool_id = if id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        id.to_string()
    };

    let data = serde_json::json!({
        "id": tool_id,
        "name": name,
        "summary": summary,
        "status": status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
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
