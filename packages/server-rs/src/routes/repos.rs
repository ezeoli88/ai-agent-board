//! Repository management routes.
//!
//! Endpoints:
//! - `GET  /api/repos`                    - List all repositories
//! - `POST /api/repos`                    - Create a new repository
//! - `GET  /api/repos/github/repos`       - (STUB) List GitHub repos
//! - `POST /api/repos/github/repos/validate` - (STUB) Validate GitHub repo URL
//! - `GET  /api/repos/local/pick-folder`  - (STUB) Open folder picker
//! - `GET  /api/repos/local/scan`         - (STUB) Scan local filesystem
//! - `POST /api/repos/local/add`          - Add local repository
//! - `GET  /api/repos/:id`                - Get repository by ID
//! - `PATCH /api/repos/:id`               - Update repository
//! - `DELETE /api/repos/:id`              - Delete repository
//! - `POST /api/repos/:id/detect-stack`   - (STUB) Re-detect stack
//! - `POST /api/repos/:id/patterns`       - Add learned pattern
//! - `DELETE /api/repos/:id/patterns`     - Clear all learned patterns
//! - `DELETE /api/repos/:id/patterns/:pattern_id` - Delete specific pattern
//!
//! IMPORTANT: Static routes (github/*, local/*) are registered BEFORE the `/:id`
//! wildcard routes to prevent path collision. This mirrors the critical ordering
//! from the TypeScript Express server.

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use tracing::info;

use crate::error::AppError;
use crate::models::repository::{
    CreateRepositoryInput, LearnedPattern, Repository, UpdateRepositoryInput,
};
use crate::AppState;

// ============================================================================
// Request / response types
// ============================================================================

/// Body for `POST /local/add`.
#[derive(Debug, Deserialize)]
struct LocalAddInput {
    name: Option<String>,
    path: Option<String>,
    default_branch: Option<String>,
    #[allow(dead_code)]
    remote_url: Option<String>,
}

/// Body for `POST /:id/patterns`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddPatternInput {
    pattern: Option<String>,
    task_id: Option<String>,
}

// ============================================================================
// Router
// ============================================================================

/// Builds the repos sub-router.
///
/// Static routes are registered first, then `/:id` routes, to avoid
/// axum matching literal segments as path parameters.
pub fn router() -> Router<AppState> {
    Router::new()
        // ---- Root routes ----
        .route("/", get(list_repos).post(create_repo))
        // ---- Static routes BEFORE /:id ----
        .route("/github/repos", get(github_list_repos))
        .route("/github/repos/validate", post(github_validate_repo))
        .route("/local/pick-folder", get(local_pick_folder))
        .route("/local/scan", get(local_scan))
        .route("/local/add", post(local_add))
        // ---- Dynamic :id routes ----
        .route(
            "/{id}",
            get(get_repo).patch(update_repo).delete(delete_repo),
        )
        .route("/{id}/detect-stack", post(detect_stack))
        .route("/{id}/patterns", post(add_pattern).delete(clear_patterns))
        .route("/{id}/patterns/{pattern_id}", delete(delete_pattern))
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/repos - List all repositories.
async fn list_repos(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("GET /repos");

    let repos = state
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
                .query_map([], |row| Ok(row_to_repository(row)))
                .map_err(AppError::Database)?;

            let mut result = Vec::new();
            for row in rows {
                result.push(row.map_err(AppError::Database)?);
            }
            Ok(result)
        })
        .await?;

    info!(count = repos.len(), "Repositories listed");
    Ok(Json(repos))
}

