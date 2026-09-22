//! Admission decisions for collaboration-related physical executions.
//! 协作相关物理执行的准入判定。

pub(crate) mod preparation_tasks;

use vulcan_session_core::v1::collaboration::{
    CollaborationExecutionPurpose, CollaborationRunControl,
};

/// Describes the result of trying to wake one durable question execution.
/// 描述尝试唤醒一个可靠问题执行的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuestionExecutionLaunchDisposition {
    /// The question is settled or its single handling execution has already ended.
    /// 问题已结算或唯一处理执行已经结束。
    AlreadyHandled,
    /// A dedicated naturally-idle auxiliary turn owns the new provider stream.
    /// 一个专用的自然空闲辅助回合已经拥有新的 Provider 流。
    Started,
    /// An in-turn question remains attached to the existing parent stream boundary.
    /// 回合内问题继续绑定现有父回合的合法流边界。
    DeferredToParentTurn,
    /// Another stream or preparation owner already occupies the target session.
    /// 目标会话已有其他流或准备所有者，占用期间等待后续边界。
    DeferredWhileBusy,
}

/// Typed result of the collaboration execution admission check.
/// 协作执行准入检查产生的类型化结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CollaborationExecutionAdmission {
    /// Purpose admitted for the physical execution.
    /// 本次物理执行获准使用的用途。
    pub(crate) purpose: CollaborationExecutionPurpose,
    /// Whether the execution may wake a retained parent turn without changing Goal state.
    /// 本次执行是否可以唤醒保留的父回合但不改变 Goal 状态。
    pub(crate) wakes_retained_parent_turn: bool,
    /// Whether this execution is forbidden from creating another business-task completion.
    /// 本次执行是否禁止再次创建业务任务完成事实。
    pub(crate) suppresses_business_completion: bool,
}

/// Admits one collaboration execution against the durable typed run-control state.
/// 根据可靠的强类型运行控制状态准入一次协作执行。
///
/// The control state is the only source used here. Backend activity, UI phase, and
/// in-memory task counts are intentionally not consulted because they cannot reject
/// a late recovery after a stop or prevent a second completion of a naturally idle task.
/// 这里只使用控制状态。故意不读取后端活动、UI 阶段或内存任务计数，因为这些状态
/// 无法可靠阻止停止后的迟到恢复，也无法阻止自然空闲任务被二次完成。
pub(crate) fn admit_collaboration_execution(
    run_control: CollaborationRunControl,
    purpose: CollaborationExecutionPurpose,
) -> Result<CollaborationExecutionAdmission, String> {
    let admission = match (run_control, purpose) {
        (CollaborationRunControl::Running, CollaborationExecutionPurpose::NormalTask) => {
            CollaborationExecutionAdmission {
                purpose,
                wakes_retained_parent_turn: false,
                suppresses_business_completion: false,
            }
        }
        (CollaborationRunControl::Running, CollaborationExecutionPurpose::InTurnQuestion)
        | (CollaborationRunControl::MainOnly, CollaborationExecutionPurpose::InTurnQuestion) => {
            CollaborationExecutionAdmission {
                purpose,
                wakes_retained_parent_turn: true,
                suppresses_business_completion: true,
            }
        }
        (
            CollaborationRunControl::NaturallyIdle,
            CollaborationExecutionPurpose::NaturallyIdleQuestion,
        ) => CollaborationExecutionAdmission {
            purpose,
            wakes_retained_parent_turn: false,
            suppresses_business_completion: true,
        },
        (CollaborationRunControl::Stopped, _) => {
            return Err(String::from(
                "The collaboration target is stopped and cannot accept new execution.",
            ));
        }
        (CollaborationRunControl::MainAndSubagents, _) => {
            return Err(String::from(
                "The collaboration target stopped main and child execution and rejects late recovery.",
            ));
        }
        (CollaborationRunControl::NaturallyIdle, _) => {
            return Err(String::from(
                "The collaboration target is naturally idle and accepts only one explicit QA execution.",
            ));
        }
        (CollaborationRunControl::MainOnly, _) => {
            return Err(String::from(
                "The collaboration target stopped main execution and accepts only a valid in-turn question.",
            ));
        }
        (CollaborationRunControl::Running, _) => {
            return Err(String::from(
                "The requested collaboration execution purpose is not valid while the target is running.",
            ));
        }
    };
    Ok(admission)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies naturally idle sessions allow exactly one QA-only purpose.
    /// 验证自然空闲会话只允许一次 QA 专用用途。
    #[test]
    fn naturally_idle_admits_only_qa() {
        let admission = admit_collaboration_execution(
            CollaborationRunControl::NaturallyIdle,
            CollaborationExecutionPurpose::NaturallyIdleQuestion,
        )
        .expect("naturally idle QA should be admitted");
        assert!(admission.suppresses_business_completion);
        assert!(
            admit_collaboration_execution(
                CollaborationRunControl::NaturallyIdle,
                CollaborationExecutionPurpose::NormalTask,
            )
            .is_err()
        );
    }

    /// Verifies MainOnly retains the parent turn for valid in-turn questions.
    /// 验证 MainOnly 为合法回合内问题保留父回合。
    #[test]
    fn main_only_admits_in_turn_question_without_goal_resume() {
        let admission = admit_collaboration_execution(
            CollaborationRunControl::MainOnly,
            CollaborationExecutionPurpose::InTurnQuestion,
        )
        .expect("MainOnly should admit a valid in-turn question");
        assert!(admission.wakes_retained_parent_turn);
        assert!(admission.suppresses_business_completion);
    }

    /// Verifies explicit stop modes reject every late recovery purpose.
    /// 验证明确停止模式拒绝所有迟到恢复用途。
    #[test]
    fn stopped_modes_reject_late_recovery() {
        for state in [
            CollaborationRunControl::Stopped,
            CollaborationRunControl::MainAndSubagents,
        ] {
            assert!(admit_collaboration_execution(
                state,
                CollaborationExecutionPurpose::InTurnQuestion,
            )
            .is_err());
            assert!(
                admit_collaboration_execution(
                    state,
                    CollaborationExecutionPurpose::NaturallyIdleQuestion,
                )
                .is_err()
            );
        }
    }
}
