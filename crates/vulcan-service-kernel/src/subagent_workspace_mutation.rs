//! subagent workspace mutation helpers.
//! subagent workspace mutation 辅助逻辑。

use std::{
    collections::BTreeMap,
    path::PathBuf,
};

use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime;
use vulcan_session_core::{SubagentRegistryEntry, SubagentStatus};

/// Disk-loaded child-agent surface batch ready to merge into service navigation and transcript caches.
/// 已从磁盘读取、可合并进服务导航与转录缓存的子智能体界面批次。
#[derive(Debug, Clone)]
pub(crate) struct WebuiSubagentSurfacePreload {
    /// Direct parent session that owns these child task states.
    /// 拥有这些子任务状态的直接父会话。
    parent_session_id: SessionId,
    /// Detail-ready child task states loaded from the segmented TaskSession store.
    /// 从分段 TaskSession 存储读取的可直接展示详情子任务状态。
    task_states: Vec<SubagentTaskPanelState>,
}

/// Hides the selected parent subagent navigation round once service-equivalent blocking work has settled.
/// 当与服务运行时等价的阻塞工作全部收敛后，隐藏该轮子代理导航。
pub(crate) fn refresh_webui_subagent_navigation_round_visibility(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let has_running =
        !running_webui_subagent_task_states_for_tree_from_live_cache(&mut guard, parent_session_id)
            .is_empty();
    let has_active_stream_task = guard.active_stream_tasks.contains_key(&parent_session_id);
    let has_active_kernel_turn = webui_session_kernel_has_active_turn(&guard, parent_session_id);
    let has_active_phase = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace_snapshot| {
            workspace_snapshot
                .session_run_phases
                .get(&parent_session_id)
                .copied()
        })
        .is_some_and(is_active_session_run_phase);
    let should_remain_visible = subagent_navigation_round_should_remain_visible(
        has_active_stream_task || has_active_kernel_turn || has_active_phase,
        has_running,
    );
    webui_trace(
        "subagent_navigation_round_visibility",
        format!(
            "parent={parent_session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_active_phase={has_active_phase} has_running={has_running} should_remain_visible={should_remain_visible}"
        ),
    );
    if !should_remain_visible {
        guard
            .session_subagent_navigation_rounds
            .remove(&parent_session_id);
    }
    Ok(())
}

/// Reprojects subagent navigation and emits a focused navigation delta.
/// 重新投影子代理导航并发出聚焦的导航增量。
pub(crate) fn emit_webui_subagent_navigation_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<Option<SubagentNavigationSnapshot>, String> {
    let (generation, parent_conversation_id, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard
            .workspace_snapshot
            .clone()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let selected_parent_conversation_id = (workspace_snapshot.selected_session
            != SessionId::nil())
        .then(|| workspace_snapshot.selected_session.to_string());
        let subagent_navigation =
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot);
        let parent_conversation_id = subagent_navigation
            .as_ref()
            .map(|navigation| navigation.parent_conversation_id.clone())
            .or(selected_parent_conversation_id);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        guard.workbench.subagent_navigation = subagent_navigation.clone();
        (generation, parent_conversation_id, subagent_navigation)
    };
    emit_service_event_with_trace(
        app,
        "subagent_navigation_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::SubagentNavigationChanged(SubagentNavigationChangedDelta {
                parent_conversation_id,
                subagent_navigation: subagent_navigation.clone(),
            }),
        })),
    );
    Ok(subagent_navigation)
}

/// Applies one safe canonical conversation checkpoint and persists it off the stream path.
/// 应用一份安全规范会话检查点，并在流式关键路径外持久化。
/// Mutates the retained workspace snapshot and emits a rebuilt workbench snapshot.
/// Controls whether one workbench refresh rebuilds structural subagent navigation.
/// 控制一次工作台刷新是否重建结构性的子代理导航。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubagentNavigationRefreshMode {
    /// Rebuild subagent navigation from the already-hydrated runtime cache.
    /// 从已经水合完成的运行时缓存重建子代理导航。
    Rebuild,
    /// Preserve the current subagent navigation while refreshing transcript content.
    /// 刷新转录内容时保留当前子代理导航。
    Preserve,
}

