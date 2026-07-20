//! Setup and configuration routes.
//!
//! Provides endpoints for initial application setup: agent detection, settings,
//! AI key validation, GitHub OAuth, and MCP configuration.
//!
//! ## Endpoints
//!
//! - `GET /api/setup/agents` - Detect installed coding CLI agents (stub)
//! - `GET /api/setup/settings` - Get application settings
//! - `PATCH /api/setup/settings` - Update application settings
//! - `POST /api/setup/validate-ai-key` - Validate an AI provider API key (stub)
//! - `POST /api/setup/validate-openrouter-key` - Validate an OpenRouter key (stub)
//! - `GET /api/setup/openrouter-models` - Get available OpenRouter models (stub)
//! - `GET /api/setup/github/auth` - Get GitHub OAuth authorization URL (stub)
//! - `POST /api/setup/github/callback` - Handle GitHub OAuth callback (stub)
//! - `GET /api/setup/status` - Get current setup status
//! - `DELETE /api/setup/ai-provider` - Disconnect AI provider
//! - `DELETE /api/setup/github` - Disconnect GitHub
//! - `GET /api/setup/mcp-config` - Get MCP server configuration
//! - `GET /api/setup/chrome-mcp-config` - Get Chrome MCP client configuration
//! - `PATCH /api/setup/chrome-mcp-config` - Update Chrome MCP client configuration

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

use crate::error::AppError;
use crate::services::settings_service;
use crate::AppState;

const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

// ============================================================================
// Request/Response Types
// ============================================================================

/// PATCH /setup/settings request body.
#[derive(Debug, Deserialize)]
struct UpdateSettingsRequest {
    default_agent_type: Option<serde_json::Value>,
    default_agent_model: Option<serde_json::Value>,
}

/// GET /setup/chrome-mcp-config response body.
#[derive(Debug, Clone, Serialize)]
struct ChromeMcpConfigResponse {
    enabled: bool,
    url: Option<String>,
}

/// PATCH /setup/chrome-mcp-config request body.
#[derive(Debug, Deserialize)]
struct UpdateChromeMcpConfigRequest {
    enabled: bool,
    url: Option<String>,
}

/// Valid agent types.
const VALID_AGENT_TYPES: &[&str] = &[
    "claude-code",
    "codex",
    "gemini",
    "copilot",
    "openrouter",
    "minimax",
];

/// POST /setup/validate-ai-key request body.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ValidateAIKeyRequest {
    provider: Option<String>,
    #[serde(rename = "apiKey")]
    api_key: Option<String>,
}

/// POST /setup/validate-openrouter-key request body.
#[derive(Debug, Deserialize)]
struct ValidateOpenRouterKeyRequest {
    #[serde(rename = "apiKey")]
    api_key: String,
}

/// POST /setup/validate-minimax-key request body.
#[derive(Debug, Deserialize)]
struct ValidateMiniMaxKeyRequest {
    #[serde(rename = "apiKey")]
    api_key: String,
}

// ============================================================================
// Router
// ============================================================================

/// Builds the setup routes sub-router.
pub fn router() -> Router<AppState> {
    Router::new()
        // Agent detection
        .route("/agents", get(get_agents))
        // Settings
        .route("/settings", get(get_settings))
        .route("/settings", patch(update_settings))
        // AI key validation
        .route("/validate-ai-key", post(validate_ai_key))
        .route("/validate-openrouter-key", post(validate_openrouter_key))
        .route("/validate-minimax-key", post(validate_minimax_key))
        .route("/openrouter-models", get(get_openrouter_models))
        // GitHub OAuth
        .route("/github/auth", get(get_github_auth))
        .route("/github/callback", post(github_callback))
        // Status
        .route("/status", get(get_status))
        // Disconnect
        .route("/ai-provider", delete(delete_ai_provider))
        .route("/github", delete(delete_github))
        // MCP config
        .route("/mcp-config", get(get_mcp_config))
        .route(
            "/chrome-mcp-config",
            get(get_chrome_mcp_config).patch(update_chrome_mcp_config),
        )
}

// ============================================================================
// Agent Detection Handlers
// ============================================================================

/// GET /api/setup/agents
///
/// Returns detected installed coding CLI agents plus API-based agents (OpenRouter).
async fn get_agents(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("GET /setup/agents");

    let agents =
        crate::services::agent_detection_service::detect_installed_agents(Some(&state.db)).await;

    info!(count = agents.len(), "Agents detected");
    Ok(Json(json!({ "agents": agents })))
}

// ============================================================================
// Settings Handlers
// ============================================================================

