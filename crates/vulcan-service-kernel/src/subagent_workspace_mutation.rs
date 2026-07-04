//! subagent workspace mutation helpers.
//! subagent workspace mutation 辅助逻辑。

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
};

use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime;
use vulcan_session_core::SessionNodeId;
#[cfg(test)]
use vulcan_session_core::{SubagentRegistryEntry, SubagentStatus};

/// Disk-loaded child-agent metadata batch ready to merge into service navigation cache.
/// 已从磁盘读取、可合并进服务导航缓存的子智能体元数据批次。
#[derive(Debug, Clone)]
pub(crate) struct WebuiSubagentMetadataPreload {
    /// Direct parent session that owns these child task states.
    /// 拥有这些子任务状态的直接父会话。
    parent_session_id: SessionId,
    /// Metadata-only child task states loaded from the segmented TaskSession index.
    /// 从分段 TaskSession 索引读取的仅含元数据子任务状态。
    metadata_states: Vec<SubagentTaskPanelState>,
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
        false,
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
    /// Rebuild subagent navigation from runtime and segmented state.
    /// 从运行时和分段状态重建子代理导航。
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
        let hydrated_window_summaries =
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows);
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
    let mut workbench = workbench_from_runtime(
        &controller_runtime,
        command_session_runtime.as_deref(),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_questions,
        &logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
        native_tools,
    );
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

