//! Task service for managing development tasks.
//!
//! Uses the `tasks` table (created in migration 1, extended in migrations 4, 7, 8, 9).
//! Supports both the legacy workflow and the two-agent workflow (PM Agent + Dev Agent).
//!
//! This module re-uses the model types from [`crate::models::task`] for the `Task`,
//! `TaskStatus`, and `UpdateTaskInput` structs. It defines its own `CreateTaskServiceInput`
//! because the service layer needs all fields optional to support both legacy and two-agent
//! workflows, while the API-facing `CreateTaskInput` model enforces required fields.

use rusqlite::{Connection, Row};
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::models::task::{Task, TaskStatus, UpdateTaskInput};

// ============================================================================
// Service-specific input type
// ============================================================================

/// Input for creating a new task at the service layer.
///
/// Unlike [`crate::models::task::CreateTaskInput`] (which requires `repository_id`
/// and `user_input` for API validation), this struct keeps all fields optional to
/// support both legacy workflow (title/description required, no repository) and
/// two-agent workflow (repository_id + user_input required).
#[derive(Debug, Clone, Default)]
pub struct CreateTaskServiceInput {
    pub repository_id: Option<String>,
    pub user_input: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub repo_url: Option<String>,
    pub target_branch: Option<String>,
    pub context_files: Option<Vec<String>>,
    pub build_command: Option<String>,
    pub agent_type: Option<String>,
    pub agent_model: Option<String>,
    pub spec_id: Option<String>,
}

/// Convert from the API model's CreateTaskInput to the service-level input.
impl From<crate::models::task::CreateTaskInput> for CreateTaskServiceInput {
    fn from(input: crate::models::task::CreateTaskInput) -> Self {
        Self {
            repository_id: Some(input.repository_id),
            user_input: Some(input.user_input),
            title: input.title,
            description: input.description,
            repo_url: input.repo_url,
            target_branch: input.target_branch,
            context_files: input.context_files,
            build_command: input.build_command,
            agent_type: input.agent_type,
            agent_model: input.agent_model,
            spec_id: input.spec_id,
        }
    }
}

// ============================================================================
// Column list (matches TS TASK_COLUMNS exactly in order)
// ============================================================================

const TASK_COLUMNS: &str = "\
    id, title, description, repo_url, target_branch, context_files, build_command, \
    status, pr_url, error, created_at, updated_at, \
    repository_id, user_input, generated_spec, generated_spec_at, \
    final_spec, spec_approved_at, was_spec_edited, branch_name, pr_number, \
    agent_type, agent_model, changes_data, conflict_files, base_commit, spec_id";

/// Whitelist of columns allowed in UPDATE operations (matches TS ALLOWED_UPDATE_COLUMNS).
const ALLOWED_UPDATE_COLUMNS: &[&str] = &[
    "title",
    "description",
    "repo_url",
    "target_branch",
    "context_files",
    "build_command",
    "status",
    "pr_url",
    "error",
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
    "base_commit",
    "spec_id",
];

// ============================================================================
// Helpers
// ============================================================================

