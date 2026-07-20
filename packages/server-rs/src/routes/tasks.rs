//! Task management routes.
//!
//! Implements the full task CRUD plus lifecycle endpoints (execute, cancel,
//! approve, request-changes, etc.) mirroring the TypeScript server's
//! `routes/tasks.ts`.
//!
//! Several endpoints are stubs that return 202/501 because they depend on
//! subsystems not yet ported (Agent System = Phase 4, Git+PR = Phase 3).

use std::convert::Infallible;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::agent::cli_prompts;
use crate::agent::types::AgentType;
use crate::agent::{APIRunnerOptions, CLIRunnerOptions};
use crate::error::AppError;
use crate::models::qa::{CreateQaRunInput, QaRun, QaRunStatus, UpdateQaRunInput};
use crate::models::task::{CreateTaskInput, TaskStatus, UpdateTaskInput};
use crate::services::agent_service::RunnerOptions;
use crate::services::git_service::GitService;
use crate::services::{qa_service, repo_service, settings_service, task_service};
use crate::utils::{SSEEvent, SSEEventType};
use crate::AppState;

// ============================================================================
// Query / body types
// ============================================================================

/// Query parameters for `GET /api/tasks`.
#[derive(Debug, Deserialize)]
struct ListTasksQuery {
    #[serde(default)]
    repository_id: Option<String>,
    #[serde(default)]
    repo_url: Option<String>,
}

/// Body for `POST /:id/request-changes`.
#[derive(Debug, Deserialize)]
struct RequestChangesBody {
    #[serde(default)]
    feedback: Option<String>,
}

/// Body for `POST /:id/feedback`.
#[derive(Debug, Deserialize)]
struct FeedbackBody {
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct QaBoardItem {
    task: crate::models::task::Task,
    latest_run: Option<QaRun>,
}

// ============================================================================
// Router
// ============================================================================

/// Builds the tasks sub-router.
///
/// All paths are relative; the caller nests this under `/api/tasks`.
pub fn router() -> Router<AppState> {
    Router::new()
        // CRUD
        .route("/", post(create_task))
        .route("/", get(list_tasks))
        .route("/qa-board/items", get(list_qa_board))
        .route("/qa-runs/{run_id}", get(get_qa_run))
        .route("/{id}", get(get_task))
        .route("/{id}", patch(update_task))
        .route("/{id}", delete(delete_task))
        // Lifecycle (stubs where noted)
        .route("/{id}/execute", post(execute_task))
        .route("/{id}/feedback", post(send_feedback))
        .route("/{id}/cancel", post(cancel_task))
        .route("/{id}/approve", post(approve_task))
        .route("/{id}/request-changes", post(request_changes))
        .route("/{id}/approve-plan", post(approve_plan_stub))
        .route("/{id}/logs", get(task_logs_stream))
        .route("/{id}/changes", get(get_changes))
        .route("/{id}/qa-runs", get(list_qa_runs).post(start_qa_run))
        .route("/{id}/pr-merged", post(pr_merged))
        .route("/{id}/pr-closed", post(pr_closed))
        .route("/{id}/start", post(start_task))
        .route("/{id}/extend", post(extend_stub))
        .route("/{id}/cleanup-worktree", post(cleanup_worktree_stub))
        .route("/{id}/open-editor", post(open_editor))
        .route("/{id}/resolve-conflicts", post(resolve_conflicts))
        .route("/{id}/pr-comments", get(pr_comments_stub))
        // Deprecated (spec endpoints removed in favour of two-agent workflow)
        .route("/{id}/generate-spec", post(deprecated_gone))
        .route("/{id}/regenerate-spec", post(deprecated_gone))
        .route("/{id}/spec", patch(deprecated_gone))
        .route("/{id}/approve-spec", post(deprecated_gone))
}

// ============================================================================
// CRUD handlers
// ============================================================================

/// POST /api/tasks - Create a new task.
async fn create_task(
    State(state): State<AppState>,
    Json(input): Json<CreateTaskInput>,
) -> Result<impl IntoResponse, AppError> {
    info!(
        repository_id = %input.repository_id,
        user_input = %input.user_input,
        "POST /tasks"
    );

    let service_input: task_service::CreateTaskServiceInput = input.into();

    let task = state
        .db
        .call(move |conn| task_service::create_task(conn, &service_input))
        .await?;

    state
        .data_emitter
        .emit_change("task", "created", Some(&task.id));

    Ok((StatusCode::CREATED, Json(task)))
}

/// GET /api/tasks - List all tasks, with optional filters.
async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<ListTasksQuery>,
) -> Result<impl IntoResponse, AppError> {
    let repo_url = query.repo_url.clone();
    let repository_id = query.repository_id.clone();

    let tasks = state
        .db
        .call(move |conn| {
            task_service::get_all_tasks(conn, repo_url.as_deref(), repository_id.as_deref())
        })
        .await?;

    Ok(Json(tasks))
}

/// GET /api/tasks/qa-board - List tasks with their latest QA run.
async fn list_qa_board(
    State(state): State<AppState>,
    Query(query): Query<ListTasksQuery>,
) -> Result<impl IntoResponse, AppError> {
    let repo_url = query.repo_url.clone();
    let repository_id = query.repository_id.clone();

    let items = state
        .db
        .call(move |conn| {
            let tasks =
                task_service::get_all_tasks(conn, repo_url.as_deref(), repository_id.as_deref())?;
            let mut items = Vec::with_capacity(tasks.len());

            for task in tasks {
                let latest_run = qa_service::list_qa_runs_for_task(conn, &task.id)?
                    .into_iter()
                    .next();
                items.push(QaBoardItem { task, latest_run });
            }

            Ok::<_, AppError>(items)
        })
        .await?;

    Ok(Json(items))
}

/// GET /api/tasks/:id - Get a single task by ID.
async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &id))
        .await?;

    Ok(Json(task))
}

/// PATCH /api/tasks/:id - Update task fields.
async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateTaskInput>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "PATCH /tasks/:id");

    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::update_task(conn, &task_id, &input))
        .await?;

    state.data_emitter.emit_change("task", "updated", Some(&id));

    Ok(Json(task))
}

/// DELETE /api/tasks/:id - Delete a task.
///
/// Also cancels any running agent and reverts workspace changes.
async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "DELETE /tasks/:id");

    // 1. Load task to get repository info for cleanup
    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await;

    // 2. Cancel running agent if any
    if state.agent_service.is_running(&id).await {
        info!(id = %id, "Cancelling running agent before deleting task");
        let _ = state.agent_service.cancel_agent(&id).await;
    }

    // 3. Clean up workspace changes (fire-and-forget, don't block deletion)
    if let Ok(ref task) = task {
        let git_service = Arc::clone(&state.git_service);
        let task_id = id.clone();
        let repo_id = task.repository_id.clone();
        let db = state.db.clone();
        tokio::spawn(async move {
            // Try worktree cleanup first
            if let Ok(exists) = git_service.worktree_exists(&task_id).await {
                if exists {
                    let _ = git_service.cleanup_worktree(&task_id, true).await;
                    return;
                }
            }

            // No worktree — revert changes in the local repo
            if let Some(repo_id) = repo_id {
                let workspace = db
                    .call(move |conn| repo_service::get_repository_by_id(conn, &repo_id))
                    .await
                    .ok()
                    .flatten()
                    .and_then(|r| repo_service::get_repo_local_path(&r));

                if let Some(workspace_path) = workspace {
                    // Discard uncommitted changes
                    let _ =
                        GitService::exec_git(&["checkout", "--", "."], &workspace_path, None).await;
                    // Remove untracked files
                    let _ = GitService::exec_git(&["clean", "-fd"], &workspace_path, None).await;
                    info!(task_id = %task_id, "Workspace changes reverted");
                }
            }
        });
    }

    // 4. Delete from DB
    let task_id = id.clone();
    state
        .db
        .call(move |conn| task_service::delete_task(conn, &task_id))
        .await?;

    state.data_emitter.emit_change("task", "deleted", Some(&id));

    Ok(StatusCode::NO_CONTENT.into_response())
}

// ============================================================================
// Lifecycle handlers
// ============================================================================