/// POST /api/repos - Create a new repository.
async fn create_repo(
    State(state): State<AppState>,
    Json(input): Json<CreateRepositoryInput>,
) -> Result<impl IntoResponse, AppError> {
    info!(name = %input.name, url = %input.url, "POST /repos");

    // Validate required fields
    if input.name.trim().is_empty() {
        return Err(AppError::Validation(
            "Repository name is required".to_string(),
        ));
    }
    if input.url.trim().is_empty() {
        return Err(AppError::Validation(
            "Repository URL is required".to_string(),
        ));
    }

    let url = input.url.clone();
    let name = input.name.clone();
    let default_branch = input.default_branch.clone();

    let repo = state
        .db
        .call(move |conn| {
            // Check for duplicate URL
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM repositories WHERE url = ?1",
                    [&url],
                    |row| row.get(0),
                )
                .ok();

            if existing.is_some() {
                return Err(AppError::Conflict(
                    "Repository already exists".to_string(),
                ));
            }

            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO repositories (id, name, url, default_branch, detected_stack, conventions, learned_patterns, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![id, name, url, default_branch, "{}", "", "[]", now, now],
            )
            .map_err(AppError::Database)?;

            info!(id = %id, name = %name, "Repository created");

            Ok(Repository {
                id,
                name,
                url,
                default_branch,
                detected_stack: Default::default(),
                conventions: String::new(),
                learned_patterns: Vec::new(),
                active_tasks_count: 0,
                created_at: now.clone(),
                updated_at: now,
            })
        })
        .await?;

    state
        .data_emitter
        .emit_change("repo", "created", Some(&repo.id));

    Ok((axum::http::StatusCode::CREATED, Json(repo)))
}

/// GET /api/repos/github/repos - STUB: return empty repos array (Phase 3).
async fn github_list_repos() -> impl IntoResponse {
    info!("GET /repos/github/repos (stub)");
    Json(json!({
        "repos": [],
        "total_count": 0,
        "has_more": false
    }))
}

/// POST /api/repos/github/repos/validate - STUB: return valid=false.
async fn github_validate_repo() -> impl IntoResponse {
    info!("POST /repos/github/repos/validate (stub)");
    Json(json!({
        "valid": false,
        "message": "GitHub integration not yet available in Rust server"
    }))
}

/// GET /api/repos/local/pick-folder - STUB: return cancelled=true.
async fn local_pick_folder() -> impl IntoResponse {
    info!("GET /repos/local/pick-folder (stub)");
    Json(json!({
        "path": null,
        "cancelled": true
    }))
}

/// Query params for local scan.
#[derive(Debug, Deserialize)]
struct LocalScanQuery {
    path: Option<String>,
}

/// GET /api/repos/local/scan - Scan local filesystem for git repositories.
async fn local_scan(
    axum::extract::Query(query): axum::extract::Query<LocalScanQuery>,
) -> Result<impl IntoResponse, AppError> {
    info!(path = ?query.path, "GET /repos/local/scan");

    let scan_path = query.path.as_deref().map(std::path::Path::new);
    let result = crate::services::local_scan_service::scan_for_repos(scan_path).await?;

    info!(count = result.total, scan_path = %result.scan_path, "Local repos scanned");
    Ok(Json(serde_json::to_value(result).unwrap()))
}

/// POST /api/repos/local/add - Add a local repository (url = "file://{path}").
async fn local_add(
    State(state): State<AppState>,
    Json(input): Json<LocalAddInput>,
) -> Result<impl IntoResponse, AppError> {
    let name = input.name.as_deref().unwrap_or("").trim().to_string();
    let path = input.path.as_deref().unwrap_or("").trim().to_string();

    if name.is_empty() || path.is_empty() {
        return Err(AppError::Validation(
            "name and path are required".to_string(),
        ));
    }

    let url = format!("file://{path}");
    let default_branch = input.default_branch.unwrap_or_else(|| "main".to_string());

    info!(name = %name, path = %path, "POST /repos/local/add");

    let url_clone = url.clone();
    let name_clone = name.clone();
    let branch_clone = default_branch.clone();

    let repo = state
        .db
        .call(move |conn| {
            // Check for duplicate URL
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM repositories WHERE url = ?1",
                    [&url_clone],
                    |row| row.get(0),
                )
                .ok();

            if existing.is_some() {
                return Err(AppError::Conflict(
                    "Repository already exists".to_string(),
                ));
            }

            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO repositories (id, name, url, default_branch, detected_stack, conventions, learned_patterns, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![id, name_clone, url_clone, branch_clone, "{}", "", "[]", now, now],
            )
            .map_err(AppError::Database)?;

            info!(id = %id, name = %name_clone, path = %url_clone, "Local repository added");

            Ok(Repository {
                id,
                name: name_clone,
                url: url_clone,
                default_branch: branch_clone,
                detected_stack: Default::default(),
                conventions: String::new(),
                learned_patterns: Vec::new(),
                active_tasks_count: 0,
                created_at: now.clone(),
                updated_at: now,
            })
        })
        .await?;

    state
        .data_emitter
        .emit_change("repo", "created", Some(&repo.id));

    Ok((axum::http::StatusCode::CREATED, Json(repo)))
}

