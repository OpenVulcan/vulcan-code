//! WebUI pending prompt queue RPC service handlers.
//! WebUI 待发送提示词队列 RPC 服务处理器。

use vulcan_service_api::{PendingPromptSummary, ServiceEventEmitter};
use vulcan_workbench_model::{
    PendingPromptPanelDropTarget, apply_pending_prompt_panel_drop_target,
    remove_pending_prompt_panel_entry,
};

use super::pending_prompts::pending_prompt_summary_from_panel_entry;
use super::{
    SharedServiceBridgeState, current_unix_millis_text, current_workspace_snapshot_from_state,
    emit_pending_prompts_changed_delta, parse_session_id, runtime_parts_from_state,
    selected_session_id_from_state, spawn_workspace_session_persist_task,
    store_workspace_snapshot_without_workbench_rebuild,
};

/// Restores one visible pending prompt into the WebUI composer and removes it from the queue.
/// 将单条可见待发送提示词恢复到 WebUI 输入区，并从队列中移除。
///
/// # Parameters
/// 参数
/// `app` emits the updated workbench snapshot.
/// `app` 发出更新后的工作台快照。
/// `state` owns retained service runtime and workspace state.
/// `state` 持有保留的服务运行时与工作区状态。
/// `entry_id` selects the pending prompt panel row to edit.
/// `entry_id` 选择要编辑的待发送提示词面板行。
///
/// # Returns
/// 返回值
/// The removed prompt summary for restoring into the composer.
/// 返回用于恢复到输入区的已移除提示词摘要。
pub(super) async fn service_edit_pending_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    entry_id: String,
) -> Result<PendingPromptSummary, String> {
    let (controller_runtime, _, _, _, _, _) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let summary = {
        let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get_mut(&selected_session_id)
        else {
            return Err(String::from("当前会话没有可编辑的待发送队列。"));
        };
        let Some(entry) = remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, &entry_id)
        else {
            return Err(String::from("没有找到待编辑的待发送提示词。"));
        };
        runtime_state.updated_at = current_unix_millis_text();
        pending_prompt_summary_from_panel_entry(&entry)
    };
    store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot)?;
    emit_pending_prompts_changed_delta(&app, &state, selected_session_id)?;
    let snapshot_for_persist = current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    spawn_workspace_session_persist_task(
        state.clone(),
        controller_runtime.clone(),
        snapshot_for_persist,
        selected_session_id,
        Some(String::from("pending_edit")),
    );
    Ok(summary)
}

/// Removes one visible pending prompt from the selected conversation queue.
/// 从当前选中会话队列中移除单条可见待发送提示词。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns retained service runtime and workspace state.
/// `state` 持有保留的服务运行时与工作区状态。
/// `entry_id` selects the pending prompt panel row to remove.
/// `entry_id` 选择要移除的待发送提示词面板行。
///
/// # Returns
/// 返回值
/// The updated workbench snapshot after removal.
/// 返回移除后的更新工作台快照。
pub(super) async fn service_remove_pending_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    entry_id: String,
) -> Result<bool, String> {
    let (controller_runtime, _, _, _, _, _) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let removed = if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&selected_session_id)
    {
        if remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, &entry_id).is_some() {
            runtime_state.updated_at = current_unix_millis_text();
            true
        } else {
            false
        }
    } else {
        false
    };
    store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot)?;
    emit_pending_prompts_changed_delta(&app, &state, selected_session_id)?;
    if removed {
        let snapshot_for_persist =
            current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
        spawn_workspace_session_persist_task(
            state.clone(),
            controller_runtime.clone(),
            snapshot_for_persist,
            selected_session_id,
            Some(String::from("pending_remove")),
        );
    }
    Ok(removed)
}

/// Reorders the visible pending prompt queue for the selected conversation.
/// 重排当前选中会话的可见待发送提示词队列。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns retained service runtime and workspace state.
/// `state` 持有保留的服务运行时与工作区状态。
/// `conversation_id` optionally selects the conversation queue to reorder.
/// `conversation_id` 可选指定要重排的会话队列。
/// `dragged_entry_id` selects the moved pending prompt row.
/// `dragged_entry_id` 选择被移动的待发送提示词行。
/// `target_entry_id` selects the row before which the dragged row should be placed.
/// `target_entry_id` 选择被移动行应放置到其前方的目标行。
///
/// # Returns
/// 返回值
/// The updated workbench snapshot after reordering.
/// 返回重排后的更新工作台快照。
pub(super) async fn service_reorder_pending_prompts(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: Option<String>,
    dragged_entry_id: String,
    target_entry_id: Option<String>,
) -> Result<bool, String> {
    let (controller_runtime, _, _, _, _, _) = runtime_parts_from_state(&state)?;
    let selected_session_id = conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .map(Ok)
        .unwrap_or_else(|| selected_session_id_from_state(&state))?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    workspace_snapshot.selected_session = selected_session_id;
    let drop_target = target_entry_id
        .filter(|entry_id| !entry_id.trim().is_empty())
        .map(PendingPromptPanelDropTarget::Before)
        .unwrap_or(PendingPromptPanelDropTarget::End);
    let did_reorder = if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&selected_session_id)
    {
        if apply_pending_prompt_panel_drop_target(
            &mut runtime_state.ui_state,
            &dragged_entry_id,
            &drop_target,
        ) {
            runtime_state.updated_at = current_unix_millis_text();
            true
        } else {
            false
        }
    } else {
        false
    };
    store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot)?;
    emit_pending_prompts_changed_delta(&app, &state, selected_session_id)?;
    if did_reorder {
        let snapshot_for_persist =
            current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
        spawn_workspace_session_persist_task(
            state.clone(),
            controller_runtime.clone(),
            snapshot_for_persist,
            selected_session_id,
            Some(String::from("pending_reorder")),
        );
    }
    Ok(did_reorder)
}
