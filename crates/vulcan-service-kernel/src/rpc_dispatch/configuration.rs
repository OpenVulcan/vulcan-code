//! Configuration RPC routing.
//! 配置 RPC 路由。

use super::*;

/// Exact public RPC wire names owned by this domain router.
/// 本领域路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_select_workbench_model",
    "service_list_workbench_model_secondary_routes",
    "service_select_workbench_model_secondary_route",
    "service_select_workbench_reasoning_effort",
    "service_select_workbench_thinking_display_mode",
    "service_select_webui_locale",
    "service_save_provider_configuration",
    "service_remove_provider_configuration",
    "service_save_provider_enabled_models",
    "service_save_provider_model_override",
    "service_save_provider_retry_settings",
    "service_save_git_commit_settings",
    "service_generate_git_commit_message",
    "service_run_git_commit_action",
    "service_save_ai_capability_vendor",
    "service_remove_ai_capability_vendor",
    "service_set_ai_capability_target_enabled",
    "service_save_ai_capability_targets",
    "service_save_session_ai_capability_configuration",
    "service_save_mcp_server_configuration",
    "service_remove_mcp_server_configuration",
];

/// Dispatches one RPC method owned by this domain.
/// 分发本领域持有的单个 RPC 方法。
///
/// # Parameters
/// 参数
/// `app` emits configuration events, `state` owns service data, `method` is the exact wire name, and `params` is normalized.
/// `app` 发出配置事件，`state` 持有服务数据，`method` 是精确线协议名称，`params` 已归一化。
///
/// # Returns
/// 返回值
/// Returns `Handled` with the service result, `Unknown` for another domain, or a parameter parsing error.
/// 返回携带服务结果的 `Handled`、表示其他领域的 `Unknown`，或参数解析错误。
pub(super) async fn dispatch(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, String> {
    // result preserves service failures inside Handled so the top-level return trace still runs.
    // result 把服务失败保留在 Handled 内，使顶层返回追踪仍会执行。
    let result = match method {
        "service_select_workbench_model" => rpc_json_result(
            service_select_workbench_model(
                app,
                state,
                rpc_required_param(params, "providerId")?,
                rpc_required_param(params, "modelId")?,
            )
            .await,
        ),
        "service_list_workbench_model_secondary_routes" => rpc_json_result(
            service_list_workbench_model_secondary_routes(
                state,
                rpc_required_param(params, "providerId")?,
                rpc_required_param(params, "modelId")?,
            )
            .await,
        ),
        "service_select_workbench_model_secondary_route" => rpc_json_result(
            service_select_workbench_model_secondary_route(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_select_workbench_reasoning_effort" => rpc_json_result(
            service_select_workbench_reasoning_effort(
                app,
                state,
                rpc_required_param(params, "effort")?,
            )
            .await,
        ),
        "service_select_workbench_thinking_display_mode" => rpc_json_result(
            service_select_workbench_thinking_display_mode(
                app,
                state,
                rpc_required_param(params, "mode")?,
            )
            .await,
        ),
        "service_select_webui_locale" => rpc_json_result(
            service_select_webui_locale(app, state, rpc_required_param(params, "locale")?).await,
        ),
        "service_save_provider_configuration" => rpc_json_result(
            service_save_provider_configuration(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_remove_provider_configuration" => rpc_json_result(
            service_remove_provider_configuration(
                app,
                state,
                rpc_required_param(params, "providerId")?,
            )
            .await,
        ),
        "service_save_provider_enabled_models" => rpc_json_result(
            service_save_provider_enabled_models(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_provider_model_override" => rpc_json_result(
            service_save_provider_model_override(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_provider_retry_settings" => rpc_json_result(
            service_save_provider_retry_settings(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_git_commit_settings" => rpc_json_result(
            service_save_git_commit_settings(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_generate_git_commit_message" => rpc_json_result(
            service_generate_git_commit_message(state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_run_git_commit_action" => rpc_json_result(
            service_run_git_commit_action(state, rpc_required_param(params, "request")?).await,
        ),
        "service_save_ai_capability_vendor" => rpc_json_result(
            service_save_ai_capability_vendor(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_remove_ai_capability_vendor" => rpc_json_result(
            service_remove_ai_capability_vendor(
                app,
                state,
                rpc_required_param(params, "vendorId")?,
            )
            .await,
        ),
        "service_set_ai_capability_target_enabled" => rpc_json_result(
            service_set_ai_capability_target_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_ai_capability_targets" => rpc_json_result(
            service_save_ai_capability_targets(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_save_session_ai_capability_configuration" => rpc_json_result(
            service_save_session_ai_capability_configuration(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_mcp_server_configuration" => rpc_json_result(
            service_save_mcp_server_configuration(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_remove_mcp_server_configuration" => rpc_json_result(
            service_remove_mcp_server_configuration(
                app,
                state,
                rpc_required_param(params, "serverId")?,
            )
            .await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
