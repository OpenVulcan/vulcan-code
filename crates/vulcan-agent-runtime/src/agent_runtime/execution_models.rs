//! Shared execution-model types reused by the executor, runtime events, and module exports.
//! 供执行器、运行时事件与模块导出复用的共享执行模型类型。

use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::context_budget::{ContextBudgetSnapshot, ContextCompactionPolicy};
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use crate::provider_config::ReasoningEffortLevel;
use crate::provider_config::{
    PersistedPromptConfiguration, PersistedPromptPersonalitySelection,
    ProjectPromptSkillsConfiguration,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
use vulcan_ai_contract::AiErrorCategory;
use vulcan_ai_contract::HostMessageKind;
use vulcan_context::{
    ContextAssembly, ContextBaseline, ContextRequestMode, RuntimeSystemUpdateEvent,
};
use vulcan_core::types::{
    ConversationAttachment, CoreEvent as CoreControllerEvent,
    ProviderContextInjection as CoreProviderContextInjection, ProviderUsageSnapshot,
};
use vulcan_hooks::SystemPromptHookBlock;
use vulcan_prompt_composer::{AdditionalPromptSegment, PromptOutputStyleSnapshot};
use vulcan_prompt_skills::ResolvedPromptSkill;
use vulcan_session_boundary::SessionBoundaryEvent;
use vulcan_session_core::v1::ClientActionId;
use vulcan_session_core::{
    CapabilityPolicySnapshot, ExecutionRuntimePolicy, ExecutionScope, ToolExposureSnapshot,
};
use vulcan_tools_manager::{VulcanToolDescriptor, VulcanToolSourceKind};
use vulcan_vmm::VmmConfigLayer;
use vulcan_workbench_model::SessionTurnUsageSegment;
use vulcan_workspace_store::data_plane::HostRuntimeHandle;

/// Request-scoped dedicated-session shaping copied from the creation-frozen session profile.
/// 从创建时冻结的会话配置复制的请求级专职会话塑形。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedicatedExecutionProfile {
    /// General tool access chosen at creation.
    /// 创建时选择的通用工具开放方式。
    pub tool_access: vulcan_session_core::v1::DedicatedToolAccess,
    /// User-authored base prompt sent as the session's own system instruction.
    /// 作为会话自身系统指令发送的用户基础提示词。
    pub base_prompt: String,
}

/// Request-scoped agent prompt snapshot consumed by the shared prompt composer.
/// 由共享提示词组合器消费的请求级代理提示词快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionAgentPromptSnapshot {
    /// Prompt text used as the agent prompt layer.
    /// 用作代理提示词层的提示词文本。
    pub text: String,
    /// Source of this fixed prompt snapshot.
    /// 该固定提示词快照的来源。
    pub source: ExecutionAgentPromptSnapshotSource,
}

/// Source kind for one request-scoped agent prompt snapshot.
/// 单个请求级代理提示词快照的来源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionAgentPromptSnapshotSource {
    /// Fixed parent runtime prompt captured for a child-agent execution.
    /// 为智能体执行捕获的固定父级运行时提示词。
    FixedParentRuntime,
}

impl ExecutionAgentPromptSnapshot {
    /// Builds one fixed parent-runtime prompt snapshot from non-empty text.
    /// 从非空文本构建固定父级运行时提示词快照。
    ///
    /// # Parameters
    /// 参数
    /// `text` is the captured parent runtime agent prompt text.
    /// `text` 是捕获到的父级运行时代理提示词文本。
    ///
    /// # Returns
    /// 返回值
    /// A fixed prompt snapshot, or `None` when the text is empty after trimming.
    /// 固定提示词快照；如果去除空白后为空则返回 `None`。
    pub fn fixed_parent_runtime(text: impl Into<String>) -> Option<Self> {
        let text = text.into();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(Self {
            text: trimmed.to_string(),
            source: ExecutionAgentPromptSnapshotSource::FixedParentRuntime,
        })
    }

    /// Returns the prompt-composer source label for this snapshot.
    /// 返回该快照用于提示词组合器的来源标签。
    ///
    /// # Returns
    /// 返回值
    /// A stable human-readable diagnostics label.
    /// 稳定的人类可读诊断标签。
    pub const fn source_label(&self) -> &'static str {
        match self.source {
            ExecutionAgentPromptSnapshotSource::FixedParentRuntime => {
                "Fixed parent runtime agent prompt snapshot"
            }
        }
    }
}

/// Provider-visible typed agent message that starts or joins one runtime turn.
/// 用于启动或加入运行回合的 Provider 可见类型化智能体消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionProviderVisibleAgentMessage {
    /// Stable delivery identifier shared by mailbox, runtime, and display records.
    /// 由 mailbox、runtime 与显示记录共享的稳定投递标识。
    pub delivery_id: String,
    /// Explicit task envelope kind assigned by the producing lifecycle.
    /// 生产生命周期明确分配的任务信封类型。
    pub kind: HostMessageKind,
    /// Stable model-visible inter-agent message type.
    /// 模型可见的稳定智能体间消息类型。
    pub message_type: String,
    /// Stable task identity independent from the display task name or agent path.
    /// 独立于展示任务名或代理路径的稳定任务身份。
    pub task_id: String,
    /// Logical task or agent path associated with the message.
    /// 该消息关联的逻辑任务或智能体路径。
    pub task_name: String,
    /// Logical sender name or path.
    /// 逻辑发送方名称或路径。
    pub sender: String,
    /// Logical recipient name or path.
    /// 逻辑接收方名称或路径。
    pub recipient: String,
    /// Model-visible child-agent result text.
    /// 模型可见的智能体结果正文。
    pub payload: String,
    /// Whether the message may wake an idle parent turn.
    /// 该消息是否允许唤醒空闲父级回合。
    pub trigger_turn: bool,
    /// Unix timestamp in milliseconds used for transcript records.
    /// 用于转录记录的 Unix 毫秒时间戳。
    pub timestamp_ms: u64,
    /// Host-only structured audit data.
    /// 仅供宿主侧使用的结构化审计数据。
    pub metadata: Option<Value>,
}

impl ExecutionProviderVisibleAgentMessage {}

/// Request-scoped session goal binding captured before provider execution.
/// 在 Provider 执行前捕获的请求级会话目标绑定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSessionGoalContext {
    /// Stable goal identifier that owns usage and lifecycle results from this execution.
    /// 拥有本次执行用量与生命周期结果的稳定目标标识。
    pub goal_id: String,
    /// Stable goal guidance delivered through an append-only provider-visible turn.
    /// 通过追加式 Provider 可见回合投递的稳定目标引导。
    pub prompt_context: String,
}

