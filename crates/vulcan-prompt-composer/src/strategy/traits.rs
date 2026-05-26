use crate::composer::request::PromptAssemblyInput;
use crate::ids::PromptStrategyId;
use crate::strategy::PromptAssemblyPlan;

/// Prompt assembly strategy contract.
/// 提示词装配策略契约。
pub trait PromptAssemblyStrategy {
    /// Return the stable strategy identifier.
    /// 返回稳定策略标识。
    fn id(&self) -> PromptStrategyId;

    /// Build one ordered node plan from the input.
    /// 基于输入构建一个有序节点计划。
    fn build_plan(&self, input: &PromptAssemblyInput) -> PromptAssemblyPlan;
}
