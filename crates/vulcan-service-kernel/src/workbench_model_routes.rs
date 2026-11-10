//! Workbench model selection and manifest-selected secondary route services.
//! 工作台模型选择与 Manifest 选择的二级路由服务。

use serde::Deserialize;
use vulcan_provider_adapters::{
    PersistedAiConfiguration, SecondaryRouteOption, SecondaryRouteSelection,
};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_service_api::{
    ProviderModelSecondaryRouteSelection, ProviderModelSecondaryRouteSummary, WorkbenchSnapshot,
};
use vulcan_workbench_model::{SessionId, SessionRuntimeStateEntry};

use super::{
    ReasoningEffortLevel, SharedServiceBridgeState, current_unix_millis_text, emit_workbench_event,
    mutate_provider_configuration, mutate_public_ai_configuration, parse_reasoning_effort,
    parse_thinking_display_mode, publish_public_ai_workbench, refresh_settings_projection_locked,
    refresh_workbench_runtime_controls_locked,
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
    let (runtime, configuration, connection_id) = {
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
        let options = runtime.cached_secondary_routes(
            &configuration,
            connection_id.as_str(),
            model_id.as_str(),
        )?;
        let option = options
            .into_iter()
            .find(|option| option.route_id == route_id)
            .ok_or_else(|| {
                format!(
                    "Secondary route `{route_id}` is not present in the current endpoint catalog."
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
        "Failed to save the secondary provider",
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

/// Resolves the exact workbench-selected public model targeted by secondary routing.
/// 解析二级路由所针对的工作台精确选中公共模型。
fn exact_public_secondary_route_target(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    runtime_state: Option<&SessionRuntimeStateEntry>,
    provider_id: &str,
    model_id: &str,
) -> Result<String, String> {
    // Use the same pending-first selection as the visible workbench; the current turn's frozen
    // route is not the user's selected target for a secondary route applied to the next request.
    // 与可见工作台共用待生效优先的选择；当前回合冻结路由并非下一次请求要应用的二级路由目标。
    let selected = super::settings_provider_projection::public_ai_workbench_selection(
        configuration,
        runtime,
        None,
        runtime_state,
    )?
    .ok_or_else(|| String::from("No executable public AI model is selected."))?;
    if selected.route.provider.id != provider_id || selected.model.id != model_id {
        return Err(format!(
            "Secondary route target `{provider_id}/{model_id}` is not the selected workbench model."
        ));
    }
    Ok(selected.route.connection.id)
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

/// Saves a reasoning choice for its exact model and updates the next request of an existing conversation.
/// 为精确模型保存思考选择，并更新已有会话的下一次请求。
/// `app` emits the resulting workbench, `state` owns settings, and `conversation_id` identifies an existing session.
/// `app` 发出更新后的工作台，`state` 持有设置，`conversation_id` 标识已有会话。
/// `effort` is the chosen level and `requested_model` identifies the draft route or verifies the session route.
/// `effort` 是所选档位，`requested_model` 标识草稿路线或校验会话路线。
/// Returns the resulting workbench after projecting the saved selection.
/// 返回已投影保存选择的工作台。
pub(super) async fn service_select_workbench_reasoning_effort(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: Option<SessionId>,
    effort: String,
    requested_model: Option<crate::conversation_submission::SubmissionModelSelection>,
) -> Result<WorkbenchSnapshot, String> {
    // requested_effort is a recognized provider-neutral level, even if a manifest has removed it.
    // requested_effort 是已知的供应商中立档位，即使清单已移除该档位。
    let requested_effort = parse_reasoning_effort(effort.trim())
        .ok_or_else(|| format!("Unknown reasoning level: {effort}"))?;
    // session pins the existing document independently of workbench navigation.
    // session 固定已有会话文档，独立于工作台导航。
    let session = match conversation_id {
        Some(id) => Some((
            id,
            crate::session_goal_lifecycle::session_v1_handle_from_state(&state, id).await?,
        )),
        None => None,
    };
    // Serialize with explicit model switches and manifest publication before validating the route.
    // 在校验路线前，与显式模型切换和清单发布串行化。
    let _model_transaction = crate::begin_public_ai_configuration_transaction().await;
    let settings_transaction = crate::begin_provider_configuration_transaction().await;
    // guard pins the runtime mirror and the latest persisted preferences through the write.
    // guard 在写入期间固定运行时镜像和最新的持久偏好。
    let mut guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    // agent_runtime owns the executable provider catalog and the user settings document.
    // agent_runtime 持有可执行供应商目录和用户设置文档。
    let agent_runtime = guard
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("Service is still starting; agent runtime is not ready."))?;
    let runtime = agent_runtime.public_ai_runtime.as_ref();
    let public_configuration = runtime.configuration_snapshot()?;
    // runtime_state is present only for the exact existing conversation targeted by the picker.
    // runtime_state 仅在选择器指向精确已有会话时存在。
    let runtime_state = conversation_id
        .map(|id| {
            guard
                .workspace_snapshot
                .as_ref()
                .filter(|workspace| workspace.sessions.iter().any(|session| session.id == id))
                .and_then(|workspace| workspace.session_runtime_state.get(&id))
                .ok_or_else(|| format!("Conversation `{id}` has no runtime state."))
        })
        .transpose()?;
    // selected_model resolves the session's exact route or the explicitly supplied draft route.
    // selected_model 解析会话的精确路线或显式传入的草稿路线。
    let selected_model = if session.is_some() {
        crate::public_ai_workbench_selection(
            &public_configuration,
            runtime,
            conversation_id,
            runtime_state,
        )?
        .ok_or_else(|| {
            String::from("Select an available model before changing its reasoning level.")
        })?
    } else {
        crate::conversation_submission::resolve_enabled_submission_model_selection(
            runtime,
            &public_configuration,
            requested_model.as_ref().ok_or_else(|| {
                String::from("A draft reasoning choice requires its exact model selection.")
            })?,
        )?
    };
    if let Some(expected) = requested_model.as_ref()
        && (expected.provider_id != selected_model.route.provider.id
            || expected.model_id != selected_model.model.id
            || expected.connection_id != selected_model.route.connection.id)
    {
        return Err(String::from(
            "The reasoning selection's model changed. Select its level again.",
        ));
    }
    // effective_effort tolerates a level removed between rendering the picker and saving.
    // effective_effort 兼容在选择器渲染与保存之间被移除的档位。
    let effective_effort =
        vulcan_agent_runtime::agent_runtime::provider_reasoning::resolve_effective_reasoning_effort(
            &selected_model.model.capabilities,
            Some(requested_effort),
        );
    // provider_id and model_id scope preferences independently of account selection.
    // provider_id 与 model_id 确定偏好范围，独立于账号选择。
    let provider_id = &selected_model.route.provider.id;
    let model_id = &selected_model.model.id;
    if let Some((_, handle)) = session.as_ref() {
        // next_request prevents a stale workbench mirror from assigning the choice to another model.
        // next_request 防止过期的工作台镜像把选择归属到其他模型。
        let next_request =
            crate::session_request_config_v1::capture_next_request_config_v1(handle.as_ref())?;
        if next_request
            .provider
            .as_ref()
            .is_some_and(|id| id != provider_id)
            || next_request.model.as_ref().is_some_and(|id| id != model_id)
            || next_request
                .connection_id
                .as_ref()
                .is_some_and(|id| id != &selected_model.route.connection.id)
        {
            return Err(String::from(
                "The session's next request uses a different model. Select its level again.",
            ));
        }
    }
    // now_ms pins the pending request timestamp before either durable write.
    // now_ms 在任一可靠写入前固定待生效请求的时间戳。
    let now_ms = crate::current_v1_unix_millis()?;
    // configuration stages the new preference; original remains available for rollback.
    // configuration 暂存新偏好，original 保留用于回滚。
    let original = &agent_runtime.provider_configuration;
    let mut configuration = original.clone();
    // preference_changed excludes compatibility fallbacks and idempotent choices.
    // preference_changed 排除兼容回退和幂等选择。
    let preference_changed = requested_effort == effective_effort
        && requested_effort != ReasoningEffortLevel::Off
        && configuration.remembered_model_reasoning_effort(provider_id, model_id)
            != Some(requested_effort);
    if preference_changed {
        configuration
            .model_reasoning_efforts
            .entry(provider_id.clone())
            .or_default()
            .insert(model_id.clone(), requested_effort);
        settings_transaction.persist(
            &configuration,
            "Failed to remember the model's reasoning level",
        )?;
    }
    if let (Some((_, handle)), Some(runtime_state)) = (session.as_ref(), runtime_state)
        && let Err(error) = crate::session_request_config_v1::set_pending_reasoning_request_v1(
            handle.as_ref(),
            effective_effort,
            crate::session_request_config_v1::SessionRequestFallback {
                provider: Some(provider_id.clone()),
                connection_id: Some(selected_model.route.connection.id.clone()),
                model: Some(model_id.clone()),
                temperature: runtime_state.active_temperature,
            },
            now_ms,
        )
    {
        // The preference and session change form one user action; restore settings if the session write fails.
        // 偏好与会话修改属于同一用户操作，会话写入失败时恢复设置。
        if preference_changed
            && let Err(rollback) = settings_transaction.persist(
                original,
                "Failed to restore the previous reasoning preference",
            )
        {
            return Err(format!("{error}; {rollback}"));
        }
        return Err(error);
    }
    // installed is the runtime we just read under the same service-state lock.
    // installed 是刚刚在同一服务状态锁内读取的运行时。
    let installed = guard
        .agent_runtime
        .as_mut()
        .ok_or_else(|| String::from("Service is still starting; agent runtime is not ready."))?;
    installed.provider_configuration = configuration.clone();
    if let Some(id) = conversation_id
        && let Some(runtime_state) = guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&id))
    {
        runtime_state.active_reasoning_effort = Some(effective_effort);
        runtime_state.updated_at = current_unix_millis_text();
    }
    refresh_workbench_runtime_controls_locked(&mut guard, &configuration);
    // workbench contains the exact route and level resulting from the serialized write.
    // workbench 包含本次串行写入后的精确路线和档位。
    let workbench = guard.workbench.clone();
    drop(guard);
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
        .ok_or_else(|| format!("Unknown thinking display mode: {mode}"))?;
    let workbench = mutate_provider_configuration(
        &state,
        "Failed to save the thinking display mode",
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

/// Saves the default tool-group folding preference without changing session overrides.
/// 保存工具分组默认折叠偏好，不改变会话覆盖值。
/// Parameters: app publishes snapshots, state owns configuration, enabled is the explicit default.
/// 参数：app 发布快照，state 持有配置，enabled 是明确的默认值。
/// Returns the refreshed workbench or an explicit persistence error.
/// 返回刷新后的工作台或明确的持久化错误。
pub(super) async fn service_save_tool_collapse_default(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    enabled: bool,
) -> Result<WorkbenchSnapshot, String> {
    // Persist only on explicit user action; loading never rewrites older configuration.
    // 仅在用户明确操作时持久化；加载时不改写旧配置。
    let workbench = mutate_provider_configuration(
        &state,
        "Failed to save the default tool collapse setting",
        |configuration| {
            configuration.tool_collapse_default_enabled = Some(enabled);
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

/// Persists whether new sessions are titled by a model from their first prompt.
/// 持久化新会话是否由模型根据首个提示词生成标题。
/// Parameters: app publishes the snapshot, state owns configuration, enabled is the explicit choice.
/// 参数：app 发布快照，state 持有配置，enabled 是明确的选择。
/// Returns the refreshed workbench or an explicit persistence error.
/// 返回刷新后的工作台或明确的持久化错误。
pub(super) async fn service_save_session_title_generation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    enabled: bool,
) -> Result<WorkbenchSnapshot, String> {
    // Persist only on explicit user action; loading never writes the default.
    // 仅在用户明确操作时持久化；加载时不写入默认值。
    let workbench = mutate_provider_configuration(
        &state,
        "Failed to save the session title generation setting",
        |configuration| {
            configuration.session_title_generation_enabled = Some(enabled);
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

/// Persists the system `tool_chain` settings chosen by the user.
/// 持久化用户选择的系统 `tool_chain` 设置。
/// Parameters: `enabled` is the default for sessions without their own choice; `max_steps` must be
/// within the supported range. Returns the refreshed workbench or an explicit error.
/// 参数：`enabled` 是未自行选择的会话的默认值；`max_steps` 必须在支持范围内。返回刷新后的工作台
/// 或明确的错误。
pub(super) async fn service_save_tool_chain_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    enabled: bool,
    max_steps: u32,
) -> Result<WorkbenchSnapshot, String> {
    let range =
        vulcan_tool_chain::MIN_TOOL_CHAIN_MAX_STEPS..=vulcan_tool_chain::MAX_TOOL_CHAIN_MAX_STEPS;
    if !range.contains(&max_steps) {
        return Err(format!(
            "tool_chain max steps must be between {} and {}, got {max_steps}.",
            range.start(),
            range.end()
        ));
    }
    // Persist only on explicit user action; loading never writes the defaults.
    // 仅在用户明确操作时持久化；加载时不写入默认值。
    let workbench = mutate_provider_configuration(
        &state,
        "Failed to save the tool_chain settings",
        |configuration| {
            configuration.tool_chain_enabled = Some(enabled);
            configuration.tool_chain_max_steps = Some(max_steps);
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

/// Persists how sessions without their own choice open files outside the workspace.
/// 持久化未自行选择的会话如何打开工作区之外的文件。
/// Parameters: `access` is the system default chosen by the user. Returns the refreshed workbench
/// or an explicit error.
/// 参数：`access` 是用户选择的系统默认值。返回刷新后的工作台或明确的错误。
pub(super) async fn service_save_outside_workspace_file_access(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    access: vulcan_session_core::OutsideWorkspaceFileAccess,
) -> Result<WorkbenchSnapshot, String> {
    // Persist only on explicit user action; loading never writes the default.
    // 仅在用户明确操作时持久化；加载时不写入默认值。
    let workbench = mutate_provider_configuration(
        &state,
        "Failed to save the outside workspace file access setting",
        |configuration| {
            configuration.outside_workspace_file_access = Some(access);
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
    let selected_locale = vulcan_i18n::normalize(locale.trim())
        .map(str::to_string)
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use vulcan_provider_adapters::{
        ActiveAiModelSelection, CredentialConnection, CredentialMode, PersistedAiConfiguration,
        ProtocolModelVariant, PublicAiRuntime, UnavailableCredentialSecretResolver,
        parse_openrouter_model_catalog,
    };
    use vulcan_workbench_model::{SessionPendingModelSelection, SessionRuntimeStateEntry};

    use super::{exact_public_secondary_route_target, service_select_workbench_reasoning_effort};
    use crate::conversation_submission::SubmissionModelSelection;
    use crate::tests::{
        provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH,
        test_support::{test_agent_runtime_bridge, test_service_state_with_parent_session},
    };
    use crate::{ProviderConfigurationFile, ReasoningEffortLevel, SessionId};
    use vulcan_service_api::ServiceEventEmitter;

    /// Builds a synthetic two-model catalog without using provider credentials or the network.
    /// 构建不使用供应商凭据或网络的合成双模型目录。
    /// Takes no parameters and returns the runtime and its installed route configuration.
    /// 无参数，返回运行时及其已安装的路线配置。
    fn two_model_fixture() -> (Arc<PublicAiRuntime>, PersistedAiConfiguration) {
        let runtime = Arc::new(
            PublicAiRuntime::first_batch(Arc::new(UnavailableCredentialSecretResolver))
                .expect("runtime should initialize"),
        );
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime.providers().expect("providers should resolve"),
                CredentialConnection {
                    id: String::from("openrouter-account"),
                    provider_id: String::from("openrouter"),
                    service_entry_id: String::from("public-api"),
                    credential_profile_id: String::from("api-key"),
                    credential_mode: CredentialMode::Token,
                    credential_id: String::from("credential-openrouter"),
                    display_name: String::from("OpenRouter account"),
                    secret_reference: String::from("secret-openrouter"),
                    management_secret_reference: None,
                    service_origin_override: None,
                },
            )
            .expect("account should register");
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-terra", true);
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-terra"),
            connection_id: String::from("openrouter-account"),
        });
        let mut catalog = parse_openrouter_model_catalog(
            "openrouter-account",
            1,
            r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Terra","context_length":8192,"architecture":{"input_modalities":["text"],"output_modalities":["text"]}},{"id":"openai/gpt-5.6-sol","name":"Sol","context_length":8192,"architecture":{"input_modalities":["text"],"output_modalities":["text"]}}]}"#,
        )
        .expect("catalog should parse");
        for model in &mut catalog.models {
            model.protocol_variants.insert(
                vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID.to_string(),
                ProtocolModelVariant {
                    capabilities: model.capabilities.clone(),
                    capability_evidence: Default::default(),
                    discovery_signature: String::from("secondary-route-test"),
                    catalog_generation: catalog.generation,
                },
            );
        }
        runtime
            .stage_source_catalog_for_testing(catalog)
            .expect("catalog should stage");
        runtime
            .replace_configuration(configuration.clone())
            .expect("configuration should install");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("catalog should activate");

        (runtime, configuration)
    }

    /// Verifies a pending next-turn model owns secondary routes before the old active model.
    /// 验证下一轮待生效模型优先于旧的当前模型持有二级路由。
    #[test]
    fn secondary_route_uses_pending_workbench_model() {
        let (runtime, configuration) = two_model_fixture();
        let state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("openrouter")),
            active_connection_id: Some(String::from("openrouter-account")),
            active_model_id: Some(String::from("openai/gpt-5.6-terra")),
            pending_model_selection: Some(SessionPendingModelSelection {
                provider_id: String::from("openrouter"),
                connection_id: String::from("openrouter-account"),
                model_id: String::from("openai/gpt-5.6-sol"),
            }),
            ..SessionRuntimeStateEntry::default()
        };
        assert_eq!(
            exact_public_secondary_route_target(
                &runtime,
                &configuration,
                Some(&state),
                "openrouter",
                "openai/gpt-5.6-sol",
            ),
            Ok(String::from("openrouter-account"))
        );
        assert!(
            exact_public_secondary_route_target(
                &runtime,
                &configuration,
                Some(&state),
                "openrouter",
                "openai/gpt-5.6-terra",
            )
            .is_err()
        );
    }

    /// Verifies two explicit model preferences survive persistence without sharing a legacy global value.
    /// 验证两个显式模型偏好各自持久化，不共用旧全局值。
    /// Takes no arguments and checks the real draft RPC and configuration document.
    /// 无参数，校验真实草稿 RPC 和配置文档。
    #[tokio::test]
    async fn draft_reasoning_choices_are_remembered_per_model() {
        // runtime and public_configuration hold the exact synthetic routes used for each RPC.
        // runtime 和 public_configuration 持有每个 RPC 使用的精确合成路线。
        let (runtime, public_configuration) = two_model_fixture();
        // service runs real configuration persistence with a synthetic provider runtime.
        // service 使用合成供应商运行时执行真实配置持久化。
        let mut service = test_service_state_with_parent_session(SessionId::new_v4());
        let mut agent_runtime = test_agent_runtime_bridge();
        agent_runtime.public_ai_runtime = Arc::clone(&runtime);
        service.agent_runtime = Some(agent_runtime);
        // state and app capture the authoritative configuration and its resulting projections.
        // state 与 app 持有权威配置和由此产生的投影。
        let state = Arc::new(Mutex::new(service));
        let app = ServiceEventEmitter::new(Vec::new());
        // directory and path keep these writes away from real user settings.
        // directory 和 path 使这些写入远离真实用户设置。
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("configuration.json");
        PROVIDER_CONFIGURATION_PATH.scope(path.clone(), async {
            // choices tracks what the synthetic catalog actually supports, rather than assuming levels.
            // choices 记录合成目录实际支持的档位，而不假定固定档位。
            let mut choices = Vec::new();
            for model_id in ["openai/gpt-5.6-terra", "openai/gpt-5.6-sol"] {
                // selection and model bind the exact route and its executable capabilities.
                // selection 与 model 绑定精确路线及其可执行能力。
                let selection = SubmissionModelSelection {
                    provider_id: String::from("openrouter"),
                    model_id: model_id.to_owned(),
                    connection_id: String::from("openrouter-account"),
                };
                let model = runtime.resolve_connection_model_selection(
                    &public_configuration, &selection.connection_id, &selection.provider_id, &selection.model_id,
                ).unwrap();
                // effort picks a supported level different from the previous model's choice.
                // effort 选择一个受支持且与前一模型选择不同的档位。
                let effort = [ReasoningEffortLevel::Low, ReasoningEffortLevel::High, ReasoningEffortLevel::Medium]
                    .into_iter()
                    .find(|effort| {
                        !choices.iter().any(|&(_, used)| used == *effort)
                            && vulcan_agent_runtime::agent_runtime::provider_reasoning::resolve_effective_reasoning_effort(
                                &model.model.capabilities, Some(*effort),
                            ) == *effort
                    }).expect("fixture models should support distinct selectable reasoning levels");
                service_select_workbench_reasoning_effort(
                    app.clone(), state.clone(), None,
                    crate::session_request_config_v1::reasoning_effort_level_key(effort),
                    Some(selection),
                ).await.unwrap();
                choices.push((model_id, effort));
            }
            // saved is reloaded from the real configuration written by both calls.
            // saved 从两次调用写入的真实配置重新读取。
            let saved: ProviderConfigurationFile =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            for (model_id, effort) in choices {
                assert_eq!(saved.remembered_model_reasoning_effort("openrouter", model_id), Some(effort));
                assert_eq!(saved.remembered_model_reasoning_effort("other-provider", model_id), None);
            }
            assert_eq!(saved.workbench_reasoning_effort, None);
        }).await;
    }
}
