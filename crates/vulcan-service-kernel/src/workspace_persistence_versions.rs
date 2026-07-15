//! Workspace persistence version guards.
//! 工作区持久化版本防护。

use super::*;

/// Allocates the next background persistence version for one WebUI session.
/// 为一个 WebUI 会话分配下一次后台持久化版本。
///
/// # Parameters
/// 参数
/// `state` owns the per-session persistence version table.
/// `state` 持有按会话归属的持久化版本表。
/// `session_id` selects the session being persisted.
/// `session_id` 选择正在持久化的会话。
///
/// # Returns
/// 返回值
/// The version assigned to the new background persistence task.
/// 返回分配给新后台持久化任务的版本。
pub(crate) fn next_webui_session_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<u64, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let next_version = guard
        .session_persist_versions
        .get(&session_id)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    guard
        .session_persist_versions
        .insert(session_id, next_version);
    Ok(next_version)
}

/// Returns whether a background persistence task is still the latest one for its session.
/// 判断某个后台持久化任务是否仍是其会话的最新任务。
///
/// # Parameters
/// 参数
/// `state` owns the per-session persistence version table.
/// `state` 持有按会话归属的持久化版本表。
/// `session_id` selects the session being persisted.
/// `session_id` 选择正在持久化的会话。
/// `persist_version` is the task-local persistence version.
/// `persist_version` 是任务本地持久化版本。
///
/// # Returns
/// 返回值
/// `true` when the task may write to disk.
/// 当该任务可以写入磁盘时返回 `true`。
pub(crate) fn webui_session_persist_version_is_current(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    persist_version: u64,
) -> bool {
    state
        .lock()
        .ok()
        .and_then(|guard| guard.session_persist_versions.get(&session_id).copied())
        == Some(persist_version)
}

/// Invalidates older background persistence snapshots for one WebUI session.
/// 使某个 WebUI 会话上较旧的后台持久化快照失效。
///
/// # Parameters
/// 参数
/// `state` owns the per-session persistence version table.
/// `state` 持有按会话归属的持久化版本表。
/// `session_id` selects the session whose pending background writes should be invalidated.
/// `session_id` 选择需要使待处理后台写入失效的会话。
pub(crate) fn invalidate_webui_session_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    let _ = next_webui_session_persist_version(state, session_id)?;
    Ok(())
}

/// Invalidates every pending persistence channel before or after a session lifecycle transition.
/// 在会话生命周期转换前后使全部待处理持久化通道失效。
///
/// # Parameters
/// 参数
/// `state` owns whole-session, transcript, and addon persistence version tables.
/// `state` 持有整会话、转录与附属状态持久化版本表。
/// `session_id` selects the session whose stale background writes must be fenced out.
/// `session_id` 选择必须隔离过期后台写入的会话。
///
/// # Returns
/// 返回值
/// Returns success after all currently allocated persistence channels advance atomically under one state lock.
/// 全部当前已分配持久化通道在同一状态锁下原子推进后返回成功。
pub(crate) fn invalidate_webui_session_lifecycle_persist_versions(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    // guard advances every persistence domain in one coherent critical section.
    // guard 在一个一致临界区内推进每个持久化领域。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // next_session_version invalidates whole-session and runtime-state persistence tasks.
    // next_session_version 使整会话与运行态持久化任务失效。
    let next_session_version = guard
        .session_persist_versions
        .get(&session_id)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    guard
        .session_persist_versions
        .insert(session_id, next_session_version);
    // next_transcript_version invalidates transcript-only persistence tasks.
    // next_transcript_version 使仅转录持久化任务失效。
    let next_transcript_version = guard
        .session_transcript_persist_versions
        .get(&session_id)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    guard
        .session_transcript_persist_versions
        .insert(session_id, next_transcript_version);
    // addon_keys capture every namespace that already has a pending or completed version lineage.
    // addon_keys 捕获已经具有待处理或已完成版本谱系的每个命名空间。
    let addon_keys = guard
        .session_addon_persist_versions
        .keys()
        .filter(|(candidate_session_id, _namespace)| *candidate_session_id == session_id)
        .copied()
        .collect::<Vec<_>>();
    for addon_key in addon_keys {
        // next_addon_version invalidates the pending writer for this exact namespace.
        // next_addon_version 使该精确命名空间的待处理写入者失效。
        let next_addon_version = guard
            .session_addon_persist_versions
            .get(&addon_key)
            .copied()
            .unwrap_or_default()
            .saturating_add(1);
        guard
            .session_addon_persist_versions
            .insert(addon_key, next_addon_version);
    }
    Ok(())
}

