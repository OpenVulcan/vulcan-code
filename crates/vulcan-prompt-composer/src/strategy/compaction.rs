use crate::composer::request::PromptAssemblyInput;
use crate::ids::PromptStrategyId;
use crate::strategy::PromptAssemblyPlan;
use crate::strategy::traits::PromptAssemblyStrategy;

/// Minimal internal context-compaction strategy used after pruning is required.
/// 在必须裁剪后使用的极简内部上下文压缩策略。
#[derive(Debug, Default)]
pub struct MinimalContextCompactionStrategy;

impl PromptAssemblyStrategy for MinimalContextCompactionStrategy {
    /// Return the minimal context-compaction strategy identifier.
    /// 返回极简上下文压缩策略标识。
    fn id(&self) -> PromptStrategyId {
        "context-compaction-minimal"
    }

    /// Build the minimal context-compaction ordered node plan.
    /// 构建极简上下文压缩的有序节点计划。
    fn build_plan(&self, _input: &PromptAssemblyInput) -> PromptAssemblyPlan {
        PromptAssemblyPlan { nodes: Vec::new() }
    }
}
