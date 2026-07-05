//! Workbench surface and subagent navigation projection helpers.
//! 工作台界面与子代理导航投影辅助逻辑。

use std::collections::BTreeMap;

use super::{
    ServiceBridgeState, conversation_state_to_session_messages,
    display_session_messages_from_workspace, filter_display_session_messages,
    select_subagent_navigation_panel_states, subagent_navigation_surface_is_visible,
    subagent_navigation_task_label, subagent_task_panel_is_openable,
    subagent_task_panel_state_has_recoverable_conversation, webui_trace,
};
use vulcan_service_api::{SubagentNavigationSnapshot, SubagentTaskSummary};
use vulcan_session_core::{SubagentRegistryEntry, SubagentStatus};
use vulcan_workbench_model::{
    SessionId, SessionMessageEntry, SubagentTaskPanelState, SubagentTaskRunState,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

/// Combined subagent navigation row projected from the authoritative runtime registry.
/// 从权威运行时注册表投影出的组合智能体导航行。
#[derive(Clone)]
struct SubagentTaskNavigationProjection {
    /// Renderable state reused by the existing panel navigation selector.
    /// 复用现有面板导航筛选器的可渲染状态。
    state: SubagentTaskPanelState,
    /// Optional catalog agent identifier derived only from agent.md prompt sources.
    /// 仅从 agent.md 提示词来源派生的可选目录智能体标识。
    agent_profile_id: Option<String>,
}

/// Returns the currently active transcript surface session id for the WebUI.
/// 返回 WebUI 当前活动转录界面的会话标识。
pub(super) fn active_surface_session_id_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> SessionId {
    let root_session_id = workspace_snapshot.selected_session;
    let has_visible_round = guard
        .session_subagent_navigation_rounds
        .contains(&root_session_id);
    let normalized_states =
        authoritative_subagent_tree_states_from_registry(guard, root_session_id);
    let active_panel_belongs_to_root = guard.active_subagent_panel.is_some_and(|selection| {
        normalized_states
            .iter()
            .any(|state| state.task_id == selection.task_id)
    });
    if guard.active_subagent_panel.is_some() && !active_panel_belongs_to_root {
        guard.active_subagent_panel = None;
    }
    if !has_visible_round && !active_panel_belongs_to_root {
        return root_session_id;
    }
    if let Some(selection) = guard.active_subagent_panel
        && active_panel_belongs_to_root
        && let Some(normalized) = normalized_states
            .iter()
            .find(|state| state.task_id == selection.task_id)
    {
        if subagent_task_panel_is_openable(
            normalized,
            webui_subagent_panel_has_recoverable_detail(workspace_snapshot, normalized),
        ) {
            return selection.task_id;
        }
    }
    guard.active_subagent_panel = None;
    root_session_id
}

/// Returns transcript messages for the currently active WebUI surface.
/// 返回 WebUI 当前活动界面的转录消息。
pub(super) fn active_surface_messages_from_workspace_and_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Vec<SessionMessageEntry> {
    let active_session_id = active_surface_session_id_from_state(guard, workspace_snapshot);
    if active_session_id != workspace_snapshot.selected_session
        && let Some(messages) =
            display_session_messages_from_workspace(workspace_snapshot, active_session_id)
        && !messages.is_empty()
    {
        return messages;
    }
    if active_session_id != workspace_snapshot.selected_session
        && let Some(state) = webui_subagent_detail_state_by_task_id(guard, active_session_id)
        && let Some(conversation_state) = state.conversation_state.as_ref()
    {
        return filter_display_session_messages(conversation_state_to_session_messages(
            conversation_state,
        ));
    }
    display_session_messages_from_workspace(workspace_snapshot, active_session_id)
        .unwrap_or_default()
}

/// Returns the active surface id from preserved navigation without recomputing task lists.
/// 不重算任务列表，直接从已保留导航中返回活动界面标识。
pub(super) fn preserved_active_surface_session_id(
    workspace_snapshot: &WorkspaceSnapshot,
    subagent_navigation: Option<&SubagentNavigationSnapshot>,
) -> SessionId {
    subagent_navigation
        .and_then(|navigation| navigation.active_conversation_id.parse::<SessionId>().ok())
        .unwrap_or(workspace_snapshot.selected_session)
}

/// Returns messages for one active surface without refreshing subagent navigation.
/// 不刷新子代理导航，返回指定活动界面的消息。
pub(super) fn surface_messages_for_session_without_subagent_navigation_refresh(
    guard: &ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    active_session_id: SessionId,
) -> Vec<SessionMessageEntry> {
    if active_session_id != workspace_snapshot.selected_session
        && let Some(messages) =
            display_session_messages_from_workspace(workspace_snapshot, active_session_id)
        && !messages.is_empty()
    {
        return messages;
    }
    if active_session_id != workspace_snapshot.selected_session
        && let Some(state) = webui_subagent_detail_state_by_task_id(guard, active_session_id)
        && let Some(conversation_state) = state.conversation_state.as_ref()
    {
        return filter_display_session_messages(conversation_state_to_session_messages(
            conversation_state,
        ));
    }
    display_session_messages_from_workspace(workspace_snapshot, active_session_id)
        .unwrap_or_default()
}

/// Returns cached render detail for one subagent task without affecting membership.
/// 返回单个子代理任务的渲染详情缓存，不影响成员关系判断。
fn webui_subagent_detail_state_by_task_id(
    guard: &ServiceBridgeState,
    task_id: SessionId,
) -> Option<&SubagentTaskPanelState> {
    guard
        .subagent_surface_detail_cache
        .get(&task_id)
        .map(|entry| &entry.task_state)
}

/// Returns whether one WebUI subagent panel has enough persisted transcript detail for review.
/// 返回 WebUI 子代理面板是否拥有足够的持久转录详情可供复看。
pub(super) fn webui_subagent_panel_has_recoverable_detail(
    workspace_snapshot: &WorkspaceSnapshot,
    state: &SubagentTaskPanelState,
) -> bool {
    display_session_messages_from_workspace(workspace_snapshot, state.task_id)
        .is_some_and(|messages| !messages.is_empty())
        || subagent_task_panel_state_has_recoverable_conversation(state)
}

/// Builds the subagent navigation snapshot for the selected parent conversation.
/// 为当前选中父会话构建子代理导航快照。
///
/// This projection reads only the runtime registry for task membership; detail caches are render-only.
/// 该投影只从运行时注册表读取任务成员关系；详情缓存仅用于渲染。
pub(super) fn subagent_navigation_snapshot_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    subagent_navigation_snapshot_from_cached_state(guard, workspace_snapshot)
}

