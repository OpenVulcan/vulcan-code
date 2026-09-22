//! runtime workspace helpers helpers.
//! runtime workspace helpers 辅助逻辑。

use super::*;
use vulcan_session_core::v1::collaboration::CollaborationChildTaskState;

/// Returns the selected project path for a VMM request scope.
/// 返回 VMM 请求作用域选中的项目路径。
pub(crate) fn vmm_project_scope_path(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("工作区尚未载入，无法保存项目 VMM 配置。"))?;
    vmm_request_project_scope(workspace_snapshot, project_id)
        .map(|project| project.path.clone())
        .ok_or_else(|| String::from("选中的项目已不存在，请重新选择。"))
}

/// Finds the project scope requested by WebUI or implied by the selected session.
/// 查找 WebUI 请求或当前选中会话隐含的项目作用域。
pub(crate) fn vmm_request_project_scope<'a>(
    workspace: &'a vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    project_id: Option<&str>,
) -> Option<&'a TaskProjectSummary> {
    project_id
        .and_then(|project_id| workspace_task_project_for_project_id(workspace, project_id))
        .or_else(|| {
            selected_workspace_session(workspace)
                .and_then(session_summary_project_id)
                .and_then(|project_id| workspace_task_project_for_project_id(workspace, project_id))
        })
        .or_else(|| workspace.task_projects.first())
}

/// Returns the currently selected workspace session when it exists.
/// 返回当前选中的工作区会话（若存在）。
pub(crate) fn selected_workspace_session(
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> Option<&SessionSummary> {
    let selected_session_id = selected_workspace_session_id(workspace)?;
    workspace
        .sessions
        .iter()
        .find(|session| session.id == selected_session_id)
}

/// Returns the selected session id when the workspace has a real selection.
/// 当工作区存在真实选中会话时返回选中会话标识。
///
/// # Parameters
/// 参数
/// `workspace` contains the retained selected session id.
/// `workspace` 包含保留的选中会话标识。
///
/// # Returns
/// 返回值
/// The selected session id when it is not the nil placeholder.
/// 当选中会话不是 nil 占位值时返回该会话标识。
pub(crate) fn selected_workspace_session_id(
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> Option<SessionId> {
    (workspace.selected_session != SessionId::nil()).then_some(workspace.selected_session)
}

/// Returns one visible project matching a non-blank project id.
/// 返回与非空白项目标识匹配的单个可见项目。
///
/// # Parameters
/// 参数
/// `workspace` contains visible project summaries.
/// `workspace` 包含可见项目摘要。
/// `project_id` is the requested project identifier.
/// `project_id` 是请求的项目标识。
///
/// # Returns
/// 返回值
/// The matching project summary when the id is non-blank and present.
/// 当项目标识非空白且存在时返回匹配的项目摘要。
pub(crate) fn workspace_task_project_for_project_id<'a>(
    workspace: &'a WorkspaceSnapshot,
    project_id: &str,
) -> Option<&'a TaskProjectSummary> {
    if project_id.trim().is_empty() {
        return None;
    }
    workspace
        .task_projects
        .iter()
        .find(|project| project.id == project_id)
}

/// Iterates visible projects followed by archived projects.
/// 依次遍历可见项目与归档项目。
///
/// # Parameters
/// 参数
/// `workspace` contains both active and archived project summaries.
/// `workspace` 包含活动与归档项目摘要。
///
/// # Returns
/// 返回值
/// An iterator over active project rows first, then archived project rows.
/// 返回先活动项目、后归档项目的迭代器。
pub(crate) fn workspace_task_projects_including_archived<'a>(
    workspace: &'a WorkspaceSnapshot,
) -> impl Iterator<Item = &'a TaskProjectSummary> + 'a {
    workspace
        .task_projects
        .iter()
        .chain(workspace.archived_task_projects.iter())
}

/// Returns one visible or archived project matching a non-blank project id.
/// 返回与非空白项目标识匹配的单个可见或归档项目。
///
/// # Parameters
/// 参数
/// `workspace` contains active and archived project summaries.
/// `workspace` 包含活动与归档项目摘要。
/// `project_id` is the requested project identifier.
/// `project_id` 是请求的项目标识。
///
/// # Returns
/// 返回值
/// The matching project summary when the id is non-blank and present in either project list.
/// 当项目标识非空白且存在于任一项目列表中时返回匹配项目摘要。
pub(crate) fn workspace_task_project_for_project_id_including_archived<'a>(
    workspace: &'a WorkspaceSnapshot,
    project_id: &str,
) -> Option<&'a TaskProjectSummary> {
    if project_id.trim().is_empty() {
        return None;
    }
    workspace_task_projects_including_archived(workspace).find(|project| project.id == project_id)
}

