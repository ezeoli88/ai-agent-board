//! Service layer for standalone spec-driven development demo workflows.

use rusqlite::{Connection, Row};
use tracing::{debug, info};

use crate::error::AppError;
use crate::models::spec::{
    CreateSpecInput, Spec, SpecAgentRole, SpecAgentSession, SpecStatus, UpdateSpecInput,
};

const SPEC_COLUMNS: &str = "\
    id, repository_id, title, user_input, status, \
    clarification_questions, clarification_answers, clarification_answered_at, \
    generated_spec, final_spec, spec_approved_at, \
    generated_plan, final_plan, plan_approved_at, \
    generated_tasks, final_tasks, tasks_approved_at, task_ids, \
    agent_type, agent_model, agent_session_id, error, created_at, updated_at";

const ALLOWED_UPDATE_COLUMNS: &[&str] = &[
    "title",
    "user_input",
    "status",
    "clarification_questions",
    "clarification_answers",
    "clarification_answered_at",
    "generated_spec",
    "final_spec",
    "spec_approved_at",
    "generated_plan",
    "final_plan",
    "plan_approved_at",
    "generated_tasks",
    "final_tasks",
    "tasks_approved_at",
    "task_ids",
    "agent_type",
    "agent_model",
    "agent_session_id",
    "error",
];

fn parse_json_array(value: Option<String>) -> Vec<String> {
    value
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

fn title_from_input(input: &str) -> String {
    let first_line = input
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(input);
    let trimmed = first_line.trim();
    if trimmed.len() > 100 {
        format!("{}...", &trimmed[..97])
    } else if trimmed.is_empty() {
        "Untitled Spec".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn row_to_spec(row: &Row) -> Result<Spec, rusqlite::Error> {
    let status_str: String = row.get(4)?;
    let status: SpecStatus = status_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;

    Ok(Spec {
        id: row.get(0)?,
        repository_id: row.get(1)?,
        title: row.get(2)?,
        user_input: row.get(3)?,
        status,
        clarification_questions: parse_json_array(row.get(5)?),
        clarification_answers: parse_json_array(row.get(6)?),
        clarification_answered_at: row.get(7)?,
        generated_spec: row.get(8)?,
        final_spec: row.get(9)?,
        spec_approved_at: row.get(10)?,
        generated_plan: row.get(11)?,
        final_plan: row.get(12)?,
        plan_approved_at: row.get(13)?,
        generated_tasks: row.get(14)?,
        final_tasks: row.get(15)?,
        tasks_approved_at: row.get(16)?,
        task_ids: parse_json_array(row.get(17)?),
        agent_type: row.get(18)?,
        agent_model: row.get(19)?,
        agent_session_id: row.get(20)?,
        error: row.get(21)?,
        created_at: row.get(22)?,
        updated_at: row.get(23)?,
    })
}

fn row_to_spec_agent_session(row: &Row) -> Result<SpecAgentSession, rusqlite::Error> {
    let role_str: String = row.get(2)?;
    let role: SpecAgentRole = role_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;

    Ok(SpecAgentSession {
        id: row.get(0)?,
        spec_id: row.get(1)?,
        role,
        agent_type: row.get(3)?,
        agent_model: row.get(4)?,
        session_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

pub fn create_spec(conn: &Connection, input: &CreateSpecInput) -> Result<Spec, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let title = input
        .title
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| title_from_input(&input.user_input));

    info!(id = %id, title = %title, "Creating spec");

    conn.execute(
        "INSERT INTO specs (
            id, repository_id, title, user_input, status,
            task_ids, agent_type, agent_model, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            id,
            input.repository_id,
            title,
            input.user_input,
            SpecStatus::Idea.as_str(),
            "[]",
            input.agent_type.as_deref(),
            input.agent_model.as_deref(),
            now,
            now,
        ],
    )
    .map_err(AppError::Database)?;

    get_spec_by_id(conn, &id)
}

pub fn get_spec_agent_session(
    conn: &Connection,
    spec_id: &str,
    role: SpecAgentRole,
) -> Result<Option<SpecAgentSession>, AppError> {
    conn.query_row(
        "SELECT id, spec_id, role, agent_type, agent_model, session_id, created_at, updated_at
            FROM spec_agent_sessions
            WHERE spec_id = ?1 AND role = ?2",
        rusqlite::params![spec_id, role.as_str()],
        row_to_spec_agent_session,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(AppError::Database(other)),
    })
}

pub fn upsert_spec_agent_session(
    conn: &Connection,
    spec_id: &str,
    role: SpecAgentRole,
    agent_type: &str,
    agent_model: Option<&str>,
    session_id: Option<&str>,
) -> Result<SpecAgentSession, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO spec_agent_sessions (
            id, spec_id, role, agent_type, agent_model, session_id, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(spec_id, role) DO UPDATE SET
            agent_type = excluded.agent_type,
            agent_model = excluded.agent_model,
            session_id = excluded.session_id,
            updated_at = excluded.updated_at",
        rusqlite::params![
            id,
            spec_id,
            role.as_str(),
            agent_type,
            agent_model,
            session_id,
            now,
            now,
        ],
    )
    .map_err(AppError::Database)?;

    get_spec_agent_session(conn, spec_id, role)?.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "Spec agent session was not found after upsert"
        ))
    })
}

