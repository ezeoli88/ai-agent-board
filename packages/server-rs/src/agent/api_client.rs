//! MiniMax API client with SSE streaming.
//!
//! Handles HTTP communication with the MiniMax chat completions API,
//! including SSE stream parsing, tool call delta accumulation, and error mapping.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::utils::SSEEmitter;

const BASE_URL: &str = "https://api.minimax.io/v1";

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccumulatedToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct StreamResult {
    pub content: String,
    pub reasoning_details: Vec<String>,
    pub tool_calls: Vec<AccumulatedToolCall>,
    pub usage: Usage,
    pub finish_reason: String,
}

// ── Client ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct MiniMaxClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl MiniMaxClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.to_string(),
            base_url: BASE_URL.to_string(),
        }
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key)).unwrap(),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("X-Reasoning-Split", HeaderValue::from_static("true"));
        headers
    }

    /// Stream a chat completion, emitting SSE events for the frontend.
    ///
    /// Content chunks are emitted as chat_message events. Tool call deltas
    /// are accumulated internally and returned in the final result.
    pub async fn stream_chat(
        &self,
        model: &str,
        messages: &[Value],
        tools: Option<&[Value]>,
        sse_emitter: &Arc<SSEEmitter>,
        task_id: &str,
        cancel: CancellationToken,
    ) -> Result<StreamResult> {
        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
            "temperature": 0.3,
        });

        if let Some(tools) = tools {
            if !tools.is_empty() {
                body["tools"] = serde_json::json!(tools);
                body["tool_choice"] = serde_json::json!("auto");
            }
        }

        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if let Ok(json) = serde_json::from_str::<Value>(&text) {
                if let Some(mapped) = parse_minimax_error(&json) {
                    return Err(anyhow!("API error {}: {}", status, mapped));
                }
            }
            return Err(anyhow!("API error {}: {}", status, text));
        }

        let mut content = String::new();
        let mut reasoning_details: Vec<String> = Vec::new();
        let mut tool_calls_map: HashMap<usize, AccumulatedToolCall> = HashMap::new();
        let mut usage = Usage::default();
        let mut finish_reason = String::new();
        let mut chunk_count: u64 = 0;

        // Content buffer for batching SSE emissions (emit every ~200 chars)
        let mut content_buffer = String::new();

        use futures::StreamExt;
        let mut stream = response.bytes_stream();
        let mut line_buffer = String::new();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    break;
                }
                chunk = stream.next() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            line_buffer.push_str(&String::from_utf8_lossy(&bytes));

                            while let Some(line_end) = line_buffer.find('\n') {
                                let line = line_buffer[..line_end].trim_end_matches('\r').to_string();
                                line_buffer = line_buffer[line_end + 1..].to_string();

                                if line.is_empty() || line.starts_with(':') {
                                    continue;
                                }

                                if let Some(data) = line.strip_prefix("data: ") {
                                    let data = data.trim();
                                    if data == "[DONE]" {
                                        continue;
                                    }

                                    if let Ok(chunk_json) = serde_json::from_str::<Value>(data) {
                                        chunk_count += 1;
                                        process_chunk(
                                            &chunk_json,
                                            &mut content,
                                            &mut content_buffer,
                                            &mut reasoning_details,
                                            &mut tool_calls_map,
                                            &mut usage,
                                            &mut finish_reason,
                                            sse_emitter,
                                            task_id,
                                        ).await;
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            let msg = format!("Stream error: {e}");
                            sse_emitter.emit_log(task_id, "error", &msg, None).await;
                            return Err(anyhow!(msg));
                        }
                        None => break,
                    }
                }
            }
        }

        // Flush remaining content buffer
        if !content_buffer.is_empty() {
            // Content is accumulated in `content` already, no need to emit partial here
        }

        if chunk_count == 0 && content.is_empty() && tool_calls_map.is_empty() {
            sse_emitter
                .emit_log(
                    task_id,
                    "warn",
                    "No response received from API (0 chunks)",
                    None,
                )
                .await;
        }

        let tool_calls: Vec<AccumulatedToolCall> = {
            let mut entries: Vec<(usize, AccumulatedToolCall)> =
                tool_calls_map.into_iter().collect();
            entries.sort_by_key(|(k, _)| *k);
            entries.into_iter().map(|(_, v)| v).collect()
        };

        Ok(StreamResult {
            content,
            reasoning_details,
            tool_calls,
            usage,
            finish_reason,
        })
    }

    /// Simple non-streaming completion for internal use (context summarization).
    pub async fn simple_completion(&self, model: &str, prompt: &str) -> Result<String> {
        let body = serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false,
            "temperature": 0.3,
            "max_tokens": 2000,
        });

        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("API error {status}: {text}"));
        }

        let data: Value = response.json().await?;
        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(content)
    }
}

// ── Chunk Processing ────────────────────────────────────────────────────

