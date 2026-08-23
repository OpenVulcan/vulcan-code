use super::*;

/// Builds the compact label used by one subagent dispatch row from its agent name only.
/// 仅根据代理名称构建单条子代理调度行使用的紧凑标签。
pub fn subagent_navigation_task_label(state: &SubagentTaskPanelState) -> String {
    let label = state.subagent_label.trim();
    if !label.is_empty() {
        return label.to_string();
    }
    String::from("智能体")
}

/// Converts one explicit persisted execution-status string into the canonical subagent run state.
/// 将一条明确存在的持久化 execution-status 字符串转换为规范子代理运行状态。
pub fn subagent_run_state_from_execution_status(
    status: Option<&str>,
) -> Option<SubagentTaskRunState> {
    match status.map(str::trim).filter(|value| !value.is_empty())? {
        "created" | "running" | "waiting_children" | "waiting_followup" | "compacting" => {
            Some(SubagentTaskRunState::Running)
        }
        "completed" | "archived" => Some(SubagentTaskRunState::Completed),
        "stopped" => Some(SubagentTaskRunState::Stopped),
        "failed" => Some(SubagentTaskRunState::Failed),
        _ => None,
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
/// A display-ready subagent panel state, `None` when the metadata has no direct parent, or a parse error.
/// 返回可直接显示的子代理面板状态；若元数据没有直接父级则返回 `None`，解析失败则返回错误。
pub fn subagent_task_panel_state_from_task_session(
    metadata: &TaskSessionMetadata,
    context: Option<&SessionContextDocument>,
    latest_event_payload: Option<&Value>,
) -> Result<Option<SubagentTaskPanelState>, SessionStoreError> {
    let Some(parent_session_id) = parent_session_id_from_task_session_parent(&metadata.parent)
    else {
        return Ok(None);
    };
    let conversation_state = match context {
        Some(document) => Some(
            serde_json::from_value::<SessionConversationState>(document.conversation_state.clone())
                .map_err(|error| {
                    SessionStoreError::new(format!(
                        "TaskSession `{}` context conversation_state is invalid: {error}",
                        metadata.task_session_id
                    ))
                })?,
        ),
        None => None,
    };
    let result_text = latest_event_payload
        .and_then(|payload| payload.get("result"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| metadata.last_result_summary.clone())
        .unwrap_or_default();
    let subagent_label = task_session_tool_display_label(
        Some(metadata.display_title.as_str()),
        metadata.description.as_deref(),
        None,
        Some(metadata.public_task_id.as_str()),
        None,
    );

    Ok(Some(SubagentTaskPanelState {
        task_id: metadata.task_session_id.as_uuid(),
        public_task_id: metadata.public_task_id.clone(),
        parent_session_id,
        subagent_label,
        description: metadata.description.clone(),
        run_state: subagent_run_state_from_task_session_status(metadata.status),
        result_text,
        conversation_state,
    }))
}

/// Builds one list-level panel state directly from the root task mirror index.
/// 直接从根任务镜像索引构建一条列表级面板状态。
///
/// # Parameters
/// 参数
/// `mirror` is the compact task summary read from the root session metadata file.
/// `mirror` 是从根会话元数据文件读取的紧凑任务摘要。
///
/// # Returns
/// 返回值
/// A display-ready state without opening the child task directory, or `None` for a rootless entry.
/// 返回无需打开子任务目录即可显示的状态；无根条目返回 `None`。
fn subagent_task_panel_state_from_task_mirror(
    mirror: &TaskSessionMirrorSummary,
) -> Option<SubagentTaskPanelState> {
    let parent_session_id = parent_session_id_from_task_session_parent(&mirror.parent)?;
    let subagent_label = task_session_tool_display_label(
        Some(mirror.display_title.as_str()),
        mirror.description.as_deref(),
        None,
        Some(mirror.public_task_id.as_str()),
        None,
    );
    Some(SubagentTaskPanelState {
        task_id: mirror.task_session_id.as_uuid(),
        public_task_id: mirror.public_task_id.clone(),
        parent_session_id,
        subagent_label,
        description: mirror.description.clone(),
        run_state: subagent_run_state_from_task_session_status(mirror.status),
        result_text: mirror.last_result_summary.clone().unwrap_or_default(),
        conversation_state: None,
    })
}

/// Loads direct child panel states from the single root task mirror index.
/// 仅从单个根任务镜像索引加载直接子代理面板状态。
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
/// A list of display-ready child panel states, or the first storage read error.
/// 返回可直接显示的子代理面板状态列表；若存储读取失败则返回首个错误。
pub fn subagent_task_panel_states_from_segmented_task_session_store(
    store: &SegmentedTaskSessionStore,
    parent_session_id: SessionId,
) -> Result<Vec<SubagentTaskPanelState>, SessionStoreError> {
    let parent = subagent_parent_ref_from_task_session_store(store, parent_session_id)?;
    let mut states = Vec::new();
    for mirror in store.load_task_session_mirror_summaries()? {
        if mirror.parent != parent || mirror.status == TaskSessionStatus::Archived {
            continue;
        }
        let Some(state) = subagent_task_panel_state_from_task_mirror(&mirror) else {
            return Err(SessionStoreError::new(format!(
                "TaskSession `{}` has no direct parent.",
                mirror.task_session_id
            )));
        };
        if state.parent_session_id != parent_session_id {
            return Err(SessionStoreError::new(format!(
                "TaskSession `{}` parent mismatch: expected `{}`, got `{}`.",
                mirror.task_session_id, parent_session_id, state.parent_session_id
            )));
        }
        states.push(state);
    }
    Ok(states)
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
    normalized_state.run_state = SubagentTaskRunState::Stopped;
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
