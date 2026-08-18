//! Shared prompt-based context-compaction runtime.
//! 共享提示词式上下文压缩运行时。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_models::{
    ExecutionAgentPromptSnapshot, ExecutionSubagentPromptContext,
};
use crate::agent_runtime::execution_preparation::prepare_execution_with_tool_snapshot;
use crate::agent_runtime::provider_protocol_tool_definition::resolved_tool_definition_to_provider_tool_definition;
use crate::agent_runtime::provider_reasoning::public_reasoning_effort_from_level;
use crate::agent_runtime::provider_runtime_context::build_runtime_prompt_layers_for_agent;
use crate::agent_runtime::provider_transport_vulcan::{
    build_responses_compaction_messages, project_public_tool_definitions, public_usage_to_core,
    resolve_public_native_tools,
};
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_bootstrap::bootstrap_runtime_stream;
use crate::agent_runtime::runtime_stream_driver::drive_runtime_stream_launch;
use crate::agent_runtime::runtime_stream_launch::prepare_runtime_stream_launch_with_plugins_and_public_ai;
use crate::agent_runtime::runtime_stream_launch_payload::build_runtime_stream_launch_payload;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::{
    ExecutionPreparationRequest, ExecutionPromptPurpose, ExecutionRunReport, ExecutionStreamEvent,
    ExtensionGeneration,
};
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::provider_config::{
    PersistedPromptSkillState, ProjectPromptSkillsConfiguration, ProviderConfigurationFile,
    ReasoningEffortLevel,
};
use crate::tooling::CommandSessionRuntime;
use async_stream::stream;
use futures_util::{StreamExt, stream::Stream};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionRequest, AiMessage, AiOperation, ContentItem, MessageRole,
    NativeToolKind, ReasoningOutputPolicy, ToolDefinition,
};
use vulcan_ai_protocols::normalize_openai_usage;
use vulcan_context::{ContextCompactionPolicy, SessionConversationState};
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport, CompactionPlan,
    CompactionResult, ContextCompactionRequestPromptMode, ProviderContextRoute,
    build_compaction_request_prompt_for_mode, extract_compaction_model_summary,
};
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_prompt_composer::PromptOutputStyleSnapshot;
use vulcan_session_core::{CapabilityPolicySnapshot, ExecutionRuntimePolicy, ToolExposureSnapshot};
use vulcan_workbench_model::SessionProviderToolTargetSelection;

mod pruning;

#[cfg(test)]
mod tests;

use pruning::{
    append_prune_report_to_error, compaction_attempt_conversation_state,
    compaction_prompt_mode_for_prune_mode, initial_compaction_prune_mode,
    input_prune_report_for_execution, next_compaction_retry_mode,
    prepared_compaction_source_for_attempt,
};

/// Exact public provider target frozen for one context-compaction execution.
/// 为单次上下文压缩执行冻结的精确公共供应商目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptCompactionModelTarget {
    /// Exact provider identifier selected by the owning session.
    /// 所属会话选择的精确供应商标识。
    pub provider_id: String,
    /// Exact credential connection identifier selected by the owning session.
    /// 所属会话选择的精确凭据连接标识。
    pub connection_id: String,
    /// Exact model identifier selected by the owning session.
    /// 所属会话选择的精确模型标识。
    pub model_id: String,
    /// Persisted non-secret identity of the exact session provider-route lease.
    /// 精确会话供应商路由租约的持久化非秘密身份。
    pub provider_route_lease_identity: vulcan_workbench_model::SessionProviderRouteLeaseIdentity,
    /// Whether the non-secret exact route explicitly enables standalone remote compaction.
    /// 不含密钥的精确路由是否显式启用独立远端压缩。
    pub remote_compaction_enabled: bool,
    /// Raw model-declared total context window used by the retained-tail formula.
    /// 尾部保留公式使用的模型原始声明总上下文窗口。
    pub context_limit: Option<usize>,
    /// Complete context-compaction policy frozen with this exact route.
    /// 随该精确路由一同冻结的完整上下文压缩策略。
    pub context_compaction_policy: ContextCompactionPolicy,
}

impl PromptCompactionModelTarget {
    /// Builds a diagnostic provider/model label without exposing credential material.
    /// 构建不暴露凭据材料的供应商/模型诊断标签。
    ///
    /// # Returns
    /// 返回值
    /// A stable `provider/model` label.
    /// 返回稳定的 `provider/model` 标签。
    pub fn label(&self) -> String {
        format!("{}/{}", self.provider_id, self.model_id)
    }
}

/// Immutable session controls required to reproduce ordinary-turn prompt and tool visibility during compaction.
/// 在压缩期间复现普通回合提示词与工具可见性所需的不可变会话控制。
#[derive(Debug, Clone)]
pub struct PromptCompactionLaunchContext {
    /// Exact execution mode used to resolve mode-specific tools.
    /// 用于解析模式专属工具的精确执行模式。
    pub execution_mode: String,
    /// Runtime policy owned by the source turn and reused only for remote tooling preparation.
    /// 来源回合持有、仅供远端工具准备复用的运行时策略。
    pub runtime_policy: ExecutionRuntimePolicy,
    /// Optional fixed agent prompt captured by a task session.
    /// 子任务会话捕获的可选固定代理提示词。
    pub agent_prompt_snapshot: Option<ExecutionAgentPromptSnapshot>,
    /// Optional provider-tool selections keyed by semantic capability.
    /// 按语义能力索引的可选供应商工具选择。
    pub provider_tool_selections_override:
        Option<BTreeMap<String, SessionProviderToolTargetSelection>>,
    /// Optional model-extension target sets captured from the source turn.
    /// 从来源回合捕获的可选模型扩展目标集合。
    pub provider_extension_target_sets_override:
        Option<BTreeMap<String, BTreeSet<SessionProviderToolTargetSelection>>>,
    /// Optional child-agent prompt context captured by the source turn.
    /// 来源回合捕获的可选子代理提示词上下文。
    pub subagent_prompt_context: Option<ExecutionSubagentPromptContext>,
    /// Optional task-session artifact root used by task-scoped tools.
    /// 子任务范围工具使用的可选任务会话制品根目录。
    pub task_session_store_root: Option<PathBuf>,
    /// Optional output-style snapshot rendered into the ordinary base instructions.
    /// 渲染到普通基础指令中的可选输出风格快照。
    pub output_style_snapshot: Option<PromptOutputStyleSnapshot>,
    /// Exact request-scoped tool exposure policy.
    /// 精确的请求级工具可见性策略。
    pub tool_exposure_override: ToolExposureSnapshot,
    /// Optional host rejection reason retained for mirror-task tool shape parity.
    /// 为镜像子任务工具形态一致性保留的可选宿主拒绝原因。
    pub tool_request_rejection_reason: Option<String>,
    /// Whether the host rejection reason applies to every visible tool.
    /// 宿主拒绝原因是否应用于全部可见工具。
    pub tool_request_rejection_all_tools: bool,
    /// Exact visible tool identifiers rejected by the source turn.
    /// 来源回合拒绝的精确可见工具标识集合。
    pub tool_request_rejection_tool_ids: Vec<String>,
    /// Exact tool identifiers hidden by the source turn.
    /// 来源回合隐藏的精确工具标识集合。
    pub tool_request_hidden_tool_ids: Vec<String>,
    /// Request-scoped prompt-skill visibility policy.
    /// 请求级提示词技能可见性策略。
    pub prompt_skill_policy_override: CapabilityPolicySnapshot,
    /// Project-level prompt-skill configuration captured by the source turn.
    /// 来源回合捕获的项目级提示词技能配置。
    pub project_prompt_skills: ProjectPromptSkillsConfiguration,
    /// Session-level prompt-skill overrides captured by the source turn.
    /// 来源回合捕获的会话级提示词技能覆盖。
    pub session_prompt_skill_overrides: BTreeMap<String, PersistedPromptSkillState>,
    /// Optional Plan-mode developer guidance captured by the source turn.
    /// 来源回合捕获的可选计划模式开发者指引。
    pub session_plan_context: Option<String>,
}

