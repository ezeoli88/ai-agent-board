//! Bash/shell command execution tool.

use super::ToolResult;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

pub fn definition() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "bash",
            "description": "Execute a bash command. Use for: running scripts, git operations, installing packages, or any terminal task. Timeout: 30s. Output truncated at 10KB. Prefer other tools over bash when possible (e.g., use read_file instead of cat, glob instead of find).",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The bash command to execute"
                    }
                },
                "required": ["command"]
            }
        }
    })
}

pub async fn execute(args: Value, cwd: &Path) -> ToolResult {
    let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");

    if command.is_empty() {
        return ToolResult::err("Error: No command provided".into());
    }

    match run_command_with_platform_shell(command, cwd).await {
        Ok(output) => ToolResult::ok(format_output(output)),
        Err(e) => ToolResult::err(format!("Error executing command: {e}")),
    }
}

async fn run_command_with_platform_shell(
    command: &str,
    cwd: &Path,
) -> Result<std::process::Output, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(git_bash) = find_windows_git_bash() {
            match run_with_timeout(&git_bash, &["-lc", command], cwd).await {
                Ok(output) => return Ok(output),
                Err(e)
                    if e.contains("cannot find the file specified") || e.contains("not found") => {}
                Err(e) => return Err(e),
            }
        }

        match run_with_timeout("powershell", &["-NoProfile", "-Command", command], cwd).await {
            Ok(output) => return Ok(output),
            Err(e) if e.contains("cannot find the file specified") || e.contains("not found") => {}
            Err(e) => return Err(e),
        }

        match run_with_timeout("pwsh", &["-NoProfile", "-Command", command], cwd).await {
            Ok(output) => return Ok(output),
            Err(e) if e.contains("cannot find the file specified") || e.contains("not found") => {}
            Err(e) => return Err(e),
        }

        return run_with_timeout("cmd", &["/C", command], cwd).await;
    }

    #[cfg(not(target_os = "windows"))]
    {
        run_with_timeout("bash", &["-lc", command], cwd).await
    }
}

#[cfg(target_os = "windows")]
fn find_windows_git_bash() -> Option<String> {
    use std::path::PathBuf;

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        candidates.push(PathBuf::from(pf).join("Git\\usr\\bin\\bash.exe"));
    }
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        candidates.push(PathBuf::from(pf86).join("Git\\usr\\bin\\bash.exe"));
    }
    candidates.push(PathBuf::from(r"C:\Program Files\Git\usr\bin\bash.exe"));
    candidates.push(PathBuf::from(
        r"C:\Program Files (x86)\Git\usr\bin\bash.exe",
    ));

    candidates
        .into_iter()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
}

async fn run_with_timeout(
    program: &str,
    args: &[&str],
    cwd: &Path,
) -> Result<std::process::Output, String> {
    let run = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new(program).args(args).current_dir(cwd).output(),
    )
    .await;

    match run {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("Command timed out after 30 seconds".into()),
    }
}

fn format_output(output: std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    let max_len = 10_000;
    let stdout_trunc = if stdout.len() > max_len {
        format!("{}...(truncated)", &stdout[..max_len])
    } else {
        stdout.to_string()
    };
    let stderr_trunc = if stderr.len() > max_len {
        format!("{}...(truncated)", &stderr[..max_len])
    } else {
        stderr.to_string()
    };

    let mut result = String::new();
    if !stdout_trunc.is_empty() {
        result.push_str(&stdout_trunc);
    }
    if !stderr_trunc.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("stderr: ");
        result.push_str(&stderr_trunc);
    }
    if exit_code != 0 {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&format!("Exit code: {exit_code}"));
    }
    if result.is_empty() {
        result = "(no output)".to_string();
    }

    result
}