/// Allocates the next live transcript persistence version for one WebUI session.
/// 为一个 WebUI 会话分配下一次运行中转录持久化版本。
///
/// # Parameters
/// # 参数
/// `state` owns the per-session live transcript persistence version table.
/// `state` 持有按会话归属的运行中转录持久化版本表。
/// `session_id` selects the session whose transcript is being persisted.
/// `session_id` 选择正在持久化转录的会话。
///
/// # Returns
/// # 返回值
/// The version assigned to the new live transcript persistence task.
/// 返回分配给新运行中转录持久化任务的版本。
pub(crate) fn next_webui_session_transcript_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<u64, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let next_version = guard
        .session_transcript_persist_versions
        .get(&session_id)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    guard
        .session_transcript_persist_versions
        .insert(session_id, next_version);
    Ok(next_version)
}

/// Returns whether a live transcript persistence task is still current for its session.
/// 判断某个运行中转录持久化任务是否仍是其会话的最新任务。
///
/// # Parameters
/// # 参数
/// `state` owns the per-session live transcript persistence version table.
/// `state` 持有按会话归属的运行中转录持久化版本表。
/// `session_id` selects the session being persisted.
/// `session_id` 选择正在持久化的会话。
/// `persist_version` is the task-local transcript persistence version.
/// `persist_version` 是任务本地转录持久化版本。
///
/// # Returns
/// # 返回值
/// `true` when the transcript task may write to disk.
/// 当该转录任务可以写入磁盘时返回 `true`。
pub(crate) fn webui_session_transcript_persist_version_is_current(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    persist_version: u64,
) -> bool {
    state.lock().ok().and_then(|guard| {
        guard
            .session_transcript_persist_versions
            .get(&session_id)
            .copied()
    }) == Some(persist_version)
}

/// Allocates the next background addon persistence version for one session namespace.
/// 为一个会话命名空间分配下一次后台附属状态持久化版本。
///
/// # Parameters
/// 参数
/// `state` owns the per-session addon persistence version table.
/// `state` 持有按会话划分的附属状态持久化版本表。
/// `session_id` selects the session whose addon namespace is being persisted.
/// `session_id` 选择正在持久化附属状态命名空间的会话。
/// `namespace` selects the addon namespace being persisted.
/// `namespace` 选择正在持久化的附属状态命名空间。
///
/// # Returns
/// 返回值
/// The version assigned to the new background addon persistence task for that namespace.
/// 返回分配给该命名空间的新后台附属状态持久化任务版本。
pub(crate) fn next_webui_session_addon_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    namespace: SessionAddonNamespace,
) -> Result<u64, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let key = (session_id, namespace);
    let next_version = guard
        .session_addon_persist_versions
        .get(&key)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    guard
        .session_addon_persist_versions
        .insert(key, next_version);
    Ok(next_version)
}

/// Returns whether a background addon persistence task is still latest for every namespace it writes.
/// 判断某个后台附属状态持久化任务是否仍是其写入的所有命名空间的最新任务。
///
/// # Parameters
/// 参数
/// `state` owns the per-session addon persistence version table.
/// `state` 持有按会话划分的附属状态持久化版本表。
/// `session_id` selects the session whose addon namespaces are being persisted.
/// `session_id` 选择正在持久化附属状态命名空间的会话。
/// `persist_versions` are the task-local addon persistence versions keyed by namespace.
/// `persist_versions` 是按命名空间记录的任务本地附属状态持久化版本。
///
/// # Returns
/// 返回值
/// `true` when the task may write the addon document.
/// 当该任务可以写入附属状态文档时返回 `true`。
pub(crate) fn webui_session_addon_persist_versions_are_current(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    persist_versions: &[(SessionAddonNamespace, u64)],
) -> bool {
    let Ok(guard) = state.lock() else {
        return false;
    };
    persist_versions.iter().all(|(namespace, version)| {
        guard
            .session_addon_persist_versions
            .get(&(session_id, *namespace))
            .copied()
            == Some(*version)
    })
}

