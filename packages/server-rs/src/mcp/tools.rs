//! MCP tool definitions and handlers.
//!
//! Each tool has a name, description, JSON Schema input definition, and an async
//! handler that operates on [`AppState`]. The tools mirror the TypeScript MCP
//! implementation:
//!
//! - Repository: `add_repository`, `list_repositories`
//! - Tasks: `create_task`, `list_tasks`, `get_task`, `update_task`, `delete_task`
//! - Workflow: `start_task`, `execute_task`, `approve_spec`, `send_feedback`,
//!   `extend_task_timeout`, `cancel_task`
//! - Review: `get_changes`, `approve_changes`, `request_changes`
//! - PR lifecycle: `mark_pr_merged`, `mark_pr_closed`
//! - Status: `get_setup_status`

use serde_json::{json, Value};
use tracing::{info, warn};

use crate::error::AppError;
use crate::models::task::TaskStatus;
use crate::services::task_service::{self, CreateTaskServiceInput};
use crate::AppState;

// ============================================================================
// Tool definition (returned by tools/list)
// ============================================================================

/// Returns the list of all tool definitions for `tools/list`.
///
/// Each tool has `name`, `description`, and `inputSchema` (JSON Schema).
pub fn tool_definitions() -> Value {
    json!({
        "tools": [
            {
                "name": "add_repository",
                "description": "Add a local repository to Agent Board. Detects the project stack automatically from the filesystem. Returns the created repository object.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Repository display name"
                        },
                        "path": {
                            "type": "string",
                            "description": "Absolute filesystem path to the local git repository"
                        },
                        "default_branch": {
                            "type": "string",
                            "description": "Default branch name (defaults to \"main\")"
                        },
                        "remote_url": {
                            "type": "string",
                            "description": "Optional remote URL override"
                        }
                    },
                    "required": ["name", "path"]
                }
            },
            {
                "name": "list_repositories",
                "description": "List all repositories registered in Agent Board. Returns an array of repository objects with id, name, url, default_branch, detected_stack, and active_tasks_count.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "create_task",
                "description": "Create a new task in Agent Board. Requires a repository_id and user_input describing what to build. Returns the created task object in 'draft' status.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repository_id": {
                            "type": "string",
                            "description": "Repository UUID to associate the task with"
                        },
                        "user_input": {
                            "type": "string",
                            "description": "Natural language description of what to build or fix"
                        },
                        "title": {
                            "type": "string",
                            "description": "Short task title (auto-generated from user_input if omitted)"
                        },
                        "description": {
                            "type": "string",
                            "description": "Detailed description (defaults to user_input)"
                        },
                        "target_branch": {
                            "type": "string",
                            "description": "Base branch for the task (defaults to \"main\")"
                        },
                        "context_files": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "File paths the agent should review first"
                        },
                        "build_command": {
                            "type": "string",
                            "description": "Build command to verify changes"
                        },
                        "agent_type": {
                            "type": "string",
                            "description": "CLI agent type override (e.g. claude-code, codex, gemini)"
                        },
                        "agent_model": {
                            "type": "string",
                            "description": "CLI agent model override"
                        }
                    },
                    "required": ["repository_id", "user_input"]
                }
            },
            {
                "name": "list_tasks",
                "description": "List tasks, optionally filtered by repository_id and/or status. Returns an array of task objects ordered by creation date (newest first).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repository_id": {
                            "type": "string",
                            "description": "Filter by repository UUID"
                        },
                        "status": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Filter by one or more task statuses (e.g. ['draft', 'planning'])"
                        }
                    }
                }
            },
            {
                "name": "get_task",
                "description": "Get the full details of a specific task by its ID, including status, branch_name, pr_url, error, and all metadata.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "update_task",
                "description": "Update editable fields of an existing task. Only fields provided will be changed. Returns the updated task object.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        },
                        "title": {
                            "type": "string",
                            "description": "New task title"
                        },
                        "description": {
                            "type": "string",
                            "description": "New description"
                        },
                        "target_branch": {
                            "type": "string",
                            "description": "New target branch"
                        },
                        "context_files": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Updated context files"
                        },
                        "build_command": {
                            "type": "string",
                            "description": "Updated build command"
                        },
                        "agent_type": {
                            "type": "string",
                            "description": "Agent type override"
                        },
                        "agent_model": {
                            "type": "string",
                            "description": "Agent model override"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "start_task",
                "description": "Start a task that is in 'draft' or 'failed' status. Creates a feature branch and launches the agent in planning mode. The agent runs asynchronously; poll the task status to track progress.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "execute_task",
                "description": "Execute a task that is in 'backlog', 'approved', 'failed', or 'changes_requested' status. For others, starts the planning agent. The agent runs asynchronously.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "approve_spec",
                "description": "Approve the generated spec for a task. Moves the task from 'pending_approval' to 'approved'. Optionally provide an edited spec.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        },
                        "final_spec": {
                            "type": "string",
                            "description": "Optionally provide an edited spec to approve instead of the generated one"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "send_feedback",
                "description": "Send feedback or a message to the agent working on a task. If the agent is running, the message is delivered directly. If the task is in 'plan_review', this approves the plan.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        },
                        "message": {
                            "type": "string",
                            "description": "Feedback message to send to the agent"
                        }
                    },
                    "required": ["task_id", "message"]
                }
            },
            {
                "name": "extend_task_timeout",
                "description": "Extend the running agent's timeout by 5 minutes. Only valid when an agent is actively running for the task. Returns the new timeout timestamp.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "cancel_task",
                "description": "Cancel the agent execution for a task. If the agent is running, kills the process. If the agent already exited but the task is stuck in an active status, resets to 'canceled'.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "get_changes",
                "description": "Get the files changed by the agent for a task. Returns a list of changed files with their diff content. Falls back to persisted changes data.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "approve_changes",
                "description": "Approve the agent's code changes and create a Pull Request. Only valid when the task is in 'awaiting_review' or 'review' status.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "request_changes",
                "description": "Request changes on a task. Sets the task to 'changes_requested' and stores the feedback for the agent.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        },
                        "feedback": {
                            "type": "string",
                            "description": "Detailed feedback describing what changes are needed"
                        }
                    },
                    "required": ["task_id", "feedback"]
                }
            },
            {
                "name": "delete_task",
                "description": "Delete a task by ID. Cancels any running agent and cleans up the worktree in the background. Returns confirmation.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "mark_pr_merged",
                "description": "Mark a task's PR as merged. Sets status to 'done' and cleans up the worktree.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "mark_pr_closed",
                "description": "Mark a task's PR as closed without merging. Sets status to 'canceled'.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Task UUID"
                        }
                    },
                    "required": ["task_id"]
                }
            },
            {
                "name": "get_setup_status",
                "description": "Get the current Agent Board setup status including AI provider configuration and secret status. Useful for checking if the board is ready to run tasks.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }
        ]
    })
}

