//! Service layer for task QA runs.

use rusqlite::{Connection, Row};

use crate::error::AppError;
use crate::models::qa::{QaRun, QaRunStatus, UpdateQaRunInput};

const QA_RUN_COLUMNS: &str = "\
    id, task_id, status, test_command, target_url, report_path, trace_path, \
    stdout, stderr, exit_code, started_at, completed_at, created_at, updated_at";

fn row_to_qa_run(row: &Row) -> Result<QaRun, rusqlite::Error> {
    let status_str: String = row.get(2)?;
    let status: QaRunStatus = status_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;

    Ok(QaRun {
        id: row.get(0)?,
        task_id: row.get(1)?,
        status,
        test_command: row.get(3)?,
        target_url: row.get(4)?,
        report_path: row.get(5)?,
        trace_path: row.get(6)?,
        stdout: row.get(7)?,
        stderr: row.get(8)?,
        exit_code: row.get(9)?,
        started_at: row.get(10)?,
        completed_at: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

pub fn list_qa_runs_for_task(conn: &Connection, task_id: &str) -> Result<Vec<QaRun>, AppError> {
    let sql =
        format!("SELECT {QA_RUN_COLUMNS} FROM qa_runs WHERE task_id = ?1 ORDER BY created_at DESC");
    let mut stmt = conn.prepare(&sql).map_err(AppError::Database)?;
    let rows = stmt
        .query_map([task_id], row_to_qa_run)
        .map_err(AppError::Database)?;

    let mut runs = Vec::new();
    for row in rows {
        runs.push(row.map_err(AppError::Database)?);
    }
    Ok(runs)
}

pub fn get_qa_run_by_id(conn: &Connection, id: &str) -> Result<QaRun, AppError> {
    let sql = format!("SELECT {QA_RUN_COLUMNS} FROM qa_runs WHERE id = ?1");
    conn.query_row(&sql, [id], row_to_qa_run)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("QA run not found: {id}"))
            }
            other => AppError::Database(other),
        })
}

pub fn create_qa_run(
    conn: &Connection,
    task_id: &str,
    test_command: &str,
    target_url: Option<&str>,
) -> Result<QaRun, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO qa_runs (
            id, task_id, status, test_command, target_url, started_at, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            id,
            task_id,
            QaRunStatus::Running.as_str(),
            test_command,
            target_url,
            now,
            now,
            now,
        ],
    )
    .map_err(AppError::Database)?;

    get_qa_run_by_id(conn, &id)
}

pub fn update_qa_run(
    conn: &Connection,
    id: &str,
    input: &UpdateQaRunInput,
) -> Result<QaRun, AppError> {
    let _existing = get_qa_run_by_id(conn, id)?;
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1usize;

    macro_rules! add_nullable_field {
        ($field:ident, $col:literal) => {
            if let Some(ref value) = input.$field {
                sets.push(format!("{} = ?{}", $col, idx));
                values.push(Box::new(value.clone()));
                idx += 1;
            }
        };
    }

    if let Some(ref status) = input.status {
        sets.push(format!("status = ?{}", idx));
        values.push(Box::new(status.as_str().to_string()));
        idx += 1;
    }

    add_nullable_field!(report_path, "report_path");
    add_nullable_field!(trace_path, "trace_path");
    add_nullable_field!(stdout, "stdout");
    add_nullable_field!(stderr, "stderr");
    add_nullable_field!(exit_code, "exit_code");
    add_nullable_field!(started_at, "started_at");
    add_nullable_field!(completed_at, "completed_at");

    if sets.is_empty() {
        return get_qa_run_by_id(conn, id);
    }

    let now = chrono::Utc::now().to_rfc3339();
    sets.push(format!("updated_at = ?{}", idx));
    values.push(Box::new(now));
    idx += 1;

    let sql = format!("UPDATE qa_runs SET {} WHERE id = ?{}", sets.join(", "), idx);
    values.push(Box::new(id.to_string()));
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
        .map_err(AppError::Database)?;

    get_qa_run_by_id(conn, id)
}
