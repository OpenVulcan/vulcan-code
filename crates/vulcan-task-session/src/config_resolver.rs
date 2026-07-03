use crate::SUBAGENT_MODE_EXPERT;
use crate::profile::SubagentProfile;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_session_core::{
    ContextCompactionPolicyRequest, ContextCompactionPolicySnapshot, ExecutionRuntimePolicy,
    ExecutionRuntimePolicyRequest, PromptSkillSnapshot, ProviderModelSnapshot,
    SessionEffectiveConfig, SessionRequestedConfig, ToolExposureSnapshot,
};

/// Task invocation configuration supplied by the spawn-agent subagent call.
/// spawn_agent 子代理调用提供的任务调用配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub fn with_profile(profile_id: impl Into<String>, prompt: impl Into<String>) -> Self {
        let mut invocation = Self::main(prompt);
        invocation.subagent_profile_id = Some(profile_id.into());
        invocation.subagent_mode = Some(SUBAGENT_MODE_EXPERT.to_string());
        invocation
    }
}

/// Input used to resolve one task-session effective configuration.
/// 用于解析一个子代理会话有效配置的输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSessionConfigResolutionInput {
    /// Global default effective configuration.
    /// 全局默认有效配置。
    pub global_default_config: SessionEffectiveConfig,
    /// Optional parent effective configuration.
    /// 可选的父级有效配置。
    pub parent_config: Option<SessionEffectiveConfig>,
    /// Optional profile loaded before invocation overrides.
    /// 调用覆盖前加载的可选配置。
    pub profile: Option<SubagentProfile>,
    /// Invocation-level override configuration.
    /// 调用级覆盖配置。
    pub invocation: TaskInvocationConfig,
}

/// Fully resolved task-session configuration plus its isolated runtime policy.
/// 完整解析的子代理会话配置及其隔离运行时策略。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSessionResolvedConfig {
    /// Requested configuration persisted for audit.
    /// 为审计持久化的请求配置。
    pub requested_config: SessionRequestedConfig,
    /// Effective configuration used by execution.
    /// 执行使用的有效配置。
    pub effective_config: SessionEffectiveConfig,
    /// Runtime policy enforced for task sessions.
    /// 对子代理会话强制执行的运行时策略。
    pub runtime_policy: ExecutionRuntimePolicy,
}

/// Resolves task-session configuration from defaults, parent state, profile, and invocation.
/// 从默认值、父级状态、配置和调用参数解析子代理会话配置。
pub struct TaskSessionConfigResolver;

impl TaskSessionConfigResolver {
    /// Resolves one task-session effective configuration.
    /// 解析一个子代理会话有效配置。
    pub fn resolve(input: TaskSessionConfigResolutionInput) -> TaskSessionResolvedConfig {
        let profile_inherit = input
            .profile
            .as_ref()
            .and_then(|profile| profile.inherit_parent_config);
        let inherit_parent_config = input
            .invocation
            .inherit_parent_config
            .or(profile_inherit)
            .unwrap_or(true);
        let profile_id = input
            .invocation
            .subagent_profile_id
            .clone()
            .or_else(|| input.profile.as_ref().map(|profile| profile.id.clone()));
        let mut effective_config = if inherit_parent_config {
            input
                .parent_config
                .clone()
                .unwrap_or(input.global_default_config)
        } else {
            input.global_default_config
        };

        effective_config.inherit_parent_config = inherit_parent_config;
        effective_config.subagent_profile_id = profile_id.clone();
        effective_config.subagent_mode = input.invocation.subagent_mode.clone();

        if let Some(profile) = input.profile.as_ref() {
            Self::apply_profile(&mut effective_config, profile);
        }

        Self::apply_invocation(&mut effective_config, &input.invocation);

        let runtime_policy =
            Self::resolve_runtime_policy(input.profile.as_ref(), &input.invocation);
        let requested_config =
            Self::build_requested_config(inherit_parent_config, profile_id, &input.invocation);

        TaskSessionResolvedConfig {
            requested_config,
            effective_config,
            runtime_policy,
        }
    }

