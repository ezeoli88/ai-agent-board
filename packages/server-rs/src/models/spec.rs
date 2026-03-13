use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecStatus {
    #[serde(rename = "draft")]
    Draft,
    #[serde(rename = "refining")]
    Refining,
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "canceled")]
    Canceled,
}

impl SpecStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Refining => "refining",
            Self::Approved => "approved",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }
}

impl std::fmt::Display for SpecStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SpecStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(Self::Draft),
            "refining" => Ok(Self::Refining),
            "approved" => Ok(Self::Approved),
            "failed" => Ok(Self::Failed),
            "canceled" => Ok(Self::Canceled),
            other => Err(format!("unknown spec status: '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub id: String,
    pub repository_id: String,
    pub title: String,
    pub user_input: String,
    pub draft_spec: Option<String>,
    pub final_spec: Option<String>,
    pub agent_type: Option<String>,
    pub agent_model: Option<String>,
    pub status: SpecStatus,
    pub task_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub approved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpecInput {
    pub repository_id: String,
    pub user_input: String,
    pub title: Option<String>,
    pub agent_type: Option<String>,
    pub agent_model: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateSpecInput {
    pub title: Option<String>,
    pub draft_spec: Option<Option<String>>,
    pub final_spec: Option<Option<String>>,
    pub status: Option<SpecStatus>,
    pub task_id: Option<Option<String>>,
    pub approved_at: Option<Option<String>>,
    pub agent_type: Option<Option<String>>,
    pub agent_model: Option<Option<String>>,
}