/// Safely parses a JSON array string, returning an empty vec on failure.
fn safe_parse_json_array(value: Option<String>) -> Vec<String> {
    match value {
        Some(s) if !s.is_empty() => {
            serde_json::from_str::<Vec<String>>(&s).unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

/// Maps a `rusqlite::Row` (from a SELECT with `TASK_COLUMNS` order) to a [`Task`].
///
/// Column order must match `TASK_COLUMNS`:
///   0:id, 1:title, 2:description, 3:repo_url, 4:target_branch, 5:context_files,
///   6:build_command, 7:status, 8:pr_url, 9:error, 10:created_at, 11:updated_at,
///   12:repository_id, 13:user_input, 14:generated_spec, 15:generated_spec_at,
///   16:final_spec, 17:spec_approved_at, 18:was_spec_edited, 19:branch_name,
///   20:pr_number, 21:agent_type, 22:agent_model, 23:changes_data, 24:conflict_files
pub fn row_to_task(row: &Row) -> Result<Task, rusqlite::Error> {
    let status_str: String = row.get(7)?;
    let status: TaskStatus = status_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;

    Ok(Task {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        repo_url: row.get(3)?,
        target_branch: row.get(4)?,
        context_files: safe_parse_json_array(row.get(5)?),
        build_command: row.get(6)?,
        status,
        pr_url: row.get(8)?,
        error: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        repository_id: row.get(12)?,
        user_input: row.get(13)?,
        generated_spec: row.get(14)?,
        generated_spec_at: row.get(15)?,
        final_spec: row.get(16)?,
        spec_approved_at: row.get(17)?,
        was_spec_edited: row.get::<_, Option<i32>>(18)?.unwrap_or(0) == 1,
        branch_name: row.get(19)?,
        pr_number: row.get(20)?,
        agent_type: row.get(21)?,
        agent_model: row.get(22)?,
        changes_data: row.get(23)?,
        conflict_files: row.get(24)?,
        base_commit: row.get(25)?,
        spec_id: row.get(26)?,
    })
}

/// Extracts a title from a spec string (first non-empty line, stripped of markdown headers).
fn extract_title_from_spec(spec: &str) -> Option<String> {
    for line in spec.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            // Remove leading markdown headers (e.g., "# ", "## ")
            let title = trimmed.trim_start_matches('#').trim_start();
            return if title.len() > 100 {
                Some(format!("{}...", &title[..97]))
            } else {
                Some(title.to_string())
            };
        }
    }
    None
}

// ============================================================================
// Service Functions
// ============================================================================

/// Creates a new task with the given input.
///
/// Supports both the legacy workflow and the two-agent workflow.
/// For two-agent workflow tasks (those with `repository_id` AND `user_input`),
/// the initial status is `draft`. For legacy tasks, the initial status is `backlog`.
pub fn create_task(
    conn: &Connection,
    input: &CreateTaskServiceInput,
) -> Result<Task, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let context_files_json = serde_json::to_string(
        input.context_files.as_deref().unwrap_or(&[]),
    )
    .unwrap_or_else(|_| "[]".to_string());

    // Determine if this is a two-agent workflow task
    let is_two_agent = input.repository_id.is_some() && input.user_input.is_some();

    // For two-agent workflow, generate title from user_input if not provided
    let title = if let Some(ref t) = input.title {
        if !t.is_empty() {
            t.clone()
        } else if is_two_agent {
            let user_input = input.user_input.as_deref().unwrap_or("");
            if user_input.len() > 50 {
                format!("{}...", &user_input[..47])
            } else {
                user_input.to_string()
            }
        } else {
            String::new()
        }
    } else if is_two_agent {
        let user_input = input.user_input.as_deref().unwrap_or("");
        if user_input.len() > 50 {
            format!("{}...", &user_input[..47])
        } else {
            user_input.to_string()
        }
    } else {
        String::new()
    };

    let description = if let Some(ref d) = input.description {
        if !d.is_empty() {
            d.clone()
        } else if is_two_agent {
            input.user_input.clone().unwrap_or_default()
        } else {
            String::new()
        }
    } else if is_two_agent {
        input.user_input.clone().unwrap_or_default()
    } else {
        String::new()
    };

    let initial_status = if is_two_agent {
        TaskStatus::Draft
    } else {
        TaskStatus::Backlog
    };

    info!(
        id = %id,
        title = %title,
        workflow = if is_two_agent { "two-agent" } else { "legacy" },
        "Creating task"
    );

    conn.execute(
        "INSERT INTO tasks (
            id, title, description, repo_url, target_branch, context_files, build_command,
            status, created_at, updated_at,
            repository_id, user_input, was_spec_edited,
            agent_type, agent_model, spec_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        rusqlite::params![
            id,
            title,
            description,
            input.repo_url.as_deref().unwrap_or(""),
            input.target_branch.as_deref().unwrap_or("main"),
            context_files_json,
            input.build_command.as_deref(),
            initial_status.as_str(),
            now,
            now,
            input.repository_id.as_deref(),
            input.user_input.as_deref(),
            0i32, // was_spec_edited defaults to false
            input.agent_type.as_deref(),
            input.agent_model.as_deref(),
            input.spec_id.as_deref(),
        ],
    )
    .map_err(AppError::Database)?;

    let task = get_task_by_id(conn, &id)?;
    info!(id = %id, status = %initial_status, "Task created successfully");
    Ok(task)
}

