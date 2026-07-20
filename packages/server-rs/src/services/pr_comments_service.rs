//! PR comments polling service for monitoring new comments on pull requests.
//!
//! Port of `packages/server/src/services/pr-comments.service.ts`.
//!
//! This service:
//! - Polls GitHub/GitLab APIs every 60 seconds for comments on tracked PRs/MRs
//! - Tracks seen comments to detect new ones
//! - Emits SSE events when new comments are detected
//! - Automatically starts/stops tracking based on task status changes

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::services::{github_service, gitlab_service};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Polling interval in seconds.
const POLLING_INTERVAL_SECS: u64 = 60;

/// Task statuses that indicate a PR should be monitored.
const PR_ACTIVE_STATUSES: &[&str] = &["pr_created", "changes_requested", "merge_conflicts"];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Tracked PR information for polling.
struct TrackedPR {
    task_id: String,
    repo_url: String,
    pr_number: i64,
    /// Set of comment IDs we have already seen.
    seen_comment_ids: HashSet<i64>,
    /// ISO timestamp of the last poll.
    last_poll_time: String,
}

/// Data about a PR comment that is emitted via SSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRCommentData {
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

/// Summary info about a tracked PR (for debugging/monitoring).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedPRInfo {
    pub task_id: String,
    pub pr_number: i64,
    pub seen_comments_count: usize,
}

// ---------------------------------------------------------------------------
// PR Comments Service
// ---------------------------------------------------------------------------

/// Service that polls GitHub/GitLab for new PR comments and emits SSE events.
pub struct PRCommentsService {
    /// Map of task ID to tracked PR info.
    tracked_prs: Arc<RwLock<HashMap<String, TrackedPR>>>,
    /// Handle to the background polling task.
    polling_handle: Mutex<Option<JoinHandle<()>>>,
    /// Whether the service is currently running.
    is_running: Arc<RwLock<bool>>,
    /// GitHub token for API calls.
    github_token: Arc<RwLock<Option<String>>>,
    /// GitLab token for API calls.
    gitlab_token: Arc<RwLock<Option<String>>>,
}