/// Immutable knowledge-library authorization captured for one provider execution.
/// 为一次 Provider 执行捕获的不可变知识库授权快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionKnowledgeAccessContext {
    /// Project that owns the current conversation.
    /// 拥有当前会话的项目。
    pub project_id: String,
    /// Exact authorized and session-enabled library identifiers.
    /// 精确的已授权且已在会话启用的知识库标识符。
    pub library_ids: Vec<String>,
    /// Whether any selected library declares a searchable mode in this turn snapshot.
    /// 本回合快照中是否有任何选中库声明可检索模式。
    pub search_enabled: bool,
}

/// Exact logical Provider-request identity retained across one host-owned stream restart.
/// 跨一次宿主拥有的流重启保留的精确逻辑 Provider 请求身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionProviderRequestContinuation {
    /// Stable logical Provider request identifier shared by every physical attempt.
    /// 由全部物理尝试共享的稳定逻辑 Provider 请求标识。
    pub logical_request_id: String,
    /// Optional canonical conversation-turn identifier retained by the logical request.
    /// 由该逻辑请求保留的可选规范会话回合标识。
    pub logical_turn_id: Option<String>,
    /// Zero-based physical Provider attempt index assigned to the replacement stream's first request.
    /// 分配给替代流首次请求的从零开始物理 Provider 尝试索引。
    pub next_provider_attempt_index: u32,
    /// Manual Continue action retained only until its first durable API permit is confirmed.
    /// 仅在首次可靠 API 许可确认前保留的手动 Continue 动作。
    pub pending_continuation_action_id: Option<vulcan_session_core::v1::ClientActionId>,
    /// Permission to restore a retained assistant tail throughout the admitted Provider loop.
    /// 在已准入 Provider 循环全程中恢复保留助手尾部的能力。
    /// Consuming the Continue action binds one API; it does not revoke this loop capability.
    /// 消费 Continue 动作只会绑定一次 API，不会撤销此循环能力。
    pub allows_assistant_tail_restore: bool,
}

/// Resolves assistant-tail recovery from the authoritative admission or host handoff.
/// 从权威准入信息或宿主交接解析助手尾部恢复能力。
/// `continuation_action_id` identifies the initial admission; `continuation` owns subsequent restarts.
/// `continuation_action_id` 标识首次准入；`continuation` 持有后续重启的状态。
/// Returns the loop capability independently of whether its one-shot action has been consumed.
/// 返回循环恢复能力，不受对应一次性动作是否已消费的影响。
pub(crate) fn resolve_assistant_tail_restore(
    continuation_action_id: Option<&vulcan_session_core::v1::ClientActionId>,
    continuation: Option<&ExecutionProviderRequestContinuation>,
) -> bool {
    // A handoff is authoritative; only the initial stream derives capability from admission.
    // 交接状态是权威来源；只有首次流根据准入动作建立能力。
    match continuation {
        Some(continuation) => continuation.allows_assistant_tail_restore,
        None => continuation_action_id.is_some(),
    }
}

