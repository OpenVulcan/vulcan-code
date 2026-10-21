// Mirror of `service/mcp-service-service.ts`. Creating a client and rotating its token return a
// token that the terminal never shows (see the secret rule in this crate's notes), so those two
// functions are offered only by the WebUI.
// `service/mcp-service-service.ts` 的镜像。新建客户端与轮换令牌会返回终端从不显示的令牌（见本 crate
// 说明中的机密规则），因此这两个函数只由 WebUI 提供。

use serde_json::{Value, json};

use super::{ServiceCall, Slot};

/// Builds the request for `service_mcp_service_snapshot`.
/// 构建 `service_mcp_service_snapshot` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_mcp_service() -> ServiceCall {
    ServiceCall::load(
        "load_mcp_service",
        "service_mcp_service_snapshot",
        json!({}),
        Slot::McpService,
    )
}

/// Builds the request for `service_set_mcp_service_enabled`; clients are left unchanged.
/// 构建 `service_set_mcp_service_enabled` 的请求；客户端保持不变。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn set_mcp_service_enabled(enabled: bool) -> ServiceCall {
    ServiceCall::mutate(
        "set_mcp_service_enabled",
        "service_set_mcp_service_enabled",
        json!({ "enabled": enabled }),
        &[Slot::McpService],
    )
}

/// Builds the request for `service_update_mcp_client`; the token is unchanged.
/// 构建 `service_update_mcp_client` 的请求；令牌保持不变。
/// `request` is the complete `{ clientId, name, enabled, sessions, mediaTargets }` document.
/// `request` 为完整的 `{ clientId, name, enabled, sessions, mediaTargets }` 文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn update_mcp_client(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "update_mcp_client",
        "service_update_mcp_client",
        json!({ "request": request }),
        &[Slot::McpService],
    )
}

/// Builds the request for `service_delete_mcp_client`; its token stops working.
/// 构建 `service_delete_mcp_client` 的请求；其令牌失效。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn delete_mcp_client(client_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "delete_mcp_client",
        "service_delete_mcp_client",
        json!({ "clientId": client_id }),
        &[Slot::McpService],
    )
}
