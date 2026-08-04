//! Configuration RPC routing.
//! 配置 RPC 路由。

use super::*;

/// Exact public RPC wire names owned by this domain router.
/// 本领域路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_select_workbench_reasoning_effort",
    "service_list_workbench_model_secondary_routes",
    "service_select_workbench_model_secondary_route",
    "service_select_workbench_thinking_display_mode",
    "service_select_webui_locale",
    "service_save_provider_retry_settings",
    "service_output_style_catalog",
    "service_save_prompt_output_style",
    "service_public_ai_settings_snapshot",
    "service_start_public_ai_oauth",
    "service_public_ai_oauth_status",
    "service_cancel_public_ai_oauth",
    "service_commit_public_ai_oauth_connection",
    "service_add_public_ai_connection",
    "service_update_public_ai_connection",
    "service_remove_public_ai_connection",
    "service_probe_public_ai_custom_provider",
    "service_save_public_ai_custom_provider",
    "service_validate_public_ai_custom_model",
    "service_remove_public_ai_custom_provider",
    "service_save_public_ai_custom_models",
    "service_save_public_ai_custom_provider_model_defaults",
    "service_public_ai_custom_model_template_suggestions",
    "service_public_ai_custom_model_template_suggestions_batch",
    "service_save_public_ai_custom_model_template_decision",
    "service_save_public_ai_custom_model_template_decisions_batch",
    "service_save_public_ai_custom_model_connection_availability",
    "service_save_public_ai_connection_model_enabled",
    "service_save_public_ai_connection_models_enabled",
    "service_save_public_ai_model_capability_overrides",
    "service_save_public_ai_provider_capability_overrides",
    "service_sync_public_ai_connection",
    "service_query_public_ai_connection_usage",
    "service_save_public_ai_usage_refresh_settings",
    "service_select_public_ai_model",
    "service_save_public_ai_model_overrides",
    "service_save_public_ai_model_enabled",
    "service_save_public_ai_provider_models_enabled",
    "service_save_public_ai_internal_model_enabled",
    "service_save_public_ai_internal_model_overrides",
    "service_save_public_ai_inference_route",
    "service_test_public_ai_inference_route",
    "service_save_public_ai_provider_tool_selection",
    "service_test_public_ai_provider_tool",
    "service_save_public_ai_provider_tool_capability_enabled",
    "service_public_ai_session_provider_tool_selections",
    "service_save_public_ai_session_provider_tool_selections",
    "service_save_public_ai_default_connection",
    "service_save_git_commit_settings",
    "service_generate_git_commit_message",
    "service_run_git_commit_action",
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
        "service_select_workbench_reasoning_effort" => rpc_json_result(
            service_select_workbench_reasoning_effort(
                app,
                state,
                rpc_required_param(params, "effort")?,
            )
            .await,
        ),
        "service_list_workbench_model_secondary_routes" => rpc_json_result(
            service_list_workbench_model_secondary_routes(
                state,
                rpc_required_param(params, "providerId")?,
                rpc_required_param(params, "modelId")?,
                rpc_optional_param(params, "forceRefresh")?.unwrap_or(false),
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
        "service_save_provider_retry_settings" => rpc_json_result(
            service_save_provider_retry_settings(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_output_style_catalog" => {
            rpc_json_result(service_output_style_catalog(state).await)
        }
        "service_save_prompt_output_style" => rpc_json_result(
            service_save_prompt_output_style(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_public_ai_settings_snapshot" => {
            rpc_json_result(service_public_ai_settings_snapshot(state).await)
        }
        "service_start_public_ai_oauth" => rpc_json_result(
            service_start_public_ai_oauth(state, rpc_required_param(params, "request")?).await,
        ),
        "service_public_ai_oauth_status" => rpc_json_result(
            service_public_ai_oauth_status(state, rpc_required_param(params, "transactionId")?)
                .await,
        ),
        "service_cancel_public_ai_oauth" => rpc_json_result(
            service_cancel_public_ai_oauth(state, rpc_required_param(params, "transactionId")?)
                .await,
        ),
        "service_commit_public_ai_oauth_connection" => rpc_json_result(
            service_commit_public_ai_oauth_connection(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_add_public_ai_connection" => rpc_json_result(
            service_add_public_ai_connection(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_update_public_ai_connection" => rpc_json_result(
            service_update_public_ai_connection(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_remove_public_ai_connection" => rpc_json_result(
            service_remove_public_ai_connection(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_probe_public_ai_custom_provider" => rpc_json_result(
            service_probe_public_ai_custom_provider(state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_save_public_ai_custom_provider" => rpc_json_result(
            service_save_public_ai_custom_provider(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_validate_public_ai_custom_model" => rpc_json_result(
            service_validate_public_ai_custom_model(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_remove_public_ai_custom_provider" => rpc_json_result(
            service_remove_public_ai_custom_provider(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_custom_models" => rpc_json_result(
            service_save_public_ai_custom_models(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_custom_provider_model_defaults" => rpc_json_result(
            service_save_public_ai_custom_provider_model_defaults(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_public_ai_custom_model_template_suggestions" => {
            rpc_json_result(service_public_ai_custom_model_template_suggestions(
                rpc_required_param(params, "request")?,
            ))
        }
        "service_public_ai_custom_model_template_suggestions_batch" => {
            rpc_json_result(service_public_ai_custom_model_template_suggestions_batch(
                rpc_required_param(params, "request")?,
            ))
        }
        "service_save_public_ai_custom_model_template_decision" => rpc_json_result(
            service_save_public_ai_custom_model_template_decision(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_custom_model_template_decisions_batch" => rpc_json_result(
            service_save_public_ai_custom_model_template_decisions_batch(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_custom_model_connection_availability" => rpc_json_result(
            service_save_public_ai_custom_model_connection_availability(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_connection_model_enabled" => rpc_json_result(
            service_save_public_ai_connection_model_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_connection_models_enabled" => rpc_json_result(
            service_save_public_ai_connection_models_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_model_capability_overrides" => rpc_json_result(
            service_save_public_ai_model_capability_overrides(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_provider_capability_overrides" => rpc_json_result(
            service_save_public_ai_provider_capability_overrides(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_sync_public_ai_connection" => rpc_json_result(
            service_sync_public_ai_connection(
                app,
                state,
                rpc_required_param(params, "connectionId")?,
            )
            .await,
        ),
        "service_query_public_ai_connection_usage" => rpc_json_result(
            service_query_public_ai_connection_usage(
                state,
                rpc_required_param(params, "connectionId")?,
            )
            .await,
        ),
        "service_save_public_ai_usage_refresh_settings" => rpc_json_result(
            service_save_public_ai_usage_refresh_settings(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_select_public_ai_model" => rpc_json_result(
            service_select_public_ai_model(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_save_public_ai_model_overrides" => rpc_json_result(
            service_save_public_ai_model_overrides(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_model_enabled" => rpc_json_result(
            service_save_public_ai_model_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_provider_models_enabled" => rpc_json_result(
            service_save_public_ai_provider_models_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_internal_model_enabled" => rpc_json_result(
            service_save_public_ai_internal_model_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_internal_model_overrides" => rpc_json_result(
            service_save_public_ai_internal_model_overrides(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_inference_route" => rpc_json_result(
            service_save_public_ai_inference_route(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_test_public_ai_inference_route" => rpc_json_result(
            service_test_public_ai_inference_route(state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_save_public_ai_provider_tool_selection" => rpc_json_result(
            service_save_public_ai_provider_tool_selection(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_test_public_ai_provider_tool" => rpc_json_result(
            service_test_public_ai_provider_tool(state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_save_public_ai_provider_tool_capability_enabled" => rpc_json_result(
            service_save_public_ai_provider_tool_capability_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_public_ai_session_provider_tool_selections" => {
            rpc_json_result(service_public_ai_session_provider_tool_selections(
                app,
                state,
                rpc_required_param(params, "conversationId")?,
            ))
        }
        "service_save_public_ai_session_provider_tool_selections" => {
            rpc_json_result(service_save_public_ai_session_provider_tool_selections(
                app,
                state,
                rpc_required_param(params, "request")?,
            ))
        }
        "service_save_public_ai_default_connection" => rpc_json_result(
            service_save_public_ai_default_connection(
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