/// Invalidates older background addon persistence snapshots for one session namespace.
/// 使某个会话命名空间上较旧的后台附属状态持久化快照失效。
///
/// # Parameters
/// 参数
/// `state` owns the per-session addon persistence version table.
/// `state` 持有按会话划分的附属状态持久化版本表。
/// `session_id` selects the session whose pending addon namespace writes should be invalidated.
/// `session_id` 选择需要使待处理附属状态命名空间写入失效的会话。
/// `namespace` selects the addon namespace whose pending writes should be invalidated.
/// `namespace` 选择需要使待处理写入失效的附属状态命名空间。
pub(crate) fn invalidate_webui_session_addon_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    namespace: SessionAddonNamespace,
) -> Result<(), String> {
    let _ = next_webui_session_addon_persist_version(state, session_id, namespace)?;
    Ok(())
}

/// Builds an empty LuaSkills catalog for degraded snapshot rendering.
/// 为降级快照渲染构建空 LuaSkills 目录。
pub(crate) fn empty_luaskills_catalog() -> TauriLuaSkillsCatalog {
    TauriLuaSkillsCatalog {
        runtime_root: PathBuf::new(),
        managed_runtime_distribution_root: PathBuf::new(),
        managed_runtime_environment_root: PathBuf::new(),
        managed_runtime_report: Default::default(),
        managed_runtime_progress: None,
        system_lua_lib_dir: PathBuf::new(),
        skills: Vec::new(),
        entry_count: 0,
        runtime_settings: vulcan_luaskills_config::LuaSkillsRuntimeSettings::default(),
        runtime_settings_file: PathBuf::new(),
        restart_required: false,
    }
}

/// Builds an empty MCP catalog for degraded snapshot rendering.
/// 为降级快照渲染构建空 MCP 目录。
pub(crate) fn empty_mcp_catalog() -> TauriMcpRuntimeCatalog {
    TauriMcpRuntimeCatalog {
        configured_server_count: 0,
        enabled_server_count: 0,
        connected_server_count: 0,
        tool_count: 0,
        prompt_count: 0,
        resource_count: 0,
        summary_rows: Vec::new(),
        tools: Vec::new(),
    }
}

/// Returns the cached LuaSkills catalog without touching the LuaSkills engine on hot paths.
/// 在热路径上返回缓存的 LuaSkills 目录，避免触碰 LuaSkills 引擎。
pub(crate) fn cached_luaskills_catalog_from_state(
    state: &ServiceBridgeState,
) -> TauriLuaSkillsCatalog {
    state
        .cached_luaskills_catalog
        .clone()
        .unwrap_or_else(empty_luaskills_catalog)
}

/// Returns the cached MCP catalog without querying the MCP runtime on hot paths.
/// 在热路径上返回缓存的 MCP 目录，避免查询 MCP 运行时。
pub(crate) fn cached_mcp_catalog_from_state(state: &ServiceBridgeState) -> TauriMcpRuntimeCatalog {
    state
        .cached_mcp_catalog
        .clone()
        .unwrap_or_else(empty_mcp_catalog)
}

/// Parses a user-facing optional session identifier and ignores local draft identifiers.
/// 解析用户界面传入的可选会话标识，并忽略本地草稿标识。
pub(crate) fn parse_session_id(value: &str) -> Option<SessionId> {
    value.parse::<SessionId>().ok()
}

/// Converts the composer reasoning string into the shared provider-config enum.
/// 将输入区思考档位字符串转换为共享 provider-config 枚举。
pub(crate) fn parse_reasoning_effort(value: &str) -> Option<ReasoningEffortLevel> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Some(ReasoningEffortLevel::Off),
        "low" => Some(ReasoningEffortLevel::Low),
        "medium" => Some(ReasoningEffortLevel::Medium),
        "high" => Some(ReasoningEffortLevel::High),
        "x-high" | "max" => Some(ReasoningEffortLevel::XHigh),
        _ => None,
    }
}