/// Mutates the retained workspace snapshot and emits a rebuilt workbench snapshot.
/// 修改保留的工作区快照并发出重建后的工作台快照。
pub(crate) fn with_workspace_mutation_and_full_workbench_rebuild_for_cold_path<F>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    mutate: F,
) -> Result<WorkspaceSnapshot, String>
where
    F: FnOnce(&mut WorkspaceSnapshot),
{
    with_workspace_mutation_and_full_workbench_rebuild_for_cold_path_with_subagent_navigation_mode(
        app,
        state,
        SubagentNavigationRefreshMode::Rebuild,
        mutate,
    )
}

/// Mutates the retained workspace snapshot without rebuilding subagent task navigation.
/// 修改保留的工作区快照但不重建子代理任务导航。
pub(crate) fn with_workspace_mutation_and_preserved_navigation_full_rebuild_for_cold_path<F>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    mutate: F,
) -> Result<WorkspaceSnapshot, String>
where
    F: FnOnce(&mut WorkspaceSnapshot),
{
    with_workspace_mutation_and_full_workbench_rebuild_for_cold_path_with_subagent_navigation_mode(
        app,
        state,
        SubagentNavigationRefreshMode::Preserve,
        mutate,
    )
}

/// Mutates the retained workspace snapshot and emits a rebuilt workbench snapshot using the requested navigation mode.
/// 按指定导航模式修改保留的工作区快照并发出重建后的工作台快照。
pub(crate) fn with_workspace_mutation_and_full_workbench_rebuild_for_cold_path_with_subagent_navigation_mode<
    F,
>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    subagent_navigation_refresh_mode: SubagentNavigationRefreshMode,
    mutate: F,
) -> Result<WorkspaceSnapshot, String>
where
    F: FnOnce(&mut WorkspaceSnapshot),
{
    let trace_started_at = current_unix_millis_value();
    let (
        workspace_snapshot,
        rebuild_generation,
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        surface_messages,
        subagent_navigation,
        pending_questions,
        logs,
        live_context_budgets,
        session_goal_summaries,
        command_session_runtime,
        native_tools,
        hydrated_window_summaries,
    ) = {
        let trace_lock_wait_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        webui_trace_if_slow(
            "workspace_mutation_lock_wait",
            trace_lock_wait_started_at,
            format!("mode={subagent_navigation_refresh_mode:?}"),
        );
        let trace_prepare_started_at = current_unix_millis_value();
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = cached_mcp_catalog_from_state(&guard);
        let logs = guard.startup.recent_logs.clone();
        let mut workspace_snapshot = guard.workspace_snapshot.take().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
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
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref());
        let command_session_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| Arc::clone(&agent_runtime.command_session_runtime));
        let live_context_budgets = guard.live_context_budgets.clone();
        let session_goal_summaries = guard.session_goal_summaries.clone();
        webui_trace_if_slow(
            "workspace_mutation_prepare",
            trace_prepare_started_at,
            format!("mode={subagent_navigation_refresh_mode:?}"),
        );
        let trace_mutate_started_at = current_unix_millis_value();
        mutate(&mut workspace_snapshot);
        webui_trace_if_slow(
            "workspace_mutation_apply",
            trace_mutate_started_at,
            format!("mode={subagent_navigation_refresh_mode:?}"),
        );
        let trace_surface_started_at = current_unix_millis_value();
        let (surface_messages, subagent_navigation) = match subagent_navigation_refresh_mode {
            SubagentNavigationRefreshMode::Rebuild => (
                active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
                subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
            ),
            SubagentNavigationRefreshMode::Preserve => {
                let subagent_navigation = guard.workbench.subagent_navigation.clone();
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
        webui_trace_if_slow(
            "workspace_mutation_surface",
            trace_surface_started_at,
            format!("mode={subagent_navigation_refresh_mode:?}"),
        );
        let trace_workbench_started_at = current_unix_millis_value();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let rebuild_generation = guard.workbench_generation;
        let returned_workspace_snapshot = workspace_snapshot.clone();
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &returned_workspace_snapshot,
        );
        let hydrated_window_summaries = hydrated_conversation_window_summaries(
            &guard.hydrated_conversation_windows,
            &guard.webui_preloaded_unclosed_subagent_roots,
        );
        let native_tools = native_tool_summaries_from_agent_runtime(guard.agent_runtime.as_ref());
        guard.workspace_snapshot = Some(workspace_snapshot);
        webui_trace_if_slow(
            "workspace_mutation_commit_workspace",
            trace_workbench_started_at,
            format!("mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation}"),
        );
        (
            returned_workspace_snapshot,
            rebuild_generation,
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            provider_configuration,
            ai_tool_configuration,
            surface_messages,
            subagent_navigation,
            pending_questions,
            logs,
            live_context_budgets,
            session_goal_summaries,
            command_session_runtime,
            native_tools,
            hydrated_window_summaries,
        )
    };
    let trace_workbench_started_at = current_unix_millis_value();
    let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
        controller: &controller_runtime,
        command_session_runtime: command_session_runtime.as_deref(),
        luaskills_catalog: &luaskills_catalog,
        token_budget_runtime: &token_budget_runtime,
        mcp_catalog: &mcp_catalog,
        provider_configuration: &provider_configuration,
        ai_tool_configuration: &ai_tool_configuration,
        workspace: &workspace_snapshot,
        surface_messages: surface_messages.as_slice(),
        subagent_navigation,
        pending_questions,
        logs: &logs,
        live_context_budgets: &live_context_budgets,
        session_goal_summaries: &session_goal_summaries,
        hydrated_conversation_windows: &hydrated_window_summaries,
        native_tools,
    });
    webui_trace_if_slow(
        "workspace_mutation_workbench",
        trace_workbench_started_at,
        format!("mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation}"),
    );
    let should_emit = {
        let trace_commit_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let is_current_generation = guard.workbench_generation == rebuild_generation;
        if is_current_generation {
            workbench.revision = rebuild_generation;
            guard.workbench = workbench.clone();
        }
        webui_trace_if_slow(
            "workspace_mutation_commit_workbench",
            trace_commit_started_at,
            format!(
                "mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation} current={is_current_generation}"
            ),
        );
        is_current_generation
    };
    if should_emit {
        let trace_emit_started_at = current_unix_millis_value();
        emit_service_event_with_trace(
            app,
            "workspace_mutation_emit",
            ServiceEvent::Workbench(Box::new(workbench)),
        );
        webui_trace_if_slow(
            "workspace_mutation_emit",
            trace_emit_started_at,
            format!("mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation}"),
        );
    } else {
        webui_trace(
            "workspace_mutation_emit_skipped",
            format!(
                "mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation} reason=stale_generation"
            ),
        );
    }
    webui_trace_if_slow(
        "workspace_mutation_total",
        trace_started_at,
        format!("mode={subagent_navigation_refresh_mode:?}"),
    );
    Ok(workspace_snapshot)
}

