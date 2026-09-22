//! Shared task invocation and resolved configuration contracts.
//! 共享任务调用及已解析配置契约。

use crate::{
    ContextCompactionPolicyRequest, ExecutionRuntimePolicy, ExecutionRuntimePolicyRequest,
    PromptSkillSnapshot, ProviderModelSnapshot, SessionEffectiveConfig, SessionRequestedConfig,
    ToolExposureSnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Subagent mode keys accepted by catalog files and spawn_agent.
/// 目录文件与 spawn_agent 接受的子代理模式键。
pub const SUBAGENT_MODE_DEFAULT: &str = "default";
/// Mirror subagent mode key accepted by catalog files and spawn_agent.
/// 目录文件与 spawn_agent 接受的镜像子代理模式键。
pub const SUBAGENT_MODE_MIRROR: &str = "mirror";
/// Expert subagent mode key accepted by catalog files and spawn_agent.
/// 目录文件与 spawn_agent 接受的专家子代理模式键。
pub const SUBAGENT_MODE_EXPERT: &str = "expert";

/// Task invocation configuration supplied by the spawn-agent subagent call.
/// spawn_agent 子代理调用提供的任务调用配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskInvocationConfig {
    /// Prompt that the task session should execute.
    /// 子代理会话需要执行的提示词。
    pub prompt: String,
    /// Optional profile identifier requested by the invocation.
    /// 调用请求的可选配置标识符。
    pub subagent_profile_id: Option<String>,
    /// Optional subagent execution mode key requested by the invocation.
    /// 调用请求的可选子代理执行模式键。
    pub subagent_mode: Option<String>,
    /// Optional inheritance override for this invocation.
    /// 本次调用的可选继承覆盖。
    pub inherit_parent_config: Option<bool>,
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
    /// Additional invocation-owned payload.
    /// 调用拥有的额外载荷。
    pub extra: Value,
}

impl TaskInvocationConfig {
    /// Creates a default task invocation that inherits the parent configuration.
    /// 创建默认继承父配置的任务调用。
    /// `prompt` is the original task text; returns an unresolved invocation with no explicit overrides.
    /// `prompt` 为原任务正文；返回无显式覆盖的未解析调用。
    pub fn main(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            subagent_profile_id: None,
            subagent_mode: None,
            inherit_parent_config: None,
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

    /// Creates a task invocation that targets a named subagent profile.
    /// 创建指向命名子代理配置的任务调用。
    /// `profile_id` selects the expert and `prompt` supplies its task; returns the unresolved expert invocation.
    /// `profile_id` 选择专家，`prompt` 提供任务；返回未解析专家调用。
    pub fn with_profile(profile_id: impl Into<String>, prompt: impl Into<String>) -> Self {
        let mut invocation = Self::main(prompt);
        invocation.subagent_profile_id = Some(profile_id.into());
        invocation.subagent_mode = Some(SUBAGENT_MODE_EXPERT.to_string());
        invocation
    }
}

/// Fully resolved task-session configuration plus its isolated runtime policy.
/// 完整解析的子代理会话配置及其隔离运行时策略。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSessionResolvedConfig {
    /// Requested configuration persisted for audit.
    /// 为审计持久化的请求配置。
    pub requested_config: SessionRequestedConfig,
    /// Effective configuration used by execution.
    /// 执行使用的有效配置。
    pub effective_config: SessionEffectiveConfig,
    /// Functional system prompt layers owned by this task session after resolution.
    /// 解析后由当前子代理会话自身拥有的功能性系统提示词层。
    pub functional_system_prompt_layers: Vec<String>,
    /// Runtime policy enforced for task sessions.
    /// 对子代理会话强制执行的运行时策略。
    pub runtime_policy: ExecutionRuntimePolicy,
}
