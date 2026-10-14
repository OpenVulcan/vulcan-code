//! V1 main-event envelope and event kinds.
//! V1 主事件 envelope 与事件种类。
//!
//! Every main-session fact is one strongly typed JSONL line with a contiguous
//! `seq`. The envelope carries the stream position, optional owning turn,
//! immutable creation time, mutable update time, and a typed projection
//! strategy. Payloads never use free-form JSON as the event structure itself.
//! 主会话的每个事实都是携带连续 `seq` 的一条强类型 JSONL 行。envelope 保存
//! 流位置、可选的所属回合、不可变的创建时间、可变的更新时间与强类型投影
//! 策略。载荷绝不用自由 JSON 充当事件结构本身。

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use vulcan_ai_contract::ProviderReplayEnvelope;

use crate::v1::budget::ProviderRequestBudgetEvidence;
use crate::v1::collaboration::CollaborationEvent;
use crate::v1::config::ActiveRequestConfig;
use crate::v1::errors::DomainError;
use crate::v1::ids::{
    ApiCallId, ClientActionId, PendingInputId, RequestStreamId, ResourceId, Seq, SubagentTaskId,
    ToolCallId, TurnId,
};
use crate::v1::ids::{JS_SAFE_INTEGER_MAX, LOCAL_ID_MIN};
use crate::v1::stats::{TurnStatBuckets, UsageSnapshot};
use vulcan_change_set::PersistentChangeSet;

/// Initial revision assigned to every newly appended event.
/// 每条新追加事件获得的初始修订号。
pub const EVENT_REVISION_INITIAL: u64 = LOCAL_ID_MIN;

/// One main-event envelope line.
/// 一条主事件 envelope 行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationEvent {
    /// Contiguous stream position, unique across the whole session.
    /// 全会话唯一的连续流位置。
    pub seq: Seq,
    /// Owning logical turn, when the event belongs to one turn.
    /// 所属逻辑回合；事件不属于任何回合时为 None。
    pub turn_id: Option<TurnId>,
    /// Immutable semantic creation time in Unix milliseconds.
    /// 不可变的语义创建时间(Unix 毫秒)。
    pub created_at_ms: i64,
    /// Mutable last-update time in Unix milliseconds.
    /// 可变的最后更新时间(Unix 毫秒)。
    pub updated_at_ms: i64,
    /// Monotonic semantic revision used to order same-seq in-place evolution.
    /// 用于排序同 seq 原位语义演化的单调修订号。
    pub event_revision: u64,
    /// Typed projection strategy; never inferred from content.
    /// 强类型投影策略；绝不从内容推断。
    pub visibility: EventVisibility,
    /// Strongly typed event payload.
    /// 强类型事件载荷。
    pub event: EventBody,
}

impl ConversationEvent {
    /// Returns the stream position.
    /// 返回流位置。
    pub fn seq(&self) -> Seq {
        self.seq
    }

    /// Returns whether this event is visible to the Provider projection.
    /// 返回该事件是否对 Provider 投影可见。
    pub fn is_provider_visible(&self) -> bool {
        self.visibility == EventVisibility::Provider
    }

    /// Applies one semantic in-place mutation and advances the event revision exactly once.
    /// 应用一次语义原位变更，并且只递增一次事件修订号。
    pub(crate) fn mutate_in_place(
        &mut self,
        updated_at_ms: i64,
        mutation: impl FnOnce(&mut EventBody),
    ) -> Result<(), DomainError> {
        if self.event_revision >= JS_SAFE_INTEGER_MAX {
            return Err(DomainError::invalid_id(
                "event_revision",
                JS_SAFE_INTEGER_MAX + 1,
            ));
        }
        mutation(&mut self.event);
        self.event_revision += 1;
        self.updated_at_ms = updated_at_ms;
        Ok(())
    }

    /// Returns the greatest session-local entity id referenced by this event,
    /// or zero when the event carries no local entity id.
    /// 返回该事件引用的最大会话本地实体 id；事件不携带本地实体 id 时返回零。
    pub fn max_referenced_local_id(&self) -> u64 {
        let turn_id = self.turn_id.map_or(0, TurnId::value);
        let body_id = match &self.event {
            EventBody::UserInput(input) => input.input_id.value(),
            EventBody::QueuedInputResolution(resolution) => resolution.input_id.value(),
            EventBody::ApiStarted(started) => started.api_call_id.value(),
            EventBody::AssistantReasoning(reasoning) => reasoning.api_call_id.value(),
            EventBody::AssistantText(text) => text.api_call_id.value(),
            EventBody::ToolCall(tool) => tool.api_call_id.value().max(tool.tool_call_id.value()),
            EventBody::ApiStats(stats) => stats.api_call_id.value(),
            EventBody::DisplayNote(note) => note.api_call_id.map_or(0, ApiCallId::value),
            EventBody::RequestConfigChanged(_) => 0,
            EventBody::ExecutionControl(control) => control.api_call_id.map_or(0, ApiCallId::value),
            EventBody::Collaboration(_) => 0,
            EventBody::ContextMaintenance(execution) => execution
                .turn_id
                .value()
                .max(execution.business_turn_id.value()),
            EventBody::SystemContext(_)
            | EventBody::TurnBoundary(_)
            | EventBody::TurnStats(_)
            | EventBody::SubagentReturn(_) => 0,
        };
        turn_id.max(body_id)
    }
}

/// Derives the next event-referenced local id from one ordered event slice.
/// 从一段有序事件切片推导下一个事件引用本地 id。
pub fn next_event_local_id(events: &[ConversationEvent]) -> u64 {
    events
        .iter()
        .map(ConversationEvent::max_referenced_local_id)
        .max()
        .unwrap_or(0)
        + 1
}

/// Typed projection strategy of one event.
/// 一条事件的强类型投影策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventVisibility {
    /// Included in Provider request construction and the UI timeline.
    /// 进入 Provider 请求构造与 UI 时间线。
    Provider,
    /// Only the UI timeline; never projected to the Provider.
    /// 仅 UI 时间线；绝不投影给 Provider。
    UiOnly,
    /// Host-internal control facts; invisible to the Provider and filtered
    /// from the neutral UI story.
    /// 宿主内部控制事实；对 Provider 不可见并从 UI 叙事中过滤。
    HostOnly,
}