/// POST /api/tasks/:id/execute - Start the agent for a task.
///
/// Accepts tasks with status: backlog, approved, failed, changes_requested.
async fn execute_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/execute");

    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await?;

    // Validate status
    let valid_statuses = [
        TaskStatus::Backlog,
        TaskStatus::Approved,
        TaskStatus::Failed,
        TaskStatus::ChangesRequested,
    ];
    if !valid_statuses.contains(&task.status) {
        return Err(AppError::Validation(format!(
            "Cannot execute task with status: {}. Valid statuses: backlog, approved, failed, changes_requested",
            task.status
        )));
    }

    let is_retry = task.status == TaskStatus::Failed;
    let is_resume = task.status == TaskStatus::ChangesRequested;

    // Update status to 'planning' SYNCHRONOUSLY before responding.
    // This ensures:
    // 1. The frontend receives the updated status and can show logs (SSE connects on planning)
    // 2. The Retry/Execute button is disabled immediately
    // Also clear error on retry.
    {
        let task_id = id.clone();
        state
            .db
            .call(move |conn| {
                task_service::update_task(
                    conn,
                    &task_id,
                    &UpdateTaskInput {
                        status: Some(TaskStatus::Planning),
                        error: if is_retry { Some(None) } else { None },
                        ..Default::default()
                    },
                )
            })
            .await?;
    }

    // Emit SSE status event so the frontend updates immediately
    state.sse_emitter.emit_status(&id, "planning").await;

    // Start agent in background
    let state_clone = state.clone();
    let id_clone = id.clone();
    let task_clone = task.clone();
    tokio::spawn(async move {
        if let Err(e) = start_agent_for_task(&state_clone, &id_clone, &task_clone).await {
            warn!(task_id = %id_clone, error = %e, "Agent failed to start on execute");

            // Update task to failed
            let tid = id_clone.clone();
            let err_msg = e.to_string();
            let _ = state_clone
                .db
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

            state_clone
                .sse_emitter
                .emit_error(&id_clone, &e.to_string())
                .await;
            state_clone
                .sse_emitter
                .emit_status(&id_clone, "failed")
                .await;
            state_clone
                .data_emitter
                .emit_change("task", "updated", Some(&id_clone));
        }
    });

    state.data_emitter.emit_change("task", "updated", Some(&id));

    Ok(Json(json!({
        "status": "started",
        "task_status": "planning",
        "message": if is_resume { "Agent resumed to address requested changes" } else { "Agent execution started" },
        "resume_mode": is_resume,
    })))
}

/// POST /api/tasks/:id/feedback - Send feedback to the agent during execution.
///
/// If the agent is running, sends the message directly to its stdin.
/// If the agent is NOT running, resumes the agent with the message as context
/// (matching the TypeScript server behavior).
async fn send_feedback(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<FeedbackBody>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/feedback");

    let message = body
        .message
        .filter(|m| !m.is_empty())
        .ok_or_else(|| AppError::Validation("Message is required".to_string()))?;

    // Build the user chat event for history storage.
    // We use store_event (not emit) because the frontend already shows user messages
    // optimistically. Broadcasting would cause duplicates in the chat UI.
    let user_chat_event = SSEEvent {
        event_type: SSEEventType::ChatMessage,
        data: json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "role": "user",
            "content": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }),
    };

    // If agent is running, send feedback directly
    if state.agent_service.is_running(&id).await {
        state.agent_service.send_feedback(&id, &message).await?;

        // Store user message in history for replay (don't broadcast)
        state.sse_emitter.store_event(&id, user_chat_event).await;

        state.data_emitter.emit_change("task", "updated", Some(&id));

        return Ok(Json(json!({ "status": "feedback_sent" })));
    }

    // Agent is NOT running — resume the agent with the user's message
    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await?;

    // Don't allow resume for terminal/draft statuses
    let terminal = [TaskStatus::Done, TaskStatus::Failed, TaskStatus::Draft];
    if terminal.contains(&task.status) {
        return Err(AppError::Validation(format!(
            "Task is in {} status. Cannot resume the agent.",
            task.status
        )));
    }

    // Store user message in history for replay (don't broadcast)
    state.sse_emitter.store_event(&id, user_chat_event).await;

    // Update status to planning so startAgent accepts it
    let task_id = id.clone();
    state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &task_id,
                &UpdateTaskInput {
                    status: Some(TaskStatus::Planning),
                    ..Default::default()
                },
            )
        })
        .await?;

    state.sse_emitter.emit_status(&id, "planning").await;

    // Start agent asynchronously with the user's message as resume context
    let state_clone = state.clone();
    let id_clone = id.clone();
    let task_clone = task.clone();
    let feedback_msg = message.clone();
    tokio::spawn(async move {
        if let Err(e) =
            resume_agent_for_task(&state_clone, &id_clone, &task_clone, &feedback_msg).await
        {
            warn!(task_id = %id_clone, error = %e, "Failed to resume agent");
            let task_id = id_clone.clone();
            let err_msg = e.to_string();
            let _ = state_clone
                .db
                .call(move |conn| {
                    task_service::update_task(
                        conn,
                        &task_id,
                        &UpdateTaskInput {
                            status: Some(TaskStatus::Failed),
                            error: Some(Some(err_msg)),
                            ..Default::default()
                        },
                    )
                })
                .await;
            state_clone
                .sse_emitter
                .emit_error(&id_clone, &e.to_string())
                .await;
            state_clone
                .sse_emitter
                .emit_status(&id_clone, "failed")
                .await;
        }
    });

    state.data_emitter.emit_change("task", "updated", Some(&id));

    Ok(Json(json!({
        "status": "agent_resumed",
        "message": "Agent resumed with your message"
    })))
}

/// POST /api/tasks/:id/cancel - Cancel task execution.
///
/// Updates the task status to `canceled` and emits SSE error + data-change events.
async fn cancel_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/cancel");

    let task_id = id.clone();
    let update = UpdateTaskInput {
        status: Some(TaskStatus::Canceled),
        error: Some(Some("Task canceled by user".to_string())),
        ..Default::default()
    };

    state
        .db
        .call(move |conn| task_service::update_task(conn, &task_id, &update))
        .await?;

    // Cancel the running agent if any
    let _ = state.agent_service.cancel_agent(&id).await;

    // Emit SSE events
    state
        .sse_emitter
        .emit_error(&id, "Task canceled by user")
        .await;
    state.sse_emitter.emit_status(&id, "canceled").await;

    // Emit data-change
    state.data_emitter.emit_change("task", "updated", Some(&id));

    Ok(Json(json!({ "status": "canceled" })))
}

/// POST /api/tasks/:id/approve - Approve changes and create PR.
async fn approve_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/approve");

    // 1. Load task & validate status
    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await?;

    if task.status != TaskStatus::AwaitingReview && task.status != TaskStatus::Review {
        return Err(AppError::Validation(format!(
            "Cannot approve task with status: {}. Expected: awaiting_review or review",
            task.status.as_str()
        )));
    }

    // 2. Set status to approved
    let task_id = id.clone();
    state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &task_id,
                &UpdateTaskInput {
                    status: Some(TaskStatus::Approved),
                    ..Default::default()
                },
            )
        })
        .await?;
    state.sse_emitter.emit_status(&id, "approved").await;

    push_and_create_pr(state, id, task).await
}

