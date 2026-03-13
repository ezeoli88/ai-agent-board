//! Spec management routes.
//!
//! Implements CRUD plus lifecycle endpoints for the Spec Agent feature:
//! refine, feedback, cancel, approve, and SSE log streaming.

use std::convert::Infallible;
use std::pin::Pin;
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
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::agent::cli_prompts;
use crate::agent::types::AgentType;
use crate::agent::{APIRunnerOptions, CLIRunnerOptions};
use crate::error::AppError;
use crate::models::spec::{CreateSpecInput, SpecStatus, UpdateSpecInput};
use crate::services::agent_service::RunnerOptions;
use crate::services::{repo_service, spec_service, task_service};
use crate::utils::{SSEEvent, SSEEventType};
use crate::AppState;

// ============================================================================
// Query / body types
// ============================================================================

/// Query parameters for `GET /api/specs`.
#[derive(Debug, Deserialize)]
struct ListSpecsQuery {
    #[serde(default)]
    repository_id: Option<String>,
}

/// Body for `POST /:id/feedback`.
#[derive(Debug, Deserialize)]
struct SpecFeedbackBody {
    #[serde(default)]
    message: Option<String>,
}

// ============================================================================
// Router
// ============================================================================

/// Builds the specs sub-router.
///
/// All paths are relative; the caller nests this under `/api/specs`.
pub fn router() -> Router<AppState> {
    Router::new()
        // CRUD
        .route("/", post(create_spec))
        .route("/", get(list_specs))
        .route("/{id}", get(get_spec))
        .route("/{id}", patch(update_spec))
        .route("/{id}", delete(delete_spec))
        // Lifecycle
        .route("/{id}/refine", post(refine_spec))
        .route("/{id}/feedback", post(send_feedback))
        .route("/{id}/cancel", post(cancel_spec))
        .route("/{id}/approve", post(approve_spec))
        .route("/{id}/logs", get(spec_logs_stream))
}

// ============================================================================
// CRUD handlers
// ============================================================================

/// POST /api/specs - Create a new spec.
async fn create_spec(
    State(state): State<AppState>,
    Json(input): Json<CreateSpecInput>,
) -> Result<impl IntoResponse, AppError> {
    info!(
        repository_id = %input.repository_id,
        "POST /specs"
    );

    let spec = state
        .db
        .call(move |conn| spec_service::create_spec(conn, &input))
        .await?;

    state
        .data_emitter
        .emit_change("spec", "created", Some(&spec.id));

    Ok((StatusCode::CREATED, Json(spec)))
}

/// GET /api/specs - List all specs, with optional filter.
async fn list_specs(
    State(state): State<AppState>,
    Query(query): Query<ListSpecsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let repository_id = query.repository_id.clone();

    let specs = state
        .db
        .call(move |conn| {
            spec_service::get_all_specs(conn, repository_id.as_deref())
        })
        .await?;

    Ok(Json(specs))
}

/// GET /api/specs/:id - Get a single spec by ID.
async fn get_spec(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let spec = state
        .db
        .call(move |conn| spec_service::get_spec_by_id(conn, &id))
        .await?;

    Ok(Json(spec))
}

/// PATCH /api/specs/:id - Update spec fields.
async fn update_spec(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateSpecInput>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "PATCH /specs/:id");

    let spec_id = id.clone();
    let spec = state
        .db
        .call(move |conn| spec_service::update_spec(conn, &spec_id, &input))
        .await?;

    state
        .data_emitter
        .emit_change("spec", "updated", Some(&id));

    Ok(Json(spec))
}

/// DELETE /api/specs/:id - Delete a spec.
///
/// Also cancels any running agent.
async fn delete_spec(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "DELETE /specs/:id");

    // Cancel running agent if any
    if state.agent_service.is_running(&id).await {
        info!(id = %id, "Cancelling running agent before deleting spec");
        let _ = state.agent_service.cancel_agent(&id).await;
    }

    let spec_id = id.clone();
    state
        .db
        .call(move |conn| spec_service::delete_spec(conn, &spec_id))
        .await?;

    // Clean up SSE history
    state.sse_emitter.clear_history(&id).await;
    state.sse_emitter.close_task(&id).await;

    state
        .data_emitter
        .emit_change("spec", "deleted", Some(&id));

    Ok(StatusCode::NO_CONTENT.into_response())
}

