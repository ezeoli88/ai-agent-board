//! Git service for managing bare repositories and worktrees.
//!
//! Port of `packages/server/src/services/git.service.ts` (~1960 LoC).
//!
//! This service enables isolated execution environments for tasks by managing:
//! - Bare repository clones (shared across tasks for the same repo)
//! - Git worktrees (one per task, branched off the bare repo)
//! - Per-repo locking to prevent concurrent `git` operations on the same bare repo
//!
//! On Windows the git child process is spawned with `CREATE_NO_WINDOW` to avoid
//! flashing console windows.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::process::Command;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Result of executing a git command.
#[derive(Debug, Clone)]
pub struct GitCommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Information about an active worktree tracked in memory.
#[derive(Debug, Clone)]
struct WorktreeInfo {
    #[allow(dead_code)]
    task_id: String,
    worktree_path: PathBuf,
    branch_name: String,
    bare_repo_path: PathBuf,
}

/// Information about a changed file in a worktree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub status: ChangeStatus,
    pub additions: i64,
    pub deletions: i64,
    /// Content of the file in the base branch (None for binary files or if too large).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_content: Option<String>,
    /// Content of the file in the current worktree (None for binary files or if too large).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_content: Option<String>,
}

/// File change status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
}

/// Result of the `setup_worktree` operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupWorktreeResult {
    /// The path to the worktree.
    pub worktree_path: PathBuf,
    /// Whether an existing worktree was reused.
    pub reused: bool,
    /// The branch name.
    pub branch_name: String,
    /// The resolved base branch used to prepare the worktree.
    pub target_branch: String,
    /// Whether the repository is empty (no commits).
    pub is_empty_repo: bool,
}

/// Maximum file size in bytes for including content in diff (100 KB).
const MAX_CONTENT_SIZE: usize = 100 * 1024;

/// UUID v4 regex pattern for validation.
fn is_valid_uuid(s: &str) -> bool {
    // Simple check: 8-4-4-4-12 hex chars
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let lengths = [8, 4, 4, 4, 12];
    for (part, &expected_len) in parts.iter().zip(lengths.iter()) {
        if part.len() != expected_len || !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
    }
    true
}

