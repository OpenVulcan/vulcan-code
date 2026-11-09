//! Explicit manual compaction admission and settlement without business continuation.
//! 显式手动压缩的准入与结算，不续跑业务。

use super::*;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};
use vulcan_session_core::v1::{
    collaboration::CollaborationExecutionId,
    document::IdempotentMutation,
    event::{DisplayNoticeKind, NoticeSeverity, TurnClosureReason},
};

#[cfg(test)]
mod tests;

/// Chooses a validated borrowed business owner or publishes an independent auxiliary turn.
/// 选择经过校验的借用业务所有者，或发布独立辅助回合。
/// `state` provides registered writers and stream owners; `session_id` selects the canonical document.
/// `state` 提供已登记写入器与流所有者；`session_id` 选择规范文档。
/// `source` binds the prepared history and `task_id` is both the stream and maintenance owner.
/// `source` 绑定已准备历史，`task_id` 同时标识流任务和维护所有者。
/// Returns the exact completion mode after source validation; borrowing never changes business run control.
/// 来源校验后返回精确完成方式；借用绝不改变业务运行控制。
pub(super) async fn begin(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
    source: vulcan_session_core::v1::context_maintenance::ManualContextMaintenanceSource,
) -> Result<ContextCompactionCommitMode, String> {
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    // Drain a prior cancelled preparation before borrowing its business boundary.
    // 借用业务边界前排空之前已取消的准备工作。
    let preparation_settlement = handle.acquire_request_preparation_settlement().await;
    // Verify the registered task before transferring the writer to the blocking pool.
    // 将写入器转交阻塞线程池前核验已登记任务。
    {
        let guard = state.lock().map_err(|error| error.to_string())?;
        if !active_stream_task_matches_id(&guard, session_id, task_id) {
            return Err(String::from(
                "Manual context compaction ownership changed before admission.",
            ));
        }
    }
    // Exact source evidence rejects a racing Stop or history edit while allowing composer draft saves.
    // 精确来源证据拒绝竞争的停止或历史编辑，同时允许输入框草稿保存。
    // The stream UUID is never reused as a business execution or user input identity.
    // 流 UUID 绝不被复用为业务执行或用户输入身份。
    let execution_id = CollaborationExecutionId::try_new(task_id.to_string())
        .map_err(|error| error.to_string())?;
    // Capture admission time before publishing the exact owner.
    // 在发布精确所有者前捕获准入时间。
    let now_ms = crate::current_v1_unix_millis()?;
    let receipt = handle
        .mutate_durable_if_changed_async(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::MetadataChanged,
            move |document| {
                // Keep exclusion inside the blocking writer even when the async waiter is cancelled.
                // 即使异步等待者取消，也在阻塞写入器内部保持互斥。
                let _preparation_settlement = preparation_settlement;
                if document
                    .metadata
                    .context_maintenance
                    .as_ref()
                    .is_some_and(|record| record.execution_id == execution_id)
                {
                    document.begin_manual_context_maintenance(&source, execution_id, now_ms)?;
                    return Ok(SessionMutationChange::Unchanged(
                        ContextCompactionCommitMode::FinishActiveStream { task_id },
                    ));
                }
                if let Some(owner) = document.borrow_manual_context_compaction(&source)? {
                    return Ok(SessionMutationChange::Unchanged(
                        ContextCompactionCommitMode::FinishBorrowedStream { task_id, owner },
                    ));
                }
                Ok(
                    match document.begin_manual_context_maintenance(
                        &source,
                        execution_id,
                        now_ms,
                    )? {
                        IdempotentMutation::Committed(_) => SessionMutationChange::Changed(
                            ContextCompactionCommitMode::FinishActiveStream { task_id },
                        ),
                        IdempotentMutation::Idempotent(_) => SessionMutationChange::Unchanged(
                            ContextCompactionCommitMode::FinishActiveStream { task_id },
                        ),
                    },
                )
            },
        )
        .await
        .map_err(|error| match &error {
            crate::session_runtime_registry_v1::SessionRuntimeError::Domain(cause)
                if matches!(cause.as_ref(), vulcan_session_core::v1::DomainError::CollaborationStateViolation {
                    field: "manual_maintenance.competing_owner",
                }) => String::from("Manual compaction is waiting for an auxiliary execution or unsettled child task. Finish or settle that work before compacting; the business turn was not changed."),
            _ => error.to_string(),
        })?;
    Ok(receipt.value)
}

