//! settings runtime projection helpers.
//! settings runtime projection 辅助逻辑。

use super::*;
use vulcan_service_api::{
    LuaSkillsCatalogChangedDelta, LuaSkillsManagedRuntimeProgressChangedDelta,
    LuaSkillsManagedRuntimeProgressSummary, PluginCatalogChangedDelta, ProjectSettingsChangedDelta,
    ProjectSettingsDomain, SubagentCatalogChangedDelta,
};

/// Carries one atomically committed plugin-owned catalog projection and its public RPC response.
/// 携带一份原子提交的插件所有目录投影及其公开 RPC 响应。
pub(crate) struct PluginCatalogProjectionUpdate {
    /// Monotonic workbench revision shared by every projected domain.
    /// 所有投影领域共享的工作台单调版本号。
    pub revision: u64,
    /// Complete LuaSkills catalog captured in the same transaction.
    /// 在同一事务中捕获的完整 LuaSkills 目录。
    pub lua_skills: LuaSkillsCatalogSummary,
    /// Plugin rows projected from the immutable plugin catalog snapshot.
    /// 从不可变插件目录快照投影出的插件行。
    pub plugins: Vec<PluginSummary>,
    /// Complete Agent Skills catalog containing plugin skills from the same snapshot.
    /// 包含同一快照插件 Skill 的完整 Agent Skills 目录。
    pub agent_skills: AgentSkillsCatalogSummary,
    /// Complete subagent catalog containing plugin agents from the same snapshot.
    /// 包含同一快照插件 Agent 的完整智能体目录。
    pub subagent_catalog: SubagentCatalogSummary,
}

impl PluginCatalogProjectionUpdate {
    /// Converts the atomic projection into the existing plugin-management RPC response.
    /// 将原子投影转换为现有插件管理 RPC 响应。
    pub(crate) fn lua_skills_update(&self) -> LuaSkillsCatalogUpdate {
        LuaSkillsCatalogUpdate {
            revision: self.revision,
            lua_skills: self.lua_skills.clone(),
            plugins: self.plugins.clone(),
        }
    }
}

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

/// Emits one ordered LuaSkills domain delta to every connected WebUI client.
/// 向每个已连接 WebUI 客户端发送一条有序的 LuaSkills 领域增量。
///
/// # Parameters
/// 参数
/// `app` broadcasts the event to all registered service sinks.
/// `app` 将事件广播给所有已注册的服务接收端。
/// `update` contains one committed revision, LuaSkills catalog, and matching plugin rows.
/// `update` 包含一个已提交版本、LuaSkills 目录及其匹配的插件行。
///
/// # Returns
/// 返回值
/// This function returns no value and emits exactly one domain event per sink.
/// 此函数无返回值，并向每个接收端恰好发送一条领域事件。
pub(crate) fn emit_luaskills_catalog_delta(
    app: &ServiceEventEmitter,
    update: LuaSkillsCatalogUpdate,
) {
    // envelope keeps LuaSkills and plugin rows on the same committed generation.
    // envelope 使 LuaSkills 与插件行保持在同一个已提交版本上。
    let envelope = WorkbenchDeltaEnvelope {
        generation: update.revision,
        delta: WorkbenchDelta::LuaSkillsCatalogChanged(Box::new(LuaSkillsCatalogChangedDelta {
            lua_skills: update.lua_skills,
            plugins: update.plugins,
        })),
    };
    emit_service_event_with_trace(
        app,
        "luaskills_catalog_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(envelope)),
    );
}

/// Emits one atomic plugin, LuaSkills, Agent Skills, and subagent catalog replacement.
/// 发送一条原子替换插件、LuaSkills、Agent Skills 与智能体目录的增量。
///
/// # Parameters
/// 参数
/// `app` broadcasts the event and `update` contains every domain projected from one catalog snapshot.
/// `app` 广播事件，`update` 包含从同一目录快照投影出的全部领域。
///
/// # Returns
/// 返回值
/// This function returns no value and emits exactly one ordered event per sink.
/// 此函数无返回值，并向每个接收端恰好发送一条有序事件。
pub(crate) fn emit_plugin_catalog_delta(
    app: &ServiceEventEmitter,
    update: &PluginCatalogProjectionUpdate,
) {
    // envelope applies every plugin-owned domain at one globally ordered revision.
    // envelope 在同一个全局有序版本上应用全部插件所有领域。
    let envelope = WorkbenchDeltaEnvelope {
        generation: update.revision,
        delta: WorkbenchDelta::PluginCatalogChanged(Box::new(PluginCatalogChangedDelta {
            lua_skills: update.lua_skills.clone(),
            plugins: update.plugins.clone(),
            agent_skills: update.agent_skills.clone(),
            subagent_catalog: update.subagent_catalog.clone(),
        })),
    };
    emit_service_event_with_trace(
        app,
        "plugin_catalog_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(envelope)),
    );
}

