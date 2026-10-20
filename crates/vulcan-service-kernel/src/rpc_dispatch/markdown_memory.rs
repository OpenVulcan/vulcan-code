//! Memory mode and Markdown memory RPC routing.
//! 记忆模式与 Markdown 记忆 RPC 路由。

use super::*;

/// Exact public RPC wire names owned by the memory domain.
/// 记忆领域持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_get_memory_settings",
    "service_save_memory_settings",
    "service_load_markdown_memory",
    "service_write_markdown_memory",
    "service_read_markdown_memory_file",
    "service_replace_markdown_memory_file",
    "service_apply_markdown_memory_project_action",
];

/// Dispatches one exact memory RPC.
/// 分发一个精确记忆 RPC。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    let result = match method {
        "service_get_memory_settings" => service_get_memory_settings(),
        "service_save_memory_settings" => {
            service_save_memory_settings(state, rpc_required_param(params, "request")?)
        }
        "service_load_markdown_memory" => {
            service_load_markdown_memory(state, rpc_required_param(params, "request")?)
        }
        "service_write_markdown_memory" => {
            service_write_markdown_memory(state, rpc_required_param(params, "request")?)
        }
        "service_read_markdown_memory_file" => {
            service_read_markdown_memory_file(rpc_required_param(params, "request")?)
        }
        "service_replace_markdown_memory_file" => {
            service_replace_markdown_memory_file(rpc_required_param(params, "request")?)
        }
        "service_apply_markdown_memory_project_action" => {
            service_apply_markdown_memory_project_action(
                state,
                rpc_required_param(params, "request")?,
            )
        }
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(
        result.map_err(WebuiServiceRpcError::Service),
    ))
}
