//! VMM-aligned retention policy snapshots and strongly typed maintenance plans.
//! 与 VMM 对齐的保留策略快照与强类型维护计划。

use crate::{MemoryAccessIdentity, MutationIdentity, ProfileNodeId, ScopeId, SubjectId};
use serde::{Deserialize, Serialize};

/// Carries the complete host-agnostic retention policy evaluated by Core.
/// 携带由 Core 评估的完整宿主无关保留策略。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Identifies the closed policy schema understood by this Core version.
    /// 标识当前 Core 版本理解的封闭策略 Schema。
    pub schema_version: u32,
    /// Counts turns retained by the effective `PostAction` plus retention hot window.
    /// 统计由 `PostAction` 与 retention 组合形成的有效热窗口保留轮次数。
    pub turn_hot_window_size: u32,
    /// Carries the session idle duration before idle-session evaluation.
    /// 携带会话进入 idle-session 评估前的空闲时长。
    pub session_idle_ms: u64,
    /// Defines the expiry window for newly recycled batches, not existing batch deadlines.
    /// 定义新回收批次的到期窗口，不改变已有批次的截止时间。
    pub trash_retention_ms: u64,
    /// Carries the lowest protected priority on the normalized zero-to-one-hundred scale.
    /// 携带零到一百规范尺度上的最低受保护优先级。
    pub protected_priority_floor: u32,
    /// Carries the lowest protected memory-level discriminant.
    /// 携带最低受保护记忆等级判别值。
    pub protected_memory_level_floor: u32,
    /// States whether shared terminal memories meeting either protection floor are skipped.
    /// 表示是否跳过满足任一保护下限的共享终态记忆。
    pub skip_protected_shared_memories: bool,
    /// Limits one due active-profile expiry convergence pass; zero disables this family.
    /// 限制一次到期活跃画像收敛批次；零值禁用该计划族。
    pub profile_expiry_limit: u32,
    /// Limits one terminal-memory recycle pass; zero disables this family.
    /// 限制一次终态记忆回收批次；零值禁用该计划族。
    pub memory_recycle_limit: u32,
    /// Limits one independent cold-turn session scan; zero disables this family.
    /// 限制一次独立冷 Turn 会话扫描；零值禁用该计划族。
    pub cold_turn_scan_limit: u32,
    /// Limits one due cold-turn job execution pass; zero disables this family.
    /// 限制一次到期冷 Turn 任务执行批次；零值禁用该计划族。
    pub cold_turn_job_limit: u32,
    /// Limits one idle-session recycle pass; zero disables this family.
    /// 限制一次闲置会话回收批次；零值禁用该计划族。
    pub idle_session_limit: u32,
    /// Limits one due vector-GC retry pass; zero disables this family.
    /// 限制一次到期向量 GC 重试批次；零值禁用该计划族。
    pub vector_gc_limit: u32,
    /// Limits one expired trash-batch purge pass; zero disables this family.
    /// 限制一次过期回收站批次清除；零值禁用该计划族。
    pub trash_purge_limit: u32,
}

/// Names the stable reason code recorded on every maintenance plan.
/// 命名记录在每个维护计划上的稳定原因码。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum RetentionReasonCode {
    /// Indicates terminal durable-memory recycling.
    /// 表示终态持久记忆回收。
    TerminalMemoryRecycle = 1,
    /// Indicates independent cold-turn maintenance.
    /// 表示独立冷 Turn 维护。
    ColdTurnRecycle = 2,
    /// Indicates idle-session compaction.
    /// 表示闲置会话压缩。
    IdleSessionRecycle = 3,
    /// Indicates trash retention expiry purge.
    /// 表示回收站保留到期清除。
    TrashExpiryPurge = 4,
    /// Indicates retry of one due sidecar vector deletion job.
    /// 表示重试一个到期旁路向量删除任务。
    VectorGcRetry = 5,
    /// Indicates deterministic expiry of one due active profile node.
    /// 表示对一个已到期活跃画像节点执行确定性过期收敛。
    ProfileExpiry = 6,
    /// Indicates explicit whole-session recycling rather than idle-session policy cleanup.
    /// 表示显式完整会话回收，不是闲置会话策略清洗。
    ManualSessionRecycle = 7,
    /// Indicates explicit selected-turn recycling without changing the session lifecycle.
    /// 表示显式所选回合回收，不改变会话生命周期。
    ManualTurnRecycle = 8,
}

/// Identifies one exact due active profile node selected for expiry convergence.
/// 标识一个被选择执行过期收敛的精确到期活跃画像节点。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedProfileNodeRef {
    /// Identifies the planned profile node.
    /// 标识计划中的画像节点。
    pub profile_node_id: ProfileNodeId,
    /// Identifies the originating subject observed by Core.
    /// 标识 Core 观察到的来源主体。
    pub expected_subject_id: Option<SubjectId>,
    /// Identifies the exact owning scope observed by Core.
    /// 标识 Core 观察到的精确归属作用域。
    pub expected_scope_id: ScopeId,
    /// Carries the required optimistic version.
    /// 携带必需的乐观版本。
    pub expected_version: u64,
    /// Carries the stable reason code for this planned item.
    /// 携带该计划项的稳定原因码。
    pub reason: RetentionReasonCode,
}

