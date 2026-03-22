//! Spec service for managing spec agent specifications.
//!
//! Uses the `specs` table (created in migration 12).
//! Functions take `&Connection` as first arg and return `Result<T, AppError>`.

use rusqlite::{Connection, Row};
use tracing::{debug, info, warn};

use crate::db::Database;
use crate::error::AppError;
use crate::models::spec::{CreateSpecInput, Spec, SpecStatus, UpdateSpecInput};

// ============================================================================
// Column list
// ============================================================================

const SPEC_COLUMNS: &str = "\
    id, repository_id, title, user_input, draft_spec, final_spec, \
    agent_type, agent_model, status, task_id, created_at, updated_at, approved_at";

// ============================================================================
// Helpers
// ============================================================================

/// Maps a `rusqlite::Row` (from a SELECT with `SPEC_COLUMNS` order) to a [`Spec`].
///
/// Column order must match `SPEC_COLUMNS`:
///   0:id, 1:repository_id, 2:title, 3:user_input, 4:draft_spec, 5:final_spec,
///   6:agent_type, 7:agent_model, 8:status, 9:task_id, 10:created_at,
///   11:updated_at, 12:approved_at
fn row_to_spec(row: &Row) -> Result<Spec, rusqlite::Error> {
    let status_str: String = row.get(8)?;
    let status: SpecStatus = status_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(
            8,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;

    Ok(Spec {
        id: row.get(0)?,
        repository_id: row.get(1)?,
        title: row.get(2)?,
        user_input: row.get(3)?,
        draft_spec: row.get(4)?,
        final_spec: row.get(5)?,
        agent_type: row.get(6)?,
        agent_model: row.get(7)?,
        status,
        task_id: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        approved_at: row.get(12)?,
    })
}

// ============================================================================
// Service Functions
// ============================================================================

/// Creates a new spec with the given input.
///
/// Title defaults to the first 100 characters of `user_input` if not provided.
pub fn create_spec(
    conn: &Connection,
    input: &CreateSpecInput,
) -> Result<Spec, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let title = if let Some(ref t) = input.title {
        if !t.is_empty() {
            t.clone()
        } else {
            default_title_from_input(&input.user_input)
        }
    } else {
        default_title_from_input(&input.user_input)
    };

    info!(
        id = %id,
        title = %title,
        repository_id = %input.repository_id,
        "Creating spec"
    );

    conn.execute(
        "INSERT INTO specs (
            id, repository_id, title, user_input, agent_type, agent_model,
            status, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            input.repository_id,
            title,
            input.user_input,
            input.agent_type.as_deref(),
            input.agent_model.as_deref(),
            SpecStatus::Draft.as_str(),
            now,
            now,
        ],
    )
    .map_err(AppError::Database)?;

    let spec = get_spec_by_id(conn, &id)?;
    info!(id = %id, "Spec created successfully");
    Ok(spec)
}

/// Retrieves all specs, ordered by creation date (newest first).
///
/// If `repository_id` is `Some`, filters specs by `repository_id`.
pub fn get_all_specs(
    conn: &Connection,
    repository_id: Option<&str>,
) -> Result<Vec<Spec>, AppError> {
    debug!(repository_id = repository_id, "Fetching all specs");

    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
        if let Some(rid) = repository_id {
            (
                format!(
                    "SELECT {SPEC_COLUMNS} FROM specs WHERE repository_id = ?1 ORDER BY created_at DESC"
                ),
                vec![Box::new(rid.to_string())],
            )
        } else {
            (
                format!("SELECT {SPEC_COLUMNS} FROM specs ORDER BY created_at DESC"),
                vec![],
            )
        };

    let mut stmt = conn.prepare(&sql).map_err(AppError::Database)?;

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|p| p.as_ref()).collect();

    let rows = stmt
        .query_map(param_refs.as_slice(), row_to_spec)
        .map_err(AppError::Database)?;

    let mut specs = Vec::new();
    for row in rows {
        specs.push(row.map_err(AppError::Database)?);
    }

    Ok(specs)
}

