use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::Instant;
use tracing::{error, info, warn};

use crate::agent::{
    APIAgentRunner, APIRunnerOptions, AgentRunResult, CLIAgentRunner, CLIRunnerOptions,
};
use crate::db::Database;
use crate::error::AppError;
use crate::models::task::{TaskStatus, UpdateTaskInput};
use crate::services::git_service::GitService;
use crate::services::task_service;
use crate::utils::{DataEventEmitter, SSEEmitter};

/// Default timeout duration (10 minutes).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Warning threshold before timeout (5 minutes before).
#[allow(dead_code)]
const WARNING_THRESHOLD: Duration = Duration::from_secs(5 * 60);

/// Extension duration (5 minutes).
const EXTENSION_DURATION: Duration = Duration::from_secs(5 * 60);

/// Maximum number of log entries per task to prevent memory leaks.
const MAX_LOGS_PER_TASK: usize = 1000;

/// A log entry recorded during agent execution.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentLogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Tracking metadata for a running agent.
///
/// The actual `CLIAgentRunner` is owned by the spawned Tokio task. This struct
/// stores only the lightweight metadata needed for management operations
/// (timeout, cancel, feedback). Communication with the runner happens via
/// a `CancellationToken` and an `mpsc` feedback channel.
#[allow(dead_code)]
struct AgentTracking {
    task_id: String,
    started_at: Instant,
    timeout_at: Instant,
    warning_sent: bool,
    /// Cancellation token shared with the runner.
    cancel_token: tokio_util::sync::CancellationToken,
    /// Feedback sender shared with the runner.
    feedback_tx: tokio::sync::mpsc::Sender<String>,
}

/// Internal enum for the spawned task to own the runner.
enum RunnerKind {
    CLI(CLIAgentRunner),
    API(APIAgentRunner),
}

/// Options for starting an agent — either CLI or API.
pub enum RunnerOptions {
    CLI(CLIRunnerOptions),
    API(APIRunnerOptions),
}

/// Service for managing agent execution.
///
/// Handles starting, stopping, monitoring, and sending feedback to agent runners.
/// Each running agent is tracked by task ID.
pub struct AgentService {
    active_agents: Arc<RwLock<HashMap<String, AgentTracking>>>,
    agent_logs: Arc<RwLock<HashMap<String, Vec<AgentLogEntry>>>>,
    sse_emitter: Arc<SSEEmitter>,
    data_emitter: Arc<DataEventEmitter>,
    db: Option<Database>,
}

impl AgentService {
    /// Creates a new agent service.
    pub fn new(sse_emitter: Arc<SSEEmitter>, data_emitter: Arc<DataEventEmitter>) -> Self {
        Self {
            active_agents: Arc::new(RwLock::new(HashMap::new())),
            agent_logs: Arc::new(RwLock::new(HashMap::new())),
            sse_emitter,
            data_emitter,
            db: None,
        }
    }

    /// Sets the database reference for task status updates on agent completion.
    pub fn set_db(&mut self, db: Database) {
        self.db = Some(db);
    }

