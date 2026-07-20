//! Secrets service for managing encrypted API keys and tokens.
//!
//! Uses the `user_secrets` table (created in migration 5).
//! Values are encrypted at rest using AES-256-GCM via [`encryption_service`].

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use crate::error::AppError;
use crate::services::encryption_service;

// ============================================================================
// Types
// ============================================================================

/// Credentials returned by [`get_ai_credentials`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AICredentials {
    pub api_key: String,
    pub provider: String,
    pub model: Option<String>,
}

/// Status summary returned by [`get_all_secrets_status`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsStatus {
    pub ai_configured: bool,
    pub github_configured: bool,
    pub gitlab_configured: bool,
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// Saves a secret to the database.
///
/// If a secret with the same `key_type` and `provider` already exists, it is updated.
/// The `value` is encrypted before storage.
///
/// - `key_type`: e.g. `"ai_api_key"`, `"github_token"`, `"gitlab_token"`
/// - `provider`: e.g. `"claude"`, `"openai"`, `"github"`, or `None`
/// - `value`: plaintext secret value (will be encrypted)
/// - `metadata`: optional JSON metadata string
pub fn save_secret(
    conn: &Connection,
    key_type: &str,
    provider: Option<&str>,
    value: &str,
    metadata: Option<&str>,
) -> Result<(), AppError> {
    info!(key_type = key_type, provider = provider, "Saving secret");

    let encrypted_value = encryption_service::encrypt(value)?;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if a secret with this key_type + provider already exists
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM user_secrets WHERE key_type = ?1 AND (provider = ?2 OR (provider IS NULL AND ?2 IS NULL))",
            rusqlite::params![key_type, provider],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::Database)?;

    if let Some(id) = existing_id {
        debug!(id = %id, "Updating existing secret");
        conn.execute(
            "UPDATE user_secrets SET encrypted_value = ?1, metadata = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![encrypted_value, metadata, now, id],
        )
        .map_err(AppError::Database)?;
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        debug!(id = %id, "Inserting new secret");
        conn.execute(
            "INSERT INTO user_secrets (id, key_type, provider, encrypted_value, metadata, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, key_type, provider, encrypted_value, metadata, now, now],
        )
        .map_err(AppError::Database)?;
    }

    info!(
        key_type = key_type,
        provider = provider,
        "Secret saved successfully"
    );
    Ok(())
}

/// Gets a secret from the database and decrypts it.
///
/// Returns the decrypted plaintext value, or `None` if not found.
///
/// - If `provider` is `Some`, filters by that provider (including `Some(None)` for NULL provider).
/// - If `provider` is `None`, returns the first secret matching `key_type`.
pub fn get_secret(
    conn: &Connection,
    key_type: &str,
    provider: Option<&str>,
) -> Result<Option<String>, AppError> {
    debug!(key_type = key_type, provider = provider, "Getting secret");

    let encrypted_value: Option<String> = if let Some(prov) = provider {
        conn.query_row(
            "SELECT encrypted_value FROM user_secrets WHERE key_type = ?1 AND (provider = ?2 OR (provider IS NULL AND ?2 IS NULL))",
            rusqlite::params![key_type, prov],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::Database)?
    } else {
        conn.query_row(
            "SELECT encrypted_value FROM user_secrets WHERE key_type = ?1 LIMIT 1",
            [key_type],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::Database)?
    };

    match encrypted_value {
        Some(enc) => match encryption_service::decrypt(&enc) {
            Ok(decrypted) => Ok(Some(decrypted)),
            Err(e) => {
                error!(
                    key_type = key_type,
                    provider = provider,
                    error = %e,
                    "Failed to decrypt secret — token was saved with a different encryption key. \
                     Re-save it in Settings > Connections."
                );
                Err(AppError::Internal(anyhow::anyhow!(
                    "Cannot decrypt {key_type} token — it was saved with a different encryption key. \
                     Please re-save it in Settings > Connections."
                )))
            }
        },
        None => {
            debug!(key_type = key_type, provider = provider, "Secret not found");
            Ok(None)
        }
    }
}

