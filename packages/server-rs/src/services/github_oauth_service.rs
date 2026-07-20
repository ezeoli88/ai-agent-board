//! GitHub OAuth service for managing the OAuth2 authorization flow.
//!
//! Port of `packages/server/src/services/github-oauth.service.ts`.
//!
//! Manages:
//! - Generation of OAuth authorization URLs with CSRF state tokens
//! - Token exchange (authorization code -> access token)
//! - In-memory state token store with expiration

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Response containing the OAuth authorization URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUrlResponse {
    pub auth_url: String,
    pub state: String,
}

/// Response from the OAuth callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// Token returned from the exchange endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub username: String,
    pub avatar_url: String,
}

/// Internal state entry.
struct StateEntry {
    #[allow(dead_code)]
    created_at: Instant,
    expires_at: Instant,
}

// ---------------------------------------------------------------------------
// GitHub OAuth Service
// ---------------------------------------------------------------------------

/// Service for managing GitHub OAuth2 authorization flow.
pub struct GitHubOAuthService {
    client_id: Option<String>,
    client_secret: Option<String>,
    redirect_uri: String,
    /// In-memory store for OAuth state tokens (state -> expiry).
    pending_states: RwLock<HashMap<String, StateEntry>>,
}

impl GitHubOAuthService {
    /// Creates a new service, reading configuration from environment variables.
    pub fn new() -> Self {
        let client_id = std::env::var("GITHUB_CLIENT_ID")
            .ok()
            .filter(|s| !s.is_empty());
        let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
            .ok()
            .filter(|s| !s.is_empty());
        let redirect_uri = std::env::var("GITHUB_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:3003/setup/github/callback".into());

        Self {
            client_id,
            client_secret,
            redirect_uri,
            pending_states: RwLock::new(HashMap::new()),
        }
    }

    /// Returns `true` if GitHub OAuth is configured (client ID and secret are set).
    pub fn is_configured(&self) -> bool {
        self.client_id.is_some() && self.client_secret.is_some()
    }

    /// Generates a GitHub OAuth authorization URL.
    ///
    /// Returns the URL to redirect the user to and the CSRF state token.
    pub fn get_auth_url(&self) -> Result<AuthUrlResponse, AppError> {
        debug!("Generating GitHub OAuth URL");

        let state = Self::generate_state_token();

        // Store state with 10 minute expiration
        {
            let now = Instant::now();
            let mut states = self
                .pending_states
                .write()
                .map_err(|_| AppError::Internal(anyhow::anyhow!("State lock poisoned")))?;
            states.insert(
                state.clone(),
                StateEntry {
                    created_at: now,
                    expires_at: now + std::time::Duration::from_secs(600),
                },
            );
        }

        let client_id = self.client_id.as_deref().unwrap_or("");
        let scopes = "repo read:user";

        let auth_url = format!(
            "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope={}&state={}",
            urlencoding::encode(client_id),
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode(scopes),
            urlencoding::encode(&state),
        );

        info!(state = %state, "Generated GitHub OAuth URL");
        Ok(AuthUrlResponse { auth_url, state })
    }

    /// Validates and consumes a state token from the OAuth callback.
    pub fn validate_state(&self, state: &str) -> bool {
        let mut states = match self.pending_states.write() {
            Ok(s) => s,
            Err(_) => return false,
        };

        match states.remove(state) {
            Some(entry) => {
                if Instant::now() > entry.expires_at {
                    warn!(state, "OAuth state expired");
                    false
                } else {
                    true
                }
            }
            None => {
                warn!(state, "OAuth state not found");
                false
            }
        }
    }

    /// Exchanges an authorization code for an access token and fetches user info.
    pub async fn exchange_code(&self, code: &str, state: &str) -> Result<OAuthToken, AppError> {
        info!("Handling GitHub OAuth callback");

        // Validate state
        if !self.validate_state(state) {
            return Err(AppError::Validation(
                "Invalid or expired state token. Please try again.".into(),
            ));
        }

        // Check configuration
        if !self.is_configured() {
            warn!("GitHub OAuth not configured, returning simulated response");
            return Ok(OAuthToken {
                access_token: "simulated-token-for-development".into(),
                username: "demo-user".into(),
                avatar_url: "https://github.com/identicons/demo.png".into(),
            });
        }

        let client_id = self.client_id.as_deref().unwrap_or("");
        let client_secret = self.client_secret.as_deref().unwrap_or("");

        // Exchange code for token
        let client = reqwest::Client::new();
        let response = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "client_id": client_id,
                "client_secret": client_secret,
                "code": code,
            }))
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!("Token exchange request failed: {e}"))
            })?;

        if !response.status().is_success() {
            error!(status = %response.status(), "Token exchange failed");
            return Err(AppError::Internal(anyhow::anyhow!("Token exchange failed")));
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: Option<String>,
            error: Option<String>,
            error_description: Option<String>,
        }

        let token_data: TokenResponse = response.json().await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to parse token response: {e}"))
        })?;

        if let Some(ref err) = token_data.error {
            error!(
                error = %err,
                description = token_data.error_description.as_deref().unwrap_or(""),
                "Token exchange error"
            );
            return Err(AppError::Internal(anyhow::anyhow!(
                "Token exchange error: {}",
                token_data.error_description.as_deref().unwrap_or(err)
            )));
        }

        let access_token = token_data
            .access_token
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("No access token in response")))?;

        // Fetch user info
        let user_response = client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {access_token}"))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "agent-board")
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("User info request failed: {e}")))?;

        if !user_response.status().is_success() {
            error!("Failed to fetch user info after token exchange");
            return Err(AppError::Internal(anyhow::anyhow!(
                "Failed to fetch user information from GitHub"
            )));
        }

        #[derive(Deserialize)]
        struct UserInfo {
            login: String,
            avatar_url: String,
        }

        let user: UserInfo = user_response
            .json()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse user info: {e}")))?;

        info!(username = %user.login, "GitHub OAuth completed successfully");

        Ok(OAuthToken {
            access_token,
            username: user.login,
            avatar_url: user.avatar_url,
        })
    }

    /// Cleans up expired state tokens. Call periodically (e.g., every 60s).
    pub fn cleanup_expired_states(&self) {
        if let Ok(mut states) = self.pending_states.write() {
            let now = Instant::now();
            states.retain(|_, entry| entry.expires_at > now);
        }
    }

    /// Generates a cryptographically secure random state token (64 hex chars).
    fn generate_state_token() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        hex::encode(bytes)
    }
}

impl Default for GitHubOAuthService {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates a GitHub token by making a test API call.
pub async fn validate_token(token: &str) -> bool {
    let client = match reqwest::Client::builder()
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert("User-Agent", "agent-board".parse().unwrap());
            h
        })
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    match client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}
