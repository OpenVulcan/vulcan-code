//! runtime workspace helpers helpers.
//! runtime workspace helpers 辅助逻辑。

use super::*;

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
        .and_then(|project_id| {
            workspace
                .task_projects
                .iter()
                .find(|project| project.id == project_id)
        })
        .or_else(|| {
            selected_workspace_session(workspace)
                .and_then(|session| session.project_id.as_deref())
                .and_then(|project_id| {
                    workspace
                        .task_projects
                        .iter()
                        .find(|project| project.id == project_id)
                })
        })
        .or_else(|| workspace.task_projects.first())
}

/// Returns the currently selected workspace session when it exists.
/// 返回当前选中的工作区会话（若存在）。
pub(crate) fn selected_workspace_session<'a>(
    workspace: &'a vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> Option<&'a SessionSummary> {
    workspace
        .sessions
        .iter()
        .find(|session| session.id == workspace.selected_session)
}

/// Reads all long-lived runtime handles required by a WebUI prompt submission.
/// 读取 WebUI 提示词提交所需的全部长生命周期运行时句柄。
pub(crate) fn runtime_parts_from_state(
    state: &SharedServiceBridgeState,
) -> Result<
    (
        ControllerRuntimeHandle,
        AgentRuntimeBridge,
        TauriLuaSkillsCatalog,
        Arc<TauriTokenBudgetRuntime>,
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
    let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; token budget runtime is not ready")
    })?;
    let mcp_catalog = cached_mcp_catalog_from_state(&guard);
    Ok((
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
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
) {
    let now = current_unix_millis_text();
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
        runtime_state.updated_at = now.clone();
    }
    let runtime_state_snapshot = if runtime_state_has_concrete_conversation_context(runtime_state) {
        sanitize_runtime_state_legacy_goal_internal_messages(runtime_state);
        Some(runtime_state.clone())
    } else {
        None
    };
    if let Some(runtime_state_snapshot) = runtime_state_snapshot.as_ref() {
        merge_runtime_state_records_into_display_messages(
            workspace_snapshot,
            session_id,
            runtime_state_snapshot,
        );
    } else {
        workspace_snapshot
            .session_messages
            .entry(session_id)
            .or_default();
    }
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
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
    let pending_subagent_count = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let running_tasks =
            running_webui_subagent_task_states_for_tree_from_live_cache(&mut guard, session_id);
        let running_task_ids = running_tasks
            .iter()
            .map(|(_, state)| state.task_id)
            .collect::<BTreeSet<_>>();
        guard
            .subagent_completion_gate_ignored_tasks
            .retain(|task_id| running_task_ids.contains(task_id));
        running_tasks
            .iter()
            .filter(|(_, state)| {
                !guard
                    .subagent_completion_gate_ignored_tasks
                    .contains(&state.task_id)
            })
            .count()
    };
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

/// Returns running child task states from the in-memory cache without segmented-store refresh.
/// 不刷新分段存储，仅从内存缓存返回仍在运行的子任务状态。
pub(crate) fn running_webui_subagent_task_states_for_tree_from_cache(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    root_parent_session_id: SessionId,
) -> Vec<(SessionId, SubagentTaskPanelState)> {
    collect_running_subagent_task_states_for_tree(root_parent_session_id, |parent_session_id| {
        let task_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| &runtime.task_runtime);
        let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
        guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .map(|tasks| tasks.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|state| {
                normalize_webui_subagent_task_state(
                    Some(workspace_snapshot),
                    &state,
                    task_runtime,
                    live_listener_task_ids.contains(&state.task_id),
                )
            })
            .collect::<Vec<_>>()
    })
}

/// Returns running child task states from live runtime handles without cloning the workspace snapshot.
/// 不克隆工作区快照，仅通过实时运行句柄返回仍在运行的子任务状态。
pub(crate) fn running_webui_subagent_task_states_for_tree_from_live_cache(
    guard: &mut ServiceBridgeState,
    root_parent_session_id: SessionId,
) -> Vec<(SessionId, SubagentTaskPanelState)> {
    collect_running_subagent_task_states_for_tree(root_parent_session_id, |parent_session_id| {
        let task_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| &runtime.task_runtime);
        let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
        guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .map(|tasks| tasks.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|state| {
                normalize_webui_subagent_task_state(
                    None,
                    &state,
                    task_runtime,
                    live_listener_task_ids.contains(&state.task_id),
                )
            })
            .collect::<Vec<_>>()
    })
}

