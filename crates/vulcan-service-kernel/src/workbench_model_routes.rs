//! Workbench model selection and OpenRouter secondary route services.
//! 工作台模型选择与 OpenRouter 二级路由服务。

use serde::Deserialize;
use serde_json::Value;
use vulcan_agent_runtime::provider_config::{self, ProviderConfigurationFile};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_service_api::{
    ProviderModelSecondaryRouteSelection, ProviderModelSecondaryRouteSummary, WorkbenchSnapshot,
};
use vulcan_workbench_model::SessionAddonNamespace;

use super::{
    SharedServiceBridgeState, active_reasoning_selection_effort, current_unix_millis_text,
    emit_workbench_event, parse_reasoning_effort, parse_thinking_display_mode,
    refresh_settings_projection_locked, refresh_workbench_runtime_controls_locked,
    selected_session_runtime_state_mut, session_addon_state_from_workspace_snapshot,
    spawn_workspace_session_addon_state_persist_task,
};

mod openrouter_routes;

pub(super) use openrouter_routes::*;

/// Provider/model secondary route selection request sent by the WebUI composer.
/// WebUI 输入区发送的 Provider/模型二级路由选择请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProviderModelSecondaryRouteRequest {
    /// Stable provider identifier.
    /// 稳定 Provider 标识。
    provider_id: String,
    /// Stable model identifier.
    /// 稳定模型标识。
    model_id: String,
    /// Selected secondary route or `None` to clear the override.
    /// 已选择的二级路由；`None` 表示清除覆盖。
    route: Option<ProviderModelSecondaryRouteSelection>,
}

/// Persists the active workbench provider/model selection.
/// 持久化当前工作台 Provider/模型选择。
///
/// # Parameters
/// 参数
/// `app` emits the updated workbench snapshot.
/// `app` 发送更新后的工作台快照。
/// `state` owns the shared service bridge state and runtime configuration.
/// `state` 持有共享服务桥状态与运行时配置。
/// `provider_id` is the requested provider identifier.
/// `provider_id` 是请求的 Provider 标识。
/// `model_id` is the requested model identifier under the provider.
/// `model_id` 是 Provider 下请求的模型标识。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot after model selection is persisted.
/// 返回模型选择持久化后的刷新工作台快照。
pub(super) async fn service_select_workbench_model(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    provider_id: String,
    model_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let provider_id = provider_id.trim().to_string();
    let model_id = model_id.trim().to_string();
    if provider_id.is_empty() || model_id.is_empty() {
        return Err(String::from("模型选择缺少 Provider 或模型标识。"));
    }

    let (
        controller_runtime,
        persist_session_id,
        persist_workspace_snapshot,
        workbench,
        default_ai_tool_configuration,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let has_model = provider_config::effective_model_choices(
            &provider_id,
            &agent_runtime.provider_configuration,
        )
        .into_iter()
        .any(|choice| choice.id == model_id);
        if !has_model {
            return Err(format!(
                "模型 `{model_id}` 不在 `{provider_id}` 的已启用模型列表中。"
            ));
        }

        let mut next_configuration = agent_runtime.provider_configuration.clone();
        next_configuration.model = Some(format!("{provider_id}/{model_id}"));
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存模型选择失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        let selected_effort = active_reasoning_selection_effort(&next_configuration);
        let openrouter_endpoint_route = (provider_id == "openrouter")
            .then(|| {
                provider_config::openrouter_endpoint_route_for_model(&next_configuration, &model_id)
            })
            .flatten();
        let persist_session_id = guard.workspace_snapshot.as_mut().and_then(|workspace| {
            selected_session_runtime_state_mut(workspace).map(|(session_id, runtime_state)| {
                runtime_state.active_provider_id = Some(provider_id.clone());
                runtime_state.active_model_id = Some(model_id.clone());
                runtime_state.active_openrouter_endpoint_route = openrouter_endpoint_route;
                if let Some(selected_effort) = selected_effort {
                    runtime_state.active_reasoning_effort = Some(selected_effort);
                }
                runtime_state.updated_at = current_unix_millis_text();
                session_id
            })
        });
        refresh_workbench_runtime_controls_locked(
            &mut guard,
            &next_configuration,
            &ai_tool_configuration,
        );
        (
            controller_runtime,
            persist_session_id,
            guard.workspace_snapshot.clone(),
            guard.workbench.clone(),
            ai_tool_configuration,
        )
    };

    if let (Some(controller_runtime), Some(session_id), Some(workspace_snapshot)) = (
        controller_runtime,
        persist_session_id,
        persist_workspace_snapshot,
    ) && let Some(addon_state) = session_addon_state_from_workspace_snapshot(
        &workspace_snapshot,
        session_id,
        &default_ai_tool_configuration,
        &[SessionAddonNamespace::RuntimeControls],
    ) {
        spawn_workspace_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime,
            workspace_snapshot,
            session_id,
            addon_state,
            vec![SessionAddonNamespace::RuntimeControls],
            Some(String::from("model_selection")),
        );
    }

    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Lists backend-managed secondary routes for one workbench provider/model pair.
