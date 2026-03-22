//! Data management routes: export, import, and delete all data.
//!
//! These endpoints handle bulk operations on the database:
//! - `GET /api/data/export` - Export all data as JSON
//! - `POST /api/data/import` - Import data from JSON (with optional merge)
//! - `DELETE /api/data` - Delete all data (requires confirmation)

use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use crate::error::AppError;
use crate::AppState;

// ============================================================================
// Types
// ============================================================================

/// Column whitelist per table to prevent SQL injection on import.
fn table_column_whitelist() -> HashMap<&'static str, &'static [&'static str]> {
    let mut m = HashMap::new();
    m.insert(
        "tasks",
        [
            "id",
            "title",
            "description",
            "repo_url",
            "target_branch",
            "context_files",
            "build_command",
            "status",
            "pr_url",
            "error",
            "created_at",
            "updated_at",
            "repository_id",
            "user_input",
            "generated_spec",
            "generated_spec_at",
            "final_spec",
            "spec_approved_at",
            "was_spec_edited",
            "branch_name",
            "pr_number",
            "agent_type",
            "agent_model",
            "changes_data",
            "conflict_files",
            "spec_id",
        ]
        .as_slice(),
    );
    m.insert(
        "event_logs",
        ["id", "entity_id", "entity_type", "timestamp", "level", "message", "event_type", "event_data"].as_slice(),
    );
    m.insert(
        "repositories",
        [
            "id",
            "name",
            "url",
            "default_branch",
            "detected_stack",
            "conventions",
            "learned_patterns",
            "created_at",
            "updated_at",
        ]
        .as_slice(),
    );
    m
}

/// The ordered list of tables that may be exported/imported.
const VALID_TABLES: &[&str] = &["tasks", "event_logs", "repositories"];

/// Export response structure.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportData {
    version: i32,
    exported_at: String,
    tasks: Vec<HashMap<String, Value>>,
    event_logs: Vec<HashMap<String, Value>>,
    repositories: Vec<HashMap<String, Value>>,
}

/// Import request body.
#[derive(Debug, Deserialize)]
struct ImportData {
    #[serde(default)]
    version: Option<i32>,
    #[serde(default, rename = "exportedAt")]
    _exported_at: Option<String>,
    #[serde(default)]
    tasks: Vec<HashMap<String, Value>>,
    #[serde(default)]
    event_logs: Vec<HashMap<String, Value>>,
    /// Legacy field for backward compatibility with old exports.
    #[serde(default)]
    task_logs: Vec<HashMap<String, Value>>,
    #[serde(default)]
    repositories: Vec<HashMap<String, Value>>,
}

/// Import query parameters.
#[derive(Debug, Deserialize)]
struct ImportQuery {
    #[serde(default)]
    merge: Option<String>,
}

/// Delete request body requiring confirmation.
#[derive(Debug, Deserialize)]
struct DeleteConfirmation {
    #[serde(default)]
    confirmation: Option<String>,
}

/// Import response.
#[derive(Debug, Serialize)]
struct ImportResponse {
    success: bool,
    imported: ImportCounts,
    merged: bool,
}

#[derive(Debug, Serialize)]
struct ImportCounts {
    tasks: usize,
    event_logs: usize,
    repositories: usize,
}

/// Delete response.
#[derive(Debug, Serialize)]
struct DeleteResponse {
    success: bool,
    deleted: DeleteCounts,
}

#[derive(Debug, Serialize)]
struct DeleteCounts {
    tasks: i64,
    event_logs: i64,
    repositories: i64,
}

// ============================================================================
// Router
// ============================================================================

