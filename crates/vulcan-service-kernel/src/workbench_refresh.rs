//! workbench refresh helpers.
//! workbench refresh 辅助逻辑。

use super::*;
use vulcan_service_api::PendingPromptsChangedDelta;

/// Input bundle for projecting runtime and workspace state into one WebUI workbench snapshot.
/// 将运行时与工作区状态投影为 WebUI 工作台快照的输入包。
pub(crate) struct WorkbenchRuntimeProjection<'a> {
    /// Controller runtime used for command-session and runtime module summaries.
    /// 用于命令会话与运行时模块摘要的控制器运行时。
    pub(crate) controller: &'a ControllerRuntimeHandle,
    /// Optional command session runtime for live terminal state projection.
    /// 用于实时终端状态投影的可选命令会话运行时。
    pub(crate) command_session_runtime: Option<&'a CommandSessionRuntime>,
    /// LuaSkills catalog snapshot already cached by the service state.
    /// 服务状态中已缓存的 LuaSkills 目录快照。
    pub(crate) luaskills_catalog: &'a TauriLuaSkillsCatalog,
    /// Optional live Vulcan Plugin catalog from the shared Agent runtime.
    /// 来自共享 Agent 运行时的可选实时 Vulcan 插件目录。
    pub(crate) vulcan_plugin_catalog:
        Option<&'a vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    /// Structured MCP states captured from the same extension generation as the plugin catalog.
    /// 从与插件目录相同扩展代次捕获的结构化 MCP 状态。
    pub(crate) mcp_server_states: &'a [vulcan_agent_runtime::mcp::McpRuntimeServerState],
    /// Token budget runtime used by module and context usage summaries.
    /// 模块与上下文用量摘要使用的 Token 预算运行时。
    pub(crate) token_budget_runtime: &'a TauriTokenBudgetRuntime,
    /// MCP runtime catalog snapshot already cached by the service state.
    /// 服务状态中已缓存的 MCP 运行时目录快照。
    pub(crate) mcp_catalog: &'a TauriMcpRuntimeCatalog,
    /// Provider configuration that drives model, settings, and capability projection.
    /// 驱动模型、设置与能力投影的 Provider 配置。
    pub(crate) provider_configuration: &'a ProviderConfigurationFile,
    /// Default AI tool configuration used when the selected session has no override.
    /// 当前会话没有覆盖配置时使用的默认 AI 工具配置。
    pub(crate) ai_tool_configuration: &'a PersistedAiToolConfiguration,
    /// Canonical workspace snapshot being projected.
    /// 正在投影的规范工作区快照。
    pub(crate) workspace: &'a vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    /// Read-only state snapshots from currently open Core VMM session controllers.
    /// 当前已打开 Core VMM 会话控制器提供的只读状态快照。
    pub(crate) vmm_session_states: &'a BTreeMap<SessionId, vulcan_vmm::VmmSessionMemoryState>,
    /// Surface messages for the active visible conversation.
    /// 当前可见会话的界面消息。
    pub(crate) surface_messages: &'a [SessionMessageEntry],
    /// Optional subagent navigation snapshot for the active surface.
    /// 当前界面的可选子代理导航快照。
    pub(crate) subagent_navigation: Option<SubagentNavigationSnapshot>,
    /// Pending ask snapshots exposed to the WebUI.
    /// 暴露给 WebUI 的待回答问题快照。
    pub(crate) pending_questions: Vec<PendingQuestionSnapshot>,
    /// Recent service startup/runtime logs to display.
    /// 需要展示的近期服务启动与运行日志。
    pub(crate) logs: &'a [String],
    /// Live context budget rows keyed by session id.
    /// 按会话标识索引的实时上下文预算行。
    pub(crate) live_context_budgets: &'a BTreeMap<SessionId, ContextBudgetSnapshot>,
    /// Session goal summaries keyed by session id.
    /// 按会话标识索引的会话目标摘要。
    pub(crate) session_goal_summaries: &'a BTreeMap<SessionId, SessionGoalSummary>,
    /// Hydrated conversation window summaries retained by the service state.
    /// 服务状态中保留的已水合会话窗口摘要。
    pub(crate) hydrated_conversation_windows: &'a [HydratedConversationWindowSummary],
    /// Native tool summaries exposed by the active agent runtime.
    /// 当前代理运行时暴露的原生工具摘要。
    pub(crate) native_tools: Vec<vulcan_service_api::NativeToolSummary>,
}

/// Refreshes the WebUI workbench after a focused workspace metadata update.
/// 在一次聚焦的工作区元数据更新后刷新 WebUI 工作台。
pub(crate) async fn refresh_workbench_snapshot(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    refresh_workbench_snapshot_with_selected_session(state, None).await
}

/// Refreshes the WebUI workbench while preserving an explicit selection target when it exists.
/// 刷新 WebUI 工作台，并在目标会话存在时保留显式选择目标。
pub(crate) async fn refresh_workbench_snapshot_for_selected_session(
    state: SharedServiceBridgeState,
    target_session_id: SessionId,
) -> Result<WorkbenchSnapshot, String> {
    refresh_workbench_snapshot_with_selected_session(state, Some(target_session_id)).await
}