// ============================================================================
// Lifecycle handlers
// ============================================================================

/// POST /api/specs/:id/refine - Start or continue refining a spec.
///
/// Validates status (draft or refining), sets status to refining,
/// resolves workspace, builds prompt, and starts agent.
async fn refine_spec(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /specs/:id/refine");

    let spec_id = id.clone();
    let spec = state
        .db
        .call(move |conn| spec_service::get_spec_by_id(conn, &spec_id))
        .await?;

    // Validate status
    if spec.status != SpecStatus::Draft && spec.status != SpecStatus::Refining {
        return Err(AppError::Validation(format!(
            "Cannot refine spec with status: {}. Valid statuses: draft, refining",
            spec.status
        )));
    }

    // Update status to refining
    let spec_id = id.clone();
    state
        .db
        .call(move |conn| {
            spec_service::update_spec(
                conn,
                &spec_id,
                &UpdateSpecInput {
                    status: Some(SpecStatus::Refining),
                    ..Default::default()
                },
            )
        })
        .await?;

    state.sse_emitter.emit_status(&id, "refining").await;

    // Resolve workspace: get repo, find local path, build repo context
    let repo_id = spec.repository_id.clone();
    let repo = state
        .db
        .call(move |conn| repo_service::get_repository_by_id(conn, &repo_id))
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Repository not found: {}",
                spec.repository_id
            ))
        })?;

    let workspace_path = repo_service::get_repo_local_path(&repo).ok_or_else(|| {
        AppError::Validation(format!(
            "Repository path not found for repo: {}",
            repo.url
        ))
    })?;

    // Build repository context string
    let repo_context = build_repo_context(&repo);

    let cwd_str = workspace_path
        .to_str()
        .unwrap_or("")
        .to_string();

    // Build the prompt
    let prompt = if let Some(ref current_spec) = spec.draft_spec {
        // Has a previous draft — refine it
        cli_prompts::build_spec_refine_prompt(
            &spec.user_input,
            current_spec,
            repo_context.as_deref(),
            Some(&cwd_str),
        )
    } else {
        // First run — generate from scratch
        cli_prompts::build_spec_prompt(
            &spec.user_input,
            repo_context.as_deref(),
            Some(&cwd_str),
        )
    };

    // Parse agent type
    let agent_type = spec
        .agent_type
        .as_deref()
        .and_then(|s| s.parse::<AgentType>().ok())
        .unwrap_or(AgentType::ClaudeCode);

    // Build runner options
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
                    "MiniMax API key not configured. Go to Settings > Connections to add it."
                        .into(),
                )
            })?;

        RunnerOptions::API(APIRunnerOptions {
            task_id: id.clone(),
            agent_type,
            prompt,
            model: spec.agent_model.clone(),
            cwd: workspace_path.clone(),
            api_key,
        })
    } else {
        RunnerOptions::CLI(CLIRunnerOptions {
            task_id: id.clone(),
            agent_type,
            prompt,
            model: spec.agent_model.clone(),
            cwd: workspace_path.clone(),
            env: std::collections::HashMap::new(),
            plan_only: true, // Spec agent is read-only
        })
    };

    // Start agent
    state
        .agent_service
        .start_agent(&id, runner_options)
        .await?;

    // Spawn background task to handle agent completion
    let state_clone = state.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        // Wait for the agent to finish by polling is_running
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if !state_clone.agent_service.is_running(&id_clone).await {
                break;
            }
        }

        // Agent finished — extract spec from accumulated output
        // The agent logs contain the output. Get them and look for SPEC_START/SPEC_END
        let logs = state_clone.agent_service.get_logs(&id_clone).await;
        let accumulated_output: String = logs
            .iter()
            .filter(|l| l.level == "assistant" || l.level == "info")
            .map(|l| l.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let draft = extract_spec_from_output(&accumulated_output)
            .or_else(|| {
                // Fallback: use full output if delimiters not found
                if !accumulated_output.trim().is_empty() {
                    Some(accumulated_output.trim().to_string())
                } else {
                    None
                }
            });

        if let Some(ref spec_text) = draft {
            // Save draft_spec
            let spec_id = id_clone.clone();
            let spec_text_clone = spec_text.clone();
            let _ = state_clone
                .db
                .call(move |conn| {
                    spec_service::update_spec(
                        conn,
                        &spec_id,
                        &UpdateSpecInput {
                            draft_spec: Some(Some(spec_text_clone)),
                            ..Default::default()
                        },
                    )
                })
                .await;

            // Emit spec_ready event via SSE
            state_clone
                .sse_emitter
                .emit(
                    &id_clone,
                    SSEEvent {
                        event_type: SSEEventType::ChatMessage,
                        data: json!({
                            "id": uuid::Uuid::new_v4().to_string(),
                            "role": "system",
                            "content": "Spec draft is ready. Review it in the Spec tab.",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }),
                    },
                )
                .await;

            info!(spec_id = %id_clone, "Spec draft extracted and saved");
        } else {
            // No spec found in output — update status to failed
            let spec_id = id_clone.clone();
            let _ = state_clone
                .db
                .call(move |conn| {
                    spec_service::update_spec(
                        conn,
                        &spec_id,
                        &UpdateSpecInput {
                            status: Some(SpecStatus::Failed),
                            ..Default::default()
                        },
                    )
                })
                .await;

            state_clone
                .sse_emitter
                .emit_error(&id_clone, "Agent completed but no spec was produced")
                .await;
            state_clone
                .sse_emitter
                .emit_status(&id_clone, "failed")
                .await;

            warn!(spec_id = %id_clone, "No spec extracted from agent output");
        }

        state_clone
            .data_emitter
            .emit_change("spec", "updated", Some(&id_clone));
    });

    state
        .data_emitter
        .emit_change("spec", "updated", Some(&id));

    Ok(Json(json!({
        "status": "refining",
        "message": "Spec agent started"
    })))
}

