//! settings runtime projection helpers.
//! settings runtime projection 辅助逻辑。

use super::*;

/// Emits one workbench snapshot to the WebUI event stream.
/// 向 WebUI 事件流发送单份工作台快照。
pub(crate) fn emit_workbench_event(app: &ServiceEventEmitter, workbench: &WorkbenchSnapshot) {
    emit_service_event_with_trace(
        app,
        "workbench_snapshot_emit",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
}

/// Emits one ordered Agent Skills domain delta to connected WebUI clients.
/// 向已连接 WebUI 客户端发送一条有序的 Agent Skills 领域增量。
pub(crate) fn emit_agent_skills_catalog_delta(
    app: &ServiceEventEmitter,
    update: AgentSkillsCatalogUpdate,
) {
    // envelope preserves the global workbench generation while replacing only the Agent Skills domain.
    // envelope 在仅替换 Agent Skills 领域时保留全局工作台版本顺序。
    let envelope = WorkbenchDeltaEnvelope {
        generation: update.revision,
        delta: WorkbenchDelta::AgentSkillsCatalogChanged(AgentSkillsCatalogChangedDelta {
            agent_skills: update.agent_skills,
        }),
    };
    emit_service_event_with_trace(
        app,
        "agent_skills_catalog_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(envelope)),
    );
}

/// Emits one session-addon namespace change event to connected UI clients.
/// 向已连接 UI 客户端发送一条会话附属状态命名空间变更事件。
///
/// # Parameters
/// 参数
/// `app` is the service event emitter.
/// `app` 是服务事件发送器。
/// `session_id` selects the changed session.
/// `session_id` 选择发生变化的会话。
/// `namespaces` are the addon namespaces that changed.
/// `namespaces` 是发生变化的附属状态命名空间。
/// `revision` is the latest known addon document revision.
/// `revision` 是当前已知的最新附属状态文档版本。
/// `updated_at` is the latest update timestamp.
/// `updated_at` 是最近更新时间戳。
pub(crate) fn emit_session_addon_changed_event(
    app: &ServiceEventEmitter,
    session_id: SessionId,
    namespaces: &[SessionAddonNamespace],
    revision: u64,
    updated_at: String,
) {
    let event = SessionAddonChangedEvent {
        session_id: session_id.to_string(),
        namespaces: namespaces
            .iter()
            .map(|namespace| namespace.as_key().to_string())
            .collect(),
        revision,
        updated_at,
        source_instance_id: None,
    };
    emit_service_event_with_trace(
        app,
        "session_addon_changed_emit",
        ServiceEvent::SessionAddonChanged(Box::new(event)),
    );
}

/// Builds a session addon document from a workspace snapshot and legacy runtime state.
/// 从工作区快照与旧运行态构建会话附属状态文档。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` supplies the legacy runtime state.
/// `workspace_snapshot` 提供旧运行态。
/// `session_id` selects the session being exported.
/// `session_id` 选择正在导出的会话。
/// `default_ai_tool_configuration` supplies the default AI capability template.
/// `default_ai_tool_configuration` 提供默认 AI 能力模板。
/// `namespaces` limits the addon namespaces exported from the runtime snapshot.
/// `namespaces` 限定从运行态快照导出的附属状态命名空间。
///
/// # Returns
/// 返回值
/// The derived addon state when the session runtime state exists.
/// 当会话运行态存在时返回派生附属状态。
pub(crate) fn session_addon_state_from_workspace_snapshot(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    default_ai_tool_configuration: &PersistedAiToolConfiguration,
    namespaces: &[SessionAddonNamespace],
) -> Option<SessionAddonState> {
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .or_else(|| {
            workspace_snapshot
                .archived_session_runtime_state
                .get(&session_id)
        })?;
    let updated_at = current_unix_millis_text();
    let mut addon_state = SessionAddonState::new(session_id);
    for namespace in namespaces {
        match namespace {
            SessionAddonNamespace::RuntimeControls => {
                if let Some(controls) = SessionRuntimeControls::from_runtime_state(runtime_state) {
                    let _ = addon_state.set_runtime_controls(&controls, updated_at.clone());
                }
            }
            SessionAddonNamespace::AiCapabilities => {
                let configuration = runtime_state
                    .ai_tool_configuration
                    .as_ref()
                    .unwrap_or(default_ai_tool_configuration);
                let _ = addon_state.set_ai_tool_configuration(configuration, updated_at.clone());
            }
            SessionAddonNamespace::Todolist => {
                if !runtime_state.todolist.is_empty() {
                    let _ = addon_state.set_todolist(&runtime_state.todolist, updated_at.clone());
                }
            }
            _ => {}
        }
    }
    (!addon_state.namespaces.is_empty()).then_some(addon_state)
}

/// Returns the latest update timestamp recorded in one addon document.
/// 返回一个附属状态文档中记录的最近更新时间。
///
/// # Parameters
/// 参数
/// `addon_state` is the addon document to inspect.
/// `addon_state` 是要检查的附属状态文档。
///
/// # Returns
/// 返回值
/// The latest namespace update timestamp, or the current timestamp when the document is empty.
/// 最新命名空间更新时间；文档为空时返回当前时间戳。
pub(crate) fn latest_session_addon_updated_at(addon_state: &SessionAddonState) -> String {
    addon_state
        .namespaces
        .values()
        .map(|entry| entry.updated_at.as_str())
        .max()
        .map(str::to_string)
        .unwrap_or_else(current_unix_millis_text)
}

/// Emits one service event and traces the serialized payload size when diagnostics are enabled.
/// 发送单条服务事件，并在诊断开启时记录序列化载荷大小。
pub(crate) fn emit_service_event_with_trace(
    app: &ServiceEventEmitter,
    stage: &str,
    event: ServiceEvent,
) {
    if webui_trace_enabled() {
        let payload_bytes = serde_json::to_vec(&event)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        webui_trace(stage, format!("payload_bytes={payload_bytes}"));
    }
    app.emit(event);
}

/// Refreshes settings-related projections on the retained workbench snapshot.
/// 刷新保留工作台快照上的设置相关投影。
pub(crate) fn refresh_settings_projection_locked(
    guard: &mut ServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: Option<&PersistedAiToolConfiguration>,
) {
    let ai_tool_configuration = ai_tool_configuration
        .cloned()
        .or_else(|| {
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| runtime.ai_tool_configuration.clone())
        })
        .unwrap_or_default();
    let selected_ai_tool_configuration = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&workspace.selected_session)
                .and_then(|runtime_state| runtime_state.ai_tool_configuration.clone())
        })
        .unwrap_or_else(|| ai_tool_configuration.clone());
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.ui_locale = provider_configuration.ui_locale.clone();
    guard.workbench.providers = providers(provider_configuration);
    guard.workbench.mcp_servers = mcp_servers(provider_configuration);
    guard.workbench.mcp_tools = mcp_tools(&cached_mcp_catalog_from_state(guard));
    guard.workbench.reasoning_selection = reasoning_selection(provider_configuration);
    guard.workbench.thinking_display_mode = workbench_thinking_display_mode(provider_configuration);
    guard.workbench.git_commit = git_commit_settings(provider_configuration);
    guard.workbench.host_tools = host_tools_settings(provider_configuration);
    let ai_capability_model_capabilities =
        active_provider_configuration_model_capabilities(provider_configuration);
    guard.workbench.ai_capabilities = ai_capabilities(
        &selected_ai_tool_configuration,
        ai_capability_model_capabilities.as_ref(),
    );
    guard.workbench.ai_capability_vendors = ai_capability_vendors(&selected_ai_tool_configuration);
    guard.workbench.ai_capability_vendor_catalog = ai_capability_vendor_catalog();
    guard.workbench.composer_attachment_capabilities =
        composer_attachment_capabilities(provider_configuration, &selected_ai_tool_configuration);
}