/// Refreshes the WebUI workbench and optionally overrides the refreshed selection.
/// 刷新 WebUI 工作台，并可选覆盖刷新后的选中会话。
async fn refresh_workbench_snapshot_with_selected_session(
    state: SharedServiceBridgeState,
    target_session_id: Option<SessionId>,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        preserved_workspace_run_state,
        provider_configuration,
        ai_tool_configuration,
        live_context_budgets,
        agent_runtime_for_status,
        live_listener_task_ids,
        captured_workbench_generation,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = cached_mcp_catalog_from_state(&guard);
        let logs = guard.startup.recent_logs.clone();
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            None,
        );
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let live_context_budgets = guard.live_context_budgets.clone();
        // captured_workbench_generation identifies whether a focused domain committed during asynchronous refresh I/O.
        // captured_workbench_generation 标识异步刷新 I/O 期间是否已有聚焦领域完成提交。
        let captured_workbench_generation = guard.workbench_generation;
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            preserved_workspace_run_state,
            provider_configuration,
            ai_tool_configuration,
            live_context_budgets,
            guard.agent_runtime.clone(),
            guard
                .detached_subagent_task_listeners
                .keys()
                .copied()
                .collect::<BTreeSet<_>>(),
            captured_workbench_generation,
        )
    };

    let mut refreshed_snapshot =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller_runtime.clone(),
        )
        .await?;
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );
    let selected_session_id =
        selected_session_id_after_refresh(&refreshed_snapshot, target_session_id);
    refreshed_snapshot.selected_session = selected_session_id;
    if selected_session_id != SessionId::nil() {
        hydrate_selected_session_runtime_if_needed(
            controller_runtime.clone(),
            &mut refreshed_snapshot,
            selected_session_id,
        )
        .await?;
        let refreshed_session_run_phases = refreshed_snapshot.session_run_phases.clone();
        apply_persisted_task_mirror_statuses_to_workspace(
            &controller_runtime,
            &mut refreshed_snapshot,
            selected_session_id,
            |task_id| {
                webui_subagent_task_has_live_run_for_phase(
                    refreshed_session_run_phases.get(&task_id).copied(),
                    agent_runtime_for_status
                        .as_ref()
                        .map(|runtime| &runtime.task_runtime),
                    live_listener_task_ids.contains(&task_id),
                    task_id,
                )
            },
        )?;
    }
    sync_selected_session_goal_summary_from_store(
        &state,
        controller_runtime.clone(),
        selected_session_id,
    )
    .await?;
    let (surface_messages, subagent_navigation, session_goal_summaries, hydrated_window_summaries) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            None,
        );
        preserve_active_session_run_state(
            preserved_workspace_run_state.as_ref(),
            &mut refreshed_snapshot,
        );
        let selected_session_id =
            selected_session_id_after_refresh(&refreshed_snapshot, target_session_id);
        refreshed_snapshot.selected_session = selected_session_id;
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &refreshed_snapshot,
        );
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &refreshed_snapshot)?,
            subagent_navigation_snapshot_from_cached_state(&mut guard, &refreshed_snapshot)?,
            guard.session_goal_summaries.clone(),
            hydrated_conversation_window_summaries(
                &guard.hydrated_conversation_windows,
                &guard.webui_preloaded_unclosed_subagent_roots,
            ),
        )
    };
    let vulcan_plugin_catalog = agent_runtime_for_status
        .as_ref()
        .and_then(|agent_runtime| agent_runtime.extension_generation.plugin_catalog());
    let mcp_server_states = agent_runtime_for_status
        .as_ref()
        .map(|agent_runtime| {
            agent_runtime
                .extension_generation
                .mcp_runtime()
                .server_states()
        })
        .unwrap_or_default();
    // vmm_controller_registry is cloned without retaining shared service ownership.
    // vmm_controller_registry 在不保留共享服务所有权的情况下完成克隆。
    let vmm_controller_registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .vmm_controller_registry
        .clone();
    if selected_session_id != SessionId::nil() {
        let _ = vmm_controller_registry.get_or_open(
            &controller_runtime.data_root,
            selected_session_id,
            &current_unix_millis_text(),
        )?;
    }
    // vmm_session_states are read-only snapshots used only for WebUI projection.
    // vmm_session_states 是仅用于 WebUI 投影的只读快照。
    let vmm_session_states = vmm_controller_registry.snapshots()?;
    let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
        controller: &controller_runtime,
        command_session_runtime: agent_runtime_for_status
            .as_ref()
            .map(|agent_runtime| agent_runtime.command_session_runtime.as_ref()),
        luaskills_catalog: &luaskills_catalog,
        vulcan_plugin_catalog: vulcan_plugin_catalog.as_deref(),
        mcp_server_states: &mcp_server_states,
        token_budget_runtime: &token_budget_runtime,
        mcp_catalog: &mcp_catalog,
        provider_configuration: &provider_configuration,
        ai_tool_configuration: &ai_tool_configuration,
        workspace: &refreshed_snapshot,
        vmm_session_states: &vmm_session_states,
        surface_messages: surface_messages.as_slice(),
        subagent_navigation,
        pending_questions: pending_question_snapshots_from_agent_runtime(
            agent_runtime_for_status.as_ref(),
        )?,
        logs: &logs,
        live_context_budgets: &live_context_budgets,
        session_goal_summaries: &session_goal_summaries,
        hydrated_conversation_windows: &hydrated_window_summaries,
        native_tools: native_tool_summaries_from_agent_runtime(agent_runtime_for_status.as_ref()),
    });
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        preserve_focused_catalog_domains_after_concurrent_refresh(
            &mut workbench,
            &guard.workbench,
            captured_workbench_generation,
            guard.workbench_generation,
        );
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        workbench.revision = guard.workbench_generation;
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(refreshed_snapshot);
    }
    Ok(workbench)
}