/// Shared logic for push + PR creation, used by both `approve_task` and `resolve_conflicts`.
///
/// Resolves the workspace path, commits uncommitted changes, fetches & merges the target branch,
/// pushes, and creates a PR/MR. On failure, reverts the task to `awaiting_review`.
pub async fn push_and_create_pr(
    state: AppState,
    id: String,
    task: crate::models::task::Task,
) -> Result<Json<serde_json::Value>, AppError> {
    let pr_already_exists = task.pr_url.is_some();

    // 1. Resolve workspace path
    let workspace_path = if let Some(wt) = state.git_service.get_worktree_path(&id).await {
        wt
    } else if let Some(ref repo_id) = task.repository_id {
        let repo_id = repo_id.clone();
        let repo = state
            .db
            .call(move |conn| repo_service::get_repository_by_id(conn, &repo_id))
            .await?;
        match repo {
            Some(r) => repo_service::get_repo_local_path(&r).ok_or_else(|| {
                AppError::Validation(format!("Repository path not found for repo: {}", r.url))
            })?,
            None => {
                revert_to_awaiting_review(&state, &id, "Repository not found").await;
                return Err(AppError::NotFound("Repository not found".to_string()));
            }
        }
    } else {
        revert_to_awaiting_review(&state, &id, "No workspace found for task").await;
        return Err(AppError::Validation(
            "No workspace found for task".to_string(),
        ));
    };

    // If PR already exists but workspace might have been cleaned up
    if pr_already_exists && !workspace_path.exists() {
        let task_id = id.clone();
        let pr_url = task.pr_url.clone().unwrap_or_default();
        state
            .db
            .call(move |conn| {
                task_service::update_task(
                    conn,
                    &task_id,
                    &UpdateTaskInput {
                        status: Some(TaskStatus::PrCreated),
                        error: Some(None),
                        ..Default::default()
                    },
                )
            })
            .await?;
        state
            .sse_emitter
            .emit_complete(&id, Some(&pr_url), Some("PR already exists"))
            .await;
        state.sse_emitter.emit_status(&id, "pr_created").await;
        state.data_emitter.emit_change("task", "updated", Some(&id));
        return Ok(Json(json!({ "status": "approved", "pr_url": pr_url })));
    }

    // 2. Prepare remote for PR (handles local file:// repos)
    if let Err(e) = state
        .git_service
        .prepare_worktree_remote_for_pr(&workspace_path, &task.repo_url)
        .await
    {
        revert_to_awaiting_review(&state, &id, &e.to_string()).await;
        return Err(e);
    }

    let target_branch = if task.target_branch.is_empty() {
        "main".to_string()
    } else {
        task.target_branch.clone()
    };

    // 3. Commit any uncommitted changes before merge
    let has_changes = state
        .git_service
        .has_changes(&workspace_path)
        .await
        .unwrap_or(false);
    if has_changes {
        let commit_msg = format!("feat: {}\n\nAutomated changes by Agent Board", task.title);
        state
            .sse_emitter
            .emit_log(&id, "info", "Committing uncommitted changes...", None)
            .await;
        if let Err(e) = state
            .git_service
            .commit_changes(&workspace_path, &commit_msg)
            .await
        {
            let err_msg = format!("Failed to commit changes: {e}");
            revert_to_awaiting_review(&state, &id, &err_msg).await;
            return Err(AppError::Internal(anyhow::anyhow!(err_msg)));
        }
    }

    // 4. Fetch & merge target branch
    state
        .sse_emitter
        .emit_log(
            &id,
            "info",
            &format!("Fetching latest changes from {target_branch}..."),
            None,
        )
        .await;

    if let Err(e) = state
        .git_service
        .fetch_in_worktree(&workspace_path, &target_branch)
        .await
    {
        warn!(id = %id, error = %e, "Fetch failed, continuing without update");
        state
            .sse_emitter
            .emit_log(
                &id,
                "warn",
                &format!("Could not fetch {target_branch}: {e}"),
                None,
            )
            .await;
    }

    // Merge
    let merge_ref = format!("origin/{target_branch}");
    let merge_result =
        GitService::exec_git(&["merge", &merge_ref, "--no-edit"], &workspace_path, None).await;

    if let Ok(ref r) = merge_result {
        if r.exit_code != 0 {
            // Check for merge conflicts
            let conflict_files = state
                .git_service
                .get_conflicting_files(&workspace_path)
                .await
                .unwrap_or_default();

            if !conflict_files.is_empty() {
                let conflict_json = serde_json::to_string(&conflict_files).unwrap_or_default();
                let error_msg = format!(
                    "Merge conflicts with {} in {} file(s)",
                    target_branch,
                    conflict_files.len()
                );

                let task_id = id.clone();
                state
                    .db
                    .call(move |conn| {
                        task_service::update_task(
                            conn,
                            &task_id,
                            &UpdateTaskInput {
                                status: Some(TaskStatus::MergeConflicts),
                                error: Some(Some(error_msg.clone())),
                                conflict_files: Some(Some(conflict_json)),
                                ..Default::default()
                            },
                        )
                    })
                    .await?;
                state.sse_emitter.emit_status(&id, "merge_conflicts").await;
                state
                    .sse_emitter
                    .emit_error(
                        &id,
                        &format!(
                            "Conflictos de merge detectados en {} archivo(s). Abrí VS Code para resolverlos.",
                            conflict_files.len()
                        ),
                    )
                    .await;
                state.data_emitter.emit_change("task", "updated", Some(&id));
                return Ok(Json(json!({
                    "status": "merge_conflicts",
                    "conflict_files": conflict_files,
                })));
            }

            // Not a conflict, revert
            let err_msg = format!("Merge failed: {}", r.stderr);
            revert_to_awaiting_review(&state, &id, &err_msg).await;
            return Err(AppError::Internal(anyhow::anyhow!(err_msg)));
        }
    }

    // 5. Get branch name
    let branch_name = match state.git_service.get_current_branch(&workspace_path).await {
        Ok(b) => b,
        Err(e) => {
            let err_msg = format!("Failed to get branch name: {e}");
            revert_to_awaiting_review(&state, &id, &err_msg).await;
            return Err(AppError::Internal(anyhow::anyhow!(err_msg)));
        }
    };

    state
        .sse_emitter
        .emit_log(&id, "info", &format!("Pushing branch: {branch_name}"), None)
        .await;

    // 6. Push
    if let Err(e) = state
        .git_service
        .push_branch(&workspace_path, &branch_name)
        .await
    {
        let err_msg = format!("Failed to push branch: {e}");
        revert_to_awaiting_review(&state, &id, &err_msg).await;
        return Err(AppError::Internal(anyhow::anyhow!(err_msg)));
    }

    state
        .sse_emitter
        .emit_log(&id, "info", "Branch pushed successfully", None)
        .await;

    // If PR already existed, just update status
    if pr_already_exists {
        let pr_url = task.pr_url.clone().unwrap_or_default();
        let task_id = id.clone();
        state
            .db
            .call(move |conn| {
                task_service::update_task(
                    conn,
                    &task_id,
                    &UpdateTaskInput {
                        status: Some(TaskStatus::PrCreated),
                        error: Some(None),
                        ..Default::default()
                    },
                )
            })
            .await?;
        state
            .sse_emitter
            .emit_complete(&id, Some(&pr_url), Some("Changes pushed to existing PR"))
            .await;
        state.sse_emitter.emit_status(&id, "pr_created").await;
        state.data_emitter.emit_change("task", "updated", Some(&id));
        return Ok(Json(json!({ "status": "approved", "pr_url": pr_url })));
    }

    // 7. Get remote URL for PR creation
    let raw_remote_url = state
        .git_service
        .get_remote_url(&workspace_path)
        .await
        .unwrap_or_else(|| task.repo_url.clone());
    let pr_repo_url = crate::services::gitlab_service::strip_credentials_from_url(&raw_remote_url);

    // 8. Build PR body
    let changed_files = state
        .git_service
        .get_changed_files(&workspace_path, &target_branch)
        .await
        .unwrap_or_default();
    let files_description: String = changed_files
        .iter()
        .map(|f| {
            format!(
                "- {} ({:?}: +{}/-{})",
                f.path, f.status, f.additions, f.deletions
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let pr_body = format!(
        "## Summary\n\n\
         This PR was automatically generated by Agent Board.\n\n\
         **Task:** {}\n\n\
         **Description:** {}\n\n\
         ## Changes\n\n\
         {}\n\n\
         ---\n\
         *Generated by Agent Board*\n",
        task.title,
        task.description,
        if files_description.is_empty() {
            "No file changes detected.".to_string()
        } else {
            files_description
        }
    );

    // 9. Create PR/MR based on provider
    let is_gitlab = crate::services::gitlab_service::is_gitlab_url(&pr_repo_url);

    let pr_result: Result<(String, i64), AppError> = async {
        if is_gitlab {
            state
                .sse_emitter
                .emit_log(&id, "info", "Creating GitLab Merge Request", None)
                .await;

            let gl_token = state
                .db
                .call(|conn| crate::services::secrets_service::get_gitlab_credentials(conn))
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!(
                    "Failed to read GitLab token (you may need to re-save it in Settings > Connections): {e}"
                )))?
                .ok_or_else(|| AppError::Validation(
                    "GitLab token not configured. Set it in Settings > Connections.".to_string(),
                ))?;

            let mr = crate::services::gitlab_service::create_merge_request(
                &gl_token,
                &pr_repo_url,
                &branch_name,
                &target_branch,
                &task.title,
                &pr_body,
            )
            .await?;

            Ok((mr.url, mr.number))
        } else {
            state
                .sse_emitter
                .emit_log(&id, "info", "Creating GitHub Pull Request", None)
                .await;

            let gh_token = state
                .db
                .call(|conn| crate::services::secrets_service::get_github_credentials(conn))
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!(
                    "Failed to read GitHub token (you may need to re-save it in Settings > Connections): {e}"
                )))?
                .ok_or_else(|| AppError::Validation(
                    "GitHub token not configured. Set it in Settings > Connections.".to_string(),
                ))?;

            // Parse owner/repo from URL
            let re = regex_lite::Regex::new(r"github\.com[/:]([^/]+)/([^/\s.]+)").unwrap();
            let caps = re.captures(&pr_repo_url).ok_or_else(|| {
                AppError::Validation(format!("Cannot parse GitHub owner/repo from URL: {pr_repo_url}"))
            })?;
            let owner = caps.get(1).unwrap().as_str();
            let repo = caps.get(2).unwrap().as_str().trim_end_matches(".git");

            let pr = crate::services::github_service::create_pull_request(
                &gh_token,
                owner,
                repo,
                &branch_name,
                &target_branch,
                &task.title,
                &pr_body,
            )
            .await?;

            Ok((pr.html_url, pr.number))
        }
    }
    .await;

    match pr_result {
        Ok((pr_url, pr_number)) => {
            let task_id = id.clone();
            let pr_url_clone = pr_url.clone();
            state
                .db
                .call(move |conn| {
                    task_service::update_task(
                        conn,
                        &task_id,
                        &UpdateTaskInput {
                            status: Some(TaskStatus::PrCreated),
                            pr_url: Some(Some(pr_url_clone)),
                            ..Default::default()
                        },
                    )
                })
                .await?;

            state
                .sse_emitter
                .emit_log(&id, "info", &format!("PR created: {pr_url}"), None)
                .await;
            state
                .sse_emitter
                .emit_complete(
                    &id,
                    Some(&pr_url),
                    Some(&format!("PR #{pr_number} created. Worktree preserved.")),
                )
                .await;
            state.sse_emitter.emit_status(&id, "pr_created").await;
            state.data_emitter.emit_change("task", "updated", Some(&id));

            Ok(Json(json!({ "status": "approved", "pr_url": pr_url })))
        }
        Err(e) => {
            let err_msg = format!("Failed to create PR: {e}");
            revert_to_awaiting_review(&state, &id, &err_msg).await;
            Err(AppError::Internal(anyhow::anyhow!(err_msg)))
        }
    }
}