/// Converts one shared reasoning-effort enum into the WebUI wire value.
/// 将单个共享思考档位枚举转换为 WebUI 传输值。
pub(crate) fn reasoning_effort_wire_value(effort: ReasoningEffortLevel) -> &'static str {
    match effort {
        ReasoningEffortLevel::Off => "off",
        ReasoningEffortLevel::Low => "low",
        ReasoningEffortLevel::Medium => "medium",
        ReasoningEffortLevel::High => "high",
        ReasoningEffortLevel::XHigh => "x-high",
    }
}

/// Converts the WebUI thinking display string into the shared provider-config enum.
/// 将 WebUI 思考展示字符串转换为共享 provider-config 枚举。
pub(crate) fn parse_thinking_display_mode(value: &str) -> Option<WorkbenchThinkingDisplayMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "popup_only" => Some(WorkbenchThinkingDisplayMode::PopupOnly),
        "inline_output" => Some(WorkbenchThinkingDisplayMode::InlineOutput),
        "first_inline_with_popup" => Some(WorkbenchThinkingDisplayMode::FirstInlineWithPopup),
        "first_inline_only" => Some(WorkbenchThinkingDisplayMode::FirstInlineOnly),
        _ => None,
    }
}

/// Converts one shared thinking display enum into the WebUI wire value.
/// 将单个共享思考展示枚举转换为 WebUI 传输值。
pub(crate) fn thinking_display_mode_wire_value(mode: WorkbenchThinkingDisplayMode) -> &'static str {
    match mode {
        WorkbenchThinkingDisplayMode::PopupOnly => "popup_only",
        WorkbenchThinkingDisplayMode::InlineOutput => "inline_output",
        WorkbenchThinkingDisplayMode::FirstInlineWithPopup => "first_inline_with_popup",
        WorkbenchThinkingDisplayMode::FirstInlineOnly => "first_inline_only",
    }
}

/// Resolves the WebUI thinking display mode from the shared provider configuration.
/// 从共享 Provider 配置解析 WebUI 思考展示模式。
pub(crate) fn workbench_thinking_display_mode(
    provider_configuration: &ProviderConfigurationFile,
) -> String {
    thinking_display_mode_wire_value(
        provider_configuration
            .workbench_thinking_display_mode
            .unwrap_or_default(),
    )
    .to_string()
}

