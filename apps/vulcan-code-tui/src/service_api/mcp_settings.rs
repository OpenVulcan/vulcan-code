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

const OAUTH: &[Slot] = &[Slot::McpOAuth];

/// Builds the request for `service_mcp_oauth_status`.
/// 构建 `service_mcp_oauth_status` 的请求。
/// `server_id` is the configured MCP server identity.
/// `server_id` 为已配置的 MCP 服务器身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_mcp_oauth_status(server_id: &str) -> ServiceCall {
    ServiceCall::load(
        "load_mcp_oauth_status",
        "service_mcp_oauth_status",
        json!({ "serverId": server_id }),
        Slot::McpOAuth,
    )
}

/// Builds the request for `service_start_mcp_oauth`, which replaces any earlier flow.
/// 构建 `service_start_mcp_oauth` 的请求，它会替换先前的流程。
/// `server_id` is the configured MCP server identity.
/// `server_id` 为已配置的 MCP 服务器身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn start_mcp_oauth(server_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "start_mcp_oauth",
        "service_start_mcp_oauth",
        json!({ "serverId": server_id }),
        OAUTH,
    )
}

/// Builds the request for `service_finish_mcp_oauth` with the URL the browser was redirected to.
/// 使用浏览器被重定向到的 URL 构建 `service_finish_mcp_oauth` 的请求。
/// `request` is `{serverId, flowId, callbackUrl}`.
/// `request` 为 `{serverId, flowId, callbackUrl}`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn finish_mcp_oauth(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "finish_mcp_oauth",
        "service_finish_mcp_oauth",
        json!({ "request": request }),
        OAUTH,
    )
}

/// Builds the request for `service_cancel_mcp_oauth`, which releases the flow's callback port.
/// 构建 `service_cancel_mcp_oauth` 的请求，它会释放流程的回调端口。
/// `server_id` is the configured MCP server identity.
/// `server_id` 为已配置的 MCP 服务器身份。
/// `flow_id` is the exact flow being cancelled.
/// `flow_id` 为要取消的准确流程。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn cancel_mcp_oauth(server_id: &str, flow_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "cancel_mcp_oauth",
        "service_cancel_mcp_oauth",
        json!({ "request": { "serverId": server_id, "flowId": flow_id } }),
        OAUTH,
    )
}

/// Builds the request for `service_remove_mcp_oauth`, which signs out by deleting stored credentials.
/// 构建 `service_remove_mcp_oauth` 的请求，它通过删除已存储凭据退出登录。
/// `server_id` is the configured MCP server identity.
/// `server_id` 为已配置的 MCP 服务器身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn remove_mcp_oauth(server_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "remove_mcp_oauth",
        "service_remove_mcp_oauth",
        json!({ "serverId": server_id }),
        OAUTH,
    )
}
