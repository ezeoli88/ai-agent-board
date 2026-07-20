//! List directory contents tool.

use super::ToolResult;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn definition() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "list_directory",
            "description": "List directory contents with file sizes. Directories end with '/'. Default max_depth=1 (non-recursive). Set max_depth=2 or 3 to see nested structure.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list. Defaults to working directory."
                    },
                    "max_depth": {
                        "type": "number",
                        "description": "Maximum depth to recurse. Default 1 (non-recursive)."
                    }
                },
                "required": []
            }
        }
    })
}

pub async fn execute(args: Value, cwd: &Path) -> ToolResult {
    let dir = args
        .get("path")
        .and_then(|v| v.as_str())
        .map(|s| {
            let p = Path::new(s);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            }
        })
        .unwrap_or_else(|| cwd.to_path_buf());
    let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(1) as usize;

    let mut results = Vec::new();
    list_recursive(&dir, max_depth, 0, &mut results);

    if results.is_empty() {
        return ToolResult::ok("Directory is empty.".into());
    }

    ToolResult::ok(results.join("\n"))
}

fn list_recursive(dir: &Path, max_depth: usize, current_depth: usize, results: &mut Vec<String>) {
    if current_depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            results.push(format!("Error reading {}: {e}", dir.display()));
            return;
        }
    };

    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    let indent = "  ".repeat(current_depth);

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files at top level
        if name.starts_with('.') && current_depth == 0 {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            results.push(format!("{indent}{name}/"));
            if current_depth < max_depth {
                list_recursive(&entry.path(), max_depth, current_depth + 1, results);
            }
        } else {
            let size = entry
                .metadata()
                .map(|m| format_size(m.len()))
                .unwrap_or_default();
            results.push(format!("{indent}{name} ({size})"));
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
