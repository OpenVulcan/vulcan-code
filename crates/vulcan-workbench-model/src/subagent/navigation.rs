use super::*;

/// Collects running subagent task states by walking the direct-child TaskSession tree from one parent.
/// 从一个父级开始遍历直接子 TaskSession 树，并收集仍在运行的子代理任务状态。
///
/// # Parameters
/// 参数
/// `root_parent_session_id` is the parent session whose child tree should be inspected.
/// `root_parent_session_id` 是需要检查其子任务树的父会话标识。
/// `child_states_for_parent` returns already refreshed and normalized direct-child task states for each visited parent.
/// `child_states_for_parent` 返回每个已访问父级下已经刷新并归一化的直接子任务状态。
///
/// # Returns
/// 返回值
/// A list of running child task states paired with the direct parent that owns each task card.
/// 返回仍在运行的子任务状态列表，并携带每个任务卡片所属的直接父级。
pub fn collect_running_subagent_task_states_for_tree<F>(
    root_parent_session_id: SessionId,
    mut child_states_for_parent: F,
) -> Vec<(SessionId, SubagentTaskPanelState)>
where
    F: FnMut(SessionId) -> Vec<SubagentTaskPanelState>,
{
    let mut running_states = Vec::new();
    let mut seen_parent_ids = BTreeSet::new();
    let mut pending_parent_ids = vec![root_parent_session_id];
    while let Some(parent_session_id) = pending_parent_ids.pop() {
        if !seen_parent_ids.insert(parent_session_id) {
            continue;
        }
        for state in child_states_for_parent(parent_session_id) {
            pending_parent_ids.push(state.task_id);
            if state.run_state.is_running() {
                running_states.push((parent_session_id, state));
            }
        }
    }
    running_states
}

/// Returns whether one shared panel state contains reviewable child conversation content.
/// 返回一条共享面板状态是否包含可复看的子会话内容。
///
/// # Parameters
/// 参数
/// `state` is the shared subagent panel state loaded from live runtime or the segmented TaskSession store.
/// `state` 是从实时运行时或分段 TaskSession 存储加载的共享子代理面板状态。
///
/// # Returns
/// 返回值
/// `true` when either canonical records or protocol messages can be projected into a child transcript.
/// 当规范记录或协议消息可以投影为子会话转录时返回 `true`。
pub fn subagent_task_panel_state_has_recoverable_conversation(
    state: &SubagentTaskPanelState,
) -> bool {
    state
        .conversation_state
        .as_ref()
        .is_some_and(|conversation_state| {
            !conversation_state.records.is_empty() || !conversation_state.messages.is_empty()
        })
}

/// Returns whether one normalized subagent panel may be opened by a UI surface.
/// 返回一条已归一化子代理面板是否允许被界面打开。
///
/// # Parameters
/// 参数
/// `state` is the normalized panel state shared by WebUI and service.
/// `state` 是 WebUI 与服务共用的已归一化面板状态。
/// `has_recoverable_detail` reports whether the host can still project a transcript for this task.
/// `has_recoverable_detail` 表示宿主是否仍能为该任务投影转录详情。
///
/// # Returns
/// 返回值
/// `true` when the task is running or has recoverable transcript detail.
/// 当任务仍在运行或仍有可复看的转录详情时返回 `true`。
pub fn subagent_task_panel_is_openable(
    state: &SubagentTaskPanelState,
    has_recoverable_detail: bool,
) -> bool {
    state.run_state.is_running() || has_recoverable_detail
}