/// Deletes a secret from the database.
///
/// Returns `Ok(())` always. Use [`has_secret`] first if you need to check existence.
pub fn delete_secret(
    conn: &Connection,
    key_type: &str,
    provider: Option<&str>,
) -> Result<(), AppError> {
    info!(key_type = key_type, provider = provider, "Deleting secret");

    if let Some(prov) = provider {
        conn.execute(
            "DELETE FROM user_secrets WHERE key_type = ?1 AND (provider = ?2 OR (provider IS NULL AND ?2 IS NULL))",
            rusqlite::params![key_type, prov],
        )
        .map_err(AppError::Database)?;
    } else {
        conn.execute("DELETE FROM user_secrets WHERE key_type = ?1", [key_type])
            .map_err(AppError::Database)?;
    }

    info!(key_type = key_type, provider = provider, "Secret deleted");
    Ok(())
}

/// Checks if a secret exists in the database.
pub fn has_secret(
    conn: &Connection,
    key_type: &str,
    provider: Option<&str>,
) -> Result<bool, AppError> {
    let exists: bool = if let Some(prov) = provider {
        conn.query_row(
            "SELECT 1 FROM user_secrets WHERE key_type = ?1 AND (provider = ?2 OR (provider IS NULL AND ?2 IS NULL)) LIMIT 1",
            rusqlite::params![key_type, prov],
            |_| Ok(true),
        )
        .optional()
        .map_err(AppError::Database)?
        .unwrap_or(false)
    } else {
        conn.query_row(
            "SELECT 1 FROM user_secrets WHERE key_type = ?1 LIMIT 1",
            [key_type],
            |_| Ok(true),
        )
        .optional()
        .map_err(AppError::Database)?
        .unwrap_or(false)
    };

    Ok(exists)
}