/// Returns the mutable runtime state for the currently selected regular session.
/// 返回当前选中常规会话的可变运行态。
///
/// # Parameters
/// 参数
/// `workspace` is the retained WebUI workspace snapshot.
/// `workspace` 是 WebUI 保留的工作区快照。
///
/// # Returns
/// 返回值
/// The selected session id and its mutable runtime-state entry when a regular session is selected.
/// 当当前选中常规会话时，返回会话 id 及其可变运行态条目。
pub(crate) fn selected_session_runtime_state_mut(
    workspace: &mut WorkspaceSnapshot,
) -> Option<(SessionId, &mut SessionRuntimeStateEntry)> {
    let session_id = workspace.selected_session;
    if session_id == SessionId::nil()
        || !workspace
            .sessions
            .iter()
            .any(|session| session.id == session_id)
    {
        return None;
    }
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: current_unix_millis_text(),
            ..SessionRuntimeStateEntry::default()
        });
    Some((session_id, runtime_state))
}

/// Returns the AI capability configuration owned by one session or the default template.
/// 返回指定会话拥有的 AI 能力配置，若会话尚无配置则返回默认模板。
///
/// # Parameters
/// 参数
/// `workspace` is the retained workspace snapshot that may contain session-scoped runtime state.
/// `workspace` 是可能包含会话级运行态的保留工作区快照。
/// `session_id` selects the session whose configuration should be used.
/// `session_id` 选择应使用其配置的会话。
/// `default_configuration` is the default AI capability template.
/// `default_configuration` 是默认 AI 能力配置模板。
///
/// # Returns
/// 返回值
/// The session-scoped AI capability configuration when present, otherwise the default template.
/// 存在时返回会话级 AI 能力配置，否则返回默认模板。
pub(crate) fn session_ai_tool_configuration_or_default<'a>(
    workspace: &'a WorkspaceSnapshot,
    session_id: SessionId,
    default_configuration: &'a PersistedAiToolConfiguration,
) -> &'a PersistedAiToolConfiguration {
    workspace
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.ai_tool_configuration.as_ref())
        .unwrap_or(default_configuration)
}

