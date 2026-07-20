use rusqlite::Connection;
use tracing::info;

use crate::error::AppError;

struct Migration {
    version: i64,
    description: &'static str,
    sql: &'static str,
}

/// Returns the current schema version from the database.
fn get_current_schema_version(conn: &Connection) -> Result<i64, AppError> {
    let version: i64 = conn
        .query_row(
            "SELECT version FROM schema_versions ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(version)
}

/// Runs all pending database migrations.
/// Matches the exact same schema as the TypeScript server.
pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    info!("Running database migrations");

    // Create schema_versions table if it doesn't exist
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_versions (
            version INTEGER PRIMARY KEY,
            applied_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .map_err(AppError::Database)?;

    let current_version = get_current_schema_version(conn)?;
    info!(version = current_version, "Current schema version");

    let migrations = vec![
        Migration {
            version: 1,
            description: "Create tasks table",
            sql: "
                CREATE TABLE IF NOT EXISTS tasks (
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
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
                );

                CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
                CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON tasks(created_at);
            ",
        },
        Migration {
            version: 2,
            description: "Create task_logs table",
            sql: "
                CREATE TABLE IF NOT EXISTS task_logs (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL,
                    timestamp TEXT DEFAULT CURRENT_TIMESTAMP,
                    level TEXT DEFAULT 'info',
                    message TEXT NOT NULL,
                    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_task_logs_task_id ON task_logs(task_id);
                CREATE INDEX IF NOT EXISTS idx_task_logs_timestamp ON task_logs(timestamp);
            ",
        },
        Migration {
            version: 3,
            description: "Create repositories table",
            sql: "
                CREATE TABLE IF NOT EXISTS repositories (
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

                CREATE INDEX IF NOT EXISTS idx_repositories_name ON repositories(name);
                CREATE INDEX IF NOT EXISTS idx_repositories_url ON repositories(url);
            ",
        },
        Migration {
            version: 4,
            description: "Add two-agent workflow columns to tasks table",
            sql: "
                ALTER TABLE tasks ADD COLUMN repository_id TEXT;
                ALTER TABLE tasks ADD COLUMN user_input TEXT;
                ALTER TABLE tasks ADD COLUMN generated_spec TEXT;
                ALTER TABLE tasks ADD COLUMN generated_spec_at TEXT;
                ALTER TABLE tasks ADD COLUMN final_spec TEXT;
                ALTER TABLE tasks ADD COLUMN spec_approved_at TEXT;
                ALTER TABLE tasks ADD COLUMN was_spec_edited INTEGER DEFAULT 0;
                ALTER TABLE tasks ADD COLUMN branch_name TEXT;
                ALTER TABLE tasks ADD COLUMN pr_number INTEGER;

                CREATE INDEX IF NOT EXISTS idx_tasks_repository_id ON tasks(repository_id);
            ",
        },
        Migration {
            version: 5,
            description: "Create user_secrets table for encrypted API keys and tokens",
            sql: "
                CREATE TABLE IF NOT EXISTS user_secrets (
                    id TEXT PRIMARY KEY,
                    key_type TEXT NOT NULL,
                    provider TEXT,
                    encrypted_value TEXT NOT NULL,
                    metadata TEXT,
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_user_secrets_key_provider ON user_secrets(key_type, provider);
                CREATE INDEX IF NOT EXISTS idx_user_secrets_key_type ON user_secrets(key_type);
            ",
        },
        Migration {
            version: 6,
            description: "Create user_settings table for application configuration",
            sql: "
                CREATE TABLE IF NOT EXISTS user_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
                );
            ",
        },
        Migration {
            version: 7,
            description: "Add agent_type and agent_model columns to tasks table",
            sql: "
                ALTER TABLE tasks ADD COLUMN agent_type TEXT;
                ALTER TABLE tasks ADD COLUMN agent_model TEXT;
            ",
        },
        Migration {
            version: 8,
            description: "Add changes_data column to tasks table for persisted diffs",
            sql: "
                ALTER TABLE tasks ADD COLUMN changes_data TEXT;
            ",
        },
        Migration {
            version: 9,
            description: "Add conflict_files column to tasks table for merge conflict tracking",
            sql: "
                ALTER TABLE tasks ADD COLUMN conflict_files TEXT;
            ",
        },
        Migration {
            version: 10,
            description: "Add base_commit column to tasks table for scoped diffs",
            sql: "
                ALTER TABLE tasks ADD COLUMN base_commit TEXT;
            ",
        },
        Migration {
            version: 11,
            description: "Add event_type and event_data columns to task_logs for SSE persistence",
            sql: "
                ALTER TABLE task_logs ADD COLUMN event_type TEXT DEFAULT 'log';
                ALTER TABLE task_logs ADD COLUMN event_data TEXT;
                CREATE INDEX IF NOT EXISTS idx_task_logs_event_type ON task_logs(event_type);
            ",
        },
        Migration {
            version: 12,
            description: "Create specs table for standalone SDD demo workflow",
            sql: "
                CREATE TABLE IF NOT EXISTS specs (
                    id TEXT PRIMARY KEY,
                    repository_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    user_input TEXT NOT NULL,
                    status TEXT DEFAULT 'idea',

                    generated_spec TEXT,
                    final_spec TEXT,
                    spec_approved_at TEXT,

                    generated_plan TEXT,
                    final_plan TEXT,
                    plan_approved_at TEXT,

                    generated_tasks TEXT,
                    final_tasks TEXT,
                    tasks_approved_at TEXT,
                    task_ids TEXT DEFAULT '[]',

                    agent_type TEXT,
                    agent_model TEXT,
                    error TEXT,

                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_specs_repository_id ON specs(repository_id);
                CREATE INDEX IF NOT EXISTS idx_specs_status ON specs(status);
                CREATE INDEX IF NOT EXISTS idx_specs_updated_at ON specs(updated_at);
            ",
        },
        Migration {
            version: 13,
            description: "Create generic agent_events table for task and spec SSE persistence",
            sql: "
                CREATE TABLE IF NOT EXISTS agent_events (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    timestamp TEXT DEFAULT CURRENT_TIMESTAMP,
                    level TEXT DEFAULT 'info',
                    message TEXT NOT NULL,
                    event_type TEXT DEFAULT 'log',
                    event_data TEXT
                );

                CREATE INDEX IF NOT EXISTS idx_agent_events_run_id ON agent_events(run_id);
                CREATE INDEX IF NOT EXISTS idx_agent_events_timestamp ON agent_events(timestamp);
                CREATE INDEX IF NOT EXISTS idx_agent_events_event_type ON agent_events(event_type);
            ",
        },
        Migration {
            version: 14,
            description: "Add clarification fields to specs",
            sql: "
                ALTER TABLE specs ADD COLUMN clarification_questions TEXT DEFAULT '[]';
                ALTER TABLE specs ADD COLUMN clarification_answers TEXT DEFAULT '[]';
                ALTER TABLE specs ADD COLUMN clarification_answered_at TEXT;
            ",
        },
        Migration {
            version: 15,
            description: "Add agent session id to specs for resumable spec chat",
            sql: "
                ALTER TABLE specs ADD COLUMN agent_session_id TEXT;
            ",
        },
        Migration {
            version: 16,
            description: "Create spec agent role sessions",
            sql: "
                CREATE TABLE IF NOT EXISTS spec_agent_sessions (
                    id TEXT PRIMARY KEY,
                    spec_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    agent_type TEXT NOT NULL,
                    agent_model TEXT,
                    session_id TEXT,
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (spec_id) REFERENCES specs(id) ON DELETE CASCADE,
                    UNIQUE(spec_id, role)
                );

                CREATE INDEX IF NOT EXISTS idx_spec_agent_sessions_spec_id
                    ON spec_agent_sessions(spec_id);
                CREATE INDEX IF NOT EXISTS idx_spec_agent_sessions_role
                    ON spec_agent_sessions(role);

                INSERT OR IGNORE INTO spec_agent_sessions (
                    id, spec_id, role, agent_type, agent_model, session_id, created_at, updated_at
                )
                SELECT
                    id || ':spec_writer',
                    id,
                    'spec_writer',
                    agent_type,
                    agent_model,
                    agent_session_id,
                    created_at,
                    updated_at
                FROM specs
                WHERE agent_session_id IS NOT NULL
                    AND agent_type IN ('claude-code', 'codex');
            ",
        },
        Migration {
            version: 17,
            description: "Create QA runs table",
            sql: "
                CREATE TABLE IF NOT EXISTS qa_runs (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'queued',
                    test_command TEXT NOT NULL,
                    target_url TEXT,
                    report_path TEXT,
                    trace_path TEXT,
                    stdout TEXT,
                    stderr TEXT,
                    exit_code INTEGER,
                    started_at TEXT,
                    completed_at TEXT,
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_qa_runs_task_id ON qa_runs(task_id);
                CREATE INDEX IF NOT EXISTS idx_qa_runs_status ON qa_runs(status);
                CREATE INDEX IF NOT EXISTS idx_qa_runs_created_at ON qa_runs(created_at);
            ",
        },
    ];

    for migration in &migrations {
        if migration.version > current_version {
            info!(
                version = migration.version,
                description = migration.description,
                "Applying migration"
            );

            conn.execute_batch("BEGIN TRANSACTION")
                .map_err(AppError::Database)?;

            match conn.execute_batch(migration.sql) {
                Ok(()) => {
                    conn.execute(
                        "INSERT INTO schema_versions (version) VALUES (?1)",
                        [migration.version],
                    )
                    .map_err(AppError::Database)?;
                    conn.execute_batch("COMMIT").map_err(AppError::Database)?;
                    info!(
                        version = migration.version,
                        "Migration applied successfully"
                    );
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(AppError::Database(e));
                }
            }
        }
    }

    let final_version = get_current_schema_version(conn)?;
    info!(version = final_version, "Database migrations complete");
    Ok(())
}