    /// Starts an agent for the given task.
    ///
    /// Spawns the agent as a background Tokio task and returns immediately.
    /// Supports both CLI runners (claude-code, codex, etc.) and API runners (MiniMax).
    /// The result of the agent run is communicated via SSE events.
    pub async fn start_agent(&self, task_id: &str, options: RunnerOptions) -> Result<(), AppError> {
        // Check if agent is already running
        {
            let agents = self.active_agents.read().await;
            if agents.contains_key(task_id) {
                return Err(AppError::Conflict(format!(
                    "Agent is already running for task {task_id}"
                )));
            }
        }

        // Initialize logs
        {
            let mut logs = self.agent_logs.write().await;
            logs.entry(task_id.to_string()).or_default();
        }

        // Extract workspace path and create the appropriate runner
        let (workspace_path, cancel_token, feedback_tx, runner_kind) = match options {
            RunnerOptions::CLI(cli_opts) => {
                let action = if cli_opts.resume_session_id.is_some() {
                    "Resuming CLI agent session"
                } else {
                    "Starting CLI agent"
                };
                self.log(
                    task_id,
                    "info",
                    &format!("{action}: {}", cli_opts.agent_type),
                    None,
                )
                .await;
                let workspace_path = cli_opts.cwd.clone();
                let runner = CLIAgentRunner::new(cli_opts);
                let cancel_token = runner.cancel_token();
                let feedback_tx = runner.feedback_sender();
                (
                    workspace_path,
                    cancel_token,
                    feedback_tx,
                    RunnerKind::CLI(runner),
                )
            }
            RunnerOptions::API(api_opts) => {
                self.log(
                    task_id,
                    "info",
                    &format!("Starting API agent: {}", api_opts.agent_type),
                    None,
                )
                .await;
                let workspace_path = api_opts.cwd.clone();
                let runner = APIAgentRunner::new(api_opts);
                let cancel_token = runner.cancel_token();
                let feedback_tx = runner.feedback_sender();
                (
                    workspace_path,
                    cancel_token,
                    feedback_tx,
                    RunnerKind::API(runner),
                )
            }
        };

        let now = Instant::now();
        let timeout_at = now + DEFAULT_TIMEOUT;

        // Store tracking metadata
        {
            let mut agents = self.active_agents.write().await;
            agents.insert(
                task_id.to_string(),
                AgentTracking {
                    task_id: task_id.to_string(),
                    started_at: now,
                    timeout_at,
                    warning_sent: false,
                    cancel_token: cancel_token.clone(),
                    feedback_tx,
                },
            );
        }

        // Spawn the agent execution as a background task
        let sse = Arc::clone(&self.sse_emitter);
        let data_emitter = Arc::clone(&self.data_emitter);
        let active_agents = Arc::clone(&self.active_agents);
        let _agent_logs = Arc::clone(&self.agent_logs);
        let db = self.db.clone();
        let task_id_bg = task_id.to_string();

        tokio::spawn(async move {
            let result = match runner_kind {
                RunnerKind::CLI(mut runner) => runner.run(&sse).await,
                RunnerKind::API(mut runner) => runner.run(&sse).await,
            };

            match &result {
                Ok(res) if res.success => {
                    info!(task_id = %task_id_bg, "Agent completed successfully");
                    sse.emit_log(&task_id_bg, "info", "Agent completed successfully", None)
                        .await;

                    // Extract changes from the workspace (git diff)
                    // Use base_commit (captured at task start) so the diff is
                    // scoped to only this task's changes, not all accumulated
                    // changes from previous tasks in the same worktree.
                    sse.emit_log(
                        &task_id_bg,
                        "info",
                        "Extracting changes from workspace...",
                        None,
                    )
                    .await;
                    let base_commit = if let Some(ref db) = db {
                        let tid = task_id_bg.clone();
                        db.call(move |conn| task_service::get_task_by_id(conn, &tid))
                            .await
                            .ok()
                            .and_then(|t| t.base_commit)
                    } else {
                        None
                    };
                    let changes_data =
                        extract_changes_data(&workspace_path, base_commit.as_deref()).await;
                    if changes_data.is_some() {
                        sse.emit_log(
                            &task_id_bg,
                            "info",
                            "Changes data extracted successfully",
                            None,
                        )
                        .await;
                    } else {
                        sse.emit_log(
                            &task_id_bg,
                            "warn",
                            "No changes detected in workspace",
                            None,
                        )
                        .await;
                    }

                    // Update task status to awaiting_review and persist changes
                    if let Some(ref db) = db {
                        let tid = task_id_bg.clone();
                        let pr_url = res.pr_url.clone();
                        let _ = db
                            .call(move |conn| {
                                task_service::update_task(
                                    conn,
                                    &tid,
                                    &UpdateTaskInput {
                                        status: Some(TaskStatus::AwaitingReview),
                                        changes_data: changes_data.map(Some),
                                        pr_url: pr_url.map(Some),
                                        ..Default::default()
                                    },
                                )
                            })
                            .await;
                    }

                    sse.emit_status(&task_id_bg, "awaiting_review").await;
                    // Emit awaiting_review (non-terminal) — NOT complete.
                    // The frontend closes the SSE connection on "complete",
                    // which would prevent live events if the user resumes the
                    // agent via feedback. "awaiting_review" keeps the
                    // connection open, matching the TypeScript server behavior.
                    sse.emit_awaiting_review(
                        &task_id_bg,
                        "Agent completed. Review changes before creating PR.",
                    )
                    .await;
                }
                Ok(res) => {
                    let err = res.error.as_deref().unwrap_or("Unknown error");
                    warn!(task_id = %task_id_bg, error = err, "Agent failed");

                    // Update task status to failed
                    if let Some(ref db) = db {
                        let tid = task_id_bg.clone();
                        let err_msg = err.to_string();
                        let changes = res.changes_data.clone();
                        let _ = db
                            .call(move |conn| {
                                task_service::update_task(
                                    conn,
                                    &tid,
                                    &UpdateTaskInput {
                                        status: Some(TaskStatus::Failed),
                                        error: Some(Some(err_msg)),
                                        changes_data: changes.map(Some),
                                        ..Default::default()
                                    },
                                )
                            })
                            .await;
                    }

                    sse.emit_error(&task_id_bg, err).await;
                    sse.emit_status(&task_id_bg, "failed").await;
                }
                Err(e) => {
                    error!(task_id = %task_id_bg, error = %e, "Agent error");

                    // Update task status to failed
                    if let Some(ref db) = db {
                        let tid = task_id_bg.clone();
                        let err_msg = e.to_string();
                        let _ = db
                            .call(move |conn| {
                                task_service::update_task(
                                    conn,
                                    &tid,
                                    &UpdateTaskInput {
                                        status: Some(TaskStatus::Failed),
                                        error: Some(Some(err_msg)),
                                        ..Default::default()
                                    },
                                )
                            })
                            .await;
                    }

                    sse.emit_error(&task_id_bg, &e.to_string()).await;
                    sse.emit_status(&task_id_bg, "failed").await;
                }
            }

            data_emitter.emit_change("task", "updated", Some(&task_id_bg));

            // Remove from active agents
            {
                let mut agents = active_agents.write().await;
                agents.remove(&task_id_bg);
            }
        });

        info!(
            task_id,
            timeout_at = ?timeout_at,
            "Agent started successfully"
        );

        Ok(())
    }