/// Reverts a task to `awaiting_review` after a PR creation failure.
async fn revert_to_awaiting_review(state: &AppState, task_id: &str, error_msg: &str) {
    let tid = task_id.to_string();
    let err = error_msg.to_string();
    let _ = state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &tid,
                &UpdateTaskInput {
                    status: Some(TaskStatus::AwaitingReview),
                    error: Some(Some(err)),
                    ..Default::default()
                },
            )
        })
        .await;
    state.sse_emitter.emit_error(task_id, error_msg).await;
    state
        .sse_emitter
        .emit_status(task_id, "awaiting_review")
        .await;
}

/// POST /api/tasks/:id/request-changes - Request changes on a task.
///
/// Updates status to `changes_requested` and emits a data-change event.
async fn request_changes(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RequestChangesBody>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/request-changes");

    // Validate feedback is present
    let feedback = body
        .feedback
        .filter(|f| !f.is_empty())
        .ok_or_else(|| AppError::Validation("Feedback is required".to_string()))?;

    let task_id = id.clone();
    let update = UpdateTaskInput {
        status: Some(TaskStatus::ChangesRequested),
        error: Some(Some(format!("Changes requested: {feedback}"))),
        ..Default::default()
    };

    let task = state
        .db
        .call(move |conn| task_service::update_task(conn, &task_id, &update))
        .await?;

    state.data_emitter.emit_change("task", "updated", Some(&id));

    Ok(Json(json!({
        "status": "changes_requested",
        "message": "Changes requested. Call POST /tasks/:id/execute to resume the agent.",
        "task": task,
    })))
}

/// POST /api/tasks/:id/approve-plan - STUB (Agent System, Phase 4).
async fn approve_plan_stub() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "Not Implemented",
            "message": "Plan approval requires Agent service (Phase 4)"
        })),
    )
}

/// GET /api/tasks/:id/logs - SSE stream of task log events.
///
/// On connect:
/// 1. Replays historical events (logs, chat messages, tool activity) — excluding
///    terminal events (status, complete, error) which are handled separately.
/// 2. Sends the current task status.
/// 3. If the task is in a terminal state, sends the appropriate terminal event
///    so the client knows to stop reconnecting.
/// 4. Otherwise, streams new live events via the broadcast channel.
///
/// This matches the TypeScript server behavior: historical logs + chat history
/// first, then status, then terminal event if applicable.
async fn task_logs_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!(id = %id, "GET /tasks/:id/logs (SSE)");

    // 1. Fetch historical events for replay (in-memory first, then DB fallback)
    let history = state.sse_emitter.get_history(&id).await;
    let history = if history.is_empty() {
        // Server may have restarted — try loading from DB
        let task_id_for_db = id.clone();
        let db_events = state
            .db
            .call(move |conn| {
                crate::services::task_event_service::get_events_for_task(conn, &task_id_for_db)
            })
            .await
            .unwrap_or_default();

        if !db_events.is_empty() {
            info!(id = %id, count = db_events.len(), "Warming SSE cache from DB");
            // Warm in-memory cache without re-persisting to DB
            for event in &db_events {
                state
                    .sse_emitter
                    .store_event_no_persist(&id, event.clone())
                    .await;
            }
        }
        db_events
    } else {
        history
    };

    // 2. Get current task from DB for status and terminal event data
    let task_id_for_status = id.clone();
    let task_opt = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id_for_status))
        .await
        .ok();

    let current_status = task_opt.as_ref().map(|t| t.status.to_string());

    // 3. Subscribe to new events (after fetching history to avoid gaps)
    let rx = state.sse_emitter.subscribe(&id).await;

    // Build replay events: filter OUT terminal events (Status, Complete, Error)
    // to match the TS server behavior. The TS server replays logs + chat history
    // separately from status/terminal events. If we replay Complete/Error events,
    // the frontend closes the SSE connection immediately, potentially before
    // processing the chat messages that precede them.
    let mut replay_events: Vec<Result<Event, Infallible>> = history
        .into_iter()
        .filter(|event| {
            !matches!(
                event.event_type,
                SSEEventType::Status
                    | SSEEventType::Complete
                    | SSEEventType::Error
                    | SSEEventType::AwaitingReview
            )
        })
        .map(|event| {
            let event_name = event.event_type.as_event_name();
            let data = serde_json::to_string(&event.data).unwrap_or_default();
            Ok(Event::default().event(event_name).data(data))
        })
        .collect();

    // Append current status (always sent, matching TS server)
    if let Some(ref status) = current_status {
        let data = serde_json::to_string(&json!({ "status": status })).unwrap_or_default();
        replay_events.push(Ok(Event::default().event("status").data(data)));
    }

    // Determine if task is in a terminal state
    let is_terminal = matches!(
        current_status.as_deref(),
        Some("done") | Some("failed") | Some("canceled")
    );

    if is_terminal {
        // Append terminal event so the client stops reconnecting
        if let Some(ref task) = task_opt {
            match current_status.as_deref() {
                Some("done") => {
                    let data = serde_json::to_string(&json!({
                        "pr_url": task.pr_url.as_deref().unwrap_or("")
                    }))
                    .unwrap_or_default();
                    replay_events.push(Ok(Event::default().event("complete").data(data)));
                }
                Some("failed") | Some("canceled") => {
                    let msg = task.error.as_deref().unwrap_or("Task failed");
                    let data =
                        serde_json::to_string(&json!({ "message": msg })).unwrap_or_default();
                    replay_events.push(Ok(Event::default().event("error").data(data)));
                }
                _ => {}
            }
        }
    } else {
        // Non-terminal: append awaiting_review for review/plan_review states
        match current_status.as_deref() {
            Some("review") | Some("awaiting_review") => {
                let data = serde_json::to_string(&json!({
                    "message": "Agent completed. Review changes before creating PR."
                }))
                .unwrap_or_default();
                replay_events.push(Ok(Event::default().event("awaiting_review").data(data)));
            }
            Some("plan_review") => {
                let data = serde_json::to_string(&json!({
                    "message": "Plan created. Review the plan and approve to start implementation."
                }))
                .unwrap_or_default();
                replay_events.push(Ok(Event::default().event("awaiting_review").data(data)));
            }
            _ => {}
        }
    }

    let replay_stream = tokio_stream::iter(replay_events);

    let stream: Pin<Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>> =
        if is_terminal {
            // Terminal task: only replay, no live stream
            Box::pin(replay_stream)
        } else {
            // Non-terminal: chain replay with live broadcast events
            let id_for_filter = id.clone();
            let id_for_map = id.clone();
            let live_stream = BroadcastStream::new(rx)
                .filter_map(move |result| match result {
                    Ok(event) => Some(event),
                    Err(e) => {
                        warn!(task_id = %id_for_filter, error = %e, "BroadcastStream recv error");
                        None
                    }
                })
                .map(move |event| {
                    info!(
                        task_id = %id_for_map,
                        event_type = %event.event_type.as_event_name(),
                        "SSE live stream yielding event to client"
                    );
                    let event_name = event.event_type.as_event_name();
                    let data = serde_json::to_string(&event.data).unwrap_or_default();
                    Ok(Event::default().event(event_name).data(data))
                });
            Box::pin(replay_stream.chain(live_stream))
        };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("heartbeat"),
    )
}

/// GET /api/tasks/:id/changes - Return changes data for a task.
///
/// First tries persisted `changes_data` from the DB, then falls back to
/// reading the live workspace directory (matching the TS server behavior).
async fn get_changes(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await?;

    // 1. Try persisted changes_data from DB
    if let Some(ref changes_json) = task.changes_data {
        match serde_json::from_str::<serde_json::Value>(changes_json) {
            Ok(parsed) => {
                // Verify it has actual content (not just empty arrays)
                let has_files = parsed
                    .get("files")
                    .and_then(|f| f.as_array())
                    .map_or(false, |a| !a.is_empty());
                let has_diff = parsed
                    .get("diff")
                    .and_then(|d| d.as_str())
                    .map_or(false, |s| !s.is_empty());
                if has_files || has_diff {
                    return Ok(Json(parsed).into_response());
                }
            }
            Err(_) => {
                warn!(task_id = %task.id, "Failed to parse persisted changes_data");
            }
        }
    }

    // 2. Fallback: try to read live workspace directory
    if let Some(ref repo_id) = task.repository_id {
        let repo_id = repo_id.clone();
        let repo = state
            .db
            .call(move |conn| repo_service::get_repository_by_id(conn, &repo_id))
            .await?;

        if let Some(repo) = repo {
            if let Some(local_path) = repo_service::get_repo_local_path(&repo) {
                info!(task_id = %id, path = %local_path.display(), "Trying live workspace for changes");

                // Prefer base_commit (scoped to this task) over target_branch
                // (which includes changes from all tasks on the branch).
                let base_ref = if let Some(ref bc) = task.base_commit {
                    // Verify the commit still exists in this workspace
                    if let Ok(r) = GitService::exec_git(
                        &["rev-parse", "--verify", &format!("{bc}^{{commit}}")],
                        &local_path,
                        None,
                    )
                    .await
                    {
                        if r.exit_code == 0 {
                            Some(bc.clone())
                        } else {
                            let tb = if task.target_branch.is_empty() {
                                None
                            } else {
                                Some(task.target_branch.as_str())
                            };
                            resolve_workspace_base(&local_path, tb).await
                        }
                    } else {
                        let tb = if task.target_branch.is_empty() {
                            None
                        } else {
                            Some(task.target_branch.as_str())
                        };
                        resolve_workspace_base(&local_path, tb).await
                    }
                } else {
                    let tb = if task.target_branch.is_empty() {
                        None
                    } else {
                        Some(task.target_branch.as_str())
                    };
                    resolve_workspace_base(&local_path, tb).await
                };
                let diff = get_workspace_diff_text(&local_path, base_ref.as_deref()).await;
                let files = get_workspace_changed_files(&local_path, base_ref.as_deref()).await;

                if !files.is_empty() || !diff.is_empty() {
                    return Ok(Json(json!({
                        "files": files,
                        "diff": diff,
                    }))
                    .into_response());
                }
            }
        }
    }

    // 3. No changes found
    Ok(Json(json!({ "files": [], "diff": "" })).into_response())
}

