//! Configuration RPC routing.
//! 配置 RPC 路由。

use super::*;
use crate::permission_judge;
use crate::system_prompt_settings::{
    service_reset_system_prompt, service_save_system_prompt, service_system_prompt_settings,
};
use crate::workbench_model_routes::service_save_tool_collapse_default;

/// Exact public RPC wire names owned by this domain router.
/// 本领域路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_select_workbench_reasoning_effort",
    "service_list_workbench_model_secondary_routes",
    "service_select_workbench_model_secondary_route",
    "service_select_workbench_thinking_display_mode",
    "service_save_tool_collapse_default",
    "service_select_webui_locale",
    "service_save_provider_request_trace_settings",
    "service_open_selected_provider_api_trace_directory",
    "service_clear_selected_provider_api_trace_directory",
    "service_save_provider_retry_settings",
    "service_output_style_catalog",
    "service_license_catalog",
    "service_license_body",
    "service_save_prompt_output_style",
    "service_save_prompt_answer_mode",
    "service_system_prompt_settings",
    "service_save_system_prompt",
    "service_reset_system_prompt",
    "service_public_ai_settings_snapshot",
    "service_openvulcan_account",
    "service_openvulcan_usage_page",
    "service_claim_openvulcan_prompt",
    "service_start_openvulcan_login",
    "service_openvulcan_login_status",
    "service_cancel_openvulcan_login",
    "service_commit_openvulcan_login",
    "service_logout_openvulcan_account",
    "service_provider_usage_dashboard",
    "service_start_public_ai_oauth",
    "service_public_ai_oauth_status",
    "service_cancel_public_ai_oauth",
    "service_commit_public_ai_oauth_connection",
    "service_add_public_ai_connection",
    "service_update_public_ai_connection",
    "service_save_public_ai_connection_enabled",
    "service_remove_public_ai_connection",
    "service_probe_public_ai_custom_provider",
    "service_save_public_ai_custom_provider",
    "service_validate_public_ai_custom_model",
    "service_remove_public_ai_custom_provider",
    "service_save_public_ai_custom_models",
    "service_save_public_ai_custom_model_protocol_preferences",
    "service_save_public_ai_custom_provider_model_defaults",
    "service_public_ai_custom_model_template_suggestions",
    "service_public_ai_custom_model_template_suggestions_batch",
    "service_save_public_ai_custom_model_template_decision",
    "service_save_public_ai_custom_model_template_decisions_batch",
    "service_save_public_ai_custom_model_connection_availability",
    "service_save_public_ai_connection_model_enabled",
    "service_save_public_ai_connection_models_enabled",
    "service_save_public_ai_connection_model_set_enabled",
    "service_save_public_ai_model_capability_overrides",
    "service_save_public_ai_provider_capability_overrides",
    "service_sync_public_ai_connection",
    "service_sync_provider_manifests",
    "service_get_provider_manifest_status",
    "service_preview_provider_manifest",
    "service_sync_provider_manifest",
    "service_rollback_provider_manifest",
    "service_query_public_ai_connection_usage",
    "service_save_public_ai_usage_refresh_settings",
    "service_save_public_ai_provider_priority",
    "service_save_public_ai_context_window_policy_overrides",
    "service_save_public_ai_media_delivery_policy",
    "service_reset_public_ai_media_delivery_policy",
    "service_save_public_ai_context_compaction_window_nodes",
    "service_save_public_ai_new_session_context_window_default",
    "service_ack_public_ai_model_takeover_notices",
    "service_select_public_ai_model",
    "service_save_public_ai_model_overrides",
    "service_save_public_ai_model_enabled",
    "service_save_public_ai_internal_model_enabled",
    "service_save_public_ai_model_set_enabled",
    "service_save_public_ai_internal_model_set_enabled",
    "service_save_public_ai_internal_model_overrides",
    "service_save_public_ai_inference_route",
    "service_test_public_ai_inference_route",
    "service_save_public_ai_provider_tool_selection",
    "service_save_public_ai_multimodal_understanding_candidate",
    "service_save_public_ai_media_understanding_selection",
    "service_save_public_ai_provider_extension_target_set",
    "service_test_public_ai_provider_tool",
    "service_list_provider_tasks",
    "service_cancel_provider_task",
    "service_save_public_ai_provider_tool_capability_enabled",
    "service_public_ai_session_provider_tool_selections",
    "service_save_public_ai_session_provider_tool_selections",
    "service_rebuild_public_ai_session_provider_tool_selections",
    "service_save_public_ai_default_connection",
    "service_save_git_commit_settings",
    "service_save_default_command_shell",
    "service_generate_git_commit_message",
    "service_run_git_commit_action",
    "service_run_git_pull_action",
    "service_save_mcp_server_configuration",
    "service_mcp_runtime_snapshot",
    "service_mcp_server_capabilities",
    "service_remove_mcp_server_configuration",
    "service_mcp_oauth_status",
    "service_start_mcp_oauth",
    "service_finish_mcp_oauth",
    "service_cancel_mcp_oauth",
    "service_remove_mcp_oauth",
    "service_permission_judge_settings",
    "service_permission_judge_status",
    "service_save_permission_judge_settings",
    "service_inspect_permission_judge_package",
    "service_register_permission_judge_model",
    "service_activate_permission_judge_model",
    "service_rollback_permission_judge_model",
    "service_remove_permission_judge_model",
    "service_reload_permission_judge",
    "service_benchmark_permission_judge",
    "service_install_permission_judge_cuda",
    "service_download_permission_judge_model",
    "service_permission_judge_records",
    "service_mark_permission_judge_record",
    "service_export_permission_judge_records",
    "service_clear_permission_judge_records",
    "service_project_permission_judge_policy",
    "service_save_project_permission_judge_policy",
    "service_unattended_system_default",
    "service_save_unattended_system_default",
    "service_review_prompt_system_default",
    "service_save_review_prompt_system_default",
    "service_session_review_prompt",
    "service_save_session_review_prompt",
    "service_project_unattended_default",
    "service_save_project_unattended_default",
    "service_review_rules_system_default",
    "service_save_review_rules_system_default",
    "service_project_review_rules",
    "service_save_project_review_rules",
    "service_subagent_models",
    "service_save_subagent_models",
    "service_session_subagent_models",
    "service_save_session_subagent_models",
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
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    // result preserves service failures inside Handled so the top-level return trace still runs.
    // result 把服务失败保留在 Handled 内，使顶层返回追踪仍会执行。
    let result = match method {
        "service_select_workbench_reasoning_effort" => rpc_json_result(
            service_select_workbench_reasoning_effort(
                app,
                state,
                rpc_required_param(params, "conversationId")?,
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
        "service_save_tool_collapse_default" => rpc_json_result(
            service_save_tool_collapse_default(app, state, rpc_required_param(params, "enabled")?)
                .await,
        ),
        "service_select_webui_locale" => rpc_json_result(
            service_select_webui_locale(app, state, rpc_required_param(params, "locale")?).await,
        ),
        "service_save_provider_request_trace_settings" => rpc_json_result(
            service_save_provider_request_trace_settings(
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
        "service_output_style_catalog" => {
            rpc_json_result(service_output_style_catalog(state).await)
        }
        "service_license_catalog" => {
            rpc_json_result(crate::licenses::service_license_catalog().await)
        }
        "service_license_body" => rpc_json_result(
            crate::licenses::service_license_body(
                rpc_required_param(params, "sectionId")?,
                rpc_required_param(params, "expectedSha256")?,
            )
            .await,
        ),
        "service_save_prompt_output_style" => rpc_json_result(
            service_save_prompt_output_style(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_save_prompt_answer_mode" => rpc_json_result(
            crate::answer_mode::service_save_prompt_answer_mode(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_system_prompt_settings" => rpc_json_result(
            service_system_prompt_settings(state, rpc_optional_param(params, "projectId")?).await,
        ),
        "service_save_system_prompt" => rpc_json_result(
            service_save_system_prompt(state, rpc_required_param(params, "request")?).await,
        ),
        "service_reset_system_prompt" => rpc_json_result(
            service_reset_system_prompt(state, rpc_required_param(params, "request")?).await,
        ),
        "service_public_ai_settings_snapshot" => {
            rpc_json_result(service_public_ai_settings_snapshot(state).await)
        }
        "service_openvulcan_usage_page" => rpc_json_result(
            crate::public_ai_settings::openvulcan_account::usage_page(
                state,
                rpc_required_param(params, "cursor")?,
            )
            .await,
        ),
        "service_openvulcan_account" => rpc_json_result(
            crate::public_ai_settings::openvulcan_account::snapshot(app, state).await,
        ),
        "service_claim_openvulcan_prompt" => {
            rpc_json_result(crate::public_ai_settings::openvulcan_account::claim_prompt().await)
        }
        "service_start_openvulcan_login" => rpc_json_result(
            crate::public_ai_settings::openvulcan_account::start(
                state,
                rpc_required_param(params, "region")?,
            )
            .await,
        ),
        "service_openvulcan_login_status" => rpc_json_result(
            crate::public_ai_settings::openvulcan_account::status(rpc_required_param(
                params,
                "transactionId",
            )?)
            .await,
        ),
        "service_cancel_openvulcan_login" => rpc_json_result(
            crate::public_ai_settings::openvulcan_account::cancel(rpc_required_param(
                params,
                "transactionId",
            )?)
            .await,
        ),
        "service_commit_openvulcan_login" => rpc_json_result(
            crate::public_ai_settings::openvulcan_account::commit(
                app,
                state,
                rpc_required_param(params, "transactionId")?,
            )
            .await,
        ),
        "service_logout_openvulcan_account" => {
            rpc_json_result(crate::public_ai_settings::openvulcan_account::logout(app, state).await)
        }
        "service_provider_usage_dashboard" => {
            rpc_json_result(service_provider_usage_dashboard(state).await)
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
        "service_save_public_ai_connection_enabled" => rpc_json_result(
            service_save_public_ai_connection_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
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
        "service_save_public_ai_custom_model_protocol_preferences" => rpc_json_result(
            service_save_public_ai_custom_model_protocol_preferences(
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
        "service_save_public_ai_connection_model_set_enabled" => rpc_json_result(
            service_save_public_ai_connection_model_set_enabled(
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
        "service_sync_provider_manifests" => {
            rpc_json_result(service_sync_provider_manifests(app, state).await)
        }
        "service_get_provider_manifest_status" => {
            rpc_json_result(service_get_provider_manifest_status(state).await)
        }
        "service_preview_provider_manifest" => rpc_json_result(
            service_preview_provider_manifest(state, rpc_required_param(params, "providerId")?)
                .await,
        ),
        "service_sync_provider_manifest" => rpc_json_result(
            service_sync_provider_manifest(app, state, rpc_required_param(params, "providerId")?)
                .await,
        ),
        "service_rollback_provider_manifest" => rpc_json_result(
            service_rollback_provider_manifest(
                app,
                state,
                rpc_required_param(params, "providerId")?,
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
        "service_open_selected_provider_api_trace_directory" => {
            rpc_json_result(service_open_selected_provider_api_trace_directory(state))
        }
        "service_clear_selected_provider_api_trace_directory" => {
            rpc_json_result(service_clear_selected_provider_api_trace_directory(state))
        }
        "service_save_public_ai_provider_priority" => rpc_json_result(
            service_save_public_ai_provider_priority(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_context_window_policy_overrides" => rpc_json_result(
            service_save_public_ai_context_window_policy_overrides(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_media_delivery_policy" => rpc_json_result(
            service_save_public_ai_media_delivery_policy(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_reset_public_ai_media_delivery_policy" => rpc_json_result(
            service_reset_public_ai_media_delivery_policy(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_context_compaction_window_nodes" => rpc_json_result(
            service_save_public_ai_context_compaction_window_nodes(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_new_session_context_window_default" => rpc_json_result(
            service_save_public_ai_new_session_context_window_default(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_ack_public_ai_model_takeover_notices" => rpc_json_result(
            service_ack_public_ai_model_takeover_notices(
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
        "service_save_public_ai_internal_model_enabled" => rpc_json_result(
            service_save_public_ai_internal_model_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_model_set_enabled" => rpc_json_result(
            service_save_public_ai_model_set_enabled(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_internal_model_set_enabled" => rpc_json_result(
            service_save_public_ai_internal_model_set_enabled(
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
        "service_save_public_ai_multimodal_understanding_candidate" => rpc_json_result(
            service_save_public_ai_multimodal_understanding_candidate(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_media_understanding_selection" => rpc_json_result(
            service_save_public_ai_media_understanding_selection(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_public_ai_provider_extension_target_set" => rpc_json_result(
            service_save_public_ai_provider_extension_target_set(
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
        "service_list_provider_tasks" => rpc_json_result(service_list_provider_tasks(state)),
        "service_cancel_provider_task" => rpc_json_result(
            service_cancel_provider_task(
                state,
                rpc_required_param(params, "executionId")?,
                rpc_optional_param(params, "ownerSessionId")?,
            )
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
        "service_public_ai_session_provider_tool_selections" => rpc_json_result(
            service_public_ai_session_provider_tool_selections(
                app,
                state,
                rpc_required_param(params, "conversationId")?,
            )
            .await,
        ),
        "service_save_public_ai_session_provider_tool_selections" => rpc_json_result(
            service_save_public_ai_session_provider_tool_selections(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_rebuild_public_ai_session_provider_tool_selections" => rpc_json_result(
            service_rebuild_public_ai_session_provider_tool_selections(
                app,
                state,
                rpc_required_param(params, "conversationId")?,
            )
            .await,
        ),
        "service_save_public_ai_default_connection" => rpc_json_result(
            service_save_public_ai_default_connection(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_default_command_shell" => rpc_json_result(
            service_save_default_command_shell(app, state, rpc_required_param(params, "request")?)
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
        "service_run_git_pull_action" => rpc_json_result(
            service_run_git_pull_action(state, rpc_required_param(params, "request")?).await,
        ),
        "service_save_mcp_server_configuration" => rpc_json_result(
            service_save_mcp_server_configuration(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_mcp_runtime_snapshot" => rpc_json_result(service_mcp_runtime_snapshot(state)),
        "service_mcp_server_capabilities" => rpc_json_result(
            service_mcp_server_capabilities(state, rpc_required_param(params, "serverId")?).await,
        ),
        "service_remove_mcp_server_configuration" => rpc_json_result(
            service_remove_mcp_server_configuration(
                app,
                state,
                rpc_required_param(params, "serverId")?,
            )
            .await,
        ),
        "service_mcp_oauth_status" => rpc_json_result(
            crate::mcp_oauth::service_mcp_oauth_status(
                state,
                rpc_required_param(params, "serverId")?,
            )
            .await,
        ),
        "service_start_mcp_oauth" => rpc_json_result(
            crate::mcp_oauth::service_start_mcp_oauth(
                state,
                rpc_required_param(params, "serverId")?,
            )
            .await,
        ),
        "service_finish_mcp_oauth" => rpc_json_result(
            crate::mcp_oauth::service_finish_mcp_oauth(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_cancel_mcp_oauth" => rpc_json_result(
            crate::mcp_oauth::service_cancel_mcp_oauth(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_remove_mcp_oauth" => rpc_json_result(
            crate::mcp_oauth::service_remove_mcp_oauth(
                state,
                rpc_required_param(params, "serverId")?,
            )
            .await,
        ),
        "service_permission_judge_settings" => {
            rpc_json_result(permission_judge::service_permission_judge_settings(state).await)
        }
        "service_permission_judge_status" => {
            rpc_json_result(permission_judge::service_permission_judge_status(state).await)
        }
        "service_save_permission_judge_settings" => rpc_json_result(
            permission_judge::service_save_permission_judge_settings(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_inspect_permission_judge_package" => rpc_json_result(
            permission_judge::service_inspect_permission_judge_package(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_register_permission_judge_model" => rpc_json_result(
            permission_judge::service_register_permission_judge_model(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_activate_permission_judge_model" => rpc_json_result(
            permission_judge::service_activate_permission_judge_model(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_rollback_permission_judge_model" => {
            rpc_json_result(permission_judge::service_rollback_permission_judge_model(state).await)
        }
        "service_remove_permission_judge_model" => rpc_json_result(
            permission_judge::service_remove_permission_judge_model(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_reload_permission_judge" => {
            rpc_json_result(permission_judge::service_reload_permission_judge(state).await)
        }
        "service_permission_judge_records" => rpc_json_result(
            permission_judge::service_permission_judge_records(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_mark_permission_judge_record" => rpc_json_result(
            permission_judge::service_mark_permission_judge_record(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_export_permission_judge_records" => {
            rpc_json_result(permission_judge::service_export_permission_judge_records(state).await)
        }
        "service_clear_permission_judge_records" => {
            rpc_json_result(permission_judge::service_clear_permission_judge_records(state).await)
        }
        "service_benchmark_permission_judge" => rpc_json_result(
            permission_judge::service_benchmark_permission_judge(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_install_permission_judge_cuda" => rpc_json_result(
            permission_judge::service_install_permission_judge_cuda(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_download_permission_judge_model" => rpc_json_result(
            permission_judge::service_download_permission_judge_model(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_project_permission_judge_policy" => rpc_json_result(
            permission_judge::service_project_permission_judge_policy(
                state,
                rpc_required_param(params, "projectId")?,
            )
            .await,
        ),
        "service_save_project_permission_judge_policy" => rpc_json_result(
            permission_judge::service_save_project_permission_judge_policy(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_unattended_system_default" => {
            rpc_json_result(permission_judge::service_unattended_system_default(state))
        }
        "service_save_unattended_system_default" => rpc_json_result(
            permission_judge::service_save_unattended_system_default(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_review_prompt_system_default" => rpc_json_result(
            permission_judge::service_review_prompt_system_default(state),
        ),
        "service_save_review_prompt_system_default" => rpc_json_result(
            permission_judge::service_save_review_prompt_system_default(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_session_review_prompt" => rpc_json_result(
            permission_judge::service_session_review_prompt(
                state,
                rpc_required_param(params, "sessionId")?,
            )
            .await,
        ),
        "service_save_session_review_prompt" => rpc_json_result(
            permission_judge::service_save_session_review_prompt(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_project_unattended_default" => {
            rpc_json_result(permission_judge::service_project_unattended_default(
                state,
                rpc_required_param(params, "projectId")?,
            ))
        }
        "service_save_project_unattended_default" => {
            rpc_json_result(permission_judge::service_save_project_unattended_default(
                state,
                rpc_required_param(params, "request")?,
            ))
        }
        "service_review_rules_system_default" => {
            rpc_json_result(permission_judge::service_review_rules_system_default(state))
        }
        "service_subagent_models" => {
            rpc_json_result(crate::subagent_models::service_subagent_models(state))
        }
        "service_save_subagent_models" => rpc_json_result(
            crate::subagent_models::service_save_subagent_models(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_session_subagent_models" => rpc_json_result(
            crate::subagent_models::service_session_subagent_models(
                state,
                rpc_required_param(params, "sessionId")?,
            )
            .await,
        ),
        "service_save_session_subagent_models" => rpc_json_result(
            crate::subagent_models::service_save_session_subagent_models(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_review_rules_system_default" => rpc_json_result(
            permission_judge::service_save_review_rules_system_default(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_project_review_rules" => {
            rpc_json_result(permission_judge::service_project_review_rules(
                state,
                rpc_required_param(params, "projectId")?,
            ))
        }
        "service_save_project_review_rules" => {
            rpc_json_result(permission_judge::service_save_project_review_rules(
                state,
                rpc_required_param(params, "request")?,
            ))
        }
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
