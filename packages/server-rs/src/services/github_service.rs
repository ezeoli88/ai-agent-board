//! GitHub API service for interacting with repositories, branches, and pull requests.
//!
//! Port of `packages/server/src/services/github.service.ts`.
//! Uses `reqwest` for HTTP calls to the GitHub REST API.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A repository from the user's GitHub account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRepository {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub private: bool,
    pub language: Option<String>,
    pub updated_at: String,
    pub stargazers_count: i64,
}

/// Response from listing GitHub repositories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubReposResponse {
    pub repos: Vec<GitHubRepository>,
    pub total: i64,
}

/// Result of validating a repository URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoValidation {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<GitHubRepository>,
}

/// Result of creating a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestResult {
    pub html_url: String,
    pub number: i64,
    pub state: String,
}

/// A comment on a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRComment {
    pub id: i64,
    pub body: String,
    pub author: PRCommentAuthor,
    pub created_at: String,
    pub updated_at: String,
    pub url: String,
    pub is_review_comment: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<i64>,
}

/// Author of a PR comment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRCommentAuthor {
    pub login: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Raw GitHub API response structs (for deserialization)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GHRepoResponse {
    id: i64,
    name: String,
    full_name: String,
    html_url: String,
    description: Option<String>,
    default_branch: String,
    private: bool,
    language: Option<String>,
    updated_at: Option<String>,
    stargazers_count: Option<i64>,
}

impl From<GHRepoResponse> for GitHubRepository {
    fn from(r: GHRepoResponse) -> Self {
        Self {
            id: r.id,
            name: r.name,
            full_name: r.full_name,
            html_url: r.html_url,
            description: r.description,
            default_branch: r.default_branch,
            private: r.private,
            language: r.language,
            updated_at: r
                .updated_at
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            stargazers_count: r.stargazers_count.unwrap_or(0),
        }
    }
}

#[derive(Deserialize)]
struct GHSearchResponse {
    total_count: i64,
    items: Vec<GHRepoResponse>,
}

#[derive(Deserialize)]
struct GHBranchResponse {
    name: String,
}

#[derive(Deserialize)]
struct GHPullRequestResponse {
    html_url: String,
    number: i64,
    state: String,
}

#[derive(Deserialize)]
struct GHUserResponse {
    login: String,
}

#[derive(Deserialize)]
struct GHIssueComment {
    id: i64,
    body: Option<String>,
    user: Option<GHCommentUser>,
    created_at: String,
    updated_at: String,
    html_url: String,
}

#[derive(Deserialize)]
struct GHReviewComment {
    id: i64,
    body: Option<String>,
    user: Option<GHCommentUser>,
    created_at: String,
    updated_at: String,
    html_url: String,
    path: Option<String>,
    line: Option<i64>,
}

#[derive(Deserialize)]
struct GHCommentUser {
    login: String,
    avatar_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Helper: build a reqwest client with common GitHub headers
// ---------------------------------------------------------------------------

fn github_client(token: &str) -> Result<Client, AppError> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        "application/vnd.github.v3+json".parse().unwrap(),
    );
    headers.insert(reqwest::header::USER_AGENT, "agent-board".parse().unwrap());
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {token}")
            .parse()
            .map_err(|_| AppError::Validation("Invalid GitHub token format".into()))?,
    );

    Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to build HTTP client: {e}")))
}

// ---------------------------------------------------------------------------
// Public API functions
// ---------------------------------------------------------------------------

/// Lists repositories for the authenticated user. If `search` is provided,
/// searches repositories instead of listing.
pub async fn list_repos(
    token: &str,
    page: u32,
    per_page: u32,
    search: Option<&str>,
) -> Result<GitHubReposResponse, AppError> {
    let client = github_client(token)?;

    if let Some(query) = search {
        return search_repos(&client, token, query, page, per_page).await;
    }

    info!(page, per_page, "Listing user repositories");

    let url = format!(
        "https://api.github.com/user/repos?page={page}&per_page={per_page}&sort=updated&direction=desc&type=owner"
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitHub API request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!(status = %status, body = %body, "Failed to list repositories");
        return Err(AppError::Internal(anyhow::anyhow!(
            "GitHub API error ({status}): {body}"
        )));
    }

    let raw_repos: Vec<GHRepoResponse> = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse GitHub response: {e}")))?;

    let repos: Vec<GitHubRepository> = raw_repos.into_iter().map(|r| r.into()).collect();
    let total = repos.len() as i64;

    info!(count = total, page, "User repositories listed successfully");
    Ok(GitHubReposResponse { repos, total })
}