/// Emits one ordered subagent-catalog delta to every connected WebUI client.
/// 向每个已连接 WebUI 客户端发送一条有序的智能体目录增量。
///
/// # Parameters
/// 参数
/// `app` broadcasts the event to every registered service sink.
/// `app` 将事件广播给每个已注册的服务接收端。
/// `update` contains the committed revision and complete subagent catalog.
/// `update` 包含已提交版本与完整智能体目录。
///
/// # Returns
/// 返回值
/// This function returns no value and emits exactly one domain event per sink.
/// 此函数无返回值，并向每个接收端恰好发送一条领域事件。
pub(crate) fn emit_subagent_catalog_delta(
    app: &ServiceEventEmitter,
    update: SubagentCatalogUpdate,
) {
    // envelope owns the exact revision returned to the initiating RPC caller.
    // envelope 持有返回给发起 RPC 调用端的精确版本。
    let envelope = WorkbenchDeltaEnvelope {
        generation: update.revision,
        delta: WorkbenchDelta::SubagentCatalogChanged(Box::new(SubagentCatalogChangedDelta {
            subagent_catalog: update.subagent_catalog,
        })),
    };
    emit_service_event_with_trace(
        app,
        "subagent_catalog_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(envelope)),
    );
}

/// Emits one project-settings invalidation delta to every connected WebUI client.
/// 向每个已连接 WebUI 客户端发送一条项目设置失效增量。
///
/// # Parameters
/// 参数
/// `app` broadcasts the invalidation to every registered service sink.
/// `app` 将失效通知广播给每个已注册的服务接收端。
/// `generation` is the workbench revision committed for this invalidation.
/// `generation` 是为本次失效通知提交的工作台版本。
/// `project_id` selects one exact project, or every project when absent.
/// `project_id` 选择一个准确项目；为空时选择所有项目。
/// `domain` identifies the only independently loaded settings domain that changed.
/// `domain` 标识唯一发生变化的独立加载设置领域。
///
/// # Returns
/// 返回值
/// This function returns no value and emits exactly one focused event per sink.
/// 此函数无返回值，并向每个接收端恰好发送一条聚焦事件。
pub(crate) fn emit_project_settings_changed_delta(
    app: &ServiceEventEmitter,
    generation: u64,
    project_id: Option<String>,
    domain: ProjectSettingsDomain,
) {
    // envelope carries exact target and domain ownership so consumers never reload unrelated drafts.
    // envelope 携带准确目标与领域归属，避免消费端重载无关草稿。
    let envelope = WorkbenchDeltaEnvelope {
        generation,
        delta: WorkbenchDelta::ProjectSettingsChanged(ProjectSettingsChangedDelta {
            project_id,
            domain,
        }),
    };
    emit_service_event_with_trace(
        app,
        "project_settings_changed_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(envelope)),
    );
}

/// Commits and broadcasts one project-settings invalidation generation.
/// 提交并广播一个项目设置失效版本。
///
/// # Parameters
/// 参数
/// `app` broadcasts the focused invalidation event.
/// `app` 广播聚焦的失效事件。
/// `state` owns the monotonic workbench generation.
/// `state` 持有单调递增的工作台版本。
/// `project_id` is one exact project, or a global invalidation target when absent.
/// `project_id` 是一个准确项目；为空时表示全局失效目标。
/// `domain` identifies the only independently loaded settings domain that changed.
/// `domain` 标识唯一发生变化的独立加载设置领域。
///
/// # Returns
/// 返回值
/// `Ok(())` after the revision is committed and the event is emitted.
/// 版本提交且事件发送完成后返回 `Ok(())`。
pub(crate) fn commit_and_emit_project_settings_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    project_id: Option<String>,
    domain: ProjectSettingsDomain,
) -> Result<(), String> {
    // generation orders this invalidation against every other service-side workbench mutation.
    // generation 将本次失效通知与其他所有服务端工作台变更排序。
    let generation = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        let generation = guard.workbench_generation;
        // revisions stores the exact ownership boundary represented by the emitted delta.
        // revisions 保存所发送增量代表的准确归属边界。
        let revisions = match project_id.as_ref() {
            Some(project_id) => guard
                .workbench
                .project_settings_revisions
                .by_project
                .entry(project_id.clone())
                .or_default(),
            None => &mut guard.workbench.project_settings_revisions.global,
        };
        match domain {
            ProjectSettingsDomain::Vmm => revisions.vmm = generation,
            ProjectSettingsDomain::AgentSkills => revisions.agent_skills = generation,
            ProjectSettingsDomain::CodebaseMemory => revisions.codebase_memory = generation,
        }
        generation
    };
    emit_project_settings_changed_delta(app, generation, project_id, domain);
    Ok(())
}

