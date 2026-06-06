use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_session_core::{
    ContextCompactionPolicyRequest, ExecutionRuntimePolicyRequest, PromptSkillSnapshot,
    ProviderModelSnapshot, ToolExposureSnapshot,
};

/// Declarative subagent profile used before task invocation overrides.
/// 在任务调用覆盖前使用的声明式子代理配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubagentProfile {
    /// Stable profile identifier.
    /// 稳定配置标识符。
    pub id: String,
    /// Human-readable profile name.
    /// 面向人的配置名称。
    pub name: String,
    /// Optional profile description.
    /// 可选的配置描述。
    pub description: Option<String>,
    /// Optional inheritance override.
    /// 可选的继承覆盖。
    pub inherit_parent_config: Option<bool>,
    /// Optional base system prompt injected by this profile.
    /// 该配置注入的可选基础系统提示词。
    pub base_system_prompt: Option<String>,
    /// Optional appended system prompt injected by this profile.
    /// 该配置注入的可选追加系统提示词。
    pub system_prompt_append: Option<String>,
    /// Provider and model override snapshot.
    /// 供应商与模型覆盖快照。
    pub provider_model: ProviderModelSnapshot,
    /// Tool exposure override snapshot.
    /// 工具可见性覆盖快照。
    pub tool_exposure: ToolExposureSnapshot,
    /// Prompt skill override snapshot.
    /// 提示词技能覆盖快照。
    pub prompt_skills: PromptSkillSnapshot,
    /// Context compaction override snapshot.
    /// 上下文压缩覆盖快照。
    pub context_compaction: ContextCompactionPolicyRequest,
    /// Runtime policy override request.
    /// 运行时策略覆盖请求。
    pub runtime_policy: ExecutionRuntimePolicyRequest,
    /// Optional maximum nested task depth.
    /// 可选的最大嵌套任务深度。
    pub max_depth: Option<u16>,
    /// Additional profile-owned payload.
    /// 配置拥有的额外载荷。
    pub extra: Value,
}

impl SubagentProfile {
    /// Creates a subagent profile with empty override fields.
    /// 创建覆盖字段为空的子代理配置。
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            inherit_parent_config: None,
            base_system_prompt: None,
            system_prompt_append: None,
            provider_model: ProviderModelSnapshot::default(),
            tool_exposure: ToolExposureSnapshot::default(),
            prompt_skills: PromptSkillSnapshot::default(),
            context_compaction: ContextCompactionPolicyRequest::default(),
            runtime_policy: ExecutionRuntimePolicyRequest::default(),
            max_depth: None,
            extra: Value::Null,
        }
    }
}