/// Copies retained project-settings revisions into a newly rebuilt workbench.
/// 将保留的项目设置版本复制到新重建的工作台中。
///
/// # Parameters
/// 参数
/// `workbench` is the rebuilt snapshot that does not derive settings revisions from workspace files.
/// `workbench` 是不会从工作区文件派生设置版本的重建快照。
/// `retained_workbench` is the latest committed in-memory workbench.
/// `retained_workbench` 是最新提交的内存工作台。
///
/// # Returns
/// 返回值
/// This function returns nothing and replaces only the project-settings revision snapshot.
/// 本函数无返回值，仅替换项目设置版本快照。
pub(crate) fn preserve_project_settings_revisions(
    workbench: &mut WorkbenchSnapshot,
    retained_workbench: &WorkbenchSnapshot,
) {
    workbench.project_settings_revisions = retained_workbench.project_settings_revisions.clone();
}

/// Preserves retained focused domains when a generic refresh crossed a newer commit.
/// 当通用刷新跨越了较新的提交时，保留已留存的聚焦领域。
///
/// # Parameters
/// 参数
/// `workbench` is the generic refresh result built from values captured before asynchronous I/O.
/// `workbench` 是基于异步 I/O 前捕获值构建的通用刷新结果。
/// `retained_workbench` is the latest committed in-memory workbench.
/// `retained_workbench` 是最新提交的内存工作台。
/// `captured_generation` is the workbench generation observed when refresh inputs were captured.
/// `captured_generation` 是捕获刷新输入时观察到的工作台版本。
/// `current_generation` is the workbench generation immediately before the generic refresh commits.
/// `current_generation` 是通用刷新提交前一刻的工作台版本。
///
/// # Returns
/// 返回值
/// This function returns nothing and rewrites only focused catalog domains after a concurrent commit.
/// 本函数无返回值，仅在发生并发提交后重写聚焦目录领域。
fn preserve_focused_catalog_domains_after_concurrent_refresh(
    workbench: &mut WorkbenchSnapshot,
    retained_workbench: &WorkbenchSnapshot,
    captured_generation: u64,
    current_generation: u64,
) {
    preserve_project_settings_revisions(workbench, retained_workbench);
    if captured_generation == current_generation {
        return;
    }
    // Focused Skills and Agents transactions own these domains; an older generic refresh must not overwrite them.
    // 聚焦的 Skills 与 Agents 事务拥有这些领域；较早启动的通用刷新不得覆盖它们。
    workbench.lua_skills = retained_workbench.lua_skills.clone();
    workbench.plugins = retained_workbench.plugins.clone();
    workbench.agent_skills = retained_workbench.agent_skills.clone();
    workbench.subagent_catalog = retained_workbench.subagent_catalog.clone();
}

/// Returns the selection after a refresh, preferring a visible requested session.
/// 返回刷新后的选中会话，优先使用仍可见的请求会话。
pub(crate) fn selected_session_id_after_refresh(
    refreshed_snapshot: &WorkspaceSnapshot,
    target_session_id: Option<SessionId>,
) -> SessionId {
    if let Some(target_session_id) = target_session_id {
        let target_is_visible = target_session_id != SessionId::nil()
            && refreshed_snapshot
                .sessions
                .iter()
                .any(|session| session.id == target_session_id);
        if target_is_visible {
            return target_session_id;
        }
    }
    refreshed_snapshot.selected_session
}

/// Returns whether one session phase still represents active work.
/// 判断单个会话阶段是否仍代表活跃执行。
pub(crate) fn is_active_session_run_phase(phase: SessionRunPhase) -> bool {
    matches!(
        phase,
        SessionRunPhase::Preparing
            | SessionRunPhase::MemoryRecall
            | SessionRunPhase::Requesting
            | SessionRunPhase::Thinking
            | SessionRunPhase::ToolCalling
            | SessionRunPhase::ToolRunning
    )
}

/// Maps one session run phase to its authoritative compact session state.
/// 将单个会话运行阶段映射为权威的紧凑会话状态。
///
/// # Parameters
/// 参数
/// `phase` is the exact backend lifecycle phase being projected.
/// `phase` 是正在投影的精确后端生命周期阶段。
///
/// # Returns
/// 返回值
/// The single sidebar and workspace state corresponding to the supplied phase.
/// 返回与给定阶段对应的唯一侧栏及工作区状态。
pub(crate) fn session_state_for_run_phase(phase: SessionRunPhase) -> SessionState {
    match phase {
        SessionRunPhase::Failed | SessionRunPhase::Stopped => SessionState::Blocked,
        SessionRunPhase::Idle | SessionRunPhase::Ready => SessionState::Completed,
        SessionRunPhase::Preparing
        | SessionRunPhase::MemoryRecall
        | SessionRunPhase::Requesting
        | SessionRunPhase::Thinking
        | SessionRunPhase::ToolCalling
        | SessionRunPhase::ToolRunning => SessionState::Running,
    }
}

/// Returns whether a runtime event closes the current streamed thinking segment.
/// 判断运行时事件是否会关闭当前流式思考片段。
pub(crate) fn is_provider_tool_boundary_runtime_event(event: &RuntimeEvent) -> bool {
    matches!(
        event.kind,
        RuntimeEventKind::ToolRequestStarted
            | RuntimeEventKind::ToolStarted
            | RuntimeEventKind::ToolFinished
    ) && (event.title.starts_with("工具请求 · ")
        || event.title.starts_with("工具调用 · ")
        || event.title.starts_with("工具返回 · "))
}

/// Returns the current Unix timestamp as millisecond text for SQLite metadata rows.
/// 返回当前 Unix 毫秒时间戳文本，用于 SQLite 元数据行。
pub(crate) fn current_unix_millis_text() -> String {
    OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .checked_div(1_000_000)
        .unwrap_or_default()
        .to_string()
}

/// Returns the current Unix timestamp as a millisecond integer for hot-path timing.
/// 返回当前 Unix 毫秒整数，用于热路径计时。
pub(crate) fn current_unix_millis_value() -> i128 {
    OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .checked_div(1_000_000)
        .unwrap_or_default()
}