/// Builds the subagent navigation snapshot from the current cache without refreshing segmented storage.
/// 仅从当前缓存构建子代理导航快照，不刷新分段存储。
pub(super) fn subagent_navigation_snapshot_from_cached_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    let parent_session_id = workspace_snapshot.selected_session;
    if parent_session_id == SessionId::nil() {
        return None;
    }
    let has_visible_round = guard
        .session_subagent_navigation_rounds
        .contains(&parent_session_id);
    let normalized_projection =
        authoritative_subagent_tree_projection_from_registry(guard, parent_session_id);
    let normalized_states = normalized_projection
        .iter()
        .map(|projection| projection.state.clone())
        .collect::<Vec<_>>();
    let requested_active_task_id =
        valid_active_subagent_task_id_for_states(guard, &normalized_states);
    if guard.active_subagent_panel.is_some() && requested_active_task_id.is_none() {
        guard.active_subagent_panel = None;
    }
    let has_active_panel = requested_active_task_id.is_some();
    let has_unclosed_subagents = !normalized_states.is_empty();
    if !subagent_navigation_surface_is_visible(
        has_visible_round,
        has_unclosed_subagents,
        has_active_panel,
    ) {
        return None;
    }
    let (navigation_states, active_task_id) =
        select_subagent_navigation_panel_states(normalized_states, requested_active_task_id);
    if navigation_states.is_empty() {
        return None;
    }
    let agent_profile_ids = normalized_projection
        .into_iter()
        .filter_map(|projection| {
            projection
                .agent_profile_id
                .map(|agent_profile_id| (projection.state.task_id, agent_profile_id))
        })
        .collect::<BTreeMap<_, _>>();
    let tasks = navigation_states
        .iter()
        .map(|state| {
            subagent_task_summary_from_state(
                state,
                active_task_id,
                agent_profile_ids.get(&state.task_id).cloned(),
            )
        })
        .collect::<Vec<_>>();
    Some(SubagentNavigationSnapshot {
        parent_conversation_id: parent_session_id.to_string(),
        active_conversation_id: active_task_id.unwrap_or(parent_session_id).to_string(),
        active_task_id: active_task_id.map(|task_id| task_id.to_string()),
        tasks,
    })
}

/// Extracts the catalog agent id from an agent.md prompt source.
/// 从 agent.md 提示词来源中提取目录智能体标识。
fn subagent_agent_profile_id_from_prompt_source(prompt_source: Option<&str>) -> Option<String> {
    let prompt_source = prompt_source?.trim();
    let agent_profile_id = prompt_source.strip_prefix("agent_md:")?.trim();
    (!agent_profile_id.is_empty()).then(|| agent_profile_id.to_string())
}

/// Returns one authoritative open registry entry for a child task in the selected tree.
/// 返回当前选中树中单个子任务的权威未关闭注册表条目。
///
/// # Parameters
/// 参数
/// `guard` owns the shared agent runtime handle.
/// `guard` 持有共享代理运行时句柄。
/// `root_parent_session_id` is the selected root or direct parent session.
/// `root_parent_session_id` 是选中根会话或直接父会话。
/// `task_id` is the child task session id requested by WebUI.
/// `task_id` 是 WebUI 请求的子任务会话标识。
///
/// # Returns
/// 返回值
/// `Some` when the runtime registry says the task is still open and visible; otherwise `None`.
/// 当运行时注册表确认该任务仍未关闭且可见时返回 `Some`；否则返回 `None`。
pub(super) fn open_subagent_registry_entry_for_task(
    guard: &ServiceBridgeState,
    root_parent_session_id: SessionId,
    task_id: SessionId,
) -> Result<Option<SubagentRegistryEntry>, String> {
    let Some(task_runtime) = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime)
    else {
        return Ok(None);
    };
    let entries =
        task_runtime.open_subagent_registry_entries_for_parent_tree(root_parent_session_id)?;
    Ok(entries
        .into_iter()
        .find(|entry| entry.session_id.as_uuid() == task_id))
}

