//! Single-owner startup repair that retains transient failures without retaining the service forever.
//! 单一所有者启动修复，保留瞬态失败而不永久持有服务。

use super::*;
use tokio::time::Duration;

/// One pass can finish, defer for host shutdown, or retire after its registry was replaced.
/// 一轮扫描可以完成、因宿主关闭延后，或在注册表被替换后退出。
enum PassDisposition {
    Complete,
    Suspended,
    Retired,
}

/// Checks the current service owner before starting another independently recoverable node.
/// 开始另一个可独立恢复节点前检查当前服务所有者。
/// `registry` identifies the original worker; returns an explicit error on replacement or closed admission.
/// `registry` 标识原任务；替换或准入关闭时返回明确错误。
pub(in crate::agent_collaboration) fn require_active_owner(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
) -> Result<(), String> {
    let guard = state.lock().map_err(|error| error.to_string())?;
    if !Arc::ptr_eq(&guard.session_runtime_registry, registry) {
        return Err(String::from(
            "Collaboration recovery registry was replaced.",
        ));
    }
    if guard.service_lifecycle_state.shutdown_drain_active
        || guard.service_lifecycle_state.shutdown_recovery_blocked
        || guard
            .service_lifecycle_state
            .last_recovery_failure
            .is_some()
        || guard.service_lifecycle_state.http_bridge_failed
        || registry.shutdown_admission_is_closed()
    {
        return Err(String::from(
            "Collaboration recovery is suspended by service lifecycle admission.",
        ));
    }
    Ok(())
}

/// Reads the current index for each attempt; removed sessions and newly archived sessions use current ownership.
/// 每次尝试读取当前索引；已移除及新归档会话采用当前归属。
/// `app`, `state` and `registry` are the original worker services; successful repair returns completion.
/// `app`、`state` 和 `registry` 是原任务服务；修复成功返回完成。
async fn attempt(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
) -> Result<PassDisposition, String> {
    let sessions = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        if !Arc::ptr_eq(&guard.session_runtime_registry, registry) {
            return Ok(PassDisposition::Retired);
        }
        if guard.service_lifecycle_state.shutdown_drain_active
            || guard.service_lifecycle_state.shutdown_recovery_blocked
            || guard
                .service_lifecycle_state
                .last_recovery_failure
                .is_some()
            || guard.service_lifecycle_state.http_bridge_failed
            || registry.shutdown_admission_is_closed()
        {
            return Ok(PassDisposition::Suspended);
        }
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("Collaboration recovery requires a published workspace index.")
        })?;
        workspace
            .sessions
            .iter()
            .chain(&workspace.archived_sessions)
            .map(|session| session.id)
            .collect()
    };
    recover_indexed_collaboration_owned(app, state, sessions, registry).await?;
    Ok(PassDisposition::Complete)
}

/// Starts at most one repair worker for `state`; the returned handle permits explicit host/test cancellation.
/// 最多为 `state` 启动一个修复任务；返回句柄允许宿主或测试明确取消。
/// Returns no handle when an owner already exists; reservation failure is explicit and never replaces a worker.
/// 已有所有者时不返回句柄；预留失败明确返回，绝不替换任务。
pub(crate) fn schedule(
    state: &SharedServiceBridgeState,
    app: crate::ServiceEventEmitter,
) -> Result<Option<crate::state::ServiceTaskHandle<()>>, String> {
    let registry = state
        .lock()
        .map_err(|error| error.to_string())?
        .session_runtime_registry
        .clone();
    let Some(owner) = registry
        .try_reserve_collaboration_recovery()
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    // Keep no service-state strong reference during backoff, preventing a worker/service lifetime cycle.
    // 退避期间不保留服务状态强引用，防止任务与服务生命周期互相持有。
    let weak_state = Arc::downgrade(state);
    Ok(Some(crate::spawn_service_task(async move {
        let _owner = owner;
        // Backoff is bounded but retry count is not; committed work cannot disappear after an arbitrary attempt limit.
        // 退避有界但重试次数不限；已提交工作不能在任意次数上限后消失。
        let mut delay = Duration::from_secs(1);
        let mut last_error = None;
        loop {
            let Some(state) = weak_state.upgrade() else {
                break;
            };
            match attempt(&app, &state, &registry).await {
                Ok(PassDisposition::Complete | PassDisposition::Retired) => break,
                Ok(PassDisposition::Suspended) => {}
                Err(error) => {
                    if last_error.as_ref() != Some(&error) {
                        tracing::warn!(%error, "Collaboration startup recovery will retry");
                        if let Err(diagnostic) = crate::push_webui_service_diagnostic(
                            &state,
                            format!(
                                "Agent collaboration startup recovery has not finished yet and will retry automatically: {error}"
                            ),
                        ) {
                            tracing::warn!(%diagnostic, "Collaboration recovery diagnostic publication failed");
                        }
                        last_error = Some(error);
                    }
                }
            }
            drop(state);
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(30));
        }
    })))
}