/// Gets the metadata for a secret WITHOUT decrypting the value.
/// Safe to expose to frontend.
///
/// Returns the raw metadata JSON string, or `None` if the secret is not found.
pub fn get_secret_metadata(
    conn: &Connection,
    key_type: &str,
    provider: Option<&str>,
) -> Result<Option<String>, AppError> {
    let metadata: Option<Option<String>> = if let Some(prov) = provider {
        conn.query_row(
            "SELECT metadata FROM user_secrets WHERE key_type = ?1 AND (provider = ?2 OR (provider IS NULL AND ?2 IS NULL))",
            rusqlite::params![key_type, prov],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::Database)?
    } else {
        conn.query_row(
            "SELECT metadata FROM user_secrets WHERE key_type = ?1 LIMIT 1",
            [key_type],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::Database)?
    };

    // Flatten Option<Option<String>> -> Option<String>
    Ok(metadata.flatten())
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Gets the stored AI API key credentials.
///
/// Returns the decrypted API key along with provider and model info from metadata.
/// Returns `None` if no AI secret is configured.
pub fn get_ai_credentials(conn: &Connection) -> Result<Option<AICredentials>, AppError> {
    // Get the encrypted value row with provider and metadata
    let row: Option<(String, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT encrypted_value, provider, metadata FROM user_secrets WHERE key_type = 'ai_api_key' LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(AppError::Database)?;

    let Some((encrypted_value, provider, metadata_json)) = row else {
        return Ok(None);
    };

    let api_key = match encryption_service::decrypt(&encrypted_value) {
        Ok(key) => key,
        Err(e) => {
            error!(error = %e, "Failed to decrypt AI API key");
            return Ok(None);
        }
    };

    let model = metadata_json.and_then(|json| {
        serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(String::from))
    });

    Ok(Some(AICredentials {
        api_key,
        provider: provider.unwrap_or_default(),
        model,
    }))
}

/// Gets the stored GitHub token (decrypted).
/// Returns `None` if not configured.
pub fn get_github_credentials(conn: &Connection) -> Result<Option<String>, AppError> {
    get_secret(conn, "github_token", Some("github"))
}

/// Gets the stored GitLab token (decrypted).
/// Returns `None` if not configured.
pub fn get_gitlab_credentials(conn: &Connection) -> Result<Option<String>, AppError> {
    get_secret(conn, "gitlab_token", Some("gitlab"))
}

/// Gets the status of all secrets (whether each type is configured).
pub fn get_all_secrets_status(conn: &Connection) -> Result<SecretsStatus, AppError> {
    Ok(SecretsStatus {
        ai_configured: has_secret(conn, "ai_api_key", None)?,
        github_configured: has_secret(conn, "github_token", Some("github"))?,
        gitlab_configured: has_secret(conn, "gitlab_token", Some("gitlab"))?,
    })
}

/// Helper trait to convert `QueryReturnedNoRows` into `None`.
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
            "CREATE TABLE user_secrets (
                id TEXT PRIMARY KEY,
                key_type TEXT NOT NULL,
                provider TEXT,
                encrypted_value TEXT NOT NULL,
                metadata TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            CREATE UNIQUE INDEX idx_user_secrets_key_provider ON user_secrets(key_type, provider);
            CREATE INDEX idx_user_secrets_key_type ON user_secrets(key_type);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_save_and_get_secret() {
        let conn = setup_db();
        save_secret(&conn, "ai_api_key", Some("claude"), "sk-test-123", None).unwrap();

        let val = get_secret(&conn, "ai_api_key", Some("claude")).unwrap();
        assert_eq!(val, Some("sk-test-123".to_string()));
    }

    #[test]
    fn test_get_nonexistent_secret() {
        let conn = setup_db();
        let val = get_secret(&conn, "ai_api_key", None).unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn test_upsert_secret() {
        let conn = setup_db();
        save_secret(&conn, "ai_api_key", Some("claude"), "key1", None).unwrap();
        save_secret(&conn, "ai_api_key", Some("claude"), "key2", None).unwrap();

        let val = get_secret(&conn, "ai_api_key", Some("claude")).unwrap();
        assert_eq!(val, Some("key2".to_string()));

        // Verify only one row exists
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM user_secrets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_has_secret() {
        let conn = setup_db();
        assert!(!has_secret(&conn, "ai_api_key", None).unwrap());

        save_secret(&conn, "ai_api_key", Some("claude"), "sk-123", None).unwrap();
        assert!(has_secret(&conn, "ai_api_key", None).unwrap());
        assert!(has_secret(&conn, "ai_api_key", Some("claude")).unwrap());
        assert!(!has_secret(&conn, "github_token", Some("github")).unwrap());
    }

    #[test]
    fn test_delete_secret() {
        let conn = setup_db();
        save_secret(&conn, "ai_api_key", Some("claude"), "sk-123", None).unwrap();
        assert!(has_secret(&conn, "ai_api_key", Some("claude")).unwrap());

        delete_secret(&conn, "ai_api_key", Some("claude")).unwrap();
        assert!(!has_secret(&conn, "ai_api_key", Some("claude")).unwrap());
    }

    #[test]
    fn test_get_secret_metadata() {
        let conn = setup_db();
        let meta = r#"{"model":"opus","modelName":"Claude Opus"}"#;
        save_secret(&conn, "ai_api_key", Some("claude"), "sk-123", Some(meta)).unwrap();

        let result = get_secret_metadata(&conn, "ai_api_key", Some("claude")).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("opus"));
    }

    #[test]
    fn test_get_all_secrets_status() {
        let conn = setup_db();
        let status = get_all_secrets_status(&conn).unwrap();
        assert!(!status.ai_configured);
        assert!(!status.github_configured);
        assert!(!status.gitlab_configured);

        save_secret(&conn, "ai_api_key", Some("claude"), "sk-123", None).unwrap();
        let status = get_all_secrets_status(&conn).unwrap();
        assert!(status.ai_configured);
        assert!(!status.github_configured);
    }

    #[test]
    fn test_get_ai_credentials() {
        let conn = setup_db();

        // No credentials
        let creds = get_ai_credentials(&conn).unwrap();
        assert!(creds.is_none());

        // Save credentials with metadata
        let meta = r#"{"model":"opus-4"}"#;
        save_secret(
            &conn,
            "ai_api_key",
            Some("claude"),
            "sk-ant-123",
            Some(meta),
        )
        .unwrap();

        let creds = get_ai_credentials(&conn).unwrap();
        assert!(creds.is_some());
        let creds = creds.unwrap();
        assert_eq!(creds.api_key, "sk-ant-123");
        assert_eq!(creds.provider, "claude");
        assert_eq!(creds.model, Some("opus-4".to_string()));
    }

    #[test]
    fn test_get_github_credentials() {
        let conn = setup_db();
        assert!(get_github_credentials(&conn).unwrap().is_none());

        save_secret(&conn, "github_token", Some("github"), "ghp_test123", None).unwrap();
        let token = get_github_credentials(&conn).unwrap();
        assert_eq!(token, Some("ghp_test123".to_string()));
    }

    #[test]
    fn test_get_gitlab_credentials() {
        let conn = setup_db();
        assert!(get_gitlab_credentials(&conn).unwrap().is_none());

        save_secret(&conn, "gitlab_token", Some("gitlab"), "glpat-test123", None).unwrap();
        let token = get_gitlab_credentials(&conn).unwrap();
        assert_eq!(token, Some("glpat-test123".to_string()));
    }
}