/// Minimal execution request captured from the current workbench state.
/// 从当前工作台状态捕获的最小执行请求。
#[derive(Debug, Clone)]
pub struct ExecutionPreparationRequest {
    /// Target session identifier.
    /// 目标会话标识。
    pub session_id: SessionId,
    /// Shared execution scope used by main-session and task-session routers.
    /// 主会话与子代理会话路由共用的执行范围。
    pub execution_scope: ExecutionScope,
    /// Runtime feature policy enforced for this execution request.
    /// 本次执行请求强制执行的运行时能力策略。
    pub runtime_policy: ExecutionRuntimePolicy,
    /// Optional immutable conversation Hook turn identifier captured by a Hook-enabled main-session request.
    /// 启用 Hook 的主会话请求捕获的可选不可变会话 Hook 回合标识。
    pub conversation_hook_turn_id: Option<String>,
    /// Optional durable client action that owns a V1 Continue execution.
    /// 可选的可靠客户端动作，用于标识一项 V1 Continue 执行。
    pub continuation_action_id: Option<ClientActionId>,
    /// Exact logical Provider-request continuation installed after a replay-safe host restart.
    /// 在可安全回放的宿主重启后安装的精确逻辑 Provider 请求延续身份。
    pub provider_request_continuation: Option<ExecutionProviderRequestContinuation>,
    /// Optional agent identifier that should own this execution instead of the globally active agent.
    /// 可选的本次执行代理标识，用于替代全局激活代理。
    pub execution_agent_id: Option<String>,
    /// Optional request-scoped fixed agent prompt snapshot.
    /// 可选的请求级固定代理提示词快照。
    pub agent_prompt_snapshot: Option<ExecutionAgentPromptSnapshot>,
    /// Dedicated-session request shaping, when the caller serves a dedicated session.
    /// 调用方服务于专职会话时的请求组成配置。
    pub dedicated_session: Option<DedicatedExecutionProfile>,
    /// Optional request-scoped provider override resolved from a child-agent model selector.
    /// 可选的请求级供应商覆盖，由子智能体模型选择器解析得到。
    pub provider_id_override: Option<String>,
    /// Optional request-scoped model override resolved from a child-agent model selector.
    /// 可选的请求级模型覆盖，由子智能体模型选择器解析得到。
    pub model_id_override: Option<String>,
    /// Optional request-scoped exact credential connection selected by the session.
    /// 会话选择的可选请求级精确凭据连接。
    pub connection_id_override: Option<String>,
    /// Session-level subagent model allowlist; `None` follows the system default list.
    /// 会话级子代理模型白名单；`None` 表示跟随系统默认清单。
    pub subagent_model_policy: Option<crate::provider_config::SubagentModelPolicy>,
    /// Persisted non-secret identity expected for this session's provider-route lease.
    /// 当前会话供应商路由租约预期的持久化非秘密身份。
    pub provider_route_lease_identity:
        Option<vulcan_workbench_model::SessionProviderRouteLeaseIdentity>,
    /// Optional session-selected provider tool targets keyed by semantic capability.
    /// 按语义能力索引的可选会话级供应商工具目标选择。
    pub provider_tool_selections_override: Option<
        std::collections::BTreeMap<
            String,
            vulcan_workbench_model::SessionProviderToolTargetSelection,
        >,
    >,
    /// Optional exact session-owned media-understanding routes keyed by capability.
    /// 按能力索引的可选会话自有精确媒体理解路由。
    pub provider_tool_media_understanding_selections_override: Option<
        std::collections::BTreeMap<
            String,
            vulcan_workbench_model::SessionMediaUnderstandingSelection,
        >,
    >,
    /// Optional session-enabled direct and media capability keys, independent from backend identity.
    /// 与后端身份分离的可选会话级已启用直连及媒体能力键。
    pub provider_tool_enabled_capabilities_override: Option<std::collections::BTreeSet<String>>,
    /// Optional session-selected native model-extension target sets keyed by extension capability.
    /// 按扩展能力索引的可选会话级原生模型扩展目标集合。
    pub provider_extension_target_sets_override: Option<
        std::collections::BTreeMap<
            String,
            std::collections::BTreeSet<vulcan_workbench_model::SessionProviderToolTargetSelection>,
        >,
    >,
    /// Optional session extension modes; a capability missing from Some(map) is single mode
    /// without a model, and None follows the system configuration.
    /// 可选会话扩展模式；Some(映射) 中缺失的能力表示未选模型的单模型模式，None 跟随系统配置。
    pub provider_extension_modes_override: Option<
        std::collections::BTreeMap<
            String,
            vulcan_workbench_model::SessionProviderExtensionModeSelection,
        >,
    >,
    /// Optional child-agent prompt context routed through the shared prompt composer.
    /// 通过共享提示词组合器路由的可选子智能体提示词上下文。
    pub subagent_prompt_context: Option<ExecutionSubagentPromptContext>,
    /// Exact storage directory for the session that owns this physical execution.
    /// 拥有本次物理执行的会话对应精确存储目录。
    pub session_storage_directory: Option<PathBuf>,
    /// Target session title.
    /// 目标会话标题。
    pub session_title: String,
    /// Current workspace path.
    /// 当前工作区路径。
    pub workspace_path: String,
    /// Stable VulcanCode project identity owning the current execution when one exists.
    /// 当前执行存在所属项目时对应的稳定 VulcanCode 项目身份。
    pub project_id: Option<String>,
    /// Current composer prompt.
    /// 当前提示词输入区内容。
    pub prompt: String,
    /// Output-style snapshot captured at a safe prompt boundary.
    /// 在安全提示词边界捕获的输出风格快照。
    pub output_style_snapshot: Option<PromptOutputStyleSnapshot>,
    /// Provider-visible typed agent messages seeded before this request reaches the controller.
    /// 本请求到达控制器前预先播种的 Provider 可见类型化智能体消息。
    pub provider_visible_agent_messages: Vec<ExecutionProviderVisibleAgentMessage>,
    /// Optional explicit controller turn-start mode; absence preserves the existing inference rule.
    /// 可选的控制器回合启动模式；缺省时保留现有的自动推断规则。
    pub turn_start_mode: Option<ExecutionTurnStartMode>,
    /// Canonical conversation state reconstructed from the session protocol state.
    /// 从会话协议状态重建出的规范会话状态。
    pub conversation_state: SessionConversationState,
    /// Optional context-compaction policy resolved for this execution request.
    /// 为本次执行请求解析出的可选上下文压缩策略。
    pub context_compaction_policy: Option<ContextCompactionPolicy>,
    /// Optional context window (node or custom value) selected by the owning session.
    /// 所属会话选择的可选上下文窗口（节点或自定义值）。
    pub context_window_selection: Option<vulcan_context::ContextWindowSelection>,
    /// Optional request-level input-token budget limit resolved for this execution request.
    /// 为本次执行请求解析出的可选输入 Token 预算上限。
    pub context_budget_override: Option<usize>,
    /// Absolute input-token baseline for the active BodyAfterPrefix compaction window.
    /// 当前 BodyAfterPrefix 压缩窗口使用的绝对输入 Token 基线。
    pub context_window_prefill_input_tokens: Option<usize>,
    /// Current execution-mode label.
    /// 当前执行模式标签。
    pub execution_mode: String,
    /// Optional sampling temperature override from the run-time controls.
    /// 来自运行参数控制区的可选采样温度覆盖值。
    pub temperature: Option<f32>,
    /// Session request options (prompt-cache TTL, service tier) applied when the route supports them.
    /// 路由支持时应用的会话请求选项（提示词缓存 TTL、服务档位）。
    pub session_request_options: vulcan_session_core::v1::config::RequestOptionsConfig,
    /// Shared reasoning-effort preset selected by the current workbench run controls.
    /// 当前工作台运行控制区选择的共享推理档位预设。
    pub reasoning_effort: ReasoningEffortLevel,
    /// Request-scoped tool exposure override applied by the shared preparation bridge.
    /// 由共享准备桥应用的请求级工具可见性覆盖。
    pub tool_exposure_override: ToolExposureSnapshot,
    /// Original callable tool origins limiting restored child business, Q&A and their compaction requests.
    /// 限制恢复子业务、答疑及其压缩请求的原可调用工具来源。
    pub captured_child_tool_admission: Option<
        std::collections::BTreeMap<
            String,
            vulcan_session_core::v1::child_execution::ChildToolOrigin,
        >,
    >,
    /// Optional reason used to reject visible tool calls before host execution.
    /// 在宿主执行前拒绝可见工具调用时使用的可选原因。
    pub tool_request_rejection_reason: Option<String>,
    /// Whether the rejection reason applies to every visible tool.
    /// 拒绝原因是否应用到全部可见工具。
    pub tool_request_rejection_all_tools: bool,
    /// Optional scoped tool ids rejected with `tool_request_rejection_reason`.
    /// 使用 `tool_request_rejection_reason` 拒绝的可选限定工具标识。
    pub tool_request_rejection_tool_ids: Vec<String>,
    /// Request-scoped public tool ids that must be hidden after normal exposure resolution.
    /// 正常可见性解析后仍必须按请求隐藏的公开工具标识集合。
    pub tool_request_hidden_tool_ids: Vec<String>,
    /// Prompt purpose used to choose runtime prompt assembly and tool exposure.
    /// 用于选择运行时提示词装配与工具暴露策略的提示词用途。
    pub prompt_purpose: ExecutionPromptPurpose,
    /// Request-scoped prompt-skill visibility policy applied after lifecycle state resolution.
    /// 在生命周期状态解析后应用的请求级提示词技能可见性策略。
    pub prompt_skill_policy_override: CapabilityPolicySnapshot,
    /// Project-level prompt-skill overrides captured for the current workspace.
    /// 为当前工作区捕获的项目级提示词技能覆盖。
    pub project_prompt_skills: ProjectPromptSkillsConfiguration,
    /// Session-level prompt-skill overrides captured for the current conversation.
    /// 为当前会话捕获的会话级提示词技能覆盖。
    pub session_prompt_skill_overrides:
        std::collections::BTreeMap<String, crate::provider_config::PersistedPromptSkillState>,
    /// Provider-facing context-injection overlay captured for the current run.
    /// 为当前运行捕获的 Provider 可见上下文注入覆盖层。
    pub context_injection: Option<CoreProviderContextInjection>,
    /// Request-scoped system-prompt hook blocks captured for the current run.
    /// 为当前运行捕获的请求级系统提示词 hook 块。
    pub system_prompt_hook_blocks: Vec<SystemPromptHookBlock>,
    /// Project-level VMM configuration captured for the current workspace.
    /// 为当前工作区捕获的项目级 VMM 配置。
    pub project_vmm: VmmConfigLayer,
    /// Session-level VMM configuration captured for the current conversation.
    /// 为当前会话捕获的会话级 VMM 配置。
    pub session_vmm: VmmConfigLayer,
    /// Whether the exact VMM transport was live when this execution request was admitted.
    /// 本次执行请求获准时准确 VMM 传输是否处于实时就绪状态。
    pub vmm_runtime_ready: bool,
    /// session-boundary boundary events accepted before this provider execution starts.
    /// 本次 Provider 执行开始前已经接受的 会话边界边界事件。
    pub boundary_events: Vec<SessionBoundaryEvent>,
    /// Chronological runtime system updates loaded from `runtime.jsonl` for provider-visible lowering.
    /// 从 `runtime.jsonl` 加载并用于 Provider 可见转换的时间顺序 runtime system update。
    pub runtime_system_updates: Vec<RuntimeSystemUpdateEvent>,
    /// Optional controller runtime used by host-owned tools that need durable workspace state.
    /// 需要持久工作区状态的宿主工具使用的可选控制器运行时。
    pub controller_runtime: Option<HostRuntimeHandle>,
    /// Optional host-injected V1 Goal domain port used by native Goal tools.
    /// 原生 Goal 工具使用的可选宿主注入 V1 Goal 领域端口。
    pub native_goal_runtime: Option<Arc<dyn crate::tooling::native::NativeGoalRuntime>>,
    /// Optional host-injected browser scheduler for direct AI tools in the trusted conversation.
    /// 可信会话内 AI 直接工具使用的可选宿主注入浏览器调度接口。
    pub native_browser_runtime: Option<Arc<dyn crate::tooling::native::NativeBrowserRuntime>>,
    /// Optional host-injected agent-collaboration runtime for question and session tools.
    /// 问答与会话工具使用的可选宿主注入代理协作运行时。
    pub native_agent_collaboration_runtime:
        Option<Arc<dyn crate::tooling::collaboration::NativeAgentCollaborationRuntime>>,
    /// Owner-scoped MCP bindings discovered before a provider turn and subjected to normal tool policy.
    /// Provider 回合前发现并接受常规工具策略过滤的所有者范围 MCP 绑定。
    pub supplemental_mcp_tools: Vec<crate::mcp::McpRuntimeToolBinding>,
    /// Host-owned V1 project resource resolver for explicit media tool calls, inherited by child executions.
    /// 显式媒体工具调用使用的宿主V1项目资源解析器，由子执行继承。
    pub media_resource_runtime:
        Option<Arc<dyn crate::media_resource_runtime::MediaResourceRuntime>>,
    /// Optional project-authorized knowledge-library access snapshot for native tools.
    /// 原生工具使用的可选项目授权知识库访问快照。
    pub knowledge_access: Option<ExecutionKnowledgeAccessContext>,
    /// Optional session-goal binding captured atomically for this request.
    /// 为本次请求原子捕获的可选会话目标绑定。
    pub session_goal_context: Option<ExecutionSessionGoalContext>,
    /// Optional Plan-mode developer guidance injected for the current session activation.
    /// 为当前会话计划模式激活注入的可选开发者引导。
    pub session_plan_context: Option<String>,
    /// Usage segments already consumed by internal work before this request continues.
    /// 本次请求继续执行前已经由内部工作消耗的用量分段。
    pub embedded_turn_usage_segments: Vec<SessionTurnUsageSegment>,
}

