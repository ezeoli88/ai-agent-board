//! Edit file tool — exact string replacement in existing files.

use super::ToolResult;
use serde_json::Value;
use std::path::Path;
use tokio::fs;

pub fn definition() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "edit_file",
            "description": "Replace an exact string in a file. old_str must match exactly once (including whitespace/indentation). If old_str appears 0 or >1 times, the edit fails — add more surrounding context to make it unique. Preferred over write_file for modifying existing files.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to edit"
                    },
                    "old_str": {
                        "type": "string",
                        "description": "The exact string to find and replace. Must be unique in the file."
                    },
                    "new_str": {
                        "type": "string",
                        "description": "The replacement string"
                    }
                },
                "required": ["path", "old_str", "new_str"]
            }
        }
    })
}

pub async fn execute(args: Value, cwd: &Path) -> ToolResult {
    let raw_path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let old_str = args.get("old_str").and_then(|v| v.as_str()).unwrap_or("");
    let new_str = args.get("new_str").and_then(|v| v.as_str()).unwrap_or("");

    if raw_path.is_empty() {
        return ToolResult::err("Error: No path provided".into());
    }

    let path = resolve_path(cwd, raw_path);

    if !path.starts_with(cwd) {
        return ToolResult::err(format!("Error: Path escapes working directory: {raw_path}"));
    }

    if !path.exists() {
        return ToolResult::err(format!("Error: File not found: {raw_path}"));
    }

    let content = match fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) => return ToolResult::err(format!("Error reading file: {e}")),
    };

    let occurrences = content.matches(old_str).count();
    if occurrences == 0 {
        return ToolResult::err(format!("Error: old_str not found in {raw_path}"));
    }
    if occurrences > 1 {
        return ToolResult::err(format!(
            "Error: old_str found {occurrences} times in {raw_path}. It must be unique. Add more context to make it unique."
        ));
    }

    let new_content = content.replacen(old_str, new_str, 1);
    match fs::write(&path, &new_content).await {
        Ok(_) => ToolResult::ok(format!("File edited successfully: {raw_path}")),
        Err(e) => ToolResult::err(format!("Error writing file: {e}")),
    }
}

fn resolve_path(cwd: &Path, raw: &str) -> std::path::PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}
