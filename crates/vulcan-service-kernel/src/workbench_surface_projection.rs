//! Workbench surface and subagent navigation projection helpers.
//! 工作台界面与子代理导航投影辅助逻辑。

use super::{
    ServiceBridgeState, conversation_state_to_session_messages,
    display_session_messages_from_workspace, normalize_subagent_task_panel_state,
    normalize_webui_subagent_task_state, select_subagent_navigation_panel_states,
    subagent_navigation_surface_is_visible, subagent_navigation_task_label,
    subagent_task_panel_is_openable, subagent_task_panel_state_has_recoverable_conversation,
    webui_subagent_task_has_live_run, webui_subagent_task_state_by_task_id,
};
use vulcan_service_api::{SubagentNavigationSnapshot, SubagentTaskSummary};
use vulcan_workbench_model::{
    SessionId, SessionMessageEntry, SubagentTaskPanelState, SubagentTaskRunState,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

/// Returns the currently active transcript surface session id for the WebUI.
/// 返回 WebUI 当前活动转录界面的会话标识。
pub(super) fn active_surface_session_id_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> SessionId {
    let root_session_id = workspace_snapshot.selected_session;
    if guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id != root_session_id)
    {
        guard.active_subagent_panel = None;
    }
    let has_visible_round = guard
        .session_subagent_navigation_rounds
        .contains(&root_session_id);
    let has_active_panel = guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id == root_session_id);
    if !has_visible_round && !has_active_panel {
        return root_session_id;
    }
    if let Some(selection) = guard.active_subagent_panel
        && let Some(state) = guard
            .session_subagent_tasks
            .get(&selection.parent_session_id)
            .and_then(|tasks| tasks.get(&selection.task_id))
    {
        let task_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| &runtime.task_runtime);
        let has_live_listener = guard
            .detached_subagent_task_listeners
            .contains(&state.task_id);
        let normalized = normalize_webui_subagent_task_state(
            Some(workspace_snapshot),
            state,
            task_runtime,
            has_live_listener,
        );
        if subagent_task_panel_is_openable(
            &normalized,
            webui_subagent_panel_has_recoverable_detail(workspace_snapshot, &normalized),
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
        && let Some(state) = webui_subagent_task_state_by_task_id(guard, active_session_id)
        && let Some(conversation_state) = state.conversation_state.as_ref()
    {
        return conversation_state_to_session_messages(conversation_state);
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
        && let Some(state) = webui_subagent_task_state_by_task_id(guard, active_session_id)
        && let Some(conversation_state) = state.conversation_state.as_ref()
    {
        return conversation_state_to_session_messages(conversation_state);
    }
    display_session_messages_from_workspace(workspace_snapshot, active_session_id)
        .unwrap_or_default()
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
pub(super) fn subagent_navigation_snapshot_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    let root_session_id = workspace_snapshot.selected_session;
    if root_session_id == SessionId::nil() {
        return None;
    }
    if guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id != root_session_id)
    {
        guard.active_subagent_panel = None;
    }
    let parent_session_id = root_session_id;
    let has_visible_round = guard
        .session_subagent_navigation_rounds
        .contains(&parent_session_id);
    let has_active_panel = guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id == parent_session_id);
    if !has_visible_round && !has_active_panel {
        return None;
    }
    let requested_active_task_id = guard.active_subagent_panel.and_then(|selection| {
        (selection.parent_session_id == parent_session_id).then_some(selection.task_id)
    });
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
    let normalized_states = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .map(|entries| {
            entries
                .values()
                .map(|state| {
                    normalize_webui_subagent_task_state(
                        Some(workspace_snapshot),
                        state,
                        task_runtime,
                        live_listener_task_ids.contains(&state.task_id),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let has_running_subagents = normalized_states
        .iter()
        .any(|state| state.run_state.is_running());
    if !subagent_navigation_surface_is_visible(
        has_visible_round,
        has_running_subagents,
        has_active_panel,
    ) {
        return None;
    }
    let (navigation_states, active_task_id) = select_subagent_navigation_panel_states(
        &normalized_states,
        requested_active_task_id,
        |state| webui_subagent_panel_has_recoverable_detail(workspace_snapshot, state),
    );
    if navigation_states.is_empty() {
        return None;
    }
    let tasks = navigation_states
        .iter()
        .map(|state| subagent_task_summary_from_state(state, active_task_id))
        .collect::<Vec<_>>();
    let active_conversation_id = active_task_id.unwrap_or(parent_session_id).to_string();
    Some(SubagentNavigationSnapshot {
        parent_conversation_id: parent_session_id.to_string(),
        active_conversation_id,
        active_task_id: active_task_id.map(|task_id| task_id.to_string()),
        tasks,
    })
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
    if guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id != parent_session_id)
    {
        guard.active_subagent_panel = None;
    }
    let has_visible_round = guard
        .session_subagent_navigation_rounds
        .contains(&parent_session_id);
    let has_active_panel = guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id == parent_session_id);
    if !has_visible_round && !has_active_panel {
        return None;
    }
    let requested_active_task_id = guard.active_subagent_panel.and_then(|selection| {
        (selection.parent_session_id == parent_session_id).then_some(selection.task_id)
    });
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
    let normalized_states = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .map(|entries| {
            entries
                .values()
                .map(|state| {
                    let is_live_running = webui_subagent_task_has_live_run(
                        task_runtime,
                        live_listener_task_ids.contains(&state.task_id),
                        state.task_id,
                    );
                    normalize_subagent_task_panel_state(state, is_live_running, None)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let has_running_subagents = normalized_states
        .iter()
        .any(|state| state.run_state.is_running());
    if !subagent_navigation_surface_is_visible(
        has_visible_round,
        has_running_subagents,
        has_active_panel,
    ) {
        return None;
    }
    let (navigation_states, active_task_id) = select_subagent_navigation_panel_states(
        &normalized_states,
        requested_active_task_id,
        |state| webui_subagent_panel_has_recoverable_detail(workspace_snapshot, state),
    );
    if navigation_states.is_empty() {
        return None;
    }
    let tasks = navigation_states
        .iter()
        .map(|state| subagent_task_summary_from_state(state, active_task_id))
        .collect::<Vec<_>>();
    Some(SubagentNavigationSnapshot {
        parent_conversation_id: parent_session_id.to_string(),
        active_conversation_id: active_task_id.unwrap_or(parent_session_id).to_string(),
        active_task_id: active_task_id.map(|task_id| task_id.to_string()),
        tasks,
    })
}

/// Converts one shared subagent panel state into a service API summary.
/// 将一条共享子代理面板状态转换为 service API 摘要。
fn subagent_task_summary_from_state(
    state: &SubagentTaskPanelState,
    active_task_id: Option<SessionId>,
) -> SubagentTaskSummary {
    SubagentTaskSummary {
        task_id: state.task_id.to_string(),
        public_task_id: state.public_task_id.clone(),
        label: subagent_navigation_task_label(state),
        description: state.description.clone(),
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