impl ExecutionPreparationRequest {
    /// Adopts the next Provider-request identity and retains only an unconsumed Continue action.
    /// 采用下一 Provider 请求身份，并且仅保留尚未消费的 Continue 动作。
    ///
    /// # Parameters
    /// 参数
    /// `continuation` is captured from the active telemetry state at a replay-safe stream boundary.
    /// `continuation` 是在可安全回放的流边界从活动遥测状态捕获的身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; the request adopts the exact Provider continuation identity.
    /// 不返回值；该请求采用精确的 Provider 延续身份。
    pub fn replace_retry_replay_with_provider_request_continuation(
        &mut self,
        continuation: ExecutionProviderRequestContinuation,
    ) {
        // Only telemetry can distinguish an unconsumed action from one already bound to a durable API.
        // 只有遥测状态机能区分尚未消费的动作与已经绑定可靠 API 的动作。
        self.continuation_action_id = continuation.pending_continuation_action_id.clone();
        self.provider_request_continuation = Some(continuation);
    }
}

/// Explicit controller turn-start mode requested by the host for one execution.
/// 宿主为单次执行请求指定的显式控制器回合启动模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionTurnStartMode {
    /// Append or reuse the current request as a user-authored prompt.
    /// 追加或复用当前请求作为用户编写的提示词。
    UserPrompt,
    /// Start from the canonical provider-visible snapshot without appending a new prompt.
    /// 从规范 Provider 可见快照直接启动，不追加新的提示词。
    RestoredCheckpoint,
}

/// Runtime purpose attached to one provider-facing prompt execution.
/// 附着在一次面向 Provider 的提示词执行上的运行时用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPromptPurpose {
    /// Normal user-authored conversation turn.
    /// 普通用户发起的会话回合。
    UserTurn,
    /// Agent-authored business work already persisted in canonical history.
    /// 已持久化到规范历史中的代理发起业务任务。
    AgentTask,
    /// Recovery of retained provider-visible state, optionally followed by one reconciled user-authored message.
    /// 恢复保留的 Provider 可见状态，并可在完成对账后追加一条用户编写消息。
    ExecutionRetry,
    /// Auxiliary collaboration Q&A that cannot complete or resume the original business task.
    /// 不能完成或恢复原业务任务的辅助协作答疑。
    QuestionExecution,
    /// Cache-preserving internal context-compaction summary request.
    /// 保持缓存的内部上下文压缩摘要请求。
    ContextCompactionCachePreserving,
    /// Internal context-compaction summary request.
    /// 内部上下文压缩摘要请求。
    ContextCompaction,
    /// Internal Git commit-message summary request.
    /// 内部 Git 提交信息总结请求。
    GitCommitSummary,
}