/// Returns one mutable visible project matching a non-blank project id.
/// 返回与非空白项目标识匹配的单个可变可见项目。
///
/// # Parameters
/// 参数
/// `workspace` contains visible project summaries.
/// `workspace` 包含可见项目摘要。
/// `project_id` is the requested project identifier.
/// `project_id` 是请求的项目标识。
///
/// # Returns
/// 返回值
/// The matching mutable project summary when the id is non-blank and present.
/// 当项目标识非空白且存在时返回匹配的可变项目摘要。
pub(crate) fn workspace_task_project_for_project_id_mut<'a>(
    workspace: &'a mut WorkspaceSnapshot,
    project_id: &str,
) -> Option<&'a mut TaskProjectSummary> {
    if project_id.trim().is_empty() {
        return None;
    }
    workspace
        .task_projects
        .iter_mut()
        .find(|project| project.id == project_id)
}

/// Returns the retained active session summary for one session id.
/// 返回单个会话标识对应的保留活动会话摘要。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains active session summaries.
/// `workspace_snapshot` 包含活动会话摘要。
/// `session_id` identifies the session to resolve.
/// `session_id` 标识需要解析的会话。
///
/// # Returns
/// 返回值
/// The matching active session summary when present.
/// 存在时返回匹配的活动会话摘要。
pub(crate) fn workspace_session_summary_for_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<&SessionSummary> {
    workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
}

/// Returns the mutable active session summary from a session slice.
/// 从会话切片返回可变活动会话摘要。
///
/// # Parameters
/// 参数
/// `sessions` contains active session summaries.
/// `sessions` 包含活动会话摘要。
/// `session_id` identifies the session to resolve.
/// `session_id` 标识需要解析的会话。
///
/// # Returns
/// 返回值
/// The matching mutable active session summary when present.
/// 存在时返回匹配的可变活动会话摘要。
pub(crate) fn active_session_summary_for_session_mut(
    sessions: &mut [SessionSummary],
    session_id: SessionId,
) -> Option<&mut SessionSummary> {
    sessions.iter_mut().find(|session| session.id == session_id)
}

/// Returns the mutable active session summary for one session id.
/// 返回单个会话标识对应的可变活动会话摘要。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains active session summaries.
/// `workspace_snapshot` 包含活动会话摘要。
/// `session_id` identifies the session to resolve.
/// `session_id` 标识需要解析的会话。
///
/// # Returns
/// 返回值
/// The matching mutable active session summary when present.
/// 存在时返回匹配的可变活动会话摘要。
pub(crate) fn workspace_session_summary_for_session_mut(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<&mut SessionSummary> {
    active_session_summary_for_session_mut(&mut workspace_snapshot.sessions, session_id)
}

/// Returns the persisted project id from one session summary.
/// 从单个会话摘要返回持久化项目标识。
///
/// # Parameters
/// 参数
/// `session` is the retained session summary.
/// `session` 是保留的会话摘要。
///
/// # Returns
/// 返回值
/// A non-empty project id when the session belongs to a project.
/// 当会话属于项目时返回非空项目标识。
pub(crate) fn session_summary_project_id(session: &SessionSummary) -> Option<&str> {
    session
        .project_id
        .as_deref()
        .filter(|project_id| !project_id.trim().is_empty())
}

/// Returns the owned persisted project id from one session summary.
/// 从单个会话摘要返回拥有所有权的持久化项目标识。
///
/// # Parameters
/// 参数
/// `session` is the retained session summary.
/// `session` 是保留的会话摘要。
///
/// # Returns
/// 返回值
/// A non-empty owned project id when the session belongs to a project.
/// 当会话属于项目时返回拥有所有权的非空项目标识。
pub(crate) fn session_summary_project_id_owned(session: &SessionSummary) -> Option<String> {
    session_summary_project_id(session).map(str::to_string)
}

/// Reads all long-lived runtime handles required by a WebUI prompt submission.
/// 读取 WebUI 提示词提交所需的全部长生命周期运行时句柄。
pub(crate) fn runtime_parts_from_state(
    state: &SharedServiceBridgeState,
) -> Result<
    (
        HostRuntimeHandle,
        AgentRuntimeBridge,
        TauriLuaSkillsCatalog,
        TauriMcpRuntimeCatalog,
        Vec<String>,
    ),
    String,
> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; controller runtime is not ready")
    })?;
    let agent_runtime = guard
        .agent_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
    let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
    let mcp_catalog = mcp_runtime_catalog_from_state(&guard);
    Ok((
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        mcp_catalog,
        guard.startup.recent_logs.clone(),
    ))
}

/// Ensures one WebUI session has an in-memory runtime-state document after SQLite runtime storage was removed.
/// 在 SQLite 运行态存储移除后，确保单个 WebUI 会话拥有内存运行态文档。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot that will be repaired in memory.
/// `workspace_snapshot` 是将被内存修复的 WebUI 保留工作区快照。
/// `session_id` selects the session that must be runnable.
/// `session_id` 选择必须可运行的会话。
/// `active_precheck_turn_id` optionally restores the turn id carried by the already-created precheck request.
/// `active_precheck_turn_id` 可选恢复已创建 precheck 请求携带的回合标识。
pub(crate) fn ensure_webui_session_runtime_state(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    active_precheck_turn_id: Option<&str>,
) -> Result<(), String> {
    ensure_webui_session_runtime_entry(workspace_snapshot, session_id, active_precheck_turn_id)?;
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
    Ok(())
}

