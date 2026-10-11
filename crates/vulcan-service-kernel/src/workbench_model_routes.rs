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
    SharedServiceBridgeState, current_unix_millis_text, emit_workbench_event,
    mutate_provider_configuration, mutate_provider_configuration_with_state_validation,
    mutate_public_ai_configuration, parse_reasoning_effort, parse_thinking_display_mode,
    publish_public_ai_workbench, refresh_settings_projection_locked,
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

/// Persists the active workbench reasoning effort through the shared provider configuration.
/// 通过共享 Provider 配置持久化当前工作台思考档位。
pub(super) async fn service_select_workbench_reasoning_effort(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: SessionId,
    effort: String,
) -> Result<WorkbenchSnapshot, String> {
    let selected_effort = parse_reasoning_effort(effort.trim())
        .ok_or_else(|| format!("Unknown reasoning level: {effort}"))?;
    let pending_requested_at_ms = crate::session_goal_lifecycle::current_v1_goal_unix_millis()?;
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, conversation_id)
            .await?;
    let workbench = mutate_provider_configuration_with_state_validation(
        &state,
        "Failed to save the reasoning level",
        |guard| {
            // workspace proves the caller-owned session still exists at the serialized
            // transaction boundary; selected_session is deliberately not consulted.
            // workspace 在串行事务边界证明调用方持有的会话仍然存在；这里有意不读取
            // selected_session。
            let workspace = guard
                .workspace_snapshot
                .as_ref()
                .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            if !workspace
                .sessions
                .iter()
                .any(|session| session.id == conversation_id)
            {
                return Err(format!(
                    "Conversation `{conversation_id}` is not an active workspace session."
                ));
            }
            Ok(())
        },
        |configuration| {
            // The provider configuration retains the process-wide default; the session-specific
            // next-request authority is committed separately through the V1 document below.
            // Provider 配置保留进程级默认值；会话级下一请求权威在下方通过 V1 文档独立提交。
            configuration.workbench_reasoning_effort = Some(selected_effort);
            Ok(())
        },
        |guard, configuration| -> Result<WorkbenchSnapshot, String> {
            let workspace = guard
                .workspace_snapshot
                .as_mut()
                .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            let runtime_state = workspace
                .session_runtime_state
                .get_mut(&conversation_id)
                .ok_or_else(|| format!("Conversation `{conversation_id}` has no runtime state."))?;
            let fallback =
                crate::session_request_config_v1::SessionRequestFallback::from_runtime_state(
                    runtime_state,
                );
            crate::session_request_config_v1::set_pending_reasoning_request_v1(
                handle.as_ref(),
                selected_effort,
                fallback,
                pending_requested_at_ms,
            )?;
            // Legacy runtime controls remain an in-memory WebUI adapter only and are not persisted.
            // 旧运行控制项仅保留为内存 WebUI 适配器，不再持久化。
            runtime_state.active_reasoning_effort = Some(selected_effort);
            runtime_state.updated_at = current_unix_millis_text();
            refresh_workbench_runtime_controls_locked(guard, configuration);
            Ok(guard.workbench.clone())
        },
    )
    .await??;

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
    use std::{collections::BTreeMap, sync::Arc};

    use vulcan_provider_adapters::{
        ActiveAiModelSelection, CredentialConnection, CredentialMode, PersistedAiConfiguration,
        ProtocolModelVariant, PublicAiRuntime, UnavailableCredentialSecretResolver,
        parse_openrouter_model_catalog,
    };
    use vulcan_workbench_model::{SessionPendingModelSelection, SessionRuntimeStateEntry};

    use super::exact_public_secondary_route_target;

    /// Verifies a pending next-turn model owns secondary routes before the old active model.
    /// 验证下一轮待生效模型优先于旧的当前模型持有二级路由。
    #[test]
    fn secondary_route_uses_pending_workbench_model() {
        let runtime = PublicAiRuntime::first_batch(Arc::new(UnavailableCredentialSecretResolver))
            .expect("runtime should initialize");
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
                    prompt_cache_by_protocol: BTreeMap::new(),
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
}