/// Emits one revisioned LuaSkills managed-runtime progress delta.
/// 发送一条带版本的 LuaSkills 受管运行时进度增量。
///
/// # Parameters
/// 参数
/// `app` broadcasts the event to every registered service sink.
/// `app` 将事件广播给每个已注册的服务接收端。
/// `generation` is the workbench revision committed with this progress value.
/// `generation` 是与当前进度值一并提交的工作台版本。
/// `progress` is the current native installation progress or terminal absence.
/// `progress` 是当前原生安装进度或终态空值。
///
/// # Returns
/// 返回值
/// This function returns no value and emits one focused progress event per sink.
/// 此函数无返回值，并向每个接收端发送一条聚焦进度事件。
pub(crate) fn emit_luaskills_managed_runtime_progress_delta(
    app: &ServiceEventEmitter,
    generation: u64,
    progress: Option<LuaSkillsManagedRuntimeProgressSummary>,
) {
    // envelope replaces only progress and leaves the remaining LuaSkills catalog untouched.
    // envelope 仅替换进度，并保持 LuaSkills 目录其余内容不变。
    let envelope = WorkbenchDeltaEnvelope {
        generation,
        delta: WorkbenchDelta::LuaSkillsManagedRuntimeProgressChanged(
            LuaSkillsManagedRuntimeProgressChangedDelta { progress },
        ),
    };
    emit_service_event_with_trace(
        app,
        "luaskills_managed_runtime_progress_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(envelope)),
    );
}

/// Commits and broadcasts a changed LuaSkills managed-runtime progress value.
/// 提交并广播发生变化的 LuaSkills 受管运行时进度值。
///
/// # Parameters
/// 参数
/// `app` broadcasts the focused progress event.
/// `app` 广播聚焦的进度事件。
/// `state` owns the retained workbench projection and monotonic generation.
/// `state` 持有保留的工作台投影与单调版本。
/// `progress` is the current progress value or terminal absence.
/// `progress` 是当前进度值或终态空值。
///
/// # Returns
/// 返回值
/// `Ok(true)` when a changed value was committed, or `Ok(false)` for an identical value.
/// 发生变化并提交时返回 `Ok(true)`，值相同时返回 `Ok(false)`。
pub(crate) fn commit_and_emit_luaskills_managed_runtime_progress(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    progress: Option<LuaSkillsManagedRuntimeProgressSummary>,
) -> Result<bool, String> {
    // committed_generation remains absent when the retained progress already equals the requested value.
    // committed_generation 在保留进度已等于请求值时保持为空。
    let committed_generation = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if guard.workbench.lua_skills.managed_runtime_progress == progress {
            None
        } else {
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            guard.workbench.revision = guard.workbench_generation;
            guard.workbench.lua_skills.managed_runtime_progress = progress.clone();
            Some(guard.workbench_generation)
        }
    };
    let Some(generation) = committed_generation else {
        return Ok(false);
    };
    emit_luaskills_managed_runtime_progress_delta(app, generation, progress);
    Ok(true)
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
) {
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.ui_locale = provider_configuration.ui_locale.clone();
    guard.workbench.mcp_servers = mcp_servers(provider_configuration);
    guard.workbench.mcp_tools = mcp_tools(&cached_mcp_catalog_from_state(guard));
    guard.workbench.thinking_display_mode = workbench_thinking_display_mode(provider_configuration);
    guard.workbench.git_commit = git_commit_settings(provider_configuration);
    guard.workbench.provider_retry = provider_retry_settings(provider_configuration);
    guard.workbench.provider_request_trace_enabled =
        provider_configuration.provider_request_trace_enabled;
    guard.workbench.host_tools = host_tools_settings(provider_configuration);
    guard.workbench.output_style_catalog = output_style_catalog_summary(provider_configuration);
    // Settings refreshes use the same session-owned model projection as navigation, never global picker rows.
    // 设置刷新与导航使用同一会话自有模型投影，绝不使用全局选择器行。
    let mut selected_configuration = provider_configuration.clone();
    restore_webui_runtime_controls_for_session(
        &mut selected_configuration,
        guard.workspace_snapshot.as_ref().and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&workspace.selected_session)
        }),
    );
    sync_workbench_runtime_control_fields_locked(guard, &selected_configuration);
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
pub(crate) fn refresh_workbench_runtime_controls_locked(
    guard: &mut ServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
) {
    sync_workbench_runtime_control_fields_locked(guard, provider_configuration);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
}