/// Initializes one session AI capability configuration from the default template when absent.
/// 在缺失时使用默认模板初始化单个会话的 AI 能力配置。
///
/// # Parameters
/// 参数
/// `workspace` is the retained workspace snapshot that owns the session runtime state.
/// `workspace` 是拥有会话运行态的保留工作区快照。
/// `session_id` selects the session that should receive its own configuration snapshot.
/// `session_id` 选择应接收独立配置快照的会话。
/// `default_configuration` is the default AI capability template.
/// `default_configuration` 是默认 AI 能力配置模板。
///
/// # Returns
/// 返回值
/// `true` when this call initialized the session configuration.
/// 当本次调用初始化了会话配置时返回 `true`。
pub(crate) fn ensure_session_ai_tool_configuration(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
    default_configuration: &PersistedAiToolConfiguration,
) -> bool {
    if session_id == SessionId::nil() {
        return false;
    }
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: current_unix_millis_text(),
            ..SessionRuntimeStateEntry::default()
        });
    if runtime_state.ensure_ai_tool_configuration(default_configuration) {
        runtime_state.updated_at = current_unix_millis_text();
        return true;
    }
    false
}

/// Refreshes model, reasoning, attachment, and context-usage projections after runtime controls change.
/// 在运行控制项变化后刷新模型、思考、附件与上下文占用投影。
///
/// # Parameters
/// 参数
/// `guard` is the locked service state being updated.
/// `guard` 是正在更新的服务状态锁。
/// `provider_configuration` is the effective provider configuration for the selected session.
/// `provider_configuration` 是当前选中会话的生效 Provider 配置。
/// `ai_tool_configuration` is the effective AI tool capability configuration.
/// `ai_tool_configuration` 是生效的 AI 工具能力配置。
pub(crate) fn refresh_workbench_runtime_controls_locked(
    guard: &mut ServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) {
    let context_usage = guard.workspace_snapshot.as_ref().and_then(|workspace| {
        context_usage_snapshot_from_runtime_state(
            workspace.selected_session,
            workspace
                .session_runtime_state
                .get(&workspace.selected_session),
            &guard.live_context_budgets,
            provider_configuration,
            ai_tool_configuration,
        )
    });
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.providers = providers(provider_configuration);
    guard.workbench.reasoning_selection = reasoning_selection(provider_configuration);
    guard.workbench.composer_attachment_capabilities =
        composer_attachment_capabilities(provider_configuration, ai_tool_configuration);
    guard.workbench.context_usage = context_usage;
}