    /// Applies profile-level overrides to an effective configuration.
    /// 将配置级覆盖应用到有效配置。
    fn apply_profile(config: &mut SessionEffectiveConfig, profile: &SubagentProfile) {
        overlay_provider_model(&mut config.provider_model, &profile.provider_model);
        overlay_tool_exposure(&mut config.tool_exposure, &profile.tool_exposure);
        overlay_prompt_skills(&mut config.prompt_skills, &profile.prompt_skills);
        overlay_context_compaction(&mut config.context_compaction, &profile.context_compaction);
        overlay_extra(&mut config.extra, &profile.extra);

        if let Some(base_system_prompt) = profile.base_system_prompt.as_ref() {
            config.system_prompt_layers.push(base_system_prompt.clone());
        }
        if let Some(system_prompt_append) = profile.system_prompt_append.as_ref() {
            config
                .system_prompt_layers
                .push(system_prompt_append.clone());
        }
    }

    /// Applies invocation-level overrides to an effective configuration.
    /// 将调用级覆盖应用到有效配置。
    fn apply_invocation(config: &mut SessionEffectiveConfig, invocation: &TaskInvocationConfig) {
        overlay_provider_model(&mut config.provider_model, &invocation.provider_model);
        overlay_tool_exposure(&mut config.tool_exposure, &invocation.tool_exposure);
        overlay_prompt_skills(&mut config.prompt_skills, &invocation.prompt_skills);
        overlay_context_compaction(
            &mut config.context_compaction,
            &invocation.context_compaction,
        );
        overlay_extra(&mut config.extra, &invocation.extra);

        if let Some(system_prompt) = invocation.system_prompt.as_ref() {
            config.system_prompt_layers.clear();
            config.system_prompt_layers.push(system_prompt.clone());
        }
        if let Some(system_prompt_append) = invocation.system_prompt_append.as_ref() {
            config
                .system_prompt_layers
                .push(system_prompt_append.clone());
        }
    }

    /// Builds the persisted requested configuration from invocation fields.
    /// 从调用字段构建持久化的请求配置。
    fn build_requested_config(
        inherit_parent_config: bool,
        profile_id: Option<String>,
        invocation: &TaskInvocationConfig,
    ) -> SessionRequestedConfig {
        SessionRequestedConfig {
            inherit_parent_config,
            subagent_profile_id: profile_id,
            subagent_mode: invocation.subagent_mode.clone(),
            system_prompt: invocation.system_prompt.clone(),
            system_prompt_append: invocation.system_prompt_append.clone(),
            provider_model: invocation.provider_model.clone(),
            tool_exposure: invocation.tool_exposure.clone(),
            prompt_skills: invocation.prompt_skills.clone(),
            context_compaction: invocation.context_compaction.clone(),
            runtime_policy: invocation.runtime_policy,
            extra: invocation.extra.clone(),
        }
    }

    /// Resolves the task-session runtime policy from profile and invocation overrides.
    /// 从配置与调用覆盖解析子代理会话运行时策略。
    fn resolve_runtime_policy(
        profile: Option<&SubagentProfile>,
        invocation: &TaskInvocationConfig,
    ) -> ExecutionRuntimePolicy {
        let mut runtime_policy = ExecutionRuntimePolicy::task_session();
        if let Some(profile) = profile {
            overlay_runtime_policy(&mut runtime_policy, profile.runtime_policy);
        }
        overlay_runtime_policy(&mut runtime_policy, invocation.runtime_policy);
        enforce_task_session_projection_boundary(&mut runtime_policy);
        runtime_policy
    }
}

/// Applies an optional runtime policy request to a concrete policy.
/// 将可选运行时策略请求应用到具体策略。
fn overlay_runtime_policy(
    target: &mut ExecutionRuntimePolicy,
    overlay: ExecutionRuntimePolicyRequest,
) {
    if let Some(hooks_enabled) = overlay.hooks_enabled {
        target.hooks_enabled = hooks_enabled;
    }
    if let Some(vmm_enabled) = overlay.vmm_enabled {
        target.vmm_enabled = vmm_enabled;
    }
    if let Some(memory_recall_enabled) = overlay.memory_recall_enabled {
        target.memory_recall_enabled = memory_recall_enabled;
    }
    if let Some(memory_writeback_enabled) = overlay.memory_writeback_enabled {
        target.memory_writeback_enabled = memory_writeback_enabled;
    }
    if let Some(database_projection_enabled) = overlay.database_projection_enabled {
        target.database_projection_enabled = database_projection_enabled;
    }
}