/// Synchronizes session-sensitive model controls without changing the workbench revision.
/// 同步会话敏感的模型控制字段，但不修改工作台修订号。
///
/// # Parameters
/// 参数
/// `guard` owns the retained workspace, public AI runtime, and workbench projection.
/// `guard` 持有保留工作区、公共 AI 运行时与工作台投影。
/// `provider_configuration` is the selected session's restored runtime-control configuration.
/// `provider_configuration` 是当前会话恢复后的运行控制配置。
pub(crate) fn sync_workbench_runtime_control_fields_locked(
    guard: &mut ServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
) {
    // effectivePublicAiSelection prefers the selected session's immutable process-local route and
    // degrades to current persisted or global targets only when no coherent lease exists.
    // effectivePublicAiSelection 优先使用当前会话的进程内不可变路由；仅在不存在一致租约时，
    // 才降级到当前持久化目标或全局目标。
    let effective_public_ai_selection = guard.agent_runtime.as_ref().and_then(|agent_runtime| {
        let configuration = agent_runtime
            .public_ai_runtime
            .configuration_snapshot()
            .ok()?;
        let selected_session_id = guard
            .workspace_snapshot
            .as_ref()
            .map(|workspace| workspace.selected_session);
        let selected_runtime_state = guard.workspace_snapshot.as_ref().and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&workspace.selected_session)
        });
        public_ai_workbench_selection(
            &configuration,
            agent_runtime.public_ai_runtime.as_ref(),
            selected_session_id,
            selected_runtime_state,
        )
        .ok()
        .flatten()
    });
    // public_model_capabilities belongs to the exact effective session selection.
    // public_model_capabilities 属于精确的当前会话生效选择。
    let public_model_capabilities = effective_public_ai_selection
        .as_ref()
        .map(|selection| selection.model.capabilities.clone());
    let selected_context_usage_facts = guard.workspace_snapshot.as_ref().and_then(|workspace| {
        session_context_usage_facts_from_v1_registry(
            guard.session_runtime_registry.as_ref(),
            workspace.selected_session,
        )
        .ok()
        .flatten()
    });
    let context_usage = guard.workspace_snapshot.as_ref().and_then(|workspace| {
        context_usage_snapshot_from_runtime_state(
            workspace.selected_session,
            workspace
                .session_runtime_state
                .get(&workspace.selected_session),
            selected_context_usage_facts.as_ref(),
            &guard.live_context_budgets,
            public_model_capabilities.as_ref(),
            None,
        )
    });
    let bridged_media_kinds = guard
        .agent_runtime
        .as_ref()
        .and_then(|agent_runtime| {
            let configuration = agent_runtime
                .public_ai_runtime
                .configuration_snapshot()
                .ok()?;
            let runtime_state = guard.workspace_snapshot.as_ref().and_then(|workspace| {
                workspace
                    .session_runtime_state
                    .get(&workspace.selected_session)
            });
            crate::public_ai_settings::public_ai_session_media_understanding_bridge_kinds(
                agent_runtime.public_ai_runtime.as_ref(),
                &configuration,
                runtime_state,
            )
            .ok()
        })
        .unwrap_or_default();
    if let Some(agent_runtime) = guard.agent_runtime.as_ref()
        && let Ok(configuration) = agent_runtime.public_ai_runtime.configuration_snapshot()
        && let Ok(rows) = public_ai_provider_rows_for_selection(
            &configuration,
            agent_runtime.public_ai_runtime.as_ref(),
            effective_public_ai_selection.as_ref(),
        )
    {
        guard.workbench.providers = rows;
    }
    guard.workbench.reasoning_selection =
        reasoning_selection(provider_configuration, public_model_capabilities.as_ref());
    guard.workbench.composer_attachment_capabilities = composer_attachment_capabilities(
        effective_public_ai_selection
            .as_ref()
            .map(|selection| selection.route.protocol_route.protocol_profile_id.as_str()),
        public_model_capabilities.as_ref(),
        &bridged_media_kinds,
    );
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
    if runtime_state.active_reasoning_effort_explicit {
        let Some(reasoning_effort) = runtime_state.active_reasoning_effort else {
            return;
        };
        provider_configuration.workbench_reasoning_effort = Some(reasoning_effort);
        provider_configuration.workbench_reasoning_effort_explicit = true;
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
    if let Some(agent_runtime) = guard.agent_runtime.as_mut() {
        // retiredMcpRuntime keeps the replaced user partition addressable only for existing generation owners during drain.
        // retiredMcpRuntime 仅在排空期间为既有代次持有者保留被替换用户分区的可寻址性。
        let retired_mcp_runtime = agent_runtime.extension_generation.mcp_runtime();
        // nextGeneration preserves the exact plugin catalog while atomically publishing the new user MCP partition.
        // nextGeneration 保留精确插件目录，同时原子发布新的用户 MCP 分区。
        let next_generation = Arc::new(ExtensionGeneration::new(
            agent_runtime
                .extension_generation
                .revision()
                .saturating_add(1),
            agent_runtime.extension_generation.plugin_catalog(),
            next_mcp_runtime,
            agent_runtime.extension_generation.prompt_skill_catalog(),
        ));
        agent_runtime.provider_configuration = provider_configuration.clone();
        agent_runtime.extension_generation = next_generation;
        retired_mcp_runtime.retire_user_partition_after(std::time::Duration::from_secs(30));
    }
    refresh_settings_projection_locked(&mut guard, &provider_configuration);
    Ok(guard.workbench.clone())
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
    let plugin_catalog = service_vulcan_plugin_runtime(state)?.snapshot();
    let (prompt_skill_catalog, summary) = load_agent_skills_runtime_generation(
        &provider_configuration,
        Some(plugin_catalog.as_ref()),
    );
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    publish_prompt_skill_catalog_generation(&mut guard, prompt_skill_catalog);
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
    let plugin_catalog = service_vulcan_plugin_runtime(state)?.snapshot();
    let (prompt_skill_catalog, summary) = load_agent_skills_runtime_generation(
        &provider_configuration,
        Some(plugin_catalog.as_ref()),
    );
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if !agent_skills_catalog_summary_changed(&guard.workbench.agent_skills, &summary) {
        return Ok(None);
    }
    publish_prompt_skill_catalog_generation(&mut guard, prompt_skill_catalog);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.agent_skills = summary.clone();
    Ok(Some(AgentSkillsCatalogUpdate {
        revision: guard.workbench_generation,
        agent_skills: summary,
    }))
}