/// Strongly typed event body tagged by its stable kind.
/// 按稳定 kind 打标签的强类型事件体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "several variants exceed 700 bytes, so boxing the largest one would not shrink the enum"
)]
pub enum EventBody {
    /// Dynamic system/developer semantic upsert or remove.
    /// 动态系统/developer 语义的 upsert 或 remove。
    SystemContext(SystemContextEvent),
    /// Root user input or in-turn supplementary input.
    /// 根用户输入或回合内补充输入。
    UserInput(UserInputEvent),
    /// Durable terminal resolution of one queued input before Provider scheduling.
    /// 一条排队输入在 Provider 调度前的持久化终结处置事实。
    QueuedInputResolution(QueuedInputResolutionEvent),
    /// Durable boundary proving that one physical Provider API was prepared.
    /// 证明一次物理 Provider API 已准备完成的持久化边界。
    ApiStarted(ApiStartedEvent),
    /// Displayable reasoning text of one API.
    /// 一次 API 的可显示推理正文。
    AssistantReasoning(AssistantReasoningEvent),
    /// Assistant text of one API.
    /// 一次 API 的 assistant 正文。
    AssistantText(AssistantTextEvent),
    /// One complete tool invocation (input, output, changeset, status).
    /// 一次完整工具调用(input、output、changeset、状态)。
    ToolCall(ToolCallEvent),
    /// One physical Provider API statistic record.
    /// 一次物理 Provider API 统计记录。
    ApiStats(ApiStatsEvent),
    /// Turn closure fact: completed, abandoned, or stopped.
    /// 回合闭合事实：完成、放弃或停止。
    TurnBoundary(TurnBoundaryEvent),
    /// The single terminal statistic of one truly closed turn.
    /// 一个真正闭合回合的唯一终态统计。
    TurnStats(TurnStatsEvent),
    /// Content actually returned to the parent model with its source range.
    /// 实际回流给父模型的内容及其来源范围。
    SubagentReturn(SubagentReturnEvent),
    /// Permanent UI timeline fact: errors, interruptions, retries, resume.
    /// 永久 UI 时间线事实：错误、中断、重试、恢复等。
    DisplayNote(DisplayNoteEvent),
    /// Exact non-secret request transition at one admitted user or Continue boundary.
    /// 一次已准入用户输入或 Continue 边界上的精确非秘密请求配置转换。
    RequestConfigChanged(RequestConfigChangedEvent),
    /// Provider-invisible execution control fact.
    /// Provider 不可见的执行控制事实。
    ExecutionControl(ExecutionControlEvent),
    /// Host-only typed collaboration mutation and recovery fact.
    /// 仅宿主可见的强类型协作变更与恢复事实。
    Collaboration(CollaborationEvent),
    /// Host-only start or terminal after-state of an independent maintenance execution.
    /// 独立维护执行的仅宿主起始或终态后状态。
    ContextMaintenance(super::context_maintenance::ContextMaintenanceExecution),
}

impl EventBody {
    /// Returns the stable enum variant name for diagnostics.
    /// 返回用于诊断的稳定枚举变体名。
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::SystemContext(_) => "system_context",
            Self::UserInput(_) => "user_input",
            Self::QueuedInputResolution(_) => "queued_input_resolution",
            Self::ApiStarted(_) => "api_started",
            Self::AssistantReasoning(_) => "assistant_reasoning",
            Self::AssistantText(_) => "assistant_text",
            Self::ToolCall(_) => "tool_call",
            Self::ApiStats(_) => "api_stats",
            Self::TurnBoundary(_) => "turn_boundary",
            Self::TurnStats(_) => "turn_stats",
            Self::SubagentReturn(_) => "subagent_return",
            Self::DisplayNote(_) => "display_note",
            Self::RequestConfigChanged(_) => "request_config_changed",
            Self::ExecutionControl(_) => "execution_control",
            Self::Collaboration(_) => "collaboration",
            Self::ContextMaintenance(_) => "context_maintenance",
        }
    }
}

/// Immutable resolved request configuration of one physical Provider API.
/// 一次物理 Provider API 的不可变已解析请求配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiRequestSnapshot {
    /// Provider name actually selected for dispatch.
    /// 实际选定用于发送的 Provider 名。
    pub provider: String,
    /// Optional protocol profile selected for dispatch.
    /// 实际选定用于发送的可选协议 Profile。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Model actually selected for dispatch.
    /// 实际选定用于发送的模型。
    pub model: String,
    /// Optional non-secret connection id.
    /// 可选非秘密连接 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Optional thinking level actually selected.
    /// 实际选定的可选思考级别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    /// Optional temperature actually selected.
    /// 实际选定的可选温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

impl ApiRequestSnapshot {
    /// Builds a resolved request snapshot with required provider/model fields
    /// and no optional overrides.
    /// 使用必需的 Provider/模型字段且不带可选覆盖构建已解析请求快照。
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            profile: None,
            model: model.into(),
            connection_id: None,
            thinking_level: None,
            temperature: None,
        }
    }
}

/// Immutable proof of the exact V1 projection used by one physical Provider request.
/// 一次物理 Provider 请求实际使用的精确 V1 投影不可变证明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderProjectionProof {
    /// Storage generation from which the exact event range was loaded.
    /// 精确事件区间读取自的存储 generation。
    pub segment_generation: u64,
    /// Exact document revision from which the immutable request projection was derived.
    /// 不可变请求投影派生自的精确文档修订号。
    pub document_revision: u64,
    /// Inclusive first main-event seq retained by the dispatched projection.
    /// 已派发投影包含的首个主事件 seq。
    pub retained_from_seq: Seq,
    /// Inclusive last main-event seq covered by the dispatched projection.
    /// 已派发投影覆盖的最后一个主事件 seq。
    pub through_seq: Seq,
    /// SHA-256 hex digest computed from the exact immutable dispatch projection.
    /// 从精确不可变派发投影计算得到的 SHA-256 十六进制摘要。
    pub fingerprint_sha256: String,
}

/// Durable start fact of one physical Provider API group.
/// 一次物理 Provider API 组的持久化起始事实。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiStartedEvent {
    /// Local API call id allocated before dispatch.
    /// 发送前分配的本地 API 调用 id。
    pub api_call_id: ApiCallId,
    /// Execution purpose of the call.
    /// 调用的执行用途。
    pub purpose: ApiPurposeKind,
    /// Resolved request configuration frozen for this physical call.
    /// 为本次物理调用冻结的已解析请求配置。
    pub request: ApiRequestSnapshot,
    /// Exact projection proof frozen in the same transaction before dispatch.
    /// 派发前在同一事务中冻结的精确投影证明。
    pub projection: ProviderProjectionProof,
    /// Non-sensitive request-shape evidence frozen from the exact permitted request.
    /// 从精确获准请求冻结的非敏感请求形状证据。
    pub request_budget_evidence: ProviderRequestBudgetEvidence,
    /// Continue action bound to this physical API, absent for ordinary requests.
    /// 绑定到本次物理 API 的继续动作；普通请求不存在该字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_action_id: Option<ClientActionId>,
}

