//! Typed local compaction summaries.
//! 类型化本地压缩摘要。

use serde::{Deserialize, Serialize};

/// Persisted context payload produced by a completed local summary.
/// 由已完成本地摘要产生的持久上下文载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompactionContent {
    /// Exact completed local model summary, also present in its source event.
    /// 精确完成的本地模型摘要，同时存在于其来源事件。
    Summary {
        /// Unmodified semantic text used by the next context generation.
        /// 下一上下文代次使用的未修改语义文本。
        text: String,
    },

}