/// GET /api/repos/:id - Get a repository by ID.
async fn get_repo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "GET /repos/:id");

    let repo = state
        .db
        .call(move |conn| find_repo_by_id(conn, &id))
        .await?;

    Ok(Json(repo))
}

/// PATCH /api/repos/:id - Update repository fields.
async fn update_repo(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateRepositoryInput>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "PATCH /repos/:id");

    let id_for_emit = id.clone();

    let repo = state
        .db
        .call(move |conn| {
            // Verify the repo exists first
            let _existing = find_repo_by_id(conn, &id)?;

            // Build dynamic update
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(ref branch) = input.default_branch {
                sets.push("default_branch = ?");
                params.push(Box::new(branch.clone()));
            }
            if let Some(ref conventions) = input.conventions {
                sets.push("conventions = ?");
                params.push(Box::new(conventions.clone()));
            }

            if sets.is_empty() {
                // Nothing to update, return existing
                return find_repo_by_id(conn, &id);
            }

            let now = chrono::Utc::now().to_rfc3339();
            sets.push("updated_at = ?");
            params.push(Box::new(now));
            params.push(Box::new(id.clone()));

            let sql = format!("UPDATE repositories SET {} WHERE id = ?", sets.join(", "));

            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();

            conn.execute(&sql, param_refs.as_slice())
                .map_err(AppError::Database)?;

            find_repo_by_id(conn, &id)
        })
        .await?;

    state
        .data_emitter
        .emit_change("repo", "updated", Some(&id_for_emit));

    Ok(Json(repo))
}

/// DELETE /api/repos/:id - Delete a repository.
async fn delete_repo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "DELETE /repos/:id");

    let id_for_emit = id.clone();

    state
        .db
        .call(move |conn| {
            let changes = conn
                .execute("DELETE FROM repositories WHERE id = ?1", [&id])
                .map_err(AppError::Database)?;

            if changes == 0 {
                return Err(AppError::NotFound("Repository not found".to_string()));
            }

            info!(id = %id, "Repository deleted");
            Ok(())
        })
        .await?;

    state
        .data_emitter
        .emit_change("repo", "deleted", Some(&id_for_emit));

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /api/repos/:id/detect-stack - STUB: return the repo unchanged.
async fn detect_stack(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "POST /repos/:id/detect-stack (stub)");

    let repo = state
        .db
        .call(move |conn| find_repo_by_id(conn, &id))
        .await?;

    Ok(Json(repo))
}

/// POST /api/repos/:id/patterns - Add a learned pattern.
async fn add_pattern(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<AddPatternInput>,
) -> Result<impl IntoResponse, AppError> {
    let pattern_text = input.pattern.as_deref().unwrap_or("").trim().to_string();
    let task_id = input.task_id.as_deref().unwrap_or("").trim().to_string();

    if pattern_text.is_empty() || task_id.is_empty() {
        return Err(AppError::Validation(
            "pattern and taskId are required".to_string(),
        ));
    }

    info!(id = %id, pattern = %pattern_text, "POST /repos/:id/patterns");

    let repo = state
        .db
        .call(move |conn| {
            // Get existing patterns JSON
            let existing_json: String = conn
                .query_row(
                    "SELECT COALESCE(learned_patterns, '[]') FROM repositories WHERE id = ?1",
                    [&id],
                    |row| row.get(0),
                )
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        AppError::NotFound("Repository not found".to_string())
                    }
                    other => AppError::Database(other),
                })?;

            let mut patterns: Vec<LearnedPattern> =
                serde_json::from_str(&existing_json).unwrap_or_default();

            let new_pattern = LearnedPattern {
                id: uuid::Uuid::new_v4().to_string(),
                pattern: pattern_text,
                learned_from_task_id: task_id,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            patterns.push(new_pattern);

            let patterns_json = serde_json::to_string(&patterns)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON serialize error: {e}")))?;
            let now = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "UPDATE repositories SET learned_patterns = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![patterns_json, now, id],
            )
            .map_err(AppError::Database)?;

            find_repo_by_id(conn, &id)
        })
        .await?;

    Ok(Json(repo))
}