impl Default for ExecutionPromptPurpose {
    /// Returns the default prompt purpose used by normal conversation turns.
    /// 返回普通会话回合使用的默认提示词用途。
    fn default() -> Self {
        Self::UserTurn
    }
}

impl ExecutionPromptPurpose {
    /// Returns whether this purpose must hide provider-visible tools.
    /// 返回该用途是否必须隐藏 Provider 可见工具。
    pub const fn disables_provider_tools(self) -> bool {
        matches!(self, Self::ContextCompaction | Self::GitCommitSummary)
    }

    /// Returns whether tool requests must be rejected before host execution.
    /// 返回工具请求是否必须在宿主执行前拒绝。
    pub const fn rejects_tool_requests(self) -> bool {
        matches!(
            self,
            Self::ContextCompaction
                | Self::ContextCompactionCachePreserving
                | Self::GitCommitSummary
        )
    }

    /// Returns whether this purpose may use normal user-turn tool affordances.
    /// 返回该用途是否可以使用普通用户回合的工具能力。
    pub const fn supports_user_turn_tooling(self) -> bool {
        matches!(
            self,
            Self::UserTurn
                | Self::AgentTask
                | Self::ExecutionRetry
                | Self::QuestionExecution
                | Self::ContextCompactionCachePreserving
        )
    }

    /// Returns whether the host may compact this main-conversation execution and resume it.
    /// 返回宿主是否可以压缩该主会话执行并继续执行。
    /// Retries resume the same canonical conversation from a restored checkpoint, so they share the
    /// user-turn compaction path; subagent, auxiliary and internal requests never do.
    /// 重试从恢复检查点继续同一规范会话，因此与用户回合共享压缩路径；子代理、辅助及内部请求从不参与。
    pub const fn supports_host_auto_compaction(self) -> bool {
        matches!(self, Self::UserTurn | Self::ExecutionRetry)
    }

    /// Returns whether this purpose should run completed-turn hooks.
    /// 返回该用途是否应该运行完成回合 hook。
    pub const fn runs_completed_turn_hooks(self) -> bool {
        matches!(self, Self::UserTurn | Self::AgentTask)
    }

    /// Returns a compact diagnostics label for this prompt purpose.
    /// 返回该提示词用途的紧凑诊断标签。
    pub const fn diagnostics_label(self) -> &'static str {
        match self {
            Self::UserTurn => "user-turn",
            Self::AgentTask => "agent-task",
            Self::ExecutionRetry => "execution-retry",
            Self::QuestionExecution => "question-execution",
            Self::ContextCompactionCachePreserving => "context-compaction-cache-preserving",
            Self::ContextCompaction => "context-compaction",
            Self::GitCommitSummary => "git-commit-summary",
        }
    }
}

