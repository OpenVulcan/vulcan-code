// Mirror of `service/mcp-settings-service.ts`.
// `service/mcp-settings-service.ts` 的镜像。
//
// Saving and removing MCP servers lives in `settings_actions`, as in the WebUI.
// MCP 服务器的保存与移除位于 `settings_actions`，与 WebUI 一致。

use serde_json::{Value, json};

use super::{ServiceCall, Slot};

/// Builds the request for `service_mcp_runtime_snapshot`.
/// 构建 `service_mcp_runtime_snapshot` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_mcp_runtime_snapshot() -> ServiceCall {
    ServiceCall::load(
        "load_mcp_runtime_snapshot",
        "service_mcp_runtime_snapshot",
        Value::Null,
        Slot::McpRuntime,
    )
}

/// Builds the request for `service_mcp_server_capabilities`.
/// 构建 `service_mcp_server_capabilities` 的请求。
/// `server_id` is the configured MCP server identity.
/// `server_id` 为已配置的 MCP 服务器身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_mcp_server_capabilities(server_id: &str) -> ServiceCall {
    ServiceCall::load(
        "load_mcp_server_capabilities",
        "service_mcp_server_capabilities",
        json!({ "serverId": server_id }),
        Slot::McpCapabilities,
    )
}
