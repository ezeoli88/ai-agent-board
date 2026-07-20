//! Write file tool — creates or overwrites a file.

use super::ToolResult;
use serde_json::Value;
use std::path::Path;
use tokio::fs;

pub fn definition() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Create or overwrite a file with the given content. Creates parent directories automatically. WARNING: Completely replaces existing content. For partial edits use edit_file instead.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }
        }
    })
}

pub async fn execute(args: Value, cwd: &Path) -> ToolResult {
    let raw_path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

    if raw_path.is_empty() {
        return ToolResult::err("Error: No path provided".into());
    }

    let path = resolve_path(cwd, raw_path);

    // Path traversal check
    if !path.starts_with(cwd) {
        return ToolResult::err(format!("Error: Path escapes working directory: {raw_path}"));
    }

    // Create parent directories
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return ToolResult::err(format!("Error creating directories: {e}"));
            }
        }
    }

    match fs::write(&path, content).await {
        Ok(_) => ToolResult::ok(format!("File written successfully: {raw_path}")),
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
