//! Workbench model selection and OpenRouter secondary route services.
//! 工作台模型选择与 OpenRouter 二级路由服务。

use serde::Deserialize;
use vulcan_provider_adapters::{
    PersistedAiConfiguration, SecondaryRouteOption, SecondaryRouteSelection,
};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_service_api::{
    ProviderModelSecondaryRouteSelection, ProviderModelSecondaryRouteSummary, WorkbenchSnapshot,
};
use vulcan_workbench_model::{SessionAddonNamespace, SessionRuntimeStateEntry};

use super::{
    SharedServiceBridgeState, current_unix_millis_text, emit_workbench_event,
    mutate_provider_configuration, mutate_public_ai_configuration, parse_reasoning_effort,
    parse_thinking_display_mode, publish_public_ai_workbench, refresh_settings_projection_locked,
    refresh_workbench_runtime_controls_locked, selected_session_runtime_state_mut,
    session_addon_state_from_workspace_snapshot, spawn_workspace_session_addon_state_persist_task,
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

/// Lists backend-managed secondary routes for one workbench provider/model pair.
/// 列出单个工作台 Provider/模型组合的后端二级路由。
pub(super) async fn service_list_workbench_model_secondary_routes(
    state: SharedServiceBridgeState,
    provider_id: String,
    model_id: String,
    force_refresh: bool,
) -> Result<Vec<ProviderModelSecondaryRouteSummary>, String> {
    let provider_id = provider_id.trim().to_string();
    let model_id = model_id.trim().to_string();
    if provider_id != "openrouter" {
        return Ok(Vec::new());
    }
    let (runtime, configuration, connection_id, active_route) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let runtime = std::sync::Arc::clone(&agent_runtime.public_ai_runtime);
        let configuration = runtime.configuration_snapshot()?;
        let active = exact_public_secondary_route_target(
            runtime.as_ref(),
            &configuration,
            guard.workspace_snapshot.as_ref().and_then(|workspace| {
                workspace
                    .session_runtime_state
                    .get(&workspace.selected_session)
            }),
            provider_id.as_str(),
            model_id.as_str(),
        )?;
        let settings_key =
            PersistedAiConfiguration::connection_model_key(active.as_str(), model_id.as_str());
        let active_route = configuration
            .secondary_routes
            .get(settings_key.as_str())
            .cloned()
            .unwrap_or_default();
        (runtime, configuration, active, active_route)
    };
    let choices = runtime
        .discover_secondary_routes(
            &configuration,
            connection_id.as_str(),
            model_id.as_str(),
            force_refresh,
        )
        .await?;
    Ok(choices
        .into_iter()
        .map(|choice| {
            let active = matches!(
                &active_route,
                SecondaryRouteSelection::PinnedEndpoint { route_id, .. }
                    if route_id == &choice.route_id
            );
            public_secondary_route_summary(&provider_id, &model_id, choice, active)
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
    let (runtime, current_configuration, connection_id) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let runtime = std::sync::Arc::clone(&agent_runtime.public_ai_runtime);
        let configuration = runtime.configuration_snapshot()?;
        let active = exact_public_secondary_route_target(
            runtime.as_ref(),
            &configuration,
            guard.workspace_snapshot.as_ref().and_then(|workspace| {
                workspace
                    .session_runtime_state
                    .get(&workspace.selected_session)
            }),
            provider_id.as_str(),
            model_id.as_str(),
        )?;
        (runtime, configuration, active)
    };
    let selection = if let Some(route) = request.route {
        let route_id = if route.tag.trim().is_empty() {
            route.route_id.trim()
        } else {
            route.tag.trim()
        };
        let options = runtime
            .discover_secondary_routes(
                &current_configuration,
                connection_id.as_str(),
                model_id.as_str(),
                false,
            )
            .await?;
        let option = options
            .into_iter()
            .find(|option| option.route_id == route_id)
            .ok_or_else(|| {
                format!(
                    "OpenRouter route `{route_id}` is not present in the current endpoint catalog."
                )
            })?;
        SecondaryRouteSelection::PinnedEndpoint {
            route_id: option.route_id,
            provider_display_name: option.provider_display_name,
            endpoint_display_name: option.endpoint_display_name,
        }
    } else {
        SecondaryRouteSelection::Automatic
    };
    let settings_key =
        PersistedAiConfiguration::connection_model_key(connection_id.as_str(), model_id.as_str());
    let configuration = mutate_public_ai_configuration(
        &state,
        "保存 OpenRouter 二级供应商失败",
        |configuration| {
            runtime.resolve_connection_model_selection(
                configuration,
                connection_id.as_str(),
                provider_id.as_str(),
                model_id.as_str(),
            )?;
            configuration
                .secondary_routes
                .insert(settings_key.clone(), selection.clone());
            Ok(())
        },
    )
    .await?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)
}

/// Resolves and validates the exact active public model targeted by secondary routing.
/// 解析并校验二级路由所针对的精确当前公共模型。
fn exact_public_secondary_route_target(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    runtime_state: Option<&SessionRuntimeStateEntry>,
    provider_id: &str,
    model_id: &str,
) -> Result<String, String> {
    if let Some(runtime_state) = runtime_state
        && let Some((session_provider_id, session_model_id)) = runtime_state
            .active_provider_id
            .as_deref()
            .zip(runtime_state.active_model_id.as_deref())
    {
        if session_provider_id != provider_id || session_model_id != model_id {
            return Err(format!(
                "Secondary route target `{provider_id}/{model_id}` is not the active session model."
            ));
        }
        if let Some(connection_id) = runtime_state.active_connection_id.as_deref()
            && let Ok(selection) = runtime.resolve_connection_model_selection(
                configuration,
                connection_id,
                provider_id,
                model_id,
            )
        {
            return Ok(selection.route.connection.id);
        }
        return runtime
            .resolve_model_selection(configuration, provider_id, model_id)
            .map(|selection| selection.route.connection.id);
    }
    let active = configuration
        .active
        .clone()
        .ok_or_else(|| String::from("No active public AI model is configured."))?;
    if active.provider_id != provider_id || active.model_id != model_id {
        return Err(format!(
            "Secondary route target `{provider_id}/{model_id}` is not the active public AI model."
        ));
    }
    Ok(active.connection_id)
}

/// Projects one public secondary-route option into the existing workbench contract.
/// 将一个公共二级路由选项投影到现有工作台契约。
fn public_secondary_route_summary(
    provider_id: &str,
    model_id: &str,
    option: SecondaryRouteOption,
    active: bool,
) -> ProviderModelSecondaryRouteSummary {
    let subtitle = openrouter_secondary_route_option_subtitle(&option);
    ProviderModelSecondaryRouteSummary {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        route_id: option.route_id.clone(),
        tag: option.route_id,
        provider_name: option.provider_display_name.clone(),
        endpoint_name: option.endpoint_display_name,
        label: option.provider_display_name,
        subtitle,
        active,
    }
}

/// Persists the active workbench reasoning effort through the shared provider configuration.
/// 通过共享 Provider 配置持久化当前工作台思考档位。
pub(super) async fn service_select_workbench_reasoning_effort(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    effort: String,
) -> Result<WorkbenchSnapshot, String> {
    let selected_effort =
        parse_reasoning_effort(effort.trim()).ok_or_else(|| format!("未知思考档位：{effort}"))?;
    let (controller_runtime, persist_session_id, persist_workspace_snapshot, workbench) =
        mutate_provider_configuration(
            &state,
            "保存思考档位失败",
            |configuration| {
                configuration.workbench_reasoning_effort = Some(selected_effort);
                Ok(())
            },
            |guard, configuration| {
                let controller_runtime = guard.controller_runtime.clone();
                let persist_session_id = guard.workspace_snapshot.as_mut().and_then(|workspace| {
                    selected_session_runtime_state_mut(workspace).map(
                        |(session_id, runtime_state)| {
                            runtime_state.active_reasoning_effort = Some(selected_effort);
                            runtime_state.updated_at = current_unix_millis_text();
                            session_id
                        },
                    )
                });
                refresh_workbench_runtime_controls_locked(guard, configuration);
                (
                    controller_runtime,
                    persist_session_id,
                    guard.workspace_snapshot.clone(),
                    guard.workbench.clone(),
                )
            },
        )
        .await?;

    if let (Some(controller_runtime), Some(session_id), Some(workspace_snapshot)) = (
        controller_runtime,
        persist_session_id,
        persist_workspace_snapshot,
    ) && let Some(addon_state) = session_addon_state_from_workspace_snapshot(
        &workspace_snapshot,
        session_id,
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
    let workbench = mutate_provider_configuration(
        &state,
        "保存思考展示模式失败",
        |configuration| {
            configuration.workbench_thinking_display_mode = Some(selected_mode);
            Ok(())
        },
        |guard, configuration| {
            refresh_settings_projection_locked(guard, configuration);
            guard.workbench.clone()
        },
    )
    .await?;

    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Persists the WebUI locale in the shared Vulcan configuration file.
/// 将 WebUI 语言持久化到共享 Vulcan 配置文件中。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot to connected WebUI clients.
/// `app` 用于向已连接的 WebUI 客户端发送刷新后的工作台快照。
/// `state` owns the active service runtime and retained workbench snapshot.
/// `state` 持有当前服务运行时和保留的工作台快照。
/// `locale` is the stable WebUI locale wire value.
/// `locale` 是稳定的 WebUI 语言传输值。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot after the locale is persisted.
/// 返回语言持久化后的刷新工作台快照。
pub(super) async fn service_select_webui_locale(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    locale: String,
) -> Result<WorkbenchSnapshot, String> {
    let selected_locale = parse_webui_locale(locale.trim())
        .ok_or_else(|| format!("unsupported WebUI locale: {locale}"))?;
    let workbench = mutate_provider_configuration(
        &state,
        "failed to save WebUI locale",
        |configuration| {
            configuration.ui_locale = Some(selected_locale.clone());
            Ok(())
        },
        |guard, configuration| {
            guard.startup.ui_locale = Some(selected_locale.clone());
            refresh_settings_projection_locked(guard, configuration);
            guard.workbench.clone()
        },
    )
    .await?;

    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Parses one WebUI locale wire value.
/// 解析单个 WebUI 语言传输值。
fn parse_webui_locale(value: &str) -> Option<String> {
    match value {
        "zh-CN" | "en-US" => Some(value.to_string()),
        _ => None,
    }
}
