use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Result of an agent execution run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunResult {
    /// Whether the agent completed successfully.
    pub success: bool,
    /// URL of the created pull request, if any.
    pub pr_url: Option<String>,
    /// Error message if the agent failed.
    pub error: Option<String>,
    /// Summary of what the agent accomplished.
    pub summary: Option<String>,
    /// Serialized changes/diff data.
    pub changes_data: Option<String>,
}

/// Supported agent types (CLI and API-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentType {
    ClaudeCode,
    Codex,
    Copilot,
    Gemini,
    MiniMax,
}

impl AgentType {
    /// Returns the string identifier used in API payloads and configuration.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Gemini => "gemini",
            Self::MiniMax => "minimax",
        }
    }

    /// Returns true if this agent type uses an HTTP API instead of a CLI process.
    pub fn is_api_based(&self) -> bool {
        matches!(self, Self::MiniMax)
    }
}

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AgentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude-code" => Ok(Self::ClaudeCode),
            "codex" => Ok(Self::Codex),
            "copilot" => Ok(Self::Copilot),
            "gemini" => Ok(Self::Gemini),
            "minimax" => Ok(Self::MiniMax),
            other => Err(format!("unknown agent type: '{other}'")),
        }
    }
}

/// Describes the command and arguments to spawn for a CLI agent.
#[derive(Debug, Clone)]
pub struct CLICommand {
    /// The executable name (e.g., "claude", "codex").
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// When true, the prompt is delivered via stdin instead of as a CLI argument.
    pub use_stdin: bool,
    /// When true, stdin must be closed (shutdown) after writing the prompt so the
    /// agent sees EOF and starts processing. This is needed for agents like Claude
    /// Code whose `-p` flag reads stdin until EOF as the full prompt. When false
    /// and `use_stdin` is true, stdin is kept open for interactive feedback
    /// (e.g., Gemini, Copilot).
    pub close_stdin_after_prompt: bool,
}

/// Options for constructing and running a CLI agent.
#[derive(Debug, Clone)]
pub struct CLIRunnerOptions {
    /// The entity ID (task or spec) this agent run is associated with.
    pub entity_id: String,
    /// The type of CLI agent to spawn.
    pub agent_type: AgentType,
    /// The prompt text to send to the agent.
    pub prompt: String,
    /// Optional model override (e.g., "claude-opus-4-6").
    pub model: Option<String>,
    /// Working directory for the spawned process.
    pub cwd: PathBuf,
    /// Additional environment variables to inject into the child process.
    pub env: HashMap<String, String>,
    /// Whether this is a plan-only run (read-only tools).
    pub plan_only: bool,
}

/// Options for constructing and running an API-based agent (e.g., MiniMax).
#[derive(Debug, Clone)]
pub struct APIRunnerOptions {
    /// The entity ID (task or spec) this agent run is associated with.
    pub entity_id: String,
    /// The type of API agent to run.
    pub agent_type: AgentType,
    /// The prompt text to send to the agent.
    pub prompt: String,
    /// Optional model override (e.g., "MiniMax-M1").
    pub model: Option<String>,
    /// Working directory for tool execution.
    pub cwd: PathBuf,
    /// API key for the provider.
    pub api_key: String,
}

/// Identifies what entity an agent run is associated with.
///
/// Used by `AgentService` to dispatch completion logic (update task vs spec).
#[derive(Debug, Clone)]
pub enum AgentEntityContext {
    Task { task_id: String },
    Spec { spec_id: String },
}

impl AgentEntityContext {
    pub fn entity_id(&self) -> &str {
        match self {
            Self::Task { task_id } => task_id,
            Self::Spec { spec_id } => spec_id,
        }
    }

    pub fn entity_type(&self) -> &str {
        match self {
            Self::Task { .. } => "task",
            Self::Spec { .. } => "spec",
        }
    }
}

/// Auth/subscription error rule for detecting known failure patterns in stderr.
#[derive(Debug, Clone)]
pub struct AuthErrorRule {
    /// Regex pattern to match against stderr output.
    pub pattern: regex_lite::Regex,
    /// User-friendly error message.
    pub message: &'static str,
    /// URL where the user can resolve the issue.
    pub help_url: &'static str,
}
