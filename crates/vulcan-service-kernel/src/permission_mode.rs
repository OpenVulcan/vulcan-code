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
use vulcan_provider_config::config::ProviderConfigurationFile;
use vulcan_service_api::{
    ServiceEventEmitter, SessionPermissionModeUpdateRequest, SessionPermissionModeUpdateResponse,
    SessionUnattendedUpdateRequest, SessionUnattendedUpdateResponse, WorkbenchSnapshot,
};
use vulcan_session_core::v1::config::PermissionMode;

/// Returns the mode a new session inherits from configuration when no draft override is present.
/// 返回草稿未指定覆盖值时，新会话从 configuration 继承的权限模式。
/// A removed judge backend makes automatic approval unavailable; the saved choice is retained.
/// 判定后端被移除时自动审批不可用，已保存的选择继续保留。
pub(crate) fn effective_default_permission_mode(
    configuration: &ProviderConfigurationFile,
) -> PermissionMode {
    match configuration.default_permission_mode.unwrap_or_default() {
        PermissionMode::AutoApprove if configuration.permission_judge.backend.is_none() => {
            PermissionMode::default()
        }
        mode => mode,
    }
}

/// Saves an explicit system default for future sessions and returns the published workbench.
/// 显式保存未来新会话使用的系统默认值，返回已发布的工作台。
/// `app` publishes the update, `state` owns configuration, and `mode` is the user's choice.
/// `app` 发布更新，`state` 持有配置，`mode` 是用户的选择。
pub(crate) async fn service_save_default_permission_mode(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    mode: PermissionMode,
) -> Result<WorkbenchSnapshot, String> {
    // workbench reflects the same serialized transaction as the saved setting.
    // workbench 与已保存设置来自同一串行事务。
    let workbench = crate::mutate_provider_configuration(
        &state,
        "Failed to save the default permission mode",
        |configuration| {
            match mode {
                PermissionMode::ReadOnly => {
                    return Err(String::from("Read-only is a session-only permission mode."));
                }
                PermissionMode::AutoApprove if configuration.permission_judge.backend.is_none() => {
                    return Err(String::from(
                        "Configure a permission judge before selecting automatic approval.",
                    ));
                }
                _ => {}
            }
            configuration.default_permission_mode = Some(mode);
            Ok(())
        },
        |guard, configuration| {
            crate::refresh_settings_projection_locked(guard, configuration);
            guard.workbench.clone()
        },
    )
    .await?;
    crate::emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

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
pub(crate) async fn session_access_writer(
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

/// Save `request` through the unique session writer and return the durable mode, with the
/// unattended state that leaving auto-approve clears.
/// 通过唯一会话写入器保存 `request`，返回可靠模式以及离开自动审批时被清除的无人值守状态。
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
                let access = &document.metadata.mode.access;
                let value = (access.permission, access.unattended.clone());
                Ok(if changed {
                    SessionMutationChange::Changed(value)
                } else {
                    SessionMutationChange::Unchanged(value)
                })
            },
        )
        .map_err(|error| format!("Failed to save permission mode: {error}"))?;
    let (permission_mode, unattended) = receipt.value;
    Ok(SessionPermissionModeUpdateResponse {
        session_id: session_id.to_string(),
        revision: receipt.revision,
        permission_mode,
        unattended,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{
        provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH,
        test_support::{test_agent_runtime_bridge, test_service_state_with_parent_session},
    };
    use std::sync::Mutex;

    /// Verifies the system setting is explicit, durable, and cannot enable an unconfigured judge.
    /// 验证系统设置显式持久化，且不能启用未配置的判定器。
    /// Takes no parameters and asserts the saved snapshot and effective default.
    /// 无参数，断言已保存快照与生效默认值。
    #[tokio::test]
    async fn default_permission_mode_persists_only_available_choices() {
        // directory isolates all configuration persistence from the user's settings.
        // directory 将全部配置持久化与用户设置隔离。
        let directory = tempfile::tempdir().unwrap();
        // path is the exact settings document written by the service.
        // path 是服务写入的精确设置文档。
        let path = directory.path().join("configuration.json");
        // service holds a runtime with no configured judge.
        // service 持有尚未配置判定器的运行时。
        let mut service = test_service_state_with_parent_session(SessionId::new_v4());
        service.agent_runtime = Some(test_agent_runtime_bridge());
        // state and app own the isolated save and snapshot publication.
        // state 和 app 持有隔离的保存与快照发布。
        let state = Arc::new(Mutex::new(service));
        let app = ServiceEventEmitter::new(Vec::new());
        PROVIDER_CONFIGURATION_PATH
            .scope(path.clone(), async {
                assert!(
                    service_save_default_permission_mode(
                        app.clone(),
                        state.clone(),
                        PermissionMode::AutoApprove,
                    )
                    .await
                    .is_err()
                );
                assert!(!path.exists());
                // snapshot is the service acknowledgement of the explicitly saved full-access default.
                // snapshot 是服务对显式保存完全权限默认值的回执。
                let snapshot =
                    service_save_default_permission_mode(app, state, PermissionMode::FullAccess)
                        .await
                        .unwrap();
                assert_eq!(snapshot.default_permission_mode, PermissionMode::FullAccess);
                // saved is read through serde from the actual configuration file.
                // saved 通过 serde 从实际配置文件读取。
                let mut saved: ProviderConfigurationFile =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                assert_eq!(
                    effective_default_permission_mode(&saved),
                    PermissionMode::FullAccess
                );
                saved.default_permission_mode = Some(PermissionMode::AutoApprove);
                assert_eq!(
                    effective_default_permission_mode(&saved),
                    PermissionMode::WorkspaceWrite
                );
                assert_eq!(
                    saved.default_permission_mode,
                    Some(PermissionMode::AutoApprove)
                );
                saved.permission_judge.backend = Some(vulcan_service_api::JudgeBackend::Local);
                assert_eq!(
                    effective_default_permission_mode(&saved),
                    PermissionMode::AutoApprove
                );
            })
            .await;
    }
}
