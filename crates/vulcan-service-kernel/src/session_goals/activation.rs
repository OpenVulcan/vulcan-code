//! Atomic execution admission for explicit user Goal activation.
//! 显式用户 Goal 激活的原子执行准入。

use super::*;
use vulcan_session_core::v1::collaboration::{
    CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
};
use vulcan_session_core::v1::document::{GoalModeTransition, SessionDocument};
use vulcan_session_core::v1::errors::DomainError;

/// Adds execution consent to a successful user transition inside its caller's atomic draft.
/// 在调用方原子草稿中为成功的用户转换附加执行许可。
/// The exact transition notice identifies this admission; errors roll back both mode and control.
/// 精确转换通知标识本次准入；错误同时回滚模式与控制。
/// Native AI Goal tools must not call this user-authorized activation helper.
/// 原生 AI Goal 工具不得调用此用户授权激活帮助函数。
pub(super) fn activate_user_goal_transition(
    document: &mut SessionDocument,
    transition: &GoalModeTransition,
    now_ms: i64,
) -> Result<(), DomainError> {
    // Generation and event identity prevent a later Stop from sharing an old activation receipt.
    // 代次与事件身份防止后续 Stop 共用旧激活回执。
    let operation_id = CollaborationOperationId::try_new(format!(
        "user-goal-activation:{}:{}:{}",
        document.metadata.session_id,
        document.segment_generation(),
        transition.notice_seq.value(),
    ))?;
    document.set_collaboration_run_control(SetRunControlInput {
        operation_id,
        state: CollaborationRunControl::Running,
        changed_at_ms: now_ms,
    })?;
    Ok(())
}

/// Persists an explicit user resume and execution consent in one revision.
/// 在同一修订中持久保存显式用户恢复及执行许可。
/// Inputs are validated by the canonical domain transition; returns whether it allocated a turn.
/// 输入由规范领域转换校验；返回是否分配了回合。
pub(super) fn resume_session_goal_v1(
    handle: &SessionHandle,
    objective: String,
    token_budget: Option<u64>,
    now_ms: i64,
) -> Result<SessionConditionalMutationReceipt<bool>, SessionRuntimeError> {
    handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        move |document| {
            document.apply_atomic_transition(move |document| {
                // Resume accepts only a paused or otherwise resumable Goal; repeated Active is rejected.
                // Resume 只接纳暂停或其他可恢复 Goal；重复 Active 被拒绝。
                let transition =
                    document.resume_goal(objective.clone(), token_budget, objective, now_ms)?;
                activate_user_goal_transition(document, &transition, now_ms)?;
                Ok(SessionMutationChange::Changed(
                    transition.continuation_turn_id.is_some(),
                ))
            })
        },
    )
}

/// Applies the closed user status enum without reactivating an already matching stopped Goal.
/// 应用封闭用户状态枚举，不重新激活状态已匹配但已停止的 Goal。
/// Returns an unchanged receipt for exact status no-ops; a real resume atomically adds Running.
/// 精确状态无操作返回未变回执；真实恢复原子附加 Running。
pub(super) fn update_session_goal_status_v1(
    handle: &SessionHandle,
    status: SessionGoalUserStatusUpdate,
    now_ms: i64,
) -> Result<SessionConditionalMutationReceipt<()>, SessionRuntimeError> {
    handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        move |document| {
            document.apply_atomic_transition(move |document| {
                if document.metadata.mode.goal.enabled
                    && document.metadata.mode.goal.state == status.into_goal_state()
                {
                    return Ok(SessionMutationChange::Unchanged(()));
                }
                match status {
                    SessionGoalUserStatusUpdate::Active => {
                        // Preserve the exact current definition when the user presses resume.
                        // 用户点击恢复时保留精确当前定义。
                        let objective = document.metadata.mode.goal.objective.clone();
                        let budget_tokens = document.metadata.mode.goal.budget_tokens;
                        let transition = document.resume_goal(
                            objective.clone(),
                            budget_tokens,
                            objective,
                            now_ms,
                        )?;
                        activate_user_goal_transition(document, &transition, now_ms)?;
                    }
                    SessionGoalUserStatusUpdate::Paused => {
                        document.pause_goal(GOAL_PAUSED_PROVIDER_CONTEXT, now_ms)?;
                    }
                }
                Ok(SessionMutationChange::Changed(()))
            })
        },
    )
}

#[cfg(test)]
mod tests;
