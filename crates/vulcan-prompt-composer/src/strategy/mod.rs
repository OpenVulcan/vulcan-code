//! Prompt assembly strategy definitions.
//! 提示词装配策略定义。

/// Context compaction strategy.
/// 上下文压缩策略。
pub mod compaction;
/// Primary agent strategy.
/// 主代理策略。
pub mod primary;
/// Subagent clone strategy.
/// 子代理分身策略。
pub mod subagent_clone;
/// Subagent functional strategy.
/// 子代理功能策略。
pub mod subagent_functional;
/// Strategy trait definitions.
/// 策略 trait 定义。
pub mod traits;

/// Planned node selector produced by one strategy.
/// 由策略产出的计划节点选择器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedNode {
    /// Provider base system prompt node.
    /// Provider 基础系统提示词节点。
    ProviderBaseSystem,
    /// Agent base system prompt node.
    /// Agent 基础系统提示词节点。
    AgentBaseSystem,
    /// Functional agent system prompt node.
    /// 功能代理系统提示词节点。
    FunctionalAgentSystem,
    /// Personality node.
    /// 人格节点。
    Personality,
    /// Stable output-style system prompt node.
    /// 稳定的输出风格系统提示词节点。
    OutputStyle,
    /// Tool-help bundle node.
    /// 工具帮助组合节点。
    ToolHelpBundle,
    /// Project rules node.
    /// 项目规则节点。
    ProjectRules,
    /// Environment context node.
    /// 环境上下文节点。
    EnvironmentContext,
    /// Task-goal node.
    /// 任务目标节点。
    TaskGoal,
    /// Subagent identity node.
    /// 子代理身份节点。
    SubagentIdentity,
}

/// Ordered prompt assembly plan.
/// 有序提示词装配计划。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptAssemblyPlan {
    /// Ordered node list.
    /// 有序节点列表。
    pub nodes: Vec<PlannedNode>,
}
