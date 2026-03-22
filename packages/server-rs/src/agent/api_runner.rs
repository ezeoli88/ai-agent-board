//! API-based agent runner for MiniMax and future OpenAI-compatible providers.
//!
//! Executes an agentic loop in-process: sends messages to the LLM API,
//! parses tool calls, executes tools, and feeds results back until the
//! model stops requesting tools or the turn limit is reached.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::agent::api_client::{AccumulatedToolCall, MiniMaxClient, StreamResult};
use crate::agent::tools;
use crate::agent::types::{APIRunnerOptions, AgentRunResult};
use crate::agent::xml_parser;
use crate::error::AppError;
use crate::utils::SSEEmitter;

/// Maximum agentic loop turns before stopping.
const MAX_TURNS: usize = 50;

/// Token estimation threshold for context compression.
const COMPRESSION_THRESHOLD: usize = 100_000;

/// Number of recent messages to preserve during compression.
const MESSAGES_TO_KEEP: usize = 20;

/// Default model if none specified.
const DEFAULT_MODEL: &str = "MiniMax-M2.5";

pub struct APIAgentRunner {
    options: APIRunnerOptions,
    client: MiniMaxClient,
    feedback_tx: mpsc::Sender<String>,
    feedback_rx: Option<mpsc::Receiver<String>>,
    cancel_token: CancellationToken,
    is_running: Arc<AtomicBool>,
    is_sub_agent: bool,
    max_turns: usize,
    history: Vec<Value>,
    total_tokens: u64,
}

impl APIAgentRunner {
    pub fn new(options: APIRunnerOptions) -> Self {
        let (feedback_tx, feedback_rx) = mpsc::channel(32);
        let client = MiniMaxClient::new(&options.api_key);
        Self {
            options,
            client,
            feedback_tx,
            feedback_rx: Some(feedback_rx),
            cancel_token: CancellationToken::new(),
            is_running: Arc::new(AtomicBool::new(false)),
            is_sub_agent: false,
            max_turns: MAX_TURNS,
            history: Vec::new(),
            total_tokens: 0,
        }
    }

    /// Creates a sub-agent runner with limited turns and no sub_agent tool.
    pub fn new_sub_agent(
        api_key: &str,
        model: Option<String>,
        cwd: PathBuf,
        entity_id: String,
    ) -> Self {
        let options = APIRunnerOptions {
            entity_id,
            agent_type: crate::agent::types::AgentType::MiniMax,
            prompt: String::new(), // Set by caller
            model,
            cwd,
            api_key: api_key.to_string(),
        };
        let (feedback_tx, feedback_rx) = mpsc::channel(32);
        let client = MiniMaxClient::new(api_key);
        Self {
            options,
            client,
            feedback_tx,
            feedback_rx: Some(feedback_rx),
            cancel_token: CancellationToken::new(),
            is_running: Arc::new(AtomicBool::new(false)),
            is_sub_agent: true,
            max_turns: 20,
            history: Vec::new(),
            total_tokens: 0,
        }
    }

    /// Returns a clone of the cancellation token for external cancellation.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Returns a clone of the feedback sender for sending user messages.
    pub fn feedback_sender(&self) -> mpsc::Sender<String> {
        self.feedback_tx.clone()
    }

    /// Cancels the running agent.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Returns whether the agent is currently running.
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    /// Runs the agentic loop.
    ///
    /// This is the main entry point. It builds the system prompt, adds the
    /// user message, then loops: stream → parse → execute tools → repeat.
    pub async fn run(&mut self, sse: &Arc<SSEEmitter>) -> Result<AgentRunResult, AppError> {
        self.is_running.store(true, Ordering::Relaxed);
        let result = self.run_inner(sse).await;
        self.is_running.store(false, Ordering::Relaxed);
        result
    }

