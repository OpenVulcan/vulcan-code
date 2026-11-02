//! Shared prompt-based context-compaction runtime.
//! 共享提示词式上下文压缩运行时。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_models::{
    ExecutionAgentPromptSnapshot, ExecutionSubagentPromptContext,
};
use crate::agent_runtime::execution_preparation::prepare_execution_with_tool_snapshot;
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
use vulcan_context::{ContextCompactionPolicy, SessionConversationState};
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport, CompactionPlan,
    CompactionResult, ContextCompactionRequestPromptMode, build_compaction_request_prompt_for_mode,
    extract_compaction_model_summary,
};
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_prompt_composer::PromptOutputStyleSnapshot;
use vulcan_session_core::{CapabilityPolicySnapshot, ToolExposureSnapshot};
use vulcan_workbench_model::SessionProviderToolTargetSelection;

mod pruning;
pub(crate) mod standalone_owner;

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
    /// Exact sampling temperature captured from the owning request; compaction cannot silently change active request configuration.
    /// 从所属请求捕获的精确采样温度；压缩不能静默改变活动请求配置。
    pub temperature: Option<f32>,
    /// Exact frozen user-prompt media authority for the selected history; never inferred from files or model caches.
    /// 所选历史的精确冻结用户提示词媒体权威；绝不从文件或模型缓存推断。
    pub media_projection:
        Option<Arc<dyn crate::prompt_media_projection::PromptMediaProjectionRuntime>>,
    /// Exact execution mode used to resolve mode-specific tools.
    /// 用于解析模式专属工具的精确执行模式。
    pub execution_mode: String,
    /// Optional fixed agent prompt captured by a task session.
    /// 子任务会话捕获的可选固定代理提示词。
    pub agent_prompt_snapshot: Option<ExecutionAgentPromptSnapshot>,
    /// Optional provider-tool selections keyed by semantic capability.
    /// 按语义能力索引的可选供应商工具选择。
    pub provider_tool_selections_override:
        Option<BTreeMap<String, SessionProviderToolTargetSelection>>,
    /// Optional exact session-owned media-understanding routes captured from the source turn.
    /// 从来源回合捕获的可选会话自有精确媒体理解路由。
    pub provider_tool_media_understanding_selections_override:
        Option<BTreeMap<String, vulcan_workbench_model::SessionMediaUnderstandingSelection>>,
    /// Optional enabled direct and media capability keys captured from the source turn.
    /// 从来源回合捕获的可选已启用直连及媒体能力键。
    pub provider_tool_enabled_capabilities_override: Option<BTreeSet<String>>,
    /// Optional model-extension target sets captured from the source turn.
    /// 从来源回合捕获的可选模型扩展目标集合。
    pub provider_extension_target_sets_override:
        Option<BTreeMap<String, BTreeSet<SessionProviderToolTargetSelection>>>,
    /// Optional model-extension modes captured from the source turn.
    /// 从来源回合捕获的可选模型扩展模式。
    pub provider_extension_modes_override:
        Option<BTreeMap<String, vulcan_workbench_model::SessionProviderExtensionModeSelection>>,
    /// Optional child-agent prompt context captured by the source turn.
    /// 来源回合捕获的可选子代理提示词上下文。
    pub subagent_prompt_context: Option<ExecutionSubagentPromptContext>,
    /// Exact storage directory for the source session's compaction artifacts.
    /// 来源会话压缩工件使用的精确存储目录。
    pub session_storage_directory: Option<PathBuf>,
    /// Optional output-style snapshot rendered into the ordinary base instructions.
    /// 渲染到普通基础指令中的可选输出风格快照。
    pub output_style_snapshot: Option<PromptOutputStyleSnapshot>,
    /// Exact request-scoped tool exposure policy.
    /// 精确的请求级工具可见性策略。
    pub tool_exposure_override: ToolExposureSnapshot,
    /// Original restored-child tool admission retained across internal compaction.
    /// 内部压缩期间保留的原恢复子代理工具准入范围。
    pub captured_child_tool_admission:
        Option<BTreeMap<String, vulcan_session_core::v1::child_execution::ChildToolOrigin>>,
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
            temperature: None,
            media_projection: None,
            execution_mode: String::from("build"),
            agent_prompt_snapshot: None,
            provider_tool_selections_override: None,
            provider_tool_media_understanding_selections_override: None,
            provider_tool_enabled_capabilities_override: None,
            provider_extension_target_sets_override: None,
            provider_extension_modes_override: None,
            subagent_prompt_context: None,
            session_storage_directory: None,
            output_style_snapshot: None,
            tool_exposure_override: Default::default(),
            captured_child_tool_admission: None,
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
            temperature: request.temperature,
            // Compaction planning supplies its own exact source snapshot after the ordinary controls are copied.
            // 普通控制复制后，由压缩规划提供自身精确来源快照。
            media_projection: None,
            execution_mode: request.execution_mode.clone(),
            agent_prompt_snapshot: request.agent_prompt_snapshot.clone(),
            provider_tool_selections_override: request.provider_tool_selections_override.clone(),
            provider_tool_media_understanding_selections_override: request
                .provider_tool_media_understanding_selections_override
                .clone(),
            provider_tool_enabled_capabilities_override: request
                .provider_tool_enabled_capabilities_override
                .clone(),
            provider_extension_target_sets_override: request
                .provider_extension_target_sets_override
                .clone(),
            provider_extension_modes_override: request.provider_extension_modes_override.clone(),
            subagent_prompt_context: request.subagent_prompt_context.clone(),
            session_storage_directory: request.session_storage_directory.clone(),
            output_style_snapshot: request.output_style_snapshot.clone(),
            tool_exposure_override: request.tool_exposure_override.clone(),
            captured_child_tool_admission: request.captured_child_tool_admission.clone(),
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
    context_compaction_window_node_id: Option<vulcan_context::ContextCompactionWindowNodeId>,
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
        context_compaction_window_node_id,
        context_compaction_policy_seed,
    )?;
    Ok(Some(PromptCompactionModelTarget {
        provider_id: selection.route.provider.id.clone(),
        connection_id: selection.route.connection.id.clone(),
        model_id: selection.model.id.clone(),
        provider_route_lease_identity,
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
    /// Exact session-scoped provider, connection, and model used by compaction.
    /// 压缩使用的精确会话级供应商、连接与模型。
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

/// Declared acknowledgement authority for compaction launched from an existing runtime foundation.
/// 从已有运行时基础架构启动压缩时声明的确认权威。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptCompactionOwnership {
    /// The consumer owns durable V1 preparation and terminal acknowledgements.
    /// 消费方持有可靠 V1 准备及终态确认。
    SessionV1,
    /// The standalone runtime has no V1 document or durability consumer.
    /// 独立运行时没有 V1 文档或可靠性消费方。
    RuntimeOnly,
}

/// Immutable compaction inputs with an explicit acknowledgement owner.
/// 带明确确认所有者的不可变压缩输入。
#[derive(Clone)]
pub struct PromptCompactionFoundationJobRequest {
    /// The caller declares who consumes mandatory preparation and terminal barriers.
    /// 调用方声明谁消费强制准备及终态屏障。
    pub ownership: PromptCompactionOwnership,
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
    /// Exact task-scoped provider, connection, and model used by compaction.
    /// 压缩使用的精确任务级供应商、连接与模型。
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

/// Host admission purpose chosen at the original compaction call site.
/// 在原压缩调用点选择的宿主准入用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildTaskCompactionPhase {
    /// The original business execution still owns its active lease and open turn.
    /// 原业务执行仍持有活动租约和开放回合。
    ActiveBusiness,
    /// The result has been stored and the original active lease has already been released.
    /// 结果已经保存，原活动租约已经释放。
    CompletedBusinessMaintenance,
}

/// Trusted child invocation and resolved runtime inputs handed to its durable host for compaction.
/// 向可靠宿主交接用于压缩的可信子调用与已解析运行时输入。
#[derive(Clone)]
pub struct ChildTaskCompactionRequest {
    /// Explicit admission class; the host must never infer it by falling back after a failed lease lookup.
    /// 显式准入类别；宿主绝不能在租约查找失败后以兜底方式推断。
    pub phase: ChildTaskCompactionPhase,
    /// Original task and execution identity; its resident snapshot is not durable source authority.
    /// 原任务及执行身份；其驻留快照不具有可靠来源权威。
    pub invocation: crate::agent_runtime::task_runtime::TaskExecutionRequest,
    /// Actual child preparation retaining model overrides, prompt controls and workspace ownership.
    /// 实际子准备，保留模型覆盖、提示控制与工作区归属。
    pub preparation: ExecutionPreparationRequest,
    /// Exact provider configuration resolved for the child execution.
    /// 为子执行解析的精确供应商配置。
    pub provider_configuration: ProviderConfigurationFile,
    /// Exact declared summary target; the host must not choose another route.
    /// 精确声明的摘要目标；宿主不得选择其他路由。
    pub model_target: PromptCompactionModelTarget,
    /// Already initialized runtime foundation shared with the child.
    /// 与子代理共享的已初始化运行时基础。
    pub foundation: Arc<RuntimeFoundation>,
    /// Existing optional skill runtime retained by the child execution.
    /// 子执行保留的既有可选技能运行时。
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
    /// Exact source selected before each local attempt, including retries; never a model-authored event.
    /// 每次本地尝试及重试前选定的精确来源；绝不是模型编写的事件。
    PreparedSource {
        /// Source records after this attempt's pruning, preserving original record identities.
        /// 本次剪枝后的来源记录，保留原始记录身份。
        source: vulcan_context_compaction::PreparedCompactionSource,
        /// Requested mode, which also chooses full-state versus transcript reconstruction.
        /// 请求模式，同时决定采用完整状态还是转录重建。
        mode: CompactionInputPruneMode,
    },
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
        ExecutionStreamEvent::ProviderRequestPreparationBoundary { .. } => {
            Some(prompt_compaction_progress("provider_preparation"))
        }
        ExecutionStreamEvent::ProviderLoweringRepair { .. } => {
            Some(prompt_compaction_progress("provider_lowering_repair"))
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
        | ExecutionStreamEvent::ContextCompactionPolicyRefreshRequired { .. }
        | ExecutionStreamEvent::ProviderRouteSwitchRequired { .. }
        | ExecutionStreamEvent::ThinkingCompleted { .. }
        | ExecutionStreamEvent::TransientReasoningStatusChanged { .. }
        | ExecutionStreamEvent::Finished(_) => None,
    }
}

/// Streams one prompt-based compaction job and exposes the model's thinking/output before applying the final compaction result.
/// 以流式方式执行单次提示词压缩任务，并在应用最终压缩结果前暴露模型思考与输出。
pub fn stream_prompt_compaction_job(
    request: PromptCompactionJobRequest,
) -> impl Stream<Item = PromptCompactionStreamEvent> + Send + 'static {
    // Boxed so the generator's large state lives on the heap instead of inflating every caller's future.
    // 装箱使生成器的大体积状态位于堆上，而不是撑大每个调用方的 future。
    Box::pin(stream! {
        let mut prune_mode = match initial_compaction_prune_mode(
            &request.full_context_state,
            request.input_budget,
            request.observed_input_tokens,
            request.estimated_input_tokens,
        ) {
            Ok(mode) => mode,
            Err(error) => {
                yield PromptCompactionStreamEvent::Finished(Err(error));
                return;
            }
        };
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
            let prepared_source = match prepared_compaction_source_for_attempt(
                &request.plan,
                &request.full_context_state,
                request.input_budget,
                prune_mode,
            ) {
                Ok(source) => source,
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(error));
                    return;
                }
            };
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
            yield PromptCompactionStreamEvent::PreparedSource {
                source: prepared_source.clone(),
                mode: prune_mode,
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
                    ExecutionStreamEvent::ContextCompactionRequired { reason, .. } => {
                        terminal_error = Some(prompt_compaction_request_budget_error(&reason));
                        break;
                    }
                    ExecutionStreamEvent::ProviderRouteSwitchRequired { reason, .. } => {
                        terminal_error = Some(format!(
                            "Prompt compaction cannot change Provider route mid-operation: {reason}"
                        ));
                        break;
                    }
                    ExecutionStreamEvent::ContextCompactionPolicyRefreshRequired {
                        reason,
                        ..
                    } => {
                        terminal_error = Some(format!(
                            "Prompt compaction cannot refresh its context policy mid-operation: {reason}"
                        ));
                        break;
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
    })
}

/// Streams one prompt-based compaction job through an already resolved runtime foundation.
/// 通过已解析运行时基础架构流式执行单次提示词式压缩任务。
pub fn stream_prompt_compaction_foundation_job(
    request: PromptCompactionFoundationJobRequest,
) -> impl Stream<Item = PromptCompactionStreamEvent> + Send + 'static {
    // Boxed so the generator's large state lives on the heap instead of inflating every caller's future.
    // 装箱使生成器的大体积状态位于堆上，而不是撑大每个调用方的 future。
    Box::pin(stream! {
        let mut prune_mode = match initial_compaction_prune_mode(
            &request.full_context_state,
            request.input_budget,
            request.observed_input_tokens,
            request.estimated_input_tokens,
        ) {
            Ok(mode) => mode,
            Err(error) => {
                yield PromptCompactionStreamEvent::Finished(Err(error));
                return;
            }
        };
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
            let prepared_source = match prepared_compaction_source_for_attempt(
                &request.plan,
                &request.full_context_state,
                request.input_budget,
                prune_mode,
            ) {
                Ok(source) => source,
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(error));
                    return;
                }
            };
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
            yield PromptCompactionStreamEvent::PreparedSource {
                source: prepared_source.clone(),
                mode: prune_mode,
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
            let launch = match build_runtime_stream_launch_payload(&launch_request, bootstrap, None)
            {
                Ok(launch) => launch,
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(format!(
                        "Prompt compaction Provider request continuation validation failed: {error}"
                    )));
                    return;
                }
            };
            // A RuntimeOnly compaction owns no V1 session document, so its launch must declare its
            // Provider requests outside the session ledger before the transport publishes typed facts.
            // RuntimeOnly 压缩不持有 V1 会话文档，因此其启动必须在传输层发布强类型事实之前声明
            // 其 Provider 请求位于会话账本之外。
            let launch = match standalone_owner::declare_launch_provider_ownership(
                launch,
                request.ownership,
            ) {
                Ok(launch) => launch,
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(format!(
                        "Prompt compaction standalone Provider ownership declaration failed: {error}"
                    )));
                    return;
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
                    ExecutionStreamEvent::ContextCompactionRequired { reason, .. } => {
                        terminal_error = Some(prompt_compaction_request_budget_error(&reason));
                        break;
                    }
                    ExecutionStreamEvent::ProviderRouteSwitchRequired { reason, .. } => {
                        terminal_error = Some(format!(
                            "Prompt compaction cannot change Provider route mid-operation: {reason}"
                        ));
                        break;
                    }
                    ExecutionStreamEvent::ContextCompactionPolicyRefreshRequired {
                        reason,
                        ..
                    } => {
                        terminal_error = Some(format!(
                            "Prompt compaction cannot refresh its context policy mid-operation: {reason}"
                        ));
                        break;
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
    })
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
        execution_scope: vulcan_session_core::ExecutionScope::ContextCompaction {
            node_id: vulcan_session_core::SessionNodeId::from(session_id),
        },
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::context_compaction(),
        conversation_hook_turn_id: None,
        continuation_action_id: None,
        provider_request_continuation: None,
        execution_agent_id: execution_agent_id.map(str::to_string),
        agent_prompt_snapshot: launch_context.agent_prompt_snapshot.clone(),
        dedicated_session: None,
        provider_id_override: model_target.map(|target| target.provider_id.clone()),
        model_id_override: model_target.map(|target| target.model_id.clone()),
        connection_id_override: model_target.map(|target| target.connection_id.clone()),
        subagent_model_policy: None,
        provider_route_lease_identity: model_target
            .map(|target| target.provider_route_lease_identity.clone()),
        provider_tool_selections_override: launch_context.provider_tool_selections_override.clone(),
        provider_tool_media_understanding_selections_override: launch_context
            .provider_tool_media_understanding_selections_override
            .clone(),
        provider_tool_enabled_capabilities_override: launch_context
            .provider_tool_enabled_capabilities_override
            .clone(),
        provider_extension_target_sets_override: launch_context
            .provider_extension_target_sets_override
            .clone(),
        provider_extension_modes_override: launch_context.provider_extension_modes_override.clone(),
        subagent_prompt_context: launch_context.subagent_prompt_context.clone(),
        session_storage_directory: launch_context.session_storage_directory.clone(),
        session_title: session_title.to_string(),
        workspace_path: workspace_path.to_string(),
        project_id: None,
        prompt: build_compaction_request_prompt_for_mode(prompt_mode),
        output_style_snapshot: launch_context.output_style_snapshot.clone(),
        provider_visible_agent_messages: Vec::new(),
        turn_start_mode: None,
        conversation_state,
        // The isolated summary request must retain the hard model window but cannot recursively trigger automatic compaction.
        // 隔离摘要请求必须保留模型硬窗口，但不能递归触发自动压缩。
        context_compaction_policy: model_target.map(|target| ContextCompactionPolicy {
            auto: false,
            ..target.context_compaction_policy
        }),
        context_compaction_window_node_id: model_target
            .and_then(|target| target.context_compaction_policy.compression_window_node_id),
        context_budget_override: None,
        context_window_prefill_input_tokens: None,
        execution_mode: launch_context.execution_mode.clone(),
        temperature: launch_context.temperature,
        session_request_options: Default::default(),
        reasoning_effort,
        tool_exposure_override: launch_context.tool_exposure_override.clone(),
        captured_child_tool_admission: launch_context.captured_child_tool_admission.clone(),
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
        native_goal_runtime: None,
        native_browser_runtime: None,
        native_agent_collaboration_runtime: None,
        supplemental_mcp_tools: Vec::new(),
        media_resource_runtime: None,
        knowledge_access: None,
        session_goal_context: None,
        session_plan_context: launch_context.session_plan_context.clone(),
        embedded_turn_usage_segments: Vec::new(),
    }
}

/// Converts an isolated summary-request budget boundary into an explicit context-window failure.
/// 将隔离摘要请求的预算边界转换为明确的上下文窗口失败。
///
/// # Parameters
/// 参数
/// `reason` is the exact request-budget diagnostic emitted before Provider dispatch.
/// `reason` 是 Provider 分派前发出的精确请求预算诊断。
///
/// # Returns
/// 返回值
/// An error that preserves the diagnostic and activates the existing pruned-input retry path.
/// 返回保留诊断并激活现有输入剪枝重试路径的错误。
fn prompt_compaction_request_budget_error(reason: &str) -> String {
    format!("Prompt compaction request exceeded its context window: {reason}")
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
