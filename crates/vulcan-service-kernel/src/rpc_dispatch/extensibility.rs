//! Extensibility RPC routing.
//! 扩展能力 RPC 路由。

use super::*;

/// Exact public RPC wire names owned by this domain router.
/// 本领域路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_refresh_luaskills_catalog",
    "service_describe_luaskill_config",
    "service_validate_luaskill_config",
    "service_set_luaskill_config",
    "service_delete_luaskill_config",
    "service_refresh_luaskill_config",
    "service_poll_luaskill_config_events",
    "service_inspect_local_plugin",
    "service_install_local_plugin",
    "service_update_local_plugin",
    "service_uninstall_user_plugin",
    "service_plugin_pending_operations",
    "service_continue_plugin_pending_operation",
    "service_set_vulcan_plugin_credentials",
    "service_load_plugin_ui_module",
    "service_read_plugin_text_asset",
    "service_invoke_plugin_action",
    "service_plugin_action_status",
    "service_save_luaskills_runtime_settings",
    "service_retry_luaskills_managed_runtimes",
    "service_install_luaskill",
    "service_update_luaskill",
    "service_luaskill_operation_progress",
    "service_set_luaskill_enabled",
    "service_set_vulcan_plugin_enabled",
    "service_reset_vulcan_plugin_config",
    "service_plugin_environment_ready",
    "service_prepare_plugin_environment",
    "service_start_plugin_environment_install",
    "service_plugin_environment_install_status",
    "service_plugin_environment_install_status_for_plugin",
    "service_uninstall_luaskill",
    "service_set_luaskill_tool_registration_state",
    "service_refresh_agent_skills_catalog",
    "service_open_agent_skills_user_directory",
    "service_save_agent_skill",
    "service_delete_agent_skill",
    "service_restore_agent_skill_default",
    "service_save_agent_skill_category",
    "service_delete_agent_skill_category",
    "service_set_agent_skill_category",
    "service_refresh_subagent_catalog",
    "service_open_subagent_user_directory",
    "service_set_subagent_definition_enabled",
    "service_save_subagent_definition",
    "service_delete_subagent_definition",
    "service_restore_subagent_definition_default",
    "service_set_agent_skill_enabled",
    "service_load_project_agent_skill_overrides",
    "service_set_agent_skill_project_state",
    "service_run_vmm_memory_action",
    "service_load_project_vmm_config",
    "service_preview_project_attachment_cleanup",
    "service_execute_project_attachment_cleanup",
    "service_save_vmm_memory_config",
];

