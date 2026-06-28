use super::*;

/// Builds the compact label used by one subagent dispatch row from its agent name and optional task description.
/// 根据代理名称与可选任务简介构建单条子代理调度行使用的紧凑标签。
pub fn subagent_navigation_task_label(state: &SubagentTaskPanelState) -> String {
    let label = state.subagent_label.trim();
    let description = state
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match description {
        Some(description) if !label.contains(description) => {
            format!("{label}（{description}）")
        }
        _ if !label.is_empty() => label.to_string(),
        _ => String::from("子代理"),
    }
}

/// Converts one persisted execution-status string into the canonical subagent run state.
/// 将一条持久化 execution-status 字符串转换为规范子代理运行状态。
pub fn subagent_run_state_from_execution_status(status: Option<&str>) -> SubagentTaskRunState {
    match status.unwrap_or("completed") {
        "running" => SubagentTaskRunState::Running,
        "stopped" => SubagentTaskRunState::Stopped,
        "failed" => SubagentTaskRunState::Failed,
        _ => SubagentTaskRunState::Completed,
    }
}

/// Converts a persisted TaskSession status into the workbench-facing panel run state.
/// 将持久化 TaskSession 状态转换为工作台面板使用的运行状态。
pub fn subagent_run_state_from_task_session_status(
    status: TaskSessionStatus,
) -> SubagentTaskRunState {
    match status {
        TaskSessionStatus::Created
        | TaskSessionStatus::Running
        | TaskSessionStatus::WaitingChildren
        | TaskSessionStatus::WaitingFollowup
        | TaskSessionStatus::Compacting => SubagentTaskRunState::Running,
        TaskSessionStatus::Stopped => SubagentTaskRunState::Stopped,
        TaskSessionStatus::Failed => SubagentTaskRunState::Failed,
        TaskSessionStatus::Completed | TaskSessionStatus::Archived => {
            SubagentTaskRunState::Completed
        }
    }
}

/// Builds a shared workbench panel state from segmented TaskSession metadata and optional persisted context.
/// 根据分段 TaskSession 元数据和可选持久上下文构建共享工作台面板状态。
///
/// # Parameters
/// 参数
/// `metadata` is the canonical TaskSession metadata loaded from the task metadata file.
/// `metadata` 是从任务元数据文件加载的规范 TaskSession 元数据。
/// `context` is the optional persisted context document containing the reviewable conversation state.
/// `context` 是可选的持久上下文文档，里面包含可复看的会话状态。
/// `latest_event_payload` is the newest lifecycle payload used to recover full result text when available.
/// `latest_event_payload` 是最新生命周期载荷，用于在可用时恢复完整结果文本。
///
/// # Returns
/// 返回值
/// A display-ready subagent panel state, or `None` when the metadata has no direct parent.
/// 返回可直接显示的子代理面板状态；若元数据没有直接父级则返回 `None`。
pub fn subagent_task_panel_state_from_task_session(
    metadata: &TaskSessionMetadata,
    context: Option<&SessionContextDocument>,
    latest_event_payload: Option<&Value>,
) -> Option<SubagentTaskPanelState> {
    let parent_session_id = parent_session_id_from_task_session_parent(&metadata.parent)?;
    let conversation_state = context.and_then(|document| {
        serde_json::from_value::<SessionConversationState>(document.conversation_state.clone()).ok()
    });
    let result_text = latest_event_payload
        .and_then(|payload| payload.get("result"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| metadata.last_result_summary.clone())
        .unwrap_or_default();
    let subagent_label = task_session_tool_display_label(
        Some(metadata.display_title.as_str()),
        metadata.description.as_deref(),
        metadata.effective_config.subagent_profile_id.as_deref(),
        Some(metadata.public_task_id.as_str()),
        None,
    );

    Some(SubagentTaskPanelState {
        task_id: metadata.task_session_id.as_uuid(),
        public_task_id: metadata.public_task_id.clone(),
        parent_session_id,
        subagent_label,
        description: metadata.description.clone(),
        run_state: subagent_run_state_from_task_session_status(metadata.status),
        result_text,
        conversation_state,
    })
}

/// Loads direct child panel states from a segmented TaskSession store without reading every task event.
/// 从分段 TaskSession 存储加载直接子代理面板状态，并避免读取每个子任务的完整事件历史。
///
/// # Parameters
/// 参数
/// `store` is the segmented TaskSession store rooted at the main session directory.
/// `store` 是以主会话目录为根的分段 TaskSession 存储。
/// `parent_session_id` is the direct parent session whose child tasks should be listed.
/// `parent_session_id` 是需要列出子任务的直接父会话标识。
///
/// # Returns
/// 返回值
/// A best-effort list of display-ready child panel states; unreadable items are skipped.
/// 返回可直接显示的子代理面板状态列表；无法读取的条目会被跳过。
pub fn subagent_task_panel_states_from_segmented_task_session_store(
    store: &SegmentedTaskSessionStore,
    parent_session_id: SessionId,
) -> Vec<SubagentTaskPanelState> {
    let Some(parent) = subagent_parent_ref_from_task_session_store(store, parent_session_id) else {
        return Vec::new();
    };
    store
        .list_child_task_sessions(parent)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|metadata| subagent_task_panel_state_from_task_session(&metadata, None, None))
        .filter(|state| state.parent_session_id == parent_session_id)
        .collect()
}