/// POST /api/specs/:id/feedback - Send feedback to the spec agent.
///
/// If the agent is running, sends feedback directly. If not, relaunches
/// the agent with a resume prompt incorporating the feedback.
async fn send_feedback(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SpecFeedbackBody>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /specs/:id/feedback");

    let message = body
        .message
        .filter(|m| !m.is_empty())
        .ok_or_else(|| AppError::Validation("Message is required".to_string()))?;

    // Store user chat message for history (don't broadcast — frontend shows optimistically)
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
        state.sse_emitter.store_event(&id, user_chat_event).await;

        state
            .data_emitter
            .emit_change("spec", "updated", Some(&id));

        return Ok(Json(json!({ "status": "feedback_sent" })));
    }

    // Agent is NOT running — relaunch with resume prompt
    let spec_id = id.clone();
    let spec = state
        .db
        .call(move |conn| spec_service::get_spec_by_id(conn, &spec_id))
        .await?;

    // Don't allow resume for terminal statuses
    let terminal = [SpecStatus::Approved, SpecStatus::Failed, SpecStatus::Canceled];
    if terminal.contains(&spec.status) {
        return Err(AppError::Validation(format!(
            "Spec is in {} status. Cannot resume the agent.",
            spec.status
        )));
    }

    // Store user message
    state.sse_emitter.store_event(&id, user_chat_event).await;

    // Update status to refining
    let spec_id = id.clone();
    state
        .db
        .call(move |conn| {
            spec_service::update_spec(
                conn,
                &spec_id,
                &UpdateSpecInput {
                    status: Some(SpecStatus::Refining),
                    ..Default::default()
                },
            )
        })
        .await?;

    state.sse_emitter.emit_status(&id, "refining").await;

    // Resolve workspace
    let repo_id = spec.repository_id.clone();
    let repo = state
        .db
        .call(move |conn| repo_service::get_repository_by_id(conn, &repo_id))
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Repository not found: {}",
                spec.repository_id
            ))
        })?;

    let workspace_path = repo_service::get_repo_local_path(&repo).ok_or_else(|| {
        AppError::Validation(format!(
            "Repository path not found for repo: {}",
            repo.url
        ))
    })?;

    let repo_context = build_repo_context(&repo);
    let cwd_str = workspace_path.to_str().unwrap_or("").to_string();

    let current_spec = spec.draft_spec.as_deref().unwrap_or("");

    let prompt = cli_prompts::build_spec_resume_prompt(
        &spec.user_input,
        current_spec,
        &message,
        repo_context.as_deref(),
        Some(&cwd_str),
    );

    // Parse agent type
    let agent_type = spec
        .agent_type
        .as_deref()
        .and_then(|s| s.parse::<AgentType>().ok())
        .unwrap_or(AgentType::ClaudeCode);

    let runner_options = if agent_type.is_api_based() {
        let api_key = state
            .db
            .call(|conn| {
                crate::services::secrets_service::get_secret(conn, "ai_api_key", Some("minimax"))
            })
            .await?
            .ok_or_else(|| {
                AppError::Validation("MiniMax API key not configured.".into())
            })?;

        RunnerOptions::API(APIRunnerOptions {
            task_id: id.clone(),
            agent_type,
            prompt,
            model: spec.agent_model.clone(),
            cwd: workspace_path.clone(),
            api_key,
        })
    } else {
        RunnerOptions::CLI(CLIRunnerOptions {
            task_id: id.clone(),
            agent_type,
            prompt,
            model: spec.agent_model.clone(),
            cwd: workspace_path.clone(),
            env: std::collections::HashMap::new(),
            plan_only: true,
        })
    };

    state
        .agent_service
        .start_agent(&id, runner_options)
        .await?;

    // Spawn background task to handle agent completion (same pattern as refine)
    let state_clone = state.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if !state_clone.agent_service.is_running(&id_clone).await {
                break;
            }
        }

        let logs = state_clone.agent_service.get_logs(&id_clone).await;
        let accumulated_output: String = logs
            .iter()
            .filter(|l| l.level == "assistant" || l.level == "info")
            .map(|l| l.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let draft = extract_spec_from_output(&accumulated_output).or_else(|| {
            if !accumulated_output.trim().is_empty() {
                Some(accumulated_output.trim().to_string())
            } else {
                None
            }
        });

        if let Some(ref spec_text) = draft {
            let spec_id = id_clone.clone();
            let spec_text_clone = spec_text.clone();
            let _ = state_clone
                .db
                .call(move |conn| {
                    spec_service::update_spec(
                        conn,
                        &spec_id,
                        &UpdateSpecInput {
                            draft_spec: Some(Some(spec_text_clone)),
                            ..Default::default()
                        },
                    )
                })
                .await;

            state_clone
                .sse_emitter
                .emit(
                    &id_clone,
                    SSEEvent {
                        event_type: SSEEventType::ChatMessage,
                        data: json!({
                            "id": uuid::Uuid::new_v4().to_string(),
                            "role": "system",
                            "content": "Updated spec draft is ready. Review it in the Spec tab.",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }),
                    },
                )
                .await;
        }

        state_clone
            .data_emitter
            .emit_change("spec", "updated", Some(&id_clone));
    });

    state
        .data_emitter
        .emit_change("spec", "updated", Some(&id));

    Ok(Json(json!({
        "status": "agent_resumed",
        "message": "Agent resumed with your feedback"
    })))
}