/// Searches repositories accessible to the user.
async fn search_repos(
    client: &Client,
    _token: &str,
    query: &str,
    page: u32,
    per_page: u32,
) -> Result<GitHubReposResponse, AppError> {
    info!(query, page, per_page, "Searching repositories");

    // Get the authenticated user's username for scoping the search
    let username = get_username(client).await?;
    let search_query = format!("{query} user:{username}");
    let encoded_query = urlencoding::encode(&search_query);

    let url = format!(
        "https://api.github.com/search/repositories?q={encoded_query}&page={page}&per_page={per_page}&sort=updated&order=desc"
    );

    let response =
        client.get(&url).send().await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!("GitHub search request failed: {e}"))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!(status = %status, "Failed to search repositories");
        return Err(AppError::Internal(anyhow::anyhow!(
            "GitHub search error ({status}): {body}"
        )));
    }

    let search_response: GHSearchResponse = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse search response: {e}")))?;

    let repos: Vec<GitHubRepository> = search_response
        .items
        .into_iter()
        .map(|r| r.into())
        .collect();

    info!(
        query,
        count = repos.len(),
        total = search_response.total_count,
        "Search completed"
    );

    Ok(GitHubReposResponse {
        repos,
        total: search_response.total_count,
    })
}

/// Gets the authenticated user's username.
async fn get_username(client: &Client) -> Result<String, AppError> {
    let response = client
        .get("https://api.github.com/user")
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to get user info: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "Failed to get GitHub user info"
        )));
    }

    let user: GHUserResponse = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse user response: {e}")))?;

    Ok(user.login)
}

/// Validates a repository URL by checking if it is accessible with the given token.
pub async fn validate_repo_url(token: &str, url: &str) -> Result<RepoValidation, AppError> {
    info!(url, "Validating repository URL");

    // Parse owner/repo from the URL
    let re = regex_lite::Regex::new(r"github\.com[/:]([^/]+)/([^/\s.]+)").unwrap();
    let caps = match re.captures(url) {
        Some(c) => c,
        None => {
            return Ok(RepoValidation {
                valid: false,
                error: Some("Invalid GitHub repository URL".into()),
                repo: None,
            });
        }
    };

    let owner = caps.get(1).unwrap().as_str();
    let repo = caps.get(2).unwrap().as_str().trim_end_matches(".git");

    match get_repo(token, owner, repo).await {
        Ok(repo_info) => Ok(RepoValidation {
            valid: true,
            error: None,
            repo: Some(repo_info),
        }),
        Err(e) => Ok(RepoValidation {
            valid: false,
            error: Some(e.to_string()),
            repo: None,
        }),
    }
}

/// Gets information about a specific repository.
pub async fn get_repo(token: &str, owner: &str, repo: &str) -> Result<GitHubRepository, AppError> {
    debug!(owner, repo, "Getting repository info");

    let client = github_client(token)?;
    let url = format!("https://api.github.com/repos/{owner}/{repo}");

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitHub API request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!(
            "Failed to get repository ({status}): {body}"
        )));
    }

    let raw: GHRepoResponse = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse repo response: {e}")))?;

    Ok(raw.into())
}