/// Returns whether the subagent navigation surface should be exposed for the current parent coordination round.
/// 返回当前父级协调轮次是否应该暴露子代理导航界面。
///
/// # Parameters
/// 参数
/// `has_visible_round` reports whether the parent session still owns a recorded navigation round.
/// `has_visible_round` 表示父会话是否仍拥有已记录的导航轮次。
/// `has_running_subagents` reports whether any direct or nested child task is still running.
/// `has_running_subagents` 表示是否仍有任意直接或嵌套子任务处于运行中。
/// `has_active_subagent_panel` reports whether the UI is currently rendering one child task surface.
/// `has_active_subagent_panel` 表示界面当前是否正在渲染某个子任务界面。
///
/// # Returns
/// 返回值
/// `true` when a UI should show the subagent switcher or keep the parent-return route available.
/// 当界面应显示子代理切换器或保留返回父会话路径时返回 `true`。
pub fn subagent_navigation_surface_is_visible(
    has_visible_round: bool,
    has_running_subagents: bool,
    has_active_subagent_panel: bool,
) -> bool {
    has_visible_round || has_running_subagents || has_active_subagent_panel
}

/// Returns whether a parent coordination round should keep its subagent navigation state alive after one child event settles.
/// 返回某个父级协调轮次在单个子事件收敛后是否仍应保留子代理导航状态。
///
/// # Parameters
/// 参数
/// `has_active_parent_run` reports whether the parent session is still executing or preparing a run.
/// `has_active_parent_run` 表示父会话是否仍在执行或准备运行。
/// `has_running_subagents` reports whether any direct or nested child task is still running.
/// `has_running_subagents` 表示是否仍有任意直接或嵌套子任务处于运行中。
/// `_has_queued_parent_work` is ignored because parent queued prompts are tracked separately.
/// `_has_queued_parent_work` 会被忽略，因为父级排队提示词已单独跟踪。
///
/// # Returns
/// 返回值
/// `true` when the current round still owns blocking coordination work and should remain visible.
/// 当当前轮次仍拥有阻塞协调工作并应继续显示时返回 `true`。
pub fn subagent_navigation_round_should_remain_visible(
    has_active_parent_run: bool,
    has_running_subagents: bool,
    _has_queued_parent_work: bool,
) -> bool {
    has_active_parent_run || has_running_subagents
}

/// Selects the subagent states that should appear in a navigation surface for the current parent.
/// 为当前父级选择应该出现在子代理导航界面中的状态列表。
///
/// # Parameters
/// 参数
/// `normalized_states` is the already refreshed and normalized child panel state list.
/// `normalized_states` 是已经刷新并归一化的子面板状态列表。
/// `requested_active_task_id` is the task that is currently opened, when any.
/// `requested_active_task_id` 是当前已打开的任务标识；没有打开项时为空。
/// `has_recoverable_detail` reports whether a non-running active task can still be reviewed.
/// `has_recoverable_detail` 表示非运行中的活动任务是否仍可复看。
///
/// # Returns
/// 返回值
/// The visible navigation states plus the still-valid active task id.
/// 返回可见导航状态列表，以及仍然有效的活动任务标识。
pub fn select_subagent_navigation_panel_states<F>(
    normalized_states: &[SubagentTaskPanelState],
    requested_active_task_id: Option<SessionId>,
    mut has_recoverable_detail: F,
) -> (Vec<SubagentTaskPanelState>, Option<SessionId>)
where
    F: FnMut(&SubagentTaskPanelState) -> bool,
{
    let mut navigation_states = normalized_states
        .iter()
        .filter(|state| state.run_state.is_running())
        .cloned()
        .collect::<Vec<_>>();

    if let Some(active_task_id) = requested_active_task_id
        && !navigation_states
            .iter()
            .any(|state| state.task_id == active_task_id)
        && let Some(active_state) = normalized_states.iter().find(|state| {
            state.task_id == active_task_id
                && subagent_task_panel_is_openable(state, has_recoverable_detail(state))
        })
    {
        navigation_states.push(active_state.clone());
    }

    let active_task_id = requested_active_task_id.filter(|task_id| {
        navigation_states
            .iter()
            .any(|state| state.task_id == *task_id)
    });
    (navigation_states, active_task_id)
}