/// Builds the data routes sub-router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/export", get(export_data))
        .route("/import", post(import_data))
        .route("/", delete(delete_all_data))
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/data/export
///
/// Exports all data from tasks, task_logs, and repositories tables as JSON.
async fn export_data(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("GET /data/export");

    let export = state
        .db
        .call(|conn| {
            let tasks = get_all_rows_from_table(conn, "tasks")?;
            let event_logs = get_all_rows_from_table(conn, "event_logs")?;
            let repositories = get_all_rows_from_table(conn, "repositories")?;

            info!(
                tasks = tasks.len(),
                event_logs = event_logs.len(),
                repositories = repositories.len(),
                "Export completed"
            );

            Ok(ExportData {
                version: 1,
                exported_at: chrono::Utc::now().to_rfc3339(),
                tasks,
                event_logs,
                repositories,
            })
        })
        .await?;

    Ok(Json(export))
}

/// POST /api/data/import
///
/// Imports data from a JSON body. If `?merge=true`, data is merged with existing
/// records. Otherwise, existing data is deleted first.
async fn import_data(
    State(state): State<AppState>,
    Query(query): Query<ImportQuery>,
    Json(body): Json<ImportData>,
) -> Result<impl IntoResponse, AppError> {
    info!(version = body.version, "POST /data/import");

    let merge = query.merge.as_deref() == Some("true");

    let tasks_data = body.tasks;
    // Accept both event_logs (new) and task_logs (legacy) for backward compatibility
    let event_logs_data = if !body.event_logs.is_empty() {
        body.event_logs
    } else {
        body.task_logs
    };
    let repositories_data = body.repositories;

    let imported = state
        .db
        .with_transaction(move |conn| {
            if !merge {
                // Delete in correct order due to foreign key constraints
                conn.execute("DELETE FROM event_logs", [])
                    .map_err(AppError::Database)?;
                conn.execute("DELETE FROM tasks", [])
                    .map_err(AppError::Database)?;
                conn.execute("DELETE FROM repositories", [])
                    .map_err(AppError::Database)?;
                info!("Cleared existing data for full import");
            }

            let whitelist = table_column_whitelist();

            let tasks_inserted =
                insert_rows(conn, "tasks", &tasks_data, whitelist.get("tasks").unwrap())?;
            let event_logs_inserted = insert_rows(
                conn,
                "event_logs",
                &event_logs_data,
                whitelist.get("event_logs").unwrap(),
            )?;
            let repos_inserted = insert_rows(
                conn,
                "repositories",
                &repositories_data,
                whitelist.get("repositories").unwrap(),
            )?;

            info!(
                tasks = tasks_inserted,
                event_logs = event_logs_inserted,
                repositories = repos_inserted,
                merged = merge,
                "Import completed"
            );

            Ok(ImportCounts {
                tasks: tasks_inserted,
                event_logs: event_logs_inserted,
                repositories: repos_inserted,
            })
        })
        .await?;

    Ok(Json(ImportResponse {
        success: true,
        imported,
        merged: merge,
    }))
}

/// DELETE /api/data
///
/// Deletes all data from all tables. Requires body `{ "confirmation": "DELETE" }`.
async fn delete_all_data(
    State(state): State<AppState>,
    Json(body): Json<DeleteConfirmation>,
) -> Result<impl IntoResponse, AppError> {
    info!("DELETE /data");

    if body.confirmation.as_deref() != Some("DELETE") {
        return Err(AppError::Validation(
            "Confirmation required. Send { \"confirmation\": \"DELETE\" } to confirm deletion of all data".to_string(),
        ));
    }

    let deleted = state
        .db
        .with_transaction(|conn| {
            // Count before deleting
            let count_tasks: i64 = conn
                .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
                .map_err(AppError::Database)?;
            let count_event_logs: i64 = conn
                .query_row("SELECT COUNT(*) FROM event_logs", [], |row| row.get(0))
                .map_err(AppError::Database)?;
            let count_repos: i64 = conn
                .query_row("SELECT COUNT(*) FROM repositories", [], |row| row.get(0))
                .map_err(AppError::Database)?;

            // Delete in correct order for foreign keys
            conn.execute("DELETE FROM event_logs", [])
                .map_err(AppError::Database)?;
            conn.execute("DELETE FROM tasks", [])
                .map_err(AppError::Database)?;
            conn.execute("DELETE FROM repositories", [])
                .map_err(AppError::Database)?;

            info!(
                tasks = count_tasks,
                event_logs = count_event_logs,
                repositories = count_repos,
                "All data deleted"
            );

            Ok(DeleteCounts {
                tasks: count_tasks,
                event_logs: count_event_logs,
                repositories: count_repos,
            })
        })
        .await?;

    Ok(Json(DeleteResponse {
        success: true,
        deleted,
    }))
}