/// GET /api/setup/settings
///
/// Returns the current application settings (default agent type and model).
async fn get_settings(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("GET /setup/settings");

    let (agent_type, agent_model) = state
        .db
        .call(|conn| settings_service::get_default_agent(conn))
        .await?;

    Ok(Json(json!({
        "default_agent_type": agent_type,
        "default_agent_model": agent_model,
    })))
}

/// PATCH /api/setup/settings
///
/// Updates application settings. Supports partial updates: only provided
/// fields are modified. Sending `null` for a field deletes it.
async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<UpdateSettingsRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("PATCH /setup/settings");

    // Process default_agent_type
    if let Some(ref val) = body.default_agent_type {
        if val.is_null() {
            state
                .db
                .call(|conn| settings_service::delete_setting(conn, "default_agent_type"))
                .await?;
        } else if let Some(s) = val.as_str() {
            if !VALID_AGENT_TYPES.contains(&s) {
                return Err(AppError::Validation(format!(
                    "Invalid agent type: '{}'. Valid types: {:?}",
                    s, VALID_AGENT_TYPES
                )));
            }
            let agent_type = s.to_string();
            state
                .db
                .call(move |conn| {
                    settings_service::set_setting(conn, "default_agent_type", &agent_type)
                })
                .await?;
        } else {
            return Err(AppError::Validation(
                "default_agent_type must be a string or null".to_string(),
            ));
        }
    }

    // Process default_agent_model
    if let Some(ref val) = body.default_agent_model {
        if val.is_null() {
            state
                .db
                .call(|conn| settings_service::delete_setting(conn, "default_agent_model"))
                .await?;
        } else if let Some(s) = val.as_str() {
            let agent_model = s.to_string();
            state
                .db
                .call(move |conn| {
                    settings_service::set_setting(conn, "default_agent_model", &agent_model)
                })
                .await?;
        } else {
            return Err(AppError::Validation(
                "default_agent_model must be a string or null".to_string(),
            ));
        }
    }

    // Return current settings
    let (agent_type, agent_model) = state
        .db
        .call(|conn| settings_service::get_default_agent(conn))
        .await?;

    Ok(Json(json!({
        "success": true,
        "settings": {
            "default_agent_type": agent_type,
            "default_agent_model": agent_model,
        }
    })))
}

// ============================================================================
// AI Validation Handlers (Stubs)
// ============================================================================

/// POST /api/setup/validate-ai-key
///
/// Validates an AI provider API key. Currently a stub.
/// Full implementation will call provider APIs to verify keys.
async fn validate_ai_key(
    Json(body): Json<ValidateAIKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!(provider = ?body.provider, "POST /setup/validate-ai-key (stub)");

    // Stub: AI key validation requires provider-specific API calls.
    // This will be implemented when the AI provider service is ported.
    Ok(Json(json!({
        "valid": false,
        "provider": body.provider,
        "error": "AI key validation not implemented yet in Rust server",
    })))
}

/// POST /api/setup/validate-openrouter-key
///
/// Validates an OpenRouter API key by fetching the models list.
/// Returns the full model list and a filtered free-models subset.
async fn validate_openrouter_key(
    Json(body): Json<ValidateOpenRouterKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /setup/validate-openrouter-key");

    if body.api_key.is_empty() {
        return Ok(Json(
            json!({ "valid": false, "error": "API key is required" }),
        ));
    }

    match fetch_openrouter_models(&body.api_key).await {
        Ok(models) => {
            let free_models = filter_free_models(&models);
            let mapped: Vec<serde_json::Value> = models
                .iter()
                .map(|m| json!({ "id": m.id, "name": m.name, "pricing": m.pricing }))
                .collect();
            let free_mapped: Vec<serde_json::Value> = free_models
                .iter()
                .map(|m| json!({ "id": m.id, "name": m.name, "pricing": m.pricing }))
                .collect();

            Ok(Json(json!({
                "valid": true,
                "models": mapped,
                "freeModels": free_mapped,
            })))
        }
        Err(e) => {
            warn!(error = %e, "OpenRouter key validation failed");
            let error_msg = e.to_string();
            if error_msg.contains("401") {
                Ok(Json(json!({
                    "valid": false,
                    "error": "Invalid API key. Please check your key and try again.",
                })))
            } else {
                Ok(Json(json!({ "valid": false, "error": error_msg })))
            }
        }
    }
}

