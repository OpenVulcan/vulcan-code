use crate::composer::request::PromptAssemblyInput;
use crate::ids::PromptStrategyId;
use crate::strategy::traits::PromptAssemblyStrategy;
use crate::strategy::{PlannedNode, PromptAssemblyPlan};

/// Dedicated-session strategy without general tools: only the user's base prompt and output style.
/// 无通用工具的专职会话策略：只包含用户基础提示词与输出风格。
#[derive(Debug, Default)]
pub struct DedicatedMinimalStrategy;

impl PromptAssemblyStrategy for DedicatedMinimalStrategy {
    /// Return the minimal dedicated strategy identifier.
    /// 返回极简专职策略标识。
    fn id(&self) -> PromptStrategyId {
        "dedicated_minimal"
    }

    /// Build the minimal dedicated ordered node plan; no workspace or project context is sent.
    /// 构建极简专职的有序节点计划；不发送工作区或项目上下文。
    fn build_plan(&self, _input: &PromptAssemblyInput) -> PromptAssemblyPlan {
        PromptAssemblyPlan {
            nodes: vec![
                PlannedNode::AgentBaseSystem,
                PlannedNode::OutputStyle,
                PlannedNode::ToolHelpBundle,
            ],
        }
    }
}

/// Dedicated-session strategy with general tools and workspace context.
/// 开放通用工具并携带工作区上下文的专职会话策略。
#[derive(Debug, Default)]
pub struct DedicatedToolingStrategy;

impl PromptAssemblyStrategy for DedicatedToolingStrategy {
    /// Return the tooling dedicated strategy identifier.
    /// 返回带工具专职策略标识。
    fn id(&self) -> PromptStrategyId {
        "dedicated_tooling"
    }

    /// Build the tooling dedicated ordered node plan; the base prompt replaces the persona.
    /// 构建带工具专职的有序节点计划；基础提示词取代人格。
    fn build_plan(&self, _input: &PromptAssemblyInput) -> PromptAssemblyPlan {
        PromptAssemblyPlan {
            nodes: vec![
                PlannedNode::ProviderBaseSystem,
                PlannedNode::AgentBaseSystem,
                PlannedNode::OutputStyle,
                PlannedNode::ToolHelpBundle,
                PlannedNode::ProjectRules,
                PlannedNode::EnvironmentContext,
            ],
        }
    }
}