/// Prepared execution plan rendered by the workbench before real LLM execution begins.
/// 在真实 LLM 执行开始前由工作台渲染的执行准备计划。
#[derive(Debug, Clone)]
pub struct ExecutionPreparation {
    /// Active session identifier.
    /// 当前激活会话标识。
    pub session_id: SessionId,
    /// Immutable conversation turn identifier when this execution owns one user-visible turn.
    /// 当本次执行拥有一个用户可见回合时使用的不可变会话回合标识。
    pub conversation_hook_turn_id: Option<String>,
    /// Optional durable client action that owns a V1 Continue execution.
    /// 可选的可靠客户端动作，用于标识一项 V1 Continue 执行。
    pub continuation_action_id: Option<ClientActionId>,
    /// Active session title.
    /// 当前激活会话标题。
    pub session_title: String,
    /// Active agent identifier.
    /// 当前激活代理标识。
    pub agent_id: String,
    /// Active agent display name.
    /// 当前激活代理显示名称。
    pub agent_name: String,
    /// Active agent kind label.
    /// 当前激活代理类型标签。
    pub agent_kind: String,
    /// Optional request-scoped fixed agent prompt snapshot.
    /// 可选的请求级固定代理提示词快照。
    pub agent_prompt_snapshot: Option<ExecutionAgentPromptSnapshot>,
    /// Dedicated-session request shaping, when this preparation serves a dedicated session.
    /// 本次准备服务于专职会话时的请求组成配置。
    pub dedicated_session: Option<DedicatedExecutionProfile>,
    /// Optional child-agent prompt context routed through the shared prompt composer.
    /// 通过共享提示词组合器路由的可选子智能体提示词上下文。
    pub subagent_prompt_context: Option<ExecutionSubagentPromptContext>,
    /// Validated provider/model runtime blueprint.
    /// 通过校验的 Provider/Model 运行时蓝图。
    pub model_blueprint: ProviderRuntimeBlueprint,
    /// Prompt preview shortened for activity logs.
    /// 为活动日志截断后的提示词预览。
    pub prompt_preview: String,
    /// Workspace path captured for the run.
    /// 为本次运行捕获的工作区路径。
    pub workspace_path: String,
    /// Exact storage directory for session-local runtime artifacts emitted by this run.
    /// 本次运行发出的会话本地运行时工件所使用的精确存储目录。
    pub session_storage_directory: Option<PathBuf>,
    /// Latest host-visible attachment catalog captured from the canonical conversation window for this run.
    /// 为本次运行从规范会话窗口捕获的最新宿主可见附件目录。
    pub conversation_attachments: Vec<ConversationAttachment>,
    /// Number of history rows that will be forwarded into the real model call.
    /// 将被转发到真实模型调用中的历史行数量。
    pub context_history_count: usize,
    /// Execution-mode label captured for the run.
    /// 为本次运行捕获的执行模式标签。
    pub execution_mode: String,
    /// Optional sampling temperature captured for the run.
    /// 为本次运行捕获的可选采样温度。
    pub temperature: Option<f32>,
    /// Session request options captured for the run.
    /// 为本次运行捕获的会话请求选项。
    pub session_request_options: vulcan_session_core::v1::config::RequestOptionsConfig,
    /// Prompt-composer configuration captured for this run.
    /// 为本次运行捕获的提示词组合配置。
    pub prompt_configuration: PersistedPromptConfiguration,
    /// Output-style snapshot captured for this prepared prompt boundary.
    /// 为本次已准备提示词边界捕获的输出风格快照。
    pub output_style_snapshot: Option<PromptOutputStyleSnapshot>,
    /// Prompt purpose captured for this run.
    /// 为本次运行捕获的提示词用途。
    pub prompt_purpose: ExecutionPromptPurpose,
    /// Prompt skills resolved for this run after global/project/session overrides.
    /// 为本次运行按全局、项目与会话覆盖解析出的提示词技能集合。
    pub prompt_skills: Vec<ResolvedPromptSkill>,
    /// User prompt-skill root captured for this run.
    /// 为本次运行捕获的用户提示词技能根目录。
    pub prompt_skill_user_root: Option<PathBuf>,
    /// Producer-typed developer prompt segments frozen for this prepared request.
    /// 为当前已准备请求冻结、并由生产者强类型标注的开发者提示词片段。
    pub additional_developer_segments: Vec<AdditionalPromptSegment>,
    /// Resolved skill names attached to the active agent.
    /// 附着在当前代理上的技能名称集合。
    pub skill_names: Vec<String>,
    /// Unified AI-visible tool descriptors resolved for the active mode.
    /// 为当前模式解析出的统一 AI 可见工具描述集合。
    pub tool_descriptors: Vec<VulcanToolDescriptor>,
    /// Optional reason used to reject visible tool calls before host execution.
    /// 在宿主执行前拒绝可见工具调用时使用的可选原因。
    pub tool_request_rejection_reason: Option<String>,
    /// Whether the rejection reason applies to every visible tool.
    /// 拒绝原因是否应用到全部可见工具。
    pub tool_request_rejection_all_tools: bool,
    /// Optional scoped tool ids rejected with `tool_request_rejection_reason`.
    /// 使用 `tool_request_rejection_reason` 拒绝的可选限定工具标识。
    pub tool_request_rejection_tool_ids: Vec<String>,
    /// Delegate agent identifiers exposed to the active agent.
    /// 暴露给当前代理的委派代理标识集合。
    pub delegate_agents: Vec<String>,
    /// Enabled MCP server summaries visible to the runtime foundation.
    /// 对运行时基础架构可见的已启用 MCP 服务器摘要集合。
    pub enabled_mcp_servers: Vec<String>,
    /// Non-fatal preparation warnings.
    /// 非致命的准备阶段警告集合。
    pub warnings: Vec<String>,
    /// Full provider-request preview captured right before the real API call starts.
    /// 在真实 API 调用开始前捕获的完整 Provider 请求预览。
    pub provider_request_preview_markdown: String,
    /// Structured provider-request snapshot JSON captured by `vulcan-core` right before dispatch.
    /// 由 `vulcan-core` 在分发前捕获的结构化 Provider 请求快照 JSON。
    pub provider_request_snapshot_json: Option<Value>,
    /// Optional context-budget snapshot captured from the final provider-facing request shape.
    /// 从最终面向 Provider 的请求形态捕获到的可选上下文预算快照。
    pub context_budget: Option<ContextBudgetSnapshot>,
    /// Context-compaction policy used when building request budgets and budget guards.
    /// 构建请求预算与预算守卫时使用的上下文压缩策略。
    pub context_compaction_policy: Option<ContextCompactionPolicy>,
    /// Request-level input-token budget limit used when building request budgets and budget guards.
    /// 构建请求预算与预算守卫时使用的请求级输入 Token 预算上限。
    pub context_budget_override: Option<usize>,
    /// Optional layered context assembly captured from the final provider-facing request shape.
    /// 从最终面向 Provider 的请求形态捕获到的可选分层上下文装配结果。
    pub context_assembly: Option<ContextAssembly>,
    /// Optional compact baseline snapshot derived from the current layered context assembly.
    /// 从当前分层上下文装配结果派生出的可选紧凑基线快照。
    pub context_baseline: Option<ContextBaseline>,
    /// Optional context request send-mode diagnostic captured from the final request shape.
    /// 从最终请求形态捕获的可选上下文请求发送模式诊断。
    pub context_request_mode: Option<ContextRequestMode>,
    /// Provider-facing context-injection overlay captured for the current run.
    /// 为当前运行捕获的 Provider 可见上下文注入覆盖层。
    pub context_injection: Option<CoreProviderContextInjection>,
    /// Request-scoped system-prompt hook blocks captured for the current run.
    /// 为当前运行捕获的请求级系统提示词 hook 块。
    pub system_prompt_hook_blocks: Vec<SystemPromptHookBlock>,
    /// Effective project-level VMM configuration captured for this run.
    /// 为本次运行捕获的有效项目级 VMM 配置。
    pub project_vmm: VmmConfigLayer,
    /// Effective session-level VMM configuration captured for this run.
    /// 为本次运行捕获的有效会话级 VMM 配置。
    pub session_vmm: VmmConfigLayer,
    /// Whether VMM was fully configured and runtime-ready when this execution was admitted.
    /// 本次执行准入时 VMM 是否已完成配置并处于运行就绪状态。
    pub vmm_runtime_ready: bool,
    /// session-boundary boundary events accepted before this prepared execution starts.
    /// 本次已准备执行开始前已经接受的 会话边界边界事件。
    pub boundary_events: Vec<SessionBoundaryEvent>,
    /// Chronological runtime system updates loaded from `runtime.jsonl` for provider-visible lowering.
    /// 从 `runtime.jsonl` 加载并用于 Provider 可见转换的时间顺序 runtime system update。
    pub runtime_system_updates: Vec<RuntimeSystemUpdateEvent>,
    /// Optional session-goal binding captured atomically for this execution.
    /// 为本次执行原子捕获的可选会话目标绑定。
    pub session_goal_context: Option<ExecutionSessionGoalContext>,
    /// Optional Plan-mode developer guidance injected for the current session activation.
    /// 为当前会话计划模式激活注入的可选开发者引导。
    pub session_plan_context: Option<String>,
    /// Usage segments already consumed by internal work before this prepared execution continues.
    /// 本次已准备执行继续前已经由内部工作消耗的用量分段。
    pub embedded_turn_usage_segments: Vec<SessionTurnUsageSegment>,
}