/// Closes the exact successful auxiliary owner after its canonical compaction node is committed.
/// 在规范压缩节点已提交后闭合精确成功辅助所有者。
/// `state` supplies the registered writer for `session_id`; success returns after durable closure.
/// `state` 提供 `session_id` 的已登记写入器；成功时在可靠闭合后返回。
/// `task_id` identifies the owned execution; errors retain ownership for explicit failure settlement.
/// `task_id` 标识所属执行；错误保留所有权以进行显式失败结算。
pub(super) fn complete(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
) -> Result<(), String> {
    // Resolve the same session authority used to publish the compaction node.
    // 解析发布压缩节点所使用的同一会话权威。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    // The UUID must match the auxiliary start, never just the latest session turn.
    // UUID 必须匹配辅助起始事实，不能仅匹配会话最近回合。
    let execution_id = CollaborationExecutionId::try_new(task_id.to_string())
        .map_err(|error| error.to_string())?;
    // The terminal time belongs to this explicit publication.
    // 终态时间属于本次显式发布。
    let now_ms = crate::current_v1_unix_millis()?;
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                Ok(
                    match document.finish_context_maintenance(
                        &execution_id,
                        TurnClosureReason::Completed,
                        now_ms,
                    )? {
                        IdempotentMutation::Committed(seq) => SessionMutationChange::Changed(seq),
                        IdempotentMutation::Idempotent(seq) => {
                            SessionMutationChange::Unchanged(seq)
                        }
                    },
                )
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Publishes a visible failure and closes only this manual auxiliary execution, including pre-dispatch rejection.
/// 发布可见失败并且只闭合本次手动辅助执行，包含分发前拒绝。
/// `state` supplies the registered writer for `session_id`; `task_id` selects the exact failed execution.
/// `state` 提供 `session_id` 的已登记写入器；`task_id` 选择精确失败执行。
/// `error` is the backend diagnostic; returns after the event patch and storage have the same authoritative fact.
/// `error` 是后端诊断；事件补丁与存储拥有同一权威事实后返回。
pub(super) fn fail(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
    error: &str,
) -> Result<(), String> {
    // A session event remains visible even when no physical API was admitted.
    // 即使没有物理 API 获准执行，会话事件也保持可见。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    // Exact execution matching prevents a late callback from closing replacement work.
    // 精确执行匹配防止迟到回调闭合替代工作。
    let execution_id = CollaborationExecutionId::try_new(task_id.to_string())
        .map_err(|error| error.to_string())?;
    // Failure time is independent from the already completed business turn.
    // 失败时间与已结束业务回合独立。
    let now_ms = crate::current_v1_unix_millis()?;
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                if let Some(record) = document
                    .metadata
                    .context_maintenance
                    .as_ref()
                    .filter(|record| record.execution_id == execution_id)
                {
                    // A duplicate callback cannot close a later business turn after this owner already settled.
                    // 本所有者已结算后，重复回调不能闭合后续业务回合。
                    if record.terminal.is_some() {
                        return Ok(SessionMutationChange::Unchanged(()));
                    }
                    if !document
                        .active_turn()
                        .is_some_and(|turn| turn.turn_id == record.turn_id && !turn.closed)
                    {
                        return Err(
                            vulcan_session_core::v1::DomainError::CollaborationStateViolation {
                                field: "manual_maintenance.failure_owner",
                            },
                        );
                    }
                    return document
                        .fail_execution_boundary_and_close_turn(
                            "context_compaction_failed",
                            Some(error),
                            now_ms,
                        )
                        .map(|changed| {
                            if changed {
                                SessionMutationChange::Changed(())
                            } else {
                                SessionMutationChange::Unchanged(())
                            }
                        });
                }
                // Admission may reject an existing business boundary; show the error without closing or relabeling it.
                // 准入可能拒绝已有业务边界；展示错误，但不闭合或重新归属该边界。
                document.append_display_note(
                    None,
                    DisplayNoticeKind::SystemError,
                    NoticeSeverity::Error,
                    "context_compaction_failed",
                    Some(error.to_owned()),
                    None,
                    None,
                    None,
                    now_ms,
                )?;
                Ok(SessionMutationChange::Changed(()))
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
