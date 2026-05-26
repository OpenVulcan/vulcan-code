use crate::source::snapshot::{
    AgentPromptSnapshot, EnvironmentContextSnapshot, ProjectRulesSnapshot, ProviderPromptSnapshot,
    SubagentSnapshot, TaskGoalSnapshot, ToolCatalogSnapshot,
};
use crate::types::{ModelDescriptor, PersonalitySelection, PromptAssemblyMode};

/// Complete prompt assembly input payload.
/// 完整提示词装配输入载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAssemblyInput {
    /// Composition mode.
    /// 组合模式。
    pub mode: PromptAssemblyMode,
    /// Target model descriptor.
    /// 目标模型描述。
    pub model: ModelDescriptor,
    /// Optional provider base prompt.
    /// 可选 Provider 基础提示词。
    pub provider_prompt: Option<ProviderPromptSnapshot>,
    /// Optional agent base prompt.
    /// 可选 Agent 基础提示词。
    pub agent_prompt: Option<AgentPromptSnapshot>,
    /// Requested personality selection.
    /// 请求的人格选择。
    pub personality: PersonalitySelection,
    /// Optional project rules snapshot.
    /// 可选项目规则快照。
    pub project_rules: Option<ProjectRulesSnapshot>,
    /// Optional environment context snapshot.
    /// 可选环境上下文快照。
    pub environment_context: Option<EnvironmentContextSnapshot>,
    /// Optional task-goal snapshot.
    /// 可选任务目标快照。
    pub task_goal: Option<TaskGoalSnapshot>,
    /// Visible tool catalog snapshot.
    /// 可见工具目录快照。
    pub tool_catalog: ToolCatalogSnapshot,
    /// Optional subagent snapshot.
    /// 可选子代理快照。
    pub subagent: Option<SubagentSnapshot>,
    /// Additional developer-side raw segments.
    /// 额外的开发者侧原始片段。
    pub custom_developer_segments: Vec<String>,
    /// Additional user-side raw segments.
    /// 额外的用户侧原始片段。
    pub custom_user_segments: Vec<String>,
}
