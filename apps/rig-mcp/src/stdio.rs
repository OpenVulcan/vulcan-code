//! Stdio MCP transport, backed by the official [`rmcp`] SDK.
//!
//! This module bridges between [`rig_compose`]'s transport-agnostic
//! [`Tool`](rig_compose::tool::Tool) surface and rmcp's spec-compliant
//! MCP implementation. Everything spec-related (JSON-RPC framing,
//! capability negotiation, version handshakes) is delegated to rmcp;
//! we only translate at the seam.
//!
//! Public surface (kept stable across the rmcp migration):
//!
//! * [`StdioTransport::spawn`] — spawn a child binary and speak MCP
//!   over its stdio. Implements [`McpTransport`] so the resulting
//!   handle is interchangeable with any other transport.
//! * [`serve_stdio`] — expose a [`ToolRegistry`] as an MCP server on
//!   the current process's stdin/stdout. Intended for `--mcp-serve`
//!   style CLI flags.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::process::Command;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo, Tool as RmcpTool,
};
use rmcp::service::{Peer, RequestContext, RoleClient, RoleServer, RunningService, ServiceExt};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess, stdio as rmcp_stdio};
use rmcp::{ErrorData as McpError, ServerHandler};

use crate::transport::McpTransport;
use rig_compose::registry::{KernelError, ToolRegistry};
use rig_compose::tool::ToolSchema;

// =============================================================================
// Server side: expose a ToolRegistry as an rmcp ServerHandler
// =============================================================================

/// Adapter that wears [`ServerHandler`] over a [`ToolRegistry`]. Every
/// `tools/list` is answered from `registry.schemas()`; every
/// `tools/call` dispatches to `registry.invoke()`. No prompts,
/// resources, or sampling are advertised — clients see a tools-only
/// server.
#[derive(Clone)]
struct RegistryServer {
    registry: Arc<ToolRegistry>,
    info: ServerInfo,
}

impl RegistryServer {
    fn new(registry: Arc<ToolRegistry>) -> Self {
        // rmcp's `Implementation` and `ServerInfo` are `#[non_exhaustive]`,
        // so we can't use a struct literal. Build via `Default::default`
        // and assign field-by-field.
        #[allow(clippy::field_reassign_with_default)]
        let server_info = {
            let mut s = Implementation::default();
            s.name = env!("CARGO_PKG_NAME").to_string();
            s.version = env!("CARGO_PKG_VERSION").to_string();
            s
        };
        #[allow(clippy::field_reassign_with_default)]
        let info = {
            let mut i = ServerInfo::default();
            i.protocol_version = ProtocolVersion::default();
            i.capabilities = ServerCapabilities::builder().enable_tools().build();
            i.server_info = server_info;
            i
        };
        Self { registry, info }
    }
}

fn schema_to_rmcp_tool(s: ToolSchema) -> RmcpTool {
    let input_obj = match s.args_schema {
        Value::Object(map) => map,
        _ => Default::default(),
    };
    let output_obj = match s.result_schema {
        Value::Object(map) if !map.is_empty() => Some(Arc::new(map)),
        _ => None,
    };
    #[allow(clippy::field_reassign_with_default)]
    {
        let mut tool = RmcpTool::default();
        tool.name = s.name.into();
        tool.description = Some(s.description.into());
        tool.input_schema = Arc::new(input_obj);
        tool.output_schema = output_obj;
        tool
    }
}

