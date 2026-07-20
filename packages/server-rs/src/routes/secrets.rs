//! Secrets management routes: AI keys, GitHub tokens, GitLab tokens.
//!
//! Provides CRUD endpoints for encrypted secret storage and validation
//! of personal access tokens against external APIs.
//!
//! ## Endpoints
//!
//! ### AI
//! - `POST /api/secrets/ai` - Save AI API key
//! - `DELETE /api/secrets/ai` - Delete AI API key
//! - `GET /api/secrets/ai/status` - Get AI connection status
//!
//! ### GitHub
//! - `POST /api/secrets/github` - Save GitHub token
//! - `DELETE /api/secrets/github` - Delete GitHub token
//! - `GET /api/secrets/github/status` - Get GitHub connection status
//! - `POST /api/secrets/github/validate-pat` - Validate GitHub PAT
//!
//! ### GitLab
//! - `POST /api/secrets/gitlab` - Save GitLab token
//! - `DELETE /api/secrets/gitlab` - Delete GitLab token
//! - `GET /api/secrets/gitlab/status` - Get GitLab connection status
//! - `POST /api/secrets/gitlab/validate-pat` - Validate GitLab PAT
//!
//! ### Combined
//! - `GET /api/secrets/status` - Combined status of all connections

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

use crate::error::AppError;
use crate::services::secrets_service;
use crate::AppState;

// ============================================================================
// Request/Response Types
// ============================================================================

/// POST /secrets/ai request body.
#[derive(Debug, Deserialize)]
struct SaveAISecretRequest {
    provider: String,
    #[serde(rename = "apiKey")]
    api_key: String,
    model: Option<String>,
}

/// POST /secrets/github request body.
#[derive(Debug, Deserialize)]
struct SaveGitHubSecretRequest {
    token: String,
    #[serde(rename = "connectionMethod")]
    connection_method: String,
    username: Option<String>,
    #[serde(rename = "avatarUrl")]
    avatar_url: Option<String>,
}

/// POST /secrets/github/validate-pat request body.
#[derive(Debug, Deserialize)]
struct ValidateGitHubPATRequest {
    token: String,
}

/// POST /secrets/gitlab request body.
#[derive(Debug, Deserialize)]
struct SaveGitLabSecretRequest {
    token: String,
    username: Option<String>,
    #[serde(rename = "avatarUrl")]
    avatar_url: Option<String>,
}

/// POST /secrets/gitlab/validate-pat request body.
#[derive(Debug, Deserialize)]
struct ValidateGitLabPATRequest {
    token: String,
}

/// AI connection status response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AIStatusResponse {
    connected: bool,
    provider: Option<String>,
    model: Option<String>,
    model_info: Option<ModelInfo>,
}

/// Model info returned in AI status.
#[derive(Debug, Serialize)]
struct ModelInfo {
    name: String,
    description: String,
}

/// GitHub connection status response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GitHubStatusResponse {
    connected: bool,
    username: Option<String>,
    avatar_url: Option<String>,
    connection_method: Option<String>,
}

/// GitLab connection status response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GitLabStatusResponse {
    connected: bool,
    username: Option<String>,
    avatar_url: Option<String>,
}

/// Combined status response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CombinedStatusResponse {
    ai: AIStatusResponse,
    github: GitHubStatusResponse,
    gitlab: GitLabStatusResponse,
    is_complete: bool,
}

// ============================================================================
// Router
// ============================================================================

/// Builds the secrets routes sub-router.
pub fn router() -> Router<AppState> {
    Router::new()
        // AI endpoints
        .route("/ai", post(save_ai_secret))
        .route("/ai", delete(delete_ai_secret))
        .route("/ai/status", get(get_ai_status))
        // GitHub endpoints
        .route("/github", post(save_github_secret))
        .route("/github", delete(delete_github_secret))
        .route("/github/status", get(get_github_status))
        .route("/github/validate-pat", post(validate_github_pat))
        // GitLab endpoints
        .route("/gitlab", post(save_gitlab_secret))
        .route("/gitlab", delete(delete_gitlab_secret))
        .route("/gitlab/status", get(get_gitlab_status))
        .route("/gitlab/validate-pat", post(validate_gitlab_pat))
        // Combined status
        .route("/status", get(get_combined_status))
}