    /// Runs an agent under AgentService tracking, returning the result to the caller.
    ///
    /// This is useful for non-task workflows, such as spec generation, that still need
    /// live SSE output and user feedback while the agent is running but do not want the
    /// task-specific completion behavior from `start_agent`.
    pub async fn run_tracked_agent(
        &self,
        task_id: &str,
        options: RunnerOptions,
    ) -> Result<AgentRunResult, AppError> {
        {
            let agents = self.active_agents.read().await;
            if agents.contains_key(task_id) {
                return Err(AppError::Conflict(format!(
                    "Agent is already running for {task_id}"
                )));
            }
        }

        let (cancel_token, feedback_tx, runner_kind) = match options {
            RunnerOptions::CLI(cli_opts) => {
                let action = if cli_opts.resume_session_id.is_some() {
                    "Resuming CLI agent session"
                } else {
                    "Starting CLI agent"
                };
                self.log(
                    task_id,
                    "info",
                    &format!("{action}: {}", cli_opts.agent_type),
                    None,
                )
                .await;
                let runner = CLIAgentRunner::new(cli_opts);
                let cancel_token = runner.cancel_token();
                let feedback_tx = runner.feedback_sender();
                (cancel_token, feedback_tx, RunnerKind::CLI(runner))
            }
            RunnerOptions::API(api_opts) => {
                self.log(
                    task_id,
                    "info",
                    &format!("Starting API agent: {}", api_opts.agent_type),
                    None,
                )
                .await;
                let runner = APIAgentRunner::new(api_opts);
                let cancel_token = runner.cancel_token();
                let feedback_tx = runner.feedback_sender();
                (cancel_token, feedback_tx, RunnerKind::API(runner))
            }
        };

        let now = Instant::now();
        {
            let mut agents = self.active_agents.write().await;
            agents.insert(
                task_id.to_string(),
                AgentTracking {
                    task_id: task_id.to_string(),
                    started_at: now,
                    timeout_at: now + DEFAULT_TIMEOUT,
                    warning_sent: false,
                    cancel_token,
                    feedback_tx,
                },
            );
        }

        let result = match runner_kind {
            RunnerKind::CLI(mut runner) => runner.run(&self.sse_emitter).await,
            RunnerKind::API(mut runner) => runner.run(&self.sse_emitter).await,
        };

        {
            let mut agents = self.active_agents.write().await;
            agents.remove(task_id);
        }

        match result {
            Ok(res) if res.success => {
                self.sse_emitter
                    .emit_log(task_id, "info", "Agent completed successfully", None)
                    .await;
                Ok(res)
            }
            Ok(res) => {
                let err = res
                    .error
                    .clone()
                    .unwrap_or_else(|| "Agent generation failed".to_string());
                self.sse_emitter.emit_error(task_id, &err).await;
                Ok(res)
            }
            Err(e) => {
                self.sse_emitter.emit_error(task_id, &e.to_string()).await;
                Err(e)
            }
        }
    }