/// Normalizes one cached panel state against the shared live runtime fact and an optional terminal persisted snapshot.
/// 根据共享实时运行事实与可选终态持久化快照，归一化单条缓存面板状态。
///
/// # Parameters
/// 参数
/// `state` is the cached panel state that may still contain a stale running flag.
/// `state` 是可能仍包含陈旧运行中标记的缓存面板状态。
/// `is_live_running` reports whether the shared TaskRuntime still owns the task abort handle.
/// `is_live_running` 表示共享 TaskRuntime 是否仍持有该任务的中止句柄。
/// `terminal_persisted_state` is a fresher non-running state loaded from segmented storage.
/// `terminal_persisted_state` 是从分段存储读取到的更新终态状态。
///
/// # Returns
/// 返回值
/// A panel state whose run-state matches the shared task lifecycle semantics.
/// 返回运行状态与共享任务生命周期语义一致的面板状态。
pub fn normalize_subagent_task_panel_state(
    state: &SubagentTaskPanelState,
    is_live_running: bool,
    terminal_persisted_state: Option<&SubagentTaskPanelState>,
) -> SubagentTaskPanelState {
    if is_live_running {
        let mut normalized_state = state.clone();
        normalized_state.run_state = SubagentTaskRunState::Running;
        return normalized_state;
    }
    if !state.run_state.is_running() {
        return state.clone();
    }
    if let Some(persisted_state) =
        terminal_persisted_state.filter(|state| !state.run_state.is_running())
    {
        return persisted_state.clone();
    }

    let mut normalized_state = state.clone();
    normalized_state.run_state =
        if normalized_state.result_text.trim() == SUBAGENT_TASK_STOPPED_SUMMARY_TEXT.trim() {
            SubagentTaskRunState::Stopped
        } else {
            SubagentTaskRunState::Completed
        };
    normalized_state
}

/// Merges segmented and previous live states into the canonical subagent panel cache.
/// 将分段存储与上一轮实时状态合并为规范子代理面板缓存。
///
/// # Parameters
/// 参数
/// `previous_states` is the frontend cache from the previous refresh.
/// `previous_states` 是前端上一轮刷新保留的缓存。
/// `segmented_states` are canonical TaskSession states loaded from the shared segmented store.
/// `segmented_states` 是从共享分段存储读取的规范 TaskSession 状态。
/// `is_task_live_running` is the host-supplied shared TaskRuntime lookup.
/// `is_task_live_running` 是宿主提供的共享 TaskRuntime 查询函数。
///
/// # Returns
/// 返回值
/// A task-id keyed cache map ready to replace the caller's UI cache bucket.
/// 返回按任务标识索引、可直接替换调用方 UI 缓存桶的映射。
pub fn merge_subagent_task_panel_cache<F>(
    previous_states: &BTreeMap<SessionId, SubagentTaskPanelState>,
    segmented_states: Vec<SubagentTaskPanelState>,
    mut is_task_live_running: F,
) -> BTreeMap<SessionId, SubagentTaskPanelState>
where
    F: FnMut(SessionId) -> bool,
{
    let persisted_states = segmented_states;
    let terminal_persisted_states = persisted_states
        .iter()
        .filter(|state| !state.run_state.is_running())
        .map(|state| (state.task_id, state.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut merged = previous_states
        .iter()
        .filter_map(|(task_id, state)| {
            let is_live_running = is_task_live_running(*task_id);
            let normalized = normalize_subagent_task_panel_state(
                state,
                is_live_running,
                terminal_persisted_states.get(task_id),
            );
            (normalized.run_state.is_running() && is_live_running).then_some((*task_id, normalized))
        })
        .collect::<BTreeMap<_, _>>();

    for state in persisted_states {
        let normalized = if state.run_state.is_running() {
            previous_states
                .get(&state.task_id)
                .map(|existing| {
                    normalize_subagent_task_panel_state(
                        existing,
                        is_task_live_running(existing.task_id),
                        terminal_persisted_states.get(&existing.task_id),
                    )
                })
                .filter(|existing| !existing.run_state.is_running())
                .unwrap_or_else(|| {
                    normalize_subagent_task_panel_state(
                        &state,
                        is_task_live_running(state.task_id),
                        None,
                    )
                })
        } else {
            normalize_subagent_task_panel_state(
                &state,
                is_task_live_running(state.task_id),
                terminal_persisted_states.get(&state.task_id),
            )
        };
        merged.insert(normalized.task_id, normalized);
    }

    merged
}