async fn list_qa_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let task_id = id.clone();
    state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await?;

    let task_id = id.clone();
    let runs = state
        .db
        .call(move |conn| qa_service::list_qa_runs_for_task(conn, &task_id))
        .await?;

    Ok(Json(runs))
}

async fn get_qa_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let run = state
        .db
        .call(move |conn| qa_service::get_qa_run_by_id(conn, &run_id))
        .await?;

    Ok(Json(run))
}

async fn start_qa_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<CreateQaRunInput>,
) -> Result<impl IntoResponse, AppError> {
    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await?;

    let command = validate_qa_command(input.test_command.as_deref())?;
    let target_url = input
        .target_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let workspace_path = resolve_qa_workspace(&state, &task).await?;
    let (chrome_mcp_enabled, chrome_mcp_url) = state
        .db
        .call(|conn| settings_service::get_chrome_mcp_config(conn))
        .await?;
    let chrome_mcp_url = if chrome_mcp_enabled {
        Some(
            chrome_mcp_url
                .map(|url| url.trim().to_string())
                .filter(|url| !url.is_empty())
                .ok_or_else(|| {
                    AppError::Validation(
                        "Chrome MCP is enabled but no URL is configured".to_string(),
                    )
                })?,
        )
    } else {
        None
    };

    let task_id = id.clone();
    let command_for_db = command.clone();
    let target_url_for_db = target_url.clone();
    let run = state
        .db
        .call(move |conn| {
            qa_service::create_qa_run(
                conn,
                &task_id,
                &command_for_db,
                target_url_for_db.as_deref(),
            )
        })
        .await?;

    state
        .data_emitter
        .emit_change("qa_run", "created", Some(&run.id));
    state.data_emitter.emit_change("task", "updated", Some(&id));

    let db = state.db.clone();
    let data_emitter = Arc::clone(&state.data_emitter);
    let run_id = run.id.clone();
    let task_id_for_emit = id.clone();
    tokio::spawn(async move {
        let result = execute_qa_command(
            &command,
            &workspace_path,
            target_url.as_deref(),
            chrome_mcp_url.as_deref(),
        )
        .await;
        let status = if result.exit_code == Some(0) {
            QaRunStatus::Passed
        } else {
            QaRunStatus::Failed
        };
        let completed_at = chrono::Utc::now().to_rfc3339();
        let report_path = discover_qa_artifact(&workspace_path, "playwright-report");
        let trace_path = discover_qa_artifact(&workspace_path, "test-results");
        let run_id_for_db = run_id.clone();
        let _ = db
            .call(move |conn| {
                qa_service::update_qa_run(
                    conn,
                    &run_id_for_db,
                    &UpdateQaRunInput {
                        status: Some(status),
                        report_path: Some(report_path),
                        trace_path: Some(trace_path),
                        stdout: Some(result.stdout),
                        stderr: Some(result.stderr),
                        exit_code: Some(result.exit_code),
                        completed_at: Some(Some(completed_at)),
                        ..Default::default()
                    },
                )
            })
            .await;
        data_emitter.emit_change("qa_run", "updated", Some(&run_id));
        data_emitter.emit_change("task", "updated", Some(&task_id_for_emit));
    });

    Ok((StatusCode::ACCEPTED, Json(run)))
}

async fn resolve_qa_workspace(
    state: &AppState,
    task: &crate::models::task::Task,
) -> Result<std::path::PathBuf, AppError> {
    if let Some(worktree_path) = state.git_service.get_worktree_path(&task.id).await {
        return Ok(worktree_path);
    }

    let repo_id = task.repository_id.as_ref().ok_or_else(|| {
        AppError::Validation("Task has no repository_id for QA execution".to_string())
    })?;
    let repo_id = repo_id.clone();
    let repo = state
        .db
        .call(move |conn| repo_service::get_repository_by_id(conn, &repo_id))
        .await?
        .ok_or_else(|| AppError::NotFound("Repository not found".to_string()))?;

    if let Some(local_path) = repo_service::get_repo_local_path(&repo) {
        return Ok(local_path);
    }

    let worktree_result = state
        .git_service
        .setup_worktree(&task.id, &repo.url, &task.target_branch)
        .await?;
    Ok(worktree_result.worktree_path)
}

fn validate_qa_command(command: Option<&str>) -> Result<String, AppError> {
    let command = command
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("npx playwright test --trace on --reporter=line");

    if command.len() > 300 {
        return Err(AppError::Validation(
            "QA command must be 300 characters or less".to_string(),
        ));
    }

    if command
        .chars()
        .any(|ch| matches!(ch, ';' | '&' | '|' | '>' | '<' | '`'))
    {
        return Err(AppError::Validation(
            "QA command cannot include shell control characters".to_string(),
        ));
    }

    let lower = command.to_ascii_lowercase();
    let allowed = lower.starts_with("npx playwright test")
        || lower.starts_with("npm exec playwright test")
        || lower.starts_with("pnpm playwright test")
        || lower.starts_with("yarn playwright test")
        || lower == "npm run test:e2e"
        || lower.starts_with("npm run test:e2e --")
        || lower == "pnpm test:e2e"
        || lower.starts_with("pnpm test:e2e --")
        || lower == "yarn test:e2e"
        || lower.starts_with("yarn test:e2e --");

    if !allowed {
        return Err(AppError::Validation(
            "QA command must be a Playwright or test:e2e command".to_string(),
        ));
    }

    Ok(command.to_string())
}

struct QaCommandResult {
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i64>,
}

