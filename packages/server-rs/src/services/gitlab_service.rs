//! GitLab API service for interacting with merge requests and user validation.
//!
//! Port of `packages/server/src/gitlab/client.ts` and
//! `packages/server/src/utils/gitlab-url.ts`.
//! Uses `reqwest` for HTTP calls to the GitLab v4 REST API.

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Parsed GitLab repository information.
#[derive(Debug, Clone)]
pub struct GitLabRepoInfo {
    pub owner: String,
    pub repo: String,
}

/// GitLab user info returned by the user endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabUser {
    pub id: i64,
    pub username: String,
    pub name: String,
    pub avatar_url: Option<String>,
}

/// Result of creating a merge request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRequestResult {
    pub url: String,
    pub number: i64,
}

/// Information about a GitLab merge request state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRequestInfo {
    pub state: String,
    pub iid: i64,
    pub web_url: String,
}

/// A comment/note on a merge request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MRComment {
    pub id: i64,
    pub body: String,
    pub author_login: String,
    pub author_avatar_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub url: String,
    pub is_review_comment: bool,
}

// ---------------------------------------------------------------------------
// URL utilities
// ---------------------------------------------------------------------------

/// Checks if a URL is a GitLab URL.
pub fn is_gitlab_url(url: &str) -> bool {
    url.to_lowercase().contains("gitlab.com")
}

/// Parses a GitLab repository URL and extracts owner and repo name.
///
/// Supports:
/// - `https://gitlab.com/user/repo`
/// - `https://gitlab.com/user/repo.git`
/// - `git@gitlab.com:user/repo.git`
pub fn parse_gitlab_url(url: &str) -> Result<GitLabRepoInfo, AppError> {
    let clean = url.trim_end_matches('/').trim_end_matches(".git");

    // Try HTTPS format
    let https_re = regex_lite::Regex::new(r"(?:https?://)?gitlab\.com/([^/]+)/([^/]+)").unwrap();
    if let Some(caps) = https_re.captures(clean) {
        if let (Some(owner_m), Some(repo_m)) = (caps.get(1), caps.get(2)) {
            return Ok(GitLabRepoInfo {
                owner: owner_m.as_str().to_string(),
                repo: repo_m.as_str().to_string(),
            });
        }
    }

    // Try SSH format
    let ssh_re = regex_lite::Regex::new(r"git@gitlab\.com:([^/]+)/([^/]+)").unwrap();
    if let Some(caps) = ssh_re.captures(clean) {
        if let (Some(owner_m), Some(repo_m)) = (caps.get(1), caps.get(2)) {
            return Ok(GitLabRepoInfo {
                owner: owner_m.as_str().to_string(),
                repo: repo_m.as_str().to_string(),
            });
        }
    }

    Err(AppError::Validation(format!(
        "Invalid GitLab URL format: {url}"
    )))
}

/// Strips embedded credentials from a git URL.
/// e.g. `https://oauth2:token@gitlab.com/user/repo.git` -> `https://gitlab.com/user/repo.git`
pub fn strip_credentials_from_url(url: &str) -> String {
    let re = regex_lite::Regex::new(r"//[^@]+@").unwrap();
    re.replace(url, "//").to_string()
}

/// Extracts an embedded token from a git URL with credentials.
/// e.g. `https://oauth2:TOKEN@gitlab.com/user/repo.git` -> `Some("TOKEN")`
pub fn extract_token_from_url(url: &str) -> Option<String> {
    let re = regex_lite::Regex::new(r"//[^:]+:([^@]+)@").unwrap();
    let caps = re.captures(url)?;
    caps.get(1).map(|m| m.as_str().to_string())
}

/// Creates a GitLab clone URL with embedded OAuth2 token.
pub fn to_gitlab_authenticated_clone_url(url: &str, token: &str) -> Result<String, AppError> {
    let info = parse_gitlab_url(url)?;
    Ok(format!(
        "https://oauth2:{token}@gitlab.com/{}/{}.git",
        info.owner, info.repo
    ))
}

// ---------------------------------------------------------------------------
// Helper: build reqwest client for GitLab API
// ---------------------------------------------------------------------------

fn gitlab_client(token: &str) -> Result<reqwest::Client, AppError> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "PRIVATE-TOKEN",
        token
            .parse()
            .map_err(|_| AppError::Validation("Invalid GitLab token format".into()))?,
    );
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to build HTTP client: {e}")))
}

// ---------------------------------------------------------------------------
// Public API functions
// ---------------------------------------------------------------------------

/// Validates a GitLab token by fetching the authenticated user.
pub async fn validate_token(token: &str) -> Result<GitLabUser, AppError> {
    debug!("Validating GitLab token");

    let client = gitlab_client(token)?;
    let response = client
        .get("https://gitlab.com/api/v4/user")
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitLab API request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Unauthorized(format!(
            "GitLab token validation failed ({status}): {body}"
        )));
    }

    let user: GitLabUser = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse user response: {e}")))?;

    info!(username = %user.username, "GitLab token validated successfully");
    Ok(user)
}

