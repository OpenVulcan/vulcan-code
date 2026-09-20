//! WebUI prompt lifecycle pending dispatch helpers.
//! WebUI 提示词生命周期 pending dispatch 辅助逻辑。

use super::*;
use crate::notify_webui_wait_agent_parent_interruption_if_waiting;
use crate::pending_prompt_services::{
    pending_prompt_action_id, pending_prompt_attachment_workspace_path,
    pending_prompt_queue_identity,
    reconcile_pending_prompt_attachment_leases,
    rebuild_pending_prompt_panel_from_v1,
};
use crate::session_goal_lifecycle::{current_v1_goal_unix_millis, session_v1_handle_from_state};
use vulcan_workspace_store::workspace_store::load_project_resource_catalog;

/// Inserts one visible pending prompt and starts dispatch when the selected session can run.
/// 插入单条可见待发送提示词，并在当前会话可运行时启动分发。
///
/// # Parameters
/// 参数
/// `app` emits pending-prompt and dispatch deltas.
/// `app` 发出待发送队列与分发增量。
/// `state` owns retained service state and runtime handles.
/// `state` 持有保留的服务状态与运行时句柄。
/// `entry_id` selects the visible queue row to insert.
/// `entry_id` 选择要插入的可见队列行。
///
/// # Returns
/// 返回值
/// `true` when the prompt was accepted for insertion.
/// 当提示词已被接受插入时返回 `true`。
pub(crate) async fn service_insert_pending_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    entry_id: String,
) -> Result<bool, String> {
    let selected_session_id = selected_session_id_from_state(&state)?;
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        notify_webui_wait_agent_parent_interruption_if_waiting(
            &guard,
            selected_session_id,
            "user_interrupt_prompt",
        )?;
    }
    let client_action_id = pending_prompt_action_id(&entry_id)?;
    let handle = session_v1_handle_from_state(&state, selected_session_id).await?;
    let identity = pending_prompt_queue_identity(handle.as_ref(), &client_action_id)?;
    let attachment_workspace_path =
        pending_prompt_attachment_workspace_path(&state, selected_session_id)?;
    handle
        .promote_queued_input_to_pending_turn(identity, current_v1_goal_unix_millis()?)
        .map_err(|error| format!("把 V1 待发送消息插入当前回合失败：{error}"))?;
    // The V1 promotion is already durable; later panel failures cannot retain its queue lease.
    // V1 提升已可靠提交；后续面板故障不能保留该队列租约。
    reconcile_pending_prompt_attachment_leases(
        handle.as_ref(),
        selected_session_id,
        &attachment_workspace_path,
    )
    .map_err(|error| format!("待发送消息已提升到 V1 当前回合，但附件清理待重试：{error}"))?;
    let locator = handle
        .storage_locator()
        .map_err(|error| format!("读取 V1 会话资源定位失败：{error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("加载 V1 项目资源目录失败：{error}"))?;
    rebuild_pending_prompt_panel_from_v1(
        &state,
        selected_session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    )?;
    emit_pending_prompts_changed_delta(&app, &state, selected_session_id)?;
    Ok(true)
}