// ============================================================================
// Helpers
// ============================================================================

/// Reads all rows from a table and returns them as a vector of key-value maps.
///
/// Only reads from the hardcoded `VALID_TABLES` list to prevent SQL injection.
fn get_all_rows_from_table(
    conn: &rusqlite::Connection,
    table: &str,
) -> Result<Vec<HashMap<String, Value>>, AppError> {
    if !VALID_TABLES.contains(&table) {
        return Err(AppError::Validation(format!(
            "Invalid table name: {table}"
        )));
    }

    let sql = format!("SELECT * FROM {table}");
    let mut stmt = conn.prepare(&sql).map_err(AppError::Database)?;

    let column_names: Vec<String> = stmt
        .column_names()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    let rows = stmt
        .query_map([], |row| {
            let mut map = HashMap::new();
            for (i, col_name) in column_names.iter().enumerate() {
                let val = row.get_ref(i)?;
                let json_val = match val {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => Value::Number(n.into()),
                    rusqlite::types::ValueRef::Real(f) => {
                        Value::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into()))
                    }
                    rusqlite::types::ValueRef::Text(s) => {
                        let text = String::from_utf8_lossy(s).to_string();
                        Value::String(text)
                    }
                    rusqlite::types::ValueRef::Blob(b) => {
                        Value::String(hex::encode(b))
                    }
                };
                map.insert(col_name.clone(), json_val);
            }
            Ok(map)
        })
        .map_err(AppError::Database)?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(AppError::Database)?);
    }

    Ok(result)
}

/// Inserts rows into a table, filtering columns through the whitelist.
///
/// Returns the number of rows successfully inserted.
fn insert_rows(
    conn: &rusqlite::Connection,
    table: &str,
    rows: &[HashMap<String, Value>],
    allowed_columns: &[&str],
) -> Result<usize, AppError> {
    if rows.is_empty() {
        return Ok(0);
    }

    if !VALID_TABLES.contains(&table) {
        return Err(AppError::Validation(format!(
            "Invalid table name: {table}"
        )));
    }

    let mut inserted = 0usize;

    for row in rows {
        // Filter to allowed columns only
        let columns: Vec<&str> = row
            .keys()
            .filter_map(|k| {
                let k_str = k.as_str();
                if allowed_columns.contains(&k_str) {
                    Some(k_str)
                } else {
                    warn!(column = k_str, table = table, "Skipping invalid column");
                    None
                }
            })
            .collect();

        if columns.is_empty() {
            continue;
        }

        let values: Vec<Box<dyn rusqlite::types::ToSql>> = columns
            .iter()
            .map(|col| {
                let val = &row[*col];
                json_value_to_sql(val)
            })
            .collect();

        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            columns.join(", "),
            placeholders.join(", ")
        );

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            values.iter().map(|p| p.as_ref()).collect();

        match conn.execute(&sql, param_refs.as_slice()) {
            Ok(_) => inserted += 1,
            Err(e) => {
                warn!(table = table, error = %e, "Failed to insert row");
            }
        }
    }

    Ok(inserted)
}

/// Converts a serde_json::Value to a boxed ToSql for rusqlite.
fn json_value_to_sql(value: &Value) -> Box<dyn rusqlite::types::ToSql> {
    match value {
        Value::Null => Box::new(None::<String>),
        Value::Bool(b) => Box::new(if *b { 1i32 } else { 0i32 }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(n.to_string())
            }
        }
        Value::String(s) => Box::new(s.clone()),
        // For arrays/objects, serialize to JSON string
        _ => Box::new(serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())),
    }
}