/// Loads one base prompt-skill generation once and derives its plugin-merged UI summary in memory.
/// 仅加载一次基础提示词技能代次，并在内存中派生合并插件后的 UI 摘要。
///
/// # Parameters
/// 参数
/// `provider_configuration` resolves exact roots and persisted state; `plugin_catalog` supplies one
/// already-published plugin generation.
/// `provider_configuration` 解析精确根与持久化状态；`plugin_catalog` 提供一份已经发布的插件代次。
///
/// # Returns
/// 返回值
/// The base runtime catalog and the complete user-facing summary derived without a second scan.
/// 返回基础运行时目录，以及无需第二次扫描派生的完整面向用户摘要。
fn load_agent_skills_runtime_generation(
    provider_configuration: &ProviderConfigurationFile,
    plugin_catalog: Option<&vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
) -> (
    Arc<vulcan_prompt_skills::PromptSkillCatalog>,
    AgentSkillsCatalogSummary,
) {
    let base_catalog = load_agent_skills_catalog(provider_configuration, None);
    let mut projected_catalog = base_catalog.clone();
    if let Some(plugin_catalog) = plugin_catalog
        && let Err(error) =
            projected_catalog.merge_plugin_skills(plugin_catalog.enabled_skills.values().cloned())
    {
        projected_catalog
            .errors
            .push(vulcan_prompt_skills::PromptSkillLoadError {
                path: plugin_catalog.root.clone(),
                message: error,
            });
    }
    let summary =
        agent_skills_catalog_summary_from_catalog(provider_configuration, &projected_catalog);
    (Arc::new(base_catalog), summary)
}

/// Publishes one preloaded base prompt-skill catalog while preserving plugin and MCP generations.
/// 发布一份预加载基础提示词技能目录，同时保留插件与 MCP 代次。
///
/// # Parameters
/// 参数
/// `state` is the already locked service state; `prompt_skill_catalog` is the exact replacement.
/// `state` 是已经加锁的服务状态；`prompt_skill_catalog` 是精确替代目录。
///
/// # Returns
/// 返回值
/// This in-place publication has no return value when no runtime exists yet.
/// 尚无运行时时，该原地发布不返回任何值。
fn publish_prompt_skill_catalog_generation(
    state: &mut ServiceBridgeState,
    prompt_skill_catalog: Arc<vulcan_prompt_skills::PromptSkillCatalog>,
) {
    let Some(agent_runtime) = state.agent_runtime.as_mut() else {
        return;
    };
    agent_runtime.extension_generation = Arc::new(ExtensionGeneration::new(
        agent_runtime
            .extension_generation
            .revision()
            .saturating_add(1),
        agent_runtime.extension_generation.plugin_catalog(),
        agent_runtime.extension_generation.mcp_runtime(),
        Some(prompt_skill_catalog),
    ));
}