/// Prompt-composer input describing one child-agent execution.
/// 描述一次子智能体执行的提示词组合输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSubagentPromptContext {
    /// Stable child-agent identity shown to the model.
    /// 展示给模型的稳定子智能体身份。
    pub identity: String,
    /// Delegated task text attached to this child-agent turn.
    /// 附着到本次子智能体回合的委托任务文本。
    pub delegated_task: String,
    /// Optional parent delegation guidance.
    /// 可选的父级委托说明。
    pub delegation: Option<String>,
    /// Optional functional prompt supplement for custom or profiled agents.
    /// 自定义或画像智能体使用的可选功能提示词补充。
    pub functional_system_prompt: Option<String>,
}

impl ExecutionSubagentPromptContext {
    /// Returns whether this context should use the functional subagent prompt strategy.
    /// 返回该上下文是否应使用功能型子智能体提示词策略。
    pub fn uses_functional_prompt(&self) -> bool {
        self.functional_system_prompt
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
    }
}

/// Convert a persisted personality selection into prompt-composer input.
/// 将持久化人格选择转换为提示词组合器输入。
pub(crate) fn persisted_personality_to_prompt_selection(
    selection: &PersistedPromptPersonalitySelection,
) -> vulcan_prompt_composer::PersonalitySelection {
    match selection {
        PersistedPromptPersonalitySelection::Default => {
            vulcan_prompt_composer::PersonalitySelection::Default
        }
        PersistedPromptPersonalitySelection::Named(id) => {
            vulcan_prompt_composer::PersonalitySelection::Named(id.clone())
        }
        PersistedPromptPersonalitySelection::Disabled => {
            vulcan_prompt_composer::PersonalitySelection::Disabled
        }
    }
}

impl ExecutionPreparation {
    /// Returns a compact list of human-readable summary rows for UI cards and logs.
    /// 返回供界面卡片与日志使用的紧凑人类可读摘要行列表。
    pub fn summary_rows(&self) -> Vec<String> {
        let mut rows = vec![
            format!("Session: #{} · {}", self.session_id, self.session_title),
            format!(
                "Agent: {} ({}, id={})",
                self.agent_name, self.agent_kind, self.agent_id
            ),
            format!(
                "Model: {} / {} · runtime {}",
                self.model_blueprint.provider_name,
                self.model_blueprint.model_name,
                self.model_blueprint.runtime_label
            ),
            format!(
                "Route: {}/{} · kind={}",
                self.model_blueprint.provider_id,
                self.model_blueprint.model_id,
                self.model_blueprint.runtime_kind_key()
            ),
            format!(
                "API family: {} · family={}",
                self.model_blueprint.protocol_profile_label(),
                self.model_blueprint.protocol_profile_key()
            ),
            format!("Endpoint: {}", self.model_blueprint.endpoint_summary()),
            format!("Credential: {}", self.model_blueprint.credential_source),
            format!(
                "Lifecycle: {}",
                self.model_blueprint.public_capabilities.evidence_revision
            ),
            format!(
                "Model discovery: {}",
                if self.model_blueprint.supports_remote_model_refresh {
                    "supports remote refresh"
                } else {
                    "uses the static catalog"
                }
            ),
            format!(
                "Mode: {} · temperature {}",
                self.execution_mode,
                self.temperature
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| String::from("model default"))
            ),
            self.model_blueprint.reasoning.summary_label(),
            format!("Workspace: {}", self.workspace_path),
            format!("Context: {} history messages", self.context_history_count),
            format!(
                "Capabilities: temperature override {}",
                if self.model_blueprint.supports_temperature() {
                    "supported"
                } else {
                    "not supported"
                }
            ),
            format!(
                "Modalities: {} · attachments declared {}",
                if self
                    .model_blueprint
                    .public_capabilities
                    .input_resources
                    .is_empty()
                {
                    String::from("text")
                } else {
                    self.model_blueprint
                        .public_capabilities
                        .input_resources
                        .iter()
                        .map(|kind| format!("{kind:?}").to_ascii_lowercase())
                        .collect::<Vec<_>>()
                        .join("/")
                },
                if !self
                    .model_blueprint
                    .public_capabilities
                    .input_resources
                    .is_empty()
                {
                    "declared"
                } else {
                    "not declared"
                }
            ),
        ];
        if let Some(context_budget) = self.context_budget.as_ref() {
            rows.push(format!("Budget: {}", context_budget.summary_note()));
            rows.push(format!(
                "Budget fingerprint: {}",
                context_budget.fingerprint
            ));
        }

        if let Some(context_assembly) = self.context_assembly.as_ref() {
            rows.push(format!(
                "Context layers: {}",
                context_assembly.summary_note()
            ));
        }

        if let Some(context_baseline) = self.context_baseline.as_ref() {
            rows.push(format!(
                "Context baseline: {} layers · {}",
                context_baseline.layer_count, context_baseline.fingerprint
            ));
        }

        if let Some(context_request_mode) = self.context_request_mode.as_ref() {
            rows.push(format!(
                "Context sending: {}",
                context_request_mode.summary_note()
            ));
        }

        if !self.skill_names.is_empty() {
            rows.push(format!("Skills: {}", self.skill_names.join(", ")));
        }

        let enabled_prompt_skills = self
            .prompt_skills
            .iter()
            .filter(|skill| skill.is_enabled())
            .map(|skill| skill.skill.name.as_str())
            .collect::<Vec<_>>();
        if !enabled_prompt_skills.is_empty() {
            rows.push(format!(
                "Prompt skills: {}",
                enabled_prompt_skills.join(", ")
            ));
        }

        if !self.tool_descriptors.is_empty() {
            rows.push(format!(
                "Tools: {} exposed directly",
                self.tool_descriptors.len()
            ));
        }

        let native_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::Native)
            .count();
        if native_tool_count > 0 {
            rows.push(format!("Native tools: {}", native_tool_count));
        }

        let vulcan_managed_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::VulcanManaged)
            .count();
        if vulcan_managed_tool_count > 0 {
            rows.push(format!(
                "Vulcan managed tools: {}",
                vulcan_managed_tool_count
            ));
        }

        let vulcan_plugin_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::VulcanPlugin)
            .count();
        if vulcan_plugin_tool_count > 0 {
            rows.push(format!("Vulcan plugin tools: {}", vulcan_plugin_tool_count));
        }

        let luaskills_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::LuaSkills)
            .count();
        if luaskills_tool_count > 0 {
            rows.push(format!("LuaSkills tools: {}", luaskills_tool_count));
        }

        let mcp_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::Mcp)
            .count();
        if mcp_tool_count > 0 {
            rows.push(format!("MCP tools: {}", mcp_tool_count));
        }

        let ai_host_binding_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::AiHostBinding)
            .count();
        if ai_host_binding_tool_count > 0 {
            rows.push(format!(
                "AI-only fallback tools: {}",
                ai_host_binding_tool_count
            ));
        }

        if !self.delegate_agents.is_empty() {
            rows.push(format!("Delegation: {}", self.delegate_agents.join(", ")));
        }

        if !self.enabled_mcp_servers.is_empty() {
            rows.push(format!("MCP: {}", self.enabled_mcp_servers.join("; ")));
        }

        rows.push(format!("Prompt: {}", self.prompt_preview));
        rows
    }
}