/// Creates a merge request on GitLab.
pub async fn create_merge_request(
    token: &str,
    repo_url: &str,
    source_branch: &str,
    target_branch: &str,
    title: &str,
    description: &str,
) -> Result<MergeRequestResult, AppError> {
    let clean_url = strip_credentials_from_url(repo_url);
    let info = parse_gitlab_url(&clean_url)?;
    let project_id = format!("{}/{}", info.owner, info.repo);
    let project_path = urlencoding::encode(&project_id);

    info!(
        project = %format!("{}/{}", info.owner, info.repo),
        source_branch,
        target_branch,
        title,
        "Creating merge request"
    );

    let client = gitlab_client(token)?;
    let url = format!("https://gitlab.com/api/v4/projects/{project_path}/merge_requests");

    let payload = serde_json::json!({
        "source_branch": source_branch,
        "target_branch": target_branch,
        "title": title,
        "description": description,
    });

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitLab MR creation failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!(status = %status, "Failed to create merge request");
        return Err(AppError::Internal(anyhow::anyhow!(
            "GitLab API error ({status}): {body}"
        )));
    }

    #[derive(Deserialize)]
    struct MRResponse {
        web_url: String,
        iid: i64,
    }

    let mr: MRResponse = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse MR response: {e}")))?;

    info!(url = %mr.web_url, number = mr.iid, "Merge request created successfully");

    Ok(MergeRequestResult {
        url: mr.web_url,
        number: mr.iid,
    })
}

/// Gets information about a merge request (state, iid, web_url).
pub async fn get_merge_request(
    token: &str,
    repo_url: &str,
    mr_number: i64,
) -> Result<MergeRequestInfo, AppError> {
    let clean_url = strip_credentials_from_url(repo_url);
    let info = parse_gitlab_url(&clean_url)?;
    let project_id = format!("{}/{}", info.owner, info.repo);
    let project_path = urlencoding::encode(&project_id);

    debug!(
        project = %format!("{}/{}", info.owner, info.repo),
        mr_number,
        "Fetching merge request"
    );

    let client = gitlab_client(token)?;
    let url =
        format!("https://gitlab.com/api/v4/projects/{project_path}/merge_requests/{mr_number}");

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitLab API request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!(
            "GitLab API error ({status}): {body}"
        )));
    }

    #[derive(Deserialize)]
    struct MRInfoResponse {
        state: String,
        iid: i64,
        web_url: String,
    }

    let mr: MRInfoResponse = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse MR info: {e}")))?;

    Ok(MergeRequestInfo {
        state: mr.state,
        iid: mr.iid,
        web_url: mr.web_url,
    })
}

/// Gets notes (comments) on a merge request, filtering out system notes.
pub async fn get_merge_request_notes(
    token: &str,
    repo_url: &str,
    mr_number: i64,
    since: Option<&str>,
) -> Result<Vec<MRComment>, AppError> {
    let clean_url = strip_credentials_from_url(repo_url);
    let info = parse_gitlab_url(&clean_url)?;
    let project_id = format!("{}/{}", info.owner, info.repo);
    let project_path = urlencoding::encode(&project_id);

    debug!(
        project = %format!("{}/{}", info.owner, info.repo),
        mr_number,
        ?since,
        "Fetching merge request notes"
    );

    let client = gitlab_client(token)?;
    let mut url = format!(
        "https://gitlab.com/api/v4/projects/{project_path}/merge_requests/{mr_number}/notes"
    );
    if let Some(since_ts) = since {
        url.push_str(&format!(
            "?order_by=updated_at&sort=asc&updated_after={}",
            urlencoding::encode(since_ts)
        ));
    }

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitLab API request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!(
            "GitLab API error ({status}): {body}"
        )));
    }

    #[derive(Deserialize)]
    struct NoteAuthor {
        username: String,
        avatar_url: Option<String>,
    }

    #[derive(Deserialize)]
    struct GitLabNote {
        id: i64,
        body: String,
        author: NoteAuthor,
        created_at: String,
        updated_at: String,
        system: bool,
    }

    let notes: Vec<GitLabNote> = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse notes: {e}")))?;

    // Filter out system-generated notes
    let mr_web_url = format!(
        "https://gitlab.com/{}/{}/-/merge_requests/{mr_number}",
        info.owner, info.repo
    );

    let comments: Vec<MRComment> = notes
        .into_iter()
        .filter(|n| !n.system)
        .map(|n| MRComment {
            id: n.id,
            body: n.body,
            author_login: n.author.username,
            author_avatar_url: n.author.avatar_url,
            created_at: n.created_at,
            updated_at: n.updated_at,
            url: format!("{mr_web_url}#note_{}", n.id),
            is_review_comment: false,
        })
        .collect();

    debug!(
        mr_number,
        count = comments.len(),
        "Merge request notes fetched"
    );
    Ok(comments)
}