    /// Records a log entry for a task.
    async fn log(
        &self,
        task_id: &str,
        level: &str,
        message: &str,
        data: Option<serde_json::Value>,
    ) {
        let entry = AgentLogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: level.to_string(),
            message: message.to_string(),
            data: data.clone(),
        };

        {
            let mut logs = self.agent_logs.write().await;
            let task_logs = logs.entry(task_id.to_string()).or_default();
            if task_logs.len() < MAX_LOGS_PER_TASK {
                task_logs.push(entry);
            }
        }

        self.sse_emitter
            .emit_log(task_id, level, message, data)
            .await;
    }

    /// Sends feedback to a running agent.
    pub async fn send_feedback(&self, task_id: &str, message: &str) -> Result<(), AppError> {
        let agents = self.active_agents.read().await;
        if let Some(agent) = agents.get(task_id) {
            let tx = agent.feedback_tx.clone();
            if let Err(e) = tx.send(message.to_string()).await {
                warn!(task_id, error = %e, "Failed to send feedback");
            } else {
                info!(task_id, "Feedback sent to agent");
            }
            Ok(())
        } else {
            Err(AppError::NotFound(format!(
                "No active agent for task {task_id}"
            )))
        }
    }

    /// Cancels a running agent.
    pub async fn cancel_agent(&self, task_id: &str) -> Result<(), AppError> {
        let agents = self.active_agents.read().await;
        if let Some(agent) = agents.get(task_id) {
            agent.cancel_token.cancel();
            info!(task_id, "Agent cancellation requested");
            Ok(())
        } else {
            Err(AppError::NotFound(format!(
                "No active agent for task {task_id}"
            )))
        }
    }

    /// Returns whether an agent is running for the given task.
    pub async fn is_running(&self, task_id: &str) -> bool {
        let agents = self.active_agents.read().await;
        agents.contains_key(task_id)
    }

    /// Returns the number of currently active agents.
    pub async fn get_active_count(&self) -> usize {
        let agents = self.active_agents.read().await;
        agents.len()
    }

    /// Returns the logs for a task.
    pub async fn get_logs(&self, task_id: &str) -> Vec<AgentLogEntry> {
        let logs = self.agent_logs.read().await;
        logs.get(task_id).cloned().unwrap_or_default()
    }

    /// Extends the timeout for a running agent.
    ///
    /// Returns the new timeout instant.
    pub async fn extend_timeout(&self, task_id: &str) -> Result<Instant, AppError> {
        let mut agents = self.active_agents.write().await;
        if let Some(agent) = agents.get_mut(task_id) {
            agent.timeout_at = Instant::now() + EXTENSION_DURATION;
            agent.warning_sent = false;
            info!(task_id, "Timeout extended");
            Ok(agent.timeout_at)
        } else {
            Err(AppError::NotFound(format!(
                "No active agent for task {task_id}"
            )))
        }
    }
}

/// Extracts changes data (file list + diff) from a workspace directory after
/// an agent completes, and returns it as a serialized JSON string suitable for
/// storing in `changes_data`.
///
/// This mirrors the TypeScript server's `persistChangesData` method which calls
/// `gitService.getChangedFiles()` and `gitService.getDiff()` after agent success.
async fn extract_changes_data(
    workspace_path: &std::path::Path,
    target_branch: Option<&str>,
) -> Option<String> {
    use serde_json::json;

    // Resolve the base ref for diffing (target_branch or fallback)
    let base_ref = resolve_diff_base(workspace_path, target_branch).await;

    // Get the full diff text
    let diff = get_workspace_diff(workspace_path, base_ref.as_deref()).await;

    // Get the list of changed files with stats
    let files = get_changed_file_list(workspace_path, base_ref.as_deref()).await;

    let changes = json!({
        "files": files,
        "diff": diff,
    });

    match serde_json::to_string(&changes) {
        Ok(s) => {
            info!(
                files_count = files.len(),
                diff_len = diff.len(),
                "Extracted changes data from workspace"
            );
            Some(s)
        }
        Err(e) => {
            warn!("Failed to serialize changes data: {e}");
            None
        }
    }
}