// ============================================================================
// Tool dispatch
// ============================================================================

/// Dispatches a `tools/call` request to the appropriate tool handler.
///
/// Returns an MCP tool result with `content` array containing text items.
pub async fn call_tool(state: &AppState, name: &str, args: Value) -> Value {
    match name {
        "add_repository" => handle_add_repository(state, args).await,
        "list_repositories" => handle_list_repositories(state).await,
        "create_task" => handle_create_task(state, args).await,
        "list_tasks" => handle_list_tasks(state, args).await,
        "get_task" => handle_get_task(state, args).await,
        "update_task" => handle_update_task(state, args).await,
        "start_task" => handle_start_task(state, args).await,
        "execute_task" => handle_execute_task(state, args).await,
        "approve_spec" => handle_approve_spec(state, args).await,
        "send_feedback" => handle_send_feedback(state, args).await,
        "extend_task_timeout" => handle_extend_task_timeout(state, args).await,
        "cancel_task" => handle_cancel_task(state, args).await,
        "get_changes" => handle_get_changes(state, args).await,
        "approve_changes" => handle_approve_changes(state, args).await,
        "request_changes" => handle_request_changes(state, args).await,
        "delete_task" => handle_delete_task(state, args).await,
        "mark_pr_merged" => handle_pr_lifecycle(state, args, "pr-merged").await,
        "mark_pr_closed" => handle_pr_lifecycle(state, args, "pr-closed").await,
        "get_setup_status" => handle_get_setup_status(state).await,
        _ => tool_error(&format!("Unknown tool: {name}")),
    }
}

// ============================================================================
// Tool result helpers
// ============================================================================

/// Wraps a value as an MCP tool result with a single text content item.
fn tool_result(data: &Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string())
        }]
    })
}

/// Returns an MCP tool error result.
fn tool_error(message: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": message
        }],
        "isError": true
    })
}

/// Returns an error result from an `AppError`.
fn tool_app_error(context: &str, err: AppError) -> Value {
    warn!(%err, context = context, "MCP tool error");
    tool_error(&format!("{context}: {err}"))
}

// ============================================================================
// Repository tools
// ============================================================================