/// POST /api/specs/:id/cancel - Cancel spec refinement.
///
/// Cancels the running agent and sets status to canceled.
async fn cancel_spec(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /specs/:id/cancel");

    let spec_id = id.clone();
    let update = UpdateSpecInput {
        status: Some(SpecStatus::Canceled),
        ..Default::default()
    };

    state
        .db
        .call(move |conn| spec_service::update_spec(conn, &spec_id, &update))
        .await?;

    // Cancel the running agent if any
    let _ = state.agent_service.cancel_agent(&id).await;

    // Emit SSE events
    state
        .sse_emitter
        .emit_error(&id, "Spec refinement canceled by user")
        .await;
    state.sse_emitter.emit_status(&id, "canceled").await;

    state
        .data_emitter
        .emit_change("spec", "updated", Some(&id));

    Ok(Json(json!({ "status": "canceled" })))
}

/// POST /api/specs/:id/approve - Approve the spec and create a Task.
///
/// Takes final_spec or draft_spec, creates a Task via task_service,
/// and updates the spec with the task_id and approved status.
async fn approve_spec(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /specs/:id/approve");

    let spec_id = id.clone();
    let spec = state
        .db
        .call(move |conn| spec_service::get_spec_by_id(conn, &spec_id))
        .await?;

    // Get the spec content (prefer final_spec, fallback to draft_spec)
    let spec_content = spec
        .final_spec
        .as_ref()
        .or(spec.draft_spec.as_ref())
        .ok_or_else(|| {
            AppError::Validation("No spec content to approve. Generate a spec first.".to_string())
        })?
        .clone();

    // Create a task from the spec
    let task_input = task_service::CreateTaskServiceInput {
        repository_id: Some(spec.repository_id.clone()),
        user_input: Some(spec_content.clone()),
        title: Some(spec.title.clone()),
        description: Some(spec_content),
        agent_type: spec.agent_type.clone(),
        agent_model: spec.agent_model.clone(),
        ..Default::default()
    };

    let task = state
        .db
        .call(move |conn| task_service::create_task(conn, &task_input))
        .await?;

    // Approve the task immediately (skip PM agent phase)
    let task_id = task.id.clone();
    state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &task_id,
                &crate::models::task::UpdateTaskInput {
                    status: Some(crate::models::task::TaskStatus::Approved),
                    ..Default::default()
                },
            )
        })
        .await?;

    // Update spec with task_id and approved status
    let now = chrono::Utc::now().to_rfc3339();
    let spec_id = id.clone();
    let task_id_for_spec = task.id.clone();
    state
        .db
        .call(move |conn| {
            spec_service::update_spec(
                conn,
                &spec_id,
                &UpdateSpecInput {
                    status: Some(SpecStatus::Approved),
                    task_id: Some(Some(task_id_for_spec)),
                    approved_at: Some(Some(now)),
                    ..Default::default()
                },
            )
        })
        .await?;

    state
        .data_emitter
        .emit_change("spec", "updated", Some(&id));
    state
        .data_emitter
        .emit_change("task", "created", Some(&task.id));

    Ok(Json(json!({
        "status": "approved",
        "task": {
            "id": task.id,
            "title": task.title,
            "status": "approved",
        }
    })))
}

