//! Exact source impact and separately confirmed manual recycle contracts.
//! 准确来源影响与单独确认的人工回收契约。
use serde::{Deserialize, Serialize};

/// Selects one family of exact active-store identities, never display names.
/// 选择一种准确活动存储身份，绝不使用显示名称。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VmmNativeRemovalTarget {
    /// Selects complete memory sessions, including direct writes without source turns.
    /// 选择完整记忆会话，包含没有来源回合的直写。
    Sessions {
        /// Exact session source IDs within the selected owner project.
        /// 所选归属项目内的准确来源会话 ID。
        ids: Vec<String>,
    },
    /// Selects individual turns and records explicitly attributed to them.
    /// 选择独立回合及明确归属于它们的记录。
    Turns {
        /// Exact canonical Core turn IDs within the selected owner project.
        /// 所选归属项目内的准确 Core 规范回合 ID。
        ids: Vec<String>,
    },
}

/// Requests source impact only; it carries no deletion permission or profile policy.
/// 仅请求来源影响，不携带删除权限或画像策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VmmNativeRemovalQuery {
    /// Stable owning memory-project identity.
    /// 稳定归属记忆项目身份。
    pub vmm_project_id: u64,
    /// One to one hundred exact selected identities before deduplication.
    /// 去重前一至一百个准确所选身份。
    pub selection: VmmNativeRemovalTarget,
}

/// Identifies one context edge by its full persisted composite key.
/// 按完整持久复合键标识一条情境边。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRemovalEvidenceIdentity {
    /// Exact owning memory ID.
    /// 准确归属记忆 ID。
    pub memory_id: String,
    /// Stored context key.
    /// 已存储情境键。
    pub context_key: String,
    /// Stored context value, part of the identity rather than mutable display text.
    /// 已存储情境值，属于身份的一部分而非可变显示文本。
    pub context_value: String,
}

/// Reports related source identities without deciding which profiles should be removed.
/// 报告关联来源身份，不决定应该移除哪些画像。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRemovalImpact {
    /// Validated and normalized exact query.
    /// 已校验并归一化的准确查询。
    pub query: VmmNativeRemovalQuery,
    /// Single authoritative snapshot version used by the inspection.
    /// 本次检查使用的单份权威快照版本。
    pub storage_version: u64,
    /// Effective project hot-window size used to identify protected recent turns.
    /// 用于识别受保护近期回合的项目有效热窗口大小。
    pub hot_window_size: u32,
    /// Selected sessions or exact source sessions of selected turns.
    /// 所选会话或所选回合的准确来源会话。
    pub session_source_ids: Vec<String>,
    /// Selected canonical turns, numerically ordered by their native identity.
    /// 所选规范回合，按原生身份的数值排序。
    pub turn_ids: Vec<String>,
    /// Ordinary memories selected by source turn or, for whole sessions, origin session.
    /// 按来源回合选中的普通记忆；完整会话选择时也按来源会话选中。
    pub memory_ids: Vec<String>,
    /// Related project profile candidates, not an authorized deletion set.
    /// 关联项目画像候选，不是已授权删除集合。
    pub project_profile_node_ids: Vec<String>,
    /// Related fixed-user profile candidates, not an authorized deletion set.
    /// 关联固定用户画像候选，不是已授权删除集合。
    pub user_profile_node_ids: Vec<String>,
    /// Context edges belonging only to the selected ordinary memories.
    /// 仅属于所选普通记忆的情境边。
    pub context_evidence: Vec<VmmNativeRemovalEvidenceIdentity>,
    /// Selected turns still awaiting extraction.
    /// 所选且仍等待提炼的回合。
    pub pending_turn_ids: Vec<String>,
    /// Selected turns currently inside each source session's effective hot window.
    /// 所选且当前处于各来源会话有效热窗口内的回合。
    pub hot_window_turn_ids: Vec<String>,
    /// Selected memories declaring a vector generation; not a physical index health assertion.
    /// 声明了向量代次的所选记忆，不作为物理索引健康断言。
    pub vector_memory_ids: Vec<String>,
}

/// Freezes a server-owned source selection; all independent profiles remain active.
/// 冻结服务端来源选择；全部独立画像保持活动存储归属。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRemovalPreview {
    /// Opaque single-use confirmation identity, valid for five minutes.
    /// 不透明单次确认身份，有效期五分钟。
    pub preview_id: String,
    /// Exact observed sources, counts and hot-window impact.
    /// 准确观察到的来源、数量及热窗口影响。
    pub impact: VmmNativeRemovalImpact,
    /// Effective retention duration applied from the actual recycle time.
    /// 从实际回收时刻起应用的有效保留时长。
    pub trash_retention_ms: u64,
}

/// Confirms only a previously prepared selection, without accepting replacement targets.
/// 只确认此前已准备的选择，不接受替换目标。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VmmNativeRemovalConfirmation {
    /// Exact opaque identity returned by the server preview.
    /// 服务端预览返回的准确不透明身份。
    pub preview_id: String,
    /// Explicit destructive-action consent; the service requires true.
    /// 明确破坏性操作同意；服务要求为真。
    pub confirmed: bool,
}

/// Reports the committed original-record move without claiming profile removal.
/// 报告已提交的原记录移动，不声称移除了画像。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRemovalResult {
    /// Storage version after the atomic move.
    /// 原子移动后的存储版本。
    pub storage_version: u64,
    /// Exact restore/purge batch identity.
    /// 准确恢复或清除批次身份。
    pub batch_id: String,
    /// Executed selection and its original observed impact; listed profiles were preserved.
    /// 执行的选择及其原始观察影响；列出的画像已保留。
    pub impact: VmmNativeRemovalImpact,
}