/// POST /api/setup/validate-minimax-key
///
/// Validates a MiniMax API key by making a test request.
async fn validate_minimax_key(
    Json(body): Json<ValidateMiniMaxKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /setup/validate-minimax-key");

    if body.api_key.is_empty() {
        return Ok(Json(
            json!({ "valid": false, "error": "API key is required" }),
        ));
    }

    match crate::agent::api_client::validate_minimax_key(&body.api_key).await {
        Ok(true) => Ok(Json(json!({ "valid": true }))),
        Ok(false) => Ok(Json(json!({
            "valid": false,
            "error": "Invalid API key. Please check your key and try again.",
        }))),
        Err(e) => {
            warn!(error = %e, "MiniMax key validation failed");
            Ok(Json(json!({ "valid": false, "error": e.to_string() })))
        }
    }
}

/// GET /api/setup/openrouter-models
///
/// Returns available OpenRouter models using the stored API key.
async fn get_openrouter_models(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /setup/openrouter-models");

    // Read stored API key from secrets
    let api_key_opt = state
        .db
        .call(|conn| {
            crate::services::secrets_service::get_secret(conn, "ai_api_key", Some("openrouter"))
        })
        .await?;

    let api_key = match api_key_opt {
        Some(key) => key,
        None => {
            return Ok(Json(json!({
                "models": [],
                "freeModels": [],
                "error": "No OpenRouter API key configured",
            })));
        }
    };

    match fetch_openrouter_models(&api_key).await {
        Ok(models) => {
            let free_models = filter_free_models(&models);
            let mapped: Vec<serde_json::Value> = models
                .iter()
                .map(|m| json!({ "id": m.id, "name": m.name, "pricing": m.pricing }))
                .collect();
            let free_mapped: Vec<serde_json::Value> = free_models
                .iter()
                .map(|m| json!({ "id": m.id, "name": m.name, "pricing": m.pricing }))
                .collect();

            Ok(Json(json!({
                "models": mapped,
                "freeModels": free_mapped,
            })))
        }
        Err(e) => {
            warn!(error = %e, "Failed to fetch OpenRouter models");
            Ok(Json(json!({
                "models": [],
                "freeModels": [],
                "error": e.to_string(),
            })))
        }
    }
}

// ============================================================================
// OpenRouter API Helpers
// ============================================================================

#[derive(Debug, serde::Deserialize)]
struct OpenRouterAPIModel {
    id: String,
    name: String,
    pricing: Option<OpenRouterPricing>,
    #[allow(dead_code)]
    supported_parameters: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OpenRouterPricing {
    prompt: String,
    completion: String,
}

#[derive(Debug)]
struct OpenRouterModel {
    id: String,
    name: String,
    pricing: Option<OpenRouterPricing>,
}

/// Fetches models from the OpenRouter API, filtered to those supporting tools.
async fn fetch_openrouter_models(api_key: &str) -> Result<Vec<OpenRouterModel>, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{OPENROUTER_BASE_URL}/models?supported_parameters=tools"
        ))
        .header("Authorization", format!("Bearer {api_key}"))
        .header("HTTP-Referer", "https://dash-agent.local")
        .header("X-Title", "dash-agent")
        .send()
        .await
        .map_err(|e| format!("OpenRouter API request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("OpenRouter API error: {status} - {body}"));
    }

    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        data: Vec<OpenRouterAPIModel>,
    }

    let data: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenRouter response: {e}"))?;

    // Filter to models that support tools
    let models = data
        .data
        .into_iter()
        .filter(|m| {
            m.supported_parameters
                .as_ref()
                .map_or(true, |params| params.iter().any(|p| p == "tools"))
        })
        .map(|m| OpenRouterModel {
            id: m.id,
            name: m.name,
            pricing: m.pricing,
        })
        .collect();

    Ok(models)
}

/// Filters models to only those with zero pricing (free models).
fn filter_free_models(models: &[OpenRouterModel]) -> Vec<&OpenRouterModel> {
    models
        .iter()
        .filter(|m| {
            m.pricing.as_ref().map_or(false, |p| {
                p.prompt.parse::<f64>().unwrap_or(1.0) == 0.0
                    && p.completion.parse::<f64>().unwrap_or(1.0) == 0.0
            })
        })
        .collect()
}

// ============================================================================
// GitHub OAuth Handlers (Stubs)
// ============================================================================

/// GET /api/setup/github/auth
///
/// Returns a GitHub OAuth authorization URL.
/// Currently a stub since OAuth is not yet configured in the Rust server.
async fn get_github_auth() -> Result<impl IntoResponse, AppError> {
    info!("GET /setup/github/auth (stub)");

    Ok(Json(json!({
        "authUrl": "http://localhost:3003/setup/github/callback?code=demo&state=demo-state",
        "state": "demo-state",
        "configured": false,
    })))
}