/// Real execution report returned by the async provider runtime after one run attempt.
/// 异步 Provider 运行时在单次执行后返回的真实执行报告。
#[derive(Debug, Clone)]
pub struct ExecutionRunReport {
    /// Session identifier owning the run.
    /// 拥有本次运行的会话标识。
    pub session_id: SessionId,
    /// Prepared execution snapshot when preparation succeeded.
    /// 当准备阶段成功时生成的执行准备快照。
    pub preparation: Option<ExecutionPreparation>,
    /// Assistant output returned by the real provider call.
    /// 真实 Provider 调用返回的助手输出。
    pub assistant_output: Option<String>,
    /// Assistant thinking output returned by the real provider call.
    /// 真实 Provider 调用返回的助手思考输出。
    pub thinking_output: Option<String>,
    /// Canonical conversation state returned from the last safe host-controlled controller boundary.
    /// 从最后一个安全的宿主控制器边界返回的规范会话状态。
    pub conversation_state: Option<SessionConversationState>,
    /// Number of real provider requests dispatched while this run was active.
    /// 本次运行期间真实发出的 Provider 请求次数。
    pub provider_request_count: usize,
    /// Latest successful provider usage captured by this run, excluding failed or aborted calls.
    /// 本次运行捕获到的最近一次成功 Provider 用量，不包含失败或中止调用。
    pub last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    /// Cumulative successful provider usage captured by this run, excluding failed or aborted calls.
    /// 本次运行捕获到的成功 Provider 累计用量，不包含失败或中止调用。
    pub cumulative_successful_provider_usage: Option<ProviderUsageSnapshot>,
    /// Successful MCP discovery summary rows collected during the run.
    /// 运行期间收集到的 MCP 成功发现摘要行集合。
    pub mcp_discovery_rows: Vec<String>,
    /// Failed MCP discovery summary rows collected during the run.
    /// 运行期间收集到的 MCP 失败发现摘要行集合。
    pub mcp_discovery_errors: Vec<String>,
    /// Non-fatal warnings collected during the run.
    /// 运行期间收集到的非致命警告集合。
    pub warnings: Vec<String>,
    /// Terminal error when the run failed.
    /// 当运行失败时的最终错误信息。
    pub error: Option<String>,
    /// Structured category of the Provider request whose non-retried failure ended the run.
    /// 以不再重试的失败结束本次运行的 Provider 请求的结构化类别。
    ///
    /// `None` means the run succeeded or ended for a reason other than a classified Provider
    /// failure; consumers decide on this category instead of the display text in `error`.
    /// `None` 表示运行成功，或因已分类 Provider 失败以外的原因结束；调用方依据此类别判定，
    /// 而不是解析 `error` 中的展示文本。
    pub provider_failure_category: Option<AiErrorCategory>,
    /// Controller-owned event trace captured while the run was executing.
    /// 运行执行期间捕获的控制器自有事件轨迹。
    pub controller_events: Vec<CoreControllerEvent>,
    /// Usage segments embedded before the visible assistant response of this run.
    /// 本次运行可见助手响应之前嵌入的用量分段。
    pub embedded_turn_usage_segments: Vec<SessionTurnUsageSegment>,
}

impl ExecutionRunReport {
    /// Creates one success report.
    /// 创建一份成功执行报告。
    pub(crate) fn success(
        session_id: SessionId,
        preparation: ExecutionPreparation,
        thinking_output: Option<String>,
        assistant_output: Option<String>,
        conversation_state: Option<SessionConversationState>,
        provider_request_count: usize,
        mcp_discovery_rows: Vec<String>,
        mcp_discovery_errors: Vec<String>,
        warnings: Vec<String>,
        controller_events: Vec<CoreControllerEvent>,
        last_successful_provider_usage: Option<ProviderUsageSnapshot>,
        cumulative_successful_provider_usage: Option<ProviderUsageSnapshot>,
    ) -> Self {
        let embedded_turn_usage_segments = preparation.embedded_turn_usage_segments.clone();
        Self {
            session_id,
            preparation: Some(preparation),
            assistant_output,
            thinking_output,
            conversation_state,
            provider_request_count,
            last_successful_provider_usage,
            cumulative_successful_provider_usage,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            error: None,
            provider_failure_category: None,
            controller_events,
            embedded_turn_usage_segments,
        }
    }

    /// Creates one failure report.
    /// 创建一份失败执行报告。
    pub(crate) fn failure(
        session_id: SessionId,
        preparation: Option<ExecutionPreparation>,
        provider_request_count: usize,
        last_successful_provider_usage: Option<ProviderUsageSnapshot>,
        cumulative_successful_provider_usage: Option<ProviderUsageSnapshot>,
        mcp_discovery_rows: Vec<String>,
        mcp_discovery_errors: Vec<String>,
        warnings: Vec<String>,
        controller_events: Vec<CoreControllerEvent>,
        conversation_state: Option<SessionConversationState>,
        error: impl Into<String>,
    ) -> Self {
        let embedded_turn_usage_segments = preparation
            .as_ref()
            .map(|preparation| preparation.embedded_turn_usage_segments.clone())
            .unwrap_or_default();
        Self {
            session_id,
            preparation,
            assistant_output: None,
            thinking_output: None,
            conversation_state,
            provider_request_count,
            last_successful_provider_usage,
            cumulative_successful_provider_usage,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            error: Some(error.into()),
            provider_failure_category: None,
            controller_events,
            embedded_turn_usage_segments,
        }
    }

    /// Records the structured category of the Provider failure that ended this run.
    /// 记录结束本次运行的 Provider 失败的结构化类别。
    pub(crate) fn with_provider_failure_category(
        mut self,
        category: Option<AiErrorCategory>,
    ) -> Self {
        self.provider_failure_category = category;
        self
    }

    /// Returns whether the current report completed successfully.
    /// 返回当前报告是否成功完成。
    pub const fn is_success(&self) -> bool {
        self.error.is_none()
    }
}