/// Ensures one WebUI session has a runtime entry without rebuilding presentation history.
/// 确保一个 WebUI 会话拥有运行态条目，且不重建展示历史。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the exact session runtime map.
/// `workspace_snapshot` 持有精确会话运行态映射。
/// `session_id` selects the session that must have an entry.
/// `session_id` 选择必须拥有条目的会话。
/// `active_precheck_turn_id` optionally fills an absent preparation owner.
/// `active_precheck_turn_id` 可选填充缺失的准备所有者。
///
/// # Returns
/// 返回值
/// Success after in-place entry materialization and optional owner restoration.
/// 原地物化条目并可选恢复所有者后返回成功。
pub(crate) fn ensure_webui_session_runtime_entry(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    active_precheck_turn_id: Option<&str>,
) -> Result<(), String> {
    // now timestamps only a new entry or restored preparation owner.
    // now 只为新条目或恢复的准备所有者提供时间戳。
    let now = current_unix_millis_text();
    // runtime_state is the exact in-place row; no canonical history is cloned.
    // runtime_state 是精确的原地记录；不会复制规范历史。
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    if let Some(turn_id) = active_precheck_turn_id
        .map(str::trim)
        .filter(|turn_id| !turn_id.is_empty())
        && runtime_state
            .active_precheck_turn_id
            .as_deref()
            .is_none_or(|current| current.trim().is_empty())
    {
        runtime_state.active_precheck_turn_id = Some(turn_id.to_string());
        runtime_state.updated_at = now;
    }
    Ok(())
}

/// Returns whether the session has a submitted Plan artifact awaiting user confirmation.
/// 返回会话是否存在等待用户确认的已提交计划产物。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains retained session runtime state.
/// `workspace_snapshot` 包含保留的会话运行态。
/// `session_id` identifies the session to inspect.
/// `session_id` 标识需要检查的会话。
///
/// # Returns
/// 返回值
/// `true` when a Plan artifact is still pending a user decision.
/// 当计划产物仍在等待用户决策时返回 `true`。
pub(crate) fn session_has_pending_plan_artifact(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.plan_mode.artifact.as_ref())
        .is_some()
}

/// Returns whether cached WebUI state still reports a running child subagent.
/// 返回 WebUI 缓存状态是否仍报告存在运行中的子代理。
#[cfg(test)]
pub(crate) fn session_has_running_subagent_tasks_from_cache(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    !running_webui_subagent_task_states_for_tree_from_cache(guard, workspace_snapshot, session_id)
        .is_empty()
}

/// Gate state used to decide whether a completed model request closes the logical user turn.
/// 用于判断一次模型请求完成是否闭合逻辑用户回合的闸门状态。
#[derive(Debug, Clone, Copy)]
pub(crate) struct WebuiTurnCompletionGate {
    /// Number of still-running descendant subagent tasks under this parent session.
    /// 该父会话下仍在运行的后代子代理任务数量。
    pub(crate) pending_subagent_count: usize,
    /// Whether the shared hook runtime still has the original user turn open.
    /// 共享 hook 运行时是否仍保留原始用户回合。
    pub(crate) has_active_hook_turn: bool,
}

impl WebuiTurnCompletionGate {
    /// Returns whether the logical user turn still has unfinished subagent work.
    /// 返回逻辑用户回合是否仍有未完成的子代理工作。
    pub(crate) fn has_pending_work(self) -> bool {
        self.pending_subagent_count > 0
    }

    /// Converts the gate into the hook payload completion status.
    /// 将闸门状态转换为 hook 载荷完成状态。
    pub(crate) fn completion_status(self) -> ConversationTurnCompletionStatus {
        if self.has_pending_work() {
            ConversationTurnCompletionStatus::AwaitingSubagents
        } else {
            ConversationTurnCompletionStatus::Completed
        }
    }
}

/// Reads the shared turn-completion gate without mutating the transcript application path.
/// 在不改变转录应用路径的情况下读取共享回合完成闸门。
pub(crate) fn webui_turn_completion_gate_from_state(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
) -> Result<WebuiTurnCompletionGate, String> {
    // The durable document is the completion gate; live task cards cannot settle a parent turn.
    // 可靠文档才是完成门，实时任务卡片不能结算父回合。
    let session_runtime_registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        std::sync::Arc::clone(&guard.session_runtime_registry)
    };
    let session_handle = session_runtime_registry
        .get(session_id)
        .map_err(|error| format!("failed to resolve durable session completion gate: {error}"))?;
    let durable_has_pending_collaboration = session_handle
        .read(|document| {
            document
                .active_turn()
                .map(|turn| {
                    !document
                        .completion_obligations(turn.turn_id)
                        .open_child_task_ids
                        .is_empty()
                })
                .unwrap_or_else(|| {
                    document.collaboration_state().child_tasks.values().any(|task| {
                        matches!(
                            task.state,
                            CollaborationChildTaskState::Reserved
                                | CollaborationChildTaskState::Active
                        )
                    })
                })
        })
        .map_err(|error| format!("failed to read durable collaboration completion gate: {error}"))?;
    let pending_subagent_count = usize::from(durable_has_pending_collaboration);
    let has_active_hook_turn = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map(|hook_runtime| hook_runtime.has_active_turn(session_id))
        .unwrap_or(false);
    Ok(WebuiTurnCompletionGate {
        pending_subagent_count,
        has_active_hook_turn,
    })
}

