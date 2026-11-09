//! Durable ownership of an auxiliary context-maintenance execution.
//! 辅助上下文维护执行的可靠归属。

use super::{
    collaboration::CollaborationExecutionId,
    errors::DomainError,
    event::TurnClosureReason,
    ids::{CompactionId, Seq, TurnId},
};
use serde::{Deserialize, Serialize};

/// Opaque in-memory source captured with a durable compaction projection; it is never persisted.
/// 与可靠压缩投影一同捕获的不透明内存来源；绝不持久化。
/// All execution state and event revisions remain exact while composer-only draft edits may proceed.
/// 全部执行状态与事件修订保持精确，同时允许仅输入框草稿的编辑。
#[derive(Debug, Clone, PartialEq)]
pub struct ManualContextMaintenanceSource {
    /// The full metadata source prevents unrelated control or configuration changes from being ignored.
    /// 完整元数据来源防止忽略其他控制或配置变化。
    pub(crate) metadata: super::config::MetadataDocument,
    /// Locked context and committed compaction identity must still match the planned source.
    /// 锁定上下文与已提交压缩身份必须仍匹配规划来源。
    pub(crate) runtime_lock: super::context::RuntimeLock,
    /// Resident event identities and revisions detect in-place changes without copying historical text.
    /// 常驻事件身份及修订检测原位变化，无须复制历史正文。
    pub(crate) event_revisions: Vec<(Seq, u64)>,
}

/// In-memory proof that an explicit manual summary may borrow one idle business turn.
/// 显式手动摘要可借用某个空闲业务回合的内存证明。
/// The host must retain its exact stream owner; this proof grants no business continuation.
/// 宿主必须保留精确流所有者；此证明不授予业务续跑许可。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManualContextCompactionBorrow {
    pub(crate) turn_id: TurnId,
    pub(crate) start_seq: Seq,
    pub(crate) segment_generation: u64,
    pub(crate) after_seq: Seq,
    pub(crate) source_compaction_id: Option<CompactionId>,
    pub(crate) run_control: super::collaboration::CollaborationRunControl,
}

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
    /// Original child execution identity; manual root maintenance has no child execution.
    /// 原子执行身份；手动主会话维护没有子执行身份。
    #[serde(deserialize_with = "super::config::deserialize_required_nullable")]
    pub business_execution_id: Option<CollaborationExecutionId>,
    /// Original completed business turn, never reopened by maintenance.
    /// 原已完成业务回合，绝不被维护重开。
    pub business_turn_id: TurnId,
    /// Separate physical API owner allocated for this auxiliary execution.
    /// 为本辅助执行分配的独立物理 API 所有者。
    pub turn_id: TurnId,
    /// Exact history generation owning this execution.
    /// 拥有此执行的精确历史代次。
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
            || self.business_execution_id.as_ref() == Some(&self.execution_id)
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
