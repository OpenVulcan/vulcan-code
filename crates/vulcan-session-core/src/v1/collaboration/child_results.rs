//! Durable non-success facts from an owned child execution.
//! 所属子执行的可靠非成功事实。

use super::*;
use crate::v1::event::TurnOutcome;

/// Exact permanent diagnostic copied from the failed child's business turn.
/// 从失败子任务业务回合复制的精确永久诊断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationChildTaskFailureNote {
    /// Original child display-note event.
    /// 原子文档提示事件。
    pub event_seq: Seq,
    /// Host failure code retained without guessing from natural-language text.
    /// 保留的宿主失败码，不从自然语言文本猜测。
    pub reason_code: String,
    /// Producer-redacted original detail, if supplied.
    /// 生产端提供的原始脱敏详情（若存在）。
    pub redacted_detail: Option<String>,
}

/// Frozen non-success source proof built only from a durable canonical child.
/// 仅从可靠规范子文档构造的冻结非成功来源凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationChildTaskNonSuccessProof {
    /// Exact source session.
    /// 精确来源会话。
    pub child_session_id: String,
    /// Source generation captured when the child result was built.
    /// 构造子结果时捕获的来源代次。
    pub child_segment_generation: u64,
    /// Original execution proven by the child root input action.
    /// 由子根输入动作证明的原执行。
    pub execution_id: CollaborationExecutionId,
    /// Exact original business turn.
    /// 精确原业务回合。
    pub child_turn_id: TurnId,
    /// Original root input event.
    /// 原根输入事件。
    pub child_start_seq: Seq,
    /// Unique business terminal statistic.
    /// 唯一业务终态统计。
    pub child_terminal_seq: Seq,
    /// Source closure retained independently of the runtime result classification.
    /// 独立于运行时结果分类保留的来源闭合结果。
    pub child_turn_outcome: TurnOutcome,
    /// Authenticated host result; successful work uses the separate exact content contract.
    /// 已认证宿主结果；成功任务使用独立精确正文契约。
    pub outcome: CollaborationChildTaskTerminalOutcome,
    /// Original failure diagnostics within this exact business turn.
    /// 该精确业务回合中的原失败诊断。
    pub failure_notes: Vec<CollaborationChildTaskFailureNote>,
}

/// Parent-owned immutable receipt whose source is a specific child execution.
/// 来源为具体子执行的父侧不可变回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationChildTaskNonSuccessResult {
    /// Complete immutable source facts.
    /// 完整不可变来源事实。
    pub proof: CollaborationChildTaskNonSuccessProof,
    /// Exact parent operation recording this result.
    /// 记录此结果的精确父操作。
    pub operation_id: CollaborationOperationId,
    /// Parent event proving result publication.
    /// 证明结果发布的父事件。
    pub event_seq: Seq,
    /// Atomic parent revision that first publishes this report.
    /// 首次发布此报告的原子父修订。
    pub parent_revision: u64,
}

impl CollaborationChildTaskNonSuccessProof {
    /// Validates source shape without inferring another session's current state.
    /// 校验来源形状，不推断另一会话的当前状态。
    /// Returns an explicit error for successful, unordered, or diagnostically unsupported facts.
    /// 成功、无序或缺乏诊断支持的事实返回明确错误。
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.child_session_id.trim().is_empty()
            || self.child_session_id.trim() != self.child_session_id
            || !(1..=JS_SAFE_INTEGER_MAX).contains(&self.child_segment_generation)
            || self.child_start_seq >= self.child_terminal_seq
            || self.child_turn_outcome == TurnOutcome::Completed
            || self.outcome == CollaborationChildTaskTerminalOutcome::Completed
            || (self.outcome == CollaborationChildTaskTerminalOutcome::Failed
                && self.failure_notes.is_empty())
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.source",
            });
        }
        // Diagnostic ordering and range prevent a report from borrowing another execution's failure.
        // 诊断顺序与范围防止报告借用其他执行的失败。
        let mut previous = self.child_start_seq;
        for note in &self.failure_notes {
            if note.event_seq <= previous
                || note.event_seq >= self.child_terminal_seq
                || note.reason_code.trim().is_empty()
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.failure_note",
                });
            }
            previous = note.event_seq;
        }
        Ok(())
    }
}

impl CollaborationChildTaskNonSuccessResult {
    /// Verifies this receipt against its owning reservation and terminal outcome.
    /// 按所属预留与终态结果核验此回执。
    /// `task` is the canonical parent record; mismatched bindings return an explicit error.
    /// `task` 为规范父记录；绑定不匹配返回明确错误。
    pub fn validate(&self, task: &CollaborationChildTaskRecord) -> Result<(), DomainError> {
        self.proof.validate()?;
        if !(1..=JS_SAFE_INTEGER_MAX).contains(&self.parent_revision)
            || task.state != CollaborationChildTaskState::Terminal
            || task.terminal_outcome != Some(self.proof.outcome)
            || task.execution_id != self.proof.execution_id
            || task.child.session_id != self.proof.child_session_id
            || task.child.segment_generation != self.proof.child_segment_generation
            || task
                .child
                .turn_id
                .is_some_and(|turn| turn != self.proof.child_turn_id)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.reservation",
            });
        }
        Ok(())
    }
}
