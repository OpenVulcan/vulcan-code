//! Captures a precise auxiliary stop boundary before interrupting its physical API.
//! 在中断物理 API 之前捕获精确辅助停止边界。

use vulcan_session_core::v1::collaboration::{
    CollaborationExecutionId, CollaborationExecutionPurpose,
};
use vulcan_session_core::v1::document::{SessionDocument, TurnOrigin};
use vulcan_session_core::v1::{ApiPurposeKind, DomainError, TerminalSettlementScope};

/// Stable reason shared by early intent, final settlement and recovery.
/// 早期意图、最终结算与恢复共享的稳定原因。
pub(super) const QUESTION_STOP_REASON: &str = "question_execution_stopped";
/// Exact user-stop detail retained across the same settlement attempt.
/// 同一次结算尝试保留的精确用户停止详情。
pub(super) const QUESTION_STOP_DETAIL: &str = "The user stopped the question execution.";

/// Persists the exact QA stop intent before fallible browser cleanup or API interruption.
/// 在可能失败的浏览器清理或 API 中断之前，持久化精确答疑停止意图。
/// `handle` is the canonical target; session, execution and owner generation identify this stop transaction.
/// `handle` 为规范目标；会话、执行与所有者代际标识当前停止事务。
/// Returns its frozen scope; errors retain the registered owner and available repair facts.
/// 返回冻结范围；发生错误时保留已登记所有者及可用恢复事实。
pub(super) async fn persist_question_stop_intent(
    state: &crate::state::SharedServiceBridgeState,
    handle: &std::sync::Arc<crate::session_runtime_registry_v1::SessionHandle>,
    session_id: crate::SessionId,
    execution_id: &CollaborationExecutionId,
    owner_generation: String,
) -> Result<TerminalSettlementScope, String> {
    use crate::terminal_settlement_service::{
        TerminalSettlementIntentTransition, clear_terminal_pre_intent_repair,
        persist_terminal_settlement_intent, retain_terminal_pre_intent_repair,
        terminal_settlement_id,
    };
    // scope and turn_id are captured together after the old stream and result workers have stopped.
    // scope 与 turn_id 在旧流及结果工作器停止之后一起捕获。
    let (scope, turn_id) = handle
        .read(|document| {
            let scope = question_stop_scope(document, execution_id)?;
            let turn_id = document
                .active_turn()
                .ok_or(DomainError::NoActiveTurn)?
                .turn_id;
            Ok::<_, DomainError>((scope, turn_id))
        })
        .map_err(|error| format!("Failed to read question stop intent: {error}"))?
        .map_err(|error| format!("Invalid question stop intent: {error}"))?;
    retain_terminal_pre_intent_repair(
        state,
        session_id,
        crate::state::TerminalPreIntentRepair {
            owner_generation: owner_generation.clone(),
            reason_code: QUESTION_STOP_REASON,
            redacted_detail: Some(QUESTION_STOP_DETAIL.to_owned()),
            scope: scope.clone(),
        },
    )?;
    // now_ms dates this durable intent without changing its idempotent semantic identity.
    // now_ms 为可靠意图记录时间，不改变其幂等语义身份。
    let now_ms = crate::current_v1_unix_millis()?;
    persist_terminal_settlement_intent(
        handle,
        TerminalSettlementIntentTransition::Begin {
            record: vulcan_session_core::v1::TerminalSettlementRecord {
                settlement_id: terminal_settlement_id(session_id, &owner_generation),
                session_id: session_id.to_string(),
                turn_id,
                scope: scope.clone(),
                owner_generation: owner_generation.clone(),
                state: vulcan_session_core::v1::TerminalSettlementState::Pending,
                reason_code: QUESTION_STOP_REASON.to_owned(),
                updated_at_ms: now_ms,
            },
        },
    )
    .await?;
    clear_terminal_pre_intent_repair(state, session_id, &owner_generation);
    Ok(scope)
}

/// Proves the kernel-selected question owns the current document turn and optional API.
/// 证明内核选定的问题拥有当前文档回合及可选 API。
/// `execution_id` comes from the stopped kernel; `document` supplies canonical ownership facts.
/// `execution_id` 来自被停止的内核；`document` 提供规范归属事实。
/// Returns an immutable settlement scope or rejects a mismatched owner without mutation.
/// 返回不可变结算范围，或不作变更地拒绝错误所有者。
pub(super) fn question_stop_scope(
    document: &SessionDocument,
    execution_id: &CollaborationExecutionId,
) -> Result<TerminalSettlementScope, DomainError> {
    // execution must be the exact record retained by the kernel, never the most recent question.
    // execution 必须是内核保留的精确记录，绝不选取最新问题。
    let execution = document
        .collaboration_state()
        .question_executions
        .get(execution_id)
        .ok_or_else(|| DomainError::CollaborationNotFound {
            kind: "question_execution",
            identity: execution_id.as_str().to_owned(),
        })?;
    // turn_id follows the exclusive binding defined by the execution purpose.
    // turn_id 遵循执行用途定义的互斥绑定。
    let (turn_id, auxiliary) = match (
        execution.purpose,
        execution.parent_turn_id,
        execution.execution_turn_id,
    ) {
        (CollaborationExecutionPurpose::InTurnQuestion, Some(turn_id), None) => (turn_id, false),
        (CollaborationExecutionPurpose::NaturallyIdleQuestion, None, Some(turn_id)) => {
            (turn_id, true)
        }
        _ => {
            return Err(DomainError::CollaborationStateViolation {
                field: "question_stop.turn_binding",
            });
        }
    };
    // turn proves logical ownership even when no physical request started during preparation.
    // turn 即使在准备期间尚无物理请求启动时，也证明逻辑归属。
    let turn = document.active_turn().ok_or(DomainError::NoActiveTurn)?;
    if turn.turn_id != turn_id || (turn.origin == TurnOrigin::QuestionExecution) != auxiliary {
        return Err(DomainError::CollaborationStateViolation {
            field: "question_stop.turn_owner",
        });
    }
    // api_call_id is captured before interruption so the later durable settlement proves the same API.
    // api_call_id 在中断之前捕获，使后续可靠结算证明相同 API。
    let api_call_id = match document.active_api() {
        Some(api) if api.turn_id == turn_id && api.purpose == ApiPurposeKind::QuestionExecution => {
            Some(api.api_call_id)
        }
        None => None,
        Some(_) => {
            return Err(DomainError::CollaborationStateViolation {
                field: "question_stop.api_owner",
            });
        }
    };
    Ok(TerminalSettlementScope::QuestionExecution {
        execution_id: execution_id.clone(),
        api_call_id,
    })
}

#[cfg(test)]
mod tests;
