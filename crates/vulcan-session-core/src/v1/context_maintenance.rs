//! Durable ownership of an auxiliary context-maintenance execution.
//! 辅助上下文维护执行的可靠归属。

use super::{
    collaboration::CollaborationExecutionId,
    errors::DomainError,
    event::TurnClosureReason,
    ids::{CompactionId, Seq, TurnId},
};
use serde::{Deserialize, Serialize};

/// Terminal maintenance fact, independent from the original business outcome.
/// 维护终态事实，独立于原业务结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextMaintenanceTerminal {
    /// Explicit auxiliary closure classification.
    /// 明确辅助闭合分类。
    pub reason: TurnClosureReason,
    /// Exact committed summary node; present only for successful maintenance.
    /// 精确已提交摘要节点；仅成功维护时存在。
    pub compaction_id: Option<CompactionId>,
    /// Unique terminal statistics sequence of the auxiliary turn.
    /// 辅助回合的唯一终态统计序号。
    pub end_seq: Seq,
    /// Actual completion timestamp in milliseconds.
    /// 实际完成时间戳，单位毫秒。
    pub completed_at_ms: i64,
}

/// Latest maintenance execution and its exact completed-business source.
/// 最近维护执行及其精确已完成业务来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextMaintenanceExecution {
    /// Unique auxiliary execution identity, distinct from the business execution.
    /// 唯一辅助执行身份，与业务执行区分。
    pub execution_id: CollaborationExecutionId,
    /// Original completed business identity.
    /// 原已完成业务身份。
    pub business_execution_id: CollaborationExecutionId,
    /// Original completed business turn, never reopened by maintenance.
    /// 原已完成业务回合，绝不被维护重开。
    pub business_turn_id: TurnId,
    /// Separate physical API owner allocated for this auxiliary execution.
    /// 为本辅助执行分配的独立物理 API 所有者。
    pub turn_id: TurnId,
    /// Exact child history generation owning this execution.
    /// 拥有此执行的精确子历史代次。
    pub segment_generation: u64,
    /// Durable source revision verified before admission.
    /// 准入前核验的可靠来源修订。
    pub source_revision: u64,
    /// Compaction pointer preceding this maintenance attempt.
    /// 本次维护尝试之前的压缩指针。
    pub source_compaction_id: Option<CompactionId>,
    /// Immutable HostOnly start anchor.
    /// 不可变 HostOnly 起始锚点。
    pub started_seq: Seq,
    /// Admission timestamp in milliseconds.
    /// 准入时间戳，单位毫秒。
    pub started_at_ms: i64,
    /// Present only after exact auxiliary closure has been published.
    /// 仅在精确辅助闭合已发布后存在。
    pub terminal: Option<ContextMaintenanceTerminal>,
}

impl ContextMaintenanceExecution {
    /// Validates intrinsic identity and terminal ordering against `generation`; returns explicit malformed-state errors.
    /// 根据 `generation` 校验固有身份及终态顺序；畸形状态返回明确错误。
    pub fn validate(&self, generation: u64) -> Result<(), DomainError> {
        if self.segment_generation != generation
            || generation == 0
            || self.source_revision == 0
            || self.execution_id == self.business_execution_id
            || self.turn_id <= self.business_turn_id
            || self.started_at_ms < 0
            || self.terminal.as_ref().is_some_and(|terminal| {
                terminal.end_seq <= self.started_seq
                    || terminal.completed_at_ms < self.started_at_ms
                    || (terminal.reason == TurnClosureReason::Completed)
                        != terminal.compaction_id.is_some()
                    || terminal
                        .compaction_id
                        .is_some_and(|id| Some(id) == self.source_compaction_id)
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "context_maintenance.record",
            });
        }
        Ok(())
    }
}