/// Normalizes one subagent task state against the shared task runtime without disk fallback scans.
/// 根据共享任务运行时归一化单条子代理任务状态，且不执行磁盘回退扫描。
pub(crate) fn normalize_webui_subagent_task_state(
    workspace_snapshot: Option<&WorkspaceSnapshot>,
    state: &SubagentTaskPanelState,
    task_runtime: Option<&Arc<TaskRuntime>>,
    has_live_listener: bool,
) -> SubagentTaskPanelState {
    let is_live_running = webui_subagent_task_has_live_run_in_workspace(
        workspace_snapshot,
        task_runtime,
        has_live_listener,
        state.task_id,
    );
    normalize_subagent_task_panel_state(state, is_live_running, None)
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

/// Returns whether one child task still has a live WebUI listener or shared runtime abort handle.
/// 返回单个子任务是否仍拥有 WebUI 实时监听器或共享运行时中止句柄。
pub(crate) fn webui_subagent_task_has_live_run(
    task_runtime: Option<&Arc<TaskRuntime>>,
    has_live_listener: bool,
    task_id: SessionId,
) -> bool {
    has_live_listener
        || task_runtime.is_some_and(|runtime| runtime.is_task_running(task_id).unwrap_or(false))
}

/// Refreshes WebUI subagent task cache from a provided TaskRuntime registry.
/// 从指定 TaskRuntime 注册表刷新 WebUI 子智能体任务缓存。
///
/// # Parameters
/// 参数
/// `guard` receives grouped task states by direct parent session.
/// `guard` 接收按直接父会话分组的任务状态。
/// `workspace_snapshot` provides the durable segmented store root.
/// `workspace_snapshot` 提供持久化分段存储根目录。
/// `root_parent_session_id` is the root session whose child-agent tree is being projected.
/// `root_parent_session_id` 是正在投影其子智能体树的根会话。
/// `task_runtime` is the canonical child-agent runtime used by model-visible orchestration tools.
/// `task_runtime` 是模型可见编排工具使用的规范子智能体运行时。
///
/// # Returns
/// 返回值
/// The number of open registry entries mirrored into WebUI task buckets.
/// 返回已镜像进 WebUI 任务桶的未关闭注册表条目数量。
#[cfg(test)]
pub(crate) fn refresh_webui_subagent_task_cache_from_task_runtime_registry(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    root_parent_session_id: SessionId,
    task_runtime: &TaskRuntime,
) -> usize {
    let Some(store_root) = webui_task_session_store_root_for_parent_session(
        workspace_snapshot,
        root_parent_session_id,
    ) else {
        return 0;
    };
    if let Err(error) =
        task_runtime.configure_task_session_store_root(root_parent_session_id, store_root)
    {
        eprintln!("[webui-service] configure subagent task-session root failed: {error}");
        return 0;
    }
    if let Err(error) =
        task_runtime.restore_persisted_subagent_registry_for_parent_tree(root_parent_session_id)
    {
        eprintln!("[webui-service] restore subagent registry failed: {error}");
        return 0;
    }
    let Ok(registry) = task_runtime.subagent_registry_snapshot() else {
        return 0;
    };
    let mut grouped_states: BTreeMap<SessionId, Vec<SubagentTaskPanelState>> = BTreeMap::new();
    let mut visited_parent_ids = BTreeSet::new();
    let mut pending_parent_ids = VecDeque::from([SessionNodeId::from(root_parent_session_id)]);
    while let Some(parent_node_id) = pending_parent_ids.pop_front() {
        if !visited_parent_ids.insert(parent_node_id) {
            continue;
        }
        for entry in registry.list_for_parent(parent_node_id) {
            if entry.status == SubagentStatus::Archived {
                continue;
            }
            pending_parent_ids.push_back(entry.session_id);
            let direct_parent_session_id = SessionId::from(entry.parent_session_id);
            let previous_state = guard
                .session_subagent_tasks
                .get(&direct_parent_session_id)
                .and_then(|tasks| tasks.get(&SessionId::from(entry.session_id)));
            grouped_states
                .entry(direct_parent_session_id)
                .or_default()
                .push(webui_subagent_task_panel_state_from_registry_entry(
                    entry,
                    previous_state,
                ));
        }
    }
    let restored_count = grouped_states.values().map(Vec::len).sum();
    for (parent_session_id, states) in grouped_states {
        merge_webui_subagent_task_cache_for_session(guard, parent_session_id, states);
    }
    restored_count
}

/// Converts one runtime registry entry into a WebUI task-panel state.
/// 将单个运行时注册表条目转换为 WebUI 任务面板状态。
#[cfg(test)]
fn webui_subagent_task_panel_state_from_registry_entry(
    entry: &SubagentRegistryEntry,
    previous_state: Option<&SubagentTaskPanelState>,
) -> SubagentTaskPanelState {
    let result_text = entry
        .result_summary
        .clone()
        .filter(|text| !text.trim().is_empty())
        .or_else(|| previous_state.map(|state| state.result_text.clone()))
        .unwrap_or_default();
    let conversation_state = previous_state.and_then(|state| state.conversation_state.clone());
    let description = entry
        .last_task_message
        .clone()
        .or_else(|| previous_state.and_then(|state| state.description.clone()));
    SubagentTaskPanelState {
        task_id: SessionId::from(entry.session_id),
        public_task_id: entry.agent_id.clone(),
        parent_session_id: SessionId::from(entry.parent_session_id),
        subagent_label: entry.agent_id.clone(),
        description,
        run_state: subagent_task_run_state_from_registry_status(entry.status),
        result_text,
        conversation_state,
    }
}

/// Converts one registry status into the WebUI panel lifecycle state.
/// 将单个注册表状态转换为 WebUI 面板生命周期状态。
#[cfg(test)]
fn subagent_task_run_state_from_registry_status(status: SubagentStatus) -> SubagentTaskRunState {
    match status {
        SubagentStatus::Failed => SubagentTaskRunState::Failed,
        SubagentStatus::Stopped => SubagentTaskRunState::Stopped,
        SubagentStatus::Completed | SubagentStatus::Archived => SubagentTaskRunState::Completed,
        SubagentStatus::Created
        | SubagentStatus::Running
        | SubagentStatus::WaitingChildren
        | SubagentStatus::WaitingFollowup
        | SubagentStatus::Compacting => SubagentTaskRunState::Running,
    }
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
    let (root_session_id, project_id) = if let Some(project_id) =
        owning_project_id_for_session(workspace_snapshot, parent_session_id)
    {
        (parent_session_id, project_id)
    } else {
        let root_session_id = workspace_snapshot.selected_session;
        if root_session_id == SessionId::nil() {
            return None;
        }
        let project_id = owning_project_id_for_session(workspace_snapshot, root_session_id)?;
        (root_session_id, project_id)
    };
    vulcan_workspace_store::workspace_store::find_segmented_session_directory_for_task_store(
        &vulcan_workspace_store::data_plane::application_data_root(),
        &project_id,
        &root_session_id.to_string(),
    )
    .ok()
    .flatten()
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

/// Loads one WebUI subagent task detail from the shared segmented store without scanning sibling task details.
/// 从共享分段存储加载单个 WebUI 子代理详情，避免扫描兄弟任务详情。
pub(crate) fn segmented_webui_subagent_task_panel_state_for_task(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_session_id: SessionId,
) -> Option<SubagentTaskPanelState> {
    // Measures single-task segmented detail lookup without sibling scans.
    // 衡量不扫描兄弟任务的单任务分段详情读取耗时。
    let detail_lookup_started_at = current_unix_millis_value();
    let Some(store) =
        webui_task_session_store_for_parent_session(workspace_snapshot, parent_session_id)
    else {
        webui_trace_if_slow(
            "subagent_detail_lookup_slow",
            detail_lookup_started_at,
            format!("parent={parent_session_id} task={task_session_id} found=false no_store=true"),
        );
        return None;
    };
    let result = subagent_task_panel_state_detail_from_segmented_task_session_store(
        &store,
        SessionNodeId::from(task_session_id),
    )
    .filter(|state| state.parent_session_id == parent_session_id);
    webui_trace_if_slow(
        "subagent_detail_lookup_slow",
        detail_lookup_started_at,
        format!(
            "parent={parent_session_id} task={task_session_id} found={}",
            result.is_some()
        ),
    );
    result
}

/// Loads one WebUI subagent task detail from the selected root tree by task id.
/// 按任务标识从当前选中根会话树加载单个 WebUI 子代理详情。
pub(crate) fn segmented_webui_subagent_task_panel_state_for_root_tree_task(
    workspace_snapshot: &WorkspaceSnapshot,
    root_parent_session_id: SessionId,
    task_session_id: SessionId,
) -> Option<SubagentTaskPanelState> {
    // Uses the root store as the trust boundary, then keeps the persisted direct parent from metadata.
    // 使用根存储作为可信边界，然后保留元数据中的持久直接父级。
    let detail_lookup_started_at = current_unix_millis_value();
    let Some(store) =
        webui_task_session_store_for_parent_session(workspace_snapshot, root_parent_session_id)
    else {
        webui_trace_if_slow(
            "subagent_tree_detail_lookup_slow",
            detail_lookup_started_at,
            format!(
                "root_parent={root_parent_session_id} task={task_session_id} found=false no_store=true"
            ),
        );
        return None;
    };
    let result = subagent_task_panel_state_detail_from_segmented_task_session_store(
        &store,
        SessionNodeId::from(task_session_id),
    )
    .filter(|state| state.parent_session_id != SessionId::nil());
    webui_trace_if_slow(
        "subagent_tree_detail_lookup_slow",
        detail_lookup_started_at,
        format!(
            "root_parent={root_parent_session_id} task={task_session_id} found={}",
            result.is_some()
        ),
    );
    result
}

/// Finds one cached WebUI subagent task state by child task identifier across direct-parent buckets.
/// 跨直接父级缓存桶按子任务标识查找一条 WebUI 子代理任务状态。
pub(crate) fn webui_subagent_task_state_by_task_id(
    guard: &ServiceBridgeState,
    task_id: SessionId,
) -> Option<&SubagentTaskPanelState> {
    guard
        .session_subagent_tasks
        .values()
        .find_map(|tasks| tasks.get(&task_id))
}

/// Resolves the direct parent session for one cached WebUI subagent task.
/// 解析单个缓存 WebUI 子代理任务的直接父会话。
pub(crate) fn webui_subagent_task_parent_session_id_by_task_id(
    guard: &ServiceBridgeState,
    preferred_parent_session_id: SessionId,
    task_id: SessionId,
) -> Option<SessionId> {
    if guard
        .session_subagent_tasks
        .get(&preferred_parent_session_id)
        .is_some_and(|tasks| tasks.contains_key(&task_id))
    {
        return Some(preferred_parent_session_id);
    }
    guard
        .session_subagent_tasks
        .iter()
        .find_map(|(parent_session_id, tasks)| {
            tasks.contains_key(&task_id).then_some(*parent_session_id)
        })
        .or_else(|| {
            guard
                .subagent_surface_detail_cache
                .get(&task_id)
                .map(|entry| entry.parent_session_id)
        })
}

/// Merges preloaded segmented task states into the in-memory WebUI subagent cache.
/// 将预加载的分段任务状态合并到 WebUI 内存子代理缓存。
pub(crate) fn merge_webui_subagent_task_cache_for_session(
    guard: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    segmented_states: Vec<SubagentTaskPanelState>,
) {
    if parent_session_id == SessionId::nil() {
        return;
    }
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
    let previous_states = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .cloned()
        .unwrap_or_default();
    let merged_states =
        merge_subagent_task_panel_cache(&previous_states, segmented_states, |task_id| {
            webui_subagent_task_has_live_run(
                task_runtime,
                live_listener_task_ids.contains(&task_id),
                task_id,
            )
        });
    if merged_states.is_empty() {
        guard.session_subagent_tasks.remove(&parent_session_id);
    } else {
        guard
            .session_subagent_tasks
            .insert(parent_session_id, merged_states);
    }
    if let Some(selection) = guard.active_subagent_panel
        && selection.parent_session_id == parent_session_id
        && !guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .is_some_and(|tasks| tasks.contains_key(&selection.task_id))
    {
        guard.active_subagent_panel = None;
    }
}

/// Stores one loaded subagent detail in the service cache and mirrors its messages into the workspace snapshot.
/// 将单个已加载子代理详情写入服务缓存，并把它的消息镜像进工作区快照。
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
    guard.subagent_surface_detail_cache.insert(
        task_state.task_id,
        WebuiSubagentSurfaceDetailCacheEntry {
            parent_session_id,
            task_state,
            messages,
            last_access_ms: now,
        },
    );
    prune_webui_subagent_surface_detail_cache(guard, now);
}

/// Loads every unclosed WebUI subagent metadata state from disk without loading child transcript details.
/// 在不加载子转录详情的情况下，从磁盘读取所有未关闭 WebUI 子智能体元数据状态。
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
/// Disk-loaded metadata batches grouped by direct parent session.
/// 返回按直接父会话分组的磁盘元数据批次。
pub(crate) fn load_webui_unclosed_subagent_metadata_preload(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> Vec<WebuiSubagentMetadataPreload> {
    if parent_session_id == SessionId::nil() {
        return Vec::new();
    }
    let Some(store) =
        webui_task_session_store_for_parent_session(workspace_snapshot, parent_session_id)
    else {
        return Vec::new();
    };
    let metadata_records = match store.load_all_task_session_metadata() {
        Ok(metadata_records) => metadata_records,
        Err(error) => {
            eprintln!("[webui-service] load subagent task metadata failed: {}", error.message);
            return Vec::new();
        }
    };
    let mut grouped_states: BTreeMap<SessionId, Vec<SubagentTaskPanelState>> = BTreeMap::new();
    for metadata in metadata_records {
        if metadata.status == vulcan_task_session::TaskSessionStatus::Archived {
            continue;
        }
        let Some(state) = subagent_task_panel_state_from_task_session(&metadata, None, None) else {
            continue;
        };
        grouped_states
            .entry(state.parent_session_id)
            .or_default()
            .push(state);
    }
    let mut batches = Vec::new();
    let mut visited_parent_ids = BTreeSet::new();
    let mut pending_parent_ids = VecDeque::from([parent_session_id]);
    while let Some(direct_parent_session_id) = pending_parent_ids.pop_front() {
        if !visited_parent_ids.insert(direct_parent_session_id) {
            continue;
        }
        let Some(states) = grouped_states.remove(&direct_parent_session_id) else {
            continue;
        };
        for state in &states {
            pending_parent_ids.push_back(state.task_id);
        }
        batches.push(WebuiSubagentMetadataPreload {
            parent_session_id: direct_parent_session_id,
            metadata_states: states,
        });
    }
    batches
}

/// Applies disk-loaded child-agent metadata batches to service navigation caches.
/// 将磁盘读取的子智能体元数据批次应用到服务导航缓存。
///
/// # Parameters
/// 参数
/// `guard` is the shared service state receiving task-panel caches.
/// `guard` 是接收任务面板缓存的共享服务状态。
/// `preload` is the already loaded disk metadata batch list.
/// `preload` 是已经完成磁盘读取的元数据批次列表。
///
/// # Returns
/// 返回值
/// The number of metadata task states merged into the WebUI cache.
/// 返回已合并进 WebUI 缓存的元数据任务状态数量。
pub(crate) fn apply_webui_unclosed_subagent_metadata_preload(
    guard: &mut ServiceBridgeState,
    preload: Vec<WebuiSubagentMetadataPreload>,
) -> usize {
    let mut restored_count = 0;
    for batch in preload {
        restored_count += batch.metadata_states.len();
        merge_webui_subagent_task_cache_for_session(
            guard,
            batch.parent_session_id,
            batch.metadata_states,
        );
    }
    restored_count
}

/// Restores one loaded subagent detail cache entry into the workspace and task-panel caches.
/// 将单个已加载子代理详情缓存恢复进工作区与任务面板缓存。
pub(crate) fn restore_webui_subagent_surface_detail_cache_for_task(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_session_id: SessionId,
) -> bool {
    let now = current_unix_millis_value();
    prune_webui_subagent_surface_detail_cache(guard, now);
    let Some(entry) = guard
        .subagent_surface_detail_cache
        .get_mut(&task_session_id)
        .filter(|entry| entry.parent_session_id == parent_session_id)
    else {
        return false;
    };
    entry.last_access_ms = now;
    if let Some(conversation_state) = entry.task_state.conversation_state.clone() {
        replace_webui_subagent_task_conversation_state(
            workspace_snapshot,
            task_session_id,
            conversation_state,
        );
    } else if !entry.messages.is_empty() {
        workspace_snapshot
            .session_messages
            .insert(task_session_id, entry.messages.clone());
        workspace_snapshot
            .hydrated_session_ids
            .insert(task_session_id);
    }
    guard
        .session_subagent_tasks
        .entry(parent_session_id)
        .or_default()
        .insert(task_session_id, entry.task_state.clone());
    true
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