/// `add_repository` - Create a repository from a local filesystem path.
async fn handle_add_repository(state: &AppState, args: Value) -> Value {
    let name = args["name"].as_str().unwrap_or("").trim().to_string();
    let path = args["path"].as_str().unwrap_or("").trim().to_string();
    let default_branch = args["default_branch"]
        .as_str()
        .unwrap_or("main")
        .to_string();

    if name.is_empty() || path.is_empty() {
        return tool_error("Missing required parameters: name, path");
    }

    let url = format!("file://{path}");

    let result = state
        .db
        .call(move |conn| {
            // Check for duplicate
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM repositories WHERE url = ?1",
                    [&url],
                    |row| row.get(0),
                )
                .ok();

            if let Some(existing_id) = existing {
                return Ok(json!({
                    "error": "duplicate_repository",
                    "message": format!("Repository already exists at this path (id: {existing_id})"),
                    "hint": format!("Use list_repositories or get repository with id: {existing_id}")
                }));
            }

            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO repositories (id, name, url, default_branch, detected_stack, conventions, learned_patterns, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![id, name, url, default_branch, "{}", "", "[]", now, now],
            )
            .map_err(AppError::Database)?;

            info!(id = %id, name = %name, "Repository created via MCP");

            Ok(json!({
                "id": id,
                "name": name,
                "url": url,
                "default_branch": default_branch,
                "detected_stack": {},
                "conventions": "",
                "learned_patterns": [],
                "active_tasks_count": 0,
                "created_at": now,
                "updated_at": now
            }))
        })
        .await;

    match result {
        Ok(data) => {
            // Emit data change event if it was a successful creation (no error field)
            if data.get("error").is_none() {
                if let Some(id) = data["id"].as_str() {
                    state.data_emitter.emit_change("repo", "created", Some(id));
                }
            }
            tool_result(&data)
        }
        Err(e) => tool_app_error("Error adding repository", e),
    }
}

/// `list_repositories` - List all registered repositories.
async fn handle_list_repositories(state: &AppState) -> Value {
    let result = state
        .db
        .call(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT r.*, \
                       (SELECT COUNT(*) FROM tasks t \
                        WHERE t.repository_id = r.id \
                        AND t.status NOT IN ('done', 'archived')) as active_tasks_count \
                     FROM repositories r \
                     ORDER BY r.created_at DESC",
                )
                .map_err(AppError::Database)?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(json!({
                        "id": row.get::<_, String>("id").unwrap_or_default(),
                        "name": row.get::<_, String>("name").unwrap_or_default(),
                        "url": row.get::<_, String>("url").unwrap_or_default(),
                        "default_branch": row.get::<_, String>("default_branch").unwrap_or_else(|_| "main".to_string()),
                        "detected_stack": serde_json::from_str::<Value>(
                            &row.get::<_, String>("detected_stack").unwrap_or_else(|_| "{}".to_string())
                        ).unwrap_or(json!({})),
                        "conventions": row.get::<_, String>("conventions").unwrap_or_default(),
                        "learned_patterns": serde_json::from_str::<Value>(
                            &row.get::<_, String>("learned_patterns").unwrap_or_else(|_| "[]".to_string())
                        ).unwrap_or(json!([])),
                        "active_tasks_count": row.get::<_, i64>("active_tasks_count").unwrap_or(0),
                        "created_at": row.get::<_, String>("created_at").unwrap_or_default(),
                        "updated_at": row.get::<_, String>("updated_at").unwrap_or_default(),
                    }))
                })
                .map_err(AppError::Database)?;

            let mut repos = Vec::new();
            for row in rows {
                repos.push(row.map_err(AppError::Database)?);
            }

            info!(count = repos.len(), "list_repositories via MCP");
            Ok(Value::Array(repos))
        })
        .await;

    match result {
        Ok(data) => tool_result(&data),
        Err(e) => tool_app_error("Error listing repositories", e),
    }
}

// ============================================================================
// Task tools
// ============================================================================

/// `create_task` - Create a new task in draft status.
async fn handle_create_task(state: &AppState, args: Value) -> Value {
    let repository_id = args["repository_id"].as_str().map(String::from);
    let user_input = args["user_input"].as_str().map(String::from);

    if repository_id.is_none() || user_input.is_none() {
        return tool_error("Missing required parameters: repository_id, user_input");
    }

    let input = CreateTaskServiceInput {
        repository_id,
        user_input,
        title: args["title"].as_str().map(String::from),
        description: args["description"].as_str().map(String::from),
        repo_url: None, // Will be resolved from repository_id if needed
        target_branch: args["target_branch"].as_str().map(String::from),
        context_files: args["context_files"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        }),
        build_command: args["build_command"].as_str().map(String::from),
        agent_type: args["agent_type"].as_str().map(String::from),
        agent_model: args["agent_model"].as_str().map(String::from),
    };

    let result = state
        .db
        .call(move |conn| {
            let task = task_service::create_task(conn, &input)?;
            let task_json = serde_json::to_value(&task)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON error: {e}")))?;
            Ok(task_json)
        })
        .await;

    match result {
        Ok(data) => {
            if let Some(id) = data["id"].as_str() {
                state.data_emitter.emit_change("task", "created", Some(id));
            }
            tool_result(&data)
        }
        Err(e) => tool_app_error("Error creating task", e),
    }
}