impl Default for PromptCompactionLaunchContext {
    /// Builds the normal main-session defaults used when no pending ordinary turn exists.
    /// 在不存在待执行普通回合时构建普通主会话默认值。
    ///
    /// # Returns
    /// 返回值
    /// A context with build-mode execution and no session-specific overrides.
    /// 返回构建模式执行且不含会话专属覆盖的上下文。
    fn default() -> Self {
        Self {
            execution_mode: String::from("build"),
            runtime_policy: ExecutionRuntimePolicy::main_session(),
            agent_prompt_snapshot: None,
            provider_tool_selections_override: None,
            provider_extension_target_sets_override: None,
            subagent_prompt_context: None,
            task_session_store_root: None,
            output_style_snapshot: None,
            tool_exposure_override: Default::default(),
            tool_request_rejection_reason: None,
            tool_request_rejection_all_tools: false,
            tool_request_rejection_tool_ids: Vec::new(),
            tool_request_hidden_tool_ids: Vec::new(),
            prompt_skill_policy_override: Default::default(),
            project_prompt_skills: Default::default(),
            session_prompt_skill_overrides: BTreeMap::new(),
            session_plan_context: None,
        }
    }
}

impl PromptCompactionLaunchContext {
    /// Freezes the ordinary-turn controls that affect prompt assembly or model-visible tool shape.
    /// 冻结会影响提示词装配或模型可见工具形态的普通回合控制。
    ///
    /// # Parameters
    /// 参数
    /// `request` is the already materialized source-turn preparation request.
    /// `request` 是已经物化的来源回合准备请求。
    ///
    /// # Returns
    /// 返回值
    /// An immutable compaction launch context with dynamic conversation overlays intentionally excluded.
    /// 返回一份不可变压缩启动上下文，并有意排除动态会话覆盖层。
    pub fn from_execution_request(request: &ExecutionPreparationRequest) -> Self {
        Self {
            execution_mode: request.execution_mode.clone(),
            runtime_policy: request.runtime_policy,
            agent_prompt_snapshot: request.agent_prompt_snapshot.clone(),
            provider_tool_selections_override: request.provider_tool_selections_override.clone(),
            provider_extension_target_sets_override: request
                .provider_extension_target_sets_override
                .clone(),
            subagent_prompt_context: request.subagent_prompt_context.clone(),
            task_session_store_root: request.task_session_store_root.clone(),
            output_style_snapshot: request.output_style_snapshot.clone(),
            tool_exposure_override: request.tool_exposure_override.clone(),
            tool_request_rejection_reason: request.tool_request_rejection_reason.clone(),
            tool_request_rejection_all_tools: request.tool_request_rejection_all_tools,
            tool_request_rejection_tool_ids: request.tool_request_rejection_tool_ids.clone(),
            tool_request_hidden_tool_ids: request.tool_request_hidden_tool_ids.clone(),
            prompt_skill_policy_override: request.prompt_skill_policy_override.clone(),
            project_prompt_skills: request.project_prompt_skills.clone(),
            session_prompt_skill_overrides: request.session_prompt_skill_overrides.clone(),
            session_plan_context: request.session_plan_context.clone(),
        }
    }
}

/// Resolves one compaction target through the same exact-account rules used by normal session turns.
/// 按普通会话回合相同的精确账号规则解析单个压缩目标。
///
/// # Parameters
/// 参数
/// `runtime` owns the authoritative catalog and connection graph; the optional overrides identify
/// the session-selected provider, credential connection, and model.
/// `runtime` 持有权威目录与连接图；可选覆盖标识会话选择的供应商、凭据连接与模型。
/// `context_compaction_policy_seed` carries behavior-only task settings resolved with the exact model window.
/// `context_compaction_policy_seed` 携带需要结合精确模型窗口解析的仅行为任务设置。
///
/// # Returns
/// 返回值
/// The immutable exact target, `None` only when neither a session target nor a global active model
/// exists, or an explicit error for an incomplete or invalid selection.
/// 返回不可变精确目标；仅当会话目标与全局活动模型均不存在时返回 `None`；选择不完整或无效时返回显式错误。
pub fn resolve_prompt_compaction_model_target(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    session_id: &str,
    expected_route_lease: Option<&vulcan_workbench_model::SessionProviderRouteLeaseIdentity>,
    provider_id_override: Option<&str>,
    connection_id_override: Option<&str>,
    model_id_override: Option<&str>,
    context_compaction_policy_seed: Option<ContextCompactionPolicy>,
) -> Result<Option<PromptCompactionModelTarget>, String> {
    let configuration = runtime.configuration_snapshot()?;
    if provider_id_override.is_none()
        && model_id_override.is_none()
        && connection_id_override.is_none()
        && configuration.active.is_none()
    {
        return Ok(None);
    }
    // blueprint reuses the same session lease authority as an ordinary provider turn.
    // blueprint 复用与普通供应商回合相同的会话租约权威。
    let blueprint = crate::agent_runtime::provider::resolve_public_ai_provider_blueprint(
        runtime,
        session_id,
        expected_route_lease,
        ReasoningEffortLevel::Off,
        provider_id_override,
        model_id_override,
        connection_id_override,
    )?;
    // selection is guaranteed for a dispatchable blueprint and carries every frozen profile.
    // selection 对可分派蓝图必然存在，并携带全部冻结 Profile。
    let selection = blueprint.frozen_selection.as_ref().ok_or_else(|| {
        String::from("Context compaction model resolution did not freeze a provider selection.")
    })?;
    // remoteCompactionEnabled avoids credential resolution when the selected route opted out.
    // remoteCompactionEnabled 在所选路由未启用时避免解析凭据。
    let remote_compaction_enabled = selection.route.protocol_route.protocol_profile_id
        == vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID
        && selection.route.protocol_route.remote_compaction_enabled();
    // providerRouteLeaseIdentity persists only the non-secret identity needed for restart validation.
    // providerRouteLeaseIdentity 仅持久化重启校验所需的非秘密身份。
    let provider_route_lease_identity = blueprint
        .session_provider_route_lease_identity()
        .ok_or_else(|| {
            String::from("Context compaction model target has no route lease identity.")
        })?;
    let context_limit =
        crate::context_budget::model_declared_context_limit(&blueprint.public_capabilities);
    let context_compaction_policy = configuration.resolve_context_compaction_policy(
        selection.route.provider.id.as_str(),
        selection.model.id.as_str(),
        context_limit,
        context_compaction_policy_seed,
    );
    Ok(Some(PromptCompactionModelTarget {
        provider_id: selection.route.provider.id.clone(),
        connection_id: selection.route.connection.id.clone(),
        model_id: selection.model.id.clone(),
        provider_route_lease_identity,
        remote_compaction_enabled,
        context_limit,
        context_compaction_policy,
    }))
}

