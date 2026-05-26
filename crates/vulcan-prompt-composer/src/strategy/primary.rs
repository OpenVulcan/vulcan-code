use crate::composer::request::PromptAssemblyInput;
use crate::ids::PromptStrategyId;
use crate::strategy::traits::PromptAssemblyStrategy;
use crate::strategy::{PlannedNode, PromptAssemblyPlan};

/// Primary/root agent strategy.
/// 主代理根策略。
#[derive(Debug, Default)]
pub struct PrimaryAgentStrategy;

impl PromptAssemblyStrategy for PrimaryAgentStrategy {
    /// Return the primary strategy identifier.
    /// 返回主策略标识。
    fn id(&self) -> PromptStrategyId {
        "primary"
    }

    /// Build the primary ordered node plan.
    /// 构建主代理的有序节点计划。
    fn build_plan(&self, _input: &PromptAssemblyInput) -> PromptAssemblyPlan {
        PromptAssemblyPlan {
            nodes: vec![
                PlannedNode::ProviderBaseSystem,
                PlannedNode::AgentBaseSystem,
                PlannedNode::Personality,
                PlannedNode::ToolHelpBundle,
                PlannedNode::ProjectRules,
                PlannedNode::EnvironmentContext,
                PlannedNode::TaskGoal,
            ],
        }
    }
}