/// Resolves the best base reference for diffing in a workspace.
///
/// `base_ref_hint` can be a commit hash (from `base_commit`) or a branch name
/// (from `target_branch`). If it looks like a commit hash (40 hex chars), it is
/// verified directly. Otherwise it is resolved as a branch name.
async fn resolve_diff_base(
    workspace_path: &std::path::Path,
    base_ref_hint: Option<&str>,
) -> Option<String> {
    if let Some(hint) = base_ref_hint {
        // If it looks like a full SHA-1 hash, verify it directly
        let is_commit_hash = hint.len() >= 7 && hint.chars().all(|c| c.is_ascii_hexdigit());
        if is_commit_hash {
            let verify_ref = format!("{hint}^{{commit}}");
            let result = GitService::exec_git(
                &["rev-parse", "--verify", &verify_ref],
                workspace_path,
                None,
            )
            .await;
            if let Ok(r) = result {
                if r.exit_code == 0 {
                    return Some(hint.to_string());
                }
            }
        }

        // Try as origin/<branch>
        let remote_ref = format!("origin/{hint}");
        let result = GitService::exec_git(
            &["rev-parse", "--verify", &remote_ref],
            workspace_path,
            None,
        )
        .await;
        if let Ok(r) = result {
            if r.exit_code == 0 {
                return Some(remote_ref);
            }
        }

        // Try as local branch
        let result =
            GitService::exec_git(&["rev-parse", "--verify", hint], workspace_path, None).await;
        if let Ok(r) = result {
            if r.exit_code == 0 {
                return Some(hint.to_string());
            }
        }
    }

    // Fallback: try origin/main, origin/master, main, master
    for candidate in &["origin/main", "origin/master", "main", "master"] {
        let result =
            GitService::exec_git(&["rev-parse", "--verify", candidate], workspace_path, None).await;
        if let Ok(r) = result {
            if r.exit_code == 0 {
                return Some(candidate.to_string());
            }
        }
    }

    None
}

/// Gets the full diff text from a workspace.
async fn get_workspace_diff(workspace_path: &std::path::Path, base_ref: Option<&str>) -> String {
    let mut diff = String::new();

    // Get committed changes against base
    if let Some(base) = base_ref {
        if let Ok(result) =
            GitService::exec_git(&["diff", base, "HEAD"], workspace_path, None).await
        {
            if result.exit_code == 0 && !result.stdout.is_empty() {
                diff.push_str(&result.stdout);
                diff.push('\n');
            }
        }
    }

    // Include uncommitted staged changes
    if let Ok(result) = GitService::exec_git(&["diff", "--cached"], workspace_path, None).await {
        if result.exit_code == 0 && !result.stdout.is_empty() {
            diff.push_str(&result.stdout);
            diff.push('\n');
        }
    }

    // Include uncommitted unstaged changes
    if let Ok(result) = GitService::exec_git(&["diff"], workspace_path, None).await {
        if result.exit_code == 0 && !result.stdout.is_empty() {
            diff.push_str(&result.stdout);
            diff.push('\n');
        }
    }

    diff
}

