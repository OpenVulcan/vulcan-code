//! Retains child auxiliary admission until its exact durable execution is settled.
//! 在精确可靠执行结算前保留子辅助准入。

use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::{
    ChildAuxiliaryExecutionGuard, ChildAuxiliaryExecutionPurpose,
};
use vulcan_session_core::v1::TerminalSettlementScope;
use vulcan_session_core::v1::collaboration::{
    CollaborationExecutionId, CollaborationQuestionExecutionRecord,
};

/// Service ownership outlives Provider abort, retry, and failed terminal publication.
/// 服务所有权独立于 Provider 中止、重试及失败终态发布而存续。
pub(crate) struct ChildQuestionExecutionOwner {
    /// Exact auxiliary execution whose completion permits release.
    /// 完成后允许释放的精确辅助执行。
    execution_id: CollaborationExecutionId,
    /// Dispatch admission is enabled only after the transferred owner's publication check succeeds.
    /// 仅在转交所有者的发布校验成功后启用分发准入。
    dispatch_ready: bool,
    /// TaskRuntime exclusion kept alive without counting auxiliary work as business.
    /// 保持存活的 TaskRuntime 互斥，不将辅助工作计入业务。
    _admission: ChildAuxiliaryExecutionGuard,
}

impl ChildQuestionExecutionOwner {
    /// Returns the exact execution retained for authorized host stop and recovery operations.
    /// 返回为已授权宿主停止与恢复操作保留的精确执行。
    pub(crate) fn execution_id(&self) -> &CollaborationExecutionId {
        &self.execution_id
    }
}

/// Transfers a child reservation after Core admission, including failed publication that needs repair.
/// 在 Core 准入之后转交子预留，包括发布失败且需要修复的情况。
/// `execution` must match the canonical document and `admission` must protect its child.
/// `execution` 必须匹配规范文档，`admission` 必须保护对应子任务。
/// Publication errors retain a non-dispatchable owner for scoped cleanup; invalid identity is rejected before transfer.
/// 发布错误保留不可分发所有者供范围清理；无效身份在转交之前拒绝。
pub(crate) fn retain_child_question_execution_owner(
    state: &SharedServiceBridgeState,
    execution: &CollaborationQuestionExecutionRecord,
    admission: ChildAuxiliaryExecutionGuard,
) -> Result<(), String> {
    // The guard is the exact process-local task admission, never a caller-provided string.
    // 守卫是精确的进程内任务准入，而非调用者提供的字符串。
    let session_id = admission.task_id();
    if admission.purpose() != ChildAuxiliaryExecutionPurpose::Question
        || execution.participant.session_id != session_id.to_string()
        || execution.participant.task_id.as_ref().map(|id| id.as_str())
            != Some(execution.participant.session_id.as_str())
        || execution.state != CollaborationQuestionExecutionState::Running
    {
        return Err(String::from(
            "Child question reservation does not match the running participant.",
        ));
    }
    // Read document ownership outside the service lock to preserve lock ordering.
    // 在服务锁外读取文档所有权，保持锁顺序。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    handle
        .read(|document| {
            if document.segment_generation() != execution.participant.segment_generation
                || document
                    .collaboration_state()
                    .question_executions
                    .get(&execution.execution_id)
                    != Some(execution)
            {
                Err(String::from(
                    "Child question ownership requires its exact execution.",
                ))
            } else {
                Ok(())
            }
        })
        .map_err(|error| error.to_string())??;
    // Keep the transferred guard until explicit settlement, even when a stream future is dropped.
    // 保持转交守卫直到显式结算，即使流 Future 被丢弃也不释放。
    {
        // Retain exclusion before checking publication, since cleanup can itself fail to persist.
        // 在校验发布前保留互斥，因为清理本身也可能无法持久化。
        let mut service = state.lock().map_err(|error| error.to_string())?;
        if service
            .child_question_execution_owners
            .contains_key(&session_id)
        {
            return Err(String::from(
                "Child question execution already has a retained admission owner.",
            ));
        }
        service.child_question_execution_owners.insert(
            session_id,
            ChildQuestionExecutionOwner {
                execution_id: execution.execution_id.clone(),
                dispatch_ready: false,
                _admission: admission,
            },
        );
    }
    handle.read(|document| {
        if document.segment_generation() != execution.participant.segment_generation
            || document.memory_revision() != document.durable_revision()
            || document.collaboration_state().question_executions.get(&execution.execution_id) != Some(execution)
        {
            Err(String::from("Child question ownership requires its exact durable execution; admission retained for cleanup."))
        } else {
            Ok(())
        }
    }).map_err(|error| error.to_string())??;
    // Activate only the same owner; failure leaves its task reservation held for recovery.
    // 仅激活同一所有者；失败时保持任务预留，交由恢复处理。
    let mut service = state.lock().map_err(|error| error.to_string())?;
    let owner = service
        .child_question_execution_owners
        .get_mut(&session_id)
        .filter(|owner| owner.execution_id == execution.execution_id)
        .ok_or_else(|| String::from("Child question admission owner changed during transfer."))?;
    owner.dispatch_ready = true;
    Ok(())
}