/// Operation of one dynamic system-context mutation.
/// 一次动态系统上下文修改的操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemContextOperation {
    /// Upsert the semantic key with the given content.
    /// 以给定内容 upsert 该语义键。
    Upsert,
    /// Remove the semantic key.
    /// 移除该语义键。
    Remove,
}

/// Scope of one dynamic system-context entry.
/// 一条动态系统上下文条目的作用域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemContextScope {
    /// Workspace/global scope.
    /// 工作区/全局作用域。
    Global,
    /// Project scope.
    /// 项目作用域。
    Project,
    /// Session scope.
    /// 会话作用域。
    Session,
    /// Single-turn scope.
    /// 单回合作用域。
    Turn,
}

/// Provider-side role projection hint for one system semantic.
/// 一条系统语义的 Provider 侧角色投影提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemRoleHint {
    /// Map to the developer layer.
    /// 映射到 developer 层。
    Developer,
    /// Map to a system layer.
    /// 映射到 system 层。
    System,
    /// Wrap as a hidden user prompt only when the protocol lacks system layers.
    /// 仅当协议缺少系统层时套壳为隐藏用户提示。
    HiddenUserShell,
}

/// One dynamic system-context semantic event.
/// 一条动态系统上下文语义事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemContextEvent {
    /// Stable semantic key of the entry.
    /// 条目的稳定语义键。
    pub context_key: String,
    /// Upsert or remove operation.
    /// upsert 或 remove 操作。
    pub operation: SystemContextOperation,
    /// Effective scope of the entry.
    /// 条目的生效作用域。
    pub scope: SystemContextScope,
    /// Optional single-turn TTL.
    /// 可选单回合 TTL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_turns: Option<u64>,
    /// Optional Provider role projection hint.
    /// 可选 Provider 角色投影提示。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_role_hint: Option<SystemRoleHint>,
    /// Semantic content value.
    /// 语义内容值。
    pub content: String,
}

/// Source of one user input.
/// 一条用户输入的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInputSource {
    /// Root prompt that starts a fresh turn.
    /// 开启新回合的根提示词。
    Root,
    /// Supplementary prompt injected while the turn is still open.
    /// 回合仍然开放时注入的过程内补充提示词。
    PendingInTurn,
    /// Queued prompt scheduled after the previous turn closes.
    /// 前一回合关闭后调度入流的排队提示词。
    QueuedScheduled,
    /// Host-authenticated agent communication projected through the protocol User role.
    /// 宿主认证的代理通信，通过协议 User 角色投影。
    AgentMessage,
}

/// One user input event (root, supplementary, or scheduled queued).
/// 一条用户输入事件(根、补充或排队调度)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserInputEvent {
    /// Host-owned parameters for this prompt occurrence; absent legacy data must not be guessed as pending.
    /// 本次提示词出现位置的宿主参数；缺失的旧数据不能被猜测为待准备状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<super::user_prompt::UserPromptParameters>,
    /// Stable external idempotency identity of the client submission.
    /// 客户端提交的稳定外部幂等身份。
    pub client_action_id: ClientActionId,
    /// Stable local input id used for cross-file deduplication.
    /// 用于跨文件去重的稳定本地输入 id。
    pub input_id: PendingInputId,
    /// Where this input came from.
    /// 该输入的来源。
    pub input_source: UserInputSource,
    /// Input text body.
    /// 输入正文。
    pub text: String,
    /// Ordered project resource ids attached to the input.
    /// 附着在该输入上的有序项目资源 id。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_ids: Vec<ResourceId>,
}

/// Durable resolution applied to one queued input before it leaves the future-turn queue.
/// 一条排队输入离开未来回合队列前所应用的持久化处置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuedInputResolutionKind {
    /// The queued input was restored into the durable composer draft.
    /// 排队输入被恢复到持久化输入区草稿。
    MovedToDraft,
    /// The queued input was explicitly removed.
    /// 排队输入被显式移除。
    Removed,
    /// The queued input was atomically guided into the currently open logical Turn.
    /// 排队输入被原子地引导进当前开放逻辑回合。
    PromotedToPendingInTurn,
}

/// Durable receipt fact for one queued-input resolution.
/// 一次排队输入处置的持久化 receipt 事实。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedInputResolutionEvent {
    /// Original admitted prompt parameters retained for exact queue resolution and restored-draft retries.
    /// 为精确队列终结及恢复草稿重试保留的原准入提示词参数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<crate::v1::UserPromptParameters>,
    /// Original external idempotency identity of the queued submission.
    /// 原始排队提交的外部幂等身份。
    pub client_action_id: ClientActionId,
    /// Stable session-local identity of the resolved queued input.
    /// 被处置排队输入的稳定会话本地身份。
    pub input_id: PendingInputId,
    /// Exact terminal resolution selected by the user.
    /// 用户选择的精确终结处置。
    pub resolution: QueuedInputResolutionKind,
    /// Original queued text retained for exact retry/conflict verification.
    /// 为精确重试与冲突校验保留的原始排队正文。
    pub text: String,
    /// Original ordered resources retained for exact retry/conflict verification.
    /// 为精确重试与冲突校验保留的原始有序资源。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_ids: Vec<ResourceId>,
}

/// Progress status of one streaming assistant semantic.
/// 一条流式 assistant 语义的进度状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantSemanticStatus {
    /// Still streaming in memory.
    /// 仍在内存中流式输出。
    Streaming,
    /// Finished normally.
    /// 正常完成。
    Completed,
    /// Persisted streaming state projected to interrupted after a crash.
    /// 崩溃后把已持久化的流式状态投影为 interrupted。
    Interrupted,
}