/// Marks one child task run as non-blocking for the parent turn completion gate.
/// 将单个子任务运行标记为不阻塞父级回合完成闸门。
///
/// # Parameters
/// 参数
/// `guard` owns the in-memory service state.
/// `guard` 持有内存中的服务状态。
/// `task_id` identifies the child task whose current run is asynchronous to the parent turn.
/// `task_id` 标识当前运行相对父回合为异步的子任务。
pub(crate) fn mark_webui_subagent_completion_gate_ignored(
    guard: &mut ServiceBridgeState,
    task_id: SessionId,
) {
    if guard.subagent_completion_gate_ignored_tasks.insert(task_id) {
        webui_trace(
            "subagent_completion_gate_ignored",
            format!("task={task_id}"),
        );
    }
}

/// Clears the non-blocking parent completion-gate marker for one child task.
/// 清理单个子任务的非阻塞父级完成闸门标记。
///
/// # Parameters
/// 参数
/// `guard` owns the in-memory service state.
/// `guard` 持有内存中的服务状态。
/// `task_id` identifies the child task whose marker should be removed.
/// `task_id` 标识需要移除标记的子任务。
pub(crate) fn clear_webui_subagent_completion_gate_ignored(
    guard: &mut ServiceBridgeState,
    task_id: SessionId,
) {
    if guard
        .subagent_completion_gate_ignored_tasks
        .remove(&task_id)
    {
        webui_trace(
            "subagent_completion_gate_ignore_cleared",
            format!("task={task_id}"),
        );
    }
}

/// Marks the provided running WebUI subagent tasks as stopped and clears their live listener routes.
/// 将给定的 WebUI 运行中子代理标记为已停止，并清理它们的实时监听路由。
pub(crate) fn mark_webui_subagent_tasks_stopped_in_state(
    guard: &mut ServiceBridgeState,
    task_ids: &[(SessionId, SessionId)],
) {
    for (direct_parent_session_id, task_id) in task_ids {
        if let Some(tasks) = guard
            .session_subagent_tasks
            .get_mut(direct_parent_session_id)
            && let Some(state) = tasks.get_mut(task_id)
        {
            state.run_state = SubagentTaskRunState::Stopped;
            if state.result_text.trim().is_empty() {
                state.result_text = SUBAGENT_TASK_STOPPED_SUMMARY_TEXT.to_string();
            }
        }
        guard.detached_subagent_task_listeners.remove(task_id);
        clear_webui_subagent_completion_gate_ignored(guard, *task_id);
        clear_webui_subagent_parent_turn_binding(guard, *task_id);
        guard
            .session_subagent_navigation_rounds
            .remove(direct_parent_session_id);
        if guard.active_subagent_panel.is_some_and(|selection| {
            selection.parent_session_id == *direct_parent_session_id
                && selection.task_id == *task_id
        }) {
            guard.active_subagent_panel = None;
        }
    }
}