/// POST /api/setup/github/callback
///
/// Handles the GitHub OAuth callback. Currently a stub.
async fn github_callback() -> Result<Json<serde_json::Value>, AppError> {
    info!("POST /setup/github/callback (stub)");

    Err(AppError::Validation(
        "GitHub OAuth is not configured on this server".to_string(),
    ))
}

// ============================================================================
// Status Handlers
// ============================================================================

/// GET /api/setup/status
///
/// Returns the current server setup status.
async fn get_status() -> Result<impl IntoResponse, AppError> {
    info!("GET /setup/status");

    Ok(Json(json!({
        "serverConfigured": true,
        "githubOAuthConfigured": false,
    })))
}

/// DELETE /api/setup/ai-provider
///
/// Disconnects the AI provider. This is mainly a confirmation endpoint
/// since the actual config is stored in localStorage on the frontend.
async fn delete_ai_provider() -> Result<impl IntoResponse, AppError> {
    info!("DELETE /setup/ai-provider");

    Ok(Json(json!({
        "success": true,
        "message": "AI provider disconnected. Please clear your local storage.",
    })))
}

/// DELETE /api/setup/github
///
/// Disconnects GitHub. This is mainly a confirmation endpoint.
async fn delete_github() -> Result<impl IntoResponse, AppError> {
    info!("DELETE /setup/github");

    Ok(Json(json!({
        "success": true,
        "message": "GitHub disconnected. Please clear your local storage.",
    })))
}

// ============================================================================
// MCP Configuration Handler
// ============================================================================

/// GET /api/setup/mcp-config
///
/// Returns the MCP server configuration for IDE/CLI setup.
/// Provides the current server URL and port.
async fn get_mcp_config(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("GET /setup/mcp-config");

    let port = state.config.port;
    // In production, the URL would be built from the request's host header.
    // For now, use localhost with the configured port.
    let url = format!("http://localhost:{port}/api/mcp");

    Ok(Json(json!({
        "url": url,
        "port": port,
    })))
}

/// GET /api/setup/chrome-mcp-config
///
/// Returns the Chrome MCP client configuration used by QA runs.
async fn get_chrome_mcp_config(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /setup/chrome-mcp-config");

    let (enabled, url) = state
        .db
        .call(|conn| settings_service::get_chrome_mcp_config(conn))
        .await?;

    Ok(Json(ChromeMcpConfigResponse { enabled, url }))
}

/// PATCH /api/setup/chrome-mcp-config
///
/// Updates the Chrome MCP client configuration used by QA runs.
async fn update_chrome_mcp_config(
    State(state): State<AppState>,
    Json(body): Json<UpdateChromeMcpConfigRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("PATCH /setup/chrome-mcp-config");

    let url = normalize_chrome_mcp_url(body.enabled, body.url.as_deref())?;
    let enabled = body.enabled;
    let url_for_db = url.clone();

    state
        .db
        .call(move |conn| {
            settings_service::set_chrome_mcp_config(conn, enabled, url_for_db.as_deref())
        })
        .await?;

    let config = ChromeMcpConfigResponse {
        enabled: body.enabled,
        url,
    };

    Ok(Json(json!({
        "success": true,
        "config": config,
    })))
}

fn normalize_chrome_mcp_url(enabled: bool, url: Option<&str>) -> Result<Option<String>, AppError> {
    let url = url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    if enabled && url.is_none() {
        return Err(AppError::Validation(
            "Chrome MCP URL is required when Chrome MCP is enabled".to_string(),
        ));
    }

    if let Some(ref value) = url {
        let parsed = reqwest::Url::parse(value).map_err(|_| {
            AppError::Validation("Chrome MCP URL must be a valid HTTP or HTTPS URL".to_string())
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(AppError::Validation(
                "Chrome MCP URL must use http or https".to_string(),
            ));
        }
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_mcp_url_requires_value_when_enabled() {
        let error = normalize_chrome_mcp_url(true, Some("   ")).unwrap_err();
        assert!(error.to_string().contains("required"));
    }

    #[test]
    fn chrome_mcp_url_allows_http_urls() {
        let url = normalize_chrome_mcp_url(true, Some(" http://localhost:9222/mcp ")).unwrap();
        assert_eq!(url, Some("http://localhost:9222/mcp".to_string()));
    }

    #[test]
    fn chrome_mcp_url_rejects_non_http_urls() {
        let error = normalize_chrome_mcp_url(true, Some("file:///tmp/mcp")).unwrap_err();
        assert!(error.to_string().contains("http or https"));
    }
}