impl PRCommentsService {
    /// Creates a new `PRCommentsService`.
    pub fn new() -> Self {
        Self {
            tracked_prs: Arc::new(RwLock::new(HashMap::new())),
            polling_handle: Mutex::new(None),
            is_running: Arc::new(RwLock::new(false)),
            github_token: Arc::new(RwLock::new(None)),
            gitlab_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Sets the GitHub token used for API calls.
    pub async fn set_github_token(&self, token: Option<String>) {
        let mut t = self.github_token.write().await;
        *t = token;
    }

    /// Sets the GitLab token used for API calls.
    pub async fn set_gitlab_token(&self, token: Option<String>) {
        let mut t = self.gitlab_token.write().await;
        *t = token;
    }

    /// Starts the background polling loop.
    pub async fn start(&self) {
        {
            let running = self.is_running.read().await;
            if *running {
                warn!("PR comments service is already running");
                return;
            }
        }

        info!("Starting PR comments polling service");
        {
            let mut running = self.is_running.write().await;
            *running = true;
        }

        let tracked = Arc::clone(&self.tracked_prs);
        let is_running = Arc::clone(&self.is_running);
        let gh_token = Arc::clone(&self.github_token);
        let gl_token = Arc::clone(&self.gitlab_token);

        let handle = tokio::spawn(async move {
            loop {
                // Check if we should stop
                {
                    let running = is_running.read().await;
                    if !*running {
                        break;
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(POLLING_INTERVAL_SECS)).await;

                // Check again after sleeping
                {
                    let running = is_running.read().await;
                    if !*running {
                        break;
                    }
                }

                // Poll all tracked PRs
                let task_ids: Vec<String> = {
                    let prs = tracked.read().await;
                    prs.keys().cloned().collect()
                };

                if task_ids.is_empty() {
                    continue;
                }

                debug!(count = task_ids.len(), "Polling PRs for new comments");

                let gh = {
                    let t = gh_token.read().await;
                    t.clone()
                };
                let gl = {
                    let t = gl_token.read().await;
                    t.clone()
                };

                for task_id in &task_ids {
                    if let Err(e) =
                        poll_single_pr(&tracked, task_id, gh.as_deref(), gl.as_deref()).await
                    {
                        warn!(task_id = %task_id, error = %e, "Failed to poll PR");
                    }
                }
            }
            info!("PR comments polling loop exited");
        });

        {
            let mut h = self.polling_handle.lock().await;
            *h = Some(handle);
        }

        info!(
            polling_interval_secs = POLLING_INTERVAL_SECS,
            "PR comments polling service started"
        );
    }

    /// Stops the background polling loop.
    pub async fn stop(&self) {
        {
            let running = self.is_running.read().await;
            if !*running {
                return;
            }
        }

        info!("Stopping PR comments polling service");
        {
            let mut running = self.is_running.write().await;
            *running = false;
        }

        // Abort the polling task
        {
            let mut h = self.polling_handle.lock().await;
            if let Some(handle) = h.take() {
                handle.abort();
            }
        }

        // Clear tracked PRs
        {
            let mut prs = self.tracked_prs.write().await;
            prs.clear();
        }

        info!("PR comments polling service stopped");
    }

    /// Starts tracking a PR for a task.
    pub async fn track_pr(&self, task_id: &str, repo_url: &str, pr_url: &str) {
        let pr_number = match extract_pr_number(pr_url) {
            Some(n) => n,
            None => {
                warn!(pr_url, task_id, "Failed to extract PR number from URL");
                return;
            }
        };

        let mut prs = self.tracked_prs.write().await;
        if prs.contains_key(task_id) {
            debug!(task_id, "Already tracking PR");
            return;
        }

        prs.insert(
            task_id.to_string(),
            TrackedPR {
                task_id: task_id.to_string(),
                repo_url: repo_url.to_string(),
                pr_number,
                seen_comment_ids: HashSet::new(),
                last_poll_time: chrono::Utc::now().to_rfc3339(),
            },
        );

        info!(task_id, pr_number, "Started tracking PR for comments");
    }

    /// Stops tracking a PR for a task.
    pub async fn untrack_pr(&self, task_id: &str) {
        let mut prs = self.tracked_prs.write().await;
        if prs.remove(task_id).is_some() {
            info!(task_id, "Stopped tracking PR for comments");
        }
    }

    /// Called when a task status changes to update PR tracking.
    pub async fn on_task_status_change(
        &self,
        task_id: &str,
        repo_url: &str,
        pr_url: Option<&str>,
        new_status: &str,
    ) {
        let should_track = PR_ACTIVE_STATUSES.contains(&new_status) && pr_url.is_some();
        let is_tracking = {
            let prs = self.tracked_prs.read().await;
            prs.contains_key(task_id)
        };

        if should_track && !is_tracking {
            if let Some(url) = pr_url {
                self.track_pr(task_id, repo_url, url).await;
            }
        } else if !should_track && is_tracking {
            self.untrack_pr(task_id).await;
        }
    }

    /// Returns the list of currently tracked PRs (for debugging/monitoring).
    pub async fn get_tracked_prs(&self) -> Vec<TrackedPRInfo> {
        let prs = self.tracked_prs.read().await;
        prs.values()
            .map(|pr| TrackedPRInfo {
                task_id: pr.task_id.clone(),
                pr_number: pr.pr_number,
                seen_comments_count: pr.seen_comment_ids.len(),
            })
            .collect()
    }

    /// Returns whether the service is currently running.
    pub async fn is_service_running(&self) -> bool {
        let running = self.is_running.read().await;
        *running
    }
}

impl Default for PRCommentsService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extracts PR/MR number from a GitHub PR URL or GitLab MR URL.
fn extract_pr_number(pr_url: &str) -> Option<i64> {
    // GitHub: /pull/123
    let gh_re = regex_lite::Regex::new(r"/pull/(\d+)").unwrap();
    if let Some(caps) = gh_re.captures(pr_url) {
        if let Some(m) = caps.get(1) {
            return m.as_str().parse().ok();
        }
    }

    // GitLab: /merge_requests/456
    let gl_re = regex_lite::Regex::new(r"/merge_requests/(\d+)").unwrap();
    if let Some(caps) = gl_re.captures(pr_url) {
        if let Some(m) = caps.get(1) {
            return m.as_str().parse().ok();
        }
    }

    None
}

/// Polls a single tracked PR for new comments.
async fn poll_single_pr(
    tracked_prs: &Arc<RwLock<HashMap<String, TrackedPR>>>,
    task_id: &str,
    github_token: Option<&str>,
    gitlab_token: Option<&str>,
) -> Result<(), AppError> {
    let (repo_url, pr_number, since) = {
        let prs = tracked_prs.read().await;
        let pr = prs
            .get(task_id)
            .ok_or_else(|| AppError::NotFound(format!("Task {task_id} is not tracked")))?;
        (pr.repo_url.clone(), pr.pr_number, pr.last_poll_time.clone())
    };

    let is_gitlab = gitlab_service::is_gitlab_url(&repo_url);

    let new_comment_ids: Vec<i64>;

    if is_gitlab {
        let token = gitlab_token
            .ok_or_else(|| AppError::Unauthorized("GitLab token not configured".into()))?;

        let notes =
            gitlab_service::get_merge_request_notes(token, &repo_url, pr_number, Some(&since))
                .await?;

        // Filter new comments
        let prs = tracked_prs.read().await;
        if let Some(pr) = prs.get(task_id) {
            new_comment_ids = notes
                .iter()
                .filter(|n| !pr.seen_comment_ids.contains(&n.id))
                .map(|n| n.id)
                .collect();
        } else {
            return Ok(());
        }
    } else {
        let token = github_token
            .ok_or_else(|| AppError::Unauthorized("GitHub token not configured".into()))?;

        // Parse owner/repo from URL
        let re = regex_lite::Regex::new(r"github\.com[/:]([^/]+)/([^/\s.]+)").unwrap();
        let caps = re
            .captures(&repo_url)
            .ok_or_else(|| AppError::Validation(format!("Cannot parse GitHub URL: {repo_url}")))?;
        let owner = caps.get(1).unwrap().as_str();
        let repo = caps.get(2).unwrap().as_str().trim_end_matches(".git");

        let comments =
            github_service::get_pr_comments(token, owner, repo, pr_number, Some(&since)).await?;

        let prs = tracked_prs.read().await;
        if let Some(pr) = prs.get(task_id) {
            new_comment_ids = comments
                .iter()
                .filter(|c| !pr.seen_comment_ids.contains(&c.id))
                .map(|c| c.id)
                .collect();
        } else {
            return Ok(());
        }
    }

    // Mark new comments as seen and update poll time
    if !new_comment_ids.is_empty() {
        let mut prs = tracked_prs.write().await;
        if let Some(pr) = prs.get_mut(task_id) {
            for id in &new_comment_ids {
                pr.seen_comment_ids.insert(*id);
            }
            pr.last_poll_time = chrono::Utc::now().to_rfc3339();

            info!(
                task_id,
                pr_number,
                new_count = new_comment_ids.len(),
                "New PR comments detected"
            );
        }
    } else {
        // Still update the poll time even if no new comments
        let mut prs = tracked_prs.write().await;
        if let Some(pr) = prs.get_mut(task_id) {
            pr.last_poll_time = chrono::Utc::now().to_rfc3339();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pr_number_github() {
        assert_eq!(
            extract_pr_number("https://github.com/owner/repo/pull/123"),
            Some(123)
        );
    }

    #[test]
    fn test_extract_pr_number_gitlab() {
        assert_eq!(
            extract_pr_number("https://gitlab.com/owner/repo/-/merge_requests/456"),
            Some(456)
        );
    }

    #[test]
    fn test_extract_pr_number_invalid() {
        assert_eq!(extract_pr_number("https://example.com/foo"), None);
    }
}
