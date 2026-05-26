use crate::composer::request::PromptAssemblyInput;
use crate::ids::PromptStrategyId;
use crate::strategy::traits::PromptAssemblyStrategy;
use crate::strategy::{PlannedNode, PromptAssemblyPlan};

/// Functional-style subagent strategy.
/// 功能式子代理策略。
#[derive(Debug, Default)]
pub struct SubagentFunctionalStrategy;

impl PromptAssemblyStrategy for SubagentFunctionalStrategy {
    /// Return the functional strategy identifier.
    /// 返回功能策略标识。
    fn id(&self) -> PromptStrategyId {
        "subagent_functional"
    }

    /// Build the functional-mode ordered node plan.
    /// 构建功能模式的有序节点计划。
    fn build_plan(&self, _input: &PromptAssemblyInput) -> PromptAssemblyPlan {
        PromptAssemblyPlan {
            nodes: vec![
                PlannedNode::ProviderBaseSystem,
                PlannedNode::AgentBaseSystem,
                PlannedNode::FunctionalAgentSystem,
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
