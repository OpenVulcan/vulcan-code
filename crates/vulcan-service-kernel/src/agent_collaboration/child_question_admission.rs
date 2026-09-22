//! Publishes child QA admission without releasing task exclusion on a failed write.
//! 发布子答疑准入，写入失败时不释放任务互斥。

use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::ChildAuxiliaryExecutionGuard;
use vulcan_session_core::v1::collaboration::{
    CollaborationQuestionExecutionRecord, CollaborationQuestionRecord,
};

/// Prepares a canonical child execution and transfers its task guard before reporting publication errors.
/// 准备规范子执行，在报告发布错误前转交其任务守卫。
/// `state`, `registry`, and `handle` identify the owner; `admission` protects its business lane.
/// `state`、`registry` 和 `handle` 标识所有者；`admission` 保护其业务执行通道。
/// `question` is the accepted source fact; `started_at_ms` dates the atomic preparation.
/// `question` 是已接纳来源事实；`started_at_ms` 记录原子准备时间。
/// Returns the published execution or an error after attempting scoped cleanup with exclusion retained.
/// 返回已发布执行，或在保持互斥并尝试范围清理后返回错误。
pub(super) async fn prepare_child_question_execution(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    handle: &Arc<SessionHandle>,
    admission: ChildAuxiliaryExecutionGuard,
    question: CollaborationQuestionRecord,
    started_at_ms: i64,
) -> Result<CollaborationQuestionExecutionRecord, String> {
    // Establish identity before any memory mutation, rather than recovering it from a failed receipt.
    // 在任何内存修改前建立身份，不从失败回执猜测恢复身份。
    let session_id = admission.task_id();
    if handle.session_id() != session_id
        || question.target.session_id != session_id.to_string()
        || question.target.task_id.as_ref().map(|id| id.as_str())
            != Some(question.target.session_id.as_str())
    {
        return Err(String::from(
            "Child question admission does not match its canonical target.",
        ));
    }
    // Move the lifecycle guard into blocking work so cancellation of its waiter cannot unload the owner.
    // 将生命周期守卫移入阻塞工作，防止等待者取消时卸载所有者。
    let lifecycle = registry
        .acquire_lifecycle_transition(session_id)
        .await
        .map_err(|error| error.to_string())?;
    // The registry is the unique authority for the loaded handle.
    // 注册表是已加载句柄的唯一权威。
    let canonical = registry
        .get(session_id)
        .map_err(|error| error.to_string())?;
    if !Arc::ptr_eq(&canonical, handle) {
        return Err(String::from("Child question admission handle is stale."));
    }
    // Blocking ownership survives a dropped JoinHandle until publication and transfer actually finish.
    // 阻塞所有权在 JoinHandle 被丢弃后继续保持，直到发布及转交实际结束。
    let state_for_write = state.clone();
    let handle_for_write = handle.clone();
    let (execution, failure) = crate::spawn_service_blocking_task(move || {
        // This is the exact Core receipt captured before the fallible persistence phase.
        // 这是在可能失败的持久化阶段之前捕获的精确 Core 回执。
        let mut prepared_execution = None;
        let publication = handle_for_write.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                participants::require_document_message_admission(document, &question.source, &question.target)?;
                // Core preparation is failure-atomic; capture only a successful domain result.
                // Core 准备具有失败原子性；只捕获成功领域结果。
                let prepared = document.prepare_question_execution_before_api(question, started_at_ms)?;
                prepared_execution = Some(match &prepared {
                    IdempotentMutation::Committed(receipt) | IdempotentMutation::Idempotent(receipt) => receipt.execution.clone(),
                });
                Ok(session_change(prepared))
            },
        );
        // No successful Core mutation means there is no auxiliary execution requiring retained exclusion.
        // 没有成功 Core 修改意味着不存在需要保留互斥的辅助执行。
        let Some(execution) = prepared_execution else {
            return Err(match publication {
                Err(error) => format!("Child question admission failed before execution creation: {error}"),
                Ok(_) => String::from("Child question admission returned no Core execution receipt."),
            });
        };
        // The task guard stays on this stack until the service takes it, including failed flushes.
        // 包括刷写失败在内，任务守卫始终留在此栈上，直到服务接管。
        let transfer = child_execution_ownership::retain_child_question_execution_owner(
            &state_for_write, &execution, admission,
        );
        // Keep both errors: failed publication and failed dispatch activation describe distinct stages.
        // 保留两项错误：发布失败与分发激活失败描述不同阶段。
        let failure = match (publication, transfer) {
            (Ok(_), Ok(())) => None,
            (Err(error), Ok(())) => Some(format!("Child question publication failed: {error}")),
            (Ok(_), Err(error)) => Some(error),
            (Err(publication_error), Err(transfer_error)) => Some(format!(
                "Child question publication failed: {publication_error}; ownership transfer: {transfer_error}"
            )),
        };
        drop(lifecycle);
        Ok::<_, String>((execution, failure))
    }).await.map_err(|error| format!("Child question publication task failed: {error}"))??;
    if let Some(error) = failure {
        crate::agent_stream::fail_undispatched_question_execution(
            state, session_id, &execution, &error,
        )
        .await?;
        return Err(error);
    }
    Ok(execution)
}

#[cfg(test)]
mod tests;