/// `list_tasks` - List tasks with optional filters.
async fn handle_list_tasks(state: &AppState, args: Value) -> Value {
    let repository_id = args["repository_id"].as_str().map(String::from);
    let status_filter: Option<Vec<String>> = args["status"].as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    });

    let result = state
        .db
        .call(move |conn| {
            let tasks = task_service::get_all_tasks(conn, None, repository_id.as_deref())?;

            // Apply status filter if provided
            let filtered = if let Some(ref statuses) = status_filter {
                tasks
                    .into_iter()
                    .filter(|t| statuses.contains(&t.status.as_str().to_string()))
                    .collect::<Vec<_>>()
            } else {
                tasks
            };

            info!(count = filtered.len(), "list_tasks via MCP");

            let tasks_json = serde_json::to_value(&filtered)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON error: {e}")))?;
            Ok(tasks_json)
        })
        .await;

    match result {
        Ok(data) => tool_result(&data),
        Err(e) => tool_app_error("Error listing tasks", e),
    }
}

/// `get_task` - Get full task details by ID.
async fn handle_get_task(state: &AppState, args: Value) -> Value {
    let task_id = match args["task_id"].as_str() {
        Some(id) => id.to_string(),
        None => return tool_error("Missing required parameter: task_id"),
    };

    let result = state
        .db
        .call(move |conn| {
            let task = task_service::get_task_by_id(conn, &task_id)?;
            let task_json = serde_json::to_value(&task)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON error: {e}")))?;
            Ok(task_json)
        })
        .await;

    match result {
        Ok(data) => tool_result(&data),
        Err(e) => tool_app_error("Error getting task", e),
    }
}

/// `update_task` - Update editable fields of an existing task.
async fn handle_update_task(state: &AppState, args: Value) -> Value {
    let task_id = match args["task_id"].as_str() {
        Some(id) => id.to_string(),
        None => return tool_error("Missing required parameter: task_id"),
    };

    let mut update = crate::models::task::UpdateTaskInput::default();

    if let Some(title) = args["title"].as_str() {
        update.title = Some(title.to_string());
    }
    if let Some(desc) = args["description"].as_str() {
        update.description = Some(desc.to_string());
    }
    if let Some(branch) = args["target_branch"].as_str() {
        update.target_branch = Some(branch.to_string());
    }
    if let Some(cmd) = args["build_command"].as_str() {
        update.build_command = Some(Some(cmd.to_string()));
    }
    if let Some(agent) = args["agent_type"].as_str() {
        update.agent_type = Some(Some(agent.to_string()));
    }
    if let Some(model) = args["agent_model"].as_str() {
        update.agent_model = Some(Some(model.to_string()));
    }
    if let Some(files) = args["context_files"].as_array() {
        let files_vec: Vec<String> = files
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        update.context_files = Some(files_vec);
    }

    let tid = task_id.clone();
    let result = state
        .db
        .call(move |conn| {
            let task = task_service::update_task(conn, &tid, &update)?;
            serde_json::to_value(&task)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON error: {e}")))
        })
        .await;

    match result {
        Ok(data) => {
            state
                .data_emitter
                .emit_change("task", "updated", Some(&task_id));
            info!(id = %task_id, "update_task (MCP)");
            tool_result(&data)
        }
        Err(e) => tool_app_error("Error updating task", e),
    }
}

// ============================================================================
// Workflow tools
// ============================================================================

