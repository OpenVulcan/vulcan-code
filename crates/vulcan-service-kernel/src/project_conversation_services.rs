//! Project and conversation list service handlers.
//! 项目与会话列表服务处理器。

use super::*;

mod conversation_services;
mod project_services;

pub(super) use conversation_services::{
    service_archive_conversation, service_mark_conversation_read, service_rename_conversation,
    service_select_conversation,
};
pub use project_services::service_add_project_initial_directory;
pub(super) use project_services::{
    service_archive_project_conversations, service_open_project_directory,
    service_open_project_directory_path, service_remove_project, service_rename_project,
    service_reorder_projects, service_reset_project_icon, service_set_expanded_task_projects,
    service_set_project_icon, service_toggle_project_pinned,
};

/// Normalizes and validates a user-provided display name.
/// 规范化并校验用户提供的显示名称。
pub(super) fn normalize_non_empty_name(name: &str) -> Result<String, String> {
    let normalized_name = name.trim();
    if normalized_name.is_empty() {
        return Err(String::from("名称不能为空。"));
    }
    Ok(normalized_name.to_string())
}

/// Reads the controller runtime handle from shared service state.
/// 从共享服务状态中读取控制器运行时句柄。
pub(super) fn controller_runtime_from_state(
    state: &SharedServiceBridgeState,
) -> Result<ControllerRuntimeHandle, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .controller_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; controller runtime is not ready"))
}

/// Reads the selected session identifier from the retained workspace state.
/// 从保留的工作区状态中读取当前选中会话标识。
pub(super) fn selected_session_id_from_state(
    state: &SharedServiceBridgeState,
) -> Result<SessionId, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref()
        && workspace_snapshot.selected_session != SessionId::nil()
    {
        return Ok(workspace_snapshot.selected_session);
    }
    guard
        .workbench
        .selected_conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .ok_or_else(|| String::from("当前没有选中的会话。"))
}
