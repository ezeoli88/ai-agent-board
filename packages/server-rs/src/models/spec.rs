use serde::{Deserialize, Serialize};

/// Lifecycle for the standalone SDD demo workflow.
///
/// This is intentionally separate from `TaskStatus` so specs can evolve through
/// ideation, review, planning, and task breakdown before creating implementation
/// tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecStatus {
    #[serde(rename = "idea")]
    Idea,
    #[serde(rename = "clarifying")]
    Clarifying,
    #[serde(rename = "spec_draft")]
    SpecDraft,
    #[serde(rename = "spec_review")]
    SpecReview,
    #[serde(rename = "plan")]
    Plan,
    #[serde(rename = "task_breakdown")]
    TaskBreakdown,
    #[serde(rename = "ready_for_implementation")]
    ReadyForImplementation,
    #[serde(rename = "implemented")]
    Implemented,
    #[serde(rename = "failed")]
    Failed,
}

impl SpecStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idea => "idea",
            Self::Clarifying => "clarifying",
            Self::SpecDraft => "spec_draft",
            Self::SpecReview => "spec_review",
            Self::Plan => "plan",
            Self::TaskBreakdown => "task_breakdown",
            Self::ReadyForImplementation => "ready_for_implementation",
            Self::Implemented => "implemented",
            Self::Failed => "failed",
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
            "idea" => Ok(Self::Idea),
            "clarifying" => Ok(Self::Clarifying),
            "spec_draft" => Ok(Self::SpecDraft),
            "spec_review" => Ok(Self::SpecReview),
            "plan" => Ok(Self::Plan),
            "task_breakdown" => Ok(Self::TaskBreakdown),
            "ready_for_implementation" => Ok(Self::ReadyForImplementation),
            "implemented" => Ok(Self::Implemented),
            "failed" => Ok(Self::Failed),
            other => Err(format!("unknown spec status: '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecAgentRole {
    #[serde(rename = "clarifier")]
    Clarifier,
    #[serde(rename = "clarification_parser")]
    ClarificationParser,
    #[serde(rename = "spec_writer")]
    SpecWriter,
    #[serde(rename = "plan_writer")]
    PlanWriter,
    #[serde(rename = "task_breakdown")]
    TaskBreakdown,
    #[serde(rename = "reviewer")]
    Reviewer,
}

impl SpecAgentRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clarifier => "clarifier",
            Self::ClarificationParser => "clarification_parser",
            Self::SpecWriter => "spec_writer",
            Self::PlanWriter => "plan_writer",
            Self::TaskBreakdown => "task_breakdown",
            Self::Reviewer => "reviewer",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Clarifier => "Clarifier",
            Self::ClarificationParser => "Clarification Parser",
            Self::SpecWriter => "Spec Writer",
            Self::PlanWriter => "Plan Writer",
            Self::TaskBreakdown => "Task Breakdown Writer",
            Self::Reviewer => "Spec Reviewer",
        }
    }
}

impl std::fmt::Display for SpecAgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SpecAgentRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "clarifier" => Ok(Self::Clarifier),
            "clarification_parser" => Ok(Self::ClarificationParser),
            "spec_writer" => Ok(Self::SpecWriter),
            "plan_writer" => Ok(Self::PlanWriter),
            "task_breakdown" => Ok(Self::TaskBreakdown),
            "reviewer" => Ok(Self::Reviewer),
            other => Err(format!("unknown spec agent role: '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecAgentSession {
    pub id: String,
    pub spec_id: String,
    pub role: SpecAgentRole,
    pub agent_type: String,
    pub agent_model: Option<String>,
    pub session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub id: String,
    pub repository_id: String,
    pub title: String,
    pub user_input: String,
    pub status: SpecStatus,

    pub clarification_questions: Vec<String>,
    pub clarification_answers: Vec<String>,
    pub clarification_answered_at: Option<String>,

    pub generated_spec: Option<String>,
    pub final_spec: Option<String>,
    pub spec_approved_at: Option<String>,

    pub generated_plan: Option<String>,
    pub final_plan: Option<String>,
    pub plan_approved_at: Option<String>,

    pub generated_tasks: Option<String>,
    pub final_tasks: Option<String>,
    pub tasks_approved_at: Option<String>,
    pub task_ids: Vec<String>,

    pub agent_type: Option<String>,
    pub agent_model: Option<String>,
    pub agent_session_id: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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
    pub user_input: Option<String>,
    pub status: Option<SpecStatus>,
    pub clarification_questions: Option<Vec<String>>,
    pub clarification_answers: Option<Vec<String>>,
    pub clarification_answered_at: Option<Option<String>>,
    pub generated_spec: Option<Option<String>>,
    pub final_spec: Option<Option<String>>,
    pub spec_approved_at: Option<Option<String>>,
    pub generated_plan: Option<Option<String>>,
    pub final_plan: Option<Option<String>>,
    pub plan_approved_at: Option<Option<String>>,
    pub generated_tasks: Option<Option<String>>,
    pub final_tasks: Option<Option<String>>,
    pub tasks_approved_at: Option<Option<String>>,
    pub task_ids: Option<Vec<String>>,
    pub agent_type: Option<Option<String>>,
    pub agent_model: Option<Option<String>>,
    pub agent_session_id: Option<Option<String>>,
    pub error: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApproveSpecStepInput {
    pub content: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnswerClarificationsInput {
    pub answers: Vec<String>,
}