/// `start_task` - Start a task (draft/failed → planning), spawn agent.
async fn handle_start_task(state: &AppState, args: Value) -> Value {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return tool_error("task_id is required"),
    };

    // Load task
    let tid = task_id.clone();
    let task = match state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &tid))
        .await
    {
        Ok(t) => t,
        Err(e) => return tool_app_error("start_task", e),
    };

    // Validate status
    if task.status != TaskStatus::Draft && task.status != TaskStatus::Failed {
        return tool_error(&format!(
            "Cannot start task with status: {}. Expected: draft or failed",
            task.status.as_str()
        ));
    }

    // Generate branch name
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
    let task_suffix = &task_id[..task_id.len().min(8)];
    let branch_name = format!("feature/{title_slug}-{task_suffix}");

    let is_retry = task.status == TaskStatus::Failed;

    // Update task
    let tid = task_id.clone();
    let branch_clone = branch_name.clone();
    if let Err(e) = state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &tid,
                &crate::models::task::UpdateTaskInput {
                    branch_name: Some(Some(branch_clone)),
                    status: Some(TaskStatus::Planning),
                    error: if is_retry { Some(None) } else { None },
                    ..Default::default()
                },
            )
        })
        .await
    {
        return tool_app_error("start_task", e);
    }

    state.sse_emitter.emit_status(&task_id, "planning").await;

    // Start agent in background
    let state_clone = state.clone();
    let id_clone = task_id.clone();
    let task_clone = task.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::routes::tasks::start_agent_for_task(&state_clone, &id_clone, &task_clone).await
        {
            warn!(task_id = %id_clone, error = %e, "Failed to start agent (MCP)");
            let tid = id_clone.clone();
            let err_msg = e.to_string();
            let _ = state_clone
                .db
                .call(move |conn| {
                    task_service::update_task(
                        conn,
                        &tid,
                        &crate::models::task::UpdateTaskInput {
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

    state
        .data_emitter
        .emit_change("task", "updated", Some(&task_id));
    info!(id = %task_id, "start_task (MCP)");
    tool_result(&json!({ "status": "started", "message": "Agent started" }))
}

/// `execute_task` - Execute a task from backlog/approved/failed/changes_requested.
///
/// Mirrors the TS `/execute` endpoint and the frontend retry button behavior.
async fn handle_execute_task(state: &AppState, args: Value) -> Value {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return tool_error("task_id is required"),
    };

    // Load task
    let tid = task_id.clone();
    let task = match state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &tid))
        .await
    {
        Ok(t) => t,
        Err(e) => return tool_app_error("execute_task", e),
    };

    // Validate status
    let valid_statuses = [
        TaskStatus::Backlog,
        TaskStatus::Approved,
        TaskStatus::Failed,
        TaskStatus::ChangesRequested,
    ];
    if !valid_statuses.contains(&task.status) {
        return tool_error(&format!(
            "Cannot execute task with status: {}. Valid statuses: backlog, approved, failed, changes_requested",
            task.status.as_str()
        ));
    }

    let is_retry = task.status == TaskStatus::Failed;
    let is_resume = task.status == TaskStatus::ChangesRequested;

    // Update status to planning synchronously + clear error on retry
    let tid = task_id.clone();
    if let Err(e) = state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &tid,
                &crate::models::task::UpdateTaskInput {
                    status: Some(TaskStatus::Planning),
                    error: if is_retry { Some(None) } else { None },
                    ..Default::default()
                },
            )
        })
        .await
    {
        return tool_app_error("execute_task", e);
    }

    state.sse_emitter.emit_status(&task_id, "planning").await;

    // Start agent in background
    let state_clone = state.clone();
    let id_clone = task_id.clone();
    let task_clone = task.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::routes::tasks::start_agent_for_task(&state_clone, &id_clone, &task_clone).await
        {
            warn!(task_id = %id_clone, error = %e, "Failed to start agent (MCP execute_task)");
            let tid = id_clone.clone();
            let err_msg = e.to_string();
            let _ = state_clone
                .db
                .call(move |conn| {
                    task_service::update_task(
                        conn,
                        &tid,
                        &crate::models::task::UpdateTaskInput {
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
            state_clone
                .data_emitter
                .emit_change("task", "updated", Some(&id_clone));
        }
    });

    state
        .data_emitter
        .emit_change("task", "updated", Some(&task_id));
    info!(id = %task_id, is_resume, "execute_task (MCP)");

    tool_result(&json!({
        "status": "started",
        "task_status": "planning",
        "message": if is_resume { "Agent resumed to address requested changes" } else { "Agent execution started" },
        "resume_mode": is_resume,
    }))
}

/// `approve_spec` - Approve a generated spec for a task.
async fn handle_approve_spec(state: &AppState, args: Value) -> Value {
    let task_id = match args["task_id"].as_str() {
        Some(id) => id.to_string(),
        None => return tool_error("Missing required parameter: task_id"),
    };

    let final_spec = args["final_spec"].as_str().map(String::from);

    let result = state
        .db
        .call(move |conn| {
            let task = task_service::approve_spec(conn, &task_id, final_spec.as_deref())?;
            let task_json = serde_json::to_value(&task)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON error: {e}")))?;
            Ok(task_json)
        })
        .await;

    match result {
        Ok(data) => {
            if let Some(id) = data["id"].as_str() {
                state.data_emitter.emit_change("task", "updated", Some(id));
            }
            tool_result(&data)
        }
        Err(e) => tool_app_error("Error approving spec", e),
    }
}

/// `send_feedback` - Send feedback to a running agent or resume an idle agent.
async fn handle_send_feedback(state: &AppState, args: Value) -> Value {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return tool_error("task_id is required"),
    };
    let message = match args.get("message").and_then(|v| v.as_str()) {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => return tool_error("message is required"),
    };

    // Load task
    let tid = task_id.clone();
    let task = match state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &tid))
        .await
    {
        Ok(t) => t,
        Err(e) => return tool_app_error("send_feedback", e),
    };

    // If agent is running, send feedback directly
    if state.agent_service.is_running(&task_id).await {
        let _ = state.agent_service.send_feedback(&task_id, &message).await;
        state
            .data_emitter
            .emit_change("task", "updated", Some(&task_id));
        info!(id = %task_id, "send_feedback direct (MCP)");
        return tool_result(&json!({ "status": "feedback_sent" }));
    }

    // Agent is NOT running — check if task is in a terminal/draft state
    let terminal = ["done", "failed", "draft"];
    if terminal.contains(&task.status.as_str()) {
        return tool_error(&format!(
            "Task is in '{}' status. Cannot send feedback to a terminal/draft task. Use execute_task to retry from 'failed', or create a new task.",
            task.status
        ));
    }

    // plan_review: approve the plan
    if task.status == TaskStatus::PlanReview {
        // Store user message in SSE history
        state
            .sse_emitter
            .store_event(
                &task_id,
                crate::utils::SSEEvent {
                    event_type: crate::utils::SSEEventType::ChatMessage,
                    data: json!({
                        "id": uuid::Uuid::new_v4().to_string(),
                        "role": "user",
                        "message": message,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }),
                },
            )
            .await;

        // Approve plan = resume agent
        let tid = task_id.clone();
        if let Err(e) = state
            .db
            .call(move |conn| {
                task_service::update_task(
                    conn,
                    &tid,
                    &crate::models::task::UpdateTaskInput {
                        status: Some(TaskStatus::Planning),
                        ..Default::default()
                    },
                )
            })
            .await
        {
            return tool_app_error("send_feedback (plan_approve)", e);
        }
        state.sse_emitter.emit_status(&task_id, "planning").await;

        // Resume agent in background
        let state_clone = state.clone();
        let id_clone = task_id.clone();
        let task_clone = task.clone();
        let msg = message.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::routes::tasks::resume_agent_for_task(
                &state_clone,
                &id_clone,
                &task_clone,
                &msg,
            )
            .await
            {
                warn!(task_id = %id_clone, error = %e, "Failed to resume agent (MCP send_feedback plan)");
                let tid = id_clone.clone();
                let err_msg = e.to_string();
                let _ = state_clone
                    .db
                    .call(move |conn| {
                        task_service::update_task(
                            conn,
                            &tid,
                            &crate::models::task::UpdateTaskInput {
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

        state
            .data_emitter
            .emit_change("task", "updated", Some(&task_id));
        info!(id = %task_id, "send_feedback plan_approved (MCP)");
        return tool_result(&json!({
            "status": "plan_approved",
            "message": "Plan approved. Agent is implementing...",
        }));
    }

    // Otherwise: store message and resume agent
    state
        .sse_emitter
        .store_event(
            &task_id,
            crate::utils::SSEEvent {
                event_type: crate::utils::SSEEventType::ChatMessage,
                data: json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "role": "user",
                    "message": message,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }),
            },
        )
        .await;

    let tid = task_id.clone();
    let _ = state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &tid,
                &crate::models::task::UpdateTaskInput {
                    status: Some(TaskStatus::Planning),
                    ..Default::default()
                },
            )
        })
        .await;
    state.sse_emitter.emit_status(&task_id, "planning").await;

    // Resume agent in background
    let state_clone = state.clone();
    let id_clone = task_id.clone();
    let task_clone = task.clone();
    let msg = message.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::routes::tasks::resume_agent_for_task(&state_clone, &id_clone, &task_clone, &msg)
                .await
        {
            warn!(task_id = %id_clone, error = %e, "Failed to resume agent (MCP send_feedback)");
            let tid = id_clone.clone();
            let err_msg = e.to_string();
            let _ = state_clone
                .db
                .call(move |conn| {
                    task_service::update_task(
                        conn,
                        &tid,
                        &crate::models::task::UpdateTaskInput {
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

    state
        .data_emitter
        .emit_change("task", "updated", Some(&task_id));
    info!(id = %task_id, "send_feedback agent_resumed (MCP)");
    tool_result(&json!({
        "status": "agent_resumed",
        "message": "Agent resumed with your message",
    }))
}

/// `extend_task_timeout` - Extend a running agent's timeout by 5 minutes.
async fn handle_extend_task_timeout(state: &AppState, args: Value) -> Value {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return tool_error("task_id is required"),
    };

    // Verify task exists
    let tid = task_id.clone();
    if let Err(e) = state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &tid))
        .await
    {
        return tool_app_error("extend_task_timeout", e);
    }

    if !state.agent_service.is_running(&task_id).await {
        return tool_error("No agent is currently running for this task. Use get_task to check current task status.");
    }

    match state.agent_service.extend_timeout(&task_id).await {
        Ok(new_timeout) => {
            let secs_from_now = new_timeout
                .saturating_duration_since(tokio::time::Instant::now())
                .as_secs();
            info!(id = %task_id, "extend_task_timeout (MCP)");
            tool_result(&json!({
                "status": "extended",
                "extended_by_seconds": 300,
                "new_timeout_in_seconds": secs_from_now,
            }))
        }
        Err(e) => tool_app_error("extend_task_timeout", e),
    }
}

/// `cancel_task` - Cancel agent execution for a task.
async fn handle_cancel_task(state: &AppState, args: Value) -> Value {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return tool_error("task_id is required"),
    };

    // Load task
    let tid = task_id.clone();
    let task = match state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &tid))
        .await
    {
        Ok(t) => t,
        Err(e) => return tool_app_error("cancel_task", e),
    };

    // If agent is running, cancel it
    if state.agent_service.is_running(&task_id).await {
        let _ = state.agent_service.cancel_agent(&task_id).await;
        state
            .data_emitter
            .emit_change("task", "updated", Some(&task_id));
        info!(id = %task_id, "cancel_task (MCP)");
        return tool_result(&json!({ "status": "canceled" }));
    }

    // Agent not running but task is stuck in an active status
    let active_statuses = [
        "planning",
        "in_progress",
        "coding",
        "plan_review",
        "approved",
        "awaiting_review",
        "merge_conflicts",
    ];
    if active_statuses.contains(&task.status.as_str()) {
        let tid = task_id.clone();
        let _ = state
            .db
            .call(move |conn| {
                task_service::update_task(
                    conn,
                    &tid,
                    &crate::models::task::UpdateTaskInput {
                        status: Some(TaskStatus::Canceled),
                        error: Some(Some(
                            "Task canceled by user (agent not running)".to_string(),
                        )),
                        ..Default::default()
                    },
                )
            })
            .await;
        state.sse_emitter.emit_status(&task_id, "canceled").await;
        state
            .data_emitter
            .emit_change("task", "updated", Some(&task_id));
        info!(id = %task_id, "cancel_task stuck (MCP)");
        return tool_result(&json!({ "status": "canceled" }));
    }

    tool_error(
        "No agent is currently running for this task. Use get_task to check current task status.",
    )
}

