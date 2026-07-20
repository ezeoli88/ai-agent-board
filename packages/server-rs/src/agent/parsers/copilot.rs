use serde_json::Value;
use tracing::debug;

use crate::utils::{SSEEmitter, SSEEvent, SSEEventType};

/// Parses a single line of Copilot CLI output.
///
/// Copilot CLI outputs both JSON (in some modes) and plain text terminal-formatted output.
/// This handles both formats:
///
/// JSON: `{ "type": "tool_use", "name": "Read", ... }`, `{ "type": "message", ... }`
///
/// Plain text patterns:
/// - `"* Read package.json"` -- tool start (bullet character)
/// - `"  + 1 line read"` -- tool result summary
/// - `"$ node --version"` -- command execution
pub async fn parse(line: &str, emitter: &SSEEmitter, task_id: &str) {
    // Try to parse as JSON first
    if let Ok(parsed) = serde_json::from_str::<Value>(line) {
        handle_json_output(&parsed, line, emitter, task_id).await;
        return;
    }

    // Handle plain text terminal output
    handle_text_output(line, emitter, task_id).await;
}

/// Handles JSON output from Copilot CLI.
async fn handle_json_output(parsed: &Value, raw_line: &str, emitter: &SSEEmitter, task_id: &str) {
    let event_type = parsed.get("type").and_then(Value::as_str).unwrap_or("");

    match event_type {
        "tool_use" => {
            let tool_name = parsed
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let tool_id = parsed.get("id").and_then(Value::as_str).unwrap_or("");
            let input = parsed.get("input");
            let summary = extract_tool_summary(input);

            emit_log(
                emitter,
                task_id,
                "info",
                &format!("Agent using tool: {tool_name}"),
            )
            .await;
            emit_tool_activity(emitter, task_id, tool_id, tool_name, &summary, "running").await;
        }

        "tool_result" => {
            let tool_use_id = parsed
                .get("tool_use_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let content = parsed.get("content").and_then(Value::as_str).unwrap_or("");
            let is_error = parsed
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let status = if is_error { "error" } else { "completed" };
            let summary = truncate(content, 200);

            emit_tool_activity(emitter, task_id, tool_use_id, "tool", summary, status).await;
            if !content.is_empty() {
                emit_log(
                    emitter,
                    task_id,
                    "info",
                    &format!("Tool result: {}", truncate(content, 500)),
                )
                .await;
            }
        }

        "message" => {
            if let Some(content) = parsed.get("content").and_then(Value::as_str) {
                emit_log(
                    emitter,
                    task_id,
                    "info",
                    &format!("Agent: {}", truncate(content, 1000)),
                )
                .await;
                emit_chat_message(emitter, task_id, "assistant", content).await;
            }
        }

        "result" => {
            if let Some(result) = parsed.get("result").and_then(Value::as_str) {
                emit_log(
                    emitter,
                    task_id,
                    "info",
                    &format!("Result: {}", truncate(result, 500)),
                )
                .await;
            }
        }

        _ => {
            // Log unknown JSON events as raw output
            debug!(
                task_id,
                "Unknown Copilot JSON event: {}",
                truncate(raw_line, 200)
            );
            emit_log(
                emitter,
                task_id,
                "info",
                &format!("CLI: {}", truncate(raw_line, 500)),
            )
            .await;
        }
    }
}

/// Handles plain text terminal output from Copilot CLI.
///
/// Format examples:
/// - `"* Read package.json"` -- tool start (bullet)
/// - `"  + 1 line read"` -- tool result summary (tree branch)
/// - `"$ node --version"` -- command execution
/// - `"X Edit index.js"` -- tool failure
async fn handle_text_output(line: &str, emitter: &SSEEmitter, task_id: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }

    // Match tool start: starts with bullet character followed by tool name + arg
    // Pattern: "* Read package.json" or unicode bullet
    if let Some(rest) = strip_bullet_prefix(trimmed) {
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() == 2 {
            let tool_name = parts[0];
            let tool_arg = truncate(parts[1], 100);
            emit_log(
                emitter,
                task_id,
                "info",
                &format!("Tool: {tool_name} {tool_arg}"),
            )
            .await;
            emit_tool_activity(emitter, task_id, "", tool_name, tool_arg, "running").await;
            return;
        }
    }

    // Match tool result summary: starts with tree branch character
    // Pattern: "  + 1 line read" or unicode tree branch
    if let Some(rest) = strip_tree_prefix(trimmed) {
        emit_log(emitter, task_id, "info", &format!("Tool result: {rest}")).await;
        emit_tool_activity(
            emitter,
            task_id,
            "",
            "tool",
            truncate(rest, 200),
            "completed",
        )
        .await;
        return;
    }

    // Match Bash tool: "* Bash ls -la"
    if let Some(rest) = strip_bullet_prefix(trimmed) {
        if let Some(cmd) = rest.strip_prefix("Bash ") {
            let command = truncate(cmd, 100);
            emit_log(emitter, task_id, "info", &format!("Tool: Bash {command}")).await;
            emit_tool_activity(emitter, task_id, "", "Bash", command, "running").await;
            return;
        }
    }

    // Match tool failure: starts with cross mark
    if let Some(rest) = strip_cross_prefix(trimmed) {
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() == 2 {
            let tool_name = parts[0];
            let tool_arg = truncate(parts[1], 100);
            emit_log(
                emitter,
                task_id,
                "warn",
                &format!("Tool failed: {tool_name} {tool_arg}"),
            )
            .await;
            emit_tool_activity(emitter, task_id, "", tool_name, tool_arg, "error").await;
            return;
        }
    }

    // Match tool with args on same line: "Read src/main.rs"
    static TOOL_NAMES: &[&str] = &[
        "Read", "Glob", "Grep", "Check", "List", "Create", "Edit", "Write", "Delete",
    ];
    for tool in TOOL_NAMES {
        if let Some(rest) = trimmed.strip_prefix(tool) {
            if rest.is_empty() {
                // Just the tool name alone -- skip, wait for args
                return;
            }
            if let Some(arg) = rest.strip_prefix(' ') {
                let tool_arg = truncate(arg, 100);
                emit_log(
                    emitter,
                    task_id,
                    "info",
                    &format!("Tool: {tool} {tool_arg}"),
                )
                .await;
                emit_tool_activity(emitter, task_id, "", tool, tool_arg, "running").await;
                return;
            }
        }
    }

    // Skip "tool" standalone
    if trimmed == "tool" {
        return;
    }

    // Match todo: "* Todo: Started: implement-change"
    if let Some(rest) = strip_bullet_prefix(trimmed) {
        if let Some(todo) = rest.strip_prefix("Todo: ") {
            emit_log(emitter, task_id, "info", &format!("Todo: {todo}")).await;
            return;
        }
    }

    // Match command execution: "$ node --version"
    if let Some(cmd) = trimmed.strip_prefix("$ ") {
        emit_log(emitter, task_id, "info", &format!("Running: {cmd}")).await;
        return;
    }

    // Match permission denied
    if trimmed.contains("Permission denied") {
        emit_log(emitter, task_id, "error", trimmed).await;
        return;
    }

    // Match confirmation prompts
    if trimmed.contains("Press")
        && (trimmed.contains("Enter") || trimmed.contains('y') || trimmed.contains("return"))
    {
        emit_log(
            emitter,
            task_id,
            "warn",
            &format!("CLI waiting for input: {trimmed}"),
        )
        .await;
        return;
    }

    // Match "would you like" prompts
    if trimmed.starts_with("?Would you like") || trimmed.starts_with("Would you like") {
        emit_log(
            emitter,
            task_id,
            "warn",
            &format!("CLI waiting for input: {trimmed}"),
        )
        .await;
        return;
    }

    // Match failed to apply patch
    if trimmed.contains("Failed to apply patch") {
        emit_log(emitter, task_id, "error", trimmed).await;
        return;
    }

    // Skip stats and decorative lines
    if trimmed.starts_with("Total usage")
        || trimmed.starts_with("API time spent")
        || trimmed.starts_with("Total session")
        || trimmed.starts_with("Total code changes")
        || trimmed.starts_with("Breakdown by")
        || trimmed.contains("```")
    {
        return;
    }

    // Log other lines as info
    emit_log(emitter, task_id, "info", &format!("CLI: {trimmed}")).await;
}

/// Strips a bullet prefix (Unicode bullet or ASCII '*') from a line.
fn strip_bullet_prefix(s: &str) -> Option<&str> {
    // Unicode bullet: \u{25cf} (black circle) or \u{2022} (bullet)
    s.strip_prefix('\u{25cf}')
        .or_else(|| s.strip_prefix('\u{2022}'))
        .or_else(|| s.strip_prefix('*'))
        .map(|rest| rest.trim_start())
}

/// Strips a tree branch prefix (Unicode or ASCII) from a line.
fn strip_tree_prefix(s: &str) -> Option<&str> {
    // Unicode: \u{2514} (box drawings light up and right)
    s.strip_prefix('\u{2514}')
        .or_else(|| s.strip_prefix('+'))
        .map(|rest| rest.trim_start())
}

/// Strips a cross/failure prefix (Unicode or ASCII 'X') from a line.
fn strip_cross_prefix(s: &str) -> Option<&str> {
    // Unicode: \u{2717} (ballot x)
    s.strip_prefix('\u{2717}')
        .or_else(|| s.strip_prefix('\u{2718}'))
        .or_else(|| s.strip_prefix('X'))
        .map(|rest| rest.trim_start())
}

/// Extracts a human-readable summary from tool input parameters.
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
