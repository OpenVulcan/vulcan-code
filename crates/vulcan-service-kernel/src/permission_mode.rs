//! Session permission mode: durable metadata admission without a model turn.
//! 会话权限模式：不启动模型回合的可靠元数据准入。

use crate::current_v1_unix_millis;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionLifecycleTransitionGuard, SessionMutationAdmission,
    SessionMutationChange,
};
use crate::{SharedServiceBridgeState, parse_session_id};
use std::sync::Arc;
use vulcan_agent_runtime::model::SessionId;
use vulcan_service_api::{
    SessionPermissionModeUpdateRequest, SessionPermissionModeUpdateResponse,
    SessionUnattendedUpdateRequest, SessionUnattendedUpdateResponse,
};
use vulcan_session_core::v1::config::PermissionMode;

/// Durably records the draft's permission mode when a new session materializes.
/// 新会话物化时可靠记录草稿的权限模式。
/// `handle` owns the V1 document of the session being materialized.
/// `handle` 持有正在物化会话的 V1 文档。
pub(crate) fn set_session_permission_mode_v1(
    handle: &SessionHandle,
    mode: PermissionMode,
    now_ms: i64,
) -> Result<(), String> {
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                Ok(if document.set_permission_mode(mode, now_ms)? {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .map_err(|error| format!("Failed to save permission mode: {error}"))?;
    Ok(())
}

/// Resolves the writable handle of one active regular session while holding its lifecycle
/// transition, so access changes serialize with archive and delete.
/// 在持有生命周期转换的同时解析一个活动普通会话的可写句柄，使授权修改与归档、删除串行。
async fn session_access_writer(
    state: &SharedServiceBridgeState,
    raw_session_id: &str,
) -> Result<
    (
        SessionId,
        SessionLifecycleTransitionGuard,
        Arc<SessionHandle>,
    ),
    String,
> {
    let session_id = parse_session_id(raw_session_id)
        .ok_or_else(|| String::from("Invalid session identity."))?;
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
                "Session access settings require an active regular session.",
            ));
        }
        Arc::clone(&guard.session_runtime_registry)
    };
    let lifecycle = registry
        .acquire_lifecycle_transition(session_id)
        .await
        .map_err(|error| error.to_string())?;
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(state, session_id).await?;
    Ok((session_id, lifecycle, handle))
}

/// Save `request` through the unique session writer and return the durable mode.
/// 通过唯一会话写入器保存 `request`，返回可靠模式。
/// An active API keeps running; its next tool call reads the saved mode.
/// 活动 API 继续运行；其下一次工具调用读取已保存的模式。
pub(crate) async fn service_set_session_permission_mode(
    state: SharedServiceBridgeState,
    request: SessionPermissionModeUpdateRequest,
) -> Result<SessionPermissionModeUpdateResponse, String> {
    let (session_id, _lifecycle, handle) =
        session_access_writer(&state, &request.session_id).await?;
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

/// Turns unattended mode on with the deny policy the user confirmed, or off; returns the durable
/// state. An active API keeps running; its next tool call reads the saved state.
/// 以用户确认的拒绝策略开启或关闭无人值守模式；返回可靠状态。活动 API 继续运行；其下一次工具调用
/// 读取已保存的状态。
pub(crate) async fn service_set_session_unattended(
    state: SharedServiceBridgeState,
    request: SessionUnattendedUpdateRequest,
) -> Result<SessionUnattendedUpdateResponse, String> {
    let (session_id, _lifecycle, handle) =
        session_access_writer(&state, &request.session_id).await?;
    let now_ms = current_v1_unix_millis()?;
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let changed = document.set_unattended(request.unattended, now_ms)?;
                let unattended = document.metadata.mode.access.unattended.clone();
                Ok(if changed {
                    SessionMutationChange::Changed(unattended)
                } else {
                    SessionMutationChange::Unchanged(unattended)
                })
            },
        )
        .map_err(|error| format!("Failed to save unattended mode: {error}"))?;
    Ok(SessionUnattendedUpdateResponse {
        session_id: session_id.to_string(),
        revision: receipt.revision,
        unattended: receipt.value,
    })
}
