//! Local scan service for discovering git repositories on the filesystem.
//!
//! Port of `packages/server/src/services/local-scan.service.ts`.
//!
//! Scans a base directory (one level deep) for subdirectories containing a `.git`
//! folder, then inspects each to extract basic repository metadata.

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::services::stack_detector_service;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A locally discovered git repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalRepository {
    pub name: String,
    pub path: String,
    pub current_branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    pub remote_url: Option<String>,
    pub has_package_json: bool,
    pub language: Option<String>,
}

/// Response from scanning for local repositories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalReposResponse {
    pub repos: Vec<LocalRepository>,
    pub scan_path: String,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Scans the given base path (one level deep) for git repositories.
///
/// If `base_path` is `None`, falls back to the `LOCAL_SCAN_DIR` environment
/// variable, then auto-detects from the git root of the current/user directory.
pub async fn scan_for_repos(base_path: Option<&Path>) -> Result<LocalReposResponse, AppError> {
    let scan_path = match base_path {
        Some(p) => p.to_path_buf(),
        None => get_default_scan_path(),
    };

    info!(scan_path = %scan_path.display(), "Scanning for local repos");

    let mut repos = Vec::new();

    let mut entries = fs::read_dir(&scan_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read directory: {e}")))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read entry: {e}")))?
    {
        let file_type = match entry.file_type().await {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }

        let dir_path = entry.path();
        let git_path = dir_path.join(".git");

        // Check for .git directory
        if fs::metadata(&git_path).await.is_err() {
            continue;
        }

        match inspect_repo(&dir_path, &name_str).await {
            Ok(repo) => repos.push(repo),
            Err(e) => {
                warn!(
                    dir = %name_str,
                    error = %e,
                    "Failed to inspect repo"
                );
            }
        }
    }

    // Sort alphabetically
    repos.sort_by(|a, b| a.name.cmp(&b.name));

    let total = repos.len();
    let scan_path_str = scan_path.to_string_lossy().to_string();

    info!(scan_path = %scan_path_str, repos_found = total, "Scan complete");

    Ok(LocalReposResponse {
        repos,
        scan_path: scan_path_str,
        total,
    })
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Inspects a single git repository directory to extract metadata.
async fn inspect_repo(repo_path: &Path, name: &str) -> Result<LocalRepository, AppError> {
    // Get current branch
    let current_branch = sync_git_output(&["branch", "--show-current"], repo_path)
        .unwrap_or_else(|| "main".to_string());

    // Detect default branch (prefer remote HEAD)
    let default_branch = sync_git_output(&["symbolic-ref", "refs/remotes/origin/HEAD"], repo_path)
        .map(|head| head.replace("refs/remotes/origin/", ""));

    // Get remote URL
    let remote_url = sync_git_output(&["remote", "get-url", "origin"], repo_path);

    // Check for package.json
    let has_package_json = fs::metadata(repo_path.join("package.json")).await.is_ok();

    // Detect language
    let language = stack_detector_service::detect_language(repo_path, has_package_json).await;

    Ok(LocalRepository {
        name: name.to_string(),
        path: repo_path.to_string_lossy().to_string(),
        current_branch,
        default_branch,
        remote_url,
        has_package_json,
        language,
    })
}

/// Runs a synchronous git command and returns trimmed stdout, or `None` on failure.
///
/// We use `std::process::Command` (synchronous) for these one-shot queries because
/// they are fast and don't benefit from async I/O.
fn sync_git_output(args: &[&str], cwd: &Path) -> Option<String> {
    let mut cmd = StdCommand::new("git");
    cmd.args(args).current_dir(cwd);

    // On Windows, hide the console window
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

/// Determines the default scan path.
///
/// 1. Uses `LOCAL_SCAN_DIR` env var if set.
/// 2. Detects the git root of the current / user directory and goes one level up.
/// 3. Falls back to the current working directory.
fn get_default_scan_path() -> PathBuf {
    // Check env var
    if let Ok(dir) = std::env::var("LOCAL_SCAN_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }

    let base_dir = std::env::var("AGENT_BOARD_USER_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    info!(base_dir = %base_dir.display(), "Resolving default scan path");

    // Try to get git root
    let mut cmd = StdCommand::new("git");
    cmd.args(["rev-parse", "--show-toplevel"])
        .current_dir(&base_dir);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    if let Ok(output) = cmd.output() {
        if output.status.success() {
            let git_root = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !git_root.is_empty() {
                let git_root_path = PathBuf::from(&git_root);
                // Go one level up so sibling repos are visible
                if let Some(parent) = git_root_path.parent() {
                    info!(scan_path = %parent.display(), "Default scan path resolved from git root");
                    return parent.to_path_buf();
                }
            }
        }
    }

    debug!(fallback = %base_dir.display(), "Git root detection failed, using base dir");
    base_dir
}