/// Restores WebUI runtime controls from one session-scoped runtime state when possible.
/// 在可行时从单个会话运行态恢复 WebUI 运行控制项。
///
/// # Parameters
/// 参数
/// `provider_configuration` is the mutable provider configuration used to build the next workbench.
/// `provider_configuration` 是用于构建下一份工作台快照的可变 Provider 配置。
/// `runtime_state` is the selected session runtime-state entry.
/// `runtime_state` 是当前选中会话的运行态条目。
pub(crate) fn restore_webui_runtime_controls_for_session(
    provider_configuration: &mut ProviderConfigurationFile,
    runtime_state: Option<&SessionRuntimeStateEntry>,
) {
    let Some(runtime_state) = runtime_state else {
        return;
    };
    if let (Some(provider_id), Some(model_id)) = (
        runtime_state.active_provider_id.as_deref(),
        runtime_state.active_model_id.as_deref(),
    ) {
        let model_is_available =
            provider_config::effective_model_choices(provider_id, provider_configuration)
                .into_iter()
                .any(|choice| choice.id == model_id);
        if model_is_available {
            provider_configuration.model = Some(format!("{provider_id}/{model_id}"));
        }
    }
    if let Some(reasoning_effort) = runtime_state.active_reasoning_effort {
        provider_configuration.workbench_reasoning_effort = Some(reasoning_effort);
    }
}

/// Stores provider and MCP runtime changes after a configuration mutation.
/// 在配置变更后存储 Provider 与 MCP 运行时变化。
pub(crate) fn store_provider_and_mcp_configuration_after_change(
    state: &SharedServiceBridgeState,
    provider_configuration: ProviderConfigurationFile,
    next_mcp_runtime: McpRuntimeAccess,
) -> Result<WorkbenchSnapshot, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let ai_tool_configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.ai_tool_configuration.clone())
        .unwrap_or_default();
    if let Some(agent_runtime) = guard.agent_runtime.as_mut() {
        if let Some(previous_mcp_runtime) = agent_runtime.mcp_runtime.take() {
            previous_mcp_runtime.shutdown();
        }
        agent_runtime.provider_configuration = provider_configuration.clone();
        agent_runtime.mcp_runtime = Some(next_mcp_runtime);
    }
    refresh_settings_projection_locked(
        &mut guard,
        &provider_configuration,
        Some(&ai_tool_configuration),
    );
    Ok(guard.workbench.clone())
}

/// Returns the canonical WebUI request key for one provider field.
/// 返回单个 Provider 字段的规范 WebUI 请求键。
pub(crate) fn provider_field_request_key(field: ProviderFieldKey) -> &'static str {
    match field {
        ProviderFieldKey::ApiKey => "apiKey",
        ProviderFieldKey::BaseUrl => "baseUrl",
        ProviderFieldKey::ResourceName => "resourceName",
        ProviderFieldKey::AccountId => "accountId",
    }
}