/// GET /api/specs/:id/logs - SSE stream of spec log events.
///
/// Follows the same pattern as task_logs_stream in tasks.rs:
/// 1. Replays historical events (logs, chat messages, tool activity)
/// 2. Sends current spec status
/// 3. If terminal, sends terminal event; otherwise streams live events
async fn spec_logs_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!(id = %id, "GET /specs/:id/logs (SSE)");

    // 1. Fetch historical events for replay
    let history = state.sse_emitter.get_history(&id).await;
    let history = if history.is_empty() {
        // Try loading from DB (server may have restarted)
        let spec_id_for_db = id.clone();
        let db_events = state
            .db
            .call(move |conn| {
                crate::services::task_event_service::get_events_for_task(conn, &spec_id_for_db)
            })
            .await
            .unwrap_or_default();

        if !db_events.is_empty() {
            info!(id = %id, count = db_events.len(), "Warming SSE cache from DB for spec");
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

    // 2. Get current spec status from DB
    let spec_id_for_status = id.clone();
    let spec_opt = state
        .db
        .call(move |conn| spec_service::get_spec_by_id(conn, &spec_id_for_status))
        .await
        .ok();

    let current_status = spec_opt.as_ref().map(|s| s.status.as_str().to_string());

    // 3. Subscribe to new events
    let rx = state.sse_emitter.subscribe(&id).await;

    // Build replay events: filter OUT terminal events
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

    // Append current status
    if let Some(ref status) = current_status {
        let data = serde_json::to_string(&json!({ "status": status })).unwrap_or_default();
        replay_events.push(Ok(Event::default().event("status").data(data)));
    }

    // Determine if spec is in a terminal state
    let is_terminal = matches!(
        current_status.as_deref(),
        Some("approved") | Some("failed") | Some("canceled")
    );

    if is_terminal {
        if let Some(ref spec) = spec_opt {
            match current_status.as_deref() {
                Some("approved") => {
                    let data = serde_json::to_string(&json!({
                        "message": "Spec approved and task created"
                    }))
                    .unwrap_or_default();
                    replay_events.push(Ok(Event::default().event("complete").data(data)));
                }
                Some("failed") | Some("canceled") => {
                    let msg = if spec.status == SpecStatus::Canceled {
                        "Spec refinement was canceled"
                    } else {
                        "Spec refinement failed"
                    };
                    let data =
                        serde_json::to_string(&json!({ "message": msg })).unwrap_or_default();
                    replay_events.push(Ok(Event::default().event("error").data(data)));
                }
                _ => {}
            }
        }
    }

    let replay_stream = tokio_stream::iter(replay_events);

    let stream: Pin<Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>> =
        if is_terminal {
            Box::pin(replay_stream)
        } else {
            let id_for_filter = id.clone();
            let id_for_map = id.clone();
            let live_stream = BroadcastStream::new(rx)
                .filter_map(move |result| match result {
                    Ok(event) => Some(event),
                    Err(e) => {
                        warn!(spec_id = %id_for_filter, error = %e, "BroadcastStream recv error");
                        None
                    }
                })
                .map(move |event| {
                    info!(
                        spec_id = %id_for_map,
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

// ============================================================================
// Helpers
// ============================================================================

/// Extracts a spec from the agent output using SPEC_START/SPEC_END delimiters.
fn extract_spec_from_output(output: &str) -> Option<String> {
    let start_marker = "---SPEC_START---";
    let end_marker = "---SPEC_END---";
    if let Some(start_idx) = output.find(start_marker) {
        let content_start = start_idx + start_marker.len();
        if let Some(end_idx) = output[content_start..].find(end_marker) {
            let spec = output[content_start..content_start + end_idx].trim();
            if !spec.is_empty() {
                return Some(spec.to_string());
            }
        }
    }
    None
}

/// Builds a repository context string from a Repository model.
///
/// This mirrors the `build_repo_context` section in tasks.rs `resolve_workspace`.
fn build_repo_context(repo: &crate::models::repository::Repository) -> Option<String> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_spec_from_output_with_delimiters() {
        let output = "Some preamble text\n---SPEC_START---\n# My Spec\n\nDetails here\n---SPEC_END---\nSome trailing text";
        let result = extract_spec_from_output(output);
        assert_eq!(result, Some("# My Spec\n\nDetails here".to_string()));
    }

    #[test]
    fn test_extract_spec_from_output_no_delimiters() {
        let output = "Just some regular output without markers";
        let result = extract_spec_from_output(output);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_spec_from_output_empty_between_markers() {
        let output = "---SPEC_START---\n\n---SPEC_END---";
        let result = extract_spec_from_output(output);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_spec_from_output_only_start_marker() {
        let output = "---SPEC_START---\nContent but no end marker";
        let result = extract_spec_from_output(output);
        assert_eq!(result, None);
    }
}