/// Immutable request payload required to execute one prompt-based compaction job.
/// 执行单次提示词式压缩任务所需的不可变请求载荷。
#[derive(Clone)]
pub struct PromptCompactionJobRequest {
    /// Target session identifier.
    /// 目标会话标识。
    pub session_id: crate::model::SessionId,
    /// Target session title used by preparation diagnostics.
    /// 供准备阶段诊断使用的目标会话标题。
    pub session_title: String,
    /// Effective workspace path associated with the session.
    /// 与该会话关联的生效工作区路径。
    pub workspace_path: String,
    /// Planned compaction window whose source messages should be summarized.
    /// 其源消息需要被总结的已规划压缩窗口。
    pub plan: CompactionPlan,
    /// Full runtime conversation cloned before any visible compaction status rows are inserted.
    /// 插入任何可见压缩状态行之前克隆的完整运行时会话。
    pub full_context_state: SessionConversationState,
    /// Optional target input budget used to decide clone-only pruning for window downgrades.
    /// 用于判断窗口降级时是否执行仅克隆剪枝的可选目标输入预算。
    pub input_budget: Option<CompactionInputBudget>,
    /// Latest observed provider input tokens used before local estimates when deciding downgrade.
    /// 判断是否降级时优先于本地估算使用的最近一次 Provider 实测输入 Token。
    pub observed_input_tokens: Option<usize>,
    /// Latest live budget input estimate used when provider feedback is unavailable.
    /// Provider 反馈不可用时使用的最近一次实时预算输入估算。
    pub estimated_input_tokens: Option<usize>,
    /// Provider configuration already adjusted to the intended provider/model snapshot.
    /// 已经调整到目标供应商/模型快照的 Provider 配置。
    pub provider_configuration: ProviderConfigurationFile,
    /// Exact session-scoped provider, connection, and model used by both remote and local compaction.
    /// 远端与本地压缩共同使用的精确会话级供应商、连接与模型。
    pub model_target: Option<PromptCompactionModelTarget>,
    /// Exact session agent whose visible tool catalog must be preserved during compaction.
    /// 压缩期间必须保留其可见工具目录的精确会话代理。
    pub execution_agent_id: Option<String>,
    /// Frozen ordinary-turn controls used to reproduce prompt instructions and visible tools.
    /// 用于复现提示指令与可见工具的已冻结普通回合控制。
    pub launch_context: PromptCompactionLaunchContext,
    /// Process-local public AI runtime shared with the service execution boundary.
    /// 与服务执行边界共享的进程内公共 AI 运行时。
    pub public_ai_runtime: Arc<vulcan_provider_adapters::PublicAiRuntime>,
    /// Optional provider/model label kept only for diagnostics.
    /// 仅供诊断使用的可选供应商/模型标签。
    pub model_label: Option<String>,
    /// Effective reasoning preset that should be forwarded into the internal summary call.
    /// 应转发到内部总结调用中的生效推理档位。
    pub reasoning_effort: ReasoningEffortLevel,
    /// Shared agent-profile catalog used to rebuild one fresh runtime foundation.
    /// 用于重建新运行时基础架构的共享代理画像目录。
    pub agent_profile_catalog: AgentProfileCatalog,
    /// Optional LuaSkills runtime handle reused by the preparation pipeline.
    /// 被准备链路复用的可选 LuaSkills 运行时句柄。
    pub luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Shared plugin runtime reused by the exact ordinary/compaction tool projection.
    /// 被精确普通/压缩工具投影复用的共享插件运行时。
    pub vulcan_plugin_runtime: vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    /// Atomically published plugin catalog and MCP generation frozen for this compaction job.
    /// 为本次压缩任务冻结的原子发布插件目录与 MCP 代次。
    pub extension_generation: Arc<ExtensionGeneration>,
    /// Shared resumable task runtime reused by host-controlled transport execution.
    /// 供宿主控制传输执行复用的共享可恢复任务运行时。
    pub task_runtime: Arc<TaskRuntime>,
    /// Shared question runtime reused by the execution foundation.
    /// 供执行基础架构复用的共享提问运行时。
    pub question_runtime: Arc<crate::agent_runtime::question_runtime::QuestionRuntime>,
    /// Shared command-session runtime reused by the execution foundation.
    /// 供执行基础架构复用的共享命令会话运行时。
    pub command_session_runtime: Arc<CommandSessionRuntime>,
}

/// Immutable request payload required to execute one prompt-based compaction job from an already resolved runtime foundation.
/// 从已解析运行时基础架构执行单次提示词式压缩任务所需的不可变请求载荷。
#[derive(Clone)]
pub struct PromptCompactionFoundationJobRequest {
    /// Target session identifier.
    /// 目标会话标识。
    pub session_id: crate::model::SessionId,
    /// Target session title used by preparation diagnostics.
    /// 供准备阶段诊断使用的目标会话标题。
    pub session_title: String,
    /// Effective workspace path associated with the session.
    /// 与该会话关联的生效工作区路径。
    pub workspace_path: String,
    /// Planned compaction window whose source messages should be summarized.
    /// 其源消息需要被总结的已规划压缩窗口。
    pub plan: CompactionPlan,
    /// Full runtime conversation cloned before any visible compaction status rows are inserted.
    /// 插入任何可见压缩状态行之前克隆的完整运行时会话。
    pub full_context_state: SessionConversationState,
    /// Optional target input budget used to decide clone-only pruning for window downgrades.
    /// 用于判断窗口降级时是否执行仅克隆剪枝的可选目标输入预算。
    pub input_budget: Option<CompactionInputBudget>,
    /// Latest observed provider input tokens used before local estimates when deciding downgrade.
    /// 判断是否降级时优先于本地估算使用的最近一次 Provider 实测输入 Token。
    pub observed_input_tokens: Option<usize>,
    /// Latest live budget input estimate used when provider feedback is unavailable.
    /// Provider 反馈不可用时使用的最近一次实时预算输入估算。
    pub estimated_input_tokens: Option<usize>,
    /// Provider configuration already adjusted to the intended provider/model snapshot.
    /// 已经调整到目标供应商/模型快照的 Provider 配置。
    pub provider_configuration: ProviderConfigurationFile,
    /// Exact task-scoped provider, connection, and model used by both remote and local compaction.
    /// 远端与本地压缩共同使用的精确任务级供应商、连接与模型。
    pub model_target: Option<PromptCompactionModelTarget>,
    /// Exact task agent whose visible tool catalog must be preserved during compaction.
    /// 压缩期间必须保留其可见工具目录的精确任务代理。
    pub execution_agent_id: Option<String>,
    /// Frozen task-turn controls used to reproduce prompt instructions and visible tools.
    /// 用于复现提示指令与可见工具的已冻结任务回合控制。
    pub launch_context: PromptCompactionLaunchContext,
    /// Optional provider/model label kept only for diagnostics.
    /// 仅供诊断使用的可选供应商/模型标签。
    pub model_label: Option<String>,
    /// Effective reasoning preset that should be forwarded into the internal summary call.
    /// 应转发到内部总结调用中的生效推理档位。
    pub reasoning_effort: ReasoningEffortLevel,
    /// Runtime foundation that already owns resolved agents, tools, LuaSkills, MCP, and shared runtimes.
    /// 已持有解析后代理、工具、LuaSkills、MCP 与共享运行时的运行时基础架构。
    pub foundation: Arc<RuntimeFoundation>,
    /// Optional LuaSkills runtime handle reused by the preparation pipeline.
    /// 被准备链路复用的可选 LuaSkills 运行时句柄。
    pub luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
}

/// Terminal result returned by one prompt-based compaction job after the internal summary request settles.
/// 单次提示词式压缩任务在内部总结请求收敛后返回的终态结果。
#[derive(Debug, Clone)]
pub struct PromptCompactionExecution {
    /// Final prompt-shaped compaction result ready to be applied to the session state.
    /// 准备好应用到会话状态的最终提示词形态压缩结果。
    pub compaction_result: CompactionResult,
    /// Whether the model request fell back; prompt compaction now treats this as an invalid terminal state.
    /// 模型请求是否发生回退；提示词压缩现在将其视为无效终态。
    pub fallback_used: bool,
    /// Number of provider requests dispatched while producing the summary.
    /// 产出该总结期间发出的 Provider 请求次数。
    pub provider_request_count: usize,
    /// Latest successful provider usage captured while producing the summary.
    /// 产出该总结期间捕获到的最近一次成功 Provider 用量。
    pub last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    /// Provider identifier used by the successful compaction request.
    /// 成功压缩请求使用的 Provider 标识。
    pub provider_id: Option<String>,
    /// Model identifier used by the successful compaction request.
    /// 成功压缩请求使用的模型标识。
    pub model_id: Option<String>,
    /// Execution profile used by the successful compaction request.
    /// 成功压缩请求使用的执行配置。
    pub execution_profile: PromptCompactionExecutionProfile,
    /// Optional clone-only pruning report for the successful compaction attempt.
    /// 成功压缩尝试对应的可选仅克隆剪枝报告。
    pub input_prune_report: Option<CompactionInputPruneReport>,
}