/// Parses one MCP transport key from the WebUI request payload.
/// 从 WebUI 请求载荷解析单个 AI 工具类型键。
pub(crate) fn parse_ai_tool_kind(value: &str) -> Result<AiToolKind, String> {
    match value.trim() {
        "web_search" => Ok(AiToolKind::WebSearch),
        "understand_image" => Ok(AiToolKind::UnderstandImage),
        "image_generation" => Ok(AiToolKind::ImageGeneration),
        "video_generation" => Ok(AiToolKind::VideoGeneration),
        "speech_synthesis" => Ok(AiToolKind::SpeechSynthesis),
        "music_generation" => Ok(AiToolKind::MusicGeneration),
        "music_cover" => Ok(AiToolKind::MusicCover),
        other => Err(format!("不支持的 AI 能力类型：{other}")),
    }
}

/// Reprojects Agent Skills after a focused prompt-skill configuration change.
/// 在聚焦的提示词技能配置变更后重新投影 Agent Skills。
pub(crate) async fn refresh_agent_skills_local_state(
    state: &SharedServiceBridgeState,
) -> Result<AgentSkillsCatalogUpdate, String> {
    // Keep configuration capture and domain projection ordered with every skill configuration mutation.
    // 使配置捕获与领域投影和每次技能配置变更保持有序。
    let _configuration_transaction = begin_provider_configuration_transaction().await;
    let provider_configuration = provider_configuration_from_state(state)?;
    let summary = agent_skills_catalog_summary(&provider_configuration);
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.agent_skills = summary.clone();
    Ok(AgentSkillsCatalogUpdate {
        revision: guard.workbench_generation,
        agent_skills: summary,
    })
}

/// Reprojects Agent Skills only when the complete domain summary changed.
/// 仅在完整领域摘要发生变化时重新投影 Agent Skills。
///
/// # Parameters
/// 参数
/// - `state`: Shared service state containing the committed Agent Skills projection.
/// - `state`：包含已提交 Agent Skills 投影的共享服务状态。
///
/// # Returns
/// 返回值
/// The committed revisioned update, or `None` when the filesystem event produced no effective change.
/// 已提交的带版本更新；文件系统事件未产生有效变化时返回 `None`。
pub(crate) async fn refresh_agent_skills_local_state_if_changed(
    state: &SharedServiceBridgeState,
) -> Result<Option<AgentSkillsCatalogUpdate>, String> {
    // Keep configuration capture and projection ordered with concurrent settings mutations.
    // 使配置捕获和投影与并发设置变更保持有序。
    let _configuration_transaction = begin_provider_configuration_transaction().await;
    let provider_configuration = provider_configuration_from_state(state)?;
    let summary = agent_skills_catalog_summary(&provider_configuration);
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if !agent_skills_catalog_summary_changed(&guard.workbench.agent_skills, &summary) {
        return Ok(None);
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.agent_skills = summary.clone();
    Ok(Some(AgentSkillsCatalogUpdate {
        revision: guard.workbench_generation,
        agent_skills: summary,
    }))
}

/// Returns whether two complete Agent Skills domain summaries differ.
/// 返回两份完整 Agent Skills 领域摘要是否不同。
fn agent_skills_catalog_summary_changed(
    current: &AgentSkillsCatalogSummary,
    next: &AgentSkillsCatalogSummary,
) -> bool {
    current != next
}

/// Reprojects LuaSkills after a focused tool-registration configuration change.
/// 在聚焦的工具注册配置变更后重新投影 LuaSkills。
pub(crate) async fn refresh_luaskills_local_state(
    state: &SharedServiceBridgeState,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep registration-state capture ordered with every skill configuration mutation.
    // 使注册状态捕获与每次技能配置变更保持有序。
    let _configuration_transaction = begin_provider_configuration_transaction().await;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let provider_configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone())
        .unwrap_or_default();
    let catalog = cached_luaskills_catalog_from_state(&guard);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.lua_skills = lua_skills_catalog_summary(&catalog, &provider_configuration);
    guard.workbench.plugins = plugins_from_luaskills(&catalog, &provider_configuration);
    Ok(LuaSkillsCatalogUpdate {
        revision: guard.workbench_generation,
        lua_skills: guard.workbench.lua_skills.clone(),
        plugins: guard.workbench.plugins.clone(),
    })
}

