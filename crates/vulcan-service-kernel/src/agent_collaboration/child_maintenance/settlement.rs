//! Exact maintenance settlement after the owning asynchronous work has stopped.
//! 所属异步工作停止后的精确维护结算。

use super::*;
use vulcan_session_core::v1::DomainError;
use vulcan_session_core::v1::event::TurnClosureReason;

/// Settles only `execution_id` on the canonical `handle` after its Provider task has been joined.
/// 在供应商任务已等待退出之后，仅结算规范 `handle` 上的 `execution_id`。
/// `reason` records the requested terminal and `now_ms` dates it; `state` retains failed publication ownership.
/// `reason` 记录请求终态，`now_ms` 标记时间；`state` 保留发布失败的所有权。
/// Returns after durable release; retries preserve an existing terminal and never touch a successor execution.
/// 可靠释放后返回；重试保留已有终态，绝不触碰后继执行。
/// The caller must stop and join asynchronous work before calling; this function drains admitted blocking writers.
/// 调用者必须先停止并等待异步工作退出；本函数排空已准入的阻塞写入器。
pub(super) async fn settle(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    handle: &Arc<SessionHandle>,
    execution_id: &CollaborationExecutionId,
    reason: TurnClosureReason,
    now_ms: i64,
) -> Result<(), String> {
    // Preparation can outlive its aborted waiter; retain its settlement gate inside the actual writer.
    // 准备可能比被中止的等待者存活更久；在实际写入器内保留其结算门。
    let preparation_settlement = handle.acquire_request_preparation_settlement().await;
    // Blocking work keeps all ownership until its final publication and release have really finished.
    // 阻塞工作保持全部所有权，直到最终发布与释放实际完成。
    let state = Arc::clone(state);
    // Keep the canonical handle instead of resolving another document during retry.
    // 保留规范句柄，而不是在重试中解析另一个文档。
    let handle = Arc::clone(handle);
    // Capture the exact maintenance identity, independently of the latest metadata value.
    // 捕获精确维护身份，独立于最新元数据值。
    let execution_id = execution_id.clone();
    lifecycle_write::with_canonical_handles(registry, vec![Arc::clone(&handle)], move || {
        // Moving the guard here also covers cancellation while storage is blocking.
        // 将守卫移入此处也覆盖存储阻塞期间的取消。
        let _preparation_settlement = preparation_settlement;
        // A missing owner is only an idempotent success for this exact, already durable terminal.
        // 只有本精确终态已经可靠保存时，缺失所有者才属于幂等成功。
        let expected = {
            let service = state.lock().map_err(|error| error.to_string())?;
            match service.child_maintenance_owners.get(&handle.session_id()) {
                Some(owner) if owner.execution.execution_id == execution_id => {
                    Some(owner.execution.clone())
                }
                Some(_) => return Err(String::from("Child maintenance settlement owner changed.")),
                None => None,
            }
        };
        handle
            .read(|document| {
                let current = document
                    .metadata
                    .context_maintenance
                    .as_ref()
                    .ok_or_else(|| {
                        String::from("Child maintenance settlement has no domain execution.")
                    })?;
                if current.execution_id != execution_id
                    || current.segment_generation != document.segment_generation()
                {
                    return Err(String::from(
                        "Child maintenance settlement execution changed.",
                    ));
                }
                if !document
                    .metadata
                    .child_execution_profile
                    .as_ref()
                    .is_some_and(|profile| {
                        profile.business_execution_id == current.business_execution_id.as_str()
                            && profile.business_turn_id == current.business_turn_id
                            && profile.segment_generation == current.segment_generation
                            && profile.task_id.as_str() == handle.session_id().to_string()
                    })
                {
                    return Err(String::from(
                        "Child maintenance settlement business profile changed.",
                    ));
                }
                if let Some(expected) = &expected {
                    let mut start = current.clone();
                    start.terminal = None;
                    if &start != expected {
                        return Err(String::from("Child maintenance settlement start changed."));
                    }
                } else if current.terminal.is_none()
                    || document.memory_revision() != document.durable_revision()
                {
                    return Err(String::from(
                        "Child maintenance settlement has no retained owner.",
                    ));
                }
                Ok(())
            })
            .map_err(|error| error.to_string())??;
        if expected.is_none() {
            return Ok(());
        }
        // Disable dispatch even when terminal publication fails and the owner must remain for repair.
        // 即使终态发布失败且必须保留所有者供修复，也禁用分发。
        state
            .lock()
            .map_err(|error| error.to_string())?
            .child_maintenance_owners
            .get_mut(&handle.session_id())
            .filter(|owner| owner.execution.execution_id == execution_id)
            .ok_or_else(|| String::from("Child maintenance settlement owner disappeared."))?
            .dispatch_ready = false;
        // Physical interruption is published separately so retries can finish a partially settled API safely.
        // 单独发布物理中断，使重试能安全完成部分结算的 API。
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                |document| {
                    let current = document
                        .metadata
                        .context_maintenance
                        .as_ref()
                        .filter(|record| record.execution_id == execution_id)
                        .ok_or(DomainError::CollaborationStateViolation {
                            field: "context_maintenance.settlement_owner",
                        })?;
                    if current.terminal.is_some() {
                        return Ok(SessionMutationChange::Unchanged(()));
                    }
                    if !document.active_turn().is_some_and(|turn| {
                        !turn.closed
                            && turn.turn_id == current.turn_id
                            && turn.origin == TurnOrigin::ContextMaintenance
                    }) {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "context_maintenance.settlement_turn",
                        });
                    }
                    if reason == TurnClosureReason::Completed && document.active_api().is_some() {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "context_maintenance.success_api_in_flight",
                        });
                    }
                    match document.interrupt_active_api(now_ms) {
                        Ok(_) => Ok(SessionMutationChange::Changed(())),
                        Err(DomainError::NoActiveApi) => Ok(SessionMutationChange::Unchanged(())),
                        Err(error) => Err(error),
                    }
                },
            )
            .map_err(|error| {
                format!("Child maintenance API settlement failed; admission retained: {error}")
            })?;
        // An already committed success or failure is immutable, including after a failed terminal flush.
        // 已提交成功或失败不可变，包括终态刷写失败之后。
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::MetadataChanged,
                |document| {
                    let current = document
                        .metadata
                        .context_maintenance
                        .as_ref()
                        .filter(|record| record.execution_id == execution_id)
                        .ok_or(DomainError::CollaborationStateViolation {
                            field: "context_maintenance.settlement_terminal_owner",
                        })?;
                    if current.terminal.is_some() {
                        return Ok(SessionMutationChange::Unchanged(()));
                    }
                    document
                        .finish_child_context_maintenance(&execution_id, reason, now_ms)
                        .map(|result| match result {
                            IdempotentMutation::Committed(_) => SessionMutationChange::Changed(()),
                            IdempotentMutation::Idempotent(_) => {
                                SessionMutationChange::Unchanged(())
                            }
                        })
                },
            )
            .map_err(|error| {
                format!("Child maintenance terminal settlement failed; admission retained: {error}")
            })?;
        // Retry also publishes dirty revisions left by a prior failed flush before releasing exclusion.
        // 重试在释放互斥前也发布此前刷写失败留下的脏修订。
        handle.flush_all().map_err(|error| error.to_string())?;
        release_owner(&state, &handle, &execution_id)
    })
    .await
}

#[cfg(test)]
mod tests;