async fn process_chunk(
    chunk: &Value,
    content: &mut String,
    content_buffer: &mut String,
    reasoning_details: &mut Vec<String>,
    tool_calls_map: &mut HashMap<usize, AccumulatedToolCall>,
    usage: &mut Usage,
    finish_reason: &mut String,
    sse_emitter: &Arc<SSEEmitter>,
    task_id: &str,
) {
    // Usage
    if let Some(u) = chunk.get("usage") {
        usage.prompt_tokens = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        usage.completion_tokens = u
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        usage.total_tokens = u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    }

    // API-level error
    if let Some(err) = chunk.get("error") {
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown API error");
        sse_emitter
            .emit_log(task_id, "error", &format!("API error: {msg}"), None)
            .await;
        return;
    }

    let choice = match chunk.get("choices").and_then(|c| c.get(0)) {
        Some(c) => c,
        None => return,
    };

    if let Some(fr) = choice.get("finish_reason").and_then(|v| v.as_str()) {
        *finish_reason = fr.to_string();
    }

    let delta = match choice.get("delta") {
        Some(d) => d,
        None => return,
    };

    // Reasoning details
    if let Some(rd) = delta.get("reasoning_details").and_then(|v| v.as_array()) {
        for item in rd {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                reasoning_details.push(text.to_string());
            }
        }
    }
    if let Some(rc) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
        reasoning_details.push(rc.to_string());
    }

    // Content
    if let Some(c) = delta.get("content").and_then(|v| v.as_str()) {
        content.push_str(c);
        content_buffer.push_str(c);
    }

    // Tool calls
    if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in tcs {
            let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

            let entry = tool_calls_map
                .entry(idx)
                .or_insert_with(|| AccumulatedToolCall {
                    id: String::new(),
                    call_type: "function".to_string(),
                    function: ToolCallFunction {
                        name: String::new(),
                        arguments: String::new(),
                    },
                });

            if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                entry.id = id.to_string();
            }
            if let Some(func) = tc.get("function") {
                if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                    entry.function.name = name.to_string();
                }
                if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                    entry.function.arguments.push_str(args);
                }
            }
        }
    }
}

// ── Error Parsing ───────────────────────────────────────────────────────

fn parse_minimax_error(data: &Value) -> Option<String> {
    if let Some(base_resp) = data.get("base_resp").or_else(|| data.get("baseResp")) {
        if let Some(code) = read_u64_field(base_resp, &["status_code", "statusCode", "code"]) {
            if code != 0 {
                let msg = base_resp
                    .get("status_msg")
                    .or_else(|| base_resp.get("statusMessage"))
                    .or_else(|| base_resp.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                return Some(format_minimax_error(code, msg));
            }
        }
    }

    if let Some(error) = data.get("error") {
        if let Some(code) = read_u64_field(error, &["code", "status_code", "statusCode"]) {
            if code != 0 {
                let msg = error
                    .get("message")
                    .or_else(|| error.get("msg"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                return Some(format_minimax_error(code, msg));
            }
        }
    }

    None
}

fn format_minimax_error(code: u64, msg: &str) -> String {
    match minimax_error_solution(code) {
        Some(solution) => format!("MiniMax error {code}: {msg}. {solution}"),
        None => format!("MiniMax error {code}: {msg}"),
    }
}

fn minimax_error_solution(code: u64) -> Option<&'static str> {
    match code {
        1000 | 1001 | 1002 | 1024 | 1033 | 1039 => Some("Please retry your request later."),
        1004 | 2049 => Some("Check your API key and make sure it is correct and active."),
        1008 => Some("Check your account balance."),
        1026 | 1027 => Some("Change your input content."),
        1041 => Some("Connection limit reached; contact MiniMax if the issue persists."),
        2056 => Some("Usage limit exceeded; wait for the next 5-hour window."),
        _ => None,
    }
}

fn read_u64_field(data: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(value) = data.get(*key) {
            if let Some(v) = value.as_u64() {
                return Some(v);
            }
            if let Some(s) = value.as_str() {
                if let Ok(v) = s.parse::<u64>() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Validates a MiniMax API key via a minimal chat completion request.
///
/// The chat/completions endpoint reliably returns HTTP 401 for invalid keys
/// and HTTP 200 for valid ones. We use `max_tokens: 1` to minimize cost.
pub async fn validate_minimax_key(api_key: &str) -> Result<bool> {
    let client = reqwest::Client::new();
    let url = format!("{BASE_URL}/chat/completions");

    let body = serde_json::json!({
        "model": "MiniMax-M2.5",
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1,
        "stream": false,
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(15))
        .json(&body)
        .send()
        .await?;

    let status = resp.status();

    // HTTP 200 = key is valid (got a response)
    // HTTP 429 = key is valid but rate-limited (only authenticated keys get 429)
    if status.is_success() || status.as_u16() == 429 {
        return Ok(true);
    }

    // HTTP 401 = invalid key
    if status.as_u16() == 401 {
        return Ok(false);
    }

    // Other HTTP errors — parse body for details
    let text = resp.text().await.unwrap_or_default();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(mapped) = parse_minimax_error(&json) {
            return Err(anyhow!("{}", mapped));
        }
    }

    Err(anyhow!(
        "Unexpected response from MiniMax API: HTTP {}",
        status
    ))
}