/// Identifies one exact memory and its required optimistic version inside a plan.
/// 标识计划内一个精确记忆及其必需乐观版本。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedMemoryRef {
    /// Identifies the planned memory.
    /// 标识计划记忆。
    pub memory_id: String,
    /// Carries the required current version.
    /// 携带必需当前版本。
    pub expected_version: u64,
    /// Carries the immutable access identity required when the plan is applied.
    /// 携带应用计划时所需的不可变访问身份。
    pub expected_access: MemoryAccessIdentity,
    /// Carries the stable reason code for this planned item.
    /// 携带该计划项的稳定原因码。
    pub reason: RetentionReasonCode,
}

/// Identifies one exact host session inside a retention plan.
/// 标识保留计划内一个精确宿主会话。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedSessionRef {
    /// Identifies the host session source.
    /// 标识宿主会话来源。
    pub session_source_id: String,
    /// Carries the stable reason code for this planned item.
    /// 携带该计划项的稳定原因码。
    pub reason: RetentionReasonCode,
}

/// Identifies one exact persisted cold-turn job selected for execution.
/// 标识一个被选中执行的精确持久冷 Turn 任务。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedColdTurnJobRef {
    /// Identifies the backend-owned durable job.
    /// 标识由后端拥有的持久任务。
    pub job_id: String,
    /// Identifies the session whose old unreferenced turns are processed.
    /// 标识处理其旧无引用 Turn 的会话。
    pub session_source_id: String,
    /// Carries the stable reason code for this planned item.
    /// 携带该计划项的稳定原因码。
    pub reason: RetentionReasonCode,
}

/// Identifies one exact persisted vector-GC retry job.
/// 标识一个精确持久向量 GC 重试任务。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedVectorGcRef {
    /// Identifies the backend-owned durable job.
    /// 标识由后端拥有的持久任务。
    pub job_id: String,
    /// Identifies the obsolete sidecar vector.
    /// 标识过时旁路向量。
    pub vector_id: String,
    /// Carries the stable reason code for this planned item.
    /// 携带该计划项的稳定原因码。
    pub reason: RetentionReasonCode,
}

/// Identifies one exact recycle batch eligible for permanent purge.
/// 标识一个可永久清除的精确回收批次。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedTrashBatchRef {
    /// Identifies the backend-owned recycle batch.
    /// 标识由后端拥有的回收批次。
    pub batch_id: String,
    /// Carries the stable reason code for this planned item.
    /// 携带该计划项的稳定原因码。
    pub reason: RetentionReasonCode,
}

/// Carries the complete strongly typed maintenance plan set for one retention evaluation.
/// 携带一次保留策略评估的完整强类型维护计划集合。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPlanSet {
    /// Carries the persistent authorization and reconciliation identity for this plan set.
    /// 携带该计划集的持久授权与对账身份。
    pub identity: MutationIdentity,
    /// Identifies the subject whose facts produced this plan set.
    /// 标识产生该计划集事实的主体。
    pub subject_id: SubjectId,
    /// Identifies the exact write scope governed by this plan set.
    /// 标识该计划集管理的精确写入作用域。
    pub scope_id: ScopeId,
    /// Carries due active profile nodes to materialize as expired audit rows.
    /// 携带需要真实收敛为过期审计行的到期活跃画像节点。
    pub profile_expiry: Vec<PlannedProfileNodeRef>,
    /// Carries terminal durable memories to move out of the hot tables.
    /// 携带需要迁出热表的终态持久记忆。
    pub terminal_memory_recycle: Vec<PlannedMemoryRef>,
    /// Carries sessions that need independent cold-turn jobs enqueued.
    /// 携带需要入队独立冷 Turn 任务的会话。
    pub cold_turn_enqueue: Vec<PlannedSessionRef>,
    /// Carries due persisted cold-turn jobs selected for execution.
    /// 携带被选中执行的到期持久冷 Turn 任务。
    pub cold_turn_execute: Vec<PlannedColdTurnJobRef>,
    /// Carries long-idle sessions selected for compaction.
    /// 携带被选中压缩的长期闲置会话。
    pub idle_session_recycle: Vec<PlannedSessionRef>,
    /// Carries due persisted sidecar vector deletion retries.
    /// 携带到期的持久旁路向量删除重试。
    pub vector_gc_retry: Vec<PlannedVectorGcRef>,
    /// Carries recycle batches whose finite trash window has expired.
    /// 携带有限回收站窗口已过期的回收批次。
    pub trash_purge: Vec<PlannedTrashBatchRef>,
}

impl RetentionPolicy {
    /// Returns the frozen first-release v1 retention policy aligned with VMM defaults.
    /// 返回与 VMM 默认值对齐的首次发布 v1 保留策略。
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            schema_version: 1,
            turn_hot_window_size: 8,
            session_idle_ms: 1_296_000_000,
            trash_retention_ms: 2_592_000_000,
            // VMM P1 maps to the native normalized priority value 60.
            // VMM 的 P1 映射到原生规范化优先级数值 60。
            protected_priority_floor: 60,
            protected_memory_level_floor: 3,
            skip_protected_shared_memories: true,
            profile_expiry_limit: 256,
            memory_recycle_limit: 128,
            cold_turn_scan_limit: 64,
            cold_turn_job_limit: 16,
            idle_session_limit: 32,
            vector_gc_limit: 128,
            trash_purge_limit: 64,
        }
    }
}