/// Provider-facing launch profile selected for one prompt-compaction attempt.
/// 单次提示词压缩尝试选择的 Provider 可见启动配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptCompactionExecutionProfile {
    /// Standalone Codex Responses compact endpoint with route-bound opaque checkpoint items.
    /// 独立 Codex Responses compact 端点，并保留绑定路由的不透明检查点项。
    RemoteResponses,
    /// Full-fidelity cache-preserving launch that keeps normal prompt topology and tool schemas.
    /// 保持完整保真缓存的启动方式，保留普通提示词拓扑与工具 schema。
    CachePreservingFullFidelity,
    /// Minimal pruned launch that strips tool schemas and normal assistant prompt overhead.
    /// 极简裁剪启动方式，移除工具 schema 与普通助手提示词开销。
    PrunedMinimal,
}

impl PromptCompactionExecutionProfile {
    /// Returns the stable persisted label for this execution profile.
    /// 返回该执行配置的稳定持久化标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::RemoteResponses => "remote_responses",
            Self::CachePreservingFullFidelity => "cache_preserving_full_fidelity",
            Self::PrunedMinimal => "pruned_minimal",
        }
    }
}

/// Lightweight progress update emitted before buffered model output is committed.
/// 在缓冲的模型输出提交前发出的轻量进度更新。
#[derive(Debug, Clone)]
pub struct PromptCompactionProgressEvent {
    /// Stable machine-readable progress stage.
    /// 稳定的机器可读进度阶段。
    pub stage: String,
}

/// Incremental event emitted while one prompt-compaction request is running.
/// 单次提示词压缩请求运行期间发出的增量事件。
#[derive(Debug, Clone)]
pub enum PromptCompactionStreamEvent {
    /// Progress update that is safe to show even if the current attempt later retries.
    /// 即使当前尝试随后重试也可以安全展示的进度更新。
    Progress(PromptCompactionProgressEvent),
    /// A normal execution stream event that should be mirrored into transcript UI or task logs.
    /// 应镜像到转录界面或任务日志中的普通执行流事件。
    Stream(ExecutionStreamEvent),
    /// Terminal compaction result after the internal summary request settles.
    /// 内部总结请求收敛后的终态压缩结果。
    Finished(Result<PromptCompactionExecution, String>),
}

/// Builds a prompt-compaction progress stream item.
/// 构建一条提示词压缩进度流事件。
fn prompt_compaction_progress(stage: impl Into<String>) -> PromptCompactionStreamEvent {
    PromptCompactionStreamEvent::Progress(PromptCompactionProgressEvent {
        stage: stage.into(),
    })
}

/// Projects safe runtime lifecycle events into compaction progress updates.
/// 将安全的运行时生命周期事件投影为压缩进度更新。
fn prompt_compaction_progress_for_runtime_event(
    event: &ExecutionStreamEvent,
) -> Option<PromptCompactionStreamEvent> {
    match event {
        ExecutionStreamEvent::Prepared(_) => Some(prompt_compaction_progress("prepared")),
        ExecutionStreamEvent::RequestReady { .. } => {
            Some(prompt_compaction_progress("request_ready"))
        }
        ExecutionStreamEvent::ProviderRequestCheckpoint { .. } => {
            Some(prompt_compaction_progress("provider_checkpoint"))
        }
        ExecutionStreamEvent::ProviderLoweringRepair { .. } => {
            Some(prompt_compaction_progress("provider_lowering_repair"))
        }
        ExecutionStreamEvent::WireRequestReady { .. } => {
            Some(prompt_compaction_progress("wire_request_ready"))
        }
        ExecutionStreamEvent::RequestStarted { .. } => {
            Some(prompt_compaction_progress("request_started"))
        }
        ExecutionStreamEvent::ThinkingDelta { .. } => {
            Some(prompt_compaction_progress("thinking_delta"))
        }
        ExecutionStreamEvent::OutputDelta { .. } => {
            Some(prompt_compaction_progress("output_delta"))
        }
        ExecutionStreamEvent::ProviderRequestTelemetry(_)
        | ExecutionStreamEvent::ProviderRequestDurabilityBoundary { .. }
        | ExecutionStreamEvent::RuntimeEvent { .. }
        | ExecutionStreamEvent::ConversationCheckpoint { .. }
        | ExecutionStreamEvent::ContextCompactionRequired { .. }
        | ExecutionStreamEvent::ThinkingCompleted { .. }
        | ExecutionStreamEvent::TransientReasoningStatusChanged { .. }
        | ExecutionStreamEvent::Finished(_) => None,
    }
}

/// Converts a provider-reported remote Responses compaction usage object into turn accounting.
/// 将供应商报告的 Responses 远端压缩用量对象转换为回合用量统计。
///
/// # Parameters
/// 参数
/// `usage` is the optional raw OpenAI-compatible `usage` object retained by the compact response.
/// `usage` 是 compact 响应保留的可选 OpenAI 兼容原始 `usage` 对象。
///
/// # Returns
/// 返回值
/// A normalized core usage snapshot only when the protocol value is an object.
/// 仅当协议值为对象时返回规范化的核心用量快照。
fn remote_responses_usage_to_core(
    usage: Option<&serde_json::Value>,
) -> Option<ProviderUsageSnapshot> {
    usage
        .filter(|value| value.is_object())
        .map(normalize_openai_usage)
        .map(|usage| public_usage_to_core(&usage))
}