/// Checks that an embedded stream has retained its exact TaskRuntime exclusion.
/// 检查嵌入流已保留其精确 TaskRuntime 互斥。
/// `session_id` and `execution_id` identify the dispatch; returns a missing-owner error.
/// `session_id` 与 `execution_id` 标识分发；缺少所有者时返回错误。
pub(crate) fn require_child_question_execution_owner(
    state: &SharedServiceBridgeState,
    session_id: Uuid,
    execution_id: &CollaborationExecutionId,
) -> Result<(), String> {
    // Ownership is authoritative in this map, not inferred from a running API.
    // 所有权由此映射表确定，不从运行中 API 推断。
    let service = state.lock().map_err(|error| error.to_string())?;
    if service
        .child_question_execution_owners
        .get(&session_id)
        .is_some_and(|owner| &owner.execution_id == execution_id && owner.dispatch_ready)
    {
        Ok(())
    } else {
        Err(String::from(
            "Child question stream has no retained execution admission owner.",
        ))
    }
}

/// Releases only the child reservation whose auxiliary terminal is durably complete.
/// 仅释放辅助终态已可靠完成的子预留。
/// `scope` supplies exact execution identity; unrelated scopes leave ownership unchanged.
/// `scope` 提供精确执行身份；无关范围保持所有权不变。
/// Returns an error and retains exclusion when terminal evidence is insufficient.
/// 终态证据不足时返回错误并保留互斥。
pub(crate) fn release_child_question_execution_owner(
    state: &SharedServiceBridgeState,
    session_id: Uuid,
    scope: &TerminalSettlementScope,
) -> Result<(), String> {
    let TerminalSettlementScope::QuestionExecution { execution_id, .. } = scope else {
        return Ok(());
    };
    {
        // Avoid loading documents for ordinary sessions or stale cleanup belonging to another execution.
        // 普通会话或属于另一执行的过期清理无需加载文档。
        let service = state.lock().map_err(|error| error.to_string())?;
        if !service
            .child_question_execution_owners
            .get(&session_id)
            .is_some_and(|owner| &owner.execution_id == execution_id)
        {
            return Ok(());
        }
    }
    // A dropped Provider future or in-memory completion is not durable release authority.
    // 被丢弃的 Provider Future 或内存完成不构成可靠释放权威。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    handle
        .read(|document| {
            if document.memory_revision() != document.durable_revision()
                || document.active_api().is_some()
                || !document
                    .collaboration_state()
                    .question_executions
                    .get(execution_id)
                    .is_some_and(|execution| {
                        execution.state == CollaborationQuestionExecutionState::Completed
                    })
            {
                Err(String::from(
                    "Child question admission cannot release before durable terminal completion.",
                ))
            } else {
                Ok(())
            }
        })
        .map_err(|error| error.to_string())??;
    // Recheck the exact owner after document inspection; never remove a replacement reservation.
    // 文档检查后重新核对精确所有者；绝不移除替代预留。
    let mut service = state.lock().map_err(|error| error.to_string())?;
    if service
        .child_question_execution_owners
        .get(&session_id)
        .is_some_and(|owner| &owner.execution_id == execution_id)
    {
        service.child_question_execution_owners.remove(&session_id);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