/// Returns whether one child task still has a live run after honoring terminal workspace phases.
/// 在优先遵守工作区终态阶段后，返回单个子任务是否仍有实时运行。
pub(crate) fn webui_subagent_task_has_live_run_in_workspace(
    workspace_snapshot: Option<&WorkspaceSnapshot>,
    task_runtime: Option<&Arc<TaskRuntime>>,
    has_live_listener: bool,
    task_id: SessionId,
) -> bool {
    webui_subagent_task_has_live_run_for_phase(
        webui_subagent_task_workspace_phase(workspace_snapshot, task_id),
        task_runtime,
        has_live_listener,
        task_id,
    )
}

/// Returns whether one child task still has a live run after applying one known workspace phase.
/// 在应用单个已知工作区阶段后，返回单个子任务是否仍有实时运行。
pub(crate) fn webui_subagent_task_has_live_run_for_phase(
    workspace_phase: Option<SessionRunPhase>,
    task_runtime: Option<&Arc<TaskRuntime>>,
    has_live_listener: bool,
    task_id: SessionId,
) -> bool {
    let has_runtime_run =
        task_runtime.is_some_and(|runtime| runtime.is_task_running(task_id).unwrap_or(false));
    if has_runtime_run {
        return true;
    }
    has_live_listener && workspace_phase.is_none_or(is_active_session_run_phase)
}

/// Returns the workspace phase recorded for one child task when present.
/// 返回工作区中为某个子任务记录的阶段。
fn webui_subagent_task_workspace_phase(
    workspace_snapshot: Option<&WorkspaceSnapshot>,
    task_id: SessionId,
) -> Option<SessionRunPhase> {
    workspace_snapshot.and_then(|workspace| workspace.session_run_phases.get(&task_id).copied())
}