/// Attempts the explicitly enabled standalone Codex Responses compaction route before local prompt compaction.
/// 在本地提示词压缩前，尝试执行显式启用的独立 Codex Responses 远端压缩路由。
///
/// # Parameters
/// 参数
/// `runtime` owns the current provider routes and credential boundary; `session_id` scopes the
/// stable cache key; `plan` selects the source window; `full_context_state` preserves its lossless
/// protocol messages; `model_target` and `reasoning_effort` freeze the owning turn controls.
/// `runtime` 持有当前供应商路由与凭据边界；`session_id` 限定稳定缓存键；`plan` 选择源窗口；
/// `full_context_state` 保留其无损协议消息；`model_target` 与 `reasoning_effort` 冻结所属回合控制。
///
/// # Returns
/// 返回值
/// A route-bound remote result when the switch is enabled and the endpoint succeeds, `None` when
/// the route is disabled or a non-semantic remote failure permits local fallback, or an error when
/// the provider has already exposed semantic output or a side effect.
/// 当开关启用且端点成功时返回绑定路由的远端结果；当路由关闭或非语义远端失败允许本地回退时返回 `None`；当供应商已经产生语义输出或副作用时返回错误。
async fn try_remote_responses_compaction(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    foundation: &RuntimeFoundation,
    provider_configuration: &ProviderConfigurationFile,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    session_id: crate::model::SessionId,
    session_title: &str,
    workspace_path: &str,
    execution_agent_id: Option<&str>,
    launch_context: &PromptCompactionLaunchContext,
    plan: &CompactionPlan,
    full_context_state: &SessionConversationState,
    model_target: &PromptCompactionModelTarget,
    reasoning_effort: ReasoningEffortLevel,
) -> Result<Option<PromptCompactionExecution>, String> {
    if plan.source_messages.is_empty() {
        return Ok(None);
    }
    // preparedExecution freezes one route for lossless history projection and the eventual dispatch.
    // preparedExecution 为无损历史投影与最终分派冻结同一路由。
    let prepared_execution = match runtime
        .prepare_responses_compact_execution(
            model_target.provider_id.as_str(),
            model_target.connection_id.as_str(),
            model_target.model_id.as_str(),
        )
        .await
    {
        Ok(execution) => execution,
        Err(failure) => {
            remote_compaction_failure_allows_local_fallback(&failure)?;
            return Ok(None);
        }
    };
    // toolingRequest resolves the exact model-visible tool snapshot used by an ordinary cache-preserving turn.
    // toolingRequest 解析普通缓存保留回合使用的精确模型可见工具快照。
    let mut tooling_request = build_prompt_compaction_launch_request(
        session_id,
        session_title,
        workspace_path,
        execution_agent_id,
        launch_context,
        Some(model_target),
        reasoning_effort,
        full_context_state.clone(),
        ContextCompactionRequestPromptMode::FullFidelityMerge,
    );
    // Remote compaction carries the ordinary model-visible tool catalog and base instructions, not the local summarizer prompt policy.
    // 远端压缩携带普通回合的模型可见工具目录与基础指令，而不是本地总结器提示策略。
    tooling_request.prompt_purpose = ExecutionPromptPurpose::UserTurn;
    tooling_request.runtime_policy = launch_context.runtime_policy;
    // toolingPreparation freezes tool visibility, schemas, and the independently resolved route for race validation.
    // toolingPreparation 冻结工具可见性、Schema 与独立解析的路由，用于竞争校验。
    let tooling_preparation = match prepare_execution_with_tool_snapshot(
        foundation,
        luaskills_runtime,
        provider_configuration,
        &tooling_request,
    ) {
        Ok(preparation) => preparation,
        // Local tool preparation has not dispatched provider work, so the configured local path remains safe.
        // 本地工具准备尚未分派 Provider 工作，因此配置的本地路径仍可安全使用。
        Err(_) => return Ok(None),
    };
    validate_remote_compaction_tooling_route(
        &tooling_preparation.preparation.model_blueprint,
        &prepared_execution,
    )?;
    // providerTools retains the canonical host definitions before route-specific schema normalization.
    // providerTools 在路由专属 Schema 正规化前保留规范宿主定义。
    let provider_tools = tooling_preparation
        .tool_snapshot
        .resolved_tool_definitions
        .iter()
        .map(resolved_tool_definition_to_provider_tool_definition)
        .collect();
    // tools uses the identical compatibility projector as ordinary Responses execution.
    // tools 使用与普通 Responses 执行完全相同的兼容投影器。
    let tools = project_public_tool_definitions(&prepared_execution.route, provider_tools);
    // nativeTools mirrors ordinary Codex native-tool capability projection, including image generation.
    // nativeTools 镜像普通 Codex 原生工具能力投影，包括图片生成。
    let native_tools = resolve_public_native_tools(
        prepared_execution
            .route
            .protocol_route
            .protocol_profile_id
            .as_str(),
        prepared_execution
            .model
            .capabilities
            .output_resources
            .as_slice(),
        prepared_execution
            .model
            .capabilities
            .provider_native_tools
            .clone(),
    );
    // messages use the ordinary route-aware bridge so prior checkpoints, tools, and images survive recompaction.
    // messages 使用普通路由感知桥接，使既有检查点、工具与图片在再次压缩时得以保留。
    let mut messages = build_responses_compaction_messages(
        full_context_state,
        &plan.source_messages,
        &prepared_execution,
    )
    .await
    .map_err(|error| format!("Remote Responses compaction input projection failed: {error}"))?;
    if messages.is_empty() {
        return Ok(None);
    }
    // activeAgent is the exact definition selected during the already validated tooling preparation.
    // activeAgent 是已验证工具准备阶段选择的精确代理定义。
    let active_agent = foundation
        .agent_definition(&tooling_preparation.preparation.agent_id)
        .ok_or_else(|| {
            format!(
                "Prepared compaction agent `{}` is no longer available in the frozen runtime foundation.",
                tooling_preparation.preparation.agent_id
            )
        })?;
    // runtimePrompt renders the same ordinary-turn layered instructions used by normal Codex execution.
    // runtimePrompt 渲染与普通 Codex 执行相同的普通回合分层指令。
    let runtime_prompt = match build_runtime_prompt_layers_for_agent(
        &tooling_preparation.preparation,
        Some(active_agent),
    ) {
        Ok(runtime_prompt) => runtime_prompt,
        // Prompt composition has not dispatched provider work, so local compaction remains safe.
        // 提示词装配尚未分派 Provider 工作，因此本地压缩仍可安全执行。
        Err(_) => return Ok(None),
    };
    prepend_remote_compaction_instructions(&mut messages, runtime_prompt.fallback_text);
    let request = build_remote_responses_compaction_request(
        session_id,
        model_target.model_id.as_str(),
        messages,
        tools,
        native_tools,
        reasoning_effort,
    );
    let execution = match runtime
        .execute_prepared_responses_compact(prepared_execution, request)
        .await
    {
        Ok(execution) => execution,
        Err(failure) => {
            remote_compaction_failure_allows_local_fallback(&failure)?;
            return Ok(None);
        }
    };
    let provider_id = execution.provider_id.clone();
    let connection_id = execution.connection_id.clone();
    let route_fingerprint = execution.route_fingerprint.clone();
    let protocol_profile_id = execution.protocol_profile_id.clone();
    let model_id = execution.model_id.clone();
    // lastSuccessfulProviderUsage accounts for the physical remote compact request in turn statistics.
    // lastSuccessfulProviderUsage 将远端 compact 物理请求计入回合统计。
    let last_successful_provider_usage =
        remote_responses_usage_to_core(execution.result.usage.as_ref());
    let compaction_result = CompactionResult::from_remote_responses_with_metadata(
        plan,
        ProviderContextRoute {
            provider_id: provider_id.clone(),
            connection_id,
            route_fingerprint,
            protocol_profile_id,
            model_id: model_id.clone(),
        },
        execution.result.output_items,
        execution.result.usage,
        execution.result.response_id,
    )?;
    Ok(Some(PromptCompactionExecution {
        compaction_result,
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage,
        provider_id: Some(provider_id),
        model_id: Some(model_id),
        execution_profile: PromptCompactionExecutionProfile::RemoteResponses,
        input_prune_report: None,
    }))
}

/// Verifies that tool visibility was resolved against the immutable route used for compact dispatch.
/// 验证工具可见性是针对压缩分派所用不可变路由解析的。
///
/// # Parameters
/// 参数
/// `blueprint` is the independent host preparation snapshot and `execution` owns the credential-bearing
/// public route that will be dispatched.
/// `blueprint` 是独立宿主准备快照；`execution` 持有即将分派的含凭据公共路由。
///
/// # Returns
/// 返回值
/// `Ok(())` only when provider, service entry, connection, protocol, secondary route, and model all match.
/// 仅当供应商、服务入口、连接、协议、二级路由与模型全部一致时返回 `Ok(())`。
fn validate_remote_compaction_tooling_route(
    blueprint: &crate::agent_runtime::provider::ProviderRuntimeBlueprint,
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
) -> Result<(), String> {
    let route = &execution.route;
    if blueprint.provider_id != route.provider.id
        || blueprint.service_entry_id != route.entry.id
        || blueprint.connection_id != route.connection.id
        || blueprint.protocol_profile_id != route.protocol_route.protocol_profile_id
        || blueprint.secondary_route != route.secondary_route
        || blueprint.model_id != execution.model.id
    {
        return Err(String::from(
            "Remote Responses compaction tooling route changed during preparation; dispatch was rejected.",
        ));
    }
    Ok(())
}

/// Determines whether one controlled remote-compaction failure may use local prompt compaction.
/// 判断一次受控远端压缩失败是否允许改用本地提示词压缩。
///
/// # Parameters
/// 参数
/// `failure` carries dispatch, semantic-output, side-effect, authentication, and HTTP evidence.
/// `failure` 携带分派、语义输出、副作用、身份认证与 HTTP 证据。
///
/// # Returns
/// 返回值
/// Success only when local fallback is safe; otherwise an explicit terminal diagnostic.
/// 仅在本地回退安全时返回成功，否则返回显式终态诊断。
fn remote_compaction_failure_allows_local_fallback(
    failure: &vulcan_provider_adapters::ProviderExecutionFailure,
) -> Result<(), String> {
    if failure.error.semantic_output_observed || failure.error.side_effect_observed {
        return Err(format!(
            "Remote Responses compaction cannot safely fall back after dispatch: {failure}"
        ));
    }
    if matches!(
        failure.error.category,
        AiErrorCategory::Authentication | AiErrorCategory::PermissionDenied
    ) || matches!(failure.error.http_status, Some(401 | 403))
    {
        return Err(format!(
            "Remote Responses compaction authentication or permission failed: {failure}"
        ));
    }
    Ok(())
}