/// Gets the list of changed files with status and line counts.
async fn get_changed_file_list(
    workspace_path: &std::path::Path,
    base_ref: Option<&str>,
) -> Vec<serde_json::Value> {
    use serde_json::json;
    use std::collections::HashMap as StdHashMap;

    let mut file_statuses: StdHashMap<String, &str> = StdHashMap::new();

    // Get committed changes
    let diff_args: Vec<&str> = if let Some(base) = base_ref {
        vec!["diff", "--name-status", base, "HEAD"]
    } else {
        vec!["diff", "--name-status", "--root", "HEAD"]
    };

    if let Ok(result) = GitService::exec_git(&diff_args, workspace_path, None).await {
        if result.exit_code == 0 {
            for line in result.stdout.lines().filter(|l| !l.is_empty()) {
                let parts: Vec<&str> = line.splitn(2, '\t').collect();
                if parts.len() >= 2 {
                    let status = match parts[0] {
                        "A" => "added",
                        "D" => "deleted",
                        _ => "modified",
                    };
                    file_statuses.insert(parts[1].to_string(), status);
                }
            }
        }
    }

    // Get uncommitted changes
    // NOTE: We must use the raw stdout (not trimmed) because `git status --porcelain`
    // uses a fixed-width format " M filename" where the leading space is significant.
    // exec_git() trims the full output, which strips the leading space on the first line.
    // To work around this, we parse more defensively using the last 2 non-space chars
    // before the filename.
    if let Ok(result) = GitService::exec_git(&["status", "--porcelain"], workspace_path, None).await
    {
        if result.exit_code == 0 {
            for line in result.stdout.lines().filter(|l| !l.is_empty()) {
                // git status --porcelain format: "XY filename"
                // But exec_git trims the output, so the first line may lose its leading space.
                // Instead of relying on fixed positions, find the filename after the status codes.
                // The filename starts after "XY " (2 status chars + 1 space).
                // However if trim() ate the leading space, we might have "M filename" or "?? filename".
                // Safest approach: split on first space after status chars.
                let file_path = if line.len() >= 4 && line.as_bytes()[2] == b' ' {
                    // Normal format: "XY filename" (e.g., " M index.js" or "?? new.js")
                    &line[3..]
                } else if line.len() >= 3 && line.as_bytes()[1] == b' ' {
                    // Trimmed format: "X filename" (e.g., "M index.js" after trim ate leading space)
                    &line[2..]
                } else {
                    continue;
                };

                let file_path = file_path.trim().to_string();
                if file_path.is_empty() {
                    continue;
                }

                // Determine status from the available status chars
                let status = if line.contains("??") || line.starts_with('A') || line.contains('A') {
                    "added"
                } else if line.starts_with('D') || line.contains('D') {
                    "deleted"
                } else {
                    "modified"
                };

                file_statuses.entry(file_path).or_insert(status);
            }
        }
    }

    // Build file list with numstat and file content
    let mut files = Vec::new();
    let max_content_size = 500_000; // 500KB limit per file

    for (file_path, status) in &file_statuses {
        let (mut additions, mut deletions) = (0i64, 0i64);

        // Get line counts from committed numstat
        let numstat_args: Vec<&str> = if let Some(base) = base_ref {
            vec!["diff", "--numstat", base, "HEAD", "--", file_path]
        } else {
            vec!["diff", "--numstat", "--root", "HEAD", "--", file_path]
        };

        if let Ok(result) = GitService::exec_git(&numstat_args, workspace_path, None).await {
            if result.exit_code == 0 && !result.stdout.is_empty() {
                let parts: Vec<&str> = result.stdout.split('\t').collect();
                if parts.len() >= 2 {
                    additions = parts[0].trim().parse().unwrap_or(0);
                    deletions = parts[1].trim().parse().unwrap_or(0);
                }
            }
        }

        // Fallback: uncommitted numstat
        if additions == 0 && deletions == 0 && *status != "deleted" {
            if let Ok(result) = GitService::exec_git(
                &["diff", "--numstat", "--", file_path],
                workspace_path,
                None,
            )
            .await
            {
                if result.exit_code == 0 && !result.stdout.is_empty() {
                    let parts: Vec<&str> = result.stdout.split('\t').collect();
                    if parts.len() >= 2 {
                        additions = parts[0].trim().parse().unwrap_or(0);
                        deletions = parts[1].trim().parse().unwrap_or(0);
                    }
                }
            }
        }

        // Get file content for diff rendering
        let mut file_json = json!({
            "path": file_path,
            "status": status,
            "additions": additions,
            "deletions": deletions,
        });

        match *status {
            "added" => {
                file_json["oldContent"] = json!("");
                let full_path = workspace_path.join(file_path);
                if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                    if content.len() <= max_content_size {
                        if additions == 0 {
                            additions = content.lines().count() as i64;
                            file_json["additions"] = json!(additions);
                        }
                        file_json["newContent"] = json!(content);
                    }
                }
            }
            "deleted" => {
                if let Some(base) = base_ref {
                    let spec = format!("{base}:{file_path}");
                    if let Ok(r) =
                        GitService::exec_git(&["show", &spec], workspace_path, None).await
                    {
                        if r.exit_code == 0 && r.stdout.len() <= max_content_size {
                            file_json["oldContent"] = json!(r.stdout);
                        }
                    }
                }
                file_json["newContent"] = json!("");
            }
            "modified" => {
                if let Some(base) = base_ref {
                    let spec = format!("{base}:{file_path}");
                    if let Ok(r) =
                        GitService::exec_git(&["show", &spec], workspace_path, None).await
                    {
                        if r.exit_code == 0 && r.stdout.len() <= max_content_size {
                            file_json["oldContent"] = json!(r.stdout);
                        }
                    }
                } else {
                    file_json["oldContent"] = json!("");
                }
                let full_path = workspace_path.join(file_path);
                if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                    if content.len() <= max_content_size {
                        file_json["newContent"] = json!(content);
                    }
                }
            }
            _ => {}
        }

        files.push(file_json);
    }

    files
}
