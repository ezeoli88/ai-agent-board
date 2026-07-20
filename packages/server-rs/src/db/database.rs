use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tracing::{debug, info};

use crate::error::AppError;

/// Thread-safe wrapper around a rusqlite Connection.
///
/// rusqlite's Connection is `!Send`, so we wrap it in a `Mutex` and access it
/// via `tokio::task::spawn_blocking` to avoid holding it across `.await` points.
#[derive(Clone)]
pub struct Database {
    inner: Arc<Mutex<Connection>>,
}

impl Database {
    /// Opens (or creates) a SQLite database at the given path.
    pub fn open(path: &Path) -> Result<Self, AppError> {
        info!("Opening database at {}", path.display());

        let conn = Connection::open(path).map_err(AppError::Database)?;

        // Enable foreign keys and WAL mode for better concurrent read performance
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;",
        )
        .map_err(AppError::Database)?;

        info!("Database opened successfully");
        Ok(Database {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// Executes a closure with access to the database connection, running it on
    /// a blocking thread to avoid blocking the Tokio runtime.
    pub async fn call<F, R>(&self, f: F) -> Result<R, AppError>
    where
        F: FnOnce(&Connection) -> Result<R, AppError> + Send + 'static,
        R: Send + 'static,
    {
        let db = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let conn = db
                .lock()
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Database lock poisoned: {e}")))?;
            f(&conn)
        })
        .await?
    }

    /// Executes a closure within a SQLite transaction.
    pub async fn with_transaction<F, R>(&self, f: F) -> Result<R, AppError>
    where
        F: FnOnce(&Connection) -> Result<R, AppError> + Send + 'static,
        R: Send + 'static,
    {
        let db = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let conn = db
                .lock()
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Database lock poisoned: {e}")))?;
            conn.execute_batch("BEGIN TRANSACTION")
                .map_err(AppError::Database)?;
            match f(&conn) {
                Ok(result) => {
                    conn.execute_batch("COMMIT").map_err(AppError::Database)?;
                    debug!("Transaction committed");
                    Ok(result)
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })
        .await?
    }
}