/// Prepends ordinary Codex base instructions without changing the canonical compact input order.
/// 在不改变规范压缩输入顺序的前提下前置普通 Codex 基础指令。
///
/// # Parameters
/// 参数
/// `messages` owns the route-projected compact input; `instructions` is the rendered ordinary-turn system prompt.
/// `messages` 持有路由投影后的压缩输入；`instructions` 是渲染后的普通回合系统提示词。
fn prepend_remote_compaction_instructions(messages: &mut Vec<AiMessage>, instructions: String) {
    if instructions.is_empty() {
        return;
    }
    messages.insert(
        0,
        AiMessage {
            role: MessageRole::System,
            content: vec![ContentItem::Text { text: instructions }],
            provider_context_items: Vec::new(),
        },
    );
}

/// Builds the provider-neutral request sent to the standalone Responses compact endpoint.
/// 构建发送给独立 Responses compact 端点的供应商无关请求。
///
/// # Parameters
/// 参数
/// `session_id` creates a stable cache scope; `model_id` is the active model snapshot; `messages`
/// is the lossless route-projected planner window.
/// `session_id` 创建稳定缓存作用域；`model_id` 是当前活动模型快照；`messages` 是无损路由投影后的规划窗口。
///
/// # Returns
/// 返回值
/// A continuation-free conversation request retaining canonical provider items and multimodal content.
/// 返回不带延续信息且保留规范供应商项与多模态内容的对话请求。
fn build_remote_responses_compaction_request(
    session_id: crate::model::SessionId,
    model_id: &str,
    messages: Vec<AiMessage>,
    tools: Vec<ToolDefinition>,
    native_tools: Vec<NativeToolKind>,
    reasoning_effort: ReasoningEffortLevel,
) -> AiExecutionRequest {
    // publicReasoningEffort carries the same effective turn effort into the official compact payload.
    // publicReasoningEffort 把同一生效回合推理档位带入官方压缩载荷。
    let public_reasoning_effort = public_reasoning_effort_from_level(reasoning_effort);
    // reasoningOutput mirrors Codex's current-turn summary control only when reasoning is active.
    // reasoningOutput 仅在推理启用时镜像 Codex 当前回合的摘要控制。
    let reasoning_output = if public_reasoning_effort.is_some() {
        ReasoningOutputPolicy::SummaryAuto
    } else {
        ReasoningOutputPolicy::ProviderDefault
    };
    AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: model_id.to_string(),
        // The root Vulcan task id is the exact Codex session/thread identity and prompt-cache scope.
        // Vulcan 根任务标识是精确的 Codex 会话/线程身份与提示缓存作用域。
        cache_scope_id: Some(session_id.to_string()),
        continuation: None,
        messages,
        tools,
        native_tools,
        tool_choice: Default::default(),
        parallel_tool_calls: Default::default(),
        structured_output: Default::default(),
        stop_sequences: Vec::new(),
        prompt_cache: Default::default(),
        reasoning_effort: public_reasoning_effort,
        reasoning_output,
        model_parameters: Default::default(),
        resources: Vec::new(),
        output_token_limit: None,
    }
}

/// Streams one prompt-based compaction job and exposes the model's thinking/output before applying the final compaction result.
/// 以流式方式执行单次提示词压缩任务，并在应用最终压缩结果前暴露模型思考与输出。
pub fn stream_prompt_compaction_job(
    request: PromptCompactionJobRequest,
) -> impl Stream<Item = PromptCompactionStreamEvent> + Send + 'static {
    stream! {
        // remoteFoundation resolves the exact ordinary-turn tool catalog before optional remote compaction.
        // remoteFoundation 在可选远端压缩前解析普通回合使用的精确工具目录。
        let remote_foundation = if request
            .model_target
            .as_ref()
            .is_some_and(|target| target.remote_compaction_enabled)
        {
            yield prompt_compaction_progress("prepare_remote_tooling");
            // foundation reuses the atomically published plugin/MCP generation owned by this queued job.
            // foundation 复用本排队任务持有的原子发布插件与 MCP 代次。
            let mut foundation = RuntimeFoundation::build_from_extension_generation_with_runtimes_and_plugins(
                &request.provider_configuration,
                &request.agent_profile_catalog,
                request.luaskills_runtime.clone(),
                Some(request.vulcan_plugin_runtime.clone()),
                Arc::clone(&request.extension_generation),
                Arc::clone(&request.task_runtime),
                Arc::clone(&request.question_runtime),
                Arc::clone(&request.command_session_runtime),
            );
            foundation.install_public_ai_runtime(Arc::clone(&request.public_ai_runtime));
            Some(foundation)
        } else {
            None
        };
        if let (Some(model_target), Some(foundation)) = (
            request.model_target.as_ref(),
            remote_foundation.as_ref(),
        ) {
            yield prompt_compaction_progress("remote_compaction_check");
            match try_remote_responses_compaction(
                request.public_ai_runtime.as_ref(),
                foundation,
                &request.provider_configuration,
                request.luaskills_runtime.clone(),
                request.session_id,
                &request.session_title,
                &request.workspace_path,
                request.execution_agent_id.as_deref(),
                &request.launch_context,
                &request.plan,
                &request.full_context_state,
                model_target,
                request.reasoning_effort,
            ).await {
                Ok(Some(execution)) => {
                    yield prompt_compaction_progress("remote_compaction_complete");
                    yield PromptCompactionStreamEvent::Finished(Ok(execution));
                    return;
                }
                Ok(None) => {
                    yield prompt_compaction_progress("remote_compaction_local_fallback");
                }
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(error));
                    return;
                }
            }
        }
        let mut prune_mode = initial_compaction_prune_mode(
            &request.full_context_state,
            request.input_budget,
            request.observed_input_tokens,
            request.estimated_input_tokens,
        );
        let mut full_fidelity_attempts = 0usize;
        let mut pruned_attempts = 0usize;
        let mut provider_request_count = 0usize;
        loop {
            if prune_mode == CompactionInputPruneMode::None {
                full_fidelity_attempts = full_fidelity_attempts.saturating_add(1);
            } else {
                pruned_attempts = pruned_attempts.saturating_add(1);
            }
            yield prompt_compaction_progress("prepare_input");
            let prepared_source = prepared_compaction_source_for_attempt(
                &request.plan,
                &request.full_context_state,
                request.input_budget,
                prune_mode,
            );
            yield prompt_compaction_progress("build_request");
            let source_conversation_state = match compaction_attempt_conversation_state(
                &request.full_context_state,
                &prepared_source.source_messages,
                prune_mode,
            ) {
                Ok(source_conversation_state) => source_conversation_state,
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(error));
                    return;
                }
            };
            let prompt_mode =
                compaction_prompt_mode_for_prune_mode(prepared_source.prune_report.mode);
            let launch_request = build_prompt_compaction_launch_request(
                request.session_id,
                &request.session_title,
                &request.workspace_path,
                request.execution_agent_id.as_deref(),
                &request.launch_context,
                request.model_target.as_ref(),
                request.reasoning_effort,
                source_conversation_state,
                prompt_mode,
            );

            yield prompt_compaction_progress("prepare_runtime");
            let launch = prepare_runtime_stream_launch_with_plugins_and_public_ai(
                &request.provider_configuration,
                &request.agent_profile_catalog,
                request.luaskills_runtime.clone(),
                Some(request.vulcan_plugin_runtime.clone()),
                Arc::clone(&request.extension_generation),
                Arc::clone(&request.task_runtime),
                Arc::clone(&request.question_runtime),
                Arc::clone(&request.command_session_runtime),
                Some(Arc::clone(&request.public_ai_runtime)),
                launch_request,
            )
            .await;

            let launch = match launch {
                Ok(launch) => {
                    yield prompt_compaction_progress("runtime_ready");
                    launch
                }
                Err(report) => {
                    let error = build_prompt_compaction_failure_message(
                        request.model_label.as_deref(),
                        &report,
                    );
                    match next_compaction_retry_mode(
                        &error,
                        prune_mode,
                        full_fidelity_attempts,
                        pruned_attempts,
                        request.input_budget,
                    ) {
                        Some(next_mode) => {
                            prune_mode = next_mode;
                            continue;
                        }
                        None => {
                            yield PromptCompactionStreamEvent::Finished(Err(
                                append_prune_report_to_error(error, &prepared_source.prune_report),
                            ));
                            return;
                        }
                    }
                }
            };

            let runtime_stream =
                drive_runtime_stream_launch(request.provider_configuration.clone(), launch, None);
            futures_util::pin_mut!(runtime_stream);
            let mut terminal_error = None;
            while let Some(event) = runtime_stream.next().await {
                match event {
                    ExecutionStreamEvent::Finished(report) => {
                        provider_request_count =
                            provider_request_count.saturating_add(report.provider_request_count);
                        match finalize_prompt_compaction_report(
                            &request.plan,
                            request.model_label.as_deref(),
                            report,
                        ) {
                            Ok(mut execution) => {
                                execution.provider_request_count = provider_request_count;
                                execution.input_prune_report = input_prune_report_for_execution(
                                    &prepared_source.prune_report,
                                );
                                yield PromptCompactionStreamEvent::Finished(Ok(execution));
                                return;
                            }
                            Err(error) => {
                                terminal_error = Some(error);
                                break;
                            }
                        }
                    }
                    event => {
                        if let Some(progress) = prompt_compaction_progress_for_runtime_event(&event)
                        {
                            yield progress;
                        }
                        yield PromptCompactionStreamEvent::Stream(event);
                    }
                }
            }

            let error = terminal_error.unwrap_or_else(|| {
                String::from("Prompt compaction stream ended without a terminal execution report.")
            });
            match next_compaction_retry_mode(
                &error,
                prune_mode,
                full_fidelity_attempts,
                pruned_attempts,
                request.input_budget,
            ) {
                Some(next_mode) => {
                    prune_mode = next_mode;
                }
                None => {
                    yield PromptCompactionStreamEvent::Finished(Err(
                        append_prune_report_to_error(error, &prepared_source.prune_report),
                    ));
                    return;
                }
            }
        }
    }
}