/// Displayable reasoning semantic of one API.
/// 一次 API 的可显示推理语义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantReasoningEvent {
    /// Owning physical API call.
    /// 所属物理 API 调用。
    pub api_call_id: ApiCallId,
    /// Displayable reasoning body accumulated in memory during streaming.
    /// 流式期间在内存中累积的可显示推理正文。
    pub text: String,
    /// Stream progress status.
    /// 流式进度状态。
    pub status: AssistantSemanticStatus,
    /// Provider source, allowed only on the last Provider-sourced event of a
    /// completed API group (see the ordering rules).
    /// Provider 来源；只允许出现在已完成的 API 组最后一个 Provider 来源
    /// 事件上(见顺序规则)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_source: Option<ProviderSource>,
}

/// Assistant text semantic of one API.
/// 一次 API 的 assistant 正文语义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantTextEvent {
    /// Owning physical API call.
    /// 所属物理 API 调用。
    pub api_call_id: ApiCallId,
    /// Assistant body accumulated in memory during streaming.
    /// 流式期间在内存中累积的 assistant 正文。
    pub text: String,
    /// Stream progress status.
    /// 流式进度状态。
    pub status: AssistantSemanticStatus,
    /// Provider source, allowed only on the last Provider-sourced event of a
    /// completed API group (see the ordering rules).
    /// Provider 来源；只允许出现在已完成的 API 组最后一个 Provider 来源
    /// 事件上(见顺序规则)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_source: Option<ProviderSource>,
    /// File lines quoted by the body's `ui-code` fences, captured once when the body is sealed;
    /// display-only and never sent to a Provider.
    /// 正文 `ui-code` 围栏引用的文件行，在正文封口时抓取一次；仅用于展示，绝不发送给 Provider。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_snapshots: Vec<crate::v1::answer_components::AnswerCodeSnapshot>,
}

/// Lifecycle status of one tool invocation.
/// 一次工具调用的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    /// Tool invocation is durably prepared but its body has not started.
    /// 工具调用已经持久化准备完成，但正文尚未开始执行。
    Prepared,
    /// Tool process started and still running.
    /// 工具进程已启动且仍在运行。
    Running,
    /// Tool is blocked waiting for a user answer.
    /// 工具阻塞等待用户回答。
    WaitingUser,
    /// Tool finished successfully.
    /// 工具成功完成。
    Completed,
    /// Tool finished with an error.
    /// 工具以错误结束。
    Failed,
    /// Tool was cancelled by the user.
    /// 工具被用户取消。
    Cancelled,
    /// Tool was stopped by the host.
    /// 工具被宿主停止。
    Stopped,
    /// A running tool was interrupted by a host stop or crash projection.
    /// 运行中工具被宿主停止或崩溃投影中断。
    Interrupted,
}

/// Stable machine-readable reason for one synthetic terminal tool error output.
/// 一条合成工具终态错误输出的稳定机器可读原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTerminalErrorKind {
    /// Tool execution failed without a host-provided structured output.
    /// 工具执行失败且宿主未提供结构化输出。
    Failed,
    /// Tool execution was cancelled without a host-provided structured output.
    /// 工具执行被取消且宿主未提供结构化输出。
    Cancelled,
    /// Tool execution was stopped by the host without a structured output.
    /// 工具执行被宿主停止且未提供结构化输出。
    Stopped,
    /// Tool execution was interrupted by shutdown or crash recovery.
    /// 工具执行被关闭流程或崩溃恢复中断。
    Interrupted,
}

impl ToolTerminalErrorKind {
    /// Returns the fixed Provider-visible explanation for this terminal reason.
    /// 返回该终态原因固定的 Provider 可见说明。
    fn message(self) -> &'static str {
        match self {
            Self::Failed => "Tool execution failed.",
            Self::Cancelled => "Tool execution was cancelled.",
            Self::Stopped => "Tool execution was stopped by the host.",
            Self::Interrupted => "Tool execution was interrupted before completion.",
        }
    }
}

/// Strongly typed error object embedded in a synthetic terminal tool output.
/// 嵌入合成工具终态输出中的强类型错误对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolTerminalError {
    /// Stable error classification.
    /// 稳定错误分类。
    kind: ToolTerminalErrorKind,
    /// Fixed Provider-visible explanation.
    /// 固定的 Provider 可见说明。
    message: &'static str,
}

/// Strongly typed canonical envelope for a synthetic terminal tool output.
/// 合成工具终态输出的强类型规范信封。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolTerminalFailure {
    /// Always false because the invocation did not complete successfully.
    /// 始终为 false，因为调用没有成功完成。
    ok: bool,
    /// Structured terminal error fact.
    /// 结构化终态错误事实。
    error: ToolTerminalError,
}

/// Kind of one external navigation-only resource reference.
/// 一个仅用于导航的外部资源引用种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalResourceKind {
    /// A persistent terminal instance.
    /// 持久化终端实例。
    Terminal,
    /// A subagent task session.
    /// 子代理任务会话。
    Subagent,
    /// A Lua persistent session.
    /// Lua 持久化会话。
    LuaSession,
}

/// One external resource reference used only for navigation.
/// 一个仅用于导航的外部资源引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalResourceRef {
    /// Kind of the external resource.
    /// 外部资源的种类。
    pub kind: ExternalResourceKind,
    /// Raw external identity string.
    /// 原始外部身份字符串。
    pub id: String,
}

/// Stable display category of one project-owned resource generated by a tool.
/// 工具生成的一项项目级资源的稳定显示类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedResourceKind {
    /// Generated audio resource.
    /// 生成的音频资源。
    Audio,
    /// Generated image resource.
    /// 生成的图片资源。
    Image,
    /// Generated video resource.
    /// 生成的视频资源。
    Video,
    /// Generated PDF resource.
    /// 生成的 PDF 资源。
    Pdf,
    /// Generated generic file resource.
    /// 生成的通用文件资源。
    File,
}

/// Ordered project-resource reference produced by one completed tool call.
/// 一次已完成工具调用产生的有序项目资源引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedResourceRef {
    /// Project-scoped identity of the generated primary resource.
    /// 生成主资源的项目级身份。
    pub resource_id: ResourceId,
    /// Stable display category persisted independently from catalog metadata.
    /// 独立于目录元数据持久化的稳定显示类别。
    pub kind: GeneratedResourceKind,
    /// Optional project-scoped identity of an explicit generated preview.
    /// 显式生成预览的可选项目级身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_resource_id: Option<ResourceId>,
}