async fn execute_qa_command(
    command: &str,
    workspace_path: &std::path::Path,
    target_url: Option<&str>,
    chrome_mcp_url: Option<&str>,
) -> QaCommandResult {
    let mut cmd = if cfg!(windows) {
        let mut command_builder = tokio::process::Command::new("cmd");
        command_builder.args(["/C", command]);
        command_builder
    } else {
        let mut command_builder = tokio::process::Command::new("sh");
        command_builder.args(["-lc", command]);
        command_builder
    };
    cmd.current_dir(workspace_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(target_url) = target_url {
        cmd.env("PLAYWRIGHT_TEST_BASE_URL", target_url);
        cmd.env("BASE_URL", target_url);
    }

    if let Some(chrome_mcp_url) = chrome_mcp_url {
        cmd.env("CHROME_MCP_URL", chrome_mcp_url);
        cmd.env("MCP_CHROME_URL", chrome_mcp_url);
    }

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match tokio::time::timeout(Duration::from_secs(10 * 60), cmd.output()).await {
        Ok(Ok(output)) => QaCommandResult {
            stdout: Some(truncate_qa_output(&String::from_utf8_lossy(&output.stdout))),
            stderr: Some(truncate_qa_output(&String::from_utf8_lossy(&output.stderr))),
            exit_code: Some(output.status.code().unwrap_or(1) as i64),
        },
        Ok(Err(e)) => QaCommandResult {
            stdout: None,
            stderr: Some(format!("Failed to execute QA command: {e}")),
            exit_code: Some(1),
        },
        Err(_) => QaCommandResult {
            stdout: None,
            stderr: Some("QA command timed out after 10 minutes".to_string()),
            exit_code: Some(124),
        },
    }
}

fn truncate_qa_output(value: &str) -> String {
    const MAX_CHARS: usize = 120_000;
    if value.chars().count() <= MAX_CHARS {
        value.to_string()
    } else {
        let mut truncated = value.chars().take(MAX_CHARS).collect::<String>();
        truncated.push_str("\n[output truncated]");
        truncated
    }
}

fn discover_qa_artifact(workspace_path: &std::path::Path, folder: &str) -> Option<String> {
    if workspace_path.join(folder).exists() {
        Some(folder.to_string())
    } else {
        None
    }
}

/// Resolves the best base ref for diffing in a workspace.
async fn resolve_workspace_base(
    workspace_path: &std::path::Path,
    target_branch: Option<&str>,
) -> Option<String> {
    if let Some(branch) = target_branch {
        let remote_ref = format!("origin/{branch}");
        if let Ok(r) = GitService::exec_git(
            &["rev-parse", "--verify", &remote_ref],
            workspace_path,
            None,
        )
        .await
        {
            if r.exit_code == 0 {
                return Some(remote_ref);
            }
        }
        if let Ok(r) =
            GitService::exec_git(&["rev-parse", "--verify", branch], workspace_path, None).await
        {
            if r.exit_code == 0 {
                return Some(branch.to_string());
            }
        }
    }
    for candidate in &["origin/main", "origin/master", "main", "master"] {
        if let Ok(r) =
            GitService::exec_git(&["rev-parse", "--verify", candidate], workspace_path, None).await
        {
            if r.exit_code == 0 {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

/// Gets the full diff text from a workspace.
async fn get_workspace_diff_text(
    workspace_path: &std::path::Path,
    base_ref: Option<&str>,
) -> String {
    let mut diff = String::new();
    if let Some(base) = base_ref {
        if let Ok(r) = GitService::exec_git(&["diff", base, "HEAD"], workspace_path, None).await {
            if r.exit_code == 0 && !r.stdout.is_empty() {
                diff.push_str(&r.stdout);
                diff.push('\n');
            }
        }
    }
    if let Ok(r) = GitService::exec_git(&["diff", "--cached"], workspace_path, None).await {
        if r.exit_code == 0 && !r.stdout.is_empty() {
            diff.push_str(&r.stdout);
            diff.push('\n');
        }
    }
    if let Ok(r) = GitService::exec_git(&["diff"], workspace_path, None).await {
        if r.exit_code == 0 && !r.stdout.is_empty() {
            diff.push_str(&r.stdout);
            diff.push('\n');
        }
    }
    diff
}

/// Gets the list of changed files with status, line counts, and content.
async fn get_workspace_changed_files(
    workspace_path: &std::path::Path,
    base_ref: Option<&str>,
) -> Vec<serde_json::Value> {
    let mut file_statuses: std::collections::HashMap<String, &str> =
        std::collections::HashMap::new();

    // Committed changes via name-status
    let diff_args: Vec<&str> = if let Some(base) = base_ref {
        vec!["diff", "--name-status", base, "HEAD"]
    } else {
        vec!["diff", "--name-status", "--root", "HEAD"]
    };

    if let Ok(r) = GitService::exec_git(&diff_args, workspace_path, None).await {
        if r.exit_code == 0 {
            for line in r.stdout.lines().filter(|l| !l.is_empty()) {
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

    // Uncommitted changes via status --porcelain
    // NOTE: exec_git() trims the full stdout, which can strip the leading space
    // from " M filename" on the first line. Parse defensively.
    if let Ok(r) = GitService::exec_git(&["status", "--porcelain"], workspace_path, None).await {
        if r.exit_code == 0 {
            for line in r.stdout.lines().filter(|l| !l.is_empty()) {
                let file_path = if line.len() >= 4 && line.as_bytes()[2] == b' ' {
                    &line[3..]
                } else if line.len() >= 3 && line.as_bytes()[1] == b' ' {
                    &line[2..]
                } else {
                    continue;
                };
                let file_path = file_path.trim().to_string();
                if file_path.is_empty() {
                    continue;
                }

                let status = if line.contains("??") || line.contains('A') {
                    "added"
                } else if line.contains('D') {
                    "deleted"
                } else {
                    "modified"
                };
                file_statuses.entry(file_path).or_insert(status);
            }
        }
    }

    let mut files = Vec::new();
    let max_content_size = 500_000; // 500KB limit per file

    for (file_path, status) in &file_statuses {
        let (mut additions, mut deletions) = (0i64, 0i64);

        // Try committed numstat first
        let numstat_args: Vec<&str> = if let Some(base) = base_ref {
            vec!["diff", "--numstat", base, "HEAD", "--", file_path]
        } else {
            vec!["diff", "--numstat", "--root", "HEAD", "--", file_path]
        };
        if let Ok(r) = GitService::exec_git(&numstat_args, workspace_path, None).await {
            if r.exit_code == 0 && !r.stdout.is_empty() {
                let parts: Vec<&str> = r.stdout.split('\t').collect();
                if parts.len() >= 2 {
                    additions = parts[0].trim().parse().unwrap_or(0);
                    deletions = parts[1].trim().parse().unwrap_or(0);
                }
            }
        }

        // Fallback: uncommitted numstat
        if additions == 0 && deletions == 0 && *status != "deleted" {
            if let Ok(r) = GitService::exec_git(
                &["diff", "--numstat", "--", file_path],
                workspace_path,
                None,
            )
            .await
            {
                if r.exit_code == 0 && !r.stdout.is_empty() {
                    let parts: Vec<&str> = r.stdout.split('\t').collect();
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
                if let Some(content) =
                    read_file_content(workspace_path, file_path, max_content_size).await
                {
                    if additions == 0 {
                        additions = content.lines().count() as i64;
                        file_json["additions"] = json!(additions);
                    }
                    file_json["newContent"] = json!(content);
                }
            }
            "deleted" => {
                if let Some(base) = base_ref {
                    if let Some(content) =
                        read_file_at_ref(workspace_path, file_path, base, max_content_size).await
                    {
                        file_json["oldContent"] = json!(content);
                    }
                }
                file_json["newContent"] = json!("");
            }
            "modified" => {
                if let Some(base) = base_ref {
                    if let Some(content) =
                        read_file_at_ref(workspace_path, file_path, base, max_content_size).await
                    {
                        file_json["oldContent"] = json!(content);
                    }
                } else {
                    file_json["oldContent"] = json!("");
                }
                if let Some(content) =
                    read_file_content(workspace_path, file_path, max_content_size).await
                {
                    file_json["newContent"] = json!(content);
                }
            }
            _ => {}
        }

        files.push(file_json);
    }
    files
}

/// Reads a file from the workspace directory.
async fn read_file_content(
    workspace_path: &std::path::Path,
    file_path: &str,
    max_size: usize,
) -> Option<String> {
    let full_path = workspace_path.join(file_path);
    match tokio::fs::read_to_string(&full_path).await {
        Ok(content) if content.len() <= max_size => Some(content),
        _ => None,
    }
}

/// Reads a file at a specific git ref.
async fn read_file_at_ref(
    workspace_path: &std::path::Path,
    file_path: &str,
    git_ref: &str,
    max_size: usize,
) -> Option<String> {
    let spec = format!("{git_ref}:{file_path}");
    match GitService::exec_git(&["show", &spec], workspace_path, None).await {
        Ok(r) if r.exit_code == 0 && r.stdout.len() <= max_size => Some(r.stdout),
        _ => None,
    }
}

/// POST /api/tasks/:id/pr-merged - Mark PR as merged, set status to done.
async fn pr_merged(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/pr-merged");

    let task_id = id.clone();
    let update = UpdateTaskInput {
        status: Some(TaskStatus::Done),
        ..Default::default()
    };

    state
        .db
        .call(move |conn| task_service::update_task(conn, &task_id, &update))
        .await?;

    state.data_emitter.emit_change("task", "updated", Some(&id));

    Ok(Json(json!({
        "status": "done",
        "message": "PR marked as merged."
    })))
}

/// POST /api/tasks/:id/pr-closed - Mark PR as closed, set status to canceled.
async fn pr_closed(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/pr-closed");

    let task_id = id.clone();
    let update = UpdateTaskInput {
        status: Some(TaskStatus::Canceled),
        ..Default::default()
    };

    state
        .db
        .call(move |conn| task_service::update_task(conn, &task_id, &update))
        .await?;

    state.data_emitter.emit_change("task", "updated", Some(&id));

    Ok(Json(json!({
        "status": "canceled",
        "message": "PR marked as closed."
    })))
}

/// POST /api/tasks/:id/start - Start task execution directly (chat mode).
///
/// Replaces the old generate-spec -> approve-spec -> execute flow.
/// Creates a branch and starts the agent in one step.
/// Accepts tasks with status: draft, failed.
async fn start_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/start");

    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await?;

    // Validate status - only draft or failed
    if task.status != TaskStatus::Draft && task.status != TaskStatus::Failed {
        return Err(AppError::Validation(format!(
            "Cannot start task with status: {}. Expected: draft or failed",
            task.status
        )));
    }

    // Generate branch name: feature/{title-slug}-{task-id-prefix}
    let title_slug: String = task
        .title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let title_slug = &title_slug[..title_slug.len().min(40)];
    let task_suffix = &id[..id.len().min(8)];
    let branch_name = format!("feature/{title_slug}-{task_suffix}");

    let is_retry = task.status == TaskStatus::Failed;

    // Update task: set branch, status to planning, clear error if retry
    let task_id = id.clone();
    let branch_clone = branch_name.clone();
    state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &task_id,
                &UpdateTaskInput {
                    branch_name: Some(Some(branch_clone)),
                    status: Some(TaskStatus::Planning),
                    error: if is_retry { Some(None) } else { None },
                    ..Default::default()
                },
            )
        })
        .await?;

    // Emit SSE status change
    state.sse_emitter.emit_status(&id, "planning").await;

    // Start agent in background
    let state_clone = state.clone();
    let id_clone = id.clone();
    let task_clone = task.clone();
    tokio::spawn(async move {
        if let Err(e) = start_agent_for_task(&state_clone, &id_clone, &task_clone).await {
            warn!(task_id = %id_clone, error = %e, "Failed to start agent");
            // Update task to failed
            let task_id = id_clone.clone();
            let err_msg = e.to_string();
            let _ = state_clone
                .db
                .call(move |conn| {
                    task_service::update_task(
                        conn,
                        &task_id,
                        &UpdateTaskInput {
                            status: Some(TaskStatus::Failed),
                            error: Some(Some(err_msg)),
                            ..Default::default()
                        },
                    )
                })
                .await;
            state_clone
                .sse_emitter
                .emit_status(&id_clone, "failed")
                .await;
            state_clone
                .sse_emitter
                .emit_error(&id_clone, &e.to_string())
                .await;
        }
    });

    state.data_emitter.emit_change("task", "updated", Some(&id));

    Ok(Json(
        json!({ "status": "started", "message": "Agent started" }),
    ))
}

/// POST /api/tasks/:id/extend - STUB (Agent System, Phase 4).
async fn extend_stub() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "Not Implemented",
            "message": "Timeout extension requires Agent service (Phase 4)"
        })),
    )
}