/// Streams one prompt-based compaction job through an already resolved runtime foundation.
/// 通过已解析运行时基础架构流式执行单次提示词式压缩任务。
pub fn stream_prompt_compaction_foundation_job(
    request: PromptCompactionFoundationJobRequest,
) -> impl Stream<Item = PromptCompactionStreamEvent> + Send + 'static {
    stream! {
        if let (Some(runtime), Some(model_target)) = (
            request.foundation.public_ai_runtime(),
            request
                .model_target
                .as_ref()
                .filter(|target| target.remote_compaction_enabled),
        ) {
            yield prompt_compaction_progress("remote_compaction_check");
            match try_remote_responses_compaction(
                runtime.as_ref(),
                &request.foundation,
                &request.provider_configuration,
                request.luaskills_runtime.clone(),
                request.session_id,
                &request.session_title,
                &request.workspace_path,
                request.execution_agent_id.as_deref(),
                &request.launch_context,
                &request.plan,
                &request.full_context_state,
                model_target,
                request.reasoning_effort,
            ).await {
                Ok(Some(execution)) => {
                    yield prompt_compaction_progress("remote_compaction_complete");
                    yield PromptCompactionStreamEvent::Finished(Ok(execution));
                    return;
                }
                Ok(None) => {
                    yield prompt_compaction_progress("remote_compaction_local_fallback");
                }
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(error));
                    return;
                }
            }
        }
        let mut prune_mode = initial_compaction_prune_mode(
            &request.full_context_state,
            request.input_budget,
            request.observed_input_tokens,
            request.estimated_input_tokens,
        );
        let mut full_fidelity_attempts = 0usize;
        let mut pruned_attempts = 0usize;
        let mut provider_request_count = 0usize;
        loop {
            if prune_mode == CompactionInputPruneMode::None {
                full_fidelity_attempts = full_fidelity_attempts.saturating_add(1);
            } else {
                pruned_attempts = pruned_attempts.saturating_add(1);
            }
            yield prompt_compaction_progress("prepare_input");
            let prepared_source = prepared_compaction_source_for_attempt(
                &request.plan,
                &request.full_context_state,
                request.input_budget,
                prune_mode,
            );
            yield prompt_compaction_progress("build_request");
            let source_conversation_state = match compaction_attempt_conversation_state(
                &request.full_context_state,
                &prepared_source.source_messages,
                prune_mode,
            ) {
                Ok(source_conversation_state) => source_conversation_state,
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(error));
                    return;
                }
            };
            let prompt_mode =
                compaction_prompt_mode_for_prune_mode(prepared_source.prune_report.mode);
            let launch_request = build_prompt_compaction_launch_request(
                request.session_id,
                &request.session_title,
                &request.workspace_path,
                request.execution_agent_id.as_deref(),
                &request.launch_context,
                request.model_target.as_ref(),
                request.reasoning_effort,
                source_conversation_state,
                prompt_mode,
            );

            yield prompt_compaction_progress("prepare_execution");
            let prepared = match prepare_execution_with_tool_snapshot(
                &request.foundation,
                request.luaskills_runtime.clone(),
                &request.provider_configuration,
                &launch_request,
            ) {
                Ok(prepared) => {
                    yield prompt_compaction_progress("execution_ready");
                    prepared
                }
                Err(error) => {
                    let error = format!("Prompt compaction preparation failed: {error}");
                    match next_compaction_retry_mode(
                        &error,
                        prune_mode,
                        full_fidelity_attempts,
                        pruned_attempts,
                        request.input_budget,
                    ) {
                        Some(next_mode) => {
                            prune_mode = next_mode;
                            continue;
                        }
                        None => {
                            yield PromptCompactionStreamEvent::Finished(Err(
                                append_prune_report_to_error(error, &prepared_source.prune_report),
                            ));
                            return;
                        }
                    }
                }
            };
            yield prompt_compaction_progress("bootstrap_runtime");
            let bootstrap = bootstrap_runtime_stream(
                &request.foundation,
                request.luaskills_runtime.clone(),
                &launch_request,
                prepared.preparation,
                prepared.tool_snapshot,
            )
            .await;
            let bootstrap = match bootstrap {
                Ok(bootstrap) => {
                    yield prompt_compaction_progress("runtime_ready");
                    bootstrap
                }
                Err(error) => {
                    let error = format!("Prompt compaction bootstrap failed: {error}");
                    match next_compaction_retry_mode(
                        &error,
                        prune_mode,
                        full_fidelity_attempts,
                        pruned_attempts,
                        request.input_budget,
                    ) {
                        Some(next_mode) => {
                            prune_mode = next_mode;
                            continue;
                        }
                        None => {
                            yield PromptCompactionStreamEvent::Finished(Err(
                                append_prune_report_to_error(error, &prepared_source.prune_report),
                            ));
                            return;
                        }
                    }
                }
            };
            let launch = build_runtime_stream_launch_payload(&launch_request, bootstrap, None);

            let runtime_stream =
                drive_runtime_stream_launch(request.provider_configuration.clone(), launch, None);
            futures_util::pin_mut!(runtime_stream);
            let mut terminal_error = None;
            while let Some(event) = runtime_stream.next().await {
                match event {
                    ExecutionStreamEvent::Finished(report) => {
                        provider_request_count =
                            provider_request_count.saturating_add(report.provider_request_count);
                        match finalize_prompt_compaction_report(
                            &request.plan,
                            request.model_label.as_deref(),
                            report,
                        ) {
                            Ok(mut execution) => {
                                execution.provider_request_count = provider_request_count;
                                execution.input_prune_report = input_prune_report_for_execution(
                                    &prepared_source.prune_report,
                                );
                                yield PromptCompactionStreamEvent::Finished(Ok(execution));
                                return;
                            }
                            Err(error) => {
                                terminal_error = Some(error);
                                break;
                            }
                        }
                    }
                    event => {
                        if let Some(progress) = prompt_compaction_progress_for_runtime_event(&event)
                        {
                            yield progress;
                        }
                        yield PromptCompactionStreamEvent::Stream(event);
                    }
                }
            }

            let error = terminal_error.unwrap_or_else(|| {
                String::from("Prompt compaction stream ended without a terminal execution report.")
            });
            match next_compaction_retry_mode(
                &error,
                prune_mode,
                full_fidelity_attempts,
                pruned_attempts,
                request.input_budget,
            ) {
                Some(next_mode) => {
                    prune_mode = next_mode;
                }
                None => {
                    yield PromptCompactionStreamEvent::Finished(Err(
                        append_prune_report_to_error(error, &prepared_source.prune_report),
                    ));
                    return;
                }
            }
        }
    }
}

