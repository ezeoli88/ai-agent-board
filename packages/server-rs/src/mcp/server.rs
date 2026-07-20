//! MCP server that dispatches JSON-RPC 2.0 method calls.
//!
//! Operates in **stateless mode**: each POST request creates a fresh `McpServer`,
//! dispatches the request, and returns the response. No session management.

use serde_json::json;
use tracing::{debug, warn};

use crate::AppState;

use super::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use super::{resources, tools};

/// The MCP server dispatcher. Holds a reference to application state for the
/// duration of a single request.
pub struct McpServer<'a> {
    state: &'a AppState,
}

impl<'a> McpServer<'a> {
    /// Creates a new server instance bound to the given application state.
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Dispatches a JSON-RPC request to the appropriate handler.
    ///
    /// Supported methods:
    /// - `initialize` - Returns server capabilities
    /// - `tools/list` - Returns available tools
    /// - `tools/call` - Invokes a tool by name
    /// - `resources/list` - Returns available resource templates
    /// - `resources/read` - Reads a resource by URI
    /// - `notifications/initialized` - Acknowledged (no-op)
    /// - anything else - Returns method_not_found error
    pub async fn dispatch(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        debug!(method = %req.method, "MCP dispatch");

        match req.method.as_str() {
            "initialize" => self.handle_initialize(req.id),
            "tools/list" => self.handle_tools_list(req.id),
            "tools/call" => self.handle_tools_call(req.id, req.params).await,
            "resources/list" => self.handle_resources_list(req.id),
            "resources/read" => self.handle_resources_read(req.id, req.params).await,
            "notifications/initialized" => {
                // Notification acknowledgement - return empty success
                JsonRpcResponse::success(req.id, json!({}))
            }
            _ => {
                warn!(method = %req.method, "Unknown MCP method");
                JsonRpcResponse::method_not_found(req.id)
            }
        }
    }

    /// Handles `initialize` - returns server info and capabilities.
    fn handle_initialize(&self, id: Option<serde_json::Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {},
                    "resources": {}
                },
                "serverInfo": {
                    "name": "agent-board",
                    "version": "0.2.14"
                }
            }),
        )
    }

    /// Handles `tools/list` - returns all tool definitions.
    fn handle_tools_list(&self, id: Option<serde_json::Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(id, tools::tool_definitions())
    }

    /// Handles `tools/call` - dispatches to the named tool handler.
    async fn handle_tools_call(
        &self,
        id: Option<serde_json::Value>,
        params: Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let params = params.unwrap_or(json!({}));

        let tool_name = match params.get("name").and_then(|n| n.as_str()) {
            Some(name) => name.to_string(),
            None => {
                return JsonRpcResponse::invalid_params(id, "Missing 'name' in tools/call params");
            }
        };

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        debug!(tool = %tool_name, "Calling MCP tool");

        let result = tools::call_tool(self.state, &tool_name, arguments).await;
        JsonRpcResponse::success(id, result)
    }

    /// Handles `resources/list` - returns all resource definitions.
    fn handle_resources_list(&self, id: Option<serde_json::Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(id, resources::resource_definitions())
    }

    /// Handles `resources/read` - reads a resource by URI.
    async fn handle_resources_read(
        &self,
        id: Option<serde_json::Value>,
        params: Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let params = params.unwrap_or(json!({}));

        let uri = match params.get("uri").and_then(|u| u.as_str()) {
            Some(u) => u.to_string(),
            None => {
                return JsonRpcResponse::invalid_params(
                    id,
                    "Missing 'uri' in resources/read params",
                );
            }
        };

        debug!(uri = %uri, "Reading MCP resource");

        let result = resources::read_resource(self.state, &uri).await;
        JsonRpcResponse::success(id, result)
    }
}