/// 列出单个工作台 Provider/模型组合的后端二级路由。
pub(super) async fn service_list_workbench_model_secondary_routes(
    state: SharedServiceBridgeState,
    provider_id: String,
    model_id: String,
) -> Result<Vec<ProviderModelSecondaryRouteSummary>, String> {
    let provider_id = provider_id.trim().to_string();
    let model_id = model_id.trim().to_string();
    if provider_id != "openrouter" {
        return Ok(Vec::new());
    }
    let provider_configuration = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?
            .provider_configuration
            .clone()
    };
    ensure_provider_model_enabled(&provider_configuration, &provider_id, &model_id)?;
    let active_route =
        provider_config::openrouter_endpoint_route_for_model(&provider_configuration, &model_id);
    let api_key = provider_configuration
        .provider
        .get("openrouter")
        .map(|entry| entry.options.resolved_api_key())
        .transpose()?
        .flatten();
    let choices = fetch_openrouter_secondary_route_choices(model_id.clone(), api_key).await?;
    Ok(choices
        .into_iter()
        .map(|choice| {
            let active = active_route
                .as_ref()
                .is_some_and(|selection| selection.tag == choice.tag);
            choice.into_summary(&provider_id, &model_id, active)
        })
        .collect())
}

/// Persists or clears one backend-managed secondary route for the active WebUI workbench model.
/// 为当前 WebUI 工作台模型持久化或清除一条后端二级路由。
pub(super) async fn service_select_workbench_model_secondary_route(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ProviderModelSecondaryRouteRequest,
) -> Result<WorkbenchSnapshot, String> {
    let provider_id = request.provider_id.trim().to_string();
    let model_id = request.model_id.trim().to_string();
    if provider_id != "openrouter" {
        return Err(String::from("当前只有 OpenRouter 支持二级供应商选择。"));
    }
    let selection = request.route.map(openrouter_route_selection_from_service);
    let (
        controller_runtime,
        persist_session_id,
        persist_workspace_snapshot,
        workbench,
        default_ai_tool_configuration,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        ensure_provider_model_enabled(
            &agent_runtime.provider_configuration,
            &provider_id,
            &model_id,
        )?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        provider_config::set_openrouter_endpoint_route_for_model(
            &mut next_configuration,
            &model_id,
            selection.clone(),
        );
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存 OpenRouter 二级供应商失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        let persist_session_id = guard.workspace_snapshot.as_mut().and_then(|workspace| {
            selected_session_runtime_state_mut(workspace).map(|(session_id, runtime_state)| {
                if runtime_state.active_provider_id.as_deref() == Some("openrouter")
                    && runtime_state.active_model_id.as_deref() == Some(model_id.as_str())
                {
                    runtime_state.active_openrouter_endpoint_route = selection.clone();
                    runtime_state.updated_at = current_unix_millis_text();
                }
                session_id
            })
        });
        refresh_workbench_runtime_controls_locked(
            &mut guard,
            &next_configuration,
            &ai_tool_configuration,
        );
        (
            controller_runtime,
            persist_session_id,
            guard.workspace_snapshot.clone(),
            guard.workbench.clone(),
            ai_tool_configuration,
        )
    };
    if let (Some(controller_runtime), Some(session_id), Some(workspace_snapshot)) = (
        controller_runtime,
        persist_session_id,
        persist_workspace_snapshot,
    ) && let Some(addon_state) = session_addon_state_from_workspace_snapshot(
        &workspace_snapshot,
        session_id,
        &default_ai_tool_configuration,
        &[SessionAddonNamespace::RuntimeControls],
    ) {
        spawn_workspace_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime,
            workspace_snapshot,
            session_id,
            addon_state,
            vec![SessionAddonNamespace::RuntimeControls],
            Some(String::from("openrouter_endpoint_route")),
        );
    }
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Ensures one provider/model pair is currently visible in the effective model list.
/// 确认单个 Provider/模型组合当前位于有效模型列表中。
fn ensure_provider_model_enabled(
    provider_configuration: &ProviderConfigurationFile,
    provider_id: &str,
    model_id: &str,
) -> Result<(), String> {
    let has_model = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .any(|choice| choice.id == model_id);
    if has_model {
        Ok(())
    } else {
        Err(format!(
            "模型 `{model_id}` 不在 `{provider_id}` 的已启用模型列表中。"
        ))
    }
}