/// Canonical, fully-assembled tool input/output payload echoed from the
/// Provider or tool host.
/// 从 Provider 或工具宿主回传的 canonical、完整组装工具输入/输出载荷。
///
/// This is the documented exception to the phase-1 strong-typing exit gate
/// (see the V1 rearchitecture plan, exit-gate clause 2): tool arguments and
/// results are driven by dynamic per-tool manifest schemas and must be
/// preserved verbatim, so only the outer container is enforced here — every
/// payload must be a JSON object (named arguments / result fields). The
/// producer MUST assemble and validate the fields against the tool manifest
/// schema before constructing this type (`try_new`), and `serde_json::Value`
/// never enters the main event stream through any other path. If tool schemas
/// ever become statically generated, this exception is replaced by a canonical
/// strong-typed JSON structure.
/// 这是阶段 1 强类型退出门的文档化例外(见 V1 重构方案退出门条款 2)：工具
/// 参数与结果由动态的逐工具 Manifest Schema 驱动且必须原样保留，因此这里
/// 只强制外层容器——每个载荷必须是 JSON 对象(命名参数/结果字段)。生产方
/// 必须在本类型构造前(`try_new`)按工具 Manifest Schema 完成参数组装与验证；
/// `serde_json::Value` 不得以任何其他路径进入主事件流。若工具 Schema 未来
/// 变成静态生成，本例外由明确的 canonical 强类型 JSON 结构取代。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ToolPayload(serde_json::Value);

impl ToolPayload {
    /// Validates and builds a payload whose fields were assembled against the
    /// tool manifest schema. Only JSON objects are accepted.
    /// 校验并按工具 Manifest Schema 组装后的字段构建载荷。只接受 JSON 对象。
    pub fn try_new(value: serde_json::Value) -> Result<Self, crate::v1::errors::DomainError> {
        if !value.is_object() {
            return Err(crate::v1::errors::DomainError::InvalidToolPayload {
                actual_kind: json_value_kind(&value),
            });
        }
        Ok(Self(value))
    }

    /// Builds the canonical empty object payload without a fallible path.
    /// 构建规范空对象载荷，不经过可失败路径。
    pub fn empty() -> Self {
        Self(serde_json::Value::Object(serde_json::Map::new()))
    }

    /// Builds the canonical explicit output for a terminal tool error that did
    /// not receive a host-provided structured result.
    /// 为没有收到宿主结构化结果的工具错误终态构建规范明确输出。
    pub fn terminal_failure(kind: ToolTerminalErrorKind) -> Self {
        let value = serde_json::to_value(ToolTerminalFailure {
            ok: false,
            error: ToolTerminalError {
                kind,
                message: kind.message(),
            },
        })
        .expect("the strongly typed terminal tool failure always serializes");
        Self(value)
    }
}

impl<'de> Deserialize<'de> for ToolPayload {
    /// Deserializes through the same object-only validation used by producers.
    /// 通过生产方共用的仅对象校验入口执行反序列化。
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Self::try_new(value).map_err(DeserializerType::Error::custom)
    }
}

/// Returns the stable JSON kind name used by payload validation errors.
/// 返回载荷校验错误使用的稳定 JSON 类型名称。
fn json_value_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// One complete tool invocation event.
/// 一次完整工具调用事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallEvent {
    /// Owning physical API call group.
    /// 所属物理 API 调用组。
    pub api_call_id: ApiCallId,
    /// Local tool invocation id.
    /// 本地工具调用 id。
    pub tool_call_id: ToolCallId,
    /// Provider stream-item identity that owns this tool's live request draft.
    /// 拥有该工具实时请求草稿的 Provider 流 item 身份。
    pub request_stream_id: RequestStreamId,
    /// Tool name as invoked.
    /// 被调用的工具名。
    pub tool_name: String,
    /// Fully assembled input arguments.
    /// 完整组装后的输入参数。
    pub input: ToolPayload,
    /// Tool output after execution.
    /// 工具执行后的输出。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<ToolPayload>,
    /// Canonical persisted change-set for file-editing tools.
    /// 文件编辑工具的 canonical 持久化 change-set。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changeset: Option<PersistentChangeSet>,
    /// Lifecycle status updated in place on the same event.
    /// 在同一事件上原位更新的生命周期状态。
    pub status: ToolCallStatus,
    /// Tool start time in Unix milliseconds.
    /// 工具开始时间(Unix 毫秒)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<i64>,
    /// Tool finish time in Unix milliseconds.
    /// 工具结束时间(Unix 毫秒)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<i64>,
    /// Provider-native tool call id when supplied by the protocol.
    /// 协议提供时的 Provider 原生 tool call id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_tool_call_id: Option<String>,
    /// Navigation-only external resource references.
    /// 仅用于导航的外部资源引用。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_refs: Vec<ExternalResourceRef>,
    /// Ordered project-owned resources generated by this invocation.
    /// 本次调用生成的有序项目级资源。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_resources: Vec<GeneratedResourceRef>,
    /// Provider source, allowed on the last Provider-sourced event of a
    /// completed API group (see the ordering rules).
    /// Provider 来源；只允许出现在已完成的 API 组最后一个 Provider 来源
    /// 事件上(见顺序规则)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_source: Option<ProviderSource>,
}

/// Purpose of one physical Provider API call.
/// 一次物理 Provider API 调用的用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiPurposeKind {
    /// Ordinary main generation of a turn.
    /// 回合的普通主生成。
    MainGeneration,
    /// Retried physical call.
    /// 重试后的物理调用。
    RetryGeneration,
    /// Context-compaction request.
    /// 上下文压缩请求。
    Compaction,
    /// Continue/resume generation.
    /// 继续/恢复生成。
    Continue,
    /// Physical call inside or for a subagent.
    /// 子代理内部或为子代理发起的物理调用。
    Subagent,
    /// Physical call dedicated to answering one collaboration question.
    /// 专门用于回答一条协作问题的物理调用。
    QuestionExecution,
}

/// Final state of one physical Provider API call.
/// 一次物理 Provider API 调用的终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiCallStatus {
    /// Request prepared but not dispatched.
    /// 请求已准备但尚未发出。
    Prepared,
    /// Request dispatched to the Provider.
    /// 请求已发给 Provider。
    Dispatched,
    /// Response events are still streaming.
    /// 响应事件仍在流式返回。
    InProgress,
    /// Call completed successfully.
    /// 调用成功完成。
    Completed,
    /// Call failed with an error.
    /// 调用失败。
    Failed,
    /// Call was cancelled before completion.
    /// 调用在完成前被取消。
    Cancelled,
    /// Call was interrupted by a host stop or crash projection.
    /// 调用被宿主停止或崩溃投影中断。
    Interrupted,
}