/// Removes closed WebUI subagent tasks from every unclosed-agent surface cache.
/// 从所有未关闭代理界面缓存中移除已关闭的 WebUI 子代理任务。
pub(crate) fn remove_webui_closed_subagent_tasks_in_state(
    guard: &mut ServiceBridgeState,
    task_ids: &[SessionId],
) -> bool {
    if task_ids.is_empty() {
        return false;
    }
    let task_ids = task_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut changed = false;
    let parent_session_ids = guard
        .session_subagent_tasks
        .keys()
        .copied()
        .collect::<Vec<_>>();
    for parent_session_id in parent_session_ids {
        let remove_empty_parent =
            if let Some(tasks) = guard.session_subagent_tasks.get_mut(&parent_session_id) {
                for task_id in &task_ids {
                    changed |= tasks.remove(task_id).is_some();
                }
                tasks.is_empty()
            } else {
                false
            };
        if remove_empty_parent {
            guard.session_subagent_tasks.remove(&parent_session_id);
            guard
                .session_subagent_navigation_rounds
                .remove(&parent_session_id);
        }
    }
    for task_id in &task_ids {
        changed |= guard.detached_subagent_task_listeners.remove(task_id);
        clear_webui_subagent_completion_gate_ignored(guard, *task_id);
        changed |= guard
            .subagent_surface_detail_cache
            .remove(task_id)
            .is_some();
        clear_webui_subagent_parent_turn_binding(guard, *task_id);
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
    had_running_subagents: bool,
) -> Result<bool, String> {
    let now = current_unix_millis_text();
    let had_pending_precheck = workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .is_some_and(|phase| *phase == SessionRunPhase::Preparing);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let had_pending_precheck =
        had_pending_precheck || runtime_state.active_precheck_turn_id.is_some();
    let cleared_system_queue = clear_internal_pending_prompt_queue_for_stop(runtime_state);
    if !had_active_kernel_turn
        && !had_running_subagents
        && !had_pending_precheck
        && !cleared_system_queue
    {
        return Ok(false);
    }
    runtime_state.active_precheck_turn_id = None;
    runtime_state.updated_at = now.clone();
    let stop_body = if had_active_kernel_turn {
        "已按用户请求停止当前 AI 调用、运行中的子代理以及系统内部等待任务；尚未真正执行的排队消息已恢复到可再次插入状态。"
    } else {
        "已按用户请求停止当前会话的子代理与系统内部等待任务；尚未真正执行的排队消息已恢复到可再次插入状态。"
    };
    append_webui_session_message_entry_to_state(
        runtime_state,
        session_id,
        SessionMessageEntry::new(SessionMessageKind::Assistant, "执行已停止", stop_body),
    );
    let provider_request_count = runtime_state
        .last_safe_provider_request_count
        .filter(|count| *count > 0);
    let mut terminated_entry = SessionMessageEntry::hidden(
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::TurnTerminated,
        "Turn Terminated",
        "The run was stopped manually before a summary could be produced.",
    );
    terminated_entry.metadata = Some(json!({ "preview": "" }));
    let terminated_message_id =
        append_webui_session_message_entry_to_state(runtime_state, session_id, terminated_entry);
    let _ = append_webui_session_turn_stats_to_state(
        runtime_state,
        session_id,
        &terminated_message_id,
        SessionTurnStatsStatus::Stopped,
        provider_request_count,
        SessionTurnStatsKind::UserTurn,
        Vec::new(),
    );
    let runtime_state_snapshot = runtime_state.clone();
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("已按用户请求停止当前会话运行。");
        session.last_updated_label = String::from("刚刚");
    }
    merge_runtime_state_records_into_display_messages(
        workspace_snapshot,
        session_id,
        &runtime_state_snapshot,
    );
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
    clear_internal_pending_prompt_queue_for_stop(runtime_state);
    runtime_state.active_precheck_turn_id = None;
    runtime_state.updated_at = now;
}

/// Appends one canonical WebUI session message record and returns its materialized id.
/// 追加一条 WebUI 规范会话消息记录，并返回物化后的消息标识。
pub(crate) fn append_webui_session_message_entry_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    entry: SessionMessageEntry,
) -> String {
    let created_at = current_unix_millis_text();
    let conversation_record_count = runtime_state.conversation_state.records.len();
    let message_id =
        build_session_record_message_id(session_id, &created_at, conversation_record_count);
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
    );
    runtime_state.updated_at = materialized_entry.created_at;
    message_id
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
) -> Option<String> {
    let turn_stats = build_session_turn_stats_from_state_with_segments(
        &runtime_state.conversation_state,
        closing_record_id,
        status,
        provider_request_count_override,
        turn_kind,
        usage_segments,
    )?;
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
    Some(append_webui_session_message_entry_to_state(
        runtime_state,
        session_id,
        entry,
    ))
}

/// Returns the workspace path associated with one session.
/// 返回单个会话关联的工作区路径。
pub(crate) fn workspace_path_for_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> String {
    workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.as_deref())
        .and_then(|project_id| {
            workspace_snapshot
                .task_projects
                .iter()
                .find(|project| project.id == project_id)
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
    workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.clone())
        .filter(|project_id| !project_id.trim().is_empty())
}
