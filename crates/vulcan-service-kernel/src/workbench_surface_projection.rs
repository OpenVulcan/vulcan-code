//! Workbench surface and subagent navigation projection helpers.
//! 工作台界面与子代理导航投影辅助逻辑。

use super::{ServiceBridgeState, subagent_navigation_task_label};
use vulcan_service_api::{SubagentNavigationSnapshot, SubagentTaskSummary};
use vulcan_session_core::{SubagentRegistryEntry, SubagentStatus};
use vulcan_workbench_model::{SessionId, SubagentTaskPanelState, SubagentTaskRunState};
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
/// Builds the subagent navigation snapshot for the selected parent conversation.
/// 为当前选中父会话构建子代理导航快照。
///
/// This projection reads only the runtime registry for task membership; detail caches are render-only.
/// 该投影只从运行时注册表读取任务成员关系；详情缓存仅用于渲染。
pub(super) fn subagent_navigation_snapshot_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Result<Option<SubagentNavigationSnapshot>, String> {
    subagent_navigation_snapshot_from_cached_state(guard, workspace_snapshot)
}

/// Builds the subagent navigation snapshot from the current cache without refreshing segmented storage.
/// 仅从当前缓存构建子代理导航快照，不刷新分段存储。
pub(super) fn subagent_navigation_snapshot_from_cached_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Result<Option<SubagentNavigationSnapshot>, String> {
    let parent_session_id = workspace_snapshot.selected_session;
    subagent_navigation_snapshot_for_parent_from_cached_state(guard, parent_session_id, true)
}

/// Builds one subagent navigation snapshot for the requested parent conversation.
/// 为指定父会话构建一份子代理导航快照。
///
/// `use_active_panel` is true only for the currently selected parent surface.
/// `use_active_panel` 仅在当前选中的父会话界面为真。
pub(super) fn subagent_navigation_snapshot_for_parent_from_cached_state(
    guard: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    use_active_panel: bool,
) -> Result<Option<SubagentNavigationSnapshot>, String> {
    if parent_session_id == SessionId::nil() {
        return Ok(None);
    }
    let normalized_projection =
        authoritative_subagent_tree_projection_from_registry(guard, parent_session_id)?;
    // Only open or already-admitted children enter the current cohort; archived history cannot reopen it.
    // 只有开放或已经准入的子任务进入当前集合；归档历史不得因新任务而重新准入。
    // This also admits an unclosed child carried across turns before it is closed in the new turn.
    // 这也会在跨回合保留的开放子任务于新回合关闭前将其准入。
    if !normalized_projection.is_empty() {
        guard
            .session_subagent_navigation_rounds
            .entry(parent_session_id)
            .or_default()
            .extend(
                normalized_projection
                    .iter()
                    .map(|projection| projection.state.task_id),
            );
    }
    let requested_active_task_id = use_active_panel
        .then(|| valid_active_subagent_task_id_for_projection(guard, &normalized_projection))
        .flatten();
    if use_active_panel
        && guard.active_subagent_panel.is_some()
        && requested_active_task_id.is_none()
    {
        guard.active_subagent_panel = None;
    }
    if normalized_projection.is_empty() {
        return Ok(None);
    }
    let active_task_id = requested_active_task_id;
    let tasks = normalized_projection
        .iter()
        .map(|projection| {
            let mut summary = subagent_task_summary_from_state(
                &projection.state,
                active_task_id,
                projection.agent_profile_id.clone(),
            );
            if let Some(runtime) = &guard.agent_runtime {
                summary.pending_messages = runtime
                    .task_runtime
                    .pending_task_messages(projection.state.task_id)?
                    .into_iter()
                    .map(|message| vulcan_service_api::SubagentPendingMessage {
                        delivery_id: message.delivery_id,
                        message: message.message,
                        steering: message.steering,
                    })
                    .collect();
            }
            Ok(summary)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Some(SubagentNavigationSnapshot {
        parent_conversation_id: parent_session_id.to_string(),
        active_conversation_id: active_task_id.unwrap_or(parent_session_id).to_string(),
        active_task_id: active_task_id.map(|task_id| task_id.to_string()),
        tasks,
    }))
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
/// Builds panel projections from the authoritative runtime registry for every currently visible task.
/// 从权威运行时注册表为每个当前可见任务构建面板投影。
fn authoritative_subagent_tree_projection_from_registry(
    guard: &ServiceBridgeState,
    root_parent_session_id: SessionId,
) -> Result<Vec<SubagentTaskNavigationProjection>, String> {
    // admitted_task_ids belong only to the requested parent's current user turn.
    // admitted_task_ids 仅属于所请求父会话的当前用户回合。
    let admitted_task_ids = guard
        .session_subagent_navigation_rounds
        .get(&root_parent_session_id);
    let entries = if let Some(task_runtime) = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime)
    {
        task_runtime.subagent_registry_entries_for_parent_tree(
            root_parent_session_id,
            admitted_task_ids.is_some(),
        )?
    } else {
        Vec::new()
    };
    Ok(entries
        .iter()
        .filter(|entry| {
            entry.status != SubagentStatus::Archived
                || admitted_task_ids.is_some_and(|ids| ids.contains(&entry.session_id.as_uuid()))
        })
        .map(|entry| SubagentTaskNavigationProjection {
            state: subagent_task_panel_state_from_registry_entry(guard, entry),
            agent_profile_id: subagent_agent_profile_id_from_prompt_source(
                entry.prompt_source.as_deref(),
            ),
        })
        .collect::<Vec<_>>())
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
        SubagentStatus::Stopped => SubagentTaskRunState::Stopped,
        SubagentStatus::Archived => SubagentTaskRunState::Closed,
    }
}

/// Returns the active task id only when it remains present in the projected tree states.
/// 仅当活动任务仍存在于已投影树状态中时返回其标识。
fn valid_active_subagent_task_id_for_projection(
    guard: &ServiceBridgeState,
    normalized_projection: &[SubagentTaskNavigationProjection],
) -> Option<SessionId> {
    guard.active_subagent_panel.and_then(|selection| {
        normalized_projection
            .iter()
            .any(|projection| projection.state.task_id == selection.task_id)
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
        pending_messages: Vec::new(),
        task_id: state.task_id.to_string(),
        public_task_id: state.public_task_id.clone(),
        label: subagent_navigation_task_label(state),
        description: state.description.clone(),
        agent_profile_id,
        run_state: state.run_state.metadata_key().to_string(),
        is_running: state.run_state.is_running(),
        is_active: active_task_id == Some(state.task_id),
    }
}