// ============================================================================
// AI Handlers
// ============================================================================

/// POST /api/secrets/ai
///
/// Saves an AI API key. The key is encrypted before storage.
async fn save_ai_secret(
    State(state): State<AppState>,
    Json(body): Json<SaveAISecretRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!(provider = %body.provider, "POST /secrets/ai");

    if body.provider.is_empty() {
        return Err(AppError::Validation("provider is required".to_string()));
    }
    if body.api_key.is_empty() {
        return Err(AppError::Validation("apiKey is required".to_string()));
    }

    let provider = body.provider.clone();
    let api_key = body.api_key.clone();
    let model = body.model.clone();

    // Build metadata JSON
    let metadata = {
        let mut meta = serde_json::Map::new();
        meta.insert("modelName".to_string(), json!(provider));
        meta.insert(
            "modelDescription".to_string(),
            json!(format!("{} API key", provider)),
        );
        if let Some(ref m) = model {
            meta.insert("model".to_string(), json!(m));
        }
        serde_json::to_string(&meta).ok()
    };

    let provider_clone = provider.clone();
    state
        .db
        .call(move |conn| {
            secrets_service::save_secret(
                conn,
                "ai_api_key",
                Some(&provider_clone),
                &api_key,
                metadata.as_deref(),
            )
        })
        .await?;

    info!(provider = %provider, "AI secret saved successfully");

    Ok(Json(json!({
        "success": true,
        "provider": provider,
    })))
}

/// DELETE /api/secrets/ai
///
/// Deletes the stored AI API key.
async fn delete_ai_secret(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("DELETE /secrets/ai");

    state
        .db
        .call(|conn| secrets_service::delete_secret(conn, "ai_api_key", None))
        .await?;

    Ok(Json(json!({
        "success": true,
        "message": "AI API key deleted",
    })))
}