// ============================================================================
// Review tools
// ============================================================================

/// `get_changes` - Get persisted changes/diff data for a task.
async fn handle_get_changes(state: &AppState, args: Value) -> Value {
    let task_id = match args["task_id"].as_str() {
        Some(id) => id.to_string(),
        None => return tool_error("Missing required parameter: task_id"),
    };

    let result = state
        .db
        .call(move |conn| {
            let task = task_service::get_task_by_id(conn, &task_id)?;

            // Try to parse persisted changes_data
            if let Some(ref changes_json) = task.changes_data {
                match serde_json::from_str::<Value>(changes_json) {
                    Ok(data) => return Ok(data),
                    Err(_) => {
                        warn!(task_id = %task_id, "Failed to parse persisted changes_data");
                    }
                }
            }

            // No changes available
            Ok(json!({
                "error": "no_changes_available",
                "message": "No worktree or persisted changes found for this task.",
                "hint": "The agent may still be working. Use get_task to check status."
            }))
        })
        .await;

    match result {
        Ok(data) => tool_result(&data),
        Err(e) => tool_app_error("Error getting changes", e),
    }
}

/// `approve_changes` - Approve changes and create PR (awaiting_review/review → approved → pr_created).
async fn handle_approve_changes(state: &AppState, args: Value) -> Value {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return tool_error("task_id is required"),
    };

    // Load task
    let tid = task_id.clone();
    let task = match state
        .db
        .call(move |conn| task_service::get_task_by_id(conn, &tid))
        .await
    {
        Ok(t) => t,
        Err(e) => return tool_app_error("approve_changes", e),
    };

    // Validate status
    if task.status != TaskStatus::AwaitingReview && task.status != TaskStatus::Review {
        return tool_error(&format!(
            "Cannot approve task with status: {}. Expected: awaiting_review or review",
            task.status.as_str()
        ));
    }

    // Set status to approved
    let tid = task_id.clone();
    if let Err(e) = state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &tid,
                &crate::models::task::UpdateTaskInput {
                    status: Some(TaskStatus::Approved),
                    ..Default::default()
                },
            )
        })
        .await
    {
        return tool_app_error("approve_changes", e);
    }
    state.sse_emitter.emit_status(&task_id, "approved").await;

    // Push and create PR
    match crate::routes::tasks::push_and_create_pr(state.clone(), task_id.clone(), task).await {
        Ok(body) => {
            info!(id = %task_id, "approve_changes (MCP)");
            tool_result(&body.0)
        }
        Err(e) => tool_app_error("approve_changes", e),
    }
}