/// Returns whether two complete Agent Skills domain summaries differ.
/// 返回两份完整 Agent Skills 领域摘要是否不同。
fn agent_skills_catalog_summary_changed(
    current: &AgentSkillsCatalogSummary,
    next: &AgentSkillsCatalogSummary,
) -> bool {
    current != next
}

/// Reprojects and commits every plugin-owned catalog domain from one immutable plugin snapshot.
/// 从同一份不可变插件快照重新投影并提交全部插件所有的目录领域。
///
/// # Parameters
/// 参数
/// `state` owns the plugin runtime, shared task runtime, and authoritative workbench snapshot.
/// `state` 持有插件运行时、共享任务运行时与权威工作台快照。
///
/// # Returns
/// 返回值
/// Returns one revisioned projection containing LuaSkills, plugins, Agent Skills, and subagents.
/// 返回一份包含 LuaSkills、插件、Agent Skills 与智能体的带版本投影。
pub(crate) async fn refresh_plugin_catalog_local_state(
    state: &SharedServiceBridgeState,
) -> Result<PluginCatalogProjectionUpdate, String> {
    // configuration_transaction orders the complete projection with concurrent settings mutations.
    // configuration_transaction 使完整投影与并发设置变更保持有序。
    let _configuration_transaction = begin_provider_configuration_transaction().await;
    // provider_configuration is the only persisted policy snapshot used by this projection.
    // provider_configuration 是本次投影使用的唯一持久化策略快照。
    let provider_configuration = provider_configuration_from_state(state)?;
    // plugin_catalog is immutable and supplies every plugin component in this transaction.
    // plugin_catalog 不可变，并为本次事务提供全部插件组件。
    let plugin_catalog = service_vulcan_plugin_runtime(state)?.snapshot();
    // agent_skills projects ordinary and plugin skills against the exact same plugin revision.
    // agent_skills 使用完全相同的插件版本投影普通 Skill 与插件 Skill。
    let agent_skills =
        agent_skills_catalog_summary(&provider_configuration, Some(plugin_catalog.as_ref()));
    // subagent_catalog starts from ordinary roots before namespaced plugin agents are merged.
    // subagent_catalog 先加载普通目录，再合并带命名空间的插件 Agent。
    let mut subagent_catalog = vulcan_task_session::load_subagent_catalog_from_roots(
        vulcan_runtime_assets::resolve_system_subagents_resource_directory(),
        vulcan_workspace_store::data_plane::application_data_root().join("agents"),
    );
    subagent_catalog
        .merge_plugin_definitions(plugin_catalog.enabled_agents.values().cloned())
        .map_err(|error| format!("合并插件智能体目录失败：{error}"))?;
    // subagent_summary is derived before the commit so no partial workbench mutation can escape.
    // subagent_summary 在提交前完成派生，避免任何局部工作台变更外泄。
    let available_model_selectors = public_ai_available_model_selectors(state)?;
    let subagent_summary =
        subagent_catalog_summary_from_catalog(&subagent_catalog, Some(&available_model_selectors));
    // guard serializes task-runtime replacement and every visible workbench field update.
    // guard 串行化任务运行时替换与所有可见工作台字段更新。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // lua_catalog is the committed ordinary LuaSkills snapshot paired with plugin rows.
    // lua_catalog 是与插件行配对的已提交普通 LuaSkills 快照。
    let lua_catalog = cached_luaskills_catalog_from_state(&guard);
    // lua_skills is rebuilt in the transaction so the compatibility RPC response is revision-consistent.
    // lua_skills 在事务内重建，使兼容 RPC 响应保持版本一致。
    let lua_skills = lua_skills_catalog_summary(&lua_catalog, &provider_configuration);
    let mcp_server_states = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.extension_generation.mcp_runtime().server_states())
        .unwrap_or_default();
    // plugins combines ordinary LuaSkills rows and the exact immutable plugin snapshot.
    // plugins 合并普通 LuaSkills 行与完全一致的不可变插件快照。
    let plugins = plugins_from_catalogs(
        &lua_catalog,
        &provider_configuration,
        Some(plugin_catalog.as_ref()),
        &mcp_server_states,
    );
    if let Some(runtime) = guard.agent_runtime.as_mut() {
        runtime
            .task_runtime
            .replace_subagent_catalog(subagent_catalog.clone())
            .map_err(|error| format!("刷新运行时插件智能体目录失败：{error}"))?;
        // subagentCatalog publishes the same immutable generation used by TaskRuntime and subsequent workbench projections.
        // subagentCatalog 发布与 TaskRuntime 及后续工作台投影共同使用的同一不可变代次。
        runtime.subagent_catalog = Arc::new(subagent_catalog);
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.lua_skills = lua_skills.clone();
    guard.workbench.plugins = plugins.clone();
    guard.workbench.agent_skills = agent_skills.clone();
    guard.workbench.subagent_catalog = subagent_summary.clone();
    Ok(PluginCatalogProjectionUpdate {
        revision: guard.workbench_generation,
        lua_skills,
        plugins,
        agent_skills,
        subagent_catalog: subagent_summary,
    })
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
    let vulcan_plugin_catalog = guard
        .agent_runtime
        .as_ref()
        .and_then(|runtime| runtime.extension_generation.plugin_catalog());
    let mcp_server_states = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.extension_generation.mcp_runtime().server_states())
        .unwrap_or_default();
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.lua_skills = lua_skills_catalog_summary(&catalog, &provider_configuration);
    guard.workbench.plugins = plugins_from_catalogs(
        &catalog,
        &provider_configuration,
        vulcan_plugin_catalog.as_deref(),
        &mcp_server_states,
    );
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
mod skills_catalog_delta_tests {
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