/// Builds panel states from the authoritative runtime registry for every open task in the selected tree.
/// 从权威运行时注册表为当前选中树中的每个未关闭任务构建面板状态。
fn authoritative_subagent_tree_states_from_registry(
    guard: &ServiceBridgeState,
    root_parent_session_id: SessionId,
) -> Vec<SubagentTaskPanelState> {
    authoritative_subagent_tree_projection_from_registry(guard, root_parent_session_id)
        .into_iter()
        .map(|projection| projection.state)
        .collect()
}

/// Builds panel projections from the authoritative runtime registry for every open task.
/// 从权威运行时注册表为每个未关闭任务构建面板投影。
fn authoritative_subagent_tree_projection_from_registry(
    guard: &ServiceBridgeState,
    root_parent_session_id: SessionId,
) -> Vec<SubagentTaskNavigationProjection> {
    let Some(task_runtime) = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime)
    else {
        return Vec::new();
    };
    let entries =
        match task_runtime.open_subagent_registry_entries_for_parent_tree(root_parent_session_id) {
            Ok(entries) => entries,
            Err(error) => {
                webui_trace(
                    "subagent_navigation_registry_unavailable",
                    format!("parent={root_parent_session_id} error={error}"),
                );
                Vec::new()
            }
        };
    entries
        .iter()
        .map(|entry| SubagentTaskNavigationProjection {
            state: subagent_task_panel_state_from_registry_entry(guard, entry),
            agent_profile_id: subagent_agent_profile_id_from_prompt_source(
                entry.prompt_source.as_deref(),
            ),
        })
        .collect()
}

/// Converts one authoritative registry entry into a renderable WebUI panel state.
/// 将单个权威注册表条目转换为可渲染的 WebUI 面板状态。
fn subagent_task_panel_state_from_registry_entry(
    guard: &ServiceBridgeState,
    entry: &SubagentRegistryEntry,
) -> SubagentTaskPanelState {
    let task_id = entry.session_id.as_uuid();
    let cached_state = guard
        .subagent_surface_detail_cache
        .get(&task_id)
        .map(|cached| &cached.task_state);
    SubagentTaskPanelState {
        task_id,
        public_task_id: entry.agent_id.clone(),
        parent_session_id: entry.parent_session_id.as_uuid(),
        subagent_label: entry.agent_id.clone(),
        description: entry
            .last_task_message
            .clone()
            .or_else(|| cached_state.and_then(|state| state.description.clone())),
        run_state: subagent_run_state_from_registry_status(entry.status),
        result_text: entry
            .result_summary
            .clone()
            .or_else(|| cached_state.map(|state| state.result_text.clone()))
            .unwrap_or_default(),
        conversation_state: cached_state.and_then(|state| state.conversation_state.clone()),
    }
}

/// Converts the authoritative registry status into the WebUI panel run state.
/// 将权威注册表状态转换为 WebUI 面板运行状态。
fn subagent_run_state_from_registry_status(status: SubagentStatus) -> SubagentTaskRunState {
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

/// Returns the active task id only when it remains present in the projected tree states.
/// 仅当活动任务仍存在于已投影树状态中时返回其标识。
fn valid_active_subagent_task_id_for_states(
    guard: &ServiceBridgeState,
    normalized_states: &[SubagentTaskPanelState],
) -> Option<SessionId> {
    guard.active_subagent_panel.and_then(|selection| {
        normalized_states
            .iter()
            .any(|state| state.task_id == selection.task_id)
            .then_some(selection.task_id)
    })
}

/// Converts one shared subagent panel state into a service API summary.
/// 将一条共享子代理面板状态转换为 service API 摘要。
fn subagent_task_summary_from_state(
    state: &SubagentTaskPanelState,
    active_task_id: Option<SessionId>,
    agent_profile_id: Option<String>,
) -> SubagentTaskSummary {
    SubagentTaskSummary {
        task_id: state.task_id.to_string(),
        public_task_id: state.public_task_id.clone(),
        label: subagent_navigation_task_label(state),
        description: state.description.clone(),
        agent_profile_id,
        run_state: subagent_run_state_key(state.run_state).to_string(),
        is_running: state.run_state.is_running(),
        is_active: active_task_id == Some(state.task_id),
    }
}

/// Returns the service API run-state key for one shared subagent state.
/// 返回单个共享子代理状态对应的 service API 运行状态键。
pub(super) fn subagent_run_state_key(run_state: SubagentTaskRunState) -> &'static str {
    match run_state {
        SubagentTaskRunState::Running => "running",
        SubagentTaskRunState::Completed => "completed",
        SubagentTaskRunState::Stopped => "stopped",
        SubagentTaskRunState::Failed => "failed",
    }
}