/// `request_changes` - Update task status to changes_requested.
async fn handle_request_changes(state: &AppState, args: Value) -> Value {
    let task_id = match args["task_id"].as_str() {
        Some(id) => id.to_string(),
        None => return tool_error("Missing required parameter: task_id"),
    };

    let feedback = match args["feedback"].as_str() {
        Some(f) if !f.is_empty() => f.to_string(),
        _ => return tool_error("Missing required parameter: feedback"),
    };

    let result = state
        .db
        .call(move |conn| {
            let task = task_service::get_task_by_id(conn, &task_id)?;

            // Validate status
            let valid_statuses = ["pr_created", "review", "awaiting_review"];
            if !valid_statuses.contains(&task.status.as_str()) {
                return Err(AppError::Validation(format!(
                    "Cannot request changes for task with status '{}'. Expected: {}",
                    task.status,
                    valid_statuses.join(", ")
                )));
            }

            let update = crate::models::task::UpdateTaskInput {
                status: Some(TaskStatus::ChangesRequested),
                error: Some(Some(feedback)),
                ..Default::default()
            };

            let updated = task_service::update_task(conn, &task_id, &update)?;
            let task_json = serde_json::to_value(&updated)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON error: {e}")))?;
            Ok(task_json)
        })
        .await;

    match result {
        Ok(data) => {
            if let Some(id) = data["id"].as_str() {
                state.data_emitter.emit_change("task", "updated", Some(id));
            }
            tool_result(&json!({
                "status": "changes_requested",
                "message": "Changes requested. Use execute_task to resume the agent."
            }))
        }
        Err(e) => tool_app_error("Error requesting changes", e),
    }
}