    /// Verifies one LuaSkills update reaches every connected sink exactly once.
    /// 验证单次 LuaSkills 更新会恰好一次到达每个已连接接收端。
    #[test]
    fn luaskills_catalog_update_broadcasts_once_per_connected_sink() {
        // first_events captures the first connected client independently.
        // first_events 独立捕获第一个已连接客户端收到的事件。
        let first_events = Arc::new(Mutex::new(Vec::new()));
        // second_events captures the second connected client independently.
        // second_events 独立捕获第二个已连接客户端收到的事件。
        let second_events = Arc::new(Mutex::new(Vec::new()));
        // emitter owns both client sinks for one broadcast operation.
        // emitter 为单次广播操作持有两个客户端接收端。
        let emitter = ServiceEventEmitter::new(vec![
            Arc::new(CapturingServiceEventSink {
                events: Arc::clone(&first_events),
            }),
            Arc::new(CapturingServiceEventSink {
                events: Arc::clone(&second_events),
            }),
        ]);
        // lua_skills identifies the exact catalog payload shared by both clients.
        // lua_skills 标识两个客户端共享的精确目录载荷。
        let lua_skills = LuaSkillsCatalogSummary {
            notice: String::from("updated"),
            ..LuaSkillsCatalogSummary::default()
        };
        // plugins identifies the plugin projection committed with the same revision.
        // plugins 标识与同一版本一并提交的插件投影。
        let plugins = Vec::new();

        emit_luaskills_catalog_delta(
            &emitter,
            LuaSkillsCatalogUpdate {
                revision: 11,
                lua_skills: lua_skills.clone(),
                plugins: plugins.clone(),
            },
        );

        for events in [&first_events, &second_events] {
            // captured locks one client's ordered event list for exact-once verification.
            // captured 锁定单个客户端的有序事件列表以验证恰好一次语义。
            let captured = events
                .lock()
                .expect("captured service events should remain lockable");
            assert_eq!(captured.len(), 1);
            let ServiceEvent::WorkbenchDelta(envelope) = &captured[0] else {
                panic!("LuaSkills update must not emit a full workbench snapshot");
            };
            assert_eq!(envelope.generation, 11);
            assert_eq!(
                envelope.delta,
                WorkbenchDelta::LuaSkillsCatalogChanged(Box::new(LuaSkillsCatalogChangedDelta {
                    lua_skills: lua_skills.clone(),
                    plugins: plugins.clone(),
                }))
            );
        }
    }