/// Returns running child task states from the authoritative runtime registry.
/// 从权威运行时注册表返回仍在运行的子任务状态。
#[cfg(test)]
pub(crate) fn running_webui_subagent_task_states_for_tree_from_cache(
    guard: &mut ServiceBridgeState,
    _workspace_snapshot: &WorkspaceSnapshot,
    root_parent_session_id: SessionId,
) -> Vec<(SessionId, SubagentTaskPanelState)> {
    running_webui_subagent_task_states_for_tree_from_live_cache(guard, root_parent_session_id)
}

/// Returns running child task states from the authoritative runtime registry without workspace access.
/// 不访问工作区，仅从权威运行时注册表返回仍在运行的子任务状态。
pub(crate) fn running_webui_subagent_task_states_for_tree_from_live_cache(
    guard: &mut ServiceBridgeState,
    root_parent_session_id: SessionId,
) -> Vec<(SessionId, SubagentTaskPanelState)> {
    let Some(task_runtime) = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime)
    else {
        return Vec::new();
    };
    task_runtime
        .open_subagent_registry_entries_for_parent_tree(root_parent_session_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.status.is_running())
        .map(|entry| {
            let parent_session_id = entry.parent_session_id.as_uuid();
            (
                parent_session_id,
                subagent_task_panel_state_from_registry_for_gate(entry),
            )
        })
        .collect()
}

/// Builds a lightweight WebUI panel state from a registry entry for run-gate calculations.
/// 为运行闸门计算从注册表条目构建轻量 WebUI 面板状态。
fn subagent_task_panel_state_from_registry_for_gate(
    entry: vulcan_session_core::SubagentRegistryEntry,
) -> SubagentTaskPanelState {
    SubagentTaskPanelState {
        task_id: entry.session_id.as_uuid(),
        public_task_id: entry.agent_id.clone(),
        parent_session_id: entry.parent_session_id.as_uuid(),
        subagent_label: entry.agent_id,
        description: entry.last_task_message,
        run_state: SubagentTaskRunState::Running,
        result_text: entry.result_summary.unwrap_or_default(),
        conversation_state: None,
    }
}
/// Clears the non-blocking parent completion-gate marker for one exact child execution.
/// 清理单个精确子执行的非阻塞父级完成闸门标记。
///
/// # Parameters
/// 参数
/// `guard` owns the in-memory service state.
/// `guard` 持有内存中的服务状态。
/// `task_id` identifies the stable child task.
/// `task_id` 标识稳定子任务。
/// `execution_id` identifies the concrete execution whose marker should be removed.
/// `execution_id` 标识需要移除标记的具体执行。
pub(crate) fn clear_webui_subagent_completion_gate_ignored(
    guard: &mut ServiceBridgeState,
    task_id: SessionId,
    execution_id: &str,
) {
    if guard
        .subagent_completion_gate_ignored_tasks
        .remove(&SubagentExecutionKey::new(task_id, execution_id))
    {
        webui_trace(
            "subagent_completion_gate_ignore_cleared",
            format!("task={task_id} execution_id={execution_id}"),
        );
    }
}

/// Clears every completion-gate marker owned by one stable child task.
/// 清理单个稳定子任务持有的全部完成闸门标记。
///
/// # Parameters
/// 参数
/// `guard` owns the in-memory service state.
/// `guard` 持有内存中的服务状态。
/// `task_id` identifies the child task being explicitly stopped or closed.
/// `task_id` 标识正在被显式停止或关闭的子任务。
pub(crate) fn clear_all_webui_subagent_completion_gate_ignored_for_task(
    guard: &mut ServiceBridgeState,
    task_id: SessionId,
) {
    guard
        .subagent_completion_gate_ignored_tasks
        .retain(|execution_key| execution_key.task_id != task_id);
}

/// Marks the provided running WebUI subagent tasks as stopped and clears their live listener routes.
/// 将给定的 WebUI 运行中子代理标记为已停止，并清理它们的实时监听路由。
pub(crate) fn mark_webui_subagent_tasks_stopped_in_state(
    guard: &mut ServiceBridgeState,
    task_ids: &[(SessionId, SessionId)],
) {
    for (direct_parent_session_id, task_id) in task_ids {
        if let Some(entry) = guard.subagent_surface_detail_cache.get_mut(task_id) {
            entry.task_state.run_state = SubagentTaskRunState::Stopped;
            if entry.task_state.result_text.trim().is_empty() {
                entry.task_state.result_text = SUBAGENT_TASK_STOPPED_SUMMARY_TEXT.to_string();
            }
        }
        guard.detached_subagent_task_listeners.remove(task_id);
        clear_all_webui_subagent_completion_gate_ignored_for_task(guard, *task_id);
        // Stopping execution is not a new user turn and must retain closed siblings for inspection.
        // 停止执行不代表新用户回合，必须保留已关闭的兄弟任务供查看。
        if guard.active_subagent_panel.is_some_and(|selection| {
            selection.parent_session_id == *direct_parent_session_id
                && selection.task_id == *task_id
        }) {
            guard.active_subagent_panel = None;
        }
    }
}

