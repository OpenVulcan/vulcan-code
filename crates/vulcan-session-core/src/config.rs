use crate::policy::ExecutionRuntimePolicyRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Provider and model snapshot selected for one session node.
/// 单个会话节点选定的供应商与模型快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderModelSnapshot {
    /// Provider identifier.
    /// 供应商标识符。
    pub provider_id: Option<String>,
    /// Model identifier.
    /// 模型标识符。
    pub model_id: Option<String>,
    /// Reasoning effort or reasoning profile.
    /// 推理等级或推理配置。
    pub reasoning_effort: Option<String>,
    /// Execution mode selected for the node.
    /// 该节点选定的执行模式。
    pub execution_mode: Option<String>,
}

/// Tool visibility snapshot used by prompt assembly and execution.
/// 提示词组装与执行使用的工具可见性快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolExposureSnapshot {
    /// Explicitly allowed tool identifiers.
    /// 显式允许的工具标识符。
    pub allowed_tools: Vec<String>,
    /// Explicitly disabled tool identifiers.
    /// 显式禁用的工具标识符。
    pub disabled_tools: Vec<String>,
    /// Explicitly allowed AI capability identifiers.
    /// 显式允许的 AI 能力标识符。
    pub allowed_ai_capabilities: Vec<String>,
    /// Explicitly disabled AI capability identifiers.
    /// 显式禁用的 AI 能力标识符。
    pub disabled_ai_capabilities: Vec<String>,
    /// LuaSkills override payload.
    /// LuaSkills 覆盖载荷。
    pub luaskills_overrides: Value,
    /// MCP server override payload.
    /// MCP 服务覆盖载荷。
    pub mcp_server_overrides: Value,
}

/// Prompt skill snapshot visible to one session node.
/// 单个会话节点可见的提示词技能快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PromptSkillSnapshot {
    /// Enabled prompt skill identifiers.
    /// 启用的提示词技能标识符。
    pub enabled: Vec<String>,
    /// Disabled prompt skill identifiers.
    /// 禁用的提示词技能标识符。
    pub disabled: Vec<String>,
}

/// Context compaction policy snapshot for one session node.
/// 单个会话节点的上下文压缩策略快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContextCompactionPolicySnapshot {
    /// Optional token budget for the context.
    /// 可选的上下文 token 预算。
    pub context_budget: Option<u64>,
    /// Optional maximum provider request count before compaction.
    /// 压缩前允许的最大供应商请求数量。
    pub max_provider_requests: Option<usize>,
    /// Whether automatic compaction is enabled.
    /// 是否启用自动压缩。
    pub auto_compaction_enabled: bool,
}

/// Context compaction policy override before final resolution.
/// 最终解析前的上下文压缩策略覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContextCompactionPolicyRequest {
    /// Optional token budget override for the context.
    /// 可选的上下文 token 预算覆盖。
    pub context_budget: Option<u64>,
    /// Optional maximum provider request count override before compaction.
    /// 压缩前允许的最大供应商请求数量覆盖。
    pub max_provider_requests: Option<usize>,
    /// Optional automatic compaction toggle override.
    /// 可选的自动压缩开关覆盖。
    pub auto_compaction_enabled: Option<bool>,
}

/// User requested configuration before inheritance is resolved.
/// 继承解析前的用户请求配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRequestedConfig {
    /// Whether this node should inherit parent configuration.
    /// 该节点是否继承父级配置。
    pub inherit_parent_config: bool,
    /// Optional subagent profile identifier.
    /// 可选的子代理配置标识符。
    pub subagent_profile_id: Option<String>,
    /// Optional subagent execution mode key.
    /// 可选的子代理执行模式键。
    pub subagent_mode: Option<String>,
    /// Optional replacement system prompt.
    /// 可选的替换系统提示词。
    pub system_prompt: Option<String>,
    /// Optional appended system prompt.
    /// 可选的追加系统提示词。
    pub system_prompt_append: Option<String>,
    /// Provider and model override request.
    /// 供应商与模型覆盖请求。
    pub provider_model: ProviderModelSnapshot,
    /// Tool exposure override request.
    /// 工具可见性覆盖请求。
    pub tool_exposure: ToolExposureSnapshot,
    /// Prompt skill override request.
    /// 提示词技能覆盖请求。
    pub prompt_skills: PromptSkillSnapshot,
    /// Context compaction override request.
    /// 上下文压缩覆盖请求。
    pub context_compaction: ContextCompactionPolicyRequest,
    /// Runtime policy override request.
    /// 运行时策略覆盖请求。
    pub runtime_policy: ExecutionRuntimePolicyRequest,
    /// Additional caller-owned configuration payload.
    /// 调用方拥有的额外配置载荷。
    pub extra: Value,
}

impl Default for SessionRequestedConfig {
    /// Builds the default requested configuration.
    /// 构建默认请求配置。
    fn default() -> Self {
        Self {
            inherit_parent_config: true,
            subagent_profile_id: None,
            subagent_mode: None,
            system_prompt: None,
            system_prompt_append: None,
            provider_model: ProviderModelSnapshot::default(),
            tool_exposure: ToolExposureSnapshot::default(),
            prompt_skills: PromptSkillSnapshot::default(),
            context_compaction: ContextCompactionPolicyRequest::default(),
            runtime_policy: ExecutionRuntimePolicyRequest::default(),
            extra: Value::Null,
        }
    }
}

/// Fully resolved configuration used by execution and persistence.
/// 执行与持久化使用的完整解析配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEffectiveConfig {
    /// Whether parent configuration participated in resolution.
    /// 父级配置是否参与了解析。
    pub inherit_parent_config: bool,
    /// Resolved subagent profile identifier.
    /// 已解析的子代理配置标识符。
    pub subagent_profile_id: Option<String>,
    /// Resolved subagent execution mode key.
    /// 已解析的子代理执行模式键。
    pub subagent_mode: Option<String>,
    /// Resolved provider and model snapshot.
    /// 已解析的供应商与模型快照。
    pub provider_model: ProviderModelSnapshot,
    /// Ordered system prompt layers.
    /// 有序系统提示词层。
    pub system_prompt_layers: Vec<String>,
    /// Resolved tool exposure snapshot.
    /// 已解析的工具可见性快照。
    pub tool_exposure: ToolExposureSnapshot,
    /// Resolved prompt skill snapshot.
    /// 已解析的提示词技能快照。
    pub prompt_skills: PromptSkillSnapshot,
    /// Resolved context compaction policy.
    /// 已解析的上下文压缩策略。
    pub context_compaction: ContextCompactionPolicySnapshot,
    /// Additional resolved configuration payload.
    /// 额外的已解析配置载荷。
    pub extra: Value,
}

impl Default for SessionEffectiveConfig {
    /// Builds an empty resolved configuration that still inherits parent state by default.
    /// 构建默认继承父级状态的空解析配置。
    fn default() -> Self {
        Self {
            inherit_parent_config: true,
            subagent_profile_id: None,
            subagent_mode: None,
            provider_model: ProviderModelSnapshot::default(),
            system_prompt_layers: Vec::new(),
            tool_exposure: ToolExposureSnapshot::default(),
            prompt_skills: PromptSkillSnapshot::default(),
            context_compaction: ContextCompactionPolicySnapshot::default(),
            extra: Value::Null,
        }
    }
}
