//! Model Context Protocol transport abstraction.
//!
//! The kernel is transport-agnostic: a [`Tool`] is a typed async
//! function regardless of whether it runs in-process or behind a remote
//! MCP server. This module defines the trait that real MCP transports
//! (stdio, http+SSE, websocket) implement, plus an [`McpTool`] adapter
//! that turns any transport into a kernel [`Tool`].
//!
//! A concrete [`LoopbackTransport`] is included so the abstraction can be
//! exercised end-to-end in tests without an external MCP crate. Production
//! transports (`rmcp`, custom stdio, etc.) plug in by implementing
//! [`McpTransport`] — no kernel changes required.
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use rig_compose::registry::{KernelError, ToolRegistry};
use rig_compose::tool::{Tool, ToolSchema};

/// Bidirectional MCP transport. Real implementations layer JSON-RPC
/// framing, capability negotiation, and reconnection on top of this; the
/// kernel sees only `list_tools` + `call_tool`.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Stable identifier for this transport instance (typically the
    /// server URI or stdio command).
    fn endpoint(&self) -> &str;

    /// Discover the tools exposed by the remote endpoint. Called at
    /// registration time; the returned schemas are authoritative.
    async fn list_tools(&self) -> Result<Vec<ToolSchema>, KernelError>;

    /// Invoke a named tool. Implementations MUST round-trip the result
    /// JSON without modification so callers can rely on schema fidelity.
    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, KernelError>;
}

/// Kernel-facing wrapper that exposes one tool from a remote MCP server
/// as a local [`Tool`]. Skills cannot tell `McpTool` apart from a local
/// `rig_compose::Tool` implementation.
pub struct McpTool {
    transport: Arc<dyn McpTransport>,
    schema: ToolSchema,
}

impl McpTool {
    pub fn new(transport: Arc<dyn McpTransport>, schema: ToolSchema) -> Self {
        Self { transport, schema }
    }

    /// Discover all tools exposed by `transport` and wrap each as an
    /// [`McpTool`]. Register the returned vec with a
    /// `rig_compose::registry::ToolRegistry` to merge them into a global
    /// registry.
    pub async fn from_transport(
        transport: Arc<dyn McpTransport>,
    ) -> Result<Vec<Arc<dyn Tool>>, KernelError> {
        let schemas = transport.list_tools().await?;
        Ok(schemas
            .into_iter()
            .map(|schema| {
                let t: Arc<dyn Tool> = Arc::new(McpTool {
                    transport: transport.clone(),
                    schema,
                });
                t
            })
            .collect())
    }
}

#[async_trait]
impl Tool for McpTool {
    fn schema(&self) -> ToolSchema {
        self.schema.clone()
    }

    fn name(&self) -> rig_compose::tool::ToolName {
        self.schema.name.clone()
    }

    async fn invoke(&self, args: Value) -> Result<Value, KernelError> {
        self.transport.call_tool(&self.schema.name, args).await
    }
}

// =============================================================================
// LoopbackTransport — in-process transport over a local ToolRegistry
// =============================================================================

/// Pure-Rust transport that round-trips calls through a local
/// [`ToolRegistry`]. Useful for testing the MCP composition story without
/// spawning an external process.
///
/// `LoopbackTransport` also doubles as the building block for
/// `McpToolServer`-style exports in a future commit: any registry can be
/// wrapped in a transport and then attached to a real MCP server crate.
pub struct LoopbackTransport {
    endpoint: String,
    registry: ToolRegistry,
}

impl LoopbackTransport {
    pub fn new(endpoint: impl Into<String>, registry: ToolRegistry) -> Self {
        Self {
            endpoint: endpoint.into(),
            registry,
        }
    }
}

#[async_trait]
impl McpTransport for LoopbackTransport {
    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    async fn list_tools(&self) -> Result<Vec<ToolSchema>, KernelError> {
        // The registry doesn't expose iteration directly to keep the
        // trait surface small; for the loopback transport we walk the
        // inner DashMap via a public helper added below.
        Ok(self.registry.schemas())
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, KernelError> {
        self.registry.invoke(name, args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig_compose::tool::LocalTool;
    use serde_json::json;

    fn make_registry() -> ToolRegistry {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(LocalTool::new(
            ToolSchema {
                name: "math.add".into(),
                description: "add two ints".into(),
                args_schema: json!({"type": "object"}),
                result_schema: json!({"type": "integer"}),
            },
            |args| async move {
                let a = args["a"].as_i64().unwrap_or(0);
                let b = args["b"].as_i64().unwrap_or(0);
                Ok(json!(a + b))
            },
        )));
        reg
    }

    #[tokio::test]
    async fn loopback_transport_round_trip() {
        let server = make_registry();
        let transport: Arc<dyn McpTransport> =
            Arc::new(LoopbackTransport::new("loopback://test", server));

        let schemas = transport.list_tools().await.unwrap();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "math.add");

        let result = transport
            .call_tool("math.add", json!({"a": 2, "b": 3}))
            .await
            .unwrap();
        assert_eq!(result, json!(5));
    }

    #[tokio::test]
    async fn mcp_tool_indistinguishable_from_local() {
        // Register the local tool on a server-side registry, expose it
        // via loopback, and re-register the wrapped McpTool on a client
        // registry. Calls through the client registry must produce the
        // same result as direct local invocation.
        let server = make_registry();
        let transport: Arc<dyn McpTransport> =
            Arc::new(LoopbackTransport::new("loopback://test", server));

        let client = ToolRegistry::new();
        for tool in McpTool::from_transport(transport).await.unwrap() {
            client.register(tool);
        }

        let out = client
            .invoke("math.add", json!({"a": 10, "b": 32}))
            .await
            .unwrap();
        assert_eq!(out, json!(42));
    }
}