/// Unloads process-local runtime state for closed WebUI subagents without hiding their current-turn rows.
/// 卸载已关闭 WebUI 子代理的进程内运行状态，同时不隐藏其当前回合列表行。
pub(crate) fn unload_webui_closed_subagent_runtime_state(
    guard: &mut ServiceBridgeState,
    task_ids: &[SessionId],
) -> bool {
    if task_ids.is_empty() {
        return false;
    }
    let task_ids = task_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut changed = false;
    for task_id in &task_ids {
        changed |= guard
            .detached_subagent_task_listeners
            .remove(task_id)
            .is_some();
        clear_all_webui_subagent_completion_gate_ignored_for_task(guard, *task_id);
        changed |= guard
            .subagent_surface_detail_cache
            .remove(task_id)
            .is_some();
    }
    if guard
        .active_subagent_panel
        .is_some_and(|selection| task_ids.contains(&selection.task_id))
    {
        guard.active_subagent_panel = None;
        changed = true;
    }
    changed
}

/// Stops WebUI-visible session work and restores queue state to the service stopped semantics.
/// 停止 WebUI 可见会话任务，并按服务停止语义恢复队列状态。
pub(crate) fn stop_webui_session_work(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    had_active_kernel_turn: bool,
    had_active_stream_task: bool,
    stopped_running_subagents: bool,
) -> Result<bool, String> {
    let now = current_unix_millis_text();
    let had_pending_precheck = workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .is_some_and(|phase| *phase == SessionRunPhase::Preparing);
    let mut runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .cloned()
        .unwrap_or_else(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let had_pending_precheck =
        had_pending_precheck || runtime_state.active_precheck_turn_id.is_some();
    // hadActiveRunProjection closes a retained Running state even when a prior abnormal exit already lost both live owners.
    // hadActiveRunProjection 即使先前异常退出已经丢失两个实时所有者，也会收敛保留的 Running 状态。
    let had_active_run_projection = workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .is_some_and(|phase| is_active_session_run_phase(*phase))
        || workspace_session_summary_for_session(workspace_snapshot, session_id)
            .is_some_and(|session| session.status == SessionState::Running);
    if !had_active_kernel_turn
        && !had_active_stream_task
        && !stopped_running_subagents
        && !had_pending_precheck
        && !had_active_run_projection
    {
        return Ok(false);
    }
    runtime_state.active_precheck_turn_id = None;
    runtime_state.updated_at = now.clone();
    let mut terminated_entry = SessionMessageEntry::hidden(
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::TurnTerminated,
        "Turn Terminated",
        "The run was stopped manually before a summary could be produced.",
    );
    terminated_entry.metadata = Some(json!({ "preview": "" }));
    let terminated_message_id = append_webui_session_message_entry_to_state(
        &mut runtime_state,
        session_id,
        terminated_entry,
    )?;
    let _ = append_webui_session_turn_stats_to_state(
        &mut runtime_state,
        session_id,
        &terminated_message_id,
        SessionTurnStatsStatus::Stopped,
        None,
        SessionTurnStatsKind::UserTurn,
        Vec::new(),
    )?;
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    if let Some(session) = workspace_session_summary_for_session_mut(workspace_snapshot, session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("已按用户请求停止当前会话运行。");
        session.last_updated_label = String::from("刚刚");
    }
    workspace_snapshot
        .session_runtime_state
        .insert(session_id, runtime_state);
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
    Ok(true)
}

/// Applies the immediate stopped gate before slower stop projection and persistence finish.
/// 在较慢的停止投影与持久化完成前先应用即时停止门禁。
pub(crate) fn mark_webui_session_stop_requested_in_workspace(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) {
    let now = current_unix_millis_text();
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.active_precheck_turn_id = None;
    runtime_state.updated_at = now;
}

/// Appends one canonical WebUI session message record and returns its materialized id.
/// 追加一条 WebUI 规范会话消息记录，并返回物化后的消息标识。
pub(crate) fn append_webui_session_message_entry_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    entry: SessionMessageEntry,
) -> Result<String, String> {
    let created_at = current_unix_millis_text();
    let conversation_record_count = runtime_state.conversation_state.records.len();
    let message_id = next_available_session_record_message_id(
        &runtime_state.conversation_state,
        session_id,
        &created_at,
        conversation_record_count,
    )?;
    let materialized_entry = entry.with_runtime_metadata(message_id.clone(), created_at);
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: materialized_entry.id.clone(),
            kind: materialized_entry.kind,
            record_kind: materialized_entry.record_kind,
            visible: materialized_entry.visible,
            title: materialized_entry.title.clone(),
            body: materialized_entry.body.clone(),
            metadata: materialized_entry.metadata.clone(),
            created_at: materialized_entry.created_at.clone(),
        },
    )
    .map_err(|error| format!("Invalid WebUI session record identity: {error}"))?;
    runtime_state.updated_at = materialized_entry.created_at;
    Ok(message_id)
}