/// Retrieves a spec by its ID.
///
/// Returns `AppError::NotFound` if the spec does not exist.
pub fn get_spec_by_id(conn: &Connection, id: &str) -> Result<Spec, AppError> {
    debug!(id = id, "Fetching spec by ID");

    let sql = format!("SELECT {SPEC_COLUMNS} FROM specs WHERE id = ?1");
    conn.query_row(&sql, [id], row_to_spec)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Spec not found: {id}"))
            }
            other => AppError::Database(other),
        })
}

/// Updates a spec with the given input.
///
/// Only fields that are `Some` in `input` are updated. The `updated_at` timestamp
/// is always set to the current time. Returns the updated spec.
///
/// Returns `AppError::NotFound` if the spec does not exist.
pub fn update_spec(
    conn: &Connection,
    id: &str,
    input: &UpdateSpecInput,
) -> Result<Spec, AppError> {
    info!(id = id, "Updating spec");

    // Verify the spec exists
    let _existing = get_spec_by_id(conn, id)?;

    // Build dynamic SET clauses
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1usize;

    // title: Option<String>
    if let Some(ref val) = input.title {
        sets.push(format!("title = ?{}", idx));
        values.push(Box::new(val.clone()));
        idx += 1;
    }

    // draft_spec: Option<Option<String>>
    if let Some(ref val) = input.draft_spec {
        sets.push(format!("draft_spec = ?{}", idx));
        values.push(Box::new(val.clone()));
        idx += 1;
    }

    // final_spec: Option<Option<String>>
    if let Some(ref val) = input.final_spec {
        sets.push(format!("final_spec = ?{}", idx));
        values.push(Box::new(val.clone()));
        idx += 1;
    }

    // status: Option<SpecStatus>
    if let Some(ref status) = input.status {
        sets.push(format!("status = ?{}", idx));
        values.push(Box::new(status.as_str().to_string()));
        idx += 1;
    }

    // task_id: Option<Option<String>>
    if let Some(ref val) = input.task_id {
        sets.push(format!("task_id = ?{}", idx));
        values.push(Box::new(val.clone()));
        idx += 1;
    }

    // approved_at: Option<Option<String>>
    if let Some(ref val) = input.approved_at {
        sets.push(format!("approved_at = ?{}", idx));
        values.push(Box::new(val.clone()));
        idx += 1;
    }

    // agent_type: Option<Option<String>>
    if let Some(ref val) = input.agent_type {
        sets.push(format!("agent_type = ?{}", idx));
        values.push(Box::new(val.clone()));
        idx += 1;
    }

    // agent_model: Option<Option<String>>
    if let Some(ref val) = input.agent_model {
        sets.push(format!("agent_model = ?{}", idx));
        values.push(Box::new(val.clone()));
        idx += 1;
    }

    if sets.is_empty() {
        // Nothing to update, return existing spec
        return get_spec_by_id(conn, id);
    }

    // Always update updated_at
    let now = chrono::Utc::now().to_rfc3339();
    sets.push(format!("updated_at = ?{}", idx));
    values.push(Box::new(now));
    idx += 1;

    let sql = format!(
        "UPDATE specs SET {} WHERE id = ?{}",
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
        warn!(id = id, "No rows modified by UPDATE - spec may not exist");
    }

    let updated_spec = get_spec_by_id(conn, id)?;
    info!(
        id = id,
        new_status = %updated_spec.status,
        updated_at = %updated_spec.updated_at,
        "Spec updated successfully"
    );

    Ok(updated_spec)
}

/// Deletes a spec by its ID.
///
/// Returns `AppError::NotFound` if 0 rows were affected (spec didn't exist).
pub fn delete_spec(conn: &Connection, id: &str) -> Result<(), AppError> {
    info!(id = id, "Deleting spec");

    let changes = conn
        .execute("DELETE FROM specs WHERE id = ?1", [id])
        .map_err(AppError::Database)?;

    if changes == 0 {
        warn!(id = id, "Spec not found for deletion");
        return Err(AppError::NotFound(format!("Spec not found: {id}")));
    }

    info!(id = id, "Spec deleted successfully");
    Ok(())
}