pub fn list_specs(conn: &Connection, repository_id: Option<&str>) -> Result<Vec<Spec>, AppError> {
    let mut sql = format!("SELECT {SPEC_COLUMNS} FROM specs");
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(repo_id) = repository_id {
        sql.push_str(" WHERE repository_id = ?1");
        params.push(Box::new(repo_id.to_string()));
    }

    sql.push_str(" ORDER BY updated_at DESC");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(AppError::Database)?;
    let rows = stmt
        .query_map(param_refs.as_slice(), row_to_spec)
        .map_err(AppError::Database)?;

    let mut specs = Vec::new();
    for row in rows {
        specs.push(row.map_err(AppError::Database)?);
    }

    Ok(specs)
}

pub fn get_spec_by_id(conn: &Connection, id: &str) -> Result<Spec, AppError> {
    let sql = format!("SELECT {SPEC_COLUMNS} FROM specs WHERE id = ?1");
    conn.query_row(&sql, [id], row_to_spec)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Spec not found: {id}"))
            }
            other => AppError::Database(other),
        })
}

pub fn update_spec(conn: &Connection, id: &str, input: &UpdateSpecInput) -> Result<Spec, AppError> {
    let _existing = get_spec_by_id(conn, id)?;
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1usize;

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

    add_field!(title, "title");
    add_field!(user_input, "user_input");
    add_nullable_field!(clarification_answered_at, "clarification_answered_at");
    add_nullable_field!(generated_spec, "generated_spec");
    add_nullable_field!(final_spec, "final_spec");
    add_nullable_field!(spec_approved_at, "spec_approved_at");
    add_nullable_field!(generated_plan, "generated_plan");
    add_nullable_field!(final_plan, "final_plan");
    add_nullable_field!(plan_approved_at, "plan_approved_at");
    add_nullable_field!(generated_tasks, "generated_tasks");
    add_nullable_field!(final_tasks, "final_tasks");
    add_nullable_field!(tasks_approved_at, "tasks_approved_at");
    add_nullable_field!(agent_type, "agent_type");
    add_nullable_field!(agent_model, "agent_model");
    add_nullable_field!(agent_session_id, "agent_session_id");
    add_nullable_field!(error, "error");

    if let Some(ref status) = input.status {
        sets.push(format!("status = ?{}", idx));
        values.push(Box::new(status.as_str().to_string()));
        idx += 1;
    }

    if let Some(ref task_ids) = input.task_ids {
        let json = serde_json::to_string(task_ids).unwrap_or_else(|_| "[]".to_string());
        sets.push(format!("task_ids = ?{}", idx));
        values.push(Box::new(json));
        idx += 1;
    }

    if let Some(ref questions) = input.clarification_questions {
        let json = serde_json::to_string(questions).unwrap_or_else(|_| "[]".to_string());
        sets.push(format!("clarification_questions = ?{}", idx));
        values.push(Box::new(json));
        idx += 1;
    }

    if let Some(ref answers) = input.clarification_answers {
        let json = serde_json::to_string(answers).unwrap_or_else(|_| "[]".to_string());
        sets.push(format!("clarification_answers = ?{}", idx));
        values.push(Box::new(json));
        idx += 1;
    }

    if sets.is_empty() {
        return get_spec_by_id(conn, id);
    }

    let now = chrono::Utc::now().to_rfc3339();
    sets.push(format!("updated_at = ?{}", idx));
    values.push(Box::new(now));
    idx += 1;

    let sql = format!("UPDATE specs SET {} WHERE id = ?{}", sets.join(", "), idx);
    values.push(Box::new(id.to_string()));

    debug!(id = id, sql = %sql, "Updating spec");
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|p| p.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
        .map_err(AppError::Database)?;

    get_spec_by_id(conn, id)
}

pub fn delete_spec(conn: &Connection, id: &str) -> Result<(), AppError> {
    let changes = conn
        .execute("DELETE FROM specs WHERE id = ?1", [id])
        .map_err(AppError::Database)?;
    if changes == 0 {
        return Err(AppError::NotFound(format!("Spec not found: {id}")));
    }
    Ok(())
}