/// Creates a pull request on GitHub.
pub async fn create_pull_request(
    token: &str,
    owner: &str,
    repo: &str,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
) -> Result<PullRequestResult, AppError> {
    info!(owner, repo, head, base, title, "Creating pull request");

    let client = github_client(token)?;
    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls");

    let payload = serde_json::json!({
        "head": head,
        "base": base,
        "title": title,
        "body": body,
    });

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitHub PR creation failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!(status = %status, "Failed to create pull request");
        return Err(AppError::Internal(anyhow::anyhow!(
            "Failed to create PR ({status}): {body}"
        )));
    }

    let pr: GHPullRequestResponse = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse PR response: {e}")))?;

    info!(pr_url = %pr.html_url, number = pr.number, "Pull request created successfully");

    Ok(PullRequestResult {
        html_url: pr.html_url,
        number: pr.number,
        state: pr.state,
    })
}

/// Gets the branches of a repository.
pub async fn get_branches(token: &str, owner: &str, repo: &str) -> Result<Vec<String>, AppError> {
    debug!(owner, repo, "Getting repository branches");

    let client = github_client(token)?;
    let url = format!("https://api.github.com/repos/{owner}/{repo}/branches?per_page=100");

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitHub API request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!(
            "Failed to get branches ({status}): {body}"
        )));
    }

    let branches: Vec<GHBranchResponse> = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse branches: {e}")))?;

    let names: Vec<String> = branches.into_iter().map(|b| b.name).collect();

    info!(owner, repo, count = names.len(), "Branches retrieved");
    Ok(names)
}

/// Gets issue comments and review comments on a pull request.
pub async fn get_pr_comments(
    token: &str,
    owner: &str,
    repo: &str,
    pr_number: i64,
    since: Option<&str>,
) -> Result<Vec<PRComment>, AppError> {
    debug!(owner, repo, pr_number, "Getting PR comments");

    let client = github_client(token)?;
    let mut comments = Vec::new();

    // Fetch issue comments
    let mut issue_url = format!(
        "https://api.github.com/repos/{owner}/{repo}/issues/{pr_number}/comments?per_page=100"
    );
    if let Some(since_ts) = since {
        issue_url.push_str(&format!("&since={since_ts}"));
    }

    let response = client
        .get(&issue_url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitHub API request failed: {e}")))?;

    if response.status().is_success() {
        let issue_comments: Vec<GHIssueComment> = response.json().await.unwrap_or_default();

        for c in issue_comments {
            comments.push(PRComment {
                id: c.id,
                body: c.body.unwrap_or_default(),
                author: PRCommentAuthor {
                    login: c
                        .user
                        .as_ref()
                        .map_or("unknown".to_string(), |u| u.login.clone()),
                    avatar_url: c.user.as_ref().and_then(|u| u.avatar_url.clone()),
                },
                created_at: c.created_at,
                updated_at: c.updated_at,
                url: c.html_url,
                is_review_comment: false,
                path: None,
                line: None,
            });
        }
    }

    // Fetch review comments
    let mut review_url = format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}/comments?per_page=100"
    );
    if let Some(since_ts) = since {
        review_url.push_str(&format!("&since={since_ts}"));
    }

    let response = client
        .get(&review_url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitHub API request failed: {e}")))?;

    if response.status().is_success() {
        let review_comments: Vec<GHReviewComment> = response.json().await.unwrap_or_default();

        for c in review_comments {
            comments.push(PRComment {
                id: c.id,
                body: c.body.unwrap_or_default(),
                author: PRCommentAuthor {
                    login: c
                        .user
                        .as_ref()
                        .map_or("unknown".to_string(), |u| u.login.clone()),
                    avatar_url: c.user.as_ref().and_then(|u| u.avatar_url.clone()),
                },
                created_at: c.created_at,
                updated_at: c.updated_at,
                url: c.html_url,
                is_review_comment: true,
                path: c.path,
                line: c.line,
            });
        }
    }

    debug!(
        owner,
        repo,
        pr_number,
        count = comments.len(),
        "PR comments retrieved"
    );

    Ok(comments)
}

/// Validates a GitHub token by making a test API call.
pub async fn validate_token(token: &str) -> bool {
    let client = match github_client(token) {
        Ok(c) => c,
        Err(_) => return false,
    };

    match client.get("https://api.github.com/user").send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}