/// Builds the shared execution-preparation request for prompt-based context compaction.
/// 构建提示词式上下文压缩使用的共享执行准备请求。
///
/// # Parameters
/// # 参数
/// `prompt_mode` selects the prompt variant that matches the prepared compaction input.
/// `prompt_mode` 选择与已准备压缩输入匹配的提示词变体。
fn build_prompt_compaction_launch_request(
    session_id: crate::model::SessionId,
    session_title: &str,
    workspace_path: &str,
    execution_agent_id: Option<&str>,
    launch_context: &PromptCompactionLaunchContext,
    model_target: Option<&PromptCompactionModelTarget>,
    reasoning_effort: ReasoningEffortLevel,
    conversation_state: vulcan_context::SessionConversationState,
    prompt_mode: ContextCompactionRequestPromptMode,
) -> ExecutionPreparationRequest {
    let prompt_purpose = match prompt_mode {
        ContextCompactionRequestPromptMode::FullFidelityMerge => {
            ExecutionPromptPurpose::ContextCompactionCachePreserving
        }
        ContextCompactionRequestPromptMode::PrunedFallback => {
            ExecutionPromptPurpose::ContextCompaction
        }
    };
    ExecutionPreparationRequest {
        session_id,
        managed_codebase_memory: None,
        execution_scope: vulcan_session_core::ExecutionScope::ContextCompaction {
            node_id: vulcan_session_core::SessionNodeId::from(session_id),
        },
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::context_compaction(),
        conversation_hook_turn_id: None,
        execution_retry_snapshot: None,
        execution_agent_id: execution_agent_id.map(str::to_string),
        agent_prompt_snapshot: launch_context.agent_prompt_snapshot.clone(),
        provider_id_override: model_target.map(|target| target.provider_id.clone()),
        model_id_override: model_target.map(|target| target.model_id.clone()),
        connection_id_override: model_target.map(|target| target.connection_id.clone()),
        provider_route_lease_identity: model_target
            .map(|target| target.provider_route_lease_identity.clone()),
        provider_usage_budget_baseline: None,
        provider_usage_context_generation_id: None,
        provider_tool_selections_override: launch_context.provider_tool_selections_override.clone(),
        provider_extension_target_sets_override: launch_context
            .provider_extension_target_sets_override
            .clone(),
        subagent_prompt_context: launch_context.subagent_prompt_context.clone(),
        task_session_store_root: launch_context.task_session_store_root.clone(),
        session_title: session_title.to_string(),
        workspace_path: workspace_path.to_string(),
        prompt: build_compaction_request_prompt_for_mode(prompt_mode),
        output_style_snapshot: launch_context.output_style_snapshot.clone(),
        provider_visible_agent_messages: Vec::new(),
        turn_start_mode: None,
        conversation_state,
        context_compaction_policy: model_target.map(|target| target.context_compaction_policy),
        context_budget_override: None,
        context_window_prefill_input_tokens: None,
        execution_mode: launch_context.execution_mode.clone(),
        temperature: None,
        reasoning_effort,
        tool_exposure_override: launch_context.tool_exposure_override.clone(),
        tool_request_rejection_reason: launch_context.tool_request_rejection_reason.clone(),
        tool_request_rejection_all_tools: launch_context.tool_request_rejection_all_tools,
        tool_request_rejection_tool_ids: launch_context.tool_request_rejection_tool_ids.clone(),
        tool_request_hidden_tool_ids: launch_context.tool_request_hidden_tool_ids.clone(),
        prompt_purpose,
        prompt_skill_policy_override: launch_context.prompt_skill_policy_override.clone(),
        project_prompt_skills: launch_context.project_prompt_skills.clone(),
        session_prompt_skill_overrides: launch_context.session_prompt_skill_overrides.clone(),
        context_injection: None,
        system_prompt_hook_blocks: Vec::new(),
        project_vmm: Default::default(),
        session_vmm: Default::default(),
        vmm_runtime_ready: false,
        boundary_events: Vec::new(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        knowledge_access: None,
        session_goal_context: None,
        session_plan_context: launch_context.session_plan_context.clone(),
        embedded_turn_usage_segments: Vec::new(),
    }
}

/// Finalizes one terminal execution report returned by the internal summary run into the prompt-based compaction result.
/// 将内部总结运行返回的终态执行报告整理为提示词式压缩结果。
fn finalize_prompt_compaction_report(
    plan: &CompactionPlan,
    model_label: Option<&str>,
    report: ExecutionRunReport,
) -> Result<PromptCompactionExecution, String> {
    if report.is_success() {
        let provider_id = report
            .preparation
            .as_ref()
            .map(|preparation| preparation.model_blueprint.provider_id.clone());
        let model_id = report
            .preparation
            .as_ref()
            .map(|preparation| preparation.model_blueprint.model_id.clone());
        let execution_profile = match report
            .preparation
            .as_ref()
            .map(|preparation| preparation.prompt_purpose)
        {
            Some(ExecutionPromptPurpose::ContextCompactionCachePreserving) => {
                PromptCompactionExecutionProfile::CachePreservingFullFidelity
            }
            _ => PromptCompactionExecutionProfile::PrunedMinimal,
        };
        let last_successful_provider_usage = report.last_successful_provider_usage.clone();
        let summary_text = report
            .assistant_output
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| build_prompt_compaction_failure_message(model_label, &report))?;
        let summary_text = extract_compaction_model_summary(summary_text)
            .ok_or_else(|| build_prompt_compaction_failure_message(model_label, &report))?;
        return Ok(PromptCompactionExecution {
            compaction_result: CompactionResult::from_model_summary(plan, summary_text),
            fallback_used: false,
            provider_request_count: report.provider_request_count,
            last_successful_provider_usage,
            provider_id,
            model_id,
            execution_profile,
            input_prune_report: None,
        });
    }

    Err(build_prompt_compaction_failure_message(
        model_label,
        &report,
    ))
}

/// Builds one human-readable failure message for prompt-based compaction without usable plain-text summary output.
/// 当提示词式压缩没有可用纯文本总结输出时构建人类可读失败信息。
fn build_prompt_compaction_failure_message(
    model_label: Option<&str>,
    report: &ExecutionRunReport,
) -> String {
    let model_label = model_label.unwrap_or("unknown-model");
    let report_error = report
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("the model did not return a usable plain-text summary");
    format!("Prompt compaction on {model_label} did not produce a usable summary: {report_error}.")
}
