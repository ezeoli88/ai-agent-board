use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_path: PathBuf,
    pub repos_base_dir: PathBuf,
    pub worktrees_dir: PathBuf,
    pub log_level: String,
    pub auth_enabled: bool,
    pub is_binary_mode: bool,
}

/// Returns the OS-specific data directory for the application.
fn get_default_data_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            if !local_app_data.trim().is_empty() {
                return PathBuf::from(local_app_data).join("agent-board");
            }
        }
        dirs_fallback_windows()
    }

    #[cfg(target_os = "macos")]
    {
        let home = env::var("HOME").unwrap_or_else(|_| "~".into());
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("agent-board")
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Ok(xdg) = env::var("XDG_DATA_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join("agent-board");
            }
        }
        let home = env::var("HOME").unwrap_or_else(|_| "~".into());
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("agent-board")
    }
}

#[cfg(target_os = "windows")]
fn dirs_fallback_windows() -> PathBuf {
    let home = env::var("USERPROFILE")
        .unwrap_or_else(|_| env::var("HOME").unwrap_or_else(|_| "C:\\Users\\Default".into()));
    PathBuf::from(home)
        .join("AppData")
        .join("Local")
        .join("agent-board")
}

/// Searches legacy binary directories for backwards compatibility.
fn find_legacy_binary_dir(dir_name: &str) -> Option<PathBuf> {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let cwd = env::current_dir().ok();

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(ref cwd) = cwd {
        candidates.push(cwd.join(dir_name));
        candidates.push(cwd.join("data").join(dir_name));
    }
    if let Some(ref exe) = exe_dir {
        candidates.push(exe.join(dir_name));
        candidates.push(exe.join("data").join(dir_name));
    }

    candidates.into_iter().find(|p| p.exists())
}

/// Searches legacy binary database paths for backwards compatibility.
fn find_legacy_database_path() -> Option<PathBuf> {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let cwd = env::current_dir().ok();

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(ref cwd) = cwd {
        candidates.push(cwd.join("data").join("agent-board.db"));
        candidates.push(cwd.join("agent-board.db"));
    }
    if let Some(ref exe) = exe_dir {
        candidates.push(exe.join("data").join("agent-board.db"));
        candidates.push(exe.join("agent-board.db"));
    }

    candidates.into_iter().find(|p| p.exists())
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

impl Config {
    /// Loads configuration from environment variables, matching the TS server behavior.
    pub fn load(port_override: Option<u16>, database_path_override: Option<&str>) -> Self {
        let is_binary_mode = env::var("__BIN_MODE__").unwrap_or_default() == "1";
        let configured_data_root = env::var("AGENT_BOARD_DATA_DIR").ok();
        let has_configured = configured_data_root
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let data_root = if has_configured {
            PathBuf::from(configured_data_root.as_ref().unwrap())
        } else {
            get_default_data_root()
        };

        let log_level = env_or("LOG_LEVEL", "info");

        let (default_repos, default_worktrees, default_db) = if is_binary_mode {
            let repos = if !has_configured {
                find_legacy_binary_dir("repos")
            } else {
                None
            }
            .unwrap_or_else(|| data_root.join("repos"));

            let worktrees = if !has_configured {
                find_legacy_binary_dir("worktrees")
            } else {
                None
            }
            .unwrap_or_else(|| data_root.join("worktrees"));

            let db = if !has_configured {
                find_legacy_database_path()
            } else {
                None
            }
            .unwrap_or_else(|| data_root.join("data").join("agent-board.db"));

            (repos, worktrees, db)
        } else {
            // Dev mode: use OS-aware temp/data paths
            let dev_data = data_root.join("data");
            (
                data_root.join("repos"),
                data_root.join("worktrees"),
                dev_data.join("agent-board.db"),
            )
        };

        let repos_base_dir = PathBuf::from(env_or(
            "REPOS_BASE_DIR",
            default_repos.to_str().unwrap_or(""),
        ));
        let worktrees_dir = PathBuf::from(env_or(
            "WORKTREES_DIR",
            default_worktrees.to_str().unwrap_or(""),
        ));
        let database_path = if let Some(override_path) = database_path_override {
            PathBuf::from(override_path)
        } else {
            PathBuf::from(env_or("DATABASE_PATH", default_db.to_str().unwrap_or("")))
        };
        let port_env: u16 = env_or("PORT", "51767").parse().unwrap_or(51767);
        let port = port_override.unwrap_or(port_env);

        let auth_enabled = if is_binary_mode {
            env::var("AUTH_DISABLED").unwrap_or_default() != "1"
        } else {
            env::var("AUTH_ENABLED").unwrap_or_default() == "1"
        };

        Config {
            port,
            database_path,
            repos_base_dir,
            worktrees_dir,
            log_level,
            auth_enabled,
            is_binary_mode,
        }
    }

    /// Returns the directory containing the database file, creating it if needed.
    pub fn ensure_database_dir(&self) -> std::io::Result<()> {
        if let Some(parent) = self.database_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        Ok(())
    }
}
