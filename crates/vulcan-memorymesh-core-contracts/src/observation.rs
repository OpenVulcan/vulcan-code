//! Defines same-observation candidate facts without prescribing a database or SQL implementation.
//! 本文件属于公共契约层，定义同次观察候选事实，不指定数据库或SQL实现。

use crate::{
    CandidateQueryBoundary, MemoryRecord, PhysicalCandidate, ScopeAccess, ScoreContractRef,
    SubjectId, UnixNanos, VectorSpaceId,
};
use serde::{Deserialize, Serialize};

/// Selects physical-read consistency independently of mathematical fusion policy.
/// 独立于数学融合策略选择物理读取一致性，不按数据库名称推断。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum CandidateReadMode {
    /// Reads vector, optional lexical and materialized records through separate effects.
    /// 通过独立效果读取向量、可选词法及物化记录。
    SeparateChannels = 1,
    /// Requires both physical channels and their complete record union in one observation.
    /// 要求两物理通道及完整记录并集属于同一次观察。
    SingleObservation = 2,
}

crate::closed_discriminant!(CandidateReadMode { SeparateChannels = 1, SingleObservation = 2 });

/// Requests exactly one query's physical channels and records as an indivisible observation.
/// 请求恰好一条查询的物理通道及记录，作为不可分割的观察。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateObservationQuery {
    /// Identifies the authenticated semantic subject.
    /// 标识已认证语义主体。
    pub subject_id: SubjectId,
    /// Carries the resolved visibility boundary, not a database ownership inference.
    /// 携带已解析可见性边界，不推断数据库归属。
    pub scope_access: ScopeAccess,
    /// Retains the complete normalized text for this query only.
    /// 仅保留本条查询的完整规范文本。
    pub query: String,
    /// Retains this query's accepted embedding without renormalization.
    /// 保留本条查询已接受的嵌入，不重新归一化。
    pub query_embedding: Vec<f32>,
    /// Identifies the exact embedding space.
    /// 标识精确嵌入空间。
    pub vector_space_id: VectorSpaceId,
    /// Restricts candidate and current-session visibility before physical truncation.
    /// 在物理截断前限制候选及当前会话可见性。
    pub boundary: CandidateQueryBoundary,
    /// Bounds each channel independently, not the size of their unique union.
    /// 独立约束每个通道，不将单路上限误用作唯一并集上限。
    pub limit: u32,
    /// Binds the required vector producer identity.
    /// 绑定必需向量生产者身份。
    pub vector_contract: ScoreContractRef,
    /// Binds the lexical producer or explicitly disables that channel when absent.
    /// 绑定词法生产者，缺失时明确禁用该通道。
    pub lexical_contract: Option<ScoreContractRef>,
    /// Carries Core's business evaluation time, not an implicit database wall clock.
    /// 携带Core业务评估时间，不冒充隐式数据库墙上时钟。
    pub now_ns: UnixNanos,
}

/// Preserves a physically ordered channel, including a successfully observed empty result.
/// 保留物理有序通道，包含成功观察到的空结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicalCandidateChannel {
    /// Contains the exact raw score facts in source-rank order.
    /// 按来源名次顺序保存精确原始评分事实。
    pub candidates: Vec<PhysicalCandidate>,
}

/// Returns an all-or-error physical observation whose records exactly cover the channel union.
/// 返回全有或全无物理观察，记录必须精确覆盖通道并集。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateObservation {
    /// Carries the required vector channel; an empty channel is still an observed result.
    /// 携带必需向量通道；空通道仍是已观察结果。
    pub vector_channel: PhysicalCandidateChannel,
    /// Carries an enabled lexical channel; absence means disabled, not failed or empty.
    /// 携带已启用词法通道；缺失表示禁用，不表示失败或空结果。
    pub lexical_channel: Option<PhysicalCandidateChannel>,
    /// Contains every unique candidate record once, with no missing or unrelated rows.
    /// 对每个唯一候选只保存一条记录，不允许缺失或无关行。
    pub memories: Vec<MemoryRecord>,
}
