//! Immutable business results bound to accepted agent task messages.
//! 绑定已接纳代理任务消息的不可变业务结果。

use super::*;
use crate::v1::event::TurnOutcome;
use crate::v1::ids::ApiCallId;

/// One completed assistant block copied exactly from its authoritative source event.
/// 从权威来源事件精确复制的一个已完成助手文本块。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskResultText {
    /// Original target event sequence, preserving block order.
    /// 原目标事件序号，保留文本块顺序。
    pub event_seq: Seq,
    /// Complete original text, independent of later message-envelope size limits.
    /// 完整原始文本，独立于后续消息信封大小限制。
    pub text: String,
}

/// Proven final business response, distinct from reasoning, tool output and auxiliary QA.
/// 已证明的最终业务回复，与推理、工具输出及辅助答疑区分。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskFinalAssistant {
    /// Exact completed business API owning the returned text snapshot.
    /// 拥有回传正文快照的精确已完成业务 API。
    pub api_call_id: ApiCallId,
    /// Target API terminal statistic proving completion.
    /// 证明完成的目标 API 终态统计。
    pub api_stats_seq: Seq,
    /// The API's unique completed assistant snapshot, preserved without truncation.
    /// API 唯一的已完成助手快照，不截断保存。
    pub source_text: CollaborationTaskResultText,
}

/// Business terminal facts frozen independently of the currently active target turn.
/// 独立于目标当前活动回合冻结的业务终态事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskTerminal {
    /// Original claimed or steered business turn.
    /// 原认领或引导的业务回合。
    pub turn_id: TurnId,
    /// Authoritative start sequence from that turn's terminal statistic.
    /// 来自该回合终态统计的权威起始序号。
    pub turn_start_seq: Seq,
    /// Unique terminal statistic proving that this business turn ended.
    /// 证明此业务回合已经结束的唯一终态统计。
    pub turn_stats_seq: Seq,
    /// Exact host outcome; stopped or abandoned work does not fabricate assistant text.
    /// 精确宿主结果；停止或放弃的任务不伪造助手文本。
    pub outcome: TurnOutcome,
    /// Final business response exists only for successful completion.
    /// 仅成功完成时存在最终业务回复。
    pub final_assistant: Option<CollaborationTaskFinalAssistant>,
    /// Actual closure timestamp from the target statistic event.
    /// 来自目标统计事件的实际闭合时间戳。
    pub completed_at_ms: i64,
}

/// Target-owned immutable result with all original task-binding proofs.
/// 携带全部原任务绑定凭据的目标所有不可变结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskResult {
    /// Original acceptance freezes both participants and requested semantics.
    /// 原接纳冻结双方参与者及请求语义。
    pub acceptance: CollaborationTaskMessageAcceptance,
    /// Original effect freezes the exact turn and optional queue execution identity.
    /// 原效果冻结精确回合及可选的排队执行身份。
    pub effect: CollaborationTaskMessageEffect,
    /// Actual task outcome and original assistant response when successful.
    /// 实际任务结果及成功时的原始助手回复。
    pub terminal: CollaborationTaskTerminal,
    /// Stable target operation that recorded this result.
    /// 记录此结果的稳定目标操作。
    pub operation_id: CollaborationOperationId,
    /// Target revision publishing the immutable result.
    /// 发布不可变结果的目标修订。
    pub target_revision: u64,
    /// Target host event containing this result.
    /// 包含此结果的目标宿主事件。
    pub event_seq: Seq,
    /// Host recording timestamp, separate from business closure time.
    /// 宿主记录时间戳，与业务闭合时间区分。
    pub recorded_at_ms: i64,
}

impl CollaborationTaskResult {
    /// Validates exact binding and ordered proof coordinates without reading another session.
    /// 校验精确绑定与有序凭据坐标，不读取其他会话。
    /// Returns an error for delivery-only failures or inconsistent final response facts.
    /// 投递阶段失败或最终回复事实不一致时返回错误。
    pub fn validate(&self) -> Result<(), DomainError> {
        self.effect.validate(&self.acceptance)?;
        validate_timestamp(
            "collaboration.task_result.recorded_at_ms",
            self.recorded_at_ms,
        )?;
        validate_timestamp(
            "collaboration.task_result.completed_at_ms",
            self.terminal.completed_at_ms,
        )?;
        // The effect alone supplies turn ownership; a receipt never borrows a newer active task.
        // 仅由效果提供回合归属；回执绝不借用较新的活动任务。
        let (turn_id, input_seq) = match &self.effect.outcome {
            CollaborationTaskMessageEffectKind::Claimed {
                turn_id, input_seq, ..
            }
            | CollaborationTaskMessageEffectKind::Steered {
                turn_id, input_seq, ..
            } => (*turn_id, *input_seq),
            CollaborationTaskMessageEffectKind::Failed { .. } => {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.no_business_effect",
                });
            }
        };
        if self.terminal.turn_id != turn_id
            || self.terminal.turn_start_seq > input_seq
            || (matches!(
                self.effect.outcome,
                CollaborationTaskMessageEffectKind::Claimed { .. }
            ) && self.terminal.turn_start_seq != self.effect.event_seq)
            || self.terminal.turn_stats_seq <= self.effect.event_seq
            || self.terminal.turn_stats_seq <= input_seq
            || self.event_seq <= self.terminal.turn_stats_seq
            || self.target_revision < self.effect.target_revision
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.binding",
            });
        }
        match (&self.terminal.outcome, &self.terminal.final_assistant) {
            (TurnOutcome::Completed, Some(assistant)) => {
                if assistant.api_stats_seq >= self.terminal.turn_stats_seq
                    || assistant.source_text.event_seq <= self.terminal.turn_start_seq
                    || assistant.source_text.event_seq <= input_seq
                    || assistant.source_text.event_seq >= assistant.api_stats_seq
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.task_result.final_assistant",
                    });
                }
            }
            (TurnOutcome::Stopped | TurnOutcome::Abandoned, None) => {}
            _ => {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.outcome",
                });
            }
        }
        Ok(())
    }
}