/// Stable end-reason classification of one physical API call.
/// 一次物理 API 调用的稳定结束原因分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiEndReason {
    /// The Provider returned a successful terminal state.
    /// Provider 返回了成功终态。
    CompletedSuccessfully,
    /// The Provider returned an error.
    /// Provider 返回错误。
    ProviderError,
    /// The request failed due to a network fault.
    /// 请求因网络故障失败。
    NetworkError,
    /// The account balance was insufficient.
    /// 账户余额不足。
    BalanceInsufficient,
    /// The user manually interrupted the call.
    /// 用户手动中断调用。
    UserInterrupted,
    /// The host process ended before the physical call reached a terminal boundary.
    /// 宿主进程在物理调用到达终态边界前结束。
    ProcessInterrupted,
    /// Retry attempts were exhausted.
    /// 重试次数耗尽。
    RetryExhausted,
    /// The call was cancelled explicitly.
    /// 调用被显式取消。
    Cancelled,
    /// The end reason is unknown or not classified.
    /// 结束原因未知或未分类。
    Unknown,
}

/// Durable policy decision controlling whether one failed physical API may continue.
/// 控制一次失败物理 API 是否允许继续的持久化策略判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationEligibility {
    /// A later explicit client action may safely resume the same logical Turn.
    /// 后续显式客户端动作可以安全恢复同一逻辑回合。
    Allowed,
    /// The terminal failure must not be resumed without a new root Turn.
    /// 该终态失败不得在没有新根回合的情况下恢复。
    Forbidden,
}

/// Classification of one redacted API error.
/// 一条脱敏 API 错误的分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCategory {
    /// Provider-side error.
    /// Provider 侧错误。
    Provider,
    /// Network/transport error.
    /// 网络/传输错误。
    Network,
    /// Authentication or authorization error.
    /// 认证或授权错误。
    Auth,
    /// Balance-related error.
    /// 余额相关错误。
    Balance,
    /// Protocol mismatch error.
    /// 协议不匹配错误。
    Protocol,
    /// Request timeout.
    /// 请求超时。
    Timeout,
    /// Unclassified error.
    /// 未分类错误。
    Unknown,
}

/// One redacted error attached to an API statistic.
/// 附着在 API 统计上的一条脱敏错误。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactedApiError {
    /// Stable error category.
    /// 稳定错误分类。
    pub category: ApiErrorCategory,
    /// Optional stable error code from the Provider.
    /// Provider 提供的可选稳定错误码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Redacted human-readable detail; the producer must strip secrets.
    /// 脱敏后的人类可读详情；产出方必须移除秘密。
    pub message: String,
}

/// Timing checkpoints of one physical API call.
/// 一次物理 API 调用的时间点。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiTimings {
    /// Request-prepared time in Unix milliseconds.
    /// 请求准备时间(Unix 毫秒)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_at_ms: Option<i64>,
    /// Request-dispatched time in Unix milliseconds.
    /// 请求发出时间(Unix 毫秒)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched_at_ms: Option<i64>,
    /// First response-event time in Unix milliseconds.
    /// 首个响应事件时间(Unix 毫秒)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_event_at_ms: Option<i64>,
    /// First token time in Unix milliseconds.
    /// 首个 token 时间(Unix 毫秒)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_token_at_ms: Option<i64>,
    /// Completion time in Unix milliseconds.
    /// 完成时间(Unix 毫秒)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
}

/// One physical Provider API statistic event.
/// 一条物理 Provider API 统计事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiStatsEvent {
    /// Owning physical API call.
    /// 所属物理 API 调用。
    pub api_call_id: ApiCallId,
    /// Execution purpose of the call.
    /// 调用的执行用途。
    pub purpose: ApiPurposeKind,
    /// Provider name actually used.
    /// 实际使用的 Provider 名。
    pub provider: String,
    /// Optional protocol profile name.
    /// 可选协议 Profile 名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Model actually used by this call.
    /// 本次调用实际使用的模型。
    pub model: String,
    /// Non-secret connection id actually used.
    /// 实际使用的非秘密连接 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Thinking level actually used by this call.
    /// 本次调用实际使用的思考级别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    /// Temperature actually used by this call.
    /// 本次调用实际使用的温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Official usage snapshot; provider-missing dimensions stay unknown.
    /// 官方用量快照；Provider 缺失的维度保持未知。
    pub usage: UsageSnapshot,
    /// Final call status.
    /// 调用终态。
    pub status: ApiCallStatus,
    /// Optional terminal end-reason classification.
    /// 可选终态结束原因分类。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<ApiEndReason>,
    /// Optional redacted error detail.
    /// 可选脱敏错误详情。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RedactedApiError>,
    /// Frozen transport-policy decision for later explicit continuation.
    /// 为后续显式继续冻结的 transport 策略判定。
    pub continuation_eligibility: ContinuationEligibility,
    /// Timing checkpoints of the call.
    /// 调用的时间点。
    pub timing: ApiTimings,
}

/// Closure outcome of one logical turn.
/// 一个逻辑回合的闭合结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    /// A complete final assistant finished the turn.
    /// 完整最终 assistant 完成回合。
    Completed,
    /// The user explicitly abandoned the turn.
    /// 用户明确放弃回合。
    Abandoned,
    /// The turn was stopped by the host.
    /// 回合被宿主停止。
    Stopped,
}

/// Durable distinction between a business turn and an auxiliary question attempt.
/// 业务回合与辅助答疑尝试之间的持久化区分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnBoundaryPurpose {
    /// Business completion consumes one logical-turn context lifetime.
    /// 业务完成消耗一次逻辑回合上下文寿命。
    Business,
    /// Auxiliary completion records usage without consuming business context lifetime.
    /// 辅助完成记录消耗，但不消费业务上下文寿命。
    QuestionExecution,
    /// Maintenance records its own closure without consuming a business context lifetime.
    /// 维护记录自身闭合，不消耗业务上下文寿命。
    ContextMaintenance,
}

/// Explicit host classification retained independently of the aggregate turn outcome.
/// 独立于聚合回合结果保留的明确宿主分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnClosureReason {
    /// The business or auxiliary execution completed normally.
    /// 业务或辅助执行正常完成。
    Completed,
    /// The user abandoned the original logical turn.
    /// 用户放弃原逻辑回合。
    Abandoned,
    /// The host explicitly stopped the execution without classifying it as failure.
    /// 宿主明确停止执行，未将其分类为失败。
    Stopped,
    /// The host failure transition atomically closed the execution with its diagnostic.
    /// 宿主失败转换连同诊断原子闭合执行。
    Failed,
}