/// GET /api/secrets/ai/status
///
/// Returns AI connection status without exposing the API key.
async fn get_ai_status(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("GET /secrets/ai/status");

    let (has_key, metadata_json) = state
        .db
        .call(|conn| {
            let has = secrets_service::has_secret(conn, "ai_api_key", None)?;
            let meta = if has {
                secrets_service::get_secret_metadata(conn, "ai_api_key", None)?
            } else {
                None
            };
            Ok((has, meta))
        })
        .await?;

    if !has_key {
        return Ok(Json(
            serde_json::to_value(AIStatusResponse {
                connected: false,
                provider: None,
                model: None,
                model_info: None,
            })
            .unwrap(),
        ));
    }

    // Parse metadata to extract provider/model info
    let (provider, model, model_info) = if let Some(ref meta_str) = metadata_json {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
            let provider = meta
                .get("modelName")
                .and_then(|v| v.as_str())
                .map(String::from);
            let model = meta.get("model").and_then(|v| v.as_str()).map(String::from);
            let model_info = provider.as_ref().map(|p| ModelInfo {
                name: p.clone(),
                description: meta
                    .get("modelDescription")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
            (provider, model, model_info)
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    Ok(Json(
        serde_json::to_value(AIStatusResponse {
            connected: true,
            provider,
            model,
            model_info,
        })
        .unwrap(),
    ))
}

// ============================================================================
// GitHub Handlers
// ============================================================================

/// POST /api/secrets/github
///
/// Saves a GitHub token with associated metadata (username, avatar, connection method).
async fn save_github_secret(
    State(state): State<AppState>,
    Json(body): Json<SaveGitHubSecretRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /secrets/github");

    if body.token.is_empty() {
        return Err(AppError::Validation("token is required".to_string()));
    }

    let mut final_username = body.username.clone();
    let mut final_avatar_url = body.avatar_url.clone();

    // If username not provided, validate the token by calling GitHub API
    if final_username.is_none() {
        match fetch_github_user_info(&body.token).await {
            Some(info) => {
                final_username = Some(info.login);
                final_avatar_url = Some(info.avatar_url);
            }
            None => {
                return Err(AppError::Validation(
                    "Invalid GitHub token or failed to fetch user info".to_string(),
                ));
            }
        }
    }

    let token = body.token.clone();
    let connection_method = body.connection_method.clone();
    let username_for_meta = final_username.clone();
    let avatar_for_meta = final_avatar_url.clone();

    // Build metadata
    let metadata = {
        let mut meta = serde_json::Map::new();
        if let Some(ref u) = username_for_meta {
            meta.insert("username".to_string(), json!(u));
        }
        meta.insert("connectionMethod".to_string(), json!(connection_method));
        if let Some(ref a) = avatar_for_meta {
            meta.insert("avatarUrl".to_string(), json!(a));
        }
        serde_json::to_string(&meta).ok()
    };

    state
        .db
        .call(move |conn| {
            secrets_service::save_secret(
                conn,
                "github_token",
                Some("github"),
                &token,
                metadata.as_deref(),
            )
        })
        .await?;

    info!(username = ?final_username, "GitHub secret saved successfully");

    Ok(Json(json!({
        "success": true,
        "username": final_username,
        "avatarUrl": final_avatar_url,
    })))
}

/// DELETE /api/secrets/github
///
/// Deletes the stored GitHub token.
async fn delete_github_secret(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    info!("DELETE /secrets/github");

    state
        .db
        .call(|conn| secrets_service::delete_secret(conn, "github_token", Some("github")))
        .await?;

    Ok(Json(json!({
        "success": true,
        "message": "GitHub token deleted",
    })))
}

/// GET /api/secrets/github/status
///
/// Returns GitHub connection status without exposing the token.
async fn get_github_status(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("GET /secrets/github/status");

    let (has_key, metadata_json) = state
        .db
        .call(|conn| {
            let has = secrets_service::has_secret(conn, "github_token", Some("github"))?;
            let meta = if has {
                secrets_service::get_secret_metadata(conn, "github_token", Some("github"))?
            } else {
                None
            };
            Ok((has, meta))
        })
        .await?;

    if !has_key {
        return Ok(Json(
            serde_json::to_value(GitHubStatusResponse {
                connected: false,
                username: None,
                avatar_url: None,
                connection_method: None,
            })
            .unwrap(),
        ));
    }

    let (username, avatar_url, connection_method) = if let Some(ref meta_str) = metadata_json {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
            (
                meta.get("username")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                meta.get("avatarUrl")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                meta.get("connectionMethod")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            )
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    Ok(Json(
        serde_json::to_value(GitHubStatusResponse {
            connected: true,
            username,
            avatar_url,
            connection_method,
        })
        .unwrap(),
    ))
}

/// POST /api/secrets/github/validate-pat
///
/// Validates a GitHub Personal Access Token by calling the GitHub API.
/// Returns user info without storing the token.
async fn validate_github_pat(
    Json(body): Json<ValidateGitHubPATRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /secrets/github/validate-pat");

    if body.token.is_empty() {
        return Err(AppError::Validation("token is required".to_string()));
    }

    match fetch_github_user_info(&body.token).await {
        Some(info) => Ok(Json(json!({
            "valid": true,
            "username": info.login,
            "avatarUrl": info.avatar_url,
            "scopes": [],
        }))),
        None => Ok(Json(json!({
            "valid": false,
            "error": "Invalid token or unable to authenticate with GitHub",
        }))),
    }
}

// ============================================================================
// GitLab Handlers
// ============================================================================

/// POST /api/secrets/gitlab
///
/// Saves a GitLab token with associated metadata.
async fn save_gitlab_secret(
    State(state): State<AppState>,
    Json(body): Json<SaveGitLabSecretRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /secrets/gitlab");

    if body.token.is_empty() {
        return Err(AppError::Validation("token is required".to_string()));
    }

    let mut final_username = body.username.clone();
    let mut final_avatar_url = body.avatar_url.clone();

    // If username not provided, validate the token by calling GitLab API
    if final_username.is_none() {
        match fetch_gitlab_user_info(&body.token).await {
            Some(info) => {
                final_username = Some(info.username);
                final_avatar_url = Some(info.avatar_url);
            }
            None => {
                return Err(AppError::Validation(
                    "Invalid GitLab token or failed to fetch user info".to_string(),
                ));
            }
        }
    }

    let token = body.token.clone();
    let username_for_meta = final_username.clone();
    let avatar_for_meta = final_avatar_url.clone();

    // Build metadata
    let metadata = {
        let mut meta = serde_json::Map::new();
        if let Some(ref u) = username_for_meta {
            meta.insert("username".to_string(), json!(u));
        }
        if let Some(ref a) = avatar_for_meta {
            meta.insert("avatarUrl".to_string(), json!(a));
        }
        serde_json::to_string(&meta).ok()
    };

    state
        .db
        .call(move |conn| {
            secrets_service::save_secret(
                conn,
                "gitlab_token",
                Some("gitlab"),
                &token,
                metadata.as_deref(),
            )
        })
        .await?;

    info!(username = ?final_username, "GitLab secret saved successfully");

    Ok(Json(json!({
        "success": true,
        "username": final_username,
        "avatarUrl": final_avatar_url,
    })))
}

/// DELETE /api/secrets/gitlab
///
/// Deletes the stored GitLab token.
async fn delete_gitlab_secret(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    info!("DELETE /secrets/gitlab");

    state
        .db
        .call(|conn| secrets_service::delete_secret(conn, "gitlab_token", Some("gitlab")))
        .await?;

    Ok(Json(json!({
        "success": true,
        "message": "GitLab token deleted",
    })))
}

/// GET /api/secrets/gitlab/status
///
/// Returns GitLab connection status without exposing the token.
async fn get_gitlab_status(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("GET /secrets/gitlab/status");

    let (has_key, metadata_json) = state
        .db
        .call(|conn| {
            let has = secrets_service::has_secret(conn, "gitlab_token", Some("gitlab"))?;
            let meta = if has {
                secrets_service::get_secret_metadata(conn, "gitlab_token", Some("gitlab"))?
            } else {
                None
            };
            Ok((has, meta))
        })
        .await?;

    if !has_key {
        return Ok(Json(
            serde_json::to_value(GitLabStatusResponse {
                connected: false,
                username: None,
                avatar_url: None,
            })
            .unwrap(),
        ));
    }

    let (username, avatar_url) = if let Some(ref meta_str) = metadata_json {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
            (
                meta.get("username")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                meta.get("avatarUrl")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            )
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    Ok(Json(
        serde_json::to_value(GitLabStatusResponse {
            connected: true,
            username,
            avatar_url,
        })
        .unwrap(),
    ))
}

/// POST /api/secrets/gitlab/validate-pat
///
/// Validates a GitLab Personal Access Token by calling the GitLab API.
/// Returns user info without storing the token.
async fn validate_gitlab_pat(
    Json(body): Json<ValidateGitLabPATRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /secrets/gitlab/validate-pat");

    if body.token.is_empty() {
        return Err(AppError::Validation("token is required".to_string()));
    }

    match fetch_gitlab_user_info(&body.token).await {
        Some(info) => Ok(Json(json!({
            "valid": true,
            "username": info.username,
            "avatarUrl": info.avatar_url,
        }))),
        None => Ok(Json(json!({
            "valid": false,
            "error": "Invalid token or unable to authenticate with GitLab",
        }))),
    }
}

// ============================================================================
// Combined Status Handler
// ============================================================================

/// GET /api/secrets/status
///
/// Returns the combined status of all secret connections (AI, GitHub, GitLab).
async fn get_combined_status(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("GET /secrets/status");

    let (ai_has, ai_meta, gh_has, gh_meta, gl_has, gl_meta) = state
        .db
        .call(|conn| {
            let ai_has = secrets_service::has_secret(conn, "ai_api_key", None)?;
            let ai_meta = if ai_has {
                secrets_service::get_secret_metadata(conn, "ai_api_key", None)?
            } else {
                None
            };

            let gh_has = secrets_service::has_secret(conn, "github_token", Some("github"))?;
            let gh_meta = if gh_has {
                secrets_service::get_secret_metadata(conn, "github_token", Some("github"))?
            } else {
                None
            };

            let gl_has = secrets_service::has_secret(conn, "gitlab_token", Some("gitlab"))?;
            let gl_meta = if gl_has {
                secrets_service::get_secret_metadata(conn, "gitlab_token", Some("gitlab"))?
            } else {
                None
            };

            Ok((ai_has, ai_meta, gh_has, gh_meta, gl_has, gl_meta))
        })
        .await?;

    // Build AI status
    let ai = {
        let (provider, model, model_info) = if let Some(ref meta_str) = ai_meta {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
                let p = meta
                    .get("modelName")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let m = meta.get("model").and_then(|v| v.as_str()).map(String::from);
                let mi = p.as_ref().map(|name| ModelInfo {
                    name: name.clone(),
                    description: meta
                        .get("modelDescription")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
                (p, m, mi)
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        };

        AIStatusResponse {
            connected: ai_has,
            provider,
            model,
            model_info,
        }
    };

    // Build GitHub status
    let github = {
        let (username, avatar_url, connection_method) = if let Some(ref meta_str) = gh_meta {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
                (
                    meta.get("username")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    meta.get("avatarUrl")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    meta.get("connectionMethod")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                )
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        };

        GitHubStatusResponse {
            connected: gh_has,
            username,
            avatar_url,
            connection_method,
        }
    };

    // Build GitLab status
    let gitlab = {
        let (username, avatar_url) = if let Some(ref meta_str) = gl_meta {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
                (
                    meta.get("username")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    meta.get("avatarUrl")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                )
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        GitLabStatusResponse {
            connected: gl_has,
            username,
            avatar_url,
        }
    };

    let is_complete = ai.connected && (github.connected || gitlab.connected);

    Ok(Json(
        serde_json::to_value(CombinedStatusResponse {
            ai,
            github,
            gitlab,
            is_complete,
        })
        .unwrap(),
    ))
}

// ============================================================================
// HTTP Helper Functions
// ============================================================================

/// GitHub user info returned from the API.
struct GitHubUserInfo {
    login: String,
    avatar_url: String,
}

/// GitLab user info returned from the API.
struct GitLabUserInfo {
    username: String,
    avatar_url: String,
}

/// Fetches GitHub user info using a personal access token.
///
/// Makes an HTTP request to `https://api.github.com/user`.
/// Returns `None` if the token is invalid or the request fails.
async fn fetch_github_user_info(token: &str) -> Option<GitHubUserInfo> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "agent-board")
        .send()
        .await;

    match response {
        Ok(resp) => {
            if !resp.status().is_success() {
                warn!(status = %resp.status(), "GitHub user info fetch failed");
                return None;
            }
            match resp.json::<serde_json::Value>().await {
                Ok(data) => {
                    let login = data.get("login")?.as_str()?.to_string();
                    let avatar_url = data.get("avatar_url")?.as_str()?.to_string();
                    Some(GitHubUserInfo { login, avatar_url })
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse GitHub user info response");
                    None
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "GitHub user info fetch error");
            None
        }
    }
}

/// Fetches GitLab user info using a personal access token.
///
/// Makes an HTTP request to `https://gitlab.com/api/v4/user`.
/// Returns `None` if the token is invalid or the request fails.
async fn fetch_gitlab_user_info(token: &str) -> Option<GitLabUserInfo> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://gitlab.com/api/v4/user")
        .header("PRIVATE-TOKEN", token)
        .send()
        .await;

    match response {
        Ok(resp) => {
            if !resp.status().is_success() {
                warn!(status = %resp.status(), "GitLab user info fetch failed");
                return None;
            }
            match resp.json::<serde_json::Value>().await {
                Ok(data) => {
                    let username = data.get("username")?.as_str()?.to_string();
                    let avatar_url = data.get("avatar_url")?.as_str()?.to_string();
                    Some(GitLabUserInfo {
                        username,
                        avatar_url,
                    })
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse GitLab user info response");
                    None
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "GitLab user info fetch error");
            None
        }
    }
}