// ============================================================================
// Async helpers (used from agent_service completion logic)
// ============================================================================

/// Updates a spec's status, optionally setting spec_content and error.
///
/// This is an async function that uses `Database.call()` for use from
/// the agent service completion handler.
pub async fn update_spec_status(
    db: &Database,
    spec_id: &str,
    status: SpecStatus,
    draft_spec: Option<String>,
    _error_msg: Option<String>,
) -> Result<(), AppError> {
    let spec_id = spec_id.to_string();
    let status_str = status.to_string();
    info!(spec_id = %spec_id, status = %status_str, "Updating spec status");
    db.call(move |conn| {
        conn.execute(
            "UPDATE specs SET status = ?1, draft_spec = COALESCE(?2, draft_spec), updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![status_str, draft_spec, spec_id],
        ).map_err(AppError::Database)?;
        Ok(())
    })
    .await
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Generates a default title from user_input (first 100 chars).
fn default_title_from_input(user_input: &str) -> String {
    let trimmed = user_input.trim();
    // Take first line only
    let first_line = trimmed.lines().next().unwrap_or(trimmed);
    if first_line.len() > 100 {
        format!("{}...", &first_line[..97])
    } else {
        first_line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Sets up an in-memory database with the specs table.
    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE repositories (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                url TEXT NOT NULL UNIQUE,
                default_branch TEXT DEFAULT 'main',
                detected_stack TEXT,
                conventions TEXT,
                learned_patterns TEXT DEFAULT '[]',
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO repositories (id, name, url) VALUES ('repo-1', 'Test Repo', 'file:///test/repo');

            CREATE TABLE tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                repo_url TEXT NOT NULL,
                status TEXT DEFAULT 'backlog',
                target_branch TEXT DEFAULT 'main',
                context_files TEXT DEFAULT '[]',
                build_command TEXT,
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
                base_commit TEXT
            );

            CREATE TABLE specs (
                id TEXT PRIMARY KEY,
                repository_id TEXT NOT NULL,
                title TEXT NOT NULL,
                user_input TEXT NOT NULL,
                draft_spec TEXT,
                final_spec TEXT,
                agent_type TEXT,
                agent_model TEXT,
                status TEXT DEFAULT 'draft',
                task_id TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
                approved_at TEXT,
                FOREIGN KEY (repository_id) REFERENCES repositories(id),
                FOREIGN KEY (task_id) REFERENCES tasks(id)
            );
            CREATE INDEX idx_specs_repository_id ON specs(repository_id);
            CREATE INDEX idx_specs_status ON specs(status);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_create_spec() {
        let conn = setup_db();
        let input = CreateSpecInput {
            repository_id: "repo-1".to_string(),
            user_input: "Add dark mode support to the dashboard".to_string(),
            title: None,
            agent_type: Some("claude-code".to_string()),
            agent_model: None,
        };

        let spec = create_spec(&conn, &input).unwrap();
        assert_eq!(spec.title, "Add dark mode support to the dashboard");
        assert_eq!(spec.status, SpecStatus::Draft);
        assert_eq!(spec.repository_id, "repo-1");
        assert!(spec.draft_spec.is_none());
        assert!(spec.task_id.is_none());
        assert!(!spec.id.is_empty());
    }

    #[test]
    fn test_create_spec_with_title() {
        let conn = setup_db();
        let input = CreateSpecInput {
            repository_id: "repo-1".to_string(),
            user_input: "Some input".to_string(),
            title: Some("Custom Title".to_string()),
            agent_type: None,
            agent_model: None,
        };

        let spec = create_spec(&conn, &input).unwrap();
        assert_eq!(spec.title, "Custom Title");
    }

    #[test]
    fn test_create_spec_truncates_long_title() {
        let conn = setup_db();
        let long_input = "a".repeat(200);
        let input = CreateSpecInput {
            repository_id: "repo-1".to_string(),
            user_input: long_input,
            title: None,
            agent_type: None,
            agent_model: None,
        };

        let spec = create_spec(&conn, &input).unwrap();
        assert_eq!(spec.title.len(), 100); // 97 + "..."
        assert!(spec.title.ends_with("..."));
    }

    #[test]
    fn test_get_spec_by_id() {
        let conn = setup_db();
        let input = CreateSpecInput {
            repository_id: "repo-1".to_string(),
            user_input: "Find me".to_string(),
            title: None,
            agent_type: None,
            agent_model: None,
        };

        let created = create_spec(&conn, &input).unwrap();
        let found = get_spec_by_id(&conn, &created.id).unwrap();
        assert_eq!(found.user_input, "Find me");
    }

    #[test]
    fn test_get_spec_by_id_not_found() {
        let conn = setup_db();
        let result = get_spec_by_id(&conn, "nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[test]
    fn test_get_all_specs() {
        let conn = setup_db();
        for i in 0..3 {
            let input = CreateSpecInput {
                repository_id: "repo-1".to_string(),
                user_input: format!("Spec {i}"),
                title: None,
                agent_type: None,
                agent_model: None,
            };
            create_spec(&conn, &input).unwrap();
        }

        let all = get_all_specs(&conn, None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_get_all_specs_filter_by_repository_id() {
        let conn = setup_db();
        let input = CreateSpecInput {
            repository_id: "repo-1".to_string(),
            user_input: "spec 1".to_string(),
            title: None,
            agent_type: None,
            agent_model: None,
        };
        create_spec(&conn, &input).unwrap();

        let filtered = get_all_specs(&conn, Some("repo-1")).unwrap();
        assert_eq!(filtered.len(), 1);

        let empty = get_all_specs(&conn, Some("repo-2")).unwrap();
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn test_update_spec() {
        let conn = setup_db();
        let input = CreateSpecInput {
            repository_id: "repo-1".to_string(),
            user_input: "Original".to_string(),
            title: None,
            agent_type: None,
            agent_model: None,
        };
        let spec = create_spec(&conn, &input).unwrap();

        let update = UpdateSpecInput {
            title: Some("Updated Title".to_string()),
            status: Some(SpecStatus::Generating),
            ..Default::default()
        };
        let updated = update_spec(&conn, &spec.id, &update).unwrap();
        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.status, SpecStatus::Generating);
    }

    #[test]
    fn test_update_spec_draft() {
        let conn = setup_db();
        let input = CreateSpecInput {
            repository_id: "repo-1".to_string(),
            user_input: "idea".to_string(),
            title: None,
            agent_type: None,
            agent_model: None,
        };
        let spec = create_spec(&conn, &input).unwrap();

        let update = UpdateSpecInput {
            draft_spec: Some(Some("# Draft Spec\n\nDetails here".to_string())),
            ..Default::default()
        };
        let updated = update_spec(&conn, &spec.id, &update).unwrap();
        assert_eq!(
            updated.draft_spec,
            Some("# Draft Spec\n\nDetails here".to_string())
        );
    }

    #[test]
    fn test_update_spec_not_found() {
        let conn = setup_db();
        let update = UpdateSpecInput {
            title: Some("X".to_string()),
            ..Default::default()
        };
        let result = update_spec(&conn, "nonexistent", &update);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[test]
    fn test_delete_spec() {
        let conn = setup_db();
        let input = CreateSpecInput {
            repository_id: "repo-1".to_string(),
            user_input: "To delete".to_string(),
            title: None,
            agent_type: None,
            agent_model: None,
        };
        let spec = create_spec(&conn, &input).unwrap();

        delete_spec(&conn, &spec.id).unwrap();
        assert!(get_spec_by_id(&conn, &spec.id).is_err());
    }

    #[test]
    fn test_delete_spec_not_found() {
        let conn = setup_db();
        let result = delete_spec(&conn, "nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }
}