/// Returns a UTC timestamp suitable for human-readable diagnostic traces.
/// 返回适合人工阅读诊断日志的 UTC 时间戳。
pub(crate) fn current_webui_trace_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond()
    )
}

/// Returns whether WebUI execution-chain diagnostic traces are enabled.
/// 返回 WebUI 执行链诊断日志是否已启用。
pub(crate) fn webui_trace_enabled() -> bool {
    std::env::var("VULCAN_WEBUI_TRACE")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Returns whether focused WebUI submit-chain diagnostic traces are enabled.
/// 返回 WebUI 提交链路专项诊断日志是否已启用。
pub(crate) fn webui_submit_trace_enabled() -> bool {
    std::env::var("VULCAN_WEBUI_SUBMIT_TRACE")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Returns whether focused after-precheck diagnostic traces are enabled.
/// 返回 precheck 后专项诊断日志是否已启用。
pub(crate) fn webui_after_precheck_trace_enabled() -> bool {
    std::env::var("VULCAN_WEBUI_AFTER_PRECHECK_TRACE")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Emits one WebUI execution-chain diagnostic trace line when tracing is enabled.
/// 当诊断开关启用时输出一条 WebUI 执行链诊断日志。
pub(crate) fn webui_trace(stage: &str, detail: impl AsRef<str>) {
    if !webui_trace_enabled() {
        return;
    }
    eprintln!(
        "[webui-service][webui-trace] timestamp={} unix_ms={} stage={} {}",
        current_webui_trace_timestamp(),
        current_unix_millis_text(),
        stage,
        detail.as_ref()
    );
}

/// Emits one focused after-precheck diagnostic trace line.
/// 输出一条 precheck 后专项诊断日志。
pub(crate) fn webui_after_precheck_trace(stage: &str, detail: impl AsRef<str>) {
    if !webui_after_precheck_trace_enabled() {
        return;
    }
    eprintln!(
        "[webui-service][after-precheck-trace] timestamp={} unix_ms={} stage={} {}",
        current_webui_trace_timestamp(),
        current_unix_millis_text(),
        stage,
        detail.as_ref()
    );
}

/// Emits one focused submit-chain diagnostic trace line when focused or global tracing is enabled.
/// 当专项或全局诊断开关启用时输出一条提交链路诊断日志。
pub(crate) fn webui_submit_trace(stage: &str, detail: impl AsRef<str>) {
    if !webui_submit_trace_enabled() && !webui_trace_enabled() {
        return;
    }
    eprintln!(
        "[webui-service][submit-trace] timestamp={} unix_ms={} stage={} {}",
        current_webui_trace_timestamp(),
        current_unix_millis_text(),
        stage,
        detail.as_ref()
    );
}

/// Emits one WebUI timing trace when a supposedly hot path exceeds the 20ms budget.
/// 当本应为热路径的步骤超过 20ms 预算时输出 WebUI 计时日志。
pub(crate) fn webui_trace_if_slow(stage: &str, started_at_ms: i128, detail: impl AsRef<str>) {
    let elapsed_ms = current_unix_millis_value().saturating_sub(started_at_ms);
    if elapsed_ms > 20 {
        webui_trace(
            stage,
            format!("elapsed_ms={elapsed_ms} {}", detail.as_ref()),
        );
    }
}

/// Builds WebUI native-tool policy rows from the current agent runtime bridge.
/// 从当前 Agent 运行时桥接构建 WebUI 原生工具策略行。
pub(crate) fn native_tool_summaries_from_agent_runtime(
    agent_runtime: Option<&AgentRuntimeBridge>,
) -> Vec<vulcan_service_api::NativeToolSummary> {
    let Some(agent_runtime) = agent_runtime else {
        return Vec::new();
    };
    let runtime_config =
        vulcan_agent_runtime::agent_runtime::config::AgentRuntimeConfig::from_sources(
            &agent_runtime.provider_configuration,
            &agent_runtime.agent_profile_catalog,
        );
    let native_catalog = vulcan_agent_runtime::tooling::build_native_tool_catalog(
        &runtime_config,
        agent_runtime
            .provider_configuration
            .default_command_shell
            .as_deref(),
        std::sync::Arc::clone(&agent_runtime.command_session_runtime),
        agent_runtime.provider_configuration.vmm.clone(),
        Some(agent_runtime.vulcan_plugin_runtime.clone()),
        agent_runtime.luaskills_runtime.clone(),
    );
    native_catalog
        .descriptors
        .into_iter()
        .map(|descriptor| vulcan_service_api::NativeToolSummary {
            id: descriptor.id,
            name: descriptor.name,
            description: descriptor.description,
            ai_visible: descriptor.ai_visible,
            available_in_plan: descriptor.available_in_plan,
        })
        .collect()
}

/// Resolves the cached project VMM layer for one session.
/// 解析单个会话对应的缓存项目 VMM 配置层。
///
/// # Parameters
/// 参数
/// `workspace` is the in-memory workspace snapshot containing task projects.
/// `workspace` 是包含任务项目的内存工作区快照。
/// `session` is the session whose project binding should be resolved.
/// `session` 是需要解析项目绑定的会话。
///
/// # Returns
/// 返回值
/// The cached project VMM layer when the session is bound to a known project.
/// 当会话绑定到已知项目时返回缓存项目 VMM 配置层。
pub(crate) fn cached_project_vmm_layer_for_session(
    workspace: &WorkspaceSnapshot,
    session: &SessionSummary,
) -> Option<vulcan_vmm::VmmConfigLayer> {
    let project_id = session_summary_project_id(session)?;
    workspace_task_project_for_project_id_including_archived(workspace, project_id)
        .map(|project| cached_project_vmm_layer(&project.path))
}

/// Returns whether one session belongs to a restorable project id.
/// 返回单个会话是否属于可恢复的项目标识。
///
/// # Parameters
/// 参数
/// `session` is the session whose project binding should be checked.
/// `session` 是需要检查项目归属的会话。
/// `restorable_project_ids` contains visible and archived project ids that can restore sessions.
/// `restorable_project_ids` 包含可恢复会话的可见与归档项目标识。
///
/// # Returns
/// 返回值
/// `true` when the session has a non-blank project id that is restorable.
/// 当会话拥有非空白且可恢复的项目标识时返回 `true`。
fn session_has_restorable_project_id(
    session: &SessionSummary,
    restorable_project_ids: &BTreeSet<&str>,
) -> bool {
    session_summary_project_id(session)
        .is_some_and(|project_id| restorable_project_ids.contains(project_id))
}

/// Builds compact RuntimeEvent metadata for WebUI trace logs.
/// 为 WebUI 跟踪日志构建紧凑的 RuntimeEvent 元数据。
pub(crate) fn runtime_event_trace_detail(event: &RuntimeEvent) -> String {
    let metadata = event.metadata.as_ref();
    let tool_call_id = metadata
        .and_then(|value| value.get("tool_call_id"))
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    let tool_name = metadata
        .and_then(|value| value.get("tool_name"))
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    let display_state = metadata
        .and_then(|value| value.get("display_state"))
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    format!(
        "tool_call_id={tool_call_id} tool_name={tool_name} display_state={display_state} title_chars={} detail_chars={}",
        event.title.chars().count(),
        event.detail.chars().count()
    )
}

/// Builds compact detached-task metadata for WebUI trace logs.
/// 为 WebUI 跟踪日志构建紧凑的分离任务元数据。
pub(crate) fn detached_task_event_trace_detail(event: &DetachedTaskRuntimeEvent) -> String {
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RuntimeEvent {
            event,
            ..
        }) => runtime_event_trace_detail(event),
        DetachedTaskRuntimeEvent::ParentRuntime { event, .. } => runtime_event_trace_detail(event),
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::ThinkingDelta {
            session_id,
            delta,
        }) => format!("session={session_id} delta_chars={}", delta.chars().count()),
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::OutputDelta {
            session_id,
            delta,
        }) => format!("session={session_id} delta_chars={}", delta.chars().count()),
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ProviderLoweringRepair { session_id, event },
        ) => format!(
            "session={session_id} provider={} model={} action={} affected_items={}",
            event.provider_id, event.model_id, event.action, event.affected_items
        ),
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ConversationCheckpoint {
                session_id,
                conversation_state,
                provider_request_count,
                ..
            },
        ) => format!(
            "session={session_id} records={} messages={} provider_requests={provider_request_count}",
            conversation_state.records.len(),
            conversation_state.messages.len()
        ),
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Finished(report)) => {
            format!(
                "provider_requests={} output_chars={} error={}",
                report.provider_request_count,
                report
                    .assistant_output
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .count(),
                report.error.is_some()
            )
        }
        _ => String::from("detail=<none>"),
    }
}