impl TurnClosureReason {
    /// Maps explicit `self` classification to the existing turn-statistic outcome.
    /// 将明确的 `self` 分类映射到既有回合统计结果。
    /// Returns Stopped for a failure without losing the separately persisted failure classification.
    /// 失败返回 Stopped，同时不丢失单独持久化的失败分类。
    pub fn outcome(self) -> TurnOutcome {
        match self {
            Self::Completed => TurnOutcome::Completed,
            Self::Abandoned => TurnOutcome::Abandoned,
            Self::Stopped | Self::Failed => TurnOutcome::Stopped,
        }
    }
}

impl From<TurnOutcome> for TurnClosureReason {
    /// Maps an explicit ordinary `outcome`; failure callers must select Failed themselves.
    /// 映射明确普通 `outcome`；失败调用方必须自行选择 Failed。
    /// Returns the corresponding normal closure classification without inspecting message text.
    /// 返回对应普通闭合分类，不检查消息文本。
    fn from(outcome: TurnOutcome) -> Self {
        match outcome {
            TurnOutcome::Completed => Self::Completed,
            TurnOutcome::Abandoned => Self::Abandoned,
            TurnOutcome::Stopped => Self::Stopped,
        }
    }
}

/// Turn closure fact written before the unique turn statistic.
/// 在唯一回合统计之前写入的回合闭合事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnBoundaryEvent {
    /// Required host reason published atomically with closure, never inferred from diagnostics.
    /// 与闭合原子发布的必需宿主原因，绝不从诊断推断。
    pub reason: TurnClosureReason,
    /// Terminal closure outcome.
    /// 终态闭合结果。
    pub outcome: TurnOutcome,
    /// Immutable closure purpose, retained even when the opening event is compacted.
    /// 不可变闭合用途，即使起始事件被压缩也继续保留。
    pub purpose: TurnBoundaryPurpose,
}

/// The single terminal statistic event of one truly closed turn.
/// 一个真正闭合回合的唯一终态统计事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnStatsEvent {
    /// First main-event seq covered by this turn.
    /// 本回合覆盖的首个主事件 seq。
    pub start_seq: Seq,
    /// Last main-event seq covered by this turn.
    /// 本回合覆盖的最后一个主事件 seq。
    pub end_seq: Seq,
    /// Terminal closure outcome.
    /// 终态闭合结果。
    pub outcome: TurnOutcome,
    /// Turn wall-clock duration in milliseconds.
    /// 回合墙钟耗时(毫秒)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    /// Direct/subagent/total aggregate buckets.
    /// direct/subagent/total 三组聚合口径。
    pub buckets: TurnStatBuckets,
}

/// Content actually returned to the parent model with its source range.
/// 实际回流给父模型的内容及其来源范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubagentReturnEvent {
    /// Exact child generation that owns all source sequence numbers.
    /// 拥有全部来源序号的精确子代次。
    pub child_segment_generation: u64,
    /// Original root action identifying the returned child execution.
    /// 标识回流子执行的原根动作。
    pub child_root_action_id: ClientActionId,
    /// Parent session identity that owns this returned result.
    /// 拥有本次回流结果的父会话身份。
    pub parent_session_id: String,
    /// Subagent session id that produced the content.
    /// 产出该内容的子代理会话 id。
    pub child_session_id: String,
    /// Stable subagent task identity.
    /// 稳定的子代理任务身份。
    pub child_task_id: SubagentTaskId,
    /// Content actually handed to the parent model.
    /// 实际交给父模型的内容。
    pub content: String,
    /// Exact completed assistant-text seq selected as the returned body.
    /// 被选为回流正文的精确已完成 assistant 文本 seq。
    pub source_assistant_text_seq: Seq,
    /// Start of the child-session event range that produced this content.
    /// 产出该内容的子会话事件范围起始 seq。
    pub source_from_seq: Seq,
    /// End of the child-session event range that produced this content.
    /// 产出该内容的子会话事件范围结束 seq。
    pub source_to_seq: Seq,
    /// Terminal TurnStats seq inside the child session.
    /// 子会话内终态 TurnStats 的 seq。
    pub child_turn_stats_seq: Seq,
}

/// Stable notice kind of one permanent UI timeline fact.
/// 一条永久 UI 时间线事实的稳定通知种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayNoticeKind {
    /// Provider-side error.
    /// Provider 侧错误。
    ProviderError,
    /// Host/system error.
    /// 宿主/系统错误。
    SystemError,
    /// Network connection was lost.
    /// 网络连接断开。
    NetworkDisconnected,
    /// Account balance became insufficient.
    /// 账户余额不足。
    BalanceInsufficient,
    /// Automatic retry cycle started.
    /// 自动重试周期开始。
    RetryStarted,
    /// Retry attempts were exhausted.
    /// 重试次数耗尽。
    RetryExhausted,
    /// Execution resumed after interruption.
    /// 中断后恢复执行。
    Resumed,
    /// Context compaction completed.
    /// 上下文压缩完成。
    CompactionCompleted,
    /// Goal state changed (continuing, blocked, budget exhausted, complete).
    /// Goal 状态变化(继续、阻塞、预算耗尽、完成)。
    GoalStatus,
    /// Execution interrupted (manual stop or crash projection).
    /// 执行中断(手动停止或崩溃投影)。
    Interrupted,
    /// User manually stopped at a point in time.
    /// 用户在某个时间点手动停止。
    Stopped,
    /// A dedicated session locked its memory at this point.
    /// 专职会话在此处锁定了记忆。
    MemoryLocked,
    /// A dedicated session unlocked its memory at this point.
    /// 专职会话在此处解除了记忆锁定。
    MemoryUnlocked,
}

/// Severity of one permanent UI notice.
/// 一条永久 UI 通知的严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeSeverity {
    /// Informational notice.
    /// 信息性通知。
    Info,
    /// Warning notice.
    /// 警告通知。
    Warning,
    /// Error notice.
    /// 错误通知。
    Error,
}