/// Retrieves all tasks, ordered by creation date (newest first).
///
/// - If `repo_url` is `Some`, filters tasks by `repo_url`.
/// - If `repository_id` is `Some`, filters by `repository_id`.
/// - Both filters can be applied simultaneously.
pub fn get_all_tasks(
    conn: &Connection,
    repo_url: Option<&str>,
    repository_id: Option<&str>,
) -> Result<Vec<Task>, AppError> {
    debug!(repo_url = repo_url, repository_id = repository_id, "Fetching all tasks");

    let mut sql = format!("SELECT {TASK_COLUMNS} FROM tasks");
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(url) = repo_url {
        conditions.push(format!("repo_url = ?{}", params.len() + 1));
        params.push(Box::new(url.to_string()));
    }
    if let Some(rid) = repository_id {
        conditions.push(format!("repository_id = ?{}", params.len() + 1));
        params.push(Box::new(rid.to_string()));
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut stmt = conn.prepare(&sql).map_err(AppError::Database)?;

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|p| p.as_ref()).collect();

    let rows = stmt
        .query_map(param_refs.as_slice(), row_to_task)
        .map_err(AppError::Database)?;

    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row.map_err(AppError::Database)?);
    }

    Ok(tasks)
}

/// Retrieves a task by its ID.
///
/// Returns `AppError::NotFound` if the task does not exist.
pub fn get_task_by_id(conn: &Connection, id: &str) -> Result<Task, AppError> {
    debug!(id = id, "Fetching task by ID");

    let sql = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1");
    conn.query_row(&sql, [id], row_to_task)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Task not found: {id}"))
            }
            other => AppError::Database(other),
        })
}