/// Returns a compact label for one detached task event without dumping large payloads.
/// 返回单条分离任务事件的紧凑标签，避免输出大型载荷。
pub(crate) fn detached_task_event_label(event: &DetachedTaskRuntimeEvent) -> &'static str {
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Prepared(_)) => {
            "child.prepared"
        }
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ProviderRequestCheckpoint { .. },
        ) => "child.provider_request_checkpoint",
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ProviderRequestTelemetry(_),
        ) => "child.provider_request_telemetry",
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ProviderLoweringRepair { .. },
        ) => "child.provider_lowering_repair",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RequestReady { .. }) => {
            "child.request_ready"
        }
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::WireRequestReady {
            ..
        }) => "child.wire_request_ready",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RequestStarted {
            ..
        }) => "child.request_started",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::ThinkingDelta {
            ..
        }) => "child.thinking_delta",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::ThinkingCompleted {
            ..
        }) => "child.thinking_completed",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::OutputDelta { .. }) => {
            "child.output_delta"
        }
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RuntimeEvent {
            event,
            ..
        }) => match event.kind {
            RuntimeEventKind::DelegateStarted => "child.runtime.delegate_started",
            RuntimeEventKind::DelegateFinished => "child.runtime.delegate_finished",
            RuntimeEventKind::ToolStarted => "child.runtime.tool_started",
            RuntimeEventKind::ToolFinished => "child.runtime.tool_finished",
            _ => "child.runtime.event",
        },
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ConversationCheckpoint { .. },
        ) => "child.checkpoint",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Finished(_)) => {
            "child.finished"
        }
        DetachedTaskRuntimeEvent::ParentRuntime { event, .. } => match event.kind {
            RuntimeEventKind::DelegateStarted => "parent.delegate_started",
            RuntimeEventKind::DelegateFinished => "parent.delegate_finished",
            _ => "parent.runtime_event",
        },
    }
}

