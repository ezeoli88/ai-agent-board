//! Grep tool — search file contents by regex.
//!
//! Uses `regex_lite` + `walkdir` instead of `grep-*` crates to save ~2MB binary size.

use super::ToolResult;
use regex_lite::Regex;
use serde_json::Value;
use std::path::Path;
use walkdir::WalkDir;

pub fn definition() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "grep",
            "description": "Search file contents by regex. Returns 'path:line: content' per match. Max 200 matches. Skips node_modules and dotfiles. Use 'include' to filter by extension, e.g., include='*.ts'. Use context_lines for surrounding context.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search in. Defaults to working directory."
                    },
                    "include": {
                        "type": "string",
                        "description": "File extension filter (e.g., \"*.ts\", \"*.tsx\")"
                    },
                    "context_lines": {
                        "type": "number",
                        "description": "Number of context lines before and after each match. Default 0."
                    }
                },
                "required": ["pattern"]
            }
        }
    })
}

pub async fn execute(args: Value, cwd: &Path) -> ToolResult {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
    let search_path = args
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
    let include = args.get("include").and_then(|v| v.as_str());
    let context_lines = args
        .get("context_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    if pattern.is_empty() {
        return ToolResult::err("Error: No pattern provided".into());
    }

    let re = match Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return ToolResult::err(format!("Error: Invalid regex pattern: {e}")),
    };

    // Collect files to search
    let files = collect_files(&search_path, include);

    let mut results: Vec<String> = Vec::new();
    let mut match_count = 0;
    let max_matches = 200;

    for file_path in &files {
        if match_count >= max_matches {
            results.push("...(truncated at 200 matches)".to_string());
            break;
        }

        let path = Path::new(file_path);
        let rel = path
            .strip_prefix(cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let lines: Vec<&str> = content.split('\n').collect();

        if context_lines > 0 {
            for (i, line) in lines.iter().enumerate() {
                if match_count >= max_matches {
                    break;
                }
                if re.is_match(line) {
                    match_count += 1;
                    results.push(format!("--- {rel} ---"));
                    let start = i.saturating_sub(context_lines);
                    let end = (i + context_lines).min(lines.len().saturating_sub(1));
                    for j in start..=end {
                        let prefix = if j == i { ">" } else { " " };
                        results.push(format!("{prefix} {}: {}", j + 1, lines[j]));
                    }
                    results.push(String::new());
                }
            }
        } else {
            for (i, line) in lines.iter().enumerate() {
                if match_count >= max_matches {
                    break;
                }
                if re.is_match(line) {
                    match_count += 1;
                    results.push(format!("{rel}:{}: {}", i + 1, line.trim_end()));
                }
            }
        }
    }

    if results.is_empty() {
        return ToolResult::ok("No matches found.".into());
    }

    let output = results.join("\n");
    let max_len = 10_000;
    if output.len() > max_len {
        ToolResult::ok(format!("{}...(truncated)", &output[..max_len]))
    } else {
        ToolResult::ok(output)
    }
}

fn collect_files(dir: &Path, include: Option<&str>) -> Vec<String> {
    let ext_filter: Option<String> = include.map(|inc| inc.replace('*', ""));

    if dir.is_file() {
        return vec![dir.to_string_lossy().to_string()];
    }

    WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') && name != "node_modules" && name != "target"
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            if let Some(ref ext) = ext_filter {
                e.file_name().to_string_lossy().ends_with(ext.as_str())
            } else {
                true
            }
        })
        .map(|e| e.path().to_string_lossy().to_string())
        .collect()
}