// ============================================================================
// Status tools
// ============================================================================

/// `delete_task` - Delete a task by ID, cancel agent, cleanup worktree.
async fn handle_delete_task(state: &AppState, args: Value) -> Value {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return tool_error("task_id is required"),
    };

    // Check task exists
    let tid = task_id.clone();
    let task = state
        .db
        .call(move |conn| crate::services::task_service::get_task_by_id(conn, &tid))
        .await;

    if let Err(e) = &task {
        return tool_app_error(
            "delete_task",
            AppError::NotFound(format!("Task not found (id: {task_id}): {e}")),
        );
    }

    // Cancel running agent if any
    if state.agent_service.is_running(&task_id).await {
        info!(id = %task_id, "Cancelling running agent before deleting task (MCP)");
        let _ = state.agent_service.cancel_agent(&task_id).await;
    }

    // Delete task
    let tid = task_id.clone();
    let delete_result = state
        .db
        .call(move |conn| crate::services::task_service::delete_task(conn, &tid))
        .await;

    match delete_result {
        Ok(_) => {
            // Fire-and-forget worktree cleanup
            let git_service = std::sync::Arc::clone(&state.git_service);
            let tid = task_id.clone();
            tokio::spawn(async move {
                if let Ok(true) = git_service.worktree_exists(&tid).await {
                    let _ = git_service.cleanup_worktree(&tid, true).await;
                }
            });

            state
                .data_emitter
                .emit_change("task", "deleted", Some(&task_id));
            info!(id = %task_id, "delete_task (MCP)");
            tool_result(&json!({ "status": "deleted", "task_id": task_id }))
        }
        Err(e) => tool_app_error("delete_task", e),
    }
}

/// `mark_pr_merged` / `mark_pr_closed` - Update task status directly.
async fn handle_pr_lifecycle(state: &AppState, args: Value, action: &str) -> Value {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return tool_error("task_id is required"),
    };

    let (new_status, status_str, message) = match action {
        "pr-merged" => (TaskStatus::Done, "done", "PR marked as merged."),
        "pr-closed" => (TaskStatus::Canceled, "canceled", "PR marked as closed."),
        _ => return tool_error(&format!("Unknown action: {action}")),
    };

    let tid = task_id.clone();
    let result = state
        .db
        .call(move |conn| {
            task_service::update_task(
                conn,
                &tid,
                &crate::models::task::UpdateTaskInput {
                    status: Some(new_status),
                    ..Default::default()
                },
            )
        })
        .await;

    match result {
        Ok(_) => {
            state
                .data_emitter
                .emit_change("task", "updated", Some(&task_id));
            info!(id = %task_id, action = action, "pr_lifecycle (MCP)");
            tool_result(&json!({ "status": status_str, "message": message }))
        }
        Err(e) => tool_app_error(action, e),
    }
}

/// `get_setup_status` - Get secrets configuration status.
async fn handle_get_setup_status(state: &AppState) -> Value {
    let result = state
        .db
        .call(|conn| {
            let status = crate::services::secrets_service::get_all_secrets_status(conn)?;

            info!("get_setup_status via MCP");

            Ok(json!({
                "secrets": {
                    "ai_configured": status.ai_configured,
                    "github_configured": status.github_configured,
                    "gitlab_configured": status.gitlab_configured
                },
                "agents": [],
                "oauth_configured": false
            }))
        })
        .await;

    match result {
        Ok(data) => tool_result(&data),
        Err(e) => tool_app_error("Error getting setup status", e),
    }
}
