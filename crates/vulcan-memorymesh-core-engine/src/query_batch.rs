//! Plans source-compatible query reuse without sharing candidate-specific review decisions.
//! 本文件属于引擎私有层，规划同源查询复用，不共享候选专属审核决策。

use crate::algorithms::{normalize_text, simple_lowercase};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Retains first representatives and every caller position for one validated query invocation.
/// 为一次已校验查询调用保留首出现代表和全部调用位置，仅随该调用存活。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
// Keep the crate boundary explicit so future module reorganizations cannot export continuation details.
// 明确保留crate边界，防止未来模块重组意外导出续体细节。
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct QueryBatchPlan {
    /// Supplies the exact normalized representative texts used for embedding and physical reads.
    /// 提供用于嵌入和物理读取的精确归一化代表文本。
    pub(crate) unique_queries: Vec<String>,
    /// Maps each caller entry to its unique representative without changing caller order.
    /// 将每个调用项映射至唯一代表，不改变调用顺序。
    pub(crate) caller_to_unique: Vec<usize>,
}

impl QueryBatchPlan {
    /// Builds a deterministic plan from validated queries; caller limits must be checked before deduplication.
    /// 从已校验查询构建确定性计划；调用项上限必须在去重前检查，返回唯一代表及位置映射。
    pub(crate) fn new(queries: &[String]) -> Self {
        let mut unique_queries = Vec::new();
        let mut caller_to_unique = Vec::with_capacity(queries.len());
        let mut by_key = BTreeMap::new();
        for query in queries {
            let normalized = normalize_text(query);
            // Go uses simple lowercase, not contextual lowercase or Unicode case folding.
            // Go使用简单小写，不使用上下文小写或Unicode大小写折叠。
            let index = *by_key
                .entry(simple_lowercase(&normalized))
                .or_insert_with(|| {
                    let index = unique_queries.len();
                    unique_queries.push(normalized);
                    index
                });
            caller_to_unique.push(index);
        }
        Self {
            unique_queries,
            caller_to_unique,
        }
    }
}