/// POST /api/tasks/:id/cleanup-worktree - STUB (Git service, Phase 3).
async fn cleanup_worktree_stub() -> impl IntoResponse {
    Json(json!({
        "status": "no_worktree",
        "message": "No worktree exists for this task"
    }))
}

/// POST /api/tasks/:id/open-editor - Open VS Code at the worktree path.
///
/// Only valid for tasks with status `merge_conflicts`.
async fn open_editor(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/open-editor");

    // 1. Load task
    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await?;

    // 2. Validate status
    if task.status != TaskStatus::MergeConflicts {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Invalid task status",
                "message": format!(
                    "Cannot open editor for task with status: {}. Expected: merge_conflicts",
                    task.status.as_str()
                )
            })),
        ));
    }

    // 3. Get worktree path
    let worktree_path = state.git_service.get_worktree_path(&id).await;
    let worktree_path = match worktree_path {
        Some(p) => p,
        None => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "No worktree",
                    "message": "No worktree found for this task."
                })),
            ));
        }
    };

    // 4. Open VS Code (fire-and-forget)
    // On Windows, `code` is actually `code.cmd` — must run via shell.
    let path_str = worktree_path.display().to_string();
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "code", &path_str]);
        c
    } else {
        let mut c = std::process::Command::new("code");
        c.arg(&path_str);
        c
    };
    match cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {
            info!(id = %id, path = %path_str, "VS Code opened for merge conflict resolution");
        }
        Err(e) => {
            warn!(id = %id, error = %e, "Failed to launch VS Code");
        }
    }

    Ok((
        StatusCode::OK,
        Json(json!({ "opened": true, "path": path_str })),
    ))
}

/// POST /api/tasks/:id/resolve-conflicts - Mark conflicts as resolved and create PR.
///
/// Called when the user finishes resolving merge conflicts in VS Code.
/// Validates that no conflict markers remain, completes the merge commit,
/// then pushes and creates the PR.
async fn resolve_conflicts(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /tasks/:id/resolve-conflicts");

    // 1. Load task & validate status
    let task_id = id.clone();
    let task = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &task_id))
        .await?;

    if task.status != TaskStatus::MergeConflicts {
        return Err(AppError::Validation(format!(
            "Cannot resolve conflicts for task with status: {}. Expected: merge_conflicts",
            task.status.as_str()
        )));
    }

    // 2. Get worktree path
    let worktree_path = state
        .git_service
        .get_worktree_path(&id)
        .await
        .ok_or_else(|| AppError::Validation("No worktree found for this task.".to_string()))?;

    // 3. Parse conflict files from task, or detect via git
    let mut conflict_file_list: Vec<String> = task
        .conflict_files
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    if conflict_file_list.is_empty() {
        conflict_file_list = state
            .git_service
            .get_conflicting_files(&worktree_path)
            .await
            .unwrap_or_default();
    }

    // 4. Check for remaining conflict markers
    let files_with_markers = state
        .git_service
        .has_conflict_markers(&worktree_path, &conflict_file_list)
        .await?;

    if !files_with_markers.is_empty() {
        return Ok((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Aún hay archivos con conflictos",
                "files": files_with_markers
            })),
        )
            .into_response());
    }

    // 5. Stage all changes and complete the merge commit
    if let Err(e) = GitService::exec_git_or_throw(&["add", "."], &worktree_path, None).await {
        let err_msg = format!("Failed to stage changes: {e}");
        return Err(AppError::Internal(anyhow::anyhow!(err_msg)));
    }
    if let Err(e) =
        GitService::exec_git_or_throw(&["commit", "--no-edit"], &worktree_path, None).await
    {
        let err_msg = format!("Failed to complete merge commit: {e}");
        return Err(AppError::Internal(anyhow::anyhow!(err_msg)));
    }

    // 6. Clear conflict data, set status to approved
    let task_id = id.clone();
    state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &task_id,
                &UpdateTaskInput {
                    status: Some(TaskStatus::Approved),
                    conflict_files: Some(None),
                    error: Some(None),
                    ..Default::default()
                },
            )
        })
        .await?;
    state.sse_emitter.emit_status(&id, "approved").await;
    state
        .sse_emitter
        .emit_log(&id, "info", "Conflictos resueltos, creando PR...", None)
        .await;

    // 7. Push and create PR asynchronously
    let state_clone = state.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        if let Err(e) = push_and_create_pr(state_clone.clone(), id_clone.clone(), task).await {
            let err_msg = format!("Failed to create PR: {e}");
            warn!(id = %id_clone, error = %err_msg, "PR creation failed after conflict resolution");
            let task_id = id_clone.clone();
            let err_clone = err_msg.clone();
            let _ = state_clone
                .db
                .call(move |conn| {
                    task_service::update_task(
                        conn,
                        &task_id,
                        &UpdateTaskInput {
                            status: Some(TaskStatus::AwaitingReview),
                            error: Some(Some(err_clone)),
                            ..Default::default()
                        },
                    )
                })
                .await;
            state_clone
                .sse_emitter
                .emit_status(&id_clone, "awaiting_review")
                .await;
            state_clone
                .sse_emitter
                .emit_error(&id_clone, &err_msg)
                .await;
        }
    });

    Ok(Json(json!({
        "status": "resolving",
        "message": "Conflictos resueltos. Creando PR..."
    }))
    .into_response())
}

/// GET /api/tasks/:id/pr-comments - STUB: return empty comments array.
async fn pr_comments_stub() -> impl IntoResponse {
    Json(json!({ "comments": [] }))
}

// ============================================================================
// Agent launch helper
// ============================================================================