pub fn set_generated_spec(conn: &Connection, id: &str, content: &str) -> Result<Spec, AppError> {
    update_spec(
        conn,
        id,
        &UpdateSpecInput {
            generated_spec: Some(Some(content.to_string())),
            final_spec: Some(Some(content.to_string())),
            status: Some(SpecStatus::SpecDraft),
            error: Some(None),
            ..Default::default()
        },
    )
}

pub fn set_clarification_questions(
    conn: &Connection,
    id: &str,
    questions: Vec<String>,
) -> Result<Spec, AppError> {
    update_spec(
        conn,
        id,
        &UpdateSpecInput {
            clarification_questions: Some(questions),
            clarification_answers: Some(Vec::new()),
            clarification_answered_at: Some(None),
            status: Some(SpecStatus::Clarifying),
            generated_spec: Some(None),
            final_spec: Some(None),
            error: Some(None),
            ..Default::default()
        },
    )
}

pub fn set_clarification_answers(
    conn: &Connection,
    id: &str,
    answers: Vec<String>,
) -> Result<Spec, AppError> {
    update_spec(
        conn,
        id,
        &UpdateSpecInput {
            clarification_answers: Some(answers),
            clarification_answered_at: Some(Some(chrono::Utc::now().to_rfc3339())),
            error: Some(None),
            ..Default::default()
        },
    )
}

pub fn approve_spec_content(
    conn: &Connection,
    id: &str,
    content: Option<&str>,
) -> Result<Spec, AppError> {
    let existing = get_spec_by_id(conn, id)?;
    let final_content = content
        .map(ToString::to_string)
        .or(existing.final_spec)
        .or(existing.generated_spec)
        .ok_or_else(|| AppError::Validation("No spec content to approve".to_string()))?;

    update_spec(
        conn,
        id,
        &UpdateSpecInput {
            final_spec: Some(Some(final_content)),
            spec_approved_at: Some(Some(chrono::Utc::now().to_rfc3339())),
            status: Some(SpecStatus::SpecReview),
            error: Some(None),
            ..Default::default()
        },
    )
}

pub fn set_generated_plan(conn: &Connection, id: &str, content: &str) -> Result<Spec, AppError> {
    update_spec(
        conn,
        id,
        &UpdateSpecInput {
            generated_plan: Some(Some(content.to_string())),
            final_plan: Some(Some(content.to_string())),
            status: Some(SpecStatus::Plan),
            error: Some(None),
            ..Default::default()
        },
    )
}

pub fn approve_plan_content(
    conn: &Connection,
    id: &str,
    content: Option<&str>,
) -> Result<Spec, AppError> {
    let existing = get_spec_by_id(conn, id)?;
    let final_content = content
        .map(ToString::to_string)
        .or(existing.final_plan)
        .or(existing.generated_plan)
        .ok_or_else(|| AppError::Validation("No plan content to approve".to_string()))?;

    update_spec(
        conn,
        id,
        &UpdateSpecInput {
            final_plan: Some(Some(final_content)),
            plan_approved_at: Some(Some(chrono::Utc::now().to_rfc3339())),
            error: Some(None),
            ..Default::default()
        },
    )
}

pub fn set_generated_tasks(conn: &Connection, id: &str, content: &str) -> Result<Spec, AppError> {
    update_spec(
        conn,
        id,
        &UpdateSpecInput {
            generated_tasks: Some(Some(content.to_string())),
            final_tasks: Some(Some(content.to_string())),
            status: Some(SpecStatus::TaskBreakdown),
            error: Some(None),
            ..Default::default()
        },
    )
}

pub fn approve_tasks_content(
    conn: &Connection,
    id: &str,
    content: Option<&str>,
) -> Result<Spec, AppError> {
    let existing = get_spec_by_id(conn, id)?;
    let final_content = content
        .map(ToString::to_string)
        .or(existing.final_tasks)
        .or(existing.generated_tasks)
        .ok_or_else(|| AppError::Validation("No task breakdown to approve".to_string()))?;

    update_spec(
        conn,
        id,
        &UpdateSpecInput {
            final_tasks: Some(Some(final_content)),
            tasks_approved_at: Some(Some(chrono::Utc::now().to_rfc3339())),
            status: Some(SpecStatus::ReadyForImplementation),
            error: Some(None),
            ..Default::default()
        },
    )
}

pub fn mark_implemented(
    conn: &Connection,
    id: &str,
    task_ids: Vec<String>,
) -> Result<Spec, AppError> {
    update_spec(
        conn,
        id,
        &UpdateSpecInput {
            task_ids: Some(task_ids),
            status: Some(SpecStatus::Implemented),
            error: Some(None),
            ..Default::default()
        },
    )
}
