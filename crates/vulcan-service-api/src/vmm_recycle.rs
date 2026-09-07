//! Typed read-only access to native recycled records, separate from active search results.
//! 原生已回收记录的类型化只读访问，独立于活动检索结果。
use super::VmmNativeTimelineItemSummary;
use serde::{Deserialize, Serialize};

/// Selects one exact recycled record family for bounded inspection.
/// 选择一种准确已回收记录类别以进行有界查看。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VmmNativeRecycleRecordKind {
    /// Retained dialogue turns.
    /// 保留的对话回合。
    Turn,
    /// Retained durable memories.
    /// 保留的持久记忆。
    Memory,
    /// Retained user and project profile nodes.
    /// 保留的用户及项目画像节点。
    Profile,
    /// Retained context evidence rows.
    /// 保留的情境证据行。
    Evidence,
}

/// Requests a page by exact owner and batch; an optional version freezes subsequent pages.
/// 按准确归属及批次请求一页，可选版本用于冻结后续分页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VmmNativeRecycleRecordQuery {
    /// Stable owning memory-project ID.
    /// 稳定归属记忆项目 ID。
    pub vmm_project_id: u64,
    /// Exact persisted batch identity.
    /// 准确持久批次身份。
    pub batch_id: String,
    /// Record family to inspect.
    /// 要查看的记录类别。
    pub record_kind: VmmNativeRecycleRecordKind,
    /// Zero-based offset within the stable identity ordering.
    /// 稳定身份顺序内从零开始的偏移。
    pub offset: u32,
    /// Page size, validated between one and one hundred.
    /// 页大小，校验范围为一至一百。
    pub limit: u32,
    /// None explicitly requests a fresh snapshot; Some requires the exact observed version.
    /// None 明确请求新快照，Some 要求准确观察版本。
    pub expected_storage_version: Option<u64>,
}

/// Carries retained business content only; vector arrays and active-state claims are excluded.
/// 仅携带保留的业务内容，不包含向量数组或活动状态声明。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum VmmNativeRecycleRecord {
    /// Retained turn with complete dialogue and the extraction state saved at recycling.
    /// 保留的回合，包含完整对话及回收时保存的提炼状态。
    Turn {
        /// Exact persisted turn identity.
        /// 准确持久回合身份。
        turn_id: String,
        /// Exact originating session identity.
        /// 准确来源会话身份。
        session_source_id: String,
        /// Original retained dialogue in chronological order.
        /// 按时间顺序排列的原始保留对话。
        timeline: Vec<VmmNativeTimelineItemSummary>,
        /// Stored extraction state, not the containing session lifecycle.
        /// 保存的提炼状态，不是所属会话生命周期。
        status: String,
        /// Retained analysis details when extraction produced them.
        /// 提炼曾产生时保留的分析详情。
        analysis_details: Option<String>,
    },
    /// Retained memory with exact source links and original text, without its embedding.
    /// 保留的记忆及准确来源关系、原文，不含嵌入向量。
    Memory {
        /// Stable memory identity.
        /// 稳定记忆身份。
        memory_id: String,
        /// Persisted source session, absent only when the source did not record one.
        /// 持久来源会话，仅在来源未记录时缺失。
        session_source_id: Option<String>,
        /// Persisted source turn, absent for sources without a turn.
        /// 持久来源回合，不含回合的来源可缺失。
        source_turn_id: Option<String>,
        /// Original abstract and full content.
        /// 原始摘要及完整内容。
        summary: String,
        /// Complete retained memory text.
        /// 完整保留记忆文本。
        content: String,
        /// Saved memory level, lifecycle and generation source.
        /// 保存的记忆级别、生命周期及生成来源。
        level: String,
        /// Saved lifecycle state at recycling.
        /// 回收时保存的生命周期状态。
        state: String,
        /// Persisted generation-source token.
        /// 持久生成来源标记。
        source_kind: String,
    },
    /// Retained user or project profile node with all explicit source-turn links.
    /// 保留的用户或项目画像节点，包含全部明确来源回合关系。
    Profile {
        /// Stable profile-node identity.
        /// 稳定画像节点身份。
        profile_node_id: String,
        /// Exact user/project ownership class.
        /// 准确用户或项目归属类别。
        target: String,
        /// All persisted supporting turn identities.
        /// 全部持久支持回合身份。
        source_turn_ids: Vec<String>,
        /// Retained atomic profile statement.
        /// 保留的原子画像陈述。
        content: String,
    },
    /// Retained contextual support and rebuttal counters keyed by memory and context.
    /// 按记忆及情境标识保留的支持和反驳计数。
    Evidence {
        /// Exact owning memory identity.
        /// 准确归属记忆身份。
        memory_id: String,
        /// Persisted normalized context key and value.
        /// 持久规范化情境键和值。
        context_key: String,
        /// Persisted normalized context value.
        /// 持久规范化情境值。
        context_value: String,
        /// Independent saved support and rebuttal totals.
        /// 独立保存的支持及反驳总数。
        support_count: u32,
        /// Independent saved rebuttal total.
        /// 独立保存的反驳总数。
        rebuttal_count: u32,
    },
}

/// Returns one exact page and the snapshot version that subsequent pagination must preserve.
/// 返回准确一页及后续分页必须保持的快照版本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRecycleRecordPage {
    /// Normalized request that produced this page.
    /// 产生当前页的归一化请求。
    pub query: VmmNativeRecycleRecordQuery,
    /// Actual authoritative snapshot version.
    /// 实际权威快照版本。
    pub storage_version: u64,
    /// Total records in the selected family, independent of page size.
    /// 所选类别的总记录数，不受页大小影响。
    pub total: u64,
    /// Exact next offset when more records exist.
    /// 存在更多记录时的准确下一页偏移。
    pub next_offset: Option<u32>,
    /// Retained records only, never merged with active records.
    /// 仅包含保留记录，不与活动记录合并。
    pub records: Vec<VmmNativeRecycleRecord>,
}