/// Updates a task with the given input.
///
/// Only fields that are `Some` in `input` are updated. The `updated_at` timestamp
/// is always set to the current time. Returns the updated task.
///
/// Returns `AppError::NotFound` if the task does not exist.
pub fn update_task(
    conn: &Connection,
    id: &str,
    input: &UpdateTaskInput,
) -> Result<Task, AppError> {
    info!(id = id, "Updating task");

    // Verify the task exists
    let _existing = get_task_by_id(conn, id)?;

    // Build dynamic SET clauses
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1usize;

    /// Helper macro to add a column update for simple string fields (Option<String>).
    macro_rules! add_field {
        ($field:ident, $col:literal) => {
            if let Some(ref val) = input.$field {
                if ALLOWED_UPDATE_COLUMNS.contains(&$col) {
                    sets.push(format!("{} = ?{}", $col, idx));
                    values.push(Box::new(val.clone()));
                    idx += 1;
                }
            }
        };
    }

    /// Helper macro for Option<Option<T>> fields (nullable columns).
    macro_rules! add_nullable_field {
        ($field:ident, $col:literal) => {
            if let Some(ref val) = input.$field {
                if ALLOWED_UPDATE_COLUMNS.contains(&$col) {
                    sets.push(format!("{} = ?{}", $col, idx));
                    values.push(Box::new(val.clone()));
                    idx += 1;
                }
            }
        };
    }

    // Simple string fields (Option<String>)
    add_field!(title, "title");
    add_field!(description, "description");
    add_field!(repo_url, "repo_url");
    add_field!(target_branch, "target_branch");

    // status: Option<TaskStatus> -> store as string
    if let Some(ref status) = input.status {
        if ALLOWED_UPDATE_COLUMNS.contains(&"status") {
            sets.push(format!("status = ?{}", idx));
            values.push(Box::new(status.as_str().to_string()));
            idx += 1;
        }
    }

    // context_files: serialize Vec<String> to JSON
    if let Some(ref cf) = input.context_files {
        if ALLOWED_UPDATE_COLUMNS.contains(&"context_files") {
            let json = serde_json::to_string(cf).unwrap_or_else(|_| "[]".to_string());
            sets.push(format!("context_files = ?{}", idx));
            values.push(Box::new(json));
            idx += 1;
        }
    }

    // was_spec_edited: bool -> integer
    if let Some(val) = input.was_spec_edited {
        if ALLOWED_UPDATE_COLUMNS.contains(&"was_spec_edited") {
            sets.push(format!("was_spec_edited = ?{}", idx));
            values.push(Box::new(if val { 1i32 } else { 0i32 }));
            idx += 1;
        }
    }

    // Nullable string fields (Option<Option<String>>)
    add_nullable_field!(build_command, "build_command");
    add_nullable_field!(pr_url, "pr_url");
    add_nullable_field!(error, "error");
    add_nullable_field!(repository_id, "repository_id");
    add_nullable_field!(user_input, "user_input");
    add_nullable_field!(generated_spec, "generated_spec");
    add_nullable_field!(generated_spec_at, "generated_spec_at");
    add_nullable_field!(final_spec, "final_spec");
    add_nullable_field!(spec_approved_at, "spec_approved_at");
    add_nullable_field!(branch_name, "branch_name");
    add_nullable_field!(agent_type, "agent_type");
    add_nullable_field!(agent_model, "agent_model");
    add_nullable_field!(changes_data, "changes_data");
    add_nullable_field!(conflict_files, "conflict_files");
    add_nullable_field!(base_commit, "base_commit");

    // pr_number: Option<Option<i64>>
    if let Some(ref val) = input.pr_number {
        if ALLOWED_UPDATE_COLUMNS.contains(&"pr_number") {
            sets.push(format!("pr_number = ?{}", idx));
            values.push(Box::new(*val));
            idx += 1;
        }
    }

    if sets.is_empty() {
        // Nothing to update, return existing task
        return get_task_by_id(conn, id);
    }

    // Always update updated_at
    let now = chrono::Utc::now().to_rfc3339();
    sets.push(format!("updated_at = ?{}", idx));
    values.push(Box::new(now));
    idx += 1;

    let sql = format!(
        "UPDATE tasks SET {} WHERE id = ?{}",
        sets.join(", "),
        idx
    );
    values.push(Box::new(id.to_string()));

    debug!(id = id, sql = %sql, "Executing UPDATE");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        values.iter().map(|p| p.as_ref()).collect();
    let changes = conn
        .execute(&sql, param_refs.as_slice())
        .map_err(AppError::Database)?;

    debug!(id = id, changes = changes, "Rows modified by UPDATE");

    if changes == 0 {
        warn!(id = id, "No rows modified by UPDATE - task may not exist");
    }

    let updated_task = get_task_by_id(conn, id)?;
    info!(
        id = id,
        new_status = %updated_task.status,
        updated_at = %updated_task.updated_at,
        "Task updated successfully"
    );

    Ok(updated_task)
}

/// Deletes a task by its ID.
///
/// Returns `AppError::NotFound` if 0 rows were affected (task didn't exist).
pub fn delete_task(conn: &Connection, id: &str) -> Result<(), AppError> {
    info!(id = id, "Deleting task");

    let changes = conn
        .execute("DELETE FROM tasks WHERE id = ?1", [id])
        .map_err(AppError::Database)?;

    if changes == 0 {
        warn!(id = id, "Task not found for deletion");
        return Err(AppError::NotFound(format!("Task not found: {id}")));
    }

    info!(id = id, "Task deleted successfully");
    Ok(())
}

/// Retrieves tasks by status.
pub fn get_tasks_by_status(
    conn: &Connection,
    status: &TaskStatus,
) -> Result<Vec<Task>, AppError> {
    debug!(status = %status, "Fetching tasks by status");

    let sql = format!(
        "SELECT {TASK_COLUMNS} FROM tasks WHERE status = ?1 ORDER BY created_at DESC"
    );
    let mut stmt = conn.prepare(&sql).map_err(AppError::Database)?;

    let rows = stmt
        .query_map([status.as_str()], row_to_task)
        .map_err(AppError::Database)?;

    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row.map_err(AppError::Database)?);
    }

    Ok(tasks)
}