/// Resolves the shared segmented TaskSession store root for one WebUI parent.
/// 解析单个 WebUI 父会话对应的共享分段 TaskSession 存储根目录。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the selected workspace snapshot that owns session/project mappings.
/// `workspace_snapshot` 是拥有会话与项目映射的当前工作区快照。
/// `parent_session_id` is either the root parent session or a nested child session.
/// `parent_session_id` 是根父会话或嵌套子会话。
///
/// # Returns
/// 返回值
/// The root directory used by TaskRuntime and WebUI segmented readers.
/// 返回 TaskRuntime 与 WebUI 分段读取器共同使用的根目录。
pub(crate) fn webui_task_session_store_root_for_parent_session(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> Option<PathBuf> {
    let session = if let Some(session) =
        workspace_session_summary_for_session(workspace_snapshot, parent_session_id)
    {
        session
    } else {
        let root_session_id = workspace_snapshot.selected_session;
        if root_session_id == SessionId::nil() {
            return None;
        }
        workspace_session_summary_for_session(workspace_snapshot, root_session_id)?
    };
    let data_root = vulcan_workspace_store::data_plane::application_data_root();
    segmented_session_directory_for_summary(&data_root, session)
}

/// Builds the shared segmented TaskSession store for one direct WebUI parent when its selected root session belongs to a project.
/// 当 WebUI 直接父级所属的选中根会话属于项目时构建共享分段 TaskSession 存储。
pub(crate) fn webui_task_session_store_for_parent_session(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> Option<SegmentedTaskSessionStore> {
    let session_dir =
        webui_task_session_store_root_for_parent_session(workspace_snapshot, parent_session_id)?;
    Some(SegmentedTaskSessionStore::new(session_dir))
}

/// Restores the authoritative subagent registry during selected-session hydration.
/// 在选中会话水合阶段恢复权威子代理注册表。
///
/// # Parameters
/// 参数
/// `state` provides the shared runtime bridge that owns the authoritative TaskRuntime registry.
/// `state` 提供拥有权威 TaskRuntime 注册表的共享运行时桥接。
/// `workspace_snapshot` locates the selected session's task-session store directory.
/// `workspace_snapshot` 用于定位选中会话的任务会话存储目录。
/// `parent_session_id` is the selected root parent session being hydrated.
/// `parent_session_id` 是正在水合的选中根父会话。
///
/// # Returns
/// 返回值
/// `Ok(())` once the registry is restored or when no store/runtime exists for the session.
/// 当注册表已恢复，或当前会话没有可用存储/运行时时返回 `Ok(())`。
pub(crate) fn restore_webui_subagent_registry_for_session_hydration(
    state: &SharedServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> Result<(), String> {
    if parent_session_id == SessionId::nil() {
        return Ok(());
    }
    let Some(session_dir) =
        webui_task_session_store_root_for_parent_session(workspace_snapshot, parent_session_id)
    else {
        return Ok(());
    };
    let Some(agent_runtime) = ({
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.agent_runtime.clone()
    }) else {
        return Ok(());
    };
    agent_runtime
        .task_runtime
        .ensure_task_session_store_root(parent_session_id, session_dir)?;
    agent_runtime
        .task_runtime
        .restore_persisted_subagent_registry_for_parent_tree(parent_session_id)
}

/// Stores one loaded subagent detail in the service cache and mirrors its messages into the workspace snapshot.
/// 将单个已加载子代理详情写入服务缓存，并把它的消息镜像进工作区快照。
///
/// # Parameters
/// 参数
/// `guard` receives the render-only detail cache entry.
/// `guard` 接收仅用于渲染的详情缓存项。
/// `workspace_snapshot` receives child transcript messages when the task has replayable state.
/// `workspace_snapshot` 在任务存在可复放状态时接收子转录消息。
/// `parent_session_id` is retained only for the caller's grouping context.
/// `parent_session_id` 仅保留为调用方分组上下文。
/// `task_state` is the loaded child panel state to cache and mirror.
/// `task_state` 是需要缓存和镜像的已加载子面板状态。
pub(crate) fn cache_webui_subagent_surface_detail_for_task(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_state: SubagentTaskPanelState,
) {
    let now = current_unix_millis_value();
    prune_webui_subagent_surface_detail_cache(guard, now);
    if let Some(conversation_state) = task_state.conversation_state.clone() {
        replace_webui_subagent_task_conversation_state(
            workspace_snapshot,
            task_state.task_id,
            conversation_state,
        );
    }
    let messages = display_session_messages_from_workspace(workspace_snapshot, task_state.task_id)
        .unwrap_or_else(|| {
            task_state
                .conversation_state
                .as_ref()
                .map(conversation_state_to_session_messages)
                .unwrap_or_default()
        });
    let _ = parent_session_id;
    guard.subagent_surface_detail_cache.insert(
        task_state.task_id,
        WebuiSubagentSurfaceDetailCacheEntry {
            task_state,
            messages,
            last_access_ms: now,
        },
    );
    prune_webui_subagent_surface_detail_cache(guard, now);
}

/// Loads every unclosed WebUI subagent surface state from disk during session hydration.
/// 在会话水合期间从磁盘读取所有未关闭 WebUI 子智能体界面状态。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` locates the selected session's shared segmented TaskSession store.
/// `workspace_snapshot` 用于定位选中会话共享的分段 TaskSession 存储。
/// `parent_session_id` is the root parent session whose unclosed child tree should be inspected.
/// `parent_session_id` 是需要检查未关闭子树的根父会话。
///
/// # Returns
/// 返回值
/// Disk-loaded surface batches grouped by direct parent session, or a read error that must block the preload marker.
/// 返回按直接父会话分组的磁盘界面批次；若读取失败则返回必须阻止预载标记的错误。
pub(crate) fn load_webui_unclosed_subagent_surface_preload(
    state: &SharedServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> Result<Vec<WebuiSubagentSurfacePreload>, String> {
    if parent_session_id == SessionId::nil() {
        return Ok(Vec::new());
    }
    let entries = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(task_runtime) = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| &runtime.task_runtime)
        else {
            return Ok(Vec::new());
        };
        task_runtime.open_subagent_registry_entries_for_parent_tree(parent_session_id)?
    };
    if entries.is_empty() {
        return Ok(Vec::new());
    };
    let store = webui_task_session_store_for_parent_session(workspace_snapshot, parent_session_id);
    let mut states_by_parent: BTreeMap<SessionId, Vec<SubagentTaskPanelState>> = BTreeMap::new();
    for entry in &entries {
        let direct_parent_session_id = entry.parent_session_id.as_uuid();
        let state = subagent_task_panel_state_from_registry_entry_for_preload(store.as_ref(), entry);
        states_by_parent
            .entry(direct_parent_session_id)
            .or_default()
            .push(state);
    }
    Ok(states_by_parent
        .into_iter()
        .map(|(parent_session_id, task_states)| WebuiSubagentSurfacePreload {
            parent_session_id,
            task_states,
        })
        .collect())
}

/// Loads WebUI subagent states from the root task mirror index and their persisted detail records.
/// 从根任务镜像索引及其持久化详情记录加载 WebUI 子智能体状态。
///
/// # Parameters
/// 参数
/// `store` is the shared segmented task-session store for the root conversation.
/// `store` 是根会话共享的分段任务会话存储。
/// `root_session_id` identifies the root task tree to inspect.
/// `root_session_id` 标识需要检查的根任务树。
///
/// # Returns
/// 返回值
/// Visible, non-archived child panel states grouped by direct persisted parent reference.
/// 返回按直接持久化父引用分组的可见、未归档子面板状态。
/// Converts one root task mirror summary into a detail-ready WebUI subagent panel state.
/// 将单条根任务镜像摘要转换为可展示详情的 WebUI 子智能体面板状态。
///
/// # Parameters
/// 参数
/// `store` provides persisted detail data for the mirrored task session.
/// `store` 提供镜像任务会话对应的持久化详情数据。
/// `mirror` is the root-tree visibility and lifecycle summary.
/// `mirror` 是根任务树中的可见性与生命周期摘要。
///
/// # Returns
/// 返回值
/// A panel state merged from mirror visibility and replayable task details.
/// 返回由镜像可见性与可复放任务详情合并得到的面板状态。
fn subagent_task_panel_state_from_registry_entry_for_preload(
    store: Option<&SegmentedTaskSessionStore>,
    entry: &SubagentRegistryEntry,
) -> SubagentTaskPanelState {
    let parent_session_id = entry.parent_session_id.as_uuid();
    let metadata_state = SubagentTaskPanelState {
        task_id: entry.session_id.as_uuid(),
        public_task_id: entry.agent_id.clone(),
        parent_session_id,
        subagent_label: entry.agent_id.clone(),
        description: entry.last_task_message.clone(),
        run_state: subagent_run_state_from_registry_status_for_preload(entry.status),
        result_text: entry.result_summary.clone().unwrap_or_default(),
        conversation_state: None,
    };
    let detailed_state = store
        .and_then(|store| {
            subagent_task_panel_state_detail_from_segmented_task_session_store(
                store,
                entry.session_id,
            )
        })
        .filter(|state| state.parent_session_id == parent_session_id);
    // The mirror owns visibility; the task detail owns replayable transcript data.
    // 镜像记录负责可见性；任务详情负责可复放的转录数据。
    match detailed_state {
        Some(mut state) => {
            if state.public_task_id.is_empty() {
                state.public_task_id = metadata_state.public_task_id.clone();
            }
            if state.subagent_label.is_empty() {
                state.subagent_label = metadata_state.subagent_label.clone();
            }
            if state.description.is_none() {
                state.description = metadata_state.description.clone();
            }
            if state.result_text.is_empty() {
                state.result_text = metadata_state.result_text.clone();
            }
            state.run_state = metadata_state.run_state;
            state
        }
        None => metadata_state,
    }
}

/// Converts a persisted parent reference into the WebUI session id represented by that parent.
/// 将持久化父级引用转换为该父级对应的 WebUI 会话标识。
fn subagent_run_state_from_registry_status_for_preload(
    status: SubagentStatus,
) -> SubagentTaskRunState {
    match status {
        SubagentStatus::Created
        | SubagentStatus::Running
        | SubagentStatus::WaitingChildren
        | SubagentStatus::WaitingFollowup
        | SubagentStatus::Compacting => SubagentTaskRunState::Running,
        SubagentStatus::Completed => SubagentTaskRunState::Completed,
        SubagentStatus::Failed => SubagentTaskRunState::Failed,
        SubagentStatus::Stopped | SubagentStatus::Archived => SubagentTaskRunState::Stopped,
    }
}

/// Applies disk-loaded child-agent surface batches to service navigation and transcript caches.
/// 将磁盘读取的子智能体界面批次应用到服务导航与转录缓存。
///
/// # Parameters
/// 参数
/// `guard` is the shared service state receiving task-panel caches.
/// `guard` 是接收任务面板缓存的共享服务状态。
/// `workspace_snapshot` receives child transcript messages from loaded task surfaces.
/// `workspace_snapshot` 接收已加载任务界面的子转录消息。
/// `preload` is the already loaded disk surface batch list.
/// `preload` 是已经完成磁盘读取的界面批次列表。
///
/// # Returns
/// 返回值
/// The number of task states merged into the WebUI cache.
/// 返回已合并进 WebUI 缓存的任务状态数量。
pub(crate) fn apply_webui_unclosed_subagent_surface_preload(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &mut WorkspaceSnapshot,
    preload: Vec<WebuiSubagentSurfacePreload>,
) -> usize {
    let mut restored_count = 0;
    for batch in preload {
        restored_count += batch.task_states.len();
        for task_state in batch.task_states {
            cache_webui_subagent_surface_detail_for_task(
                guard,
                workspace_snapshot,
                batch.parent_session_id,
                task_state,
            );
        }
    }
    restored_count
}

/// Returns whether the selected root already has a complete in-memory unclosed subagent surface tree.
/// 返回选中根会话是否已经拥有完整的内存未关闭子智能体界面树。
///
/// # Parameters
/// 参数
/// `guard` is the shared service state that owns the detail preload marker.
/// `guard` 是持有详情预载标记的共享服务状态。
/// `_workspace_snapshot` is reserved for future validation against the selected workspace identity.
/// `_workspace_snapshot` 预留用于后续校验当前选中工作区身份。
/// `parent_session_id` is the root parent session being hydrated.
/// `parent_session_id` 是正在水合的根父会话。
///
/// # Returns
/// 返回值
/// `true` when the detail preload marker exists for this root.
/// 当该根会话存在详情预载标记时返回 `true`。
fn preloaded_unclosed_subagent_surfaces_are_complete(
    guard: &ServiceBridgeState,
    _workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> bool {
    guard
        .webui_preloaded_unclosed_subagent_roots
        .contains(&parent_session_id)
}

/// Loads render details for unclosed subagent surfaces when the selected root has not been preloaded.
/// 当选中根会话尚未预载时加载未关闭子智能体界面的渲染详情。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that tracks already-preloaded detail roots.
/// `state` 是跟踪已预载详情根会话的共享服务状态。
/// `workspace_snapshot` locates the selected session's segmented TaskSession store.
/// `workspace_snapshot` 用于定位选中会话的分段 TaskSession 存储。
/// `parent_session_id` is the root parent session whose child tree may need preload.
/// `parent_session_id` 是可能需要预载子树的根父会话。
///
/// # Returns
/// 返回值
/// `Some(preload)` when hydration must read render detail; `None` when detail was already preloaded.
/// 当水合必须读取渲染详情时返回 `Some(preload)`；详情已经预载时返回 `None`。
pub(crate) fn load_webui_unclosed_subagent_surface_preload_once(
    state: &SharedServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> Result<Option<Vec<WebuiSubagentSurfacePreload>>, String> {
    if parent_session_id == SessionId::nil() {
        return Ok(None);
    }
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if preloaded_unclosed_subagent_surfaces_are_complete(
            &guard,
            workspace_snapshot,
            parent_session_id,
        ) {
            return Ok(None);
        }
    }
    load_webui_unclosed_subagent_surface_preload(state, workspace_snapshot, parent_session_id)
        .map(Some)
}

/// Applies one root-session surface preload and marks that root as preloaded.
/// 应用一次根会话界面预载并标记该根已经预载。
///
/// # Parameters
/// 参数
/// `guard` is the shared service state receiving the preload and marker update.
/// `guard` 是接收预载结果与标记更新的共享服务状态。
/// `workspace_snapshot` receives loaded child transcript surfaces.
/// `workspace_snapshot` 接收已加载的子转录界面。
/// `parent_session_id` is the root parent session represented by the preload.
/// `parent_session_id` 是该预载结果对应的根父会话。
/// `preload` is an optional disk-loaded surface batch list.
/// `preload` 是可选的磁盘界面批次列表。
///
/// # Returns
/// 返回值
/// The number of task states merged into the WebUI cache; `None` leaves the completion marker unchanged.
/// 返回已合并进 WebUI 缓存的任务状态数量；`None` 会保持完成标记不变。
pub(crate) fn apply_webui_unclosed_subagent_surface_preload_once(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    preload: Option<Vec<WebuiSubagentSurfacePreload>>,
) -> usize {
    let Some(preload) = preload else {
        return 0;
    };
    let restored_count =
        apply_webui_unclosed_subagent_surface_preload(guard, workspace_snapshot, preload);
    if parent_session_id != SessionId::nil() {
        guard
            .webui_preloaded_unclosed_subagent_roots
            .insert(parent_session_id);
    }
    restored_count
}

/// Removes stale or excessive loaded subagent detail cache entries.
/// 移除过期或超量的已加载子代理详情缓存项。
pub(crate) fn prune_webui_subagent_surface_detail_cache(
    guard: &mut ServiceBridgeState,
    now_ms: i128,
) {
    guard.subagent_surface_detail_cache.retain(|_, entry| {
        now_ms.saturating_sub(entry.last_access_ms) <= SUBAGENT_SURFACE_DETAIL_CACHE_TTL_MS
    });
    if guard.subagent_surface_detail_cache.len() <= SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT {
        return;
    }
    let mut entries_by_age = guard
        .subagent_surface_detail_cache
        .iter()
        .map(|(task_id, entry)| (*task_id, entry.last_access_ms))
        .collect::<Vec<_>>();
    entries_by_age.sort_by_key(|(_, last_access_ms)| *last_access_ms);
    let overflow = guard
        .subagent_surface_detail_cache
        .len()
        .saturating_sub(SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT);
    for (task_id, _) in entries_by_age.into_iter().take(overflow) {
        guard.subagent_surface_detail_cache.remove(&task_id);
    }
}