/// DELETE /api/repos/:id/patterns - Clear all learned patterns.
async fn clear_patterns(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, "DELETE /repos/:id/patterns");

    let result = state
        .db
        .call(move |conn| {
            // Get current patterns to count them
            let existing_json: String = conn
                .query_row(
                    "SELECT COALESCE(learned_patterns, '[]') FROM repositories WHERE id = ?1",
                    [&id],
                    |row| row.get(0),
                )
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        AppError::NotFound("Repository not found".to_string())
                    }
                    other => AppError::Database(other),
                })?;

            let patterns: Vec<LearnedPattern> =
                serde_json::from_str(&existing_json).unwrap_or_default();
            let cleared_count = patterns.len();

            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE repositories SET learned_patterns = '[]', updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, id],
            )
            .map_err(AppError::Database)?;

            info!(id = %id, count = cleared_count, "Learned patterns cleared");

            Ok(json!({
                "success": true,
                "cleared_count": cleared_count
            }))
        })
        .await?;

    Ok(Json(result))
}

/// DELETE /api/repos/:id/patterns/:pattern_id - Delete a specific pattern.
async fn delete_pattern(
    State(state): State<AppState>,
    Path((id, pattern_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    info!(id = %id, pattern_id = %pattern_id, "DELETE /repos/:id/patterns/:pattern_id");

    let result = state
        .db
        .call(move |conn| {
            // Get existing patterns
            let existing_json: String = conn
                .query_row(
                    "SELECT COALESCE(learned_patterns, '[]') FROM repositories WHERE id = ?1",
                    [&id],
                    |row| row.get(0),
                )
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        AppError::NotFound("Repository not found".to_string())
                    }
                    other => AppError::Database(other),
                })?;

            let mut patterns: Vec<LearnedPattern> =
                serde_json::from_str(&existing_json).unwrap_or_default();

            let original_len = patterns.len();
            patterns.retain(|p| p.id != pattern_id);

            if patterns.len() == original_len {
                return Err(AppError::NotFound("Pattern not found".to_string()));
            }

            let patterns_json = serde_json::to_string(&patterns)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON serialize error: {e}")))?;
            let now = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "UPDATE repositories SET learned_patterns = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![patterns_json, now, id],
            )
            .map_err(AppError::Database)?;

            info!(id = %id, pattern_id = %pattern_id, "Learned pattern deleted");

            Ok(json!({ "success": true }))
        })
        .await?;

    Ok(Json(result))
}

// ============================================================================
// Helpers
// ============================================================================

/// Reads a single repository row from the database by ID.
///
/// Returns `AppError::NotFound` if no row matches.
fn find_repo_by_id(conn: &rusqlite::Connection, id: &str) -> Result<Repository, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT r.*, \
               (SELECT COUNT(*) FROM tasks t \
                WHERE t.repository_id = r.id \
                AND t.status NOT IN ('done', 'archived')) as active_tasks_count \
             FROM repositories r WHERE r.id = ?1",
        )
        .map_err(AppError::Database)?;

    stmt.query_row([id], |row| Ok(row_to_repository(row)))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("Repository not found".to_string())
            }
            other => AppError::Database(other),
        })
}

/// Maps a rusqlite Row to a Repository struct.
///
/// Expects the query to include all `repositories` columns plus an
/// `active_tasks_count` computed column at the end.
fn row_to_repository(row: &rusqlite::Row<'_>) -> Repository {
    let detected_stack_json: String = row
        .get::<_, String>("detected_stack")
        .unwrap_or_else(|_| "{}".to_string());
    let learned_patterns_json: String = row
        .get::<_, String>("learned_patterns")
        .unwrap_or_else(|_| "[]".to_string());

    Repository {
        id: row.get("id").unwrap_or_default(),
        name: row.get("name").unwrap_or_default(),
        url: row.get("url").unwrap_or_default(),
        default_branch: row
            .get("default_branch")
            .unwrap_or_else(|_| "main".to_string()),
        detected_stack: serde_json::from_str(&detected_stack_json).unwrap_or_default(),
        conventions: row.get("conventions").unwrap_or_default(),
        learned_patterns: serde_json::from_str(&learned_patterns_json).unwrap_or_default(),
        active_tasks_count: row.get("active_tasks_count").unwrap_or(0),
        created_at: row.get("created_at").unwrap_or_default(),
        updated_at: row.get("updated_at").unwrap_or_default(),
    }
}