/// Converts a service secondary-route selection into the persisted OpenRouter route type.
/// 将 service 二级路由选择转换为持久化 OpenRouter 路由类型。

/// Persists the active workbench reasoning effort through the shared provider configuration.
/// 通过共享 Provider 配置持久化当前工作台思考档位。
pub(super) async fn service_select_workbench_reasoning_effort(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    effort: String,
) -> Result<WorkbenchSnapshot, String> {
    let selected_effort =
        parse_reasoning_effort(effort.trim()).ok_or_else(|| format!("未知思考档位：{effort}"))?;
    let (
        controller_runtime,
        persist_session_id,
        persist_workspace_snapshot,
        workbench,
        default_ai_tool_configuration,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        next_configuration.workbench_reasoning_effort = Some(selected_effort);
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存思考档位失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        let persist_session_id = guard.workspace_snapshot.as_mut().and_then(|workspace| {
            selected_session_runtime_state_mut(workspace).map(|(session_id, runtime_state)| {
                runtime_state.active_reasoning_effort = Some(selected_effort);
                runtime_state.updated_at = current_unix_millis_text();
                session_id
            })
        });
        refresh_workbench_runtime_controls_locked(
            &mut guard,
            &next_configuration,
            &ai_tool_configuration,
        );
        (
            controller_runtime,
            persist_session_id,
            guard.workspace_snapshot.clone(),
            guard.workbench.clone(),
            ai_tool_configuration,
        )
    };

    if let (Some(controller_runtime), Some(session_id), Some(workspace_snapshot)) = (
        controller_runtime,
        persist_session_id,
        persist_workspace_snapshot,
    ) && let Some(addon_state) = session_addon_state_from_workspace_snapshot(
        &workspace_snapshot,
        session_id,
        &default_ai_tool_configuration,
        &[SessionAddonNamespace::RuntimeControls],
    ) {
        spawn_workspace_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime,
            workspace_snapshot,
            session_id,
            addon_state,
            vec![SessionAddonNamespace::RuntimeControls],
            Some(String::from("reasoning_selection")),
        );
    }

    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Persists the WebUI thinking-output display mode in the shared Vulcan configuration file.
/// 将 WebUI 思考输出展示模式持久化到共享 Vulcan 配置文件中。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot to connected WebUI clients.
/// `app` 用于向已连接的 WebUI 客户端发送刷新后的工作台快照。
/// `state` owns the active service runtime and retained workbench snapshot.
/// `state` 持有当前服务运行时和保留的工作台快照。
/// `mode` is the stable WebUI thinking display mode wire value.
/// `mode` 是稳定的 WebUI 思考展示模式传输值。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot after the preference is persisted.
/// 返回偏好持久化后的刷新工作台快照。
pub(super) async fn service_select_workbench_thinking_display_mode(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    mode: String,
) -> Result<WorkbenchSnapshot, String> {
    let selected_mode = parse_thinking_display_mode(mode.trim())
        .ok_or_else(|| format!("未知思考展示模式：{mode}"))?;
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        next_configuration.workbench_thinking_display_mode = Some(selected_mode);
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存思考展示模式失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        refresh_settings_projection_locked(&mut guard, &next_configuration, None);
        guard.workbench.clone()
    };

    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}