/// Builds the first session-record identifier not owned by canonical messages or records.
/// 构建首个未被规范消息或记录占用的会话记录标识。
///
/// # Parameters
/// 参数
/// `conversation_state` is the authoritative identity set, `session_id` and `created_at` form the stable prefix,
/// and `initial_ordinal` is the preferred first ordinal.
/// `conversation_state` 是权威身份集合，`session_id` 与 `created_at` 组成稳定前缀，
/// `initial_ordinal` 是优先尝试的首个序号。
///
/// # Returns
/// 返回值
/// Returns one unused identifier, or an explicit error if the ordinal space is exhausted.
/// 返回一个未使用标识；序号空间耗尽时返回显式错误。
pub(crate) fn next_available_session_record_message_id(
    conversation_state: &vulcan_context::SessionConversationState,
    session_id: SessionId,
    created_at: &str,
    initial_ordinal: usize,
) -> Result<String, String> {
    // ordinal probing is required because exact record compaction can create holes below surviving historical IDs.
    // 必须探测 ordinal，因为精确记录压缩会在仍存历史 ID 之前产生序号空洞。
    let mut ordinal = initial_ordinal;
    loop {
        let candidate = build_session_record_message_id(session_id, created_at, ordinal);
        let identity_is_owned = conversation_state
            .messages
            .iter()
            .any(|message| message.id == candidate)
            || conversation_state
                .records
                .iter()
                .any(|record| record.id == candidate);
        if !identity_is_owned {
            return Ok(candidate);
        }
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| String::from("Session record identity ordinal space is exhausted."))?;
    }
}

/// Appends one canonical WebUI turn-statistics record using the shared WebUI turn stats builder.
/// 使用 WebUI 轮次统计构建器追加一条 WebUI 规范轮次统计记录。
pub(crate) fn append_webui_session_turn_stats_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    closing_record_id: &str,
    status: SessionTurnStatsStatus,
    provider_request_count_override: Option<usize>,
    turn_kind: SessionTurnStatsKind,
    usage_segments: Vec<SessionTurnUsageSegment>,
) -> Result<Option<String>, String> {
    let Some(turn_stats) = build_session_turn_stats_from_state_with_segments(
        &runtime_state.conversation_state,
        closing_record_id,
        status,
        provider_request_count_override,
        turn_kind,
        usage_segments,
    ) else {
        return Ok(None);
    };
    let mut entry = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        format!("Turn Stats · {}", turn_stats.status.label()),
        turn_stats.detail_body(),
    );
    entry.record_kind = SessionMessageRecordKind::TurnStats;
    entry.metadata = serde_json::to_value(json!({
        "turn_stats": turn_stats,
        "turn_closing_record_id": closing_record_id,
    }))
    .ok();
    append_webui_session_message_entry_to_state(runtime_state, session_id, entry).map(Some)
}

/// Returns the workspace path associated with one session.
/// 返回单个会话关联的工作区路径。
pub(crate) fn workspace_path_for_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> String {
    workspace_session_summary_for_session(workspace_snapshot, session_id)
        .and_then(session_summary_project_id)
        .and_then(|project_id| {
            workspace_task_project_for_project_id(workspace_snapshot, project_id)
        })
        .map(|project| project.path.clone())
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root()))
}