/// Resolves the effective reasoning effort for the active model using the shared selector view.
/// 使用共享选择器视图解析当前激活模型的生效思考档位。
pub(crate) fn active_reasoning_selection_effort(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ReasoningEffortLevel> {
    let preferred_effort = provider_configuration
        .workbench_reasoning_effort
        .unwrap_or(ReasoningEffortLevel::Medium);
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let active_model =
        provider_config::effective_model_choices(provider_id, provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    Some(
        definition
            .reasoning_selection_view(active_model.as_ref(), preferred_effort)
            .selected_effort,
    )
}

/// Builds the WebUI reasoning selector snapshot from the shared provider configuration.
/// 从共享 Provider 配置构建 WebUI 思考选择器快照。
pub(crate) fn reasoning_selection(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ReasoningSelectionSummary> {
    let preferred_effort = provider_configuration
        .workbench_reasoning_effort
        .unwrap_or(ReasoningEffortLevel::Medium);
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let active_model =
        provider_config::effective_model_choices(provider_id, provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    let view = definition.reasoning_selection_view(active_model.as_ref(), preferred_effort);
    Some(ReasoningSelectionSummary {
        options: view
            .options
            .into_iter()
            .map(|option| ReasoningSelectionOptionSummary {
                value: reasoning_effort_wire_value(option.effort).to_string(),
                label: option.label,
            })
            .collect(),
        selected_effort: reasoning_effort_wire_value(view.selected_effort).to_string(),
        note: view.note,
    })
}

/// Creates a readable generated session title from the submitted prompt.
/// 根据提交提示词创建可读的自动会话标题。
pub(crate) fn prompt_to_session_title(prompt: &str) -> String {
    let title = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("新会话");
    title.chars().take(48).collect()
}

/// Describes one cold-path full workbench snapshot refresh request.
/// 描述一次冷路径全量工作台快照刷新请求。
pub(crate) struct FullWorkbenchColdRefreshRequest<'a> {
    /// Emits the rebuilt workbench snapshot to WebUI subscribers.
    /// 负责向 WebUI 订阅者发送重建后的工作台快照。
    pub(crate) app: &'a ServiceEventEmitter,
    /// Owns the cached runtime state used while rebuilding the surface.
    /// 持有重建界面时需要读取的运行时缓存状态。
    pub(crate) state: &'a SharedServiceBridgeState,
    /// Provides controller-level persistence and filesystem services.
    /// 提供控制器级持久化与文件系统服务。
    pub(crate) controller_runtime: &'a ControllerRuntimeHandle,
    /// Provides the LuaSkills catalog projected into the workbench snapshot.
    /// 提供需要投影到工作台快照中的 LuaSkills 目录。
    pub(crate) luaskills_catalog: &'a TauriLuaSkillsCatalog,
    /// Provides token-budget runtime data for model context displays.
    /// 提供模型上下文显示需要的 token 预算运行时数据。
    pub(crate) token_budget_runtime: &'a TauriTokenBudgetRuntime,
    /// Provides MCP server and tool summaries for the workbench snapshot.
    /// 提供工作台快照需要的 MCP 服务与工具摘要。
    pub(crate) mcp_catalog: &'a TauriMcpRuntimeCatalog,
    /// Provides configured provider and model choices.
    /// 提供已配置的供应商与模型选项。
    pub(crate) provider_configuration: &'a ProviderConfigurationFile,
    /// Provides persisted AI tool visibility configuration.
    /// 提供已持久化的 AI 工具可见性配置。
    pub(crate) ai_tool_configuration: &'a PersistedAiToolConfiguration,
    /// Carries the canonical workspace state being committed.
    /// 携带即将提交的规范工作区状态。
    pub(crate) workspace_snapshot: WorkspaceSnapshot,
    /// Carries service logs to project into the rebuilt workbench.
    /// 携带需要投影到重建工作台中的服务日志。
    pub(crate) logs: &'a [String],
    /// Selects whether subagent navigation should be rebuilt from state or preserved from cache.
    /// 选择子代理导航应从状态重建还是从缓存保留。
    pub(crate) subagent_navigation_mode: SubagentNavigationRefreshMode,
}

/// Stores the canonical workspace snapshot without rebuilding or emitting a full workbench snapshot.
/// 存储规范工作区快照，但不重建或发送全量工作台快照。
///
/// # Parameters
/// 参数
/// `state` owns the retained workspace and hydrated conversation window caches.
/// `state` 持有保留工作区与已水合会话窗口缓存。
/// `workspace_snapshot` is the canonical workspace snapshot to make visible to later service steps.
/// `workspace_snapshot` 是需要对后续服务步骤可见的规范工作区快照。
///
/// # Returns
/// 返回值
/// `Ok(())` after the snapshot is committed, or a lock diagnostic when service state cannot be acquired.
/// 快照提交后返回 `Ok(())`，若无法取得服务状态锁则返回诊断信息。
pub(crate) fn store_workspace_snapshot_without_workbench_rebuild(
    state: &SharedServiceBridgeState,
    workspace_snapshot: WorkspaceSnapshot,
) -> Result<(), String> {
    let trace_started_at = current_unix_millis_value();
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    retain_hydrated_conversation_windows_for_workspace(
        &mut guard.hydrated_conversation_windows,
        &workspace_snapshot,
    );
    guard.workspace_snapshot = Some(workspace_snapshot);
    webui_submit_trace(
        "store_workspace_snapshot_committed",
        format!(
            "elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_started_at)
        ),
    );
    Ok(())
}

/// Commits one cold-path workspace and workbench projection under the caller's state lock.
/// 在调用方持有的状态锁下提交一次冷路径工作区与工作台投影。
///
/// # Parameters
/// 参数
/// `state` is the exclusively locked retained state, `workspace_snapshot` is authoritative, and `workbench` is its matching projection.
/// `state` 是独占锁定的保留状态，`workspace_snapshot` 是权威工作区，`workbench` 是与其匹配的投影。
///
/// # Returns
/// 返回值
/// Returns the committed workbench with the newly allocated revision.
/// 返回带有新分配修订号的已提交工作台。
fn commit_cold_workbench_projection(
    state: &mut ServiceBridgeState,
    workspace_snapshot: WorkspaceSnapshot,
    mut workbench: WorkbenchSnapshot,
) -> WorkbenchSnapshot {
    crate::preserve_project_settings_revisions(&mut workbench, &state.workbench);
    state.workbench_generation = state.workbench_generation.wrapping_add(1);
    workbench.revision = state.workbench_generation;
    state.workspace_snapshot = Some(workspace_snapshot);
    state.workbench = workbench.clone();
    workbench
}

/// Stores and emits one rebuilt workbench snapshot after a cold-path workspace mutation.
/// 在冷路径工作区变更后存储并发送一份重建后的工作台快照。
///
/// # Parameters
/// 参数
/// `request` groups the runtime handles, workspace snapshot, logs, and subagent navigation mode for this refresh.
/// `request` 聚合本次刷新需要的运行时句柄、工作区快照、日志与子代理导航模式。
///
/// # Returns
/// 返回值
/// The rebuilt workbench snapshot after it has been stored and emitted.
/// 返回已存储并发送后的重建工作台快照。
pub(crate) fn store_and_emit_full_workbench_snapshot_for_cold_refresh(
    request: FullWorkbenchColdRefreshRequest<'_>,
) -> Result<WorkbenchSnapshot, String> {
    let FullWorkbenchColdRefreshRequest {
        app,
        state,
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        workspace_snapshot,
        logs,
        subagent_navigation_mode,
    } = request;
    let trace_started_at = current_unix_millis_value();
    let trace_lock_wait_started_at = current_unix_millis_value();
    // guard owns one uninterrupted capture, projection, and commit transaction.
    // guard 持有不中断的捕获、投影与提交事务。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let (
        live_context_budgets,
        surface_messages,
        subagent_navigation,
        pending_questions,
        session_goal_summaries,
        command_session_runtime,
        native_tools,
        hydrated_window_summaries,
        system_plugin_catalog,
    ) = {
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &workspace_snapshot,
        );
        webui_trace(
            "store_workbench_lock_wait",
            format!(
                "mode={subagent_navigation_mode:?} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(trace_lock_wait_started_at)
            ),
        );
        webui_submit_trace(
            "store_workbench_lock_wait",
            format!(
                "mode={subagent_navigation_mode:?} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(trace_lock_wait_started_at)
            ),
        );
        let trace_surface_started_at = current_unix_millis_value();
        let live_context_budgets = guard.live_context_budgets.clone();
        let (surface_messages, subagent_navigation) = match subagent_navigation_mode {
            SubagentNavigationRefreshMode::Rebuild => (
                active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot)?,
                subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)?,
            ),
            SubagentNavigationRefreshMode::Preserve => {
                let subagent_navigation = subagent_navigation_snapshot_from_cached_state(
                    &mut guard,
                    &workspace_snapshot,
                )?;
                let active_session_id = preserved_active_surface_session_id(
                    &workspace_snapshot,
                    subagent_navigation.as_ref(),
                );
                (
                    surface_messages_for_session_without_subagent_navigation_refresh(
                        &guard,
                        &workspace_snapshot,
                        active_session_id,
                    ),
                    subagent_navigation,
                )
            }
        };
        webui_trace(
            "store_workbench_surface_ready",
            format!(
                "mode={subagent_navigation_mode:?} messages={} has_subagent_navigation={} elapsed_ms={}",
                surface_messages.len(),
                subagent_navigation.is_some(),
                current_unix_millis_value().saturating_sub(trace_surface_started_at)
            ),
        );
        webui_submit_trace(
            "store_workbench_surface_ready",
            format!(
                "mode={subagent_navigation_mode:?} messages={} has_subagent_navigation={} elapsed_ms={}",
                surface_messages.len(),
                subagent_navigation.is_some(),
                current_unix_millis_value().saturating_sub(trace_surface_started_at)
            ),
        );
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref())?;
        let session_goal_summaries = guard.session_goal_summaries.clone();
        let command_session_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| Arc::clone(&agent_runtime.command_session_runtime));
        let native_tools = native_tool_summaries_from_agent_runtime(guard.agent_runtime.as_ref());
        let system_plugin_catalog = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.system_plugin_runtime.snapshot());
        (
            live_context_budgets,
            surface_messages,
            subagent_navigation,
            pending_questions,
            session_goal_summaries,
            command_session_runtime,
            native_tools,
            hydrated_conversation_window_summaries(
                &guard.hydrated_conversation_windows,
                &guard.webui_preloaded_unclosed_subagent_roots,
            ),
            system_plugin_catalog,
        )
    };
    let trace_workbench_started_at = current_unix_millis_value();
    let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
        controller: controller_runtime,
        command_session_runtime: command_session_runtime.as_deref(),
        luaskills_catalog,
        system_plugin_catalog: system_plugin_catalog.as_deref(),
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        workspace: &workspace_snapshot,
        surface_messages: surface_messages.as_slice(),
        subagent_navigation,
        pending_questions,
        logs,
        live_context_budgets: &live_context_budgets,
        session_goal_summaries: &session_goal_summaries,
        hydrated_conversation_windows: &hydrated_window_summaries,
        native_tools,
    });
    webui_trace(
        "store_workbench_built",
        format!(
            "mode={subagent_navigation_mode:?} conversations={} messages={} elapsed_ms={}",
            workbench.conversations.len(),
            workbench.messages.len(),
            current_unix_millis_value().saturating_sub(trace_workbench_started_at)
        ),
    );
    webui_submit_trace(
        "store_workbench_built",
        format!(
            "mode={subagent_navigation_mode:?} conversations={} messages={} elapsed_ms={}",
            workbench.conversations.len(),
            workbench.messages.len(),
            current_unix_millis_value().saturating_sub(trace_workbench_started_at)
        ),
    );
    let trace_commit_started_at = current_unix_millis_value();
    workbench = commit_cold_workbench_projection(&mut guard, workspace_snapshot, workbench);
    // Releasing the transaction before event serialization prevents emitter callbacks from observing the state lock.
    // 在事件序列化前释放事务，避免事件发射回调观察到状态锁仍被持有。
    drop(guard);
    webui_trace(
        "store_workbench_committed",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_commit_started_at)
        ),
    );
    webui_submit_trace(
        "store_workbench_committed",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_commit_started_at)
        ),
    );
    // Records the emitted workbench payload size only for the focused submit trace.
    // 仅为专项提交链路日志记录即将发送的工作台载荷大小。
    if webui_submit_trace_enabled() {
        let payload_bytes =
            serde_json::to_vec(&ServiceEvent::Workbench(Box::new(workbench.clone())))
                .map(|bytes| bytes.len())
                .unwrap_or(0);
        webui_submit_trace(
            "store_workbench_payload_ready",
            format!("mode={subagent_navigation_mode:?} payload_bytes={payload_bytes}"),
        );
    }
    let trace_emit_started_at = current_unix_millis_value();
    emit_workbench_event(app, &workbench);
    webui_trace(
        "store_workbench_emitted",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={} total_elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_emit_started_at),
            current_unix_millis_value().saturating_sub(trace_started_at)
        ),
    );
    webui_submit_trace(
        "store_workbench_emitted",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={} total_elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_emit_started_at),
            current_unix_millis_value().saturating_sub(trace_started_at)
        ),
    );
    Ok(workbench)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No-op shell host used to construct isolated persistence-version state.
    /// 用于构造隔离持久化版本状态的空操作壳宿主。
    struct PersistenceVersionTestShellHost;

    impl ServiceShellHost for PersistenceVersionTestShellHost {
        /// Ignores filesystem open requests in persistence-version tests.
        /// 在持久化版本测试中忽略文件系统打开请求。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }

        /// Ignores URL open requests in persistence-version tests.
        /// 在持久化版本测试中忽略 URL 打开请求。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Ok(())
        }
    }

    /// Verifies cold projection commit advances the current generation and stores one matching pair.
    /// 验证冷投影提交会推进当前代次并存储一组相互匹配的快照。
    #[test]
    fn cold_workbench_projection_commits_workspace_and_revision_together() {
        // state owns the exact generation and retained snapshots under test.
        // state 持有本测试的精确代次与保留快照。
        let state =
            create_service_bridge_state(std::sync::Arc::new(PersistenceVersionTestShellHost));
        // workspace_snapshot is the authoritative cold-path mutation result.
        // workspace_snapshot 是权威冷路径变更结果。
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: SessionId::nil(),
            hydrated_session_ids: BTreeSet::new(),
        };
        // workbench starts without a revision because commit owns revision allocation.
        // workbench 初始没有修订号，因为修订号由提交阶段分配。
        let workbench = fallback_workbench_snapshot(&[]);
        let mut guard = state.lock().expect("lock isolated service state");
        guard.workbench_generation = 41;

        let committed =
            commit_cold_workbench_projection(&mut guard, workspace_snapshot.clone(), workbench);

        assert_eq!(committed.revision, 42);
        assert_eq!(guard.workbench_generation, 42);
        assert_eq!(guard.workbench.revision, 42);
        // retained_workspace proves the authoritative workspace was stored in the same critical section.
        // retained_workspace 证明权威工作区已在同一临界区内存储。
        let retained_workspace = guard
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot should commit with workbench");
        assert_eq!(
            retained_workspace.selected_session,
            workspace_snapshot.selected_session
        );
        assert_eq!(
            retained_workspace.sessions.len(),
            workspace_snapshot.sessions.len()
        );
    }

    /// Verifies one lifecycle fence invalidates whole-session, transcript, and allocated addon writers only for its target session.
    /// 验证一次生命周期隔离只对目标会话的整会话、转录与已分配附属写入者生效。
    #[test]
    fn lifecycle_persistence_fence_invalidates_every_target_channel() {
        // state owns isolated version maps for this test.
        // state 持有本测试使用的隔离版本表。
        let state =
            create_service_bridge_state(std::sync::Arc::new(PersistenceVersionTestShellHost));
        // session_id is the lifecycle transition target.
        // session_id 是生命周期转换目标。
        let session_id = SessionId::from_u128(0x71);
        // other_session_id proves unrelated session versions remain current.
        // other_session_id 证明无关会话版本保持当前状态。
        let other_session_id = SessionId::from_u128(0x72);
        // whole_version represents a queued whole-session or runtime-state writer.
        // whole_version 表示排队中的整会话或运行态写入者。
        let whole_version = next_webui_session_persist_version(&state, session_id)
            .expect("whole-session version should allocate");
        // transcript_version represents a queued transcript-only writer.
        // transcript_version 表示排队中的仅转录写入者。
        let transcript_version = next_webui_session_transcript_persist_version(&state, session_id)
            .expect("transcript version should allocate");
        // addon_versions represent queued writers for two independent addon namespaces.
        // addon_versions 表示两个独立附属命名空间的排队写入者。
        let addon_versions = vec![
            (
                SessionAddonNamespace::WorkMode,
                next_webui_session_addon_persist_version(
                    &state,
                    session_id,
                    SessionAddonNamespace::WorkMode,
                )
                .expect("work-mode addon version should allocate"),
            ),
            (
                SessionAddonNamespace::RuntimeControls,
                next_webui_session_addon_persist_version(
                    &state,
                    session_id,
                    SessionAddonNamespace::RuntimeControls,
                )
                .expect("runtime-controls addon version should allocate"),
            ),
        ];
        // other_version remains valid because the fence is session-scoped.
        // other_version 保持有效，因为隔离按会话划分。
        let other_version = next_webui_session_persist_version(&state, other_session_id)
            .expect("unrelated session version should allocate");

        invalidate_webui_session_lifecycle_persist_versions(&state, session_id)
            .expect("lifecycle fence should advance every target channel");

        assert!(!webui_session_persist_version_is_current(
            &state,
            session_id,
            whole_version
        ));
        assert!(!webui_session_transcript_persist_version_is_current(
            &state,
            session_id,
            transcript_version
        ));
        assert!(!webui_session_addon_persist_versions_are_current(
            &state,
            session_id,
            &addon_versions
        ));
        assert!(webui_session_persist_version_is_current(
            &state,
            other_session_id,
            other_version
        ));
    }
}