/// Parses the project-level Agent Skill tri-state key from the WebUI.
/// 从 WebUI 解析项目级 Agent Skill 三态键。
pub(crate) fn parse_agent_skill_project_state(
    value: &str,
) -> Result<Option<PersistedPromptSkillState>, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "inherit" => Ok(None),
        "enabled" => Ok(Some(PersistedPromptSkillState::Enabled)),
        "disabled" => Ok(Some(PersistedPromptSkillState::Disabled)),
        other => Err(format!("不支持的项目级 Agent Skill 状态：{other}")),
    }
}

/// Parses a persisted LuaSkills tool registration state key from the WebUI.
/// 从 WebUI 解析持久化 LuaSkills 工具注册状态键。
pub(crate) fn parse_luaskill_tool_registration_state(
    value: &str,
) -> Result<PersistedLuaSkillToolRegistrationState, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "explicit" => Ok(PersistedLuaSkillToolRegistrationState::Explicit),
        "implicit" => Ok(PersistedLuaSkillToolRegistrationState::Implicit),
        "disabled" => Ok(PersistedLuaSkillToolRegistrationState::Disabled),
        other => Err(format!("不支持的 LuaSkills 工具注册状态：{other}")),
    }
}

#[cfg(test)]
mod agent_skills_catalog_delta_tests {
    use super::*;
    use vulcan_service_api::ServiceEventSink;

    /// Captures service events emitted by one focused unit test.
    /// 捕获单个聚焦单元测试发出的服务事件。
    struct CapturingServiceEventSink {
        /// Ordered captured service events.
        /// 按顺序捕获的服务事件。
        events: Arc<Mutex<Vec<ServiceEvent>>>,
    }

    impl ServiceEventSink for CapturingServiceEventSink {
        /// Records one emitted service event.
        /// 记录一条已发送服务事件。
        fn emit_service_event(&self, event: ServiceEvent) {
            self.events
                .lock()
                .expect("captured service events should remain lockable")
                .push(event);
        }
    }

    /// Verifies equal summaries do not request a new domain generation.
    /// 验证相同摘要不会请求新的领域版本。
    #[test]
    fn equal_agent_skills_summaries_do_not_change() {
        let summary = AgentSkillsCatalogSummary::default();
        assert!(!agent_skills_catalog_summary_changed(&summary, &summary));
    }

    /// Verifies one effective catalog field change requests a new domain generation.
    /// 验证任一有效目录字段变化都会请求新的领域版本。
    #[test]
    fn changed_agent_skills_summary_is_detected() {
        let current = AgentSkillsCatalogSummary::default();
        let next = AgentSkillsCatalogSummary {
            notice: String::from("changed"),
            ..current.clone()
        };
        assert!(agent_skills_catalog_summary_changed(&current, &next));
    }

    /// Verifies Agent Skills live updates use the ordered delta channel instead of a full snapshot.
    /// 验证 Agent Skills 实时更新使用有序增量通道而不是完整快照。
    #[test]
    fn agent_skills_catalog_update_emits_only_domain_delta() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let emitter = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
            events: Arc::clone(&events),
        })]);
        let summary = AgentSkillsCatalogSummary {
            notice: String::from("updated"),
            ..AgentSkillsCatalogSummary::default()
        };

        emit_agent_skills_catalog_delta(
            &emitter,
            AgentSkillsCatalogUpdate {
                revision: 7,
                agent_skills: summary.clone(),
            },
        );

        let captured = events
            .lock()
            .expect("captured service events should remain lockable");
        assert_eq!(captured.len(), 1);
        let ServiceEvent::WorkbenchDelta(envelope) = &captured[0] else {
            panic!("Agent Skills update must not emit a full workbench snapshot");
        };
        assert_eq!(envelope.generation, 7);
        assert_eq!(
            envelope.delta,
            WorkbenchDelta::AgentSkillsCatalogChanged(AgentSkillsCatalogChangedDelta {
                agent_skills: summary,
            })
        );
    }
}