    /// Verifies one subagent catalog update reaches every connected sink exactly once.
    /// 验证单次智能体目录更新会恰好一次到达每个已连接接收端。
    #[test]
    fn subagent_catalog_update_broadcasts_once_per_connected_sink() {
        // first_events captures one connected client independently.
        // first_events 独立捕获一个已连接客户端收到的事件。
        let first_events = Arc::new(Mutex::new(Vec::new()));
        // second_events captures the other connected client independently.
        // second_events 独立捕获另一个已连接客户端收到的事件。
        let second_events = Arc::new(Mutex::new(Vec::new()));
        // emitter owns both service sinks for one authoritative publication.
        // emitter 为单次权威发布持有两个服务接收端。
        let emitter = ServiceEventEmitter::new(vec![
            Arc::new(CapturingServiceEventSink {
                events: Arc::clone(&first_events),
            }),
            Arc::new(CapturingServiceEventSink {
                events: Arc::clone(&second_events),
            }),
        ]);
        // subagent_catalog is the exact complete payload shared by both clients.
        // subagent_catalog 是两个客户端共享的精确完整载荷。
        let subagent_catalog = SubagentCatalogSummary {
            notice: String::from("updated"),
            ..SubagentCatalogSummary::default()
        };

        emit_subagent_catalog_delta(
            &emitter,
            SubagentCatalogUpdate {
                revision: 13,
                subagent_catalog: subagent_catalog.clone(),
            },
        );

        for events in [&first_events, &second_events] {
            // captured locks one client's ordered events for exact-once verification.
            // captured 锁定单个客户端的有序事件以验证恰好一次语义。
            let captured = events
                .lock()
                .expect("captured service events should remain lockable");
            assert_eq!(captured.len(), 1);
            let ServiceEvent::WorkbenchDelta(envelope) = &captured[0] else {
                panic!("subagent catalog update must use a focused delta");
            };
            assert_eq!(envelope.generation, 13);
            assert_eq!(
                envelope.delta,
                WorkbenchDelta::SubagentCatalogChanged(Box::new(SubagentCatalogChangedDelta {
                    subagent_catalog: subagent_catalog.clone(),
                }))
            );
        }
    }

    /// Verifies one plugin catalog update replaces every owned domain in one revisioned event.
    /// 验证单次插件目录更新会通过一条带版本事件替换全部所有领域。
    #[test]
    fn plugin_catalog_update_emits_one_atomic_multi_domain_delta() {
        // events captures the single connected client's ordered plugin event.
        // events 捕获单个已连接客户端的有序插件事件。
        let events = Arc::new(Mutex::new(Vec::new()));
        // emitter owns the focused test sink.
        // emitter 持有聚焦测试接收端。
        let emitter = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
            events: Arc::clone(&events),
        })]);
        // update carries distinct notices so every catalog field can be verified independently.
        // update 携带不同提示文本，以便独立验证每个目录字段。
        let update = PluginCatalogProjectionUpdate {
            revision: 19,
            lua_skills: LuaSkillsCatalogSummary {
                notice: String::from("lua"),
                ..LuaSkillsCatalogSummary::default()
            },
            plugins: Vec::new(),
            agent_skills: AgentSkillsCatalogSummary {
                notice: String::from("skills"),
                ..AgentSkillsCatalogSummary::default()
            },
            subagent_catalog: SubagentCatalogSummary {
                notice: String::from("agents"),
                ..SubagentCatalogSummary::default()
            },
        };

        emit_plugin_catalog_delta(&emitter, &update);

        let captured = events
            .lock()
            .expect("captured service events should remain lockable");
        assert_eq!(captured.len(), 1);
        let ServiceEvent::WorkbenchDelta(envelope) = &captured[0] else {
            panic!("plugin catalog update must use one focused delta");
        };
        assert_eq!(envelope.generation, 19);
        assert_eq!(
            envelope.delta,
            WorkbenchDelta::PluginCatalogChanged(Box::new(PluginCatalogChangedDelta {
                lua_skills: update.lua_skills.clone(),
                plugins: update.plugins.clone(),
                agent_skills: update.agent_skills.clone(),
                subagent_catalog: update.subagent_catalog.clone(),
            }))
        );
    }

    /// Verifies managed-runtime progress uses one focused revisioned event.
    /// 验证受管运行时进度使用一条聚焦且带版本的事件。
    #[test]
    fn luaskills_managed_runtime_progress_emits_focused_delta() {
        // events captures the single connected client's progress event.
        // events 捕获单个已连接客户端的进度事件。
        let events = Arc::new(Mutex::new(Vec::new()));
        // emitter owns the focused test sink.
        // emitter 持有聚焦测试接收端。
        let emitter = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
            events: Arc::clone(&events),
        })]);
        // progress is the exact native progress payload expected by the client.
        // progress 是客户端预期收到的精确原生进度载荷。
        let progress = LuaSkillsManagedRuntimeProgressSummary {
            phase: String::from("download"),
            overall_percent: Some(50),
            ..LuaSkillsManagedRuntimeProgressSummary::default()
        };

        emit_luaskills_managed_runtime_progress_delta(&emitter, 17, Some(progress.clone()));

        let captured = events
            .lock()
            .expect("captured service events should remain lockable");
        assert_eq!(captured.len(), 1);
        let ServiceEvent::WorkbenchDelta(envelope) = &captured[0] else {
            panic!("managed-runtime progress must use a focused delta");
        };
        assert_eq!(envelope.generation, 17);
        assert_eq!(
            envelope.delta,
            WorkbenchDelta::LuaSkillsManagedRuntimeProgressChanged(
                LuaSkillsManagedRuntimeProgressChangedDelta {
                    progress: Some(progress),
                }
            )
        );
    }
}