    async fn run_inner(&mut self, sse: &Arc<SSEEmitter>) -> Result<AgentRunResult, AppError> {
        let task_id = &self.options.entity_id.clone();
        let model = self
            .options
            .model
            .clone()
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let cwd = self.options.cwd.clone();

        // Take ownership of feedback receiver
        let mut feedback_rx = self
            .feedback_rx
            .take()
            .expect("feedback_rx already consumed");

        // Build system prompt
        let system_prompt = self.build_system_prompt();
        self.history.push(serde_json::json!({
            "role": "system",
            "content": system_prompt,
        }));

        // Add user prompt
        self.history.push(serde_json::json!({
            "role": "user",
            "content": self.options.prompt,
        }));

        sse.emit_log(task_id, "info", &format!("Starting API agent (model: {model})"), None)
            .await;

        let tool_defs = tools::get_tool_definitions(!self.is_sub_agent);
        let tool_defs_json: Vec<Value> = tool_defs;

        let mut turn = 0;

        loop {
            turn += 1;
            if turn > self.max_turns {
                sse.emit_log(
                    task_id,
                    "warn",
                    &format!("Agent reached max turns ({0}), stopping", self.max_turns),
                    None,
                )
                .await;
                break;
            }

            // Check cancellation
            if self.cancel_token.is_cancelled() {
                sse.emit_log(task_id, "info", "Agent cancelled", None).await;
                return Ok(AgentRunResult {
                    success: false,
                    pr_url: None,
                    error: Some("Cancelled by user".into()),
                    summary: None,
                    changes_data: None,
                });
            }

            // Compress history if needed
            self.compress_history_if_needed().await;

            // Check for user feedback
            while let Ok(feedback) = feedback_rx.try_recv() {
                sse.emit_log(task_id, "info", &format!("User feedback: {feedback}"), None)
                    .await;
                self.history.push(serde_json::json!({
                    "role": "user",
                    "content": feedback,
                }));
            }

            // Build messages for API (full history)
            let messages = self.build_messages_for_api();

            // Stream completion
            sse.emit_log(
                task_id,
                "info",
                &format!("Turn {turn}: streaming response..."),
                None,
            )
            .await;

            let stream_result = self
                .client
                .stream_chat(
                    &model,
                    &messages,
                    Some(&tool_defs_json),
                    sse,
                    task_id,
                    self.cancel_token.clone(),
                )
                .await;

            let result = match stream_result {
                Ok(r) => r,
                Err(e) => {
                    let err_msg = e.to_string();
                    sse.emit_log(task_id, "error", &err_msg, None).await;
                    return Ok(AgentRunResult {
                        success: false,
                        pr_url: None,
                        error: Some(err_msg),
                        summary: None,
                        changes_data: None,
                    });
                }
            };

            // Update token tracking
            self.total_tokens = result.usage.total_tokens;

            // Parse response — extract tool calls (JSON first, XML fallback)
            let tool_calls = self.parse_tool_calls(&result);

            // Emit raw content as chat message (includes <think> blocks, markdown, etc.)
            if !result.content.is_empty() {
                sse.emit_chat_message(task_id, "assistant", &result.content).await;
            }

            // Add assistant message to history
            let mut assistant_msg = serde_json::json!({
                "role": "assistant",
                "content": result.content,
            });
            if !result.reasoning_details.is_empty() {
                assistant_msg["reasoning_details"] = serde_json::json!(
                    result.reasoning_details.iter().map(|r| serde_json::json!({"text": r})).collect::<Vec<_>>()
                );
            }
            if !tool_calls.is_empty() {
                assistant_msg["tool_calls"] = serde_json::json!(
                    tool_calls.iter().map(|tc| serde_json::json!({
                        "id": tc.id,
                        "type": tc.call_type,
                        "function": {
                            "name": tc.function.name,
                            "arguments": tc.function.arguments,
                        }
                    })).collect::<Vec<_>>()
                );
            }
            self.history.push(assistant_msg);

            // If no tool calls, the agent is done
            if tool_calls.is_empty() {
                sse.emit_log(
                    task_id,
                    "info",
                    &format!("Agent completed after {turn} turns"),
                    None,
                )
                .await;
                break;
            }

            // Execute tool calls
            for tc in &tool_calls {
                if self.cancel_token.is_cancelled() {
                    break;
                }

                let tool_name = &tc.function.name;
                let tool_id = &tc.id;

                // Parse arguments
                let args: Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::json!({}));

                // Handle sub_agent specially
                if tool_name == "sub_agent" {
                    let result = self
                        .handle_sub_agent(sse, task_id, &model, &cwd, &args)
                        .await;
                    self.history.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": tool_id,
                        "content": result,
                    }));
                    continue;
                }

                // Emit tool activity start
                let summary = format_tool_summary(tool_name, &args);
                sse.emit_tool_activity(task_id, tool_id, tool_name, &summary, "running")
                    .await;

                // Execute tool
                let tool_result = tools::execute_tool(tool_name, args, &cwd).await;

                // Emit tool activity completion
                let status = if tool_result.is_error {
                    "error"
                } else {
                    "completed"
                };
                sse.emit_tool_activity(task_id, tool_id, tool_name, &summary, status)
                    .await;

                // Add tool result to history
                self.history.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tool_id,
                    "content": tool_result.content,
                }));
            }
        }

        // Extract last assistant content as summary
        let summary = self
            .history
            .iter()
            .rev()
            .find(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("assistant"))
            .and_then(|msg| msg.get("content").and_then(|c| c.as_str()))
            .map(|s| {
                if s.len() > 500 {
                    format!("{}...", &s[..500])
                } else {
                    s.to_string()
                }
            });

        Ok(AgentRunResult {
            success: true,
            pr_url: None,
            error: None,
            summary,
            changes_data: None,
        })
    }

    /// Build the system prompt for the agent.
    fn build_system_prompt(&self) -> String {
        let cwd = self.options.cwd.to_string_lossy();

        if self.is_sub_agent {
            return format!(
                "You are a research sub-agent with full codebase access.\n\
                Working directory: {cwd}\n\n\
                Available tools: read_file, glob, grep, list_directory, bash, write_file, edit_file.\n\
                You CANNOT spawn sub-agents.\n\n\
                Investigate thoroughly, then provide a complete, structured answer.\n\
                Your final message must contain the full answer — it will be returned to the parent agent."
            );
        }

        format!(
            "You are an autonomous coding agent executing a task in a software project.\n\
            Working directory: {cwd}\n\n\
            TOOL USAGE:\n\
            - Read before editing: always use read_file before edit_file to see current content\n\
            - Use edit_file for modifications to existing files, write_file only for new files\n\
            - Use glob/grep to find files before reading them\n\
            - Use bash for git, npm, cargo, and other CLI operations\n\
            - Execute one logical step at a time, verify results, then proceed\n\
            - Use sub_agent for deep research tasks that require extensive file exploration\n\n\
            RULES:\n\
            - Be concise. Show relevant code, skip obvious explanations.\n\
            - When done with the task, provide a brief summary of what was accomplished.\n\
            - If you encounter errors, try to fix them. If stuck, explain what went wrong.\n\
            - Do NOT ask questions — you are autonomous. Make reasonable decisions.\n\
            - Focus on completing the task described in the prompt."
        )
    }

    /// Extract tool calls from the stream result.
    ///
    /// Tries structured JSON tool_calls first, falls back to XML parsing.
    /// Does NOT modify or parse the content text.
    fn parse_tool_calls(&self, result: &StreamResult) -> Vec<AccumulatedToolCall> {
        // If structured JSON tool_calls exist, use those
        if !result.tool_calls.is_empty() {
            return result.tool_calls.clone();
        }

        // XML fallback: parse content for tool calls only
        if !result.content.is_empty() {
            let parsed = xml_parser::parse_model_output(&result.content);
            if !parsed.tool_calls.is_empty() {
                return parsed
                    .tool_calls
                    .iter()
                    .enumerate()
                    .map(|(i, tc)| {
                        let args_json: Value = tc
                            .arguments
                            .iter()
                            .map(|(k, v)| (k.clone(), xml_parser::coerce_arg(v)))
                            .collect::<serde_json::Map<String, Value>>()
                            .into();
                        AccumulatedToolCall {
                            id: format!("xml_call_{}", timestamp_ms() + i as u64),
                            call_type: "function".to_string(),
                            function: crate::agent::api_client::ToolCallFunction {
                                name: tc.name.clone(),
                                arguments: serde_json::to_string(&args_json)
                                    .unwrap_or_default(),
                            },
                        }
                    })
                    .collect();
            }
        }

        vec![]
    }

    /// Handle the sub_agent tool call.
    async fn handle_sub_agent(
        &self,
        sse: &Arc<SSEEmitter>,
        task_id: &str,
        model: &str,
        cwd: &PathBuf,
        args: &Value,
    ) -> String {
        if self.is_sub_agent {
            return "Error: Sub-agents cannot spawn sub-agents".to_string();
        }

        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("No task specified");
        let max_turns = args
            .get("max_turns")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(20) as usize;

        sse.emit_log(
            task_id,
            "info",
            &format!("Spawning sub-agent (max_turns: {max_turns}): {}", &task[..task.len().min(100)]),
            None,
        )
        .await;

        let mut sub_runner = APIAgentRunner::new_sub_agent(
            &self.options.api_key,
            Some(model.to_string()),
            cwd.clone(),
            task_id.to_string(),
        );
        sub_runner.max_turns = max_turns;
        sub_runner.options.prompt = task.to_string();

        match Box::pin(sub_runner.run(sse)).await {
            Ok(result) => {
                let content = result.summary.unwrap_or_else(|| "Sub-agent completed without output.".to_string());
                sse.emit_log(task_id, "info", "Sub-agent completed", None).await;
                content
            }
            Err(e) => {
                let err = format!("Sub-agent error: {e}");
                sse.emit_log(task_id, "error", &err, None).await;
                err
            }
        }
    }

    /// Build messages for the API call, stripping old reasoning and truncating old tool results.
    fn build_messages_for_api(&self) -> Vec<Value> {
        let history_len = self.history.len();
        let recent_threshold = history_len.saturating_sub(MESSAGES_TO_KEEP);

        self.history
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                let mut msg = msg.clone();
                let is_old = i < recent_threshold;

                if is_old {
                    // Strip reasoning from old assistant messages
                    if msg.get("role").and_then(|r| r.as_str()) == Some("assistant") {
                        if let Some(obj) = msg.as_object_mut() {
                            obj.remove("reasoning_details");
                        }
                    }

                    // Truncate long tool results
                    if msg.get("role").and_then(|r| r.as_str()) == Some("tool") {
                        if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                            if content.len() > 4000 {
                                let end = content
                                    .char_indices()
                                    .nth(1500)
                                    .map(|(idx, _)| idx)
                                    .unwrap_or(content.len().min(1500));
                                msg["content"] = serde_json::json!(format!(
                                    "{}...\n[truncated, originally {} chars]",
                                    &content[..end],
                                    content.len()
                                ));
                            }
                        }
                    }
                }

                msg
            })
            .collect()
    }

    /// Compress history if estimated tokens exceed the threshold.
    async fn compress_history_if_needed(&mut self) {
        let estimated = self.estimate_history_tokens();
        if estimated < COMPRESSION_THRESHOLD {
            return;
        }

        info!(
            estimated_tokens = estimated,
            "Compressing API agent history"
        );

        let keep_count = MESSAGES_TO_KEEP.min(self.history.len());
        let split_point = self.history.len().saturating_sub(keep_count);
        if split_point <= 1 {
            // Keep at least the system prompt
            return;
        }

        // Phase 1: aggressive truncation of old messages
        for msg in &mut self.history[1..split_point] {
            let role = msg
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();

            match role.as_str() {
                "tool" => {
                    if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                        if content.len() > 200 {
                            msg["content"] =
                                serde_json::json!("[tool result truncated during compression]");
                        }
                    }
                }
                "assistant" => {
                    if let Some(obj) = msg.as_object_mut() {
                        obj.remove("reasoning_details");
                    }
                    if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                        if content.len() > 500 {
                            let end = content
                                .char_indices()
                                .nth(200)
                                .map(|(idx, _)| idx)
                                .unwrap_or(200);
                            msg["content"] = serde_json::json!(format!(
                                "{}...[compressed]",
                                &content[..end]
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        let after = self.estimate_history_tokens();
        warn!(
            before = estimated,
            after, "History compressed (phase 1 truncation)"
        );
    }

    /// Estimate total tokens using chars/4 heuristic.
    fn estimate_history_tokens(&self) -> usize {
        self.history
            .iter()
            .map(|msg| {
                let content_len = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                let tool_args_len = msg
                    .get("tool_calls")
                    .and_then(|t| serde_json::to_string(t).ok())
                    .map(|s| s.len())
                    .unwrap_or(0);
                (content_len + tool_args_len) / 4
            })
            .sum()
    }
}

/// Format a brief summary of a tool call for the UI.
fn format_tool_summary(name: &str, args: &Value) -> String {
    match name {
        "bash" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("...");
            let cmd_short = if cmd.len() > 60 {
                format!("{}...", &cmd[..60])
            } else {
                cmd.to_string()
            };
            format!("$ {cmd_short}")
        }
        "read_file" | "write_file" | "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("...");
            format!("{name}: {path}")
        }
        "glob" => {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("...");
            format!("glob: {pattern}")
        }
        "grep" => {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("...");
            format!("grep: {pattern}")
        }
        "list_directory" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            format!("ls: {path}")
        }
        "sub_agent" => {
            let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("...");
            let short = if task.len() > 60 {
                format!("{}...", &task[..60])
            } else {
                task.to_string()
            };
            format!("sub_agent: {short}")
        }
        _ => name.to_string(),
    }
}

fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
