//! Typed review policy and redacted presentation facts for the `PreCheck` command.
//! `PreCheck` 命令的强类型审核策略和脱敏展示事实，不承载权威持久记忆记录。

use serde::{Deserialize, Serialize};

use crate::{
    MemoryCategory, MemoryId, MemoryScopeClass, MemorySourceKind, ScoreExplanation, TurnId,
    UnixNanos,
};

/// Matches the closed L2 output contract's maximum one-based candidate number.
/// 与封闭 L2 输出契约允许的最大一基候选编号一致。
pub const MAX_PRECHECK_REVIEW_CANDIDATES: u32 = 256;

/// Separates query recall capacity and reviewer admission from the command's final accepted-memory limit.
/// 将查询召回容量和送审准入与命令最终采纳数量上限分离。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreCheckPolicy {
    /// Limits the complete retrieval result for each independent query.
    /// 限制每个独立查询的完整检索结果数量。
    pub recall_top_k: u32,
    /// Limits the merged, redacted candidate set sent to L2 review.
    /// 限制跨组合并并脱敏后送入 L2 审核的候选集合。
    pub review_candidate_limit: u32,
    /// Gates each query-local reviewer score after rank projection, not physical producer scores.
    /// 在各查询审核排名投影后筛选评分，不筛选物理生产者分值。
    pub min_review_score: f64,
}

impl Default for PreCheckPolicy {
    /// Uses the source VMM defaults of five hits per query, twelve reviewer candidates and a 0.75 score floor.
    /// 使用来源 VMM 的默认值：每查询五条、送审十二条、审核评分下限 0.75。
    fn default() -> Self {
        Self {
            recall_top_k: 5,
            review_candidate_limit: 12,
            min_review_score: 0.75,
        }
    }
}

/// Carries a redacted reviewer/context view without exposing or rewriting the authoritative storage record.
/// 携带脱敏审核及上下文视图，不暴露或改写权威存储记录；强化版本由引擎私有原记录提供。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreCheckReviewCandidate {
    /// Identifies the exact memory selected by L2 and lifecycle reinforcement.
    /// 标识 L2 选择及生命周期强化对应的精确记忆。
    pub memory_id: MemoryId,
    /// Retains the presentation source turn when it is known.
    /// 在已知时保留展示来源轮次。
    pub source_turn_id: Option<TurnId>,
    /// Carries the source-compatible presentation creation time.
    /// 携带与来源规则兼容的展示创建时刻。
    pub created_at_ns: UnixNanos,
    /// Describes the presentation origin without granting write authority.
    /// 描述展示来源，不授予写入权限。
    pub source_kind: MemorySourceKind,
    /// Carries the generic scope class needed by reviewer formatting.
    /// 携带审核格式化所需的通用作用域类别。
    pub scope_class: MemoryScopeClass,
    /// Carries the selected presentation category.
    /// 携带选定的展示类别。
    pub category: MemoryCategory,
    /// Contains the merged, redacted summary rather than a rewritten database field.
    /// 包含合并并脱敏后的摘要，不是改写后的数据库字段。
    pub summary: String,
    /// Contains the merged, redacted details used by review and final context assembly.
    /// 包含审核和最终上下文组装使用的合并脱敏详情。
    pub details: String,
    /// Carries the query-local projected reviewer score chosen during duplicate merging.
    /// 携带重复合并时选定的查询局部投影审核分。
    pub score: f64,
    /// Preserves score provenance and redacted context labels without copying raw memory prose or embeddings.
    /// 保留评分来源及脱敏情境标签，不复制原始记忆正文或嵌入。
    pub explanation: ScoreExplanation,
}
