use crate::composer::request::PromptAssemblyInput;
use crate::ids::PromptStrategyId;
use crate::strategy::traits::PromptAssemblyStrategy;
use crate::strategy::{PlannedNode, PromptAssemblyPlan};

/// Clone-style subagent strategy.
/// 分身式子代理策略。
#[derive(Debug, Default)]
pub struct SubagentCloneStrategy;

impl PromptAssemblyStrategy for SubagentCloneStrategy {
    /// Return the clone strategy identifier.
    /// 返回分身策略标识。
    fn id(&self) -> PromptStrategyId {
        "subagent_clone"
    }

    /// Build the clone-mode ordered node plan.
    /// 构建分身模式的有序节点计划。
    fn build_plan(&self, _input: &PromptAssemblyInput) -> PromptAssemblyPlan {
        PromptAssemblyPlan {
            nodes: vec![
                PlannedNode::ProviderBaseSystem,
                PlannedNode::AgentBaseSystem,
                PlannedNode::Personality,
                PlannedNode::SubagentIdentity,
                PlannedNode::ToolHelpBundle,
                PlannedNode::ProjectRules,
                PlannedNode::EnvironmentContext,
                PlannedNode::TaskGoal,
            ],
        }
    }
}