/// Dispatches one RPC method owned by this domain.
/// 分发本领域持有的单个 RPC 方法。
///
/// # Parameters
/// 参数
/// `app` emits domain events, `state` owns service data, `method` is the exact wire name, and `params` is normalized.
/// `app` 发出领域事件，`state` 持有服务数据，`method` 是精确线协议名称，`params` 已归一化。
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
        "service_refresh_luaskills_catalog" => {
            rpc_json_result(service_refresh_luaskills_catalog(app.clone(), state).await)
        }
        "service_describe_luaskill_config" => rpc_json_result(
            service_describe_luaskill_config(state, rpc_required_param(params, "skillId")?).await,
        ),
        "service_validate_luaskill_config" => rpc_json_result(
            service_validate_luaskill_config(state, rpc_required_param(params, "skillId")?).await,
        ),
        "service_set_luaskill_config" => rpc_json_result(
            service_set_luaskill_config(app.clone(), state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_delete_luaskill_config" => rpc_json_result(
            service_delete_luaskill_config(
                app.clone(),
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_refresh_luaskill_config" => rpc_json_result(
            service_refresh_luaskill_config(
                app.clone(),
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_poll_luaskill_config_events" => rpc_json_result(
            service_poll_luaskill_config_events(
                app.clone(),
                state,
                rpc_optional_param(params, "afterSequence")?,
            )
            .await,
        ),
        "service_inspect_local_plugin" => rpc_json_result(
            service_inspect_local_plugin(state, rpc_required_param(params, "sourcePath")?).await,
        ),
        "service_install_local_plugin" => rpc_json_result(
            service_install_local_plugin(
                app.clone(),
                state,
                rpc_required_param(params, "sourcePath")?,
                rpc_required_param(params, "contentFingerprint")?,
                rpc_required_param(params, "approvedPermissions")?,
                rpc_required_param(params, "allowRegistryDependencies")?,
                rpc_required_param(params, "initialSecrets")?,
            )
            .await,
        ),
        "service_update_local_plugin" => rpc_json_result(
            service_update_local_plugin(
                app.clone(),
                state,
                rpc_required_param(params, "sourcePath")?,
                rpc_required_param(params, "contentFingerprint")?,
                rpc_required_param(params, "approvedPermissions")?,
                rpc_required_param(params, "allowRegistryDependencies")?,
                rpc_required_param(params, "initialSecrets")?,
            )
            .await,
        ),
        "service_uninstall_user_plugin" => rpc_json_result(
            service_uninstall_user_plugin(
                app.clone(),
                state,
                rpc_required_param(params, "pluginId")?,
            )
            .await,
        ),
        "service_plugin_pending_operations" => {
            rpc_json_result(service_plugin_pending_operations(state).await)
        }
        "service_continue_plugin_pending_operation" => {
            // The whole params object is the typed continuation; invalid kind/action pairs fail here.
            // 整个 params 对象即类型明确的继续请求；无效类别与动作组合在此失败。
            let request: PluginPendingContinuation = serde_json::from_value(params.clone())
                .map_err(|error| {
                    WebuiServiceRpcError::InvalidParams(format!(
                        "invalid params for service_continue_plugin_pending_operation: {error}"
                    ))
                })?;
            rpc_json_result(
                service_continue_plugin_pending_operation(app.clone(), state, request).await,
            )
        }
        "service_set_vulcan_plugin_credentials" => rpc_json_result(
            service_set_vulcan_plugin_credentials(
                app.clone(),
                state,
                rpc_required_param(params, "pluginId")?,
                rpc_required_param(params, "changes")?,
            )
            .await,
        ),
        "service_load_plugin_ui_module" => rpc_json_result(service_load_plugin_ui_module(
            state,
            rpc_required_param(params, "pluginId")?,
            rpc_required_param(params, "mountId")?,
        )),
        "service_read_plugin_text_asset" => rpc_json_result(service_read_plugin_text_asset(
            state,
            rpc_required_param(params, "pluginId")?,
            rpc_required_param(params, "path")?,
        )),
        "service_invoke_plugin_action" => rpc_json_result(
            service_invoke_plugin_action(
                state,
                rpc_required_param(params, "pluginId")?,
                rpc_required_param(params, "actionId")?,
                rpc_required_param(params, "arguments")?,
                rpc_optional_param(params, "workspacePath")?,
                rpc_required_param(params, "runtimeIdentity")?,
                rpc_required_param(params, "identity")?,
            )
            .await,
        ),
        "service_plugin_action_status" => {
            rpc_json_result(crate::plugin_services::service_plugin_action_status(
                state,
                rpc_required_param(params, "pluginId")?,
                rpc_required_param(params, "runtimeIdentity")?,
                rpc_required_param(params, "identity")?,
                rpc_required_param(params, "cancel")?,
            ))
        }
        "service_save_luaskills_runtime_settings" => rpc_json_result(
            service_save_luaskills_runtime_settings(
                app.clone(),
                state,
                rpc_required_param(params, "settings")?,
            )
            .await,
        ),
        "service_retry_luaskills_managed_runtimes" => rpc_json_result(
            service_retry_luaskills_managed_runtimes(
                app.clone(),
                state,
                rpc_required_param(params, "target")?,
            )
            .await,
        ),
        "service_install_luaskill" => rpc_json_result(
            service_install_luaskill(
                app.clone(),
                state,
                rpc_optional_param(params, "skillId")?,
                rpc_required_param(params, "source")?,
                rpc_required_param(params, "sourceType")?,
            )
            .await,
        ),
        "service_update_luaskill" => rpc_json_result(
            service_update_luaskill(app.clone(), state, rpc_required_param(params, "skillId")?)
                .await,
        ),
        "service_luaskill_operation_progress" => {
            let correlation_id: String = rpc_required_param(params, "correlationId")?;
            rpc_json_result(luaskill_operation_progress(correlation_id.as_str()))
        }
        "service_set_luaskill_enabled" => rpc_json_result(
            service_set_luaskill_enabled(
                app.clone(),
                state,
                rpc_required_param(params, "skillId")?,
                rpc_required_param(params, "enabled")?,
            )
            .await,
        ),
        "service_set_vulcan_plugin_enabled" => rpc_json_result(
            service_set_vulcan_plugin_enabled(
                app.clone(),
                state,
                rpc_required_param(params, "pluginId")?,
                rpc_required_param(params, "enabled")?,
            )
            .await,
        ),
        "service_reset_vulcan_plugin_config" => rpc_json_result(
            service_reset_vulcan_plugin_config(
                app.clone(),
                state,
                rpc_required_param(params, "pluginId")?,
            )
            .await,
        ),
        "service_plugin_environment_ready" => rpc_json_result(
            service_plugin_environment_ready(state, rpc_required_param(params, "pluginId")?).await,
        ),
        "service_prepare_plugin_environment" => rpc_json_result(
            service_prepare_plugin_environment(state, rpc_required_param(params, "pluginId")?)
                .await,
        ),
        "service_start_plugin_environment_install" => rpc_json_result(
            service_start_plugin_environment_install(
                state,
                rpc_required_param(params, "pluginId")?,
            )
            .await,
        ),
        "service_plugin_environment_install_status" => rpc_json_result(
            service_plugin_environment_install_status(state, rpc_required_param(params, "taskId")?),
        ),
        "service_plugin_environment_install_status_for_plugin" => {
            rpc_json_result(service_plugin_environment_install_status_for_plugin(
                state,
                rpc_required_param(params, "pluginId")?,
            ))
        }
        "service_uninstall_luaskill" => rpc_json_result(
            service_uninstall_luaskill(app.clone(), state, rpc_required_param(params, "skillId")?)
                .await,
        ),
        "service_set_luaskill_tool_registration_state" => rpc_json_result(
            service_set_luaskill_tool_registration_state(
                app.clone(),
                state,
                rpc_required_param(params, "skillId")?,
                rpc_required_param(params, "localName")?,
                rpc_required_param(params, "registrationState")?,
            )
            .await,
        ),
        "service_refresh_agent_skills_catalog" => {
            rpc_json_result(service_refresh_agent_skills_catalog(app.clone(), state).await)
        }
        "service_open_agent_skills_user_directory" => {
            rpc_json_result(service_open_agent_skills_user_directory(app.clone(), state).await)
        }
        "service_save_agent_skill" => rpc_json_result(
            service_save_agent_skill(app.clone(), state, rpc_required_param(params, "skill")?)
                .await,
        ),
        "service_delete_agent_skill" => rpc_json_result(
            service_delete_agent_skill(app.clone(), state, rpc_required_param(params, "name")?)
                .await,
        ),
        "service_restore_agent_skill_default" => rpc_json_result(
            service_restore_agent_skill_default(
                app.clone(),
                state,
                rpc_required_param(params, "name")?,
            )
            .await,
        ),
        "service_save_agent_skill_category" => rpc_json_result(
            service_save_agent_skill_category(
                app.clone(),
                state,
                rpc_required_param(params, "category")?,
            )
            .await,
        ),
        "service_delete_agent_skill_category" => rpc_json_result(
            service_delete_agent_skill_category(
                app.clone(),
                state,
                rpc_required_param(params, "categoryId")?,
            )
            .await,
        ),
        "service_set_agent_skill_category" => rpc_json_result(
            service_set_agent_skill_category(
                app.clone(),
                state,
                rpc_required_param(params, "name")?,
                rpc_required_param(params, "categoryId")?,
            )
            .await,
        ),
        "service_refresh_subagent_catalog" => {
            rpc_json_result(service_refresh_subagent_catalog(app.clone(), state).await)
        }
        "service_open_subagent_user_directory" => {
            rpc_json_result(service_open_subagent_user_directory(app.clone(), state).await)
        }
        "service_set_subagent_definition_enabled" => rpc_json_result(
            service_set_subagent_definition_enabled(
                app.clone(),
                state,
                rpc_required_param(params, "agentId")?,
                rpc_required_param(params, "enabled")?,
            )
            .await,
        ),
        "service_save_subagent_definition" => rpc_json_result(
            service_save_subagent_definition(
                app.clone(),
                state,
                rpc_required_param(params, "definition")?,
            )
            .await,
        ),
        "service_delete_subagent_definition" => rpc_json_result(
            service_delete_subagent_definition(
                app.clone(),
                state,
                rpc_required_param(params, "agentId")?,
            )
            .await,
        ),
        "service_restore_subagent_definition_default" => rpc_json_result(
            service_restore_subagent_definition_default(
                app.clone(),
                state,
                rpc_required_param(params, "agentId")?,
            )
            .await,
        ),
        "service_set_agent_skill_enabled" => rpc_json_result(
            service_set_agent_skill_enabled(
                app.clone(),
                state,
                rpc_required_param(params, "name")?,
                rpc_required_param(params, "enabled")?,
            )
            .await,
        ),
        "service_load_project_agent_skill_overrides" => {
            rpc_json_result(service_load_project_agent_skill_overrides(
                state,
                rpc_required_param(params, "projectId")?,
            ))
        }
        "service_set_agent_skill_project_state" => rpc_json_result(
            service_set_agent_skill_project_state(
                app.clone(),
                state,
                rpc_required_param(params, "projectId")?,
                rpc_required_param(params, "name")?,
                rpc_required_param(params, "stateKey")?,
            )
            .await,
        ),
        "service_run_vmm_memory_action" => rpc_json_result(
            service_run_vmm_memory_action(
                app.clone(),
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_load_project_vmm_config" => rpc_json_result(service_load_project_vmm_config(
            state,
            rpc_required_param(params, "projectId")?,
        )),
        "service_preview_project_attachment_cleanup" => rpc_json_result(
            service_preview_project_attachment_cleanup(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_execute_project_attachment_cleanup" => rpc_json_result(
            service_execute_project_attachment_cleanup(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_save_vmm_memory_config" => rpc_json_result(
            service_save_vmm_memory_config(
                app.clone(),
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
