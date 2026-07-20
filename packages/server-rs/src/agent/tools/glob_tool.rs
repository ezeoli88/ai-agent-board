//! Glob pattern file search tool.

use super::ToolResult;
use globset::Glob;
use serde_json::Value;
use std::path::Path;
use walkdir::WalkDir;

pub fn definition() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "glob",
            "description": "Find files by glob pattern. Returns one path per line. Max 500 results. Ignores dotfiles and node_modules. Examples: '**/*.ts' for all TypeScript files, 'src/**/*.test.ts' for test files in src.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match (e.g., \"**/*.ts\", \"src/**/*.tsx\")"
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Directory to search in. Defaults to working directory."
                    }
                },
                "required": ["pattern"]
            }
        }
    })
}

pub async fn execute(args: Value, cwd: &Path) -> ToolResult {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
    let search_dir = args
        .get("cwd")
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

    if pattern.is_empty() {
        return ToolResult::err("Error: No pattern provided".into());
    }

    let glob = match Glob::new(pattern) {
        Ok(g) => g.compile_matcher(),
        Err(e) => return ToolResult::err(format!("Error: Invalid glob pattern: {e}")),
    };

    let base = &search_dir;
    let mut results = Vec::new();

    for entry in WalkDir::new(base)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') && name != "node_modules" && name != "target"
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_dir() {
            continue;
        }

        let rel = match entry.path().strip_prefix(base) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        // Normalize path separators for matching
        let rel_normalized = rel.replace('\\', "/");
        if glob.is_match(&rel_normalized) {
            results.push(rel_normalized);
            if results.len() >= 500 {
                results.push("...(truncated at 500 results)".to_string());
                break;
            }
        }
    }

    if results.is_empty() {
        return ToolResult::ok("No files matched the pattern.".into());
    }

    ToolResult::ok(results.join("\n"))
}