/// Converts runtime state into a WebUI workbench snapshot.
/// 将运行时状态转换为 WebUI 工作台快照。
pub(crate) fn workbench_from_runtime(input: WorkbenchRuntimeProjection<'_>) -> WorkbenchSnapshot {
    let WorkbenchRuntimeProjection {
        controller,
        command_session_runtime,
        luaskills_catalog,
        vulcan_plugin_catalog,
        mcp_server_states,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        workspace,
        vmm_session_states,
        surface_messages,
        subagent_navigation,
        pending_questions,
        logs,
        live_context_budgets,
        session_goal_summaries,
        hydrated_conversation_windows,
        native_tools,
    } = input;
    let selected_conversation_id =
        selected_workspace_session_id(workspace).map(|session_id| session_id.to_string());
    let pending_question = selected_conversation_id
        .as_ref()
        .and_then(|conversation_id| {
            pending_questions
                .iter()
                .find(|question| question.session_id == *conversation_id)
                .cloned()
        });
    let active_conversation_id = subagent_navigation
        .as_ref()
        .map(|navigation| navigation.active_conversation_id.as_str())
        .or(selected_conversation_id.as_deref());
    let messages = active_conversation_id
        .map(|conversation_id| {
            let allow_user_prompt_editing = workspace_surface_allows_user_prompt_editing(
                workspace,
                workspace.selected_session,
                subagent_navigation.as_ref(),
            );
            conversation_messages_from_session(
                conversation_id,
                surface_messages,
                allow_user_prompt_editing,
            )
        })
        .unwrap_or_default();
    let selected_runtime_state = workspace
        .session_runtime_state
        .get(&workspace.selected_session);
    // selected_vmm_session_state is a read-only controller projection for the active surface.
    // selected_vmm_session_state 是活动界面使用的只读控制器投影。
    let selected_vmm_session_state = vmm_session_states.get(&workspace.selected_session);
    let selected_session_vmm_project_layer = selected_workspace_session(workspace)
        .and_then(|session| cached_project_vmm_layer_for_session(workspace, session));
    let selected_ai_tool_configuration = selected_runtime_state
        .and_then(|runtime_state| runtime_state.ai_tool_configuration.as_ref())
        .unwrap_or(ai_tool_configuration);
    let ai_capability_model_capabilities =
        active_provider_configuration_model_capabilities(provider_configuration);
    let restorable_project_ids = workspace_task_projects_including_archived(workspace)
        .map(|project| project.id.as_str())
        .collect::<BTreeSet<_>>();
    let pending_prompts_by_conversation = workspace
        .session_runtime_state
        .iter()
        .filter_map(|(session_id, runtime_state)| {
            let pending_prompts = pending_prompt_summaries_from_runtime_state(runtime_state);
            if pending_prompts.is_empty() {
                return None;
            }
            Some(PendingPromptsChangedDelta {
                conversation_id: session_id.to_string(),
                pending_prompts,
            })
        })
        .collect::<Vec<_>>();
    let session_addons_by_conversation = workspace
        .sessions
        .iter()
        .filter_map(|session| {
            let runtime_state = workspace.session_runtime_state.get(&session.id);
            let session_goal = session_goal_summaries.get(&session.id).cloned();
            let session_todolist = session_todolist_summary_from_runtime_state(runtime_state);
            let session_plan_mode = session_plan_mode_summary_from_runtime_state(runtime_state);
            let session_vmm_policy = runtime_state
                .filter(|runtime_state| !runtime_state.vmm.is_empty())
                .map(|runtime_state| {
                    let project_layer = cached_project_vmm_layer_for_session(workspace, session);
                    session_vmm_policy_summary_from_layers(
                        &provider_configuration.vmm,
                        project_layer.as_ref(),
                        Some(runtime_state),
                    )
                });
            session_addons_changed_delta_for_retained_list(
                session.id.to_string(),
                session_goal,
                session_todolist,
                session_plan_mode,
                session_vmm_policy,
            )
        })
        .collect::<Vec<_>>();
    let session_vmm_policies_by_conversation = workspace
        .sessions
        .iter()
        .filter(|session| session_segmented_metadata_exists(session))
        .map(|session| {
            let runtime_state = workspace.session_runtime_state.get(&session.id);
            let project_layer = cached_project_vmm_layer_for_session(workspace, session);
            SessionVmmPolicyByConversation {
                conversation_id: session.id.to_string(),
                session_vmm_policy: session_vmm_policy_summary_from_layers(
                    &provider_configuration.vmm,
                    project_layer.as_ref(),
                    runtime_state,
                ),
            }
        })
        .collect::<Vec<_>>();
    WorkbenchSnapshot {
        revision: 0,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        ui_locale: provider_configuration.ui_locale.clone(),
        modules: runtime_modules(
            controller,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
        ),
        projects: workspace
            .task_projects
            .iter()
            .map(|project| ProjectSummary {
                id: project.id.clone(),
                name: project.title.clone(),
                path: project.path.clone(),
                pinned: project.pinned,
                icon_pack: project.icon_pack.clone(),
                icon_key: project.icon_key.clone(),
                codebase_memory_enabled: project.codebase_memory_enabled,
            })
            .collect(),
        expanded_project_ids: workspace.expanded_task_projects.iter().cloned().collect(),
        project_settings_revisions: vulcan_service_api::ProjectSettingsRevisionSnapshot::default(),
        conversations: workspace
            .sessions
            .iter()
            .filter(|session| session_segmented_metadata_exists(session))
            .map(|session| {
                let run_phase = workspace
                    .session_run_phases
                    .get(&session.id)
                    .copied()
                    .unwrap_or(SessionRunPhase::Idle);
                let effective_run_phase = run_phase;
                let is_running = session.status == SessionState::Running
                    || is_active_session_run_phase(run_phase);
                let unread = workspace
                    .session_runtime_state
                    .get(&session.id)
                    .is_some_and(|runtime_state| runtime_state.ui_state.unread);
                let session_plan_mode = session_plan_mode_summary_from_runtime_state(
                    workspace.session_runtime_state.get(&session.id),
                );
                ConversationSummary {
                    id: session.id.to_string(),
                    title: session.title.clone(),
                    status: if is_running {
                        SessionState::Running.label()
                    } else {
                        session.status.label()
                    }
                    .to_string(),
                    run_phase: effective_run_phase.to_string(),
                    last_updated_label: session.last_updated_label.clone(),
                    last_updated_at: session.updated_at.clone(),
                    unread,
                    attention: conversation_attention_summary_for_session(
                        session.id,
                        &pending_questions,
                        session_plan_mode.as_ref(),
                        unread,
                        Some(session.updated_at.clone()),
                    ),
                    project_id: session_summary_project_id_owned(session),
                    fork_source: conversation_fork_source_summary_from_session(session),
                }
            })
            .collect(),
        archived_projects: workspace
            .archived_task_projects
            .iter()
            .map(|project| ProjectSummary {
                id: project.id.clone(),
                name: project.title.clone(),
                path: project.path.clone(),
                pinned: project.pinned,
                icon_pack: project.icon_pack.clone(),
                icon_key: project.icon_key.clone(),
                codebase_memory_enabled: project.codebase_memory_enabled,
            })
            .collect(),
        archived_conversations: workspace
            .archived_sessions
            .iter()
            .filter(|session| session_has_restorable_project_id(session, &restorable_project_ids))
            .filter(|session| session_segmented_metadata_exists(session))
            .map(|session| ConversationSummary {
                id: session.id.to_string(),
                title: session.title.clone(),
                status: String::from("Archived"),
                run_phase: SessionRunPhase::Idle.to_string(),
                last_updated_label: session.last_updated_label.clone(),
                last_updated_at: session.updated_at.clone(),
                unread: false,
                attention: None,
                project_id: session_summary_project_id_owned(session),
                fork_source: conversation_fork_source_summary_from_session(session),
            })
            .collect(),
        selected_conversation_id,
        messages,
        hydrated_conversation_windows: hydrated_conversation_windows.to_vec(),
        conversation_hydration: None,
        subagent_navigation,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: plugins_from_catalogs(
            luaskills_catalog,
            provider_configuration,
            vulcan_plugin_catalog,
            mcp_server_states,
        ),
        lua_skills: lua_skills_catalog_summary(luaskills_catalog, provider_configuration),
        agent_skills: agent_skills_catalog_summary(
            provider_configuration,
            input.vulcan_plugin_catalog,
        ),
        subagent_catalog: subagent_catalog_summary(
            provider_configuration,
            input.vulcan_plugin_catalog,
        ),
        settings: settings(),
        providers: providers(provider_configuration),
        mcp_servers: mcp_servers(provider_configuration),
        mcp_tools: mcp_tools(mcp_catalog),
        reasoning_selection: reasoning_selection(provider_configuration),
        thinking_display_mode: workbench_thinking_display_mode(provider_configuration),
        git_commit: git_commit_settings(provider_configuration),
        provider_retry: provider_retry_settings(provider_configuration),
        host_tools: host_tools_settings_with_native_tools(provider_configuration, native_tools),
        ai_capabilities: ai_capabilities(
            selected_ai_tool_configuration,
            ai_capability_model_capabilities.as_ref(),
        ),
        default_ai_capabilities: ai_capabilities(
            ai_tool_configuration,
            ai_capability_model_capabilities.as_ref(),
        ),
        ai_capability_vendors: ai_capability_vendors(selected_ai_tool_configuration),
        ai_capability_vendor_catalog: ai_capability_vendor_catalog(),
        composer_attachment_capabilities: composer_attachment_capabilities(
            provider_configuration,
            selected_ai_tool_configuration,
        ),
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: selected_runtime_state
            .map(pending_prompt_summaries_from_runtime_state)
            .unwrap_or_default(),
        pending_prompts_by_conversation,
        pending_question,
        pending_questions,
        session_todolist: session_todolist_summary_from_runtime_state(selected_runtime_state),
        session_goal: session_goal_summaries
            .get(&workspace.selected_session)
            .cloned(),
        session_plan_mode: session_plan_mode_summary_from_runtime_state(selected_runtime_state),
        session_vmm_policy: Some(session_vmm_policy_summary_from_layers(
            &provider_configuration.vmm,
            selected_session_vmm_project_layer.as_ref(),
            selected_runtime_state,
        )),
        session_addons_by_conversation,
        session_vmm_policies_by_conversation,
        memories: memories(workspace),
        vmm_memory: vmm_memory_snapshot(workspace, provider_configuration, selected_runtime_state),
        command_sessions: command_sessions(controller, command_session_runtime),
        logs: logs
            .iter()
            .enumerate()
            .map(|(index, message)| {
                log_entry(&format!("startup-{index}"), "info", "service", message)
            })
            .chain(
                mcp_catalog
                    .summary_rows
                    .iter()
                    .enumerate()
                    .map(|(index, message)| {
                        log_entry(&format!("mcp-{index}"), "info", "mcp", message)
                    }),
            )
            .collect(),
        context_injection: context_injection_snapshot_from_runtime_state(
            selected_runtime_state,
            selected_vmm_session_state,
        ),
        context_injection_by_conversation: context_injection_by_conversation_from_workspace(
            workspace,
            vmm_session_states,
        ),
        context_usage: context_usage_snapshot_from_runtime_state(
            workspace.selected_session,
            selected_runtime_state,
            live_context_budgets,
            provider_configuration,
            ai_tool_configuration,
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    /// Verifies a generic refresh keeps its freshly rebuilt catalog domains without a concurrent commit.
    /// 验证没有并发提交时通用刷新保留自身刚重建的目录领域。
    #[test]
    fn focused_catalog_domains_keep_generic_refresh_without_concurrent_commit() {
        // refreshed_workbench represents the catalog domains rebuilt by the generic refresh.
        // refreshed_workbench 表示通用刷新重建出的目录领域。
        let mut refreshed_workbench = fallback_workbench_snapshot(&[]);
        refreshed_workbench.lua_skills.notice = String::from("refreshed lua");
        refreshed_workbench.agent_skills.notice = String::from("refreshed agent skills");
        refreshed_workbench.subagent_catalog.notice = String::from("refreshed agents");
        // retained_workbench represents an older in-memory projection at the same generation.
        // retained_workbench 表示同一版本下较旧的内存投影。
        let mut retained_workbench = fallback_workbench_snapshot(&[]);
        retained_workbench.lua_skills.notice = String::from("retained lua");
        retained_workbench.agent_skills.notice = String::from("retained agent skills");
        retained_workbench.subagent_catalog.notice = String::from("retained agents");

        preserve_focused_catalog_domains_after_concurrent_refresh(
            &mut refreshed_workbench,
            &retained_workbench,
            7,
            7,
        );

        assert_eq!(refreshed_workbench.lua_skills.notice, "refreshed lua");
        assert_eq!(
            refreshed_workbench.agent_skills.notice,
            "refreshed agent skills"
        );
        assert_eq!(
            refreshed_workbench.subagent_catalog.notice,
            "refreshed agents"
        );
    }

    /// Verifies a generic refresh preserves every focused catalog domain after a concurrent commit.
    /// 验证发生并发提交后通用刷新保留全部聚焦目录领域。
    #[test]
    fn focused_catalog_domains_preserve_newer_concurrent_commit() {
        // refreshed_workbench represents a stale generic refresh result built before the newer commit.
        // refreshed_workbench 表示在较新提交前构建的过期通用刷新结果。
        let mut refreshed_workbench = fallback_workbench_snapshot(&[]);
        refreshed_workbench.lua_skills.notice = String::from("stale lua");
        refreshed_workbench.agent_skills.notice = String::from("stale agent skills");
        refreshed_workbench.subagent_catalog.notice = String::from("stale agents");
        // retained_workbench represents the focused domains committed while asynchronous refresh I/O was running.
        // retained_workbench 表示异步刷新 I/O 期间已经提交的聚焦领域。
        let mut retained_workbench = fallback_workbench_snapshot(&[]);
        retained_workbench.lua_skills.notice = String::from("current lua");
        retained_workbench.agent_skills.notice = String::from("current agent skills");
        retained_workbench.subagent_catalog.notice = String::from("current agents");
        retained_workbench.plugins = vec![PluginSummary {
            id: String::from("current-plugin"),
            version: Some(String::from("1.0.0")),
            name: String::from("Current Plugin"),
            kind: String::from("LuaSkill / USER"),
            status: String::from("已启用"),
            runtime_state: String::from("enabled"),
            capability: String::from("1 个入口"),
            name_by_locale: BTreeMap::new(),
            capability_by_locale: BTreeMap::new(),
            enabled: true,
            desired_enabled: true,
            available: true,
            tool_count: 1,
            tools: Vec::new(),
            hook_count: 0,
            skill_count: 0,
            agent_count: 0,
            active_session_count: 0,
            diagnostics: Vec::new(),
            management_kind: String::from("luaSkill"),
            extension: None,
        }];

        preserve_focused_catalog_domains_after_concurrent_refresh(
            &mut refreshed_workbench,
            &retained_workbench,
            7,
            8,
        );

        assert_eq!(refreshed_workbench.lua_skills.notice, "current lua");
        assert_eq!(
            refreshed_workbench.agent_skills.notice,
            "current agent skills"
        );
        assert_eq!(
            refreshed_workbench.subagent_catalog.notice,
            "current agents"
        );
        assert_eq!(refreshed_workbench.plugins, retained_workbench.plugins);
    }

    /// Builds one workspace snapshot for cached project VMM layer tests.
    /// 构建用于缓存项目 VMM 配置层测试的工作区快照。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the session stored in the snapshot.
    /// `session_id` 是写入快照的会话标识。
    /// `session_project_id` is the project id stored on the session.
    /// `session_project_id` 是写入会话的项目标识。
    ///
    /// # Returns
    /// 返回值
    /// A workspace snapshot containing one whitespace-id project and one session.
    /// 返回包含一个空白标识项目与一个会话的工作区快照。
    fn workspace_snapshot_with_blank_project(
        session_id: SessionId,
        session_project_id: Option<String>,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                "   ",
                "Blank Project",
                r"D:\projects\blank",
                false,
                0,
                None,
                None,
                Vec::new(),
            )],
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                session_project_id,
                "VMM 投影测试会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies blank session project ids do not resolve cached project VMM layers.
    /// 验证空白会话项目标识不会解析缓存项目 VMM 配置层。
    #[test]
    fn cached_project_vmm_layer_for_session_ignores_blank_project_id() {
        // session_id identifies the session whose VMM layer is resolved.
        // session_id 标识需要解析 VMM 配置层的会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot contains a whitespace-id project that must not be selected.
        // workspace_snapshot 包含一个不应被选中的空白标识项目。
        let workspace_snapshot =
            workspace_snapshot_with_blank_project(session_id, Some(String::from("   ")));

        assert!(
            cached_project_vmm_layer_for_session(
                &workspace_snapshot,
                &workspace_snapshot.sessions[0]
            )
            .is_none()
        );
    }

    /// Verifies archived-session restoration ignores blank project ids.
    /// 验证归档会话恢复会忽略空白项目标识。
    #[test]
    fn session_has_restorable_project_id_ignores_blank_project_id() {
        // session stores the archived conversation project id under test.
        // session 保存测试中的归档会话项目标识。
        let session = SessionSummary::new(
            SessionId::new_v4(),
            Some(String::from("   ")),
            "归档会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        );
        // restorable_project_ids intentionally contains the same blank id.
        // restorable_project_ids 故意包含相同的空白标识。
        let restorable_project_ids = BTreeSet::from(["   "]);

        assert!(!session_has_restorable_project_id(
            &session,
            &restorable_project_ids
        ));
    }
}