/// Resolves the workspace (worktree) and repository context for a task.
///
/// Creates an isolated git worktree per task, matching the TS server behavior.
/// If the worktree already exists (e.g. on retry), it is reused.
///
/// Returns `(workspace_path, repo_context_string, task)` — the task may have
/// its `target_branch` updated if the git service resolved a different branch.
async fn resolve_workspace(
    state: &AppState,
    task_id: &str,
    task: &mut crate::models::task::Task,
) -> Result<(std::path::PathBuf, Option<String>), AppError> {
    let repo_id = task.repository_id.as_ref().ok_or_else(|| {
        AppError::Validation(
            "Task has no repository_id - cannot determine working directory".to_string(),
        )
    })?;

    let repo_id_clone = repo_id.clone();
    let repo = state
        .db
        .call(move |conn| repo_service::get_repository_by_id(conn, &repo_id_clone))
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Repository not found: {}",
                task.repository_id.as_deref().unwrap_or("unknown")
            ))
        })?;

    // Set up an isolated worktree for this task
    state
        .sse_emitter
        .emit_log(task_id, "info", "Setting up worktree for task", None)
        .await;

    let worktree_result = state
        .git_service
        .setup_worktree(task_id, &repo.url, &task.target_branch)
        .await?;

    let workspace_path = worktree_result.worktree_path;

    // Keep target_branch aligned with what git service resolved
    if task.target_branch != worktree_result.target_branch {
        let tid = task_id.to_string();
        let new_branch = worktree_result.target_branch.clone();
        let _ = state
            .db
            .call(move |conn| {
                task_service::update_task(
                    conn,
                    &tid,
                    &UpdateTaskInput {
                        target_branch: Some(new_branch),
                        ..Default::default()
                    },
                )
            })
            .await;
        task.target_branch = worktree_result.target_branch.clone();
        state
            .sse_emitter
            .emit_log(
                task_id,
                "info",
                &format!("Target branch resolved: {}", task.target_branch),
                None,
            )
            .await;
    }

    if worktree_result.reused {
        state
            .sse_emitter
            .emit_log(
                task_id,
                "info",
                &format!("Reusing existing worktree at: {}", workspace_path.display()),
                None,
            )
            .await;
    } else {
        state
            .sse_emitter
            .emit_log(
                task_id,
                "info",
                &format!("New worktree created at: {}", workspace_path.display()),
                None,
            )
            .await;
    }

    if worktree_result.is_empty_repo {
        state
            .sse_emitter
            .emit_log(
                task_id,
                "info",
                "Repository is empty (no commits) - agent will create initial project structure",
                None,
            )
            .await;
    }

    // Capture the current HEAD as base_commit for scoped diffs.
    // This ensures get_changes only shows files changed by THIS task,
    // not changes from previous tasks in the same worktree.
    if let Ok(r) = crate::services::git_service::GitService::exec_git(
        &["rev-parse", "HEAD"],
        &workspace_path,
        None,
    )
    .await
    {
        if r.exit_code == 0 {
            let base_commit = r.stdout.trim().to_string();
            if !base_commit.is_empty() {
                let tid = task_id.to_string();
                let commit = base_commit.clone();
                let _ = state
                    .db
                    .call(move |conn| {
                        task_service::update_task(
                            conn,
                            &tid,
                            &UpdateTaskInput {
                                base_commit: Some(Some(commit)),
                                ..Default::default()
                            },
                        )
                    })
                    .await;
                state
                    .sse_emitter
                    .emit_log(
                        task_id,
                        "info",
                        &format!(
                            "Base commit captured for diff: {}",
                            &base_commit[..8.min(base_commit.len())]
                        ),
                        None,
                    )
                    .await;
            }
        }
    }

    // Build repository context string
    let repo_context = {
        let mut ctx = String::new();
        ctx.push_str(&format!("## Repository: {}\n", repo.name));

        if let Ok(stack_str) = serde_json::to_string_pretty(&repo.detected_stack) {
            if stack_str != "{}" {
                ctx.push_str(&format!(
                    "\n### Detected Stack\n```json\n{}\n```\n",
                    stack_str
                ));
            }
        }

        if !repo.conventions.is_empty() {
            ctx.push_str(&format!("\n### Conventions\n{}\n", repo.conventions));
        }

        if !repo.learned_patterns.is_empty() {
            ctx.push_str("\n### Learned Patterns\n");
            for pattern in &repo.learned_patterns {
                ctx.push_str(&format!("- {}\n", pattern.pattern));
            }
        }

        if ctx.is_empty() {
            None
        } else {
            Some(ctx)
        }
    };

    Ok((workspace_path, repo_context))
}

async fn load_chrome_mcp_agent_config(
    state: &AppState,
    agent_type: &AgentType,
) -> Result<(bool, Option<String>), AppError> {
    if agent_type != &AgentType::ClaudeCode {
        return Ok((false, None));
    }

    let (enabled, url) = state
        .db
        .call(|conn| settings_service::get_chrome_mcp_config(conn))
        .await?;

    let url = url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Ok((enabled, url))
}

fn add_chrome_mcp_env(
    env: &mut std::collections::HashMap<String, String>,
    chrome_mcp_url: Option<&str>,
) {
    if let Some(url) = chrome_mcp_url {
        env.insert("CHROME_MCP_URL".to_string(), url.to_string());
        env.insert("MCP_CHROME_URL".to_string(), url.to_string());
    }
}

/// Builds CLIRunnerOptions from a task and starts the agent.
///
/// This is shared between `/start` and `/execute` endpoints. It:
/// 1. Sets up an isolated git worktree for the task
/// 2. Builds repository context for the prompt
/// 3. Builds the CLI runner options
/// 4. Calls agent_service.start_agent()
pub async fn start_agent_for_task(
    state: &AppState,
    task_id: &str,
    task: &crate::models::task::Task,
) -> Result<(), AppError> {
    let mut task = task.clone();
    let (cwd, repo_context) = resolve_workspace(state, task_id, &mut task).await?;

    // Parse agent type (default to claude-code)
    let agent_type = task
        .agent_type
        .as_deref()
        .and_then(|s| s.parse::<AgentType>().ok())
        .unwrap_or(AgentType::ClaudeCode);

    let (chrome_mcp_enabled, chrome_mcp_url) =
        load_chrome_mcp_agent_config(state, &agent_type).await?;
    let mut env = std::collections::HashMap::new();
    add_chrome_mcp_env(&mut env, chrome_mcp_url.as_deref());

    let cwd_str = cwd.to_str().unwrap_or("");
    let spec = task.user_input.as_deref().unwrap_or(&task.description);

    let mut prompt = cli_prompts::build_task_prompt(
        &task.title,
        spec,
        &task.context_files,
        repo_context.as_deref(),
        task.agent_type.as_deref(),
        Some(cwd_str),
    );
    if chrome_mcp_enabled {
        prompt = cli_prompts::append_chrome_mcp_instructions(prompt);
    }

    // Dispatch based on agent type: API-based vs CLI
    let runner_options = if agent_type.is_api_based() {
        // Fetch API key from secrets
        let api_key = state
            .db
            .call(|conn| {
                crate::services::secrets_service::get_secret(conn, "ai_api_key", Some("minimax"))
            })
            .await?
            .ok_or_else(|| {
                AppError::Validation(
                    "MiniMax API key not configured. Go to Settings → Connections to add it."
                        .into(),
                )
            })?;

        RunnerOptions::API(APIRunnerOptions {
            task_id: task_id.to_string(),
            agent_type,
            prompt,
            model: task.agent_model.clone(),
            cwd,
            api_key,
        })
    } else {
        RunnerOptions::CLI(CLIRunnerOptions {
            task_id: task_id.to_string(),
            agent_type,
            prompt,
            model: task.agent_model.clone(),
            cwd,
            env,
            plan_only: false,
            resume_session_id: None,
            chrome_mcp_enabled,
        })
    };

    state
        .agent_service
        .start_agent(task_id, runner_options)
        .await?;

    Ok(())
}

/// Resumes an agent for a task with user feedback as additional context.
///
/// Similar to `start_agent_for_task` but uses `build_resume_prompt` instead
/// of `build_task_prompt`, incorporating the user's feedback message.
pub async fn resume_agent_for_task(
    state: &AppState,
    task_id: &str,
    task: &crate::models::task::Task,
    feedback: &str,
) -> Result<(), AppError> {
    let mut task = task.clone();
    let (cwd, repo_context) = resolve_workspace(state, task_id, &mut task).await?;

    let agent_type = task
        .agent_type
        .as_deref()
        .and_then(|s| s.parse::<AgentType>().ok())
        .unwrap_or(AgentType::ClaudeCode);

    let (chrome_mcp_enabled, chrome_mcp_url) =
        load_chrome_mcp_agent_config(state, &agent_type).await?;
    let mut env = std::collections::HashMap::new();
    add_chrome_mcp_env(&mut env, chrome_mcp_url.as_deref());

    let cwd_str = cwd.to_str().unwrap_or("");
    let spec = task.user_input.as_deref().unwrap_or(&task.description);

    let mut prompt = cli_prompts::build_resume_prompt(
        &task.title,
        spec,
        feedback,
        repo_context.as_deref(),
        task.agent_type.as_deref(),
        Some(cwd_str),
    );
    if chrome_mcp_enabled {
        prompt = cli_prompts::append_chrome_mcp_instructions(prompt);
    }

    let runner_options = if agent_type.is_api_based() {
        let api_key = state
            .db
            .call(|conn| {
                crate::services::secrets_service::get_secret(conn, "ai_api_key", Some("minimax"))
            })
            .await?
            .ok_or_else(|| AppError::Validation("MiniMax API key not configured.".into()))?;

        RunnerOptions::API(APIRunnerOptions {
            task_id: task_id.to_string(),
            agent_type,
            prompt,
            model: task.agent_model.clone(),
            cwd,
            api_key,
        })
    } else {
        RunnerOptions::CLI(CLIRunnerOptions {
            task_id: task_id.to_string(),
            agent_type,
            prompt,
            model: task.agent_model.clone(),
            cwd,
            env,
            plan_only: false,
            resume_session_id: None,
            chrome_mcp_enabled,
        })
    };

    state
        .agent_service
        .start_agent(task_id, runner_options)
        .await?;

    Ok(())
}

// ============================================================================
// Deprecated endpoints (spec workflow removed)
// ============================================================================

/// Returns 410 Gone for all deprecated spec-related endpoints.
async fn deprecated_gone() -> impl IntoResponse {
    (
        StatusCode::GONE,
        Json(json!({
            "error": "Gone",
            "message": "This endpoint has been removed. Use the two-agent workflow instead."
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qa_command_allows_playwright_defaults() {
        let command = validate_qa_command(Some("npx playwright test --trace on --reporter=line"))
            .expect("default Playwright command should be allowed");

        assert_eq!(command, "npx playwright test --trace on --reporter=line");
    }

    #[test]
    fn qa_command_blocks_shell_control_characters() {
        let error = validate_qa_command(Some("npx playwright test; rm -rf .")).unwrap_err();

        assert!(error.to_string().contains("shell control"));
    }
}
