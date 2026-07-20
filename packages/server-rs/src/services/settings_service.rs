//! Settings service for managing application configuration.
//!
//! Uses the `user_settings` table (created in migration 6).

use rusqlite::Connection;
use tracing::info;

use crate::error::AppError;

const CHROME_MCP_ENABLED_KEY: &str = "qa.chrome_mcp.enabled";
const CHROME_MCP_URL_KEY: &str = "qa.chrome_mcp.url";

/// Gets a setting value by key.
/// Returns `None` if the setting does not exist.
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    let mut stmt = conn
        .prepare("SELECT value FROM user_settings WHERE key = ?1")
        .map_err(AppError::Database)?;

    let result = stmt
        .query_row([key], |row| row.get::<_, String>(0))
        .optional()
        .map_err(AppError::Database)?;

    Ok(result)
}

/// Sets a setting value. Creates or replaces the setting (upsert).
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO user_settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![key, value, now],
    )
    .map_err(AppError::Database)?;

    info!(key = key, "Setting updated");
    Ok(())
}

/// Deletes a setting by key.
pub fn delete_setting(conn: &Connection, key: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM user_settings WHERE key = ?1", [key])
        .map_err(AppError::Database)?;

    info!(key = key, "Setting deleted");
    Ok(())
}

/// Gets all settings as a list of `(key, value)` pairs.
pub fn get_all_settings(conn: &Connection) -> Result<Vec<(String, String)>, AppError> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM user_settings")
        .map_err(AppError::Database)?;

    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(AppError::Database)?;

    let mut settings = Vec::new();
    for row in rows {
        settings.push(row.map_err(AppError::Database)?);
    }

    Ok(settings)
}

/// Gets the default agent configuration.
/// Returns `(agent_type, agent_model)` where either value may be `None` if not configured.
pub fn get_default_agent(conn: &Connection) -> Result<(Option<String>, Option<String>), AppError> {
    let agent_type = get_setting(conn, "default_agent_type")?;
    let agent_model = get_setting(conn, "default_agent_model")?;
    Ok((agent_type, agent_model))
}

/// Sets the default agent configuration.
/// `agent_model` is stored only when provided.
pub fn set_default_agent(
    conn: &Connection,
    agent_type: &str,
    agent_model: &str,
) -> Result<(), AppError> {
    set_setting(conn, "default_agent_type", agent_type)?;
    if !agent_model.is_empty() {
        set_setting(conn, "default_agent_model", agent_model)?;
    }
    Ok(())
}

/// Gets the Chrome MCP configuration used by QA runs.
pub fn get_chrome_mcp_config(conn: &Connection) -> Result<(bool, Option<String>), AppError> {
    let enabled = get_setting(conn, CHROME_MCP_ENABLED_KEY)?
        .map(|value| value == "true")
        .unwrap_or(false);
    let url = get_setting(conn, CHROME_MCP_URL_KEY)?;
    Ok((enabled, url))
}

/// Sets the Chrome MCP configuration used by QA runs.
pub fn set_chrome_mcp_config(
    conn: &Connection,
    enabled: bool,
    url: Option<&str>,
) -> Result<(), AppError> {
    set_setting(
        conn,
        CHROME_MCP_ENABLED_KEY,
        if enabled { "true" } else { "false" },
    )?;

    match url {
        Some(value) if !value.is_empty() => set_setting(conn, CHROME_MCP_URL_KEY, value)?,
        _ => delete_setting(conn, CHROME_MCP_URL_KEY)?,
    }

    Ok(())
}

/// Adds the `optional()` helper used by `query_row` to convert "no rows" into `None`.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE user_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_set_and_get_setting() {
        let conn = setup_db();
        set_setting(&conn, "my_key", "my_value").unwrap();
        let val = get_setting(&conn, "my_key").unwrap();
        assert_eq!(val, Some("my_value".to_string()));
    }

    #[test]
    fn test_get_missing_setting() {
        let conn = setup_db();
        let val = get_setting(&conn, "nonexistent").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn test_upsert_setting() {
        let conn = setup_db();
        set_setting(&conn, "key", "value1").unwrap();
        set_setting(&conn, "key", "value2").unwrap();
        let val = get_setting(&conn, "key").unwrap();
        assert_eq!(val, Some("value2".to_string()));
    }

    #[test]
    fn test_delete_setting() {
        let conn = setup_db();
        set_setting(&conn, "key", "value").unwrap();
        delete_setting(&conn, "key").unwrap();
        let val = get_setting(&conn, "key").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn test_get_all_settings() {
        let conn = setup_db();
        set_setting(&conn, "a", "1").unwrap();
        set_setting(&conn, "b", "2").unwrap();
        let all = get_all_settings(&conn).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_default_agent() {
        let conn = setup_db();
        let (at, am) = get_default_agent(&conn).unwrap();
        assert_eq!(at, None);
        assert_eq!(am, None);

        set_default_agent(&conn, "claude-code", "opus").unwrap();
        let (at, am) = get_default_agent(&conn).unwrap();
        assert_eq!(at, Some("claude-code".to_string()));
        assert_eq!(am, Some("opus".to_string()));
    }

    #[test]
    fn test_chrome_mcp_config_defaults_to_disabled() {
        let conn = setup_db();
        let (enabled, url) = get_chrome_mcp_config(&conn).unwrap();

        assert!(!enabled);
        assert_eq!(url, None);
    }

    #[test]
    fn test_chrome_mcp_config_can_be_updated() {
        let conn = setup_db();
        set_chrome_mcp_config(&conn, true, Some("http://localhost:9222/mcp")).unwrap();

        let (enabled, url) = get_chrome_mcp_config(&conn).unwrap();
        assert!(enabled);
        assert_eq!(url, Some("http://localhost:9222/mcp".to_string()));

        set_chrome_mcp_config(&conn, false, None).unwrap();
        let (enabled, url) = get_chrome_mcp_config(&conn).unwrap();
        assert!(!enabled);
        assert_eq!(url, None);
    }
}
