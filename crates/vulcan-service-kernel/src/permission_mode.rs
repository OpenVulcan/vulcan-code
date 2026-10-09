//! Session permission mode: durable metadata admission without a model turn.
//! 会话权限模式：不启动模型回合的可靠元数据准入。

use crate::current_v1_unix_millis;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};
use crate::{SharedServiceBridgeState, parse_session_id};
use vulcan_service_api::{SessionPermissionModeUpdateRequest, SessionPermissionModeUpdateResponse};

/// Save `request` through the unique session writer and return the durable mode.
/// 通过唯一会话写入器保存 `request`，返回可靠模式。
/// An active API keeps running; its next tool call reads the saved mode.
/// 活动 API 继续运行；其下一次工具调用读取已保存的模式。
pub(crate) async fn service_set_session_permission_mode(
    state: SharedServiceBridgeState,
    request: SessionPermissionModeUpdateRequest,
) -> Result<SessionPermissionModeUpdateResponse, String> {
    let session_id = parse_session_id(&request.session_id)
        .ok_or_else(|| String::from("Invalid permission mode session identity."))?;
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("Workspace is not ready."))?;
        if !crate::workspace_snapshot_contains_regular_session(workspace, session_id) {
            return Err(String::from(
                "Permission mode settings require an active regular session.",
            ));
        }
        std::sync::Arc::clone(&guard.session_runtime_registry)
    };
    let _lifecycle = registry
        .acquire_lifecycle_transition(session_id)
        .await
        .map_err(|error| error.to_string())?;
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, session_id).await?;
    let now_ms = current_v1_unix_millis()?;
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                let changed = document.set_permission_mode(request.mode, now_ms)?;
                let mode = document.metadata.mode.access.permission;
                Ok(if changed {
                    SessionMutationChange::Changed(mode)
                } else {
                    SessionMutationChange::Unchanged(mode)
                })
            },
        )
        .map_err(|error| format!("Failed to save permission mode: {error}"))?;
    Ok(SessionPermissionModeUpdateResponse {
        session_id: session_id.to_string(),
        revision: receipt.revision,
        permission_mode: receipt.value,
    })
}
