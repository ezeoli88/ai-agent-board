use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QaRunStatus {
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "passed")]
    Passed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "canceled")]
    Canceled,
}

impl QaRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }
}

impl std::fmt::Display for QaRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for QaRunStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            "canceled" => Ok(Self::Canceled),
            other => Err(format!("unknown QA run status: '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaRun {
    pub id: String,
    pub task_id: String,
    pub status: QaRunStatus,
    pub test_command: String,
    pub target_url: Option<String>,
    pub report_path: Option<String>,
    pub trace_path: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i64>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateQaRunInput {
    pub target_url: Option<String>,
    pub test_command: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateQaRunInput {
    pub status: Option<QaRunStatus>,
    pub report_path: Option<Option<String>>,
    pub trace_path: Option<Option<String>>,
    pub stdout: Option<Option<String>>,
    pub stderr: Option<Option<String>>,
    pub exit_code: Option<Option<i64>>,
    pub started_at: Option<Option<String>>,
    pub completed_at: Option<Option<String>>,
}