/// Updates the spec for a task.
///
/// When `is_generated` is true (PM Agent generated the spec):
/// - Sets `generated_spec`, `generated_spec_at`, copies to `final_spec`
/// - Moves status to `pending_approval`
///
/// When `is_generated` is false (user edited the spec):
/// - Sets `final_spec` and `was_spec_edited = true`
pub fn update_spec(
    conn: &Connection,
    id: &str,
    spec: &str,
    is_generated: bool,
) -> Result<Task, AppError> {
    let now = chrono::Utc::now().to_rfc3339();

    let input = if is_generated {
        UpdateTaskInput {
            generated_spec: Some(Some(spec.to_string())),
            generated_spec_at: Some(Some(now)),
            final_spec: Some(Some(spec.to_string())),
            status: Some(TaskStatus::PendingApproval),
            ..Default::default()
        }
    } else {
        UpdateTaskInput {
            final_spec: Some(Some(spec.to_string())),
            was_spec_edited: Some(true),
            ..Default::default()
        }
    };

    update_task(conn, id, &input)
}

/// Approves the spec and moves the task to `approved` status.
///
/// If `final_spec` is provided, it is used as the approved spec. Otherwise, the task's
/// existing `final_spec` or `generated_spec` is used.
///
/// Returns an error if there is no spec to approve.
pub fn approve_spec(
    conn: &Connection,
    id: &str,
    final_spec: Option<&str>,
) -> Result<Task, AppError> {
    let existing = get_task_by_id(conn, id)?;
    let now = chrono::Utc::now().to_rfc3339();

    // Determine which spec to approve
    let spec = final_spec
        .map(|s| s.to_string())
        .or(existing.final_spec.clone())
        .or(existing.generated_spec.clone())
        .ok_or_else(|| AppError::Validation("No spec to approve".to_string()))?;

    // Check if the spec was edited by comparing with generated_spec
    let was_edited = if final_spec.is_some() {
        final_spec.map(|s| s.to_string()) != existing.generated_spec
    } else {
        existing.was_spec_edited
    };

    // Extract title from spec if current title is empty
    let title = if existing.title.is_empty() {
        extract_title_from_spec(&spec).unwrap_or_else(|| "Untitled Task".to_string())
    } else {
        existing.title.clone()
    };

    let input = UpdateTaskInput {
        final_spec: Some(Some(spec.clone())),
        spec_approved_at: Some(Some(now)),
        was_spec_edited: Some(was_edited),
        status: Some(TaskStatus::Approved),
        title: Some(title),
        description: Some(spec),
        ..Default::default()
    };

    update_task(conn, id, &input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Sets up an in-memory database with the tasks table matching all migrations.
    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                repo_url TEXT NOT NULL,
                target_branch TEXT DEFAULT 'main',
                context_files TEXT DEFAULT '[]',
                build_command TEXT,
                status TEXT DEFAULT 'backlog',
                pr_url TEXT,
                error TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
                repository_id TEXT,
                user_input TEXT,
                generated_spec TEXT,
                generated_spec_at TEXT,
                final_spec TEXT,
                spec_approved_at TEXT,
                was_spec_edited INTEGER DEFAULT 0,
                branch_name TEXT,
                pr_number INTEGER,
                agent_type TEXT,
                agent_model TEXT,
                changes_data TEXT,
                conflict_files TEXT,
                base_commit TEXT,
                spec_id TEXT
            );
            CREATE INDEX idx_tasks_status ON tasks(status);
            CREATE INDEX idx_tasks_created_at ON tasks(created_at);
            CREATE INDEX idx_tasks_repository_id ON tasks(repository_id);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_create_task_legacy() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            title: Some("Test task".to_string()),
            description: Some("A test task".to_string()),
            repo_url: Some("https://github.com/test/repo".to_string()),
            ..Default::default()
        };

        let task = create_task(&conn, &input).unwrap();
        assert_eq!(task.title, "Test task");
        assert_eq!(task.description, "A test task");
        assert_eq!(task.status, TaskStatus::Backlog);
        assert_eq!(task.target_branch, "main");
        assert!(!task.id.is_empty());
    }

    #[test]
    fn test_create_task_two_agent() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            repository_id: Some("repo-uuid-123".to_string()),
            user_input: Some("Add dark mode support".to_string()),
            ..Default::default()
        };

        let task = create_task(&conn, &input).unwrap();
        assert_eq!(task.title, "Add dark mode support");
        assert_eq!(task.description, "Add dark mode support");
        assert_eq!(task.status, TaskStatus::Draft);
        assert_eq!(task.repository_id, Some("repo-uuid-123".to_string()));
    }

    #[test]
    fn test_create_task_two_agent_truncates_long_title() {
        let conn = setup_db();
        let long_input = "a".repeat(100);
        let input = CreateTaskServiceInput {
            repository_id: Some("repo-uuid-123".to_string()),
            user_input: Some(long_input.clone()),
            ..Default::default()
        };

        let task = create_task(&conn, &input).unwrap();
        assert_eq!(task.title.len(), 50); // 47 + "..."
        assert!(task.title.ends_with("..."));
    }

    #[test]
    fn test_get_task_by_id() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            title: Some("Find me".to_string()),
            description: Some("desc".to_string()),
            ..Default::default()
        };

        let created = create_task(&conn, &input).unwrap();
        let found = get_task_by_id(&conn, &created.id).unwrap();
        assert_eq!(found.title, "Find me");
    }

    #[test]
    fn test_get_task_by_id_not_found() {
        let conn = setup_db();
        let result = get_task_by_id(&conn, "nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[test]
    fn test_get_all_tasks() {
        let conn = setup_db();
        for i in 0..3 {
            let input = CreateTaskServiceInput {
                title: Some(format!("Task {i}")),
                description: Some("desc".to_string()),
                ..Default::default()
            };
            create_task(&conn, &input).unwrap();
        }

        let all = get_all_tasks(&conn, None, None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_get_all_tasks_filter_by_repository_id() {
        let conn = setup_db();
        let input1 = CreateTaskServiceInput {
            repository_id: Some("repo-1".to_string()),
            user_input: Some("task 1".to_string()),
            ..Default::default()
        };
        let input2 = CreateTaskServiceInput {
            repository_id: Some("repo-2".to_string()),
            user_input: Some("task 2".to_string()),
            ..Default::default()
        };
        create_task(&conn, &input1).unwrap();
        create_task(&conn, &input2).unwrap();

        let filtered = get_all_tasks(&conn, None, Some("repo-1")).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].repository_id, Some("repo-1".to_string()));
    }

    #[test]
    fn test_update_task() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            title: Some("Original".to_string()),
            description: Some("desc".to_string()),
            ..Default::default()
        };
        let task = create_task(&conn, &input).unwrap();

        let update = UpdateTaskInput {
            title: Some("Updated".to_string()),
            status: Some(TaskStatus::Coding),
            ..Default::default()
        };
        let updated = update_task(&conn, &task.id, &update).unwrap();
        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.status, TaskStatus::Coding);
    }

    #[test]
    fn test_update_task_not_found() {
        let conn = setup_db();
        let update = UpdateTaskInput {
            title: Some("X".to_string()),
            ..Default::default()
        };
        let result = update_task(&conn, "nonexistent", &update);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[test]
    fn test_delete_task() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            title: Some("To delete".to_string()),
            description: Some("desc".to_string()),
            ..Default::default()
        };
        let task = create_task(&conn, &input).unwrap();

        delete_task(&conn, &task.id).unwrap();
        assert!(get_task_by_id(&conn, &task.id).is_err());
    }

    #[test]
    fn test_delete_task_not_found() {
        let conn = setup_db();
        let result = delete_task(&conn, "nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[test]
    fn test_get_tasks_by_status() {
        let conn = setup_db();
        for _ in 0..2 {
            let input = CreateTaskServiceInput {
                title: Some("task".to_string()),
                description: Some("desc".to_string()),
                ..Default::default()
            };
            create_task(&conn, &input).unwrap();
        }

        let backlog = get_tasks_by_status(&conn, &TaskStatus::Backlog).unwrap();
        assert_eq!(backlog.len(), 2);

        let coding = get_tasks_by_status(&conn, &TaskStatus::Coding).unwrap();
        assert_eq!(coding.len(), 0);
    }

    #[test]
    fn test_update_spec_generated() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            repository_id: Some("repo-1".to_string()),
            user_input: Some("Add feature X".to_string()),
            ..Default::default()
        };
        let task = create_task(&conn, &input).unwrap();

        let updated =
            update_spec(&conn, &task.id, "# Feature X\n\nDetailed spec...", true).unwrap();
        assert_eq!(updated.status, TaskStatus::PendingApproval);
        assert_eq!(
            updated.generated_spec,
            Some("# Feature X\n\nDetailed spec...".to_string())
        );
        assert_eq!(
            updated.final_spec,
            Some("# Feature X\n\nDetailed spec...".to_string())
        );
        assert!(updated.generated_spec_at.is_some());
    }

    #[test]
    fn test_update_spec_user_edited() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            repository_id: Some("repo-1".to_string()),
            user_input: Some("Add feature X".to_string()),
            ..Default::default()
        };
        let task = create_task(&conn, &input).unwrap();

        let updated = update_spec(&conn, &task.id, "My edited spec", false).unwrap();
        assert_eq!(
            updated.final_spec,
            Some("My edited spec".to_string())
        );
        assert!(updated.was_spec_edited);
    }

    #[test]
    fn test_approve_spec() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            repository_id: Some("repo-1".to_string()),
            user_input: Some("Build a feature".to_string()),
            ..Default::default()
        };
        let task = create_task(&conn, &input).unwrap();

        // First generate a spec
        update_spec(&conn, &task.id, "# My Feature\n\nSpec details", true).unwrap();

        // Approve it
        let approved = approve_spec(&conn, &task.id, None).unwrap();
        assert_eq!(approved.status, TaskStatus::Approved);
        assert!(approved.spec_approved_at.is_some());
        assert!(!approved.was_spec_edited);
    }

    #[test]
    fn test_approve_spec_with_edits() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            repository_id: Some("repo-1".to_string()),
            user_input: Some("Build a feature".to_string()),
            ..Default::default()
        };
        let task = create_task(&conn, &input).unwrap();

        // Generate a spec
        update_spec(&conn, &task.id, "# Original Spec", true).unwrap();

        // Approve with edited spec
        let approved = approve_spec(&conn, &task.id, Some("# Edited Spec")).unwrap();
        assert_eq!(approved.status, TaskStatus::Approved);
        assert_eq!(
            approved.final_spec,
            Some("# Edited Spec".to_string())
        );
        assert!(approved.was_spec_edited);
    }

    #[test]
    fn test_approve_spec_no_spec() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            repository_id: Some("repo-1".to_string()),
            user_input: Some("idea".to_string()),
            ..Default::default()
        };
        let task = create_task(&conn, &input).unwrap();

        let result = approve_spec(&conn, &task.id, None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::Validation(_)));
    }

    #[test]
    fn test_extract_title_from_spec_fn() {
        assert_eq!(
            extract_title_from_spec("# My Feature\n\nDetails here"),
            Some("My Feature".to_string())
        );
        assert_eq!(
            extract_title_from_spec("## Sub heading\n\nMore"),
            Some("Sub heading".to_string())
        );
        assert_eq!(
            extract_title_from_spec("Plain text first line"),
            Some("Plain text first line".to_string())
        );
        assert_eq!(extract_title_from_spec(""), None);
        assert_eq!(extract_title_from_spec("\n\n\n"), None);
    }

    #[test]
    fn test_context_files_round_trip() {
        let conn = setup_db();
        let input = CreateTaskServiceInput {
            title: Some("ctx test".to_string()),
            description: Some("desc".to_string()),
            context_files: Some(vec!["src/main.rs".to_string(), "README.md".to_string()]),
            ..Default::default()
        };
        let task = create_task(&conn, &input).unwrap();
        assert_eq!(task.context_files, vec!["src/main.rs", "README.md"]);
    }

    #[test]
    fn test_from_model_create_input() {
        let model_input = crate::models::task::CreateTaskInput {
            repository_id: "repo-123".to_string(),
            user_input: "Add feature".to_string(),
            title: Some("My Task".to_string()),
            description: None,
            repo_url: None,
            target_branch: Some("main".to_string()),
            context_files: None,
            build_command: None,
            agent_type: None,
            agent_model: None,
            spec_id: None,
        };
        let service_input: CreateTaskServiceInput = model_input.into();
        assert_eq!(service_input.repository_id, Some("repo-123".to_string()));
        assert_eq!(service_input.user_input, Some("Add feature".to_string()));
        assert_eq!(service_input.title, Some("My Task".to_string()));
    }
}