/// Validates that a task ID is a valid UUID format (prevents path traversal).
fn validate_task_id(task_id: &str) -> Result<(), AppError> {
    if !is_valid_uuid(task_id) {
        return Err(AppError::Validation("Invalid task ID format".into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Git Service
// ---------------------------------------------------------------------------

/// Git service for managing bare repositories and worktrees.
pub struct GitService {
    config: Arc<Config>,
    /// Per-repo locks to prevent concurrent bare repo operations (e.g., index.lock).
    repo_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// In-memory map of active worktrees keyed by task ID.
    active_worktrees: Arc<RwLock<HashMap<String, WorktreeInfo>>>,
}

impl GitService {
    /// Creates a new `GitService`.
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            repo_locks: Mutex::new(HashMap::new()),
            active_worktrees: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // -----------------------------------------------------------------------
    // Per-repo locking
    // -----------------------------------------------------------------------

    /// Acquires a per-repo lock and runs the provided async closure under it.
    async fn with_repo_lock<F, Fut, R>(&self, key: &str, f: F) -> Result<R, AppError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<R, AppError>>,
    {
        let lock = {
            let mut locks = self.repo_locks.lock().await;
            locks
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;
        f().await
    }

    // -----------------------------------------------------------------------
    // Git command execution
    // -----------------------------------------------------------------------

    /// Executes a git command and returns the result (stdout, stderr, exit code).
    ///
    /// On Windows, sets `CREATE_NO_WINDOW` to avoid flashing console windows.
    pub async fn exec_git(
        args: &[&str],
        cwd: &Path,
        env: Option<&HashMap<String, String>>,
    ) -> Result<GitCommandResult, AppError> {
        Self::exec_git_with_configs(args, cwd, env, &[]).await
    }

    async fn exec_git_with_configs(
        args: &[&str],
        cwd: &Path,
        env: Option<&HashMap<String, String>>,
        configs: &[String],
    ) -> Result<GitCommandResult, AppError> {
        let mut cmd = Command::new("git");
        for config in configs {
            cmd.arg("-c").arg(config);
        }
        cmd.args(args).current_dir(cwd);

        // On Windows, hide the console window to avoid flashing consoles.
        // `tokio::process::Command` exposes `creation_flags` via the Windows
        // `CommandExt` trait.
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to execute git: {e}")))?;

        Ok(GitCommandResult {
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            exit_code: output.status.code().unwrap_or(1),
        })
    }

    /// Executes a git command and returns stdout. Returns an error if the exit code is non-zero.
    pub async fn exec_git_or_throw(
        args: &[&str],
        cwd: &Path,
        env: Option<&HashMap<String, String>>,
    ) -> Result<String, AppError> {
        let result = Self::exec_git(args, cwd, env).await?;
        if result.exit_code != 0 {
            return Err(AppError::Internal(anyhow::anyhow!(
                "Git command failed: git {}\nStderr: {}",
                args.join(" "),
                result.stderr
            )));
        }
        Ok(result.stdout)
    }

    async fn exec_git_or_throw_with_configs(
        args: &[&str],
        cwd: &Path,
        env: Option<&HashMap<String, String>>,
        configs: &[String],
    ) -> Result<String, AppError> {
        let result = Self::exec_git_with_configs(args, cwd, env, configs).await?;
        if result.exit_code != 0 {
            return Err(AppError::Internal(anyhow::anyhow!(
                "Git command failed: git {}\nStderr: {}",
                args.join(" "),
                result.stderr
            )));
        }
        Ok(result.stdout)
    }

    // -----------------------------------------------------------------------
    // Directory helpers
    // -----------------------------------------------------------------------

    async fn ensure_dir(dir_path: &Path) -> Result<(), AppError> {
        fs::create_dir_all(dir_path)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create directory: {e}")))?;
        Ok(())
    }

    async fn directory_exists(dir_path: &Path) -> bool {
        fs::metadata(dir_path)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
    }

    /// Removes a directory with retry logic for EBUSY errors (common on Windows).
    async fn remove_directory_with_retry(
        dir_path: &Path,
        max_retries: u32,
        base_delay_ms: u64,
    ) -> Result<(), AppError> {
        let mut last_error: Option<std::io::Error> = None;

        for attempt in 0..=max_retries {
            match fs::remove_dir_all(dir_path).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // Only retry for specific error kinds that indicate locked files
                    let should_retry = matches!(
                        e.kind(),
                        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Other
                    ) || e.raw_os_error().map_or(false, |code| {
                        // EBUSY on Unix, ERROR_SHARING_VIOLATION on Windows, etc.
                        code == 16 || code == 32 || code == 145
                    });

                    if !should_retry {
                        return Err(AppError::Internal(anyhow::anyhow!(
                            "Failed to remove directory {}: {e}",
                            dir_path.display()
                        )));
                    }

                    last_error = Some(e);

                    if attempt < max_retries {
                        let jitter = rand::random::<u64>() % 200;
                        let delay_ms = base_delay_ms * 2u64.pow(attempt) + jitter;
                        debug!(
                            dir = %dir_path.display(),
                            attempt = attempt + 1,
                            delay_ms,
                            "Directory removal failed, retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        Err(AppError::Internal(anyhow::anyhow!(
            "Failed to remove directory after {max_retries} retries: {}: {}",
            dir_path.display(),
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown error".into())
        )))
    }

    // -----------------------------------------------------------------------
    // URL helpers
    // -----------------------------------------------------------------------

    /// Converts a repo URL to a directory-safe name.
    fn repo_url_to_dir(repo_url: &str) -> String {
        let clean = repo_url
            .replace("https://", "")
            .replace("http://", "")
            .replace("git@", "")
            .replace(':', "/")
            .replace(".git", "");
        clean.replace('/', "_")
    }

    /// Checks if a URL is a local file:// URL.
    fn is_local_repo_url(url: &str) -> bool {
        url.starts_with("file://") || url.starts_with("file:\\\\")
    }

    /// Extracts the local path from a file:// URL.
    fn local_repo_path(url: &str) -> String {
        url.trim_start_matches("file://")
            .trim_start_matches("file:\\\\")
            .to_string()
    }

    fn local_repo_safe_directory_configs(source_path: &Path) -> Vec<String> {
        vec![
            format!("safe.directory={}", source_path.to_string_lossy()),
            format!(
                "safe.directory={}",
                source_path.join(".git").to_string_lossy()
            ),
        ]
    }

    /// Checks if a URL is a GitLab URL.
    fn is_gitlab_url(url: &str) -> bool {
        url.to_lowercase().contains("gitlab.com")
    }

    /// Converts a remote URL to an authenticated clone URL with an embedded token.
    fn to_authenticated_clone_url(repo_url: &str, token: &str) -> String {
        if Self::is_gitlab_url(repo_url) {
            // GitLab uses oauth2:TOKEN format
            let clean = repo_url
                .replace("https://", "")
                .replace("http://", "")
                .trim_end_matches('/')
                .to_string();
            let clean = if clean.ends_with(".git") {
                clean
            } else {
                format!("{clean}.git")
            };
            format!("https://oauth2:{token}@{clean}")
        } else {
            // GitHub uses x-access-token:TOKEN format
            let clean = repo_url
                .replace("https://", "")
                .replace("http://", "")
                .trim_end_matches('/')
                .to_string();
            let clean = if clean.ends_with(".git") {
                clean
            } else {
                format!("{clean}.git")
            };
            format!("https://x-access-token:{token}@{clean}")
        }
    }

    /// Checks whether a git remote value points to a local filesystem path.
    fn is_local_path_remote(remote_url: &str) -> bool {
        let value = remote_url.trim();
        if value.is_empty() {
            return false;
        }
        if Self::is_local_repo_url(value) {
            return true;
        }
        // Windows absolute path: C:\repo or C:/repo
        if value.len() >= 3 && value.as_bytes()[1] == b':' {
            let first = value.as_bytes()[0];
            if first.is_ascii_alphabetic() {
                let sep = value.as_bytes()[2];
                if sep == b'\\' || sep == b'/' {
                    return true;
                }
            }
        }
        // UNC path: \\server\share
        if value.starts_with("\\\\") {
            return true;
        }
        // Relative path: ./repo or ../repo
        if value.starts_with("./") || value.starts_with("../") || value == "." || value == ".." {
            return true;
        }
        // Unix absolute path
        if value.starts_with('/') {
            return true;
        }
        false
    }

    // -----------------------------------------------------------------------
    // Bare repo management
    // -----------------------------------------------------------------------

    /// Checks if a bare repository is empty (has no commits).
    pub async fn is_empty_repo(&self, bare_repo_path: &Path) -> Result<bool, AppError> {
        let result = Self::exec_git(&["branch", "--list"], bare_repo_path, None).await?;
        if result.exit_code == 0 && result.stdout.is_empty() {
            let rev_result =
                Self::exec_git(&["rev-parse", "--verify", "HEAD"], bare_repo_path, None).await?;
            if rev_result.exit_code != 0 {
                info!(bare_repo = %bare_repo_path.display(), "Repository is empty (no commits)");
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Ensures the bare repository exists, cloning if not present.
    /// Uses per-repo locking to prevent duplicate clones.
    pub async fn ensure_bare_repo(&self, repo_url: &str) -> Result<PathBuf, AppError> {
        let dir_name = Self::repo_url_to_dir(repo_url);
        let bare_repo_path = self.config.repos_base_dir.join(format!("{dir_name}.git"));

        debug!(repo_url, bare_repo = %bare_repo_path.display(), "Ensuring bare repo exists");

        Self::ensure_dir(&self.config.repos_base_dir).await?;

        // Fast path: bare repo already exists
        if Self::directory_exists(&bare_repo_path).await {
            info!(bare_repo = %bare_repo_path.display(), "Bare repo already exists");
            return Ok(bare_repo_path);
        }

        let bare_path = bare_repo_path.clone();
        let repo_url_owned = repo_url.to_string();
        let repos_base = self.config.repos_base_dir.clone();

        self.with_repo_lock(&bare_path.to_string_lossy(), || {
            let bare_path = bare_path.clone();
            let repo_url_owned = repo_url_owned.clone();
            let repos_base = repos_base.clone();
            async move {
                // Re-check after acquiring lock
                if Self::directory_exists(&bare_path).await {
                    info!(bare_repo = %bare_path.display(), "Bare repo created by another task while waiting for lock");
                    return Ok(bare_path);
                }

                info!(repo_url = %repo_url_owned, bare_repo = %bare_path.display(), "Cloning bare repository");

                let clone_url = if Self::is_local_repo_url(&repo_url_owned) {
                    Self::local_repo_path(&repo_url_owned)
                } else {
                    // For remote repos, use the URL as-is (no token embedding at clone time in this simplified version)
                    repo_url_owned.clone()
                };
                let git_configs = if Self::is_local_repo_url(&repo_url_owned) {
                    Self::local_repo_safe_directory_configs(Path::new(&clone_url))
                } else {
                    vec![]
                };

                let bare_path_str = bare_path.to_string_lossy().to_string();
                match Self::exec_git_or_throw_with_configs(
                    &["clone", "--bare", &clone_url, &bare_path_str],
                    &repos_base,
                    None,
                    &git_configs,
                )
                .await
                {
                    Ok(_) => {
                        info!(bare_repo = %bare_path.display(), "Bare repository cloned successfully");

                        // For local repos, copy the source remote to the bare repo
                        if Self::is_local_repo_url(&repo_url_owned) {
                            let source = Self::local_repo_path(&repo_url_owned);
                            Self::copy_source_remote_to_bare_repo(
                                Path::new(&source),
                                &bare_path,
                            )
                            .await;
                        }

                        Ok(bare_path)
                    }
                    Err(e) => {
                        // Clean up partial clone on failure
                        let _ = fs::remove_dir_all(&bare_path).await;
                        Err(e)
                    }
                }
            }
        })
        .await
    }

    /// Copies the source repo's remote origin URL to a bare repo so that push/PR
    /// operations target the correct host instead of a local path.
    async fn copy_source_remote_to_bare_repo(source_path: &Path, bare_repo_path: &Path) {
        let git_configs = Self::local_repo_safe_directory_configs(source_path);
        let result = match Self::exec_git_with_configs(
            &["remote", "get-url", "origin"],
            source_path,
            None,
            &git_configs,
        )
        .await
        {
            Ok(r) => r,
            Err(_) => return,
        };
        if result.exit_code == 0 && !result.stdout.is_empty() {
            let source_remote = &result.stdout;
            if source_remote.contains("github.com")
                || source_remote.contains("gitlab.com")
                || source_remote.starts_with("https://")
                || source_remote.starts_with("git@")
            {
                let _ = Self::exec_git_or_throw(
                    &["remote", "set-url", "origin", source_remote],
                    bare_repo_path,
                    None,
                )
                .await;
                info!(source_remote, bare_repo = %bare_repo_path.display(), "Updated bare repo origin to source remote");
            }
        }
    }

    /// Fetches the latest changes from origin in a bare repository.
    pub async fn fetch_repo(
        &self,
        bare_repo_path: &Path,
        target_branch: Option<&str>,
        repo_url: Option<&str>,
    ) -> Result<(), AppError> {
        debug!(bare_repo = %bare_repo_path.display(), ?target_branch, "Fetching latest changes");

        let bare_path = bare_repo_path.to_path_buf();
        let target_branch_owned = target_branch.map(|s| s.to_string());
        let repo_url_owned = repo_url.map(|s| s.to_string());

        self.with_repo_lock(&bare_path.to_string_lossy(), || {
            let bare_path = bare_path.clone();
            let target_branch_owned = target_branch_owned.clone();
            let repo_url_owned = repo_url_owned.clone();
            async move {
                // Check if repository is empty
                let is_empty = {
                    let result = Self::exec_git(&["branch", "--list"], &bare_path, None).await?;
                    if result.exit_code == 0 && result.stdout.is_empty() {
                        let rev = Self::exec_git(
                            &["rev-parse", "--verify", "HEAD"],
                            &bare_path,
                            None,
                        )
                        .await?;
                        rev.exit_code != 0
                    } else {
                        false
                    }
                };
                if is_empty {
                    info!(bare_repo = %bare_path.display(), "Skipping fetch for empty repository");
                    return Ok(());
                }

                // For local repos, fetch the target branch directly
                if let Some(ref url) = repo_url_owned {
                    if Self::is_local_repo_url(url) {
                        let repo_path = Self::local_repo_path(url);
                        let git_configs =
                            Self::local_repo_safe_directory_configs(Path::new(&repo_path));
                        if let Some(ref branch) = target_branch_owned {
                            let refspec = format!("{branch}:{branch}");
                            let _ = Self::exec_git_with_configs(
                                &["fetch", &repo_path, &refspec, "--force"],
                                &bare_path,
                                None,
                                &git_configs,
                            )
                            .await;
                            debug!(target_branch = %branch, "Fetched local repo branch");
                        }
                        info!(bare_repo = %bare_path.display(), "Local repository fetched successfully");
                        return Ok(());
                    }
                }

                // Remote fetch
                let mut env_map = HashMap::new();
                let git_username = if repo_url_owned
                    .as_deref()
                    .map_or(false, Self::is_gitlab_url)
                {
                    "oauth2"
                } else {
                    "x-access-token"
                };
                // Note: credential injection via GIT_ASKPASS would require a temp script.
                // For now, we rely on the authenticated clone URL or git credential helpers.
                env_map.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
                let _ = env_map.insert("GIT_USERNAME".into(), git_username.into());

                Self::exec_git_or_throw(
                    &["fetch", "origin", "--prune"],
                    &bare_path,
                    Some(&env_map),
                )
                .await?;

                if let Some(ref branch) = target_branch_owned {
                    let refspec = format!("{branch}:{branch}");
                    match Self::exec_git_or_throw(
                        &["fetch", "origin", &refspec, "--force"],
                        &bare_path,
                        Some(&env_map),
                    )
                    .await
                    {
                        Ok(_) => debug!(target_branch = %branch, "Updated local branch from origin"),
                        Err(e) => warn!(
                            target_branch = %branch,
                            error = %e,
                            "Could not update local branch ref, will use existing"
                        ),
                    }
                }

                info!(bare_repo = %bare_path.display(), "Repository fetched successfully");
                Ok(())
            }
        })
        .await
    }

    // -----------------------------------------------------------------------
    // Target branch resolution
    // -----------------------------------------------------------------------

    /// Validates that the target branch exists in the bare repo.
    /// If not, auto-detects the actual default branch.
    async fn resolve_target_branch(
        bare_repo_path: &Path,
        requested_branch: &str,
    ) -> Result<String, AppError> {
        let check = Self::exec_git(
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{requested_branch}"),
            ],
            bare_repo_path,
            None,
        )
        .await?;
        if check.exit_code == 0 {
            return Ok(requested_branch.to_string());
        }

        warn!(
            requested_branch,
            bare_repo = %bare_repo_path.display(),
            "Target branch not found, auto-detecting default branch"
        );

        // Try reading HEAD symbolic ref
        if let Ok(head_ref) =
            Self::exec_git_or_throw(&["symbolic-ref", "HEAD"], bare_repo_path, None).await
        {
            let head_ref = head_ref.trim();
            if let Some(head_branch) = head_ref.strip_prefix("refs/heads/") {
                let verify = Self::exec_git(
                    &[
                        "show-ref",
                        "--verify",
                        "--quiet",
                        &format!("refs/heads/{head_branch}"),
                    ],
                    bare_repo_path,
                    None,
                )
                .await?;
                if verify.exit_code == 0 {
                    info!(
                        requested_branch,
                        resolved_branch = head_branch,
                        "Using HEAD branch as fallback"
                    );
                    return Ok(head_branch.to_string());
                }
            }
        }

        // List all branches and pick the first one
        if let Ok(output) = Self::exec_git_or_throw(
            &["branch", "--list", "--format=%(refname:short)"],
            bare_repo_path,
            None,
        )
        .await
        {
            let branches: Vec<&str> = output.lines().filter(|l| !l.is_empty()).collect();
            if let Some(first) = branches.first() {
                info!(
                    requested_branch,
                    resolved_branch = first,
                    "Using first available branch as fallback"
                );
                return Ok(first.to_string());
            }
        }

        warn!(
            requested_branch,
            bare_repo = %bare_repo_path.display(),
            "No branches found, returning requested branch as-is"
        );
        Ok(requested_branch.to_string())
    }

    // -----------------------------------------------------------------------
    // Worktree management
    // -----------------------------------------------------------------------

    /// Sets up a worktree for a task, reusing an existing one if available.
    pub async fn setup_worktree(
        &self,
        task_id: &str,
        repo_url: &str,
        target_branch: &str,
    ) -> Result<SetupWorktreeResult, AppError> {
        validate_task_id(task_id)?;

        let worktree_path = self.config.worktrees_dir.join(format!("task-{task_id}"));
        let branch_name = format!("feature/task-{task_id}");

        // Check if worktree already exists on disk
        if Self::directory_exists(&worktree_path).await {
            let git_file = worktree_path.join(".git");
            let is_valid = if let Ok(meta) = fs::metadata(&git_file).await {
                if meta.is_file() {
                    match fs::read_to_string(&git_file).await {
                        Ok(content) => content.contains("gitdir:"),
                        Err(_) => false,
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if is_valid {
                info!(task_id, worktree = %worktree_path.display(), "Reusing existing worktree");

                let bare_repo_path = self.ensure_bare_repo(repo_url).await?;
                let is_empty_repo = self.is_empty_repo(&bare_repo_path).await?;

                let mut resolved_branch = target_branch.to_string();

                if !is_empty_repo {
                    self.fetch_repo(&bare_repo_path, Some(target_branch), Some(repo_url))
                        .await?;
                    resolved_branch =
                        Self::resolve_target_branch(&bare_repo_path, target_branch).await?;

                    // Merge latest changes
                    let wt_str = worktree_path.to_string_lossy().to_string();
                    let wt_path = Path::new(&wt_str);
                    match Self::exec_git_or_throw(
                        &["merge", &resolved_branch, "--no-edit"],
                        wt_path,
                        None,
                    )
                    .await
                    {
                        Ok(_) => debug!(target_branch = %resolved_branch, "Merged latest changes"),
                        Err(e) => warn!(
                            target_branch = %resolved_branch,
                            error = %e,
                            "Could not auto-merge latest changes"
                        ),
                    }
                }

                // Re-track in memory
                {
                    let mut worktrees = self.active_worktrees.write().await;
                    worktrees
                        .entry(task_id.to_string())
                        .or_insert(WorktreeInfo {
                            task_id: task_id.to_string(),
                            worktree_path: worktree_path.clone(),
                            branch_name: branch_name.clone(),
                            bare_repo_path,
                        });
                }

                return Ok(SetupWorktreeResult {
                    worktree_path,
                    reused: true,
                    branch_name,
                    target_branch: resolved_branch,
                    is_empty_repo,
                });
            } else {
                // Invalid worktree, clean up
                warn!(worktree = %worktree_path.display(), "Invalid worktree directory found, cleaning up");
                if let Err(e) = Self::remove_directory_with_retry(&worktree_path, 5, 1000).await {
                    if Self::directory_exists(&worktree_path).await {
                        return Err(AppError::Internal(anyhow::anyhow!(
                            "Failed to clean up invalid worktree: {}. Error: {e}",
                            worktree_path.display()
                        )));
                    }
                }
            }
        }

        // Create a new worktree
        let result = self
            .create_worktree(task_id, repo_url, target_branch)
            .await?;

        Ok(SetupWorktreeResult {
            worktree_path: result.0,
            reused: false,
            branch_name,
            target_branch: result.1,
            is_empty_repo: result.2,
        })
    }

    /// Creates a new worktree for a task.
    async fn create_worktree(
        &self,
        task_id: &str,
        repo_url: &str,
        target_branch: &str,
    ) -> Result<(PathBuf, String, bool), AppError> {
        validate_task_id(task_id)?;
        info!(
            task_id,
            repo_url, target_branch, "Creating worktree for task"
        );

        let bare_repo_path = self.ensure_bare_repo(repo_url).await?;
        self.fetch_repo(&bare_repo_path, Some(target_branch), Some(repo_url))
            .await?;

        let resolved_branch = Self::resolve_target_branch(&bare_repo_path, target_branch).await?;
        let is_empty_repo = self.is_empty_repo(&bare_repo_path).await?;

        Self::ensure_dir(&self.config.worktrees_dir).await?;

        let worktree_path = self.config.worktrees_dir.join(format!("task-{task_id}"));
        let branch_name = format!("feature/task-{task_id}");

        if Self::directory_exists(&worktree_path).await {
            return Err(AppError::Conflict(format!(
                "Worktree already exists at {}. Use setup_worktree() to reuse.",
                worktree_path.display()
            )));
        }

        let bare_path = bare_repo_path.clone();
        let wt_path = worktree_path.clone();
        let branch = branch_name.clone();
        let resolved = resolved_branch.clone();
        let is_empty = is_empty_repo;

        self.with_repo_lock(&bare_path.to_string_lossy(), || {
            let bare_path = bare_path.clone();
            let wt_path = wt_path.clone();
            let branch = branch.clone();
            let resolved = resolved.clone();
            async move {
                // Prune orphaned worktree references
                let _ = Self::exec_git(&["worktree", "prune"], &bare_path, None).await;

                let wt_str = wt_path.to_string_lossy().to_string();

                if is_empty {
                    // For empty repos, init and create an orphan branch
                    Self::ensure_dir(&wt_path).await?;
                    Self::exec_git_or_throw(&["init"], &wt_path, None).await?;
                    Self::exec_git_or_throw(
                        &["checkout", "--orphan", &branch],
                        &wt_path,
                        None,
                    )
                    .await?;

                    // Add the bare repo's origin as remote
                    let remote_result =
                        Self::exec_git(&["remote", "get-url", "origin"], &bare_path, None).await?;
                    if remote_result.exit_code == 0 && !remote_result.stdout.is_empty() {
                        let _ = Self::exec_git_or_throw(
                            &["remote", "add", "origin", &remote_result.stdout],
                            &wt_path,
                            None,
                        )
                        .await;
                    }
                } else {
                    // Check if branch already exists
                    let branch_check = Self::exec_git(
                        &[
                            "show-ref",
                            "--verify",
                            "--quiet",
                            &format!("refs/heads/{branch}"),
                        ],
                        &bare_path,
                        None,
                    )
                    .await?;

                    if branch_check.exit_code == 0 {
                        // Branch exists, create worktree from existing branch
                        debug!(branch = %branch, "Branch already exists, using existing");
                        Self::exec_git_or_throw(
                            &["worktree", "add", &wt_str, &branch],
                            &bare_path,
                            None,
                        )
                        .await?;

                        // Merge latest changes
                        match Self::exec_git_or_throw(
                            &["merge", &resolved, "--no-edit"],
                            &wt_path,
                            None,
                        )
                        .await
                        {
                            Ok(_) => debug!(target_branch = %resolved, "Merged latest changes"),
                            Err(e) => warn!(
                                target_branch = %resolved,
                                error = %e,
                                "Could not auto-merge latest changes"
                            ),
                        }
                    } else {
                        // Create new worktree with new branch
                        debug!(branch = %branch, target = %resolved, "Creating new branch from target");
                        Self::exec_git_or_throw(
                            &["worktree", "add", &wt_str, "-b", &branch, &resolved],
                            &bare_path,
                            None,
                        )
                        .await?;
                    }
                }

                Ok(())
            }
        })
        .await?;

        // Configure git user for the worktree
        Self::exec_git_or_throw(
            &["config", "user.email", "agent@agent-board.local"],
            &worktree_path,
            None,
        )
        .await?;
        Self::exec_git_or_throw(
            &["config", "user.name", "Agent Board"],
            &worktree_path,
            None,
        )
        .await?;

        // Track the worktree
        {
            let mut worktrees = self.active_worktrees.write().await;
            worktrees.insert(
                task_id.to_string(),
                WorktreeInfo {
                    task_id: task_id.to_string(),
                    worktree_path: worktree_path.clone(),
                    branch_name: branch_name.clone(),
                    bare_repo_path: bare_repo_path.clone(),
                },
            );
        }

        info!(task_id, worktree = %worktree_path.display(), branch = %branch_name, is_empty_repo, "Worktree created successfully");
        Ok((worktree_path, resolved_branch, is_empty_repo))
    }

    /// Removes a worktree and optionally its branch.
    pub async fn cleanup_worktree(
        &self,
        task_id: &str,
        remove_branch: bool,
    ) -> Result<(), AppError> {
        validate_task_id(task_id)?;

        let (worktree_path, worktree_info) = {
            let worktrees = self.active_worktrees.read().await;
            let info = worktrees.get(task_id).cloned();
            let path = info
                .as_ref()
                .map(|i| i.worktree_path.clone())
                .unwrap_or_else(|| self.config.worktrees_dir.join(format!("task-{task_id}")));
            (path, info)
        };

        debug!(task_id, worktree = %worktree_path.display(), "Cleaning up worktree");

        // Give the OS a moment to release file handles
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let bare_repo_path = self.find_bare_repo_path(&worktree_path, task_id).await;

        // Prune orphaned references under per-repo lock
        if let Some(ref bare_path) = bare_repo_path {
            if Self::directory_exists(bare_path).await {
                let bp = bare_path.clone();
                let _ = self
                    .with_repo_lock(&bp.to_string_lossy(), || {
                        let bp = bp.clone();
                        async move {
                            let _ = Self::exec_git(&["worktree", "prune"], &bp, None).await;
                            Ok(())
                        }
                    })
                    .await;
            }
        }

        if !Self::directory_exists(&worktree_path).await {
            debug!(worktree = %worktree_path.display(), "Worktree directory does not exist, skipping");
            let mut worktrees = self.active_worktrees.write().await;
            worktrees.remove(task_id);
            return Ok(());
        }

        let mut removal_succeeded = false;

        // Strategy 1: git worktree remove --force
        if let Some(ref bare_path) = bare_repo_path {
            if Self::directory_exists(bare_path).await {
                let bp = bare_path.clone();
                let wt_str = worktree_path.to_string_lossy().to_string();
                let result = self
                    .with_repo_lock(&bp.to_string_lossy(), || {
                        let bp = bp.clone();
                        let wt_str = wt_str.clone();
                        async move {
                            Self::exec_git_or_throw(
                                &["worktree", "remove", "--force", &wt_str],
                                &bp,
                                None,
                            )
                            .await
                        }
                    })
                    .await;
                match result {
                    Ok(_) => {
                        removal_succeeded = true;
                        debug!(worktree = %worktree_path.display(), "Removed via git worktree remove");
                    }
                    Err(e) => {
                        warn!(
                            worktree = %worktree_path.display(),
                            error = %e,
                            "git worktree remove failed, attempting direct removal"
                        );
                    }
                }
            }
        }

        // Strategy 2: Direct removal with retry
        if !removal_succeeded && Self::directory_exists(&worktree_path).await {
            match Self::remove_directory_with_retry(&worktree_path, 5, 1000).await {
                Ok(()) => {
                    let _ = removal_succeeded; // suppress unused warning
                    debug!(worktree = %worktree_path.display(), "Removed via direct removal");
                }
                Err(e) => {
                    warn!(worktree = %worktree_path.display(), error = %e, "Direct removal failed");
                }
            }
        }

        // Bare repo cleanup
        if let Some(ref bare_path) = bare_repo_path {
            if Self::directory_exists(bare_path).await {
                let bp = bare_path.clone();
                let branch_info = worktree_info.clone();
                let tid = task_id.to_string();
                let _ = self
                    .with_repo_lock(&bp.to_string_lossy(), || {
                        let bp = bp.clone();
                        let branch_info = branch_info.clone();
                        let tid = tid.clone();
                        async move {
                            if remove_branch {
                                if let Some(ref info) = branch_info {
                                    let _ = Self::exec_git(
                                        &["branch", "-D", &info.branch_name],
                                        &bp,
                                        None,
                                    )
                                    .await;
                                }
                            }
                            let _ = Self::exec_git(&["worktree", "prune"], &bp, None).await;

                            // Clean up worktree metadata
                            let metadata_path = bp.join("worktrees").join(format!("task-{tid}"));
                            if Self::directory_exists(&metadata_path).await {
                                let _ =
                                    Self::remove_directory_with_retry(&metadata_path, 3, 500).await;
                            }
                            Ok(())
                        }
                    })
                    .await;
            }
        }

        // Final verification
        if Self::directory_exists(&worktree_path).await {
            error!(
                task_id,
                worktree = %worktree_path.display(),
                "Worktree directory still exists after cleanup attempts"
            );
            return Err(AppError::Internal(anyhow::anyhow!(
                "Failed to clean up worktree for task {task_id}: {}",
                worktree_path.display()
            )));
        }

        {
            let mut worktrees = self.active_worktrees.write().await;
            worktrees.remove(task_id);
        }
        info!(task_id, "Worktree cleaned up successfully");
        Ok(())
    }

    /// Gets the worktree path for a task.
    pub async fn get_worktree_path(&self, task_id: &str) -> Option<PathBuf> {
        validate_task_id(task_id).ok()?;

        // Check in-memory map first
        {
            let worktrees = self.active_worktrees.read().await;
            if let Some(info) = worktrees.get(task_id) {
                return Some(info.worktree_path.clone());
            }
        }

        // Fallback: check disk
        let disk_path = self.config.worktrees_dir.join(format!("task-{task_id}"));
        if disk_path.exists() {
            Some(disk_path)
        } else {
            None
        }
    }

    /// Checks if a worktree exists for a task.
    pub async fn worktree_exists(&self, task_id: &str) -> Result<bool, AppError> {
        validate_task_id(task_id)?;
        let path = self.config.worktrees_dir.join(format!("task-{task_id}"));
        Ok(Self::directory_exists(&path).await)
    }

    // -----------------------------------------------------------------------
    // Commit and push
    // -----------------------------------------------------------------------

    /// Commits all changes in the worktree.
    pub async fn commit_changes(
        &self,
        worktree_path: &Path,
        message: &str,
    ) -> Result<(), AppError> {
        debug!(worktree = %worktree_path.display(), message, "Committing changes");

        Self::exec_git_or_throw(&["add", "-A"], worktree_path, None).await?;

        let status = Self::exec_git(&["status", "--porcelain"], worktree_path, None).await?;
        if status.stdout.is_empty() {
            info!(worktree = %worktree_path.display(), "No changes to commit");
            return Ok(());
        }

        Self::exec_git_or_throw(&["commit", "-m", message], worktree_path, None).await?;
        info!(worktree = %worktree_path.display(), "Changes committed successfully");
        Ok(())
    }

    /// Pushes the branch to the remote origin.
    pub async fn push_branch(
        &self,
        worktree_path: &Path,
        branch_name: &str,
    ) -> Result<(), AppError> {
        debug!(worktree = %worktree_path.display(), branch_name, "Pushing branch");

        let mut env_map = HashMap::new();
        env_map.insert("GIT_TERMINAL_PROMPT".into(), "0".into());

        Self::exec_git_or_throw(
            &["push", "-u", "origin", branch_name],
            worktree_path,
            Some(&env_map),
        )
        .await?;
        info!(branch_name, "Branch pushed successfully");
        Ok(())
    }

    /// Commits all changes and pushes to remote with token-based authentication.
    ///
    /// This creates a temporary GIT_ASKPASS script for credential injection.
    pub async fn commit_and_push(
        &self,
        task_id: &str,
        message: &str,
        token: &str,
    ) -> Result<(), AppError> {
        let worktree_path = self
            .get_worktree_path(task_id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Worktree not found for task {task_id}")))?;

        // Stage and commit
        self.commit_changes(&worktree_path, message).await?;

        // Get the current branch
        let branch =
            Self::exec_git_or_throw(&["rev-parse", "--abbrev-ref", "HEAD"], &worktree_path, None)
                .await?;

        // Get remote URL to determine provider
        let remote_result =
            Self::exec_git(&["remote", "get-url", "origin"], &worktree_path, None).await?;
        let remote_url = if remote_result.exit_code == 0 {
            remote_result.stdout.clone()
        } else {
            String::new()
        };

        // Set up authenticated remote URL
        if !token.is_empty() && !remote_url.is_empty() && !Self::is_local_repo_url(&remote_url) {
            let auth_url = Self::to_authenticated_clone_url(&remote_url, token);
            let _ = Self::exec_git_or_throw(
                &["remote", "set-url", "origin", &auth_url],
                &worktree_path,
                None,
            )
            .await;
        }

        // Push
        let mut env_map = HashMap::new();
        env_map.insert("GIT_TERMINAL_PROMPT".into(), "0".into());

        Self::exec_git_or_throw(
            &["push", "-u", "origin", branch.trim()],
            &worktree_path,
            Some(&env_map),
        )
        .await?;

        info!(
            task_id,
            branch = branch.trim(),
            "Committed and pushed successfully"
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Diff and changes
    // -----------------------------------------------------------------------

    /// Gets the diff of changes in a worktree.
    pub async fn get_diff(
        &self,
        worktree_path: &Path,
        base_branch: Option<&str>,
    ) -> Result<String, AppError> {
        let mut diff = String::new();

        // Get committed changes against base branch
        let resolved_base = self
            .resolve_diff_base_ref(worktree_path, base_branch)
            .await?;
        let resolved_base = match resolved_base {
            Some(r) => Some(r),
            None => self.resolve_fallback_commit_base_ref(worktree_path).await?,
        };

        if let Some(ref base_ref) = resolved_base {
            let committed =
                Self::exec_git(&["diff", base_ref, "HEAD"], worktree_path, None).await?;
            if !committed.stdout.is_empty() {
                diff.push_str(&committed.stdout);
                diff.push('\n');
            }
        }

        // Include uncommitted changes
        let staged = Self::exec_git(&["diff", "--cached"], worktree_path, None).await?;
        let unstaged = Self::exec_git(&["diff"], worktree_path, None).await?;

        if !staged.stdout.is_empty() {
            diff.push_str("=== Staged Changes ===\n");
            diff.push_str(&staged.stdout);
            diff.push('\n');
        }
        if !unstaged.stdout.is_empty() {
            diff.push_str("=== Unstaged Changes ===\n");
            diff.push_str(&unstaged.stdout);
            diff.push('\n');
        }

        if diff.is_empty() {
            Ok("No changes detected".to_string())
        } else {
            Ok(diff)
        }
    }

    /// Gets a list of changed files in a worktree with detailed stats.
    pub async fn get_changed_files(
        &self,
        worktree_path: &Path,
        base_branch: &str,
    ) -> Result<Vec<ChangedFile>, AppError> {
        let resolved_base = self
            .resolve_diff_base_ref(worktree_path, Some(base_branch))
            .await?;
        let resolved_base = match resolved_base {
            Some(r) => Some(r),
            None => self.resolve_fallback_commit_base_ref(worktree_path).await?,
        };

        // Get committed changes
        let diff_result = if let Some(ref base_ref) = resolved_base {
            Self::exec_git(
                &["diff", "--name-status", base_ref, "HEAD"],
                worktree_path,
                None,
            )
            .await?
        } else {
            Self::exec_git(
                &["diff", "--name-status", "--root", "HEAD"],
                worktree_path,
                None,
            )
            .await?
        };

        // Get uncommitted changes
        let status_result = Self::exec_git(&["status", "--porcelain"], worktree_path, None).await?;

        let mut file_statuses: HashMap<String, ChangeStatus> = HashMap::new();

        // Process committed changes
        if diff_result.exit_code == 0 && !diff_result.stdout.is_empty() {
            for line in diff_result.stdout.lines().filter(|l| !l.is_empty()) {
                let parts: Vec<&str> = line.splitn(2, '\t').collect();
                if parts.len() < 2 {
                    continue;
                }
                let status_char = parts[0];
                let file_path = parts[1].to_string();

                let status = if status_char == "A" {
                    ChangeStatus::Added
                } else if status_char == "D" {
                    ChangeStatus::Deleted
                } else {
                    ChangeStatus::Modified
                };
                file_statuses.insert(file_path, status);
            }
        }

        // Process uncommitted changes
        if status_result.exit_code == 0 && !status_result.stdout.is_empty() {
            for line in status_result.stdout.lines().filter(|l| !l.is_empty()) {
                if line.len() < 3 {
                    continue;
                }
                let status_code = &line[..2];
                let file_path = line[3..].to_string();

                let status = if status_code.contains('A') || status_code == "??" {
                    ChangeStatus::Added
                } else if status_code.contains('D') {
                    ChangeStatus::Deleted
                } else {
                    ChangeStatus::Modified
                };

                let existing = file_statuses.get(&file_path);
                match existing {
                    None => {
                        file_statuses.insert(file_path, status);
                    }
                    Some(ChangeStatus::Added) if status == ChangeStatus::Deleted => {
                        file_statuses.remove(&file_path);
                    }
                    _ => {}
                }
            }
        }

        if file_statuses.is_empty() {
            return Ok(Vec::new());
        }

        let mut changed_files = Vec::new();

        for (file_path, status) in &file_statuses {
            // Get numstat
            let numstat_result = if let Some(ref base_ref) = resolved_base {
                Self::exec_git(
                    &["diff", "--numstat", base_ref, "HEAD", "--", file_path],
                    worktree_path,
                    None,
                )
                .await?
            } else {
                Self::exec_git(
                    &["diff", "--numstat", "--root", "HEAD", "--", file_path],
                    worktree_path,
                    None,
                )
                .await?
            };

            let (mut additions, mut deletions) = (0i64, 0i64);
            if numstat_result.exit_code == 0 && !numstat_result.stdout.is_empty() {
                let parts: Vec<&str> = numstat_result.stdout.split('\t').collect();
                if parts.len() >= 2 {
                    additions = parts[0].parse().unwrap_or(0);
                    deletions = parts[1].parse().unwrap_or(0);
                }
            }

            // If numstat didn't return results, check uncommitted
            if additions == 0 && deletions == 0 && *status != ChangeStatus::Deleted {
                let uncommitted =
                    Self::exec_git(&["diff", "--numstat", "--", file_path], worktree_path, None)
                        .await?;
                if uncommitted.exit_code == 0 && !uncommitted.stdout.is_empty() {
                    let parts: Vec<&str> = uncommitted.stdout.split('\t').collect();
                    if parts.len() >= 2 {
                        additions = parts[0].parse().unwrap_or(0);
                        deletions = parts[1].parse().unwrap_or(0);
                    }
                }
            }

            // Get file content
            let (old_content, new_content) = match status {
                ChangeStatus::Added => {
                    let new_c = self.get_file_content(worktree_path, file_path).await;
                    let new_c = new_c.filter(|c| c.len() <= MAX_CONTENT_SIZE);
                    if additions == 0 {
                        if let Some(ref c) = new_c {
                            additions = c.lines().count() as i64;
                        }
                    }
                    (Some(String::new()), new_c)
                }
                ChangeStatus::Deleted => {
                    let old_c = if let Some(ref base_ref) = resolved_base {
                        self.get_file_content_at_ref(worktree_path, file_path, base_ref)
                            .await
                    } else {
                        Some(String::new())
                    };
                    let old_c = old_c.filter(|c| c.len() <= MAX_CONTENT_SIZE);
                    (old_c, Some(String::new()))
                }
                ChangeStatus::Modified => {
                    let old_c = if let Some(ref base_ref) = resolved_base {
                        self.get_file_content_at_ref(worktree_path, file_path, base_ref)
                            .await
                    } else {
                        Some(String::new())
                    };
                    let new_c = self.get_file_content(worktree_path, file_path).await;
                    (
                        old_c.filter(|c| c.len() <= MAX_CONTENT_SIZE),
                        new_c.filter(|c| c.len() <= MAX_CONTENT_SIZE),
                    )
                }
            };

            changed_files.push(ChangedFile {
                path: file_path.clone(),
                status: status.clone(),
                additions,
                deletions,
                old_content,
                new_content,
            });
        }

        info!(
            total_files = changed_files.len(),
            base_branch, "getChangedFiles result"
        );

        Ok(changed_files)
    }

    // -----------------------------------------------------------------------
    // Diff base resolution
    // -----------------------------------------------------------------------

    /// Resolves the best available ref to use as diff base.
    async fn resolve_diff_base_ref(
        &self,
        worktree_path: &Path,
        base_branch: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let base_branch = match base_branch {
            Some(b) => b,
            None => return Ok(None),
        };

        let local_check =
            Self::exec_git(&["rev-parse", "--verify", base_branch], worktree_path, None).await?;
        if local_check.exit_code == 0 {
            return Ok(Some(base_branch.to_string()));
        }

        let remote_ref = format!("origin/{base_branch}");
        let remote_check =
            Self::exec_git(&["rev-parse", "--verify", &remote_ref], worktree_path, None).await?;
        if remote_check.exit_code == 0 {
            debug!(base_branch, remote_ref = %remote_ref, "Using remote-tracking branch as diff base");
            return Ok(Some(remote_ref));
        }

        warn!(base_branch, "Base branch ref not found for diff operations");
        Ok(None)
    }

    /// Resolves a commit-based fallback ref for diff operations.
    async fn resolve_fallback_commit_base_ref(
        &self,
        worktree_path: &Path,
    ) -> Result<Option<String>, AppError> {
        let parent_check =
            Self::exec_git(&["rev-parse", "--verify", "HEAD~1"], worktree_path, None).await?;
        if parent_check.exit_code == 0 {
            info!("Using fallback commit base for diff operations (HEAD~1)");
            return Ok(Some("HEAD~1".to_string()));
        }
        Ok(None)
    }

    // -----------------------------------------------------------------------
    // File content helpers
    // -----------------------------------------------------------------------

    /// Reads the content of a file in the current worktree.
    async fn get_file_content(&self, worktree_path: &Path, file_path: &str) -> Option<String> {
        let full_path = worktree_path.join(file_path);
        match fs::read_to_string(&full_path).await {
            Ok(content) => {
                // Check for binary content (null bytes)
                if content.contains('\0') {
                    None
                } else {
                    Some(content)
                }
            }
            Err(_) => None,
        }
    }

    /// Reads the content of a file at a specific git reference.
    async fn get_file_content_at_ref(
        &self,
        worktree_path: &Path,
        file_path: &str,
        git_ref: &str,
    ) -> Option<String> {
        let spec = format!("{git_ref}:{file_path}");
        let result = Self::exec_git(&["show", &spec], worktree_path, None)
            .await
            .ok()?;
        if result.exit_code != 0 {
            return None;
        }
        if result.stdout.contains('\0') {
            return None;
        }
        Some(result.stdout)
    }

    // -----------------------------------------------------------------------
    // Bare repo path discovery
    // -----------------------------------------------------------------------

    /// Finds the bare repo path for a worktree using multiple strategies.
    async fn find_bare_repo_path(&self, worktree_path: &Path, task_id: &str) -> Option<PathBuf> {
        // Strategy 1: active worktrees map
        {
            let worktrees = self.active_worktrees.read().await;
            if let Some(info) = worktrees.get(task_id) {
                return Some(info.bare_repo_path.clone());
            }
        }

        // Strategy 2: read .git file
        let git_file = worktree_path.join(".git");
        if let Ok(content) = fs::read_to_string(&git_file).await {
            if let Some(gitdir) = content
                .lines()
                .find_map(|line| line.strip_prefix("gitdir:").map(|s| s.trim().to_string()))
            {
                // Find "/worktrees/" or "\worktrees\" in the gitdir path
                let normalized = gitdir.replace('\\', "/");
                if let Some(idx) = normalized.rfind("/worktrees/") {
                    return Some(PathBuf::from(&gitdir[..idx]));
                }
            }
        }

        // Strategy 3: git rev-parse
        if let Ok(result) = Self::exec_git(&["rev-parse", "--git-dir"], worktree_path, None).await {
            if result.exit_code == 0 {
                let normalized = result.stdout.replace('\\', "/");
                if let Some(idx) = normalized.rfind("/worktrees/") {
                    return Some(PathBuf::from(&result.stdout[..idx]));
                }
            }
        }

        // Strategy 4: Scan repos directory
        if let Ok(mut entries) = fs::read_dir(&self.config.repos_base_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.ends_with(".git") {
                    let candidate = entry.path();
                    let worktree_ref = candidate.join("worktrees").join(format!("task-{task_id}"));
                    if Self::directory_exists(&worktree_ref).await {
                        return Some(candidate);
                    }
                }
            }
        }

        None
    }

    // -----------------------------------------------------------------------
    // Additional operations
    // -----------------------------------------------------------------------

    /// Gets the current branch name in a worktree.
    pub async fn get_current_branch(&self, worktree_path: &Path) -> Result<String, AppError> {
        Self::exec_git_or_throw(&["rev-parse", "--abbrev-ref", "HEAD"], worktree_path, None).await
    }

    /// Gets the latest commit hash in a worktree.
    pub async fn get_latest_commit(&self, worktree_path: &Path) -> Result<String, AppError> {
        Self::exec_git_or_throw(&["rev-parse", "HEAD"], worktree_path, None).await
    }

    /// Checks if a worktree has uncommitted changes.
    pub async fn has_changes(&self, worktree_path: &Path) -> Result<bool, AppError> {
        let result = Self::exec_git(&["status", "--porcelain"], worktree_path, None).await?;
        Ok(!result.stdout.is_empty())
    }

    /// Gets the remote origin URL configured in a worktree or bare repo.
    pub async fn get_remote_url(&self, repo_path: &Path) -> Option<String> {
        let result = Self::exec_git(&["remote", "get-url", "origin"], repo_path, None)
            .await
            .ok()?;
        if result.exit_code == 0 && !result.stdout.is_empty() {
            Some(result.stdout)
        } else {
            None
        }
    }

    /// Gets the list of files with merge conflicts in a worktree.
    pub async fn get_conflicting_files(
        &self,
        worktree_path: &Path,
    ) -> Result<Vec<String>, AppError> {
        let result = Self::exec_git(
            &["diff", "--name-only", "--diff-filter=U"],
            worktree_path,
            None,
        )
        .await?;
        if result.exit_code != 0 || result.stdout.is_empty() {
            return Ok(Vec::new());
        }
        Ok(result
            .stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect())
    }

    /// Checks which files still contain conflict markers.
    pub async fn has_conflict_markers(
        &self,
        worktree_path: &Path,
        files: &[String],
    ) -> Result<Vec<String>, AppError> {
        let mut files_with_markers = Vec::new();
        for file in files {
            let full_path = worktree_path.join(file);
            if let Ok(content) = fs::read_to_string(&full_path).await {
                if content.contains("<<<<<<<") {
                    files_with_markers.push(file.clone());
                }
            }
        }
        Ok(files_with_markers)
    }

    /// Gets the bare repo path for a task from the active worktrees map.
    pub async fn get_bare_repo_path_for_task(&self, task_id: &str) -> Option<PathBuf> {
        let worktrees = self.active_worktrees.read().await;
        worktrees.get(task_id).map(|i| i.bare_repo_path.clone())
    }

    /// Resolves and validates the remote URL to be used for PR creation.
    /// For local file:// repos, syncs worktree origin from source repo origin.
    pub async fn prepare_worktree_remote_for_pr(
        &self,
        worktree_path: &Path,
        repo_url: &str,
    ) -> Result<String, AppError> {
        let current_remote = self.get_remote_url(worktree_path).await.unwrap_or_default();

        if !Self::is_local_repo_url(repo_url) {
            return Ok(current_remote);
        }

        let source_path = Self::local_repo_path(repo_url);
        if !Self::directory_exists(Path::new(&source_path)).await {
            return Err(AppError::Validation(format!(
                "Local repository no longer exists at {source_path}"
            )));
        }

        let git_configs = Self::local_repo_safe_directory_configs(Path::new(&source_path));
        let source_remote_result = Self::exec_git_with_configs(
            &["remote", "get-url", "origin"],
            Path::new(&source_path),
            None,
            &git_configs,
        )
        .await?;

        let source_remote = if source_remote_result.exit_code == 0 {
            source_remote_result.stdout.trim().to_string()
        } else {
            String::new()
        };

        if source_remote.is_empty() {
            return Err(AppError::Validation(format!(
                "Local repository ({source_path}) does not have a remote origin configured"
            )));
        }

        if Self::is_local_path_remote(&source_remote) {
            return Err(AppError::Validation(format!(
                "The local repository's origin points to another local path ({source_remote}). Configure a GitHub/GitLab remote."
            )));
        }

        if !source_remote.contains("github.com") && !source_remote.contains("gitlab.com") {
            return Err(AppError::Validation(format!(
                "Agent Board only supports GitHub/GitLab. Current remote: {source_remote}"
            )));
        }

        // Sync worktree origin
        if current_remote.is_empty() || current_remote.trim() != source_remote {
            Self::exec_git_or_throw(
                &["remote", "set-url", "origin", &source_remote],
                worktree_path,
                None,
            )
            .await?;
            info!(
                worktree = %worktree_path.display(),
                source_remote,
                "Worktree origin synchronized from local source repo"
            );
        }

        Ok(source_remote)
    }

    /// Fetches a branch from the remote origin, running inside a worktree.
    pub async fn fetch_in_worktree(
        &self,
        worktree_path: &Path,
        branch: &str,
    ) -> Result<(), AppError> {
        debug!(worktree = %worktree_path.display(), branch, "Fetching branch in worktree");

        let mut env_map = HashMap::new();
        env_map.insert("GIT_TERMINAL_PROMPT".into(), "0".into());

        let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
        Self::exec_git_or_throw(
            &["fetch", "origin", &refspec],
            worktree_path,
            Some(&env_map),
        )
        .await?;

        info!(branch, "Branch fetched in worktree successfully");
        Ok(())
    }
}