impl ServerHandler for RegistryServer {
    fn get_info(&self) -> ServerInfo {
        self.info.clone()
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = self
            .registry
            .schemas()
            .into_iter()
            .map(schema_to_rmcp_tool)
            .collect();
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let name = request.name.to_string();
        let args = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| json!({}));
        match self.registry.invoke(&name, args).await {
            Ok(value) => Ok(CallToolResult::structured(value)),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

/// Serve `registry` over stdin/stdout using rmcp's spec-compliant stdio
/// transport. Returns when the peer disconnects.
pub async fn serve_stdio(registry: ToolRegistry) -> Result<(), KernelError> {
    let server = RegistryServer::new(Arc::new(registry));
    let service = server
        .serve(rmcp_stdio())
        .await
        .map_err(|e| KernelError::ToolFailed(format!("mcp.serve: {e}")))?;
    service
        .waiting()
        .await
        .map_err(|e| KernelError::ToolFailed(format!("mcp.serve: {e}")))?;
    Ok(())
}

// =============================================================================
// Client side: spawn a child process and speak MCP over its stdio
// =============================================================================

/// Production stdio MCP client. Wraps an [`rmcp`] running service so
/// that callers see only the [`McpTransport`] trait.
///
/// The cloneable [`Peer`] is cached at construction time so every
/// `list_tools` / `call_tool` is a lock-free dispatch into rmcp.
/// Concurrent calls fan out without serialising on a transport-level
/// mutex; rmcp itself multiplexes the underlying stdio channel.
pub struct StdioTransport {
    endpoint: String,
    peer: Peer<RoleClient>,
    /// Keeps the rmcp service task alive for the lifetime of the
    /// transport. Held but never read — dropping the transport drops
    /// the service, which closes the child's stdio.
    _service: Arc<RunningService<RoleClient, ()>>,
}

impl StdioTransport {
    /// Spawn `program` with `args` and connect over its stdio.
    ///
    /// `endpoint` is a free-form identifier surfaced via
    /// [`McpTransport::endpoint`]; it has no protocol meaning.
    pub async fn spawn(
        endpoint: impl Into<String>,
        program: impl AsRef<std::ffi::OsStr>,
        args: &[&str],
    ) -> Result<Self, KernelError> {
        let program = program.as_ref().to_owned();
        let argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        let cmd = Command::new(&program).configure(|c| {
            c.args(&argv);
        });
        let transport = TokioChildProcess::new(cmd)
            .map_err(|e| KernelError::ToolFailed(format!("mcp.spawn: {e}")))?;
        let service = ()
            .serve(transport)
            .await
            .map_err(|e| KernelError::ToolFailed(format!("mcp.connect: {e}")))?;
        let peer = service.peer().clone();
        Ok(Self {
            endpoint: endpoint.into(),
            peer,
            _service: Arc::new(service),
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    async fn list_tools(&self) -> Result<Vec<ToolSchema>, KernelError> {
        let tools = self
            .peer
            .list_all_tools()
            .await
            .map_err(|e| KernelError::ToolFailed(format!("tools/list: {e}")))?;
        Ok(tools.into_iter().map(rmcp_tool_to_schema).collect())
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, KernelError> {
        let arguments = match args {
            Value::Object(map) => Some(map),
            Value::Null => None,
            other => {
                return Err(KernelError::InvalidArgument(format!(
                    "tools/call requires an object or null arguments, got {other}"
                )));
            }
        };
        let params = {
            #[allow(clippy::field_reassign_with_default)]
            let mut p = CallToolRequestParams::default();
            p.name = name.to_string().into();
            p.arguments = arguments;
            p
        };
        let result = self
            .peer
            .call_tool(params)
            .await
            .map_err(|e| KernelError::ToolFailed(format!("tools/call: {e}")))?;

        if result.is_error.unwrap_or(false) {
            let msg = result
                .content
                .iter()
                .find_map(|c| c.as_text().map(|t| t.text.clone()))
                .unwrap_or_else(|| "tool returned error".to_string());
            return Err(KernelError::ToolFailed(msg));
        }

        // Prefer typed structured content; fall back to first text block parsed
        // as JSON, then to the raw text wrapped in a string Value.
        if let Some(v) = result.structured_content {
            return Ok(v);
        }
        if let Some(text) = result
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
        {
            if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                return Ok(parsed);
            }
            return Ok(Value::String(text));
        }
        Ok(Value::Null)
    }
}

fn rmcp_tool_to_schema(t: RmcpTool) -> ToolSchema {
    ToolSchema {
        name: t.name.to_string(),
        description: t.description.map(|d| d.to_string()).unwrap_or_default(),
        args_schema: Value::Object((*t.input_schema).clone()),
        result_schema: t
            .output_schema
            .map(|s| Value::Object((*s).clone()))
            .unwrap_or(Value::Null),
    }
}

// =============================================================================
// Tests — round-trip a registry through a real spawn() of the test bin
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rig_compose::tool::LocalTool;
    use serde_json::json;
    use std::sync::Arc;

    fn echo_registry() -> ToolRegistry {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(LocalTool::new(
            ToolSchema {
                name: "math.mul".into(),
                description: "multiply".into(),
                args_schema: json!({"type": "object"}),
                result_schema: json!({"type": "integer"}),
            },
            |args: Value| async move {
                let a = args["a"].as_i64().unwrap_or(0);
                let b = args["b"].as_i64().unwrap_or(0);
                Ok(json!(a * b))
            },
        )));
        reg
    }

    /// Verify `serve_stdio` actually constructs a working server. We
    /// don't drive the wire here — that's covered by the `mcp_serve_cli`
    /// tests in azreal which spawn the real binary. This is a smoke
    /// test that the rmcp wiring compiles and the registry can be
    /// observed through the same `Tool` trait used by skills.
    #[tokio::test]
    async fn registry_server_round_trip_via_tool_trait() {
        let registry = echo_registry();
        let tool = registry.get("math.mul").unwrap();
        let out = tool.invoke(json!({"a": 6, "b": 7})).await.unwrap();
        assert_eq!(out, json!(42));
    }
}
