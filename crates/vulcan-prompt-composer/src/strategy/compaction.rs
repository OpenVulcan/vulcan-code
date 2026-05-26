use crate::composer::request::PromptAssemblyInput;
use crate::ids::PromptStrategyId;
use crate::strategy::traits::PromptAssemblyStrategy;
use crate::strategy::{PlannedNode, PromptAssemblyPlan};

/// Internal context-compaction strategy.
/// 内部上下文压缩策略。
#[derive(Debug, Default)]
pub struct ContextCompactionStrategy;

impl PromptAssemblyStrategy for ContextCompactionStrategy {
    /// Return the context-compaction strategy identifier.
    /// 返回上下文压缩策略标识。
    fn id(&self) -> PromptStrategyId {
        "context-compaction"
    }

    /// Build the context-compaction ordered node plan.
    /// 构建上下文压缩的有序节点计划。
    fn build_plan(&self, _input: &PromptAssemblyInput) -> PromptAssemblyPlan {
        PromptAssemblyPlan {
            nodes: vec![
                PlannedNode::ProviderBaseSystem,
                PlannedNode::AgentBaseSystem,
                PlannedNode::Personality,
                PlannedNode::ProjectRules,
                PlannedNode::EnvironmentContext,
                PlannedNode::TaskGoal,
            ],
        }
    }
}
