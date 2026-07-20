//! MCP resource definitions and handlers.
//!
//! Resources expose data via URI patterns that MCP clients can read.
//! This implements four resource templates matching the TypeScript MCP server:
//!
//! - `agentboard://tasks/{id}`          - Task details JSON
//! - `agentboard://tasks/{id}/changes`  - Task changes/diff data
//! - `agentboard://repos/{id}`          - Repository details JSON
//! - `agentboard://status`              - Board setup status

use serde_json::{json, Value};
use tracing::{info, warn};

use crate::error::AppError;
use crate::services::{secrets_service, task_service};
use crate::AppState;

// ============================================================================
// Resource list (returned by resources/list)
// ============================================================================

/// Returns the list of all resource templates for `resources/list`.
pub fn resource_definitions() -> Value {
    json!({
        "resources": [
            {
                "uri": "agentboard://status",
                "name": "Board Status",
                "description": "Agent Board setup status: AI provider configuration and secret status",
                "mimeType": "application/json"
            }
        ],
        "resourceTemplates": [
            {
                "uriTemplate": "agentboard://tasks/{id}",
                "name": "Task Details",
                "description": "Full task details including status, branch, PR URL, and all metadata",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "agentboard://tasks/{id}/changes",
                "name": "Task Changes",
                "description": "Changed files and diff for a task from persisted data",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "agentboard://repos/{id}",
                "name": "Repository Details",
                "description": "Full repository details including detected stack, conventions, and learned patterns",
                "mimeType": "application/json"
            }
        ]
    })
}

// ============================================================================
// Resource read dispatch
// ============================================================================

/// Reads a resource by its URI. Returns an MCP resource response with `contents`.
///
/// Supported URI patterns:
/// - `agentboard://tasks/{id}`
/// - `agentboard://tasks/{id}/changes`
/// - `agentboard://repos/{id}`
/// - `agentboard://status`
pub async fn read_resource(state: &AppState, uri: &str) -> Value {
    // Parse the URI to determine which resource to read
    let stripped = uri.strip_prefix("agentboard://").unwrap_or(uri);

    if stripped == "status" {
        return read_status(state, uri).await;
    }

    if let Some(rest) = stripped.strip_prefix("tasks/") {
        // Check for /changes suffix
        if let Some(id) = rest.strip_suffix("/changes") {
            return read_task_changes(state, uri, id).await;
        }
        // Plain task ID
        return read_task(state, uri, rest).await;
    }

    if let Some(id) = stripped.strip_prefix("repos/") {
        return read_repo(state, uri, id).await;
    }

    // Unknown resource URI
    json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": serde_json::to_string_pretty(&json!({
                "error": "unknown_resource",
                "message": format!("Unknown resource URI: {uri}")
            })).unwrap_or_default()
        }]
    })
}

// ============================================================================
// Individual resource handlers
// ============================================================================

/// Reads `agentboard://tasks/{id}` - Full task details.
async fn read_task(state: &AppState, uri: &str, task_id: &str) -> Value {
    let task_id = task_id.to_string();
    let uri = uri.to_string();

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
        Ok(data) => resource_contents(&uri, &data),
        Err(AppError::NotFound(_)) => {
            let id = uri.strip_prefix("agentboard://tasks/").unwrap_or("unknown");
            resource_contents(&uri, &json!({ "error": "Task not found", "id": id }))
        }
        Err(e) => resource_contents(
            &uri,
            &json!({ "error": "Failed to read task", "message": e.to_string() }),
        ),
    }
}

/// Reads `agentboard://tasks/{id}/changes` - Task changes/diff data.
async fn read_task_changes(state: &AppState, uri: &str, task_id: &str) -> Value {
    let task_id = task_id.to_string();
    let task_id_for_error = task_id.clone();
    let uri = uri.to_string();

    let result = state
        .db
        .call(move |conn| {
            let task = task_service::get_task_by_id(conn, &task_id)?;

            // Try to parse persisted changes_data
            if let Some(ref changes_json) = task.changes_data {
                if let Ok(data) = serde_json::from_str::<Value>(changes_json) {
                    return Ok(data);
                }
                warn!(task_id = %task_id, "Failed to parse persisted changes_data");
            }

            // No changes available
            Ok(json!({
                "error": "No changes available",
                "message": "No worktree or persisted changes found for this task."
            }))
        })
        .await;

    match result {
        Ok(data) => resource_contents(&uri, &data),
        Err(AppError::NotFound(_)) => resource_contents(
            &uri,
            &json!({ "error": "Task not found", "id": task_id_for_error }),
        ),
        Err(e) => resource_contents(
            &uri,
            &json!({ "error": "Failed to read task changes", "message": e.to_string() }),
        ),
    }
}

/// Reads `agentboard://repos/{id}` - Repository details.
async fn read_repo(state: &AppState, uri: &str, repo_id: &str) -> Value {
    let repo_id = repo_id.to_string();
    let repo_id_for_error = repo_id.clone();
    let uri = uri.to_string();

    let result = state
        .db
        .call(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT r.*, \
                       (SELECT COUNT(*) FROM tasks t \
                        WHERE t.repository_id = r.id \
                        AND t.status NOT IN ('done', 'archived')) as active_tasks_count \
                     FROM repositories r WHERE r.id = ?1",
                )
                .map_err(AppError::Database)?;

            let repo = stmt
                .query_row([&repo_id], |row| {
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
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        AppError::NotFound("Repository not found".to_string())
                    }
                    other => AppError::Database(other),
                })?;

            Ok(repo)
        })
        .await;

    match result {
        Ok(data) => resource_contents(&uri, &data),
        Err(AppError::NotFound(_)) => resource_contents(
            &uri,
            &json!({ "error": "Repository not found", "id": repo_id_for_error }),
        ),
        Err(e) => resource_contents(
            &uri,
            &json!({ "error": "Failed to read repository", "message": e.to_string() }),
        ),
    }
}

/// Reads `agentboard://status` - Board setup status.
async fn read_status(state: &AppState, uri: &str) -> Value {
    let uri = uri.to_string();

    let result = state
        .db
        .call(|conn| {
            let status = secrets_service::get_all_secrets_status(conn)?;

            info!("Board status resource read via MCP");

            Ok(json!({
                "secrets": {
                    "ai_configured": status.ai_configured,
                    "github_configured": status.github_configured,
                    "gitlab_configured": status.gitlab_configured
                },
                "agents": [],
                "oauth_configured": false,
                "server_connected": true
            }))
        })
        .await;

    match result {
        Ok(data) => resource_contents(&uri, &data),
        Err(e) => resource_contents(
            &uri,
            &json!({
                "error": "Failed to retrieve status",
                "message": e.to_string(),
                "server_connected": true
            }),
        ),
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Wraps data into an MCP resource response with `contents`.
fn resource_contents(uri: &str, data: &Value) -> Value {
    json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string())
        }]
    })
}