/// Returns the owning project id for one session when it has a persisted project scope.
/// 当会话拥有持久化项目作用域时返回所属项目标识。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the session summaries.
/// `workspace_snapshot` 包含会话摘要。
/// `session_id` selects the session.
/// `session_id` 选择目标会话。
///
/// # Returns
/// 返回值
/// A non-empty project id when the session belongs to one project.
/// 当会话属于某个项目时返回非空项目标识。
pub(crate) fn owning_project_id_for_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<String> {
    workspace_session_summary_for_session(workspace_snapshot, session_id)
        .and_then(session_summary_project_id)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    /// Builds one workspace snapshot with a single session and project.
    /// 构建一个包含单个会话和项目的工作区快照。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the session stored in the snapshot.
    /// `session_id` 是写入快照的会话标识。
    /// `project_id` is the project id stored on the session summary.
    /// `project_id` 是写入会话摘要的项目标识。
    ///
    /// # Returns
    /// 返回值
    /// A workspace snapshot containing one project and one session.
    /// 返回包含一个项目与一个会话的工作区快照。
    fn workspace_snapshot_with_session_project(
        session_id: SessionId,
        project_id: Option<String>,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                "project-a",
                "Project A",
                r"D:\projects\project-a",
                false,
                0,
                None,
                None,
                Vec::new(),
            )],
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                project_id,
                "测试会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies workspace path resolution returns the owning project path for valid project ids.
    /// 验证工作区路径解析会为有效项目标识返回所属项目路径。
    #[test]
    fn workspace_path_for_session_returns_project_path_for_valid_project_id() {
        // session_id identifies the session being resolved in this test.
        // session_id 标识本测试中要解析的会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot provides one matching project for the session.
        // workspace_snapshot 为该会话提供一个匹配的项目。
        let workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("project-a")));

        assert_eq!(
            workspace_path_for_session(&workspace_snapshot, session_id),
            r"D:\projects\project-a"
        );
    }

    /// Verifies project helpers keep active-only and active-plus-archived semantics separate.
    /// 验证项目辅助函数会区分仅活动与活动加归档语义。
    #[test]
    fn workspace_task_project_including_archived_resolves_archived_project() {
        // session_id anchors the snapshot helper without affecting project lookup.
        // session_id 用于构造快照辅助数据，不影响项目查找。
        let session_id = SessionId::new_v4();
        // workspace_snapshot contains one active project and one archived project.
        // workspace_snapshot 包含一个活动项目和一个归档项目。
        let mut workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("project-a")));
        workspace_snapshot
            .archived_task_projects
            .push(TaskProjectSummary::new(
                "archived-project",
                "Archived Project",
                r"D:\\projects\\archived",
                false,
                1,
                None,
                None,
                Vec::new(),
            ));

        // project_ids records the iteration order promised by the shared helper.
        // project_ids 记录共享辅助函数承诺的遍历顺序。
        let project_ids = workspace_task_projects_including_archived(&workspace_snapshot)
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(project_ids, vec!["project-a", "archived-project"]);
        assert!(
            workspace_task_project_for_project_id(&workspace_snapshot, "archived-project")
                .is_none()
        );
        assert_eq!(
            workspace_task_project_for_project_id_including_archived(
                &workspace_snapshot,
                "archived-project"
            )
            .map(|project| project.path.as_str()),
            Some(r"D:\\projects\\archived")
        );
        assert!(
            workspace_task_project_for_project_id_including_archived(&workspace_snapshot, "   ")
                .is_none()
        );
    }

    /// Verifies active session lookup does not resolve archived sessions.
    /// 验证活动会话查找不会解析归档会话。
    #[test]
    fn workspace_session_summary_for_session_ignores_archived_sessions() {
        // active_session_id identifies the retained active session row.
        // active_session_id 标识保留的活动会话行。
        let active_session_id = SessionId::new_v4();
        // archived_session_id identifies a row that has moved to archived sessions.
        // archived_session_id 标识已经移动到归档会话的行。
        let archived_session_id = SessionId::new_v4();
        // workspace_snapshot contains both active and archived rows to prove active-only semantics.
        // workspace_snapshot 同时包含活动和归档行，用于证明仅活动语义。
        let mut workspace_snapshot = workspace_snapshot_with_session_project(
            active_session_id,
            Some(String::from("project-a")),
        );
        workspace_snapshot
            .archived_sessions
            .push(SessionSummary::new(
                archived_session_id,
                Some(String::from("project-a")),
                "归档会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ));

        assert!(
            workspace_session_summary_for_session(&workspace_snapshot, active_session_id).is_some()
        );
        assert!(
            workspace_session_summary_for_session(&workspace_snapshot, archived_session_id)
                .is_none()
        );
    }

    /// Verifies mutable active session lookup updates the matching session only.
    /// 验证可变活动会话查找只会更新匹配会话。
    #[test]
    fn workspace_session_summary_for_session_mut_updates_matching_session() {
        // session_id identifies the mutable session under test.
        // session_id 标识本测试中的可变会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot contains one session that should be resolved mutably.
        // workspace_snapshot 包含一个应被可变解析的会话。
        let mut workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("project-a")));

        {
            // session stores the mutable summary returned by the shared helper.
            // session 保存共享辅助函数返回的可变摘要。
            let session =
                workspace_session_summary_for_session_mut(&mut workspace_snapshot, session_id)
                    .expect("existing active session should resolve mutably");
            session.status = SessionState::Running;
            session.focus = String::from("正在准备请求");
        }

        assert_eq!(workspace_snapshot.sessions[0].status, SessionState::Running);
        assert_eq!(workspace_snapshot.sessions[0].focus, "正在准备请求");
        assert!(
            workspace_session_summary_for_session_mut(&mut workspace_snapshot, SessionId::new_v4())
                .is_none()
        );
    }

    /// Verifies slice-level mutable active session lookup updates the matching session only.
    /// 验证切片级可变活动会话查找只会更新匹配会话。
    #[test]
    fn active_session_summary_for_session_mut_updates_matching_session_slice() {
        // session_id identifies the mutable session inside the active session slice.
        // session_id 标识活动会话切片中的可变会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot owns the active session slice used by the helper.
        // workspace_snapshot 持有辅助函数使用的活动会话切片。
        let mut workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("project-a")));

        {
            // session stores the mutable summary returned from the slice-level helper.
            // session 保存切片级辅助函数返回的可变摘要。
            let session = active_session_summary_for_session_mut(
                &mut workspace_snapshot.sessions,
                session_id,
            )
            .expect("existing active session should resolve mutably");
            session.status = SessionState::Blocked;
            session.focus = String::from("终态报告已应用");
        }

        assert_eq!(workspace_snapshot.sessions[0].status, SessionState::Blocked);
        assert_eq!(workspace_snapshot.sessions[0].focus, "终态报告已应用");
        assert!(
            active_session_summary_for_session_mut(
                &mut workspace_snapshot.sessions,
                SessionId::new_v4()
            )
            .is_none()
        );
    }

    /// Verifies selected session id projection ignores the nil placeholder.
    /// 验证选中会话标识投影会忽略 nil 占位值。
    #[test]
    fn selected_workspace_session_id_ignores_nil_selection() {
        // session_id identifies the real session stored in the snapshot.
        // session_id 标识快照中保存的真实会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot starts with a selected session that will be cleared to nil.
        // workspace_snapshot 初始包含选中会话，随后会被清空为 nil。
        let mut workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("project-a")));
        workspace_snapshot.selected_session = SessionId::nil();

        assert_eq!(selected_workspace_session_id(&workspace_snapshot), None);
        assert!(selected_workspace_session(&workspace_snapshot).is_none());
    }

    /// Verifies blank session project ids use the same workspace-root fallback as missing projects.
    /// 验证空白会话项目标识会使用与缺失项目相同的工作区根目录回退。
    #[test]
    fn workspace_path_for_session_falls_back_for_blank_project_id() {
        // session_id identifies the session with an invalid blank project id.
        // session_id 标识携带无效空白项目标识的会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot keeps the blank project id so the fallback path is observable.
        // workspace_snapshot 保留空白项目标识，以便观察回退路径。
        let workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("   ")));

        assert_eq!(
            workspace_path_for_session(&workspace_snapshot, session_id),
            normalized_workspace_path_text_from_path(&workspace_root())
        );
    }

    /// Verifies owning project resolution drops blank session project ids.
    /// 验证所属项目解析会丢弃空白会话项目标识。
    #[test]
    fn owning_project_id_for_session_ignores_blank_project_id() {
        // session_id identifies the session whose owning project is resolved.
        // session_id 标识需要解析所属项目的会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot keeps a blank project id on the session summary.
        // workspace_snapshot 在会话摘要中保留空白项目标识。
        let workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("   ")));

        assert_eq!(
            owning_project_id_for_session(&workspace_snapshot, session_id),
            None
        );
    }

    /// Verifies owned session project id projection drops blank values.
    /// 验证拥有所有权的会话项目标识投影会丢弃空白值。
    #[test]
    fn session_summary_project_id_owned_ignores_blank_project_id() {
        // session_id identifies the session used to build the test summary.
        // session_id 标识用于构建测试摘要的会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot keeps a blank project id on its only session.
        // workspace_snapshot 在唯一会话中保留空白项目标识。
        let workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("   ")));

        assert_eq!(
            session_summary_project_id_owned(&workspace_snapshot.sessions[0]),
            None
        );
    }

    /// Verifies VMM project scope ignores blank project ids before falling back.
    /// 验证 VMM 项目作用域会先忽略空白项目标识再执行回退。
    #[test]
    fn vmm_request_project_scope_ignores_blank_project_id() {
        // session_id identifies the selected session with a blank project id.
        // session_id 标识携带空白项目标识的选中会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot contains both a normal project and a whitespace-id project.
        // workspace_snapshot 同时包含普通项目与空白标识项目。
        let mut workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("   ")));
        workspace_snapshot
            .task_projects
            .push(TaskProjectSummary::new(
                "   ",
                "Blank Project",
                r"D:\projects\blank",
                false,
                0,
                None,
                None,
                Vec::new(),
            ));

        // selected_scope stores the resolved project after blank id filtering.
        // selected_scope 保存空白标识过滤后的项目解析结果。
        let selected_scope = vmm_request_project_scope(&workspace_snapshot, Some("   "))
            .expect("VMM scope should fall back to the first visible project");

        assert_eq!(selected_scope.id, "project-a");
    }

    /// Verifies mutable project lookup shares the same blank-id filtering as immutable lookup.
    /// 验证可变项目查找与不可变查找共享相同的空白标识过滤。
    #[test]
    fn workspace_task_project_for_project_id_mut_rejects_blank_project_id() {
        // session_id anchors the snapshot helper without affecting project lookup.
        // session_id 用于构造快照辅助数据，不影响项目查找。
        let session_id = SessionId::new_v4();
        // workspace_snapshot contains one normal project and one malformed blank project.
        // workspace_snapshot 包含一个普通项目和一个异常空白项目。
        let mut workspace_snapshot =
            workspace_snapshot_with_session_project(session_id, Some(String::from("project-a")));
        workspace_snapshot
            .task_projects
            .push(TaskProjectSummary::new(
                "   ",
                "Blank Project",
                r"D:\projects\blank",
                false,
                1,
                None,
                None,
                Vec::new(),
            ));

        {
            // project stores the mutable project resolved by a valid non-blank id.
            // project 保存通过有效非空白标识解析出的可变项目。
            let project =
                workspace_task_project_for_project_id_mut(&mut workspace_snapshot, "project-a")
                    .expect("valid project id should resolve to a mutable project");
            project.title = String::from("Updated Project");
        }

        assert_eq!(workspace_snapshot.task_projects[0].title, "Updated Project");
        assert!(
            workspace_task_project_for_project_id_mut(&mut workspace_snapshot, "   ").is_none()
        );
        assert_eq!(workspace_snapshot.task_projects[1].title, "Blank Project");
    }
}
