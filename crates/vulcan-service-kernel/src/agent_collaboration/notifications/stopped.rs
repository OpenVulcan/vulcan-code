//! Forwards durable stopped-notification receipts without restarting either participant.
//! 回传可靠通知停止回执，不重启任一参与者。

use super::*;

/// Forwards every stopped notification from `target` to its verified source in `state`.
/// 将 `target` 的全部停止通知回传给 `state` 中经过校验的来源。
/// Returns new source receipt count, or aggregate errors after attempting every independent source.
/// 返回新来源回执数量，或尝试全部独立来源后汇总错误。
pub(in crate::agent_collaboration) async fn reconcile_stopped_notifications_for_session(
    state: &SharedServiceBridgeState,
    target: &Arc<SessionHandle>,
) -> Result<usize, String> {
    // Snapshot only immutable terminal candidates; source acknowledgement uses the original receipt.
    // 仅快照不可变终态候选；来源确认使用原始回执。
    let failed = target
        .read(|document| {
            document
                .collaboration_state()
                .inbound_notifications
                .values()
                .filter(|record| {
                    record.state == CollaborationNotificationState::Failed
                        && record.failure == Some(CollaborationDeliveryFailureReason::TargetStopped)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .map_err(|error| error.to_string())?;
    if failed.is_empty() {
        return Ok(0);
    }
    // Capture the canonical owner without holding the service mutex during storage work.
    // 捕获规范所有者，不在存储工作期间持有服务互斥锁。
    let (controller, registry) = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        (
            guard.controller_runtime.clone().ok_or_else(|| {
                String::from("controller runtime is unavailable for notification return")
            })?,
            Arc::clone(&guard.session_runtime_registry),
        )
    };
    // The target invocation resolves exact participants; it does not grant new sending authority.
    // 目标恢复上下文解析精确参与者，不授予新的发送权限。
    let lookup = recovery_invocation(
        Arc::clone(state),
        controller.clone(),
        registry.clone(),
        target.session_id(),
        Arc::clone(target),
    )?;
    // Keep original target storage coordinates for immutable terminal receipt recovery.
    // 为不可变终态回执恢复保留原目标存储坐标。
    let resolved = ResolvedSession {
        locator: target
            .storage_locator()
            .map_err(|error| error.to_string())?,
        handle: Arc::clone(target),
    };
    // Count only newly saved acknowledgements and attempt all sources even after a failure.
    // 只计数新保存的确认；某个来源失败后仍尝试其余来源。
    let mut delivered = 0;
    let mut errors = Vec::new();
    for inbound in failed {
        // Reuse the same immutable target-proof validator used by cold outbox recovery.
        // 复用冷出站恢复使用的同一不可变目标凭据校验器。
        let result = async {
            let source = participants::load_participant_handle(&lookup, &inbound.source).await?;
            let record = source
                .read(|document| {
                    document
                        .collaboration_state()
                        .notifications
                        .get(&inbound.notification_id)
                        .cloned()
                })
                .map_err(|error| error.to_string())?
                .ok_or_else(|| String::from("stopped notification source intent is missing"))?;
            let invocation = recovery_invocation(
                Arc::clone(state),
                controller.clone(),
                registry.clone(),
                source.session_id(),
                source,
            )?;
            recover_existing_notification_progress(&invocation, &record, &resolved).await
        }
        .await;
        match result {
            Ok(true) => delivered += 1,
            Ok(false) => {}
            Err(error) => errors.push(format!(
                "notification {} stop return failed: {error}",
                inbound.notification_id.as_str()
            )),
        }
    }
    if errors.is_empty() {
        Ok(delivered)
    } else {
        Err(errors.join("; "))
    }
}