/// Enforces the TaskSession projection-isolation boundary after all overrides.
/// 在所有覆盖之后强制执行 TaskSession 投影隔离边界。
fn enforce_task_session_projection_boundary(target: &mut ExecutionRuntimePolicy) {
    target.database_projection_enabled = false;
}

/// Applies provider/model override fields when they are present.
/// 在字段存在时应用供应商/模型覆盖。
fn overlay_provider_model(target: &mut ProviderModelSnapshot, overlay: &ProviderModelSnapshot) {
    if overlay.provider_id.is_some() {
        target.provider_id = overlay.provider_id.clone();
    }
    if overlay.model_id.is_some() {
        target.model_id = overlay.model_id.clone();
    }
    if overlay.reasoning_effort.is_some() {
        target.reasoning_effort = overlay.reasoning_effort.clone();
    }
    if overlay.execution_mode.is_some() {
        target.execution_mode = overlay.execution_mode.clone();
    }
}

/// Applies tool exposure override fields when they are present.
/// 在字段存在时应用工具可见性覆盖。
fn overlay_tool_exposure(target: &mut ToolExposureSnapshot, overlay: &ToolExposureSnapshot) {
    if overlay.policy.mode != vulcan_session_core::CapabilityPolicyMode::All
        || !overlay.policy.selectors.is_empty()
    {
        target.policy = overlay.policy.clone();
        target.allowed_tools = overlay.allowed_tools.clone();
        target.disabled_tools = overlay.disabled_tools.clone();
    }
    if !overlay.allowed_tools.is_empty() {
        target.allowed_tools = overlay.allowed_tools.clone();
    }
    if !overlay.disabled_tools.is_empty() {
        target.disabled_tools = overlay.disabled_tools.clone();
    }
    if !overlay.luaskills_overrides.is_null() {
        target.luaskills_overrides = overlay.luaskills_overrides.clone();
    }
    if !overlay.mcp_server_overrides.is_null() {
        target.mcp_server_overrides = overlay.mcp_server_overrides.clone();
    }
}

/// Applies prompt skill override fields when they are present.
/// 在字段存在时应用提示词技能覆盖。
fn overlay_prompt_skills(target: &mut PromptSkillSnapshot, overlay: &PromptSkillSnapshot) {
    if overlay.policy.mode != vulcan_session_core::CapabilityPolicyMode::All
        || !overlay.policy.selectors.is_empty()
    {
        target.policy = overlay.policy.clone();
        target.enabled = overlay.enabled.clone();
        target.disabled = overlay.disabled.clone();
    }
    if !overlay.enabled.is_empty() {
        target.enabled = overlay.enabled.clone();
    }
    if !overlay.disabled.is_empty() {
        target.disabled = overlay.disabled.clone();
    }
}

/// Applies context compaction override fields when they are present.
/// 在字段存在时应用上下文压缩覆盖。
fn overlay_context_compaction(
    target: &mut ContextCompactionPolicySnapshot,
    overlay: &ContextCompactionPolicyRequest,
) {
    if overlay.context_budget.is_some() {
        target.context_budget = overlay.context_budget;
    }
    if overlay.max_provider_requests.is_some() {
        target.max_provider_requests = overlay.max_provider_requests;
    }
    if let Some(auto_compaction_enabled) = overlay.auto_compaction_enabled {
        target.auto_compaction_enabled = auto_compaction_enabled;
    }
}

/// Applies an extra JSON payload when it is not null.
/// 在额外 JSON 载荷非空时应用它。
fn overlay_extra(target: &mut Value, overlay: &Value) {
    if !overlay.is_null() {
        *target = overlay.clone();
    }
}
