//! Exact child-document cleanup for admission failures before the start barrier is acknowledged.
//! 启动屏障确认前准入失败时，精确清理子文档。

use super::*;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};
use vulcan_session_core::v1::{DomainError, TurnId};

#[cfg(test)]
mod tests;

/// Immutable ownership captured from the admitted child's real root input.
/// 从已准入子任务真实根输入捕获的不可变归属。
#[derive(Clone, Copy)]
pub(crate) struct DetachedStartupOwner {
    /// Original document generation, never inferred again during cleanup.
    /// 原始文档代次，清理时绝不重新推断。
    pub(super) generation: u64,
    /// Original logical child turn, never replaced with the current turn.
    /// 原始逻辑子回合，绝不替换为当前回合。
    pub(super) turn_id: TurnId,
}

/// Captures the original root's ownership and rejects unrelated execution identifiers.
/// 捕获原根输入归属，并拒绝无关执行标识。
/// `handle` is the canonical child and `execution_id` is its authoritative runtime admission.
/// `handle` 为规范子句柄，`execution_id` 为权威运行时准入身份。
/// Returns frozen ownership or an explicit source-identity error without modifying the document.
/// 返回冻结归属或明确来源身份错误，不修改文档。
pub(crate) fn capture_owner(
    handle: &SessionHandle,
    execution_id: &str,
) -> Result<DetachedStartupOwner, String> {
    handle.read_full_durable(|document| {
        // The just-admitted root must still own the open child turn before subscription begins.
        // 刚准入的根输入必须在订阅开始前仍拥有开放子回合。
        let turn = document.active_turn().filter(|turn| !turn.closed)
            .ok_or_else(|| String::from("Detached startup has no open child turn."))?;
        if !document.events().iter().any(|event| event.seq == turn.start_seq
            && event.turn_id == Some(turn.turn_id)
            && matches!(&event.event, EventBody::UserInput(input) if input.client_action_id.as_str() == execution_id)) {
            return Err(String::from("Detached startup does not match the child root execution."));
        }
        Ok(DetachedStartupOwner { generation: document.segment_generation(), turn_id: turn.turn_id })
    }).map_err(|error| error.to_string())?
}

/// Closes only the rejected original child turn before a Provider can be admitted.
/// 在 Provider 能够准入之前，仅关闭被拒的原子回合。
/// `owner` is captured before subscription; `reason` retains the actual admission failure.
/// `owner` 在订阅前捕获；`reason` 保留实际准入失败。
/// Returns after persistence, leaving replaced or already terminal child work untouched.
/// 持久化后返回，保持替代执行或已经终态的子任务不变。
pub(crate) fn close_rejected_child(
    handle: &SessionHandle,
    owner: DetachedStartupOwner,
    reason: &str,
) -> Result<(), String> {
    // Freeze one timestamp for the complete failure publication.
    // 为完整失败发布冻结一个时间戳。
    let now_ms = current_v1_unix_millis()?;
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                if document.segment_generation() != owner.generation
                    || document
                        .active_turn()
                        .is_none_or(|turn| turn.closed || turn.turn_id != owner.turn_id)
                {
                    return Ok(SessionMutationChange::Unchanged(()));
                }
                // This cleanup is legal only before the start barrier releases model execution.
                // 此清理仅在启动屏障释放模型执行之前合法。
                if document.active_api().is_some() {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "detached_startup_rejection.active_api",
                    });
                }
                let changed = document.fail_execution_boundary_and_close_turn(
                    "detached_start_admission_failed",
                    Some(reason),
                    now_ms,
                )?;
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}