/// One permanent UI timeline notice event.
/// 一条永久 UI 时间线通知事件。
///
/// Host-derived numeric aggregates (tokens, call counts, elapsed time) must
/// NOT be copied into this payload; they are recomputed from persisted facts
/// with this event's own `seq` as the cutoff boundary.
/// 宿主派生的数值聚合(token、调用数、耗时)绝不能复制进该载荷；它们以本
/// 事件的 `seq` 为截止边界从已落盘事实动态重算。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayNoteEvent {
    /// Stable notice kind.
    /// 稳定通知种类。
    pub notice_kind: DisplayNoticeKind,
    /// Severity level.
    /// 严重级别。
    pub severity: NoticeSeverity,
    /// Stable machine-readable reason code.
    /// 稳定机器可读原因码。
    pub reason_code: String,
    /// Optional redacted human-readable detail.
    /// 可选脱敏人类可读详情。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redacted_detail: Option<String>,
    /// Optional related physical API call.
    /// 可选关联物理 API 调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_call_id: Option<ApiCallId>,
    /// Executed retry count at notice time.
    /// 通知时刻已执行的重试次数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
    /// Required retry limit when applicable.
    /// 适用的必要重试上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_limit: Option<u32>,
}

/// Exact before/after request configuration rendered as one permanent UI fact.
/// 作为一条永久 UI 事实呈现的精确请求配置前后值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestConfigChangedEvent {
    /// Configuration effective before the safe-boundary promotion.
    /// 安全边界提升前生效的配置。
    pub previous: ActiveRequestConfig,
    /// Configuration effective after the safe-boundary promotion.
    /// 安全边界提升后生效的配置。
    pub current: ActiveRequestConfig,
}

/// Kind of one Provider-invisible execution control fact.
/// 一条 Provider 不可见执行控制事实的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionControlKind {
    /// A currently executing segment was interrupted.
    /// 当前执行段被中断。
    ExecutionInterrupted,
    /// A retry was requested.
    /// 请求了重试。
    ExecutionRetryRequested,
    /// Retry attempts were exhausted.
    /// 重试次数耗尽。
    ExecutionRetryExhausted,
    /// The user requested to continue the open turn.
    /// 用户请求继续开放回合。
    TurnContinueRequested,
    /// The open turn resumed generation.
    /// 开放回合恢复生成。
    TurnResumed,
    /// The transport accepted the immutable request for physical dispatch.
    /// transport 已接受不可变请求并进入物理派发。
    ApiTransportObserved,
    /// Execution recovered after a crash or reconnect.
    /// 崩溃或断连后恢复执行。
    Recovered,
}

/// One Provider-invisible execution control event.
/// 一条 Provider 不可见执行控制事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionControlEvent {
    /// Stable control kind.
    /// 稳定控制种类。
    pub control_kind: ExecutionControlKind,
    /// Optional related physical API call.
    /// 可选关联物理 API 调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_call_id: Option<ApiCallId>,
    /// External idempotency identity for a client Continue action.
    /// 客户端继续动作的外部幂等身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_action_id: Option<ClientActionId>,
}

/// Provider-native replay draft supplied before the document selects coverage.
/// 文档选定覆盖位置之前由调用方提供的 Provider 原生重放草稿。
///
/// This input deliberately excludes local seq and fingerprint fields. Only the
/// owning [`crate::v1::document::SessionDocument`] may derive those values from
/// its authoritative event stream.
/// 本输入刻意排除本地 seq 与指纹字段；只有拥有权威事件流的
/// [`crate::v1::document::SessionDocument`] 可以推导这些值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSourceInput {
    /// Provider name.
    /// Provider 名。
    pub provider: String,
    /// Optional protocol profile name.
    /// 可选协议 Profile 名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Model used by the replay checkpoint.
    /// 重放检查点使用的模型。
    pub model: String,
    /// Non-secret connection id.
    /// 非秘密连接 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Provider response/message id used as the replay cursor.
    /// 用作重放游标的 Provider 响应/消息 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Additional Provider response id when the protocol uses both.
    /// 协议同时使用两者时的附加 Provider 响应 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    /// Closed strongly typed Provider replay envelope.
    /// 封闭强类型 Provider 重放封套。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<ProviderReplayEnvelope>,
}

/// Frozen fingerprint format, independent of outbound request filtering policy.
/// 冻结的指纹格式，独立于出站请求过滤策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFingerprintVersion {
    /// Hash immutable projected facts, preserving partial assistant text and reasoning.
    /// 哈希不可变投影事实，保留部分 assistant 正文与推理。
    ImmutableProjectionV1,
}

/// Provider-native replay identity and fingerprint carried by one API group.
/// 一个 API 组携带的 Provider 原生重放身份与指纹。
///
/// Allowed on reasoning, assistant text, or tool-call events; a completed API
/// group must carry exactly one and only on its last Provider-sourced event.
/// `fingerprint_sha256` must be computed over the immutable Provider-visible
/// projection only: it excludes this `provider_source` and any host-later
/// fields such as tool output, changeset, terminal status, and timing.
/// 允许出现在 reasoning、assistant 正文或工具调用事件上；已完成的 API 组
/// 必须恰好携带一个，且只挂在组内最后一个 Provider 来源事件上。
/// `fingerprint_sha256` 必须只对不可变 Provider 可见投影计算：排除本
/// `provider_source` 自身以及宿主后写的工具输出、changeset、终态与时间。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSource {
    /// Provider name.
    /// Provider 名。
    pub provider: String,
    /// Optional protocol profile name.
    /// 可选协议 Profile 名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Model used by the replay checkpoint.
    /// 重放检查点使用的模型。
    pub model: String,
    /// Non-secret connection id.
    /// 非秘密连接 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Provider response/message id used as the replay cursor.
    /// 用作重放游标的 Provider 响应/消息 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Additional Provider response id when the protocol uses both.
    /// 协议同时使用两者时的附加 Provider 响应 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    /// Closed strongly typed Provider-native replay state.
    /// 封闭强类型 Provider 原生重放状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<ProviderReplayEnvelope>,
    /// Local main-event seq covered by this replay checkpoint.
    /// 该重放检查点覆盖到的本地主事件 seq。
    pub covered_until_seq: Seq,
    /// Explicit hash contract required on every checkpoint.
    /// 每个检查点都必须携带的显式哈希契约。
    pub fingerprint_version: ProviderFingerprintVersion,
    /// Real SHA-256 hex digest of the immutable Provider-visible projection.
    /// 不可变 Provider 可见投影的真实 SHA-256 十六进制摘要。
    pub fingerprint_sha256: String,
}
