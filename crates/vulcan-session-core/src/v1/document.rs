//! V1 authoritative in-memory session document.
//! V1 权威内存会话文档。
//!
//! `SessionDocument` is the single authority for the whole session while the
//! process runs. All runtime producers call strongly typed mutation methods;
//! nothing mutates raw JSON. Mutations allocate or preserve `seq`, bump a
//! memory revision, and mark the affected files dirty. Streaming deltas only
//! update resident events and never allocate new seq values.
//! `SessionDocument` 是进程运行期间整个会话的唯一权威。所有运行时生产者
//! 只调用强类型 mutation 方法，没有任何代码直接修改原始 JSON。每次 mutation
//! 分配或保持 `seq`、递增内存修订号并标记受影响的文件 dirty。流式 delta
//! 只更新常驻事件，绝不分配新的 seq。
//!
//! Persistence ordering equals actual time ordering: the `seq` allocated by
//! each mutation is exactly the order in which the semantic fact occurred.
//! 持久化顺序等于实际时间顺序：每次 mutation 分配的 `seq` 正是该语义事实
//! 实际发生的顺序。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::v1::budget::ProviderRequestBudgetEvidence;
use crate::v1::compaction_frontier::{assert_node_order, trim_unpublished_candidates};
use crate::v1::config::{
    ActiveRequestConfig, DraftState, ForkOrigin, GoalBudgetCache, GoalConfig, GoalState,
    IdFrontier, KnowledgeLibrarySelectionConfig, KnowledgeLibrarySelectionMode,
    MAX_GOAL_OBJECTIVE_CHARS, MetadataDocument, PendingInTurnInput, PendingRequestConfig,
    ProviderExtensionTargetSetConfig, ProviderMediaUnderstandingSelection,
    ProviderToolCapabilitySelection, ProviderToolSelectionConfig, ProviderToolTargetConfig,
    QueuedInput, QueuedInputIdentity, SegmentInfo, VmmSessionPolicyConfig,
};
use crate::v1::context::{
    CompactionNode, LockedBaselineProjection, RuntimeInitializationState, RuntimeLock,
};
use crate::v1::errors::DomainError;
use crate::v1::event::*;
use crate::v1::goal::{GoalStopReason, decide_goal_continuation};
use crate::v1::ids::{
    ApiCallId, ClientActionId, CompactionId, IdAllocator, JS_SAFE_INTEGER_MAX, PendingInputId,
    RequestStreamId, ResourceId, Seq, SubagentTaskId, ToolCallId, TurnId,
};
use crate::v1::projection::{provider_visible_fingerprint, validate_system_context_event};
use crate::v1::stats::{StatBucket, TurnStatBuckets, UsageSnapshot, goal_budget_tokens};
use vulcan_change_set::{PersistentChangeSet, validate_persistent_change_set};

mod generation;

pub use generation::{
    EditGenerationInput, ForkGenerationInput, GenerationBuildOutput, RootInputReplacement,
};

/// One durable file that can become dirty.
/// 一个可能变为 dirty 的持久化文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DirtyFile {
    /// `metadata.json`.
    /// `metadata.json`。
    Metadata,
    /// `runtime.json`.
    /// `runtime.json`。
    Runtime,
    /// `compactions.json`.
    /// `compactions.json`。
    Compactions,
    /// The active tail segment file.
    /// 活动尾段分片文件。
    ActiveTail,
}

/// Where one turn came from.
/// 一个回合的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOrigin {
    /// A root user prompt opened the turn.
    /// 根用户提示词开启回合。
    UserRoot,
    /// Goal auto-continue opened the turn.
    /// Goal 自动续跑开启回合。
    GoalContinuation,
}

/// Expected semantic kind of one resident event owned by the active API.
/// 活动 API 所拥有的一条常驻事件的预期语义种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveApiEventKind {
    /// Assistant reasoning semantic.
    /// Assistant reasoning 语义。
    Reasoning,
    /// Assistant text semantic.
    /// Assistant 正文语义。
    Text,
    /// Tool invocation semantic.
    /// 工具调用语义。
    Tool,
}

/// Resident state of the currently open logical turn.
/// 当前开放逻辑回合的常驻状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTurnState {
    /// The open turn id.
    /// 开放回合 id。
    pub turn_id: TurnId,
    /// Where the turn came from.
    /// 回合来源。
    pub origin: TurnOrigin,
    /// First main-event seq of the turn.
    /// 回合的首个主事件 seq。
    pub start_seq: Seq,
    /// Whether a terminal boundary already closed the turn.
    /// 是否已经用终态边界闭合回合。
    pub closed: bool,
}

/// One client-authored input carrying its durable RPC idempotency identity.
/// 一条携带持久化 RPC 幂等身份的客户端输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientInputSubmission {
    /// Stable external idempotency identity supplied by the client.
    /// 客户端提供的稳定外部幂等身份。
    pub client_action_id: ClientActionId,
    /// Exact user text without normalization.
    /// 未经归一化的精确用户正文。
    pub text: String,
    /// Ordered project resource identities.
    /// 有序项目资源身份。
    pub resource_ids: Vec<ResourceId>,
}

impl ClientInputSubmission {
    /// Builds one strongly typed client input submission.
    /// 构建一条强类型客户端输入提交。
    ///
    /// # Parameters
    /// 参数
    /// `client_action_id` is the external RPC idempotency key; `text` and
    /// `resource_ids` are the exact model-visible input payload.
    /// `client_action_id` 是外部 RPC 幂等键；`text` 与 `resource_ids` 是精确的模型可见输入载荷。
    ///
    /// # Returns
    /// 返回值
    /// Returns a submission whose external identity is already validated.
    /// 返回外部身份已经过校验的提交。
    pub fn new(
        client_action_id: ClientActionId,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
    ) -> Self {
        Self {
            client_action_id,
            text: text.into(),
            resource_ids,
        }
    }
}

/// Durable disposition selected while accepting one future-turn input.
/// 接受一条未来回合输入时选定的持久化处置结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueInputDisposition {
    /// The input remains in the metadata FIFO queue.
    /// 输入继续保留在元数据 FIFO 队列中。
    Queued,
    /// The input atomically superseded an undispatched Goal continuation and opened a UserRoot Turn.
    /// 输入原子取代了尚未派发的 Goal 续跑，并开启一个 UserRoot 回合。
    SupersededGoalAndScheduled {
        /// Goal-continuation Turn closed by the user input.
        /// 被用户输入闭合的 Goal 续跑回合。
        stopped_turn_id: TurnId,
        /// Unique terminal TurnStats seq of the stopped Goal Turn.
        /// 被停止 Goal 回合的唯一终态 TurnStats seq。
        stopped_turn_stats_seq: Seq,
        /// Newly opened UserRoot Turn.
        /// 新开启的 UserRoot 回合。
        scheduled_turn_id: TurnId,
        /// Provider-visible seq of the scheduled user input.
        /// 已调度用户输入的 Provider 可见 seq。
        input_seq: Seq,
    },
}

/// Durable receipt of one idempotent enqueue request.
/// 一次幂等排队请求的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueInputReceipt {
    /// Exact external client action identity.
    /// 精确外部客户端动作身份。
    pub client_action_id: ClientActionId,
    /// Stable session-local pending-input identity.
    /// 稳定的会话本地待处理输入身份。
    pub input_id: PendingInputId,
    /// Authoritative queue or immediate-scheduling disposition.
    /// 权威排队或立即调度处置结果。
    pub disposition: EnqueueInputDisposition,
}

/// Durable receipt of one draft-state mutation.
/// 一次草稿状态修改的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftMutationReceipt {
    /// Exact draft state after the mutation.
    /// 修改后的精确草稿状态。
    pub draft: DraftState,
}

/// Durable receipt of restoring one queued input into the composer draft.
/// 把一条排队输入恢复到输入区草稿的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedInputDraftReceipt {
    /// Exact queue identity moved into the draft.
    /// 被移动到草稿的精确队列身份。
    pub identity: QueuedInputIdentity,
    /// Exact draft state published by the transition.
    /// 该转换发布的精确草稿状态。
    pub draft: DraftState,
}

/// Durable receipt of removing one queued input.
/// 移除一条排队输入的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedInputRemovalReceipt {
    /// Exact queue identity removed from metadata.
    /// 从元数据中移除的精确队列身份。
    pub identity: QueuedInputIdentity,
}

/// Durable receipt of guiding one queued input into the currently open logical Turn.
/// 把一条排队输入引导进当前开放逻辑回合的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedInputPendingTurnReceipt {
    /// Exact queue identity transferred without allocating a replacement input id.
    /// 无需分配替代输入 id 即完成转移的精确队列身份。
    pub identity: QueuedInputIdentity,
    /// Open logical Turn that now owns the pending input.
    /// 当前持有该过程内输入的开放逻辑回合。
    pub turn_id: TurnId,
    /// UiOnly ledger seq that durably records the queue resolution.
    /// 持久记录该队列处置的 UiOnly 账本 seq。
    pub resolution_seq: Seq,
}

/// Durable receipt of one complete FIFO queue reorder.
/// 一次完整 FIFO 队列重排的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedInputOrderReceipt {
    /// Exact complete queue identity order after the mutation.
    /// 修改后的精确完整队列身份顺序。
    pub ordered: Vec<QueuedInputIdentity>,
}

/// One explicit request to resume the current unfinished logical Turn.
/// 一次恢复当前未完成逻辑回合的显式请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationRequest {
    /// Client submission and optional supplementary user payload.
    /// 客户端提交身份与可选补充用户载荷。
    pub submission: ClientInputSubmission,
    /// Optional turn-scoped developer guidance admitted in the same durable mutation.
    /// 在同一可靠修改中准入的可选回合级 developer 引导。
    pub hidden_model_guidance: Option<String>,
}

impl ContinuationRequest {
    /// Builds one strongly typed continuation request.
    /// 构建一条强类型继续请求。
    ///
    /// # Parameters
    /// 参数
    /// `client_action_id` is the exact external idempotency key; empty text
    /// plus no resources means a pure Continue without an added user input.
    /// `client_action_id` 是精确的外部幂等键；空正文且无资源表示不附加用户输入的纯继续。
    ///
    /// # Returns
    /// 返回值
    /// Returns the immutable continuation request.
    /// 返回不可变的继续请求。
    pub fn new(
        client_action_id: ClientActionId,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
    ) -> Self {
        Self {
            submission: ClientInputSubmission::new(client_action_id, text, resource_ids),
            hidden_model_guidance: None,
        }
    }

    /// Attaches one explicit turn-scoped developer guidance payload.
    /// 附加一条显式的回合级 developer 引导载荷。
    ///
    /// # Parameters
    /// 参数
    /// `guidance` is the exact dynamic system semantic to persist with Continue admission.
    /// `guidance` 是与 Continue 准入一同持久化的精确动态系统语义。
    ///
    /// # Returns
    /// 返回值
    /// Returns the updated immutable request value.
    /// 返回更新后的不可变请求值。
    pub fn with_hidden_model_guidance(mut self, guidance: Option<String>) -> Self {
        self.hidden_model_guidance = guidance;
        self
    }
}

/// Durable receipt reconstructed from one continuation action's events.
/// 从一次继续动作的事件中重建的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinueTurnReceipt {
    /// Open logical Turn resumed by this action.
    /// 本动作恢复的开放逻辑回合。
    pub turn_id: TurnId,
    /// Optional local input identity when the action supplied new content.
    /// 本动作提供新内容时的可选本地输入身份。
    pub input_id: Option<PendingInputId>,
    /// Optional Provider-visible user-input seq.
    /// 可选的 Provider 可见用户输入 seq。
    pub input_seq: Option<Seq>,
    /// Optional seq of the turn-scoped developer guidance admitted with this action.
    /// 随本动作准入的可选回合级 developer 引导 seq。
    pub guidance_seq: Option<Seq>,
    /// Host-only seq recording the Continue request.
    /// 记录继续请求的 Host-only seq。
    pub requested_seq: Seq,
    /// Host-only seq recording that generation resumed.
    /// 记录生成已恢复的 Host-only seq。
    pub resumed_seq: Seq,
}

/// Explicit changed/no-op outcome of one idempotent domain mutation.
/// 一次幂等领域修改的显式 changed/no-op 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotentMutation<T> {
    /// The authoritative document committed one new semantic revision.
    /// 权威文档提交了一个新的语义修订。
    Committed(T),
    /// The exact action already existed and no revision changed.
    /// 完全相同的动作已经存在，且没有修订发生变化。
    Idempotent(T),
}

impl<T> IdempotentMutation<T> {
    /// Returns whether this invocation committed one new document revision.
    /// 返回本次调用是否提交了一个新的文档修订。
    pub const fn changed(&self) -> bool {
        matches!(self, Self::Committed(_))
    }

    /// Extracts the domain receipt regardless of changed/no-op status.
    /// 无论 changed/no-op 状态如何均提取领域 receipt。
    pub fn into_value(self) -> T {
        match self {
            Self::Committed(value) | Self::Idempotent(value) => value,
        }
    }
}

/// Resident state of the currently in-flight physical API group.
/// 当前在途物理 API 组的常驻状态。
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveApiState {
    /// The in-flight api call id.
    /// 在途 api call id。
    pub api_call_id: ApiCallId,
    /// Owning turn id.
    /// 所属回合 id。
    pub turn_id: TurnId,
    /// Execution purpose of the call.
    /// 调用的执行用途。
    pub purpose: ApiPurposeKind,
    /// Seq of the immutable API-start fact.
    /// 不可变 API 起始事实的 seq。
    pub started_seq: Seq,
    /// Resolved request configuration frozen before dispatch.
    /// 发送前冻结的已解析请求配置。
    pub request: ApiRequestSnapshot,
    /// Exact immutable projection proof frozen before dispatch.
    /// 派发前冻结的精确不可变投影证明。
    pub projection: ProviderProjectionProof,
    /// Seq of the reasoning event, if any.
    /// reasoning 事件的 seq(如有)。
    pub reasoning_seq: Option<Seq>,
    /// Seq of the assistant text event, if any.
    /// assistant 正文事件的 seq(如有)。
    pub text_seq: Option<Seq>,
    /// Seqs of the tool-call events in Provider order.
    /// 按 Provider 顺序排列的工具调用事件 seq。
    pub tool_seqs: Vec<Seq>,
    /// Seq of the api_stats event, if already terminal.
    /// api_stats 事件的 seq(已有终态时)。
    pub stats_seq: Option<Seq>,
    /// Whether `provider_source` was already placed on the group.
    /// 是否已在组上放置 `provider_source`。
    pub provider_source_placed: bool,
}

/// Complete terminal update for one existing tool-call event.
/// 一条既有工具调用事件的完整终态更新。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCompletion {
    /// Tool output after execution, when any.
    /// 工具执行后的输出（如有）。
    pub output: Option<ToolPayload>,
    /// Canonical persisted change-set produced by the tool, when any.
    /// 工具产生的 canonical 持久化 change-set（如有）。
    pub changeset: Option<PersistentChangeSet>,
    /// Terminal tool status.
    /// 工具终态。
    pub status: ToolCallStatus,
    /// Tool finish time in Unix milliseconds.
    /// 工具结束时间（Unix 毫秒）。
    pub finished_at_ms: i64,
    /// Navigation-only external references produced by the invocation.
    /// 本次调用产生的仅导航外部引用。
    pub external_refs: Vec<ExternalResourceRef>,
    /// Ordered project-owned resources generated by the invocation.
    /// 本次调用生成的有序项目级资源。
    pub generated_resources: Vec<GeneratedResourceRef>,
}

/// Durable receipt for one committed or exactly replayed tool completion.
/// 一次已提交或精确重放工具终态的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCompletionReceipt {
    /// Owning physical API identity.
    /// 所属物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Session-local tool identity.
    /// 会话本地工具身份。
    pub tool_call_id: ToolCallId,
    /// Seq of the durable ToolCallEvent.
    /// 持久化 ToolCallEvent 的 seq。
    pub tool_seq: Seq,
    /// Authoritative persisted terminal status.
    /// 权威持久化终态。
    pub status: ToolCallStatus,
    /// Authoritative persisted finish time in Unix milliseconds.
    /// 权威持久化结束时间（Unix 毫秒）。
    pub finished_at_ms: i64,
}

/// Immutable input used to reserve one tool-call seq before execution.
/// 在执行前用于预留一条工具调用 seq 的不可变输入。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolPreparation {
    /// Provider stream-item identity used to join live argument-draft updates.
    /// 用于关联实时参数草稿更新的 Provider 流 item 身份。
    pub request_stream_id: RequestStreamId,
    /// Tool name as emitted by the Provider.
    /// Provider 发出的工具名。
    pub tool_name: String,
    /// Fully assembled input arguments.
    /// 完整组装后的输入参数。
    pub input: ToolPayload,
    /// Provider-native tool call id when supplied by the protocol.
    /// 协议提供时的 Provider 原生工具调用 id。
    pub provider_tool_call_id: Option<String>,
}

/// Durable local identity reserved for one prepared Provider tool call.
/// 为一条已准备 Provider 工具调用预留的持久化本地身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedToolReceipt {
    /// Owning physical API identity.
    /// 所属物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Session-local tool identity used by execution and reload.
    /// 执行与重载使用的会话本地工具身份。
    pub tool_call_id: ToolCallId,
    /// Provider stream identity used only to join the argument draft.
    /// 仅用于关联参数草稿的 Provider 流身份。
    pub request_stream_id: RequestStreamId,
    /// Seq of the durable ToolCallEvent.
    /// 持久化 ToolCallEvent 的 seq。
    pub tool_seq: Seq,
}

/// Ordered durable receipt of one atomically prepared tool batch.
/// 一批原子准备工具的有序持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedToolBatchReceipt {
    /// Owning physical API identity.
    /// 所属物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Exact Provider order and local identity mapping of every tool.
    /// 每个工具的精确 Provider 顺序与本地身份映射。
    pub tools: Vec<PreparedToolReceipt>,
}

/// Complete terminal fact for one physical Provider API call.
/// 一次物理 Provider API 调用的完整终态事实。
#[derive(Debug, Clone, PartialEq)]
pub struct ApiCompletion {
    /// Resolved request identity that must equal the durable start snapshot.
    /// 必须等于持久化起始快照的已解析请求身份。
    pub request: ApiRequestSnapshot,
    /// Official Provider usage snapshot.
    /// Provider 官方用量快照。
    pub usage: UsageSnapshot,
    /// Terminal API status.
    /// API 终态。
    pub status: ApiCallStatus,
    /// Optional stable end-reason classification.
    /// 可选稳定结束原因分类。
    pub end_reason: Option<ApiEndReason>,
    /// Optional redacted API error.
    /// 可选脱敏 API 错误。
    pub error: Option<RedactedApiError>,
    /// Frozen transport-policy decision for later explicit continuation.
    /// 为后续显式继续冻结的 transport 策略判定。
    pub continuation_eligibility: ContinuationEligibility,
    /// API timing checkpoints.
    /// API 时间点。
    pub timing: ApiTimings,
    /// Optional Provider-native continuation source attached to the group.
    /// 附着到该组的可选 Provider 原生 continuation 来源。
    pub provider_source: Option<ProviderSourceInput>,
    /// Completion time in Unix milliseconds.
    /// 完成时间（Unix 毫秒）。
    pub completed_at_ms: i64,
}

/// Atomic receipt of one physical API terminal plus any in-turn inputs promoted behind it.
/// 一次物理 API 终态及其后提升的过程内输入的原子 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiGroupCompletionReceipt {
    /// Completed physical API identity.
    /// 已完成的物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Owning logical Turn that remains open until an explicit Turn completion transition.
    /// 所属逻辑回合；在显式回合完成转换前保持开放。
    pub turn_id: TurnId,
    /// Seq of the terminal ApiStats fact.
    /// 终态 ApiStats 事实的 seq。
    pub stats_seq: Seq,
    /// Ordered Provider-visible input seq values promoted after the terminal fact.
    /// 在终态事实之后提升的有序 Provider 可见输入 seq。
    pub promoted_input_seqs: Vec<Seq>,
}

/// Strongly typed input for atomically freezing a Provider request and starting its API group.
/// 用于原子冻结 Provider 请求并启动其 API 组的强类型输入。
#[derive(Debug, Clone, PartialEq)]
pub struct PrepareAndStartApiInput {
    /// Execution purpose of the physical request.
    /// 物理请求的执行用途。
    pub purpose: ApiPurposeKind,
    /// Fully resolved provider-agnostic routing configuration.
    /// 已完整解析且与具体 Provider DTO 解耦的路由配置。
    pub request: ApiRequestSnapshot,
    /// Proof computed from the exact immutable request projection consumed by transport.
    /// 从 transport 实际消费的精确不可变请求投影计算得到的证明。
    pub projection: ProviderProjectionProof,
    /// Non-sensitive budget evidence computed from the exact request released to transport.
    /// 从实际放行给 transport 的精确请求计算得到的非敏感预算证据。
    pub request_budget_evidence: ProviderRequestBudgetEvidence,
    /// Exact baseline used when the runtime is fresh; initialized sessions must match it.
    /// runtime 为 fresh 时使用的精确基线；已初始化会话必须与其完全一致。
    pub locked_baseline: LockedBaselineProjection,
    /// Continue action dispatched by this API; required only for Continue purpose.
    /// 本次 API 派发的继续动作；仅 Continue 用途必须提供。
    pub continuation_action_id: Option<ClientActionId>,
    /// Mutation time in Unix milliseconds.
    /// 领域修改时间（Unix 毫秒）。
    pub now_ms: i64,
}

/// Durable Core result returned before the immutable network request may be dispatched.
/// 不可变网络请求获准派发前返回的 Core 持久化结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedApiStart {
    /// Allocated physical API identity.
    /// 已分配的物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Seq of the durable ApiStarted fact.
    /// 持久化 ApiStarted 事实的 seq。
    pub started_seq: Seq,
    /// Exact projection proof persisted on ApiStarted.
    /// 持久化在 ApiStarted 上的精确投影证明。
    pub projection: ProviderProjectionProof,
    /// Continue action durably bound to this API, when applicable.
    /// 适用时持久化绑定到该 API 的继续动作。
    pub continuation_action_id: Option<ClientActionId>,
}

/// Durable receipt proving that transport observed one physical API request.
/// 证明 transport 已观察到一次物理 API 请求的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiTransportObservedReceipt {
    /// Physical API identity accepted by transport.
    /// transport 接受的物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Seq of the immutable ApiStarted fact.
    /// 不可变 ApiStarted 事实的 seq。
    pub started_seq: Seq,
    /// Seq of the durable transport-observed fact.
    /// 持久化 transport-observed 事实的 seq。
    pub observed_seq: Seq,
    /// Continue action owning the API, when applicable.
    /// 适用时拥有该 API 的继续动作。
    pub continuation_action_id: Option<ClientActionId>,
}

/// Durable dispatch state reconstructed for one Continue action.
/// 为一次 Continue 动作重建的持久化分派状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinuationDispatchResolution {
    /// One physical API is prepared but transport has not been observed or terminated.
    /// 一个物理 API 已准备，但尚未观察到传输，也尚未终止。
    AwaitingTransport {
        /// Physical API identity bound to the Continue action.
        /// 绑定到 Continue 动作的物理 API 身份。
        api_call_id: ApiCallId,
        /// Seq of the immutable ApiStarted event.
        /// 不可变 ApiStarted 事件的 seq。
        started_seq: Seq,
    },
    /// Transport observation is durably recorded.
    /// 传输观察已经可靠记录。
    TransportObserved(ApiTransportObservedReceipt),
    /// The physical API terminated before transport observation became durable.
    /// 物理 API 在传输观察可靠落盘前已经终止。
    TerminalBeforeTransport {
        /// Physical API identity bound to the Continue action.
        /// 绑定到 Continue 动作的物理 API 身份。
        api_call_id: ApiCallId,
        /// Seq of the immutable ApiStarted event.
        /// 不可变 ApiStarted 事件的 seq。
        started_seq: Seq,
        /// Seq of the terminal ApiStats event.
        /// 终态 ApiStats 事件的 seq。
        terminal_seq: Seq,
    },
}

/// The unique successor selected while atomically completing one Turn.
/// 原子完成一个回合时选定的唯一后继。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnSuccessor {
    /// No successor Turn was opened.
    /// 未开启后继回合。
    None,
    /// The FIFO head queued user input opened the next UserRoot Turn.
    /// FIFO 队首用户输入开启了下一个 UserRoot 回合。
    QueuedUser {
        /// Newly opened Turn identity.
        /// 新开启的回合身份。
        turn_id: TurnId,
        /// Reused queued input identity.
        /// 复用的排队输入身份。
        input_id: PendingInputId,
        /// Reused external client action identity.
        /// 复用的外部客户端动作身份。
        client_action_id: ClientActionId,
        /// Provider-visible seq of the scheduled input.
        /// 已调度输入的 Provider 可见 seq。
        input_seq: Seq,
    },
    /// An active Goal opened the next automatic continuation Turn.
    /// 活动 Goal 开启了下一个自动续跑回合。
    GoalContinuation {
        /// Newly opened Goal Turn identity.
        /// 新开启的 Goal 回合身份。
        turn_id: TurnId,
        /// Provider-visible goal-context seq.
        /// Provider 可见 Goal 上下文 seq。
        context_seq: Seq,
    },
}

/// Result of atomically completing one turn and selecting its unique successor.
/// 原子完成一个回合并选择唯一后继的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnCompletionTransition {
    /// Final statistics seq of the completed turn.
    /// 已完成回合的最终统计 seq。
    pub completed_turn_final_seq: Seq,
    /// Unique successor selected by queued-user-first policy.
    /// 按排队用户优先策略选定的唯一后继。
    pub successor: TurnSuccessor,
}

/// Exact durable child facts required to select one subagent return body.
/// 选择一份子代理回流正文所需的精确持久化子会话事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentReturnBuildRequest {
    /// Final child Turn identity.
    /// 子会话最终回合身份。
    pub final_turn_id: TurnId,
    /// Stable external task identity.
    /// 稳定的外部任务身份。
    pub child_task_id: SubagentTaskId,
    /// Exact completed assistant-text seq returned to the parent.
    /// 精确回流给父会话的已完成 assistant 正文 seq。
    pub source_assistant_text_seq: Seq,
}

/// Opaque validated input for one atomic parent-side subagent return.
/// 一次父会话侧原子子代理回流的不透明已校验输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentReturnInput {
    child_session_id: String,
    child_task_id: SubagentTaskId,
    content: String,
    source_assistant_text_seq: Seq,
    source_from_seq: Seq,
    source_to_seq: Seq,
    child_turn_stats_seq: Seq,
    totals: StatBucket,
}

/// Durable receipt for one atomic subagent content-and-statistics return.
/// 一次原子子代理正文与统计回流的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentReturnReceipt {
    /// Parent open Turn receiving the return.
    /// 接收回流的父开放回合。
    pub parent_turn_id: TurnId,
    /// Provider-visible subagent content seq.
    /// Provider 可见子代理正文 seq。
    pub return_seq: Seq,
    /// UiOnly inclusive statistics seq.
    /// UiOnly 包容性统计 seq。
    pub stats_seq: Seq,
    /// Exact child session identity.
    /// 精确子会话身份。
    pub child_session_id: String,
    /// Exact child task identity.
    /// 精确子代理任务身份。
    pub child_task_id: SubagentTaskId,
    /// Exact child terminal TurnStats seq.
    /// 精确子会话终态 TurnStats seq。
    pub child_turn_stats_seq: Seq,
    /// Exact child assistant-text seq used as the returned body.
    /// 被用作回流正文的精确子会话 assistant 文本 seq。
    pub source_assistant_text_seq: Seq,
}

/// Result of one atomic Goal configuration transition.
/// 一次 Goal 配置原子转换的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalModeTransition {
    /// Provider-visible context seq emitted by the transition, when any.
    /// 转换产生的 Provider 可见上下文 seq（如有）。
    pub context_seq: Option<Seq>,
    /// Permanent UI notice seq emitted by the transition.
    /// 转换产生的永久 UI 通知 seq。
    pub notice_seq: Seq,
    /// Goal continuation turn created by the transition, when any.
    /// 转换创建的 Goal 续跑回合（如有）。
    pub continuation_turn_id: Option<TurnId>,
}

/// Strongly typed atomic input for publishing one context-compaction generation.
/// 发布一个上下文压缩代际的强类型原子输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionCommitInput {
    /// Physical compaction API identity.
    /// 物理压缩 API 身份。
    pub api_call_id: ApiCallId,
    /// Provider semantic seq that contains the exact compaction summary.
    /// 包含精确压缩总结的 Provider 语义 seq。
    pub occurred_at_seq: Seq,
    /// Inclusive first event retained by the new Provider window.
    /// 新 Provider 窗口包含的首个保留事件。
    pub retained_from_seq: Seq,
    /// Exact persisted compaction summary.
    /// 精确持久化的压缩总结。
    pub summary: String,
    /// Exact eight-kind locked baseline segments refreshed for the new generation.
    /// 为新代际刷新的精确八种锁定基线片段。
    pub locked_baseline: LockedBaselineProjection,
}

impl CompactionCommitInput {
    /// Builds one complete compaction commit input without classifying any source text.
    /// 构建一份完整压缩提交输入，且不对任何来源正文做分类猜测。
    pub fn new(
        api_call_id: ApiCallId,
        occurred_at_seq: Seq,
        retained_from_seq: Seq,
        summary: impl Into<String>,
        locked_baseline: LockedBaselineProjection,
    ) -> Self {
        Self {
            api_call_id,
            occurred_at_seq,
            retained_from_seq,
            summary: summary.into(),
            locked_baseline,
        }
    }
}

/// Validated input for one atomic Plan-mode transition.
/// 一次原子 Plan 模式转换的已校验输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanModeTransitionInput {
    /// Target Plan enablement.
    /// 目标 Plan 启用状态。
    enabled: bool,
    /// Exact provider-visible enter or exit semantic.
    /// 精确的 Provider 可见进入或退出语义。
    provider_context: String,
}

/// Complete metadata-only session selection mutation committed as one domain revision.
/// 作为一个领域修订提交的完整纯 metadata 会话选择修改。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSelectionBundleInput {
    /// Optional session-owned output-style catalog identity.
    /// 可选的会话自有输出风格目录身份。
    pub output_style: Option<String>,
    /// Complete session-owned Provider tool selection.
    /// 完整的会话自有 Provider 工具选择。
    pub provider_tools: ProviderToolSelectionConfig,
    /// Complete session-owned knowledge-library selection.
    /// 完整的会话自有知识库选择。
    pub knowledge_libraries: KnowledgeLibrarySelectionConfig,
    /// Complete tri-state VMM session policy.
    /// 完整的 VMM 会话三态策略。
    pub vmm: VmmSessionPolicyConfig,
}

impl PlanModeTransitionInput {
    /// Builds one Plan transition input with mandatory non-empty semantic content.
    /// 使用必填的非空语义内容构建 Plan 转换输入。
    pub fn try_new(
        enabled: bool,
        provider_context: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            enabled,
            provider_context: validate_system_context_content(provider_context.into())?,
        })
    }
}

impl ActiveApiState {
    /// Returns the seq of the last Provider-sourced event of the group, or
    /// `None` when the group produced no Provider-sourced semantics yet.
    /// 返回组内最后一个 Provider 来源事件的 seq；组尚未产生任何
    /// Provider 来源语义时返回 `None`。
    fn last_provider_sourced_seq(&self) -> Option<Seq> {
        if let Some(seq) = self.tool_seqs.last() {
            return Some(*seq);
        }
        if let Some(seq) = self.text_seq {
            return Some(seq);
        }
        self.reasoning_seq
    }
}

/// Owned, immutable flush candidate set captured at one revision.
/// 在某一个修订号捕获的自有、不可变 flush 候选集。
#[derive(Debug, Clone)]
pub struct FlushCandidate {
    /// Revision this candidate covers.
    /// 该候选覆盖的修订号。
    pub revision: u64,
    /// Files that must be re-published.
    /// 必须重新发布的文件。
    pub dirty: Vec<DirtyFile>,
    /// Exact event seq values changed since the last durable revision, sorted
    /// in ascending order for the logical live-patch channel.
    /// 自最近持久化修订以来真实变化的事件 seq，按升序排列并供逻辑实时补丁通道使用。
    pub changed_event_seqs: Vec<Seq>,
    /// Metadata document candidate.
    /// metadata 文档候选。
    pub metadata: MetadataDocument,
    /// Runtime lock candidate.
    /// runtime 锁定候选。
    pub runtime_lock: RuntimeLock,
    /// Compaction nodes candidate.
    /// 压缩节点候选。
    pub compactions: Vec<CompactionNode>,
    /// Active tail events only. Sealed segments are immutable and never
    /// rewritten by a flush, so the candidate never copies the full history
    /// (plan 5.2.15).
    /// 仅活动尾段事件。封口分片不可变、flush 永不重写，因此候选从不复制
    /// 完整历史(计划 5.2.15)。
    pub tail_events: Vec<ConversationEvent>,
}

/// Exact event patch produced by one in-memory document revision.
/// 一次内存文档修订产生的精确事件补丁。
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryRevisionReceipt {
    /// Exact in-memory revision owning this patch.
    /// 拥有该补丁的精确内存修订号。
    pub revision: u64,
    /// Changed event seq values in ascending order.
    /// 按升序排列的已变化事件 seq。
    pub changed_event_seqs: Vec<Seq>,
    /// Complete current snapshots of the changed events in the same order.
    /// 与 seq 同序的已变化事件完整当前快照。
    pub changed_events: Vec<ConversationEvent>,
}

/// The authoritative in-memory V1 session document.
/// V1 权威内存会话文档。
#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct SessionDocument {
    /// Lightweight metadata (mode, config, input state, split map).
    /// 轻量元数据(模式、配置、输入区状态、分片映射)。
    pub metadata: MetadataDocument,
    /// Current context-generation lock.
    /// 当前上下文代次锁定。
    pub runtime_lock: RuntimeLock,
    /// Compaction node history in occurrence order.
    /// 按发生顺序排列的压缩节点历史。
    pub compactions: Vec<CompactionNode>,
    /// Authoritative contiguous fact stream (`seq i` lives at index `i - 1`).
    /// 权威连续事实流(`seq i` 位于索引 `i - 1`)。
    events: Vec<ConversationEvent>,
    /// Derivable index: tool call id -> event seq.
    /// 可派生索引：工具调用 id -> 事件 seq。
    tool_seq_by_id: HashMap<ToolCallId, Seq>,
    /// Resident open-turn state.
    /// 常驻开放回合状态。
    active_turn: Option<ActiveTurnState>,
    /// Resident in-flight API state.
    /// 常驻在途 API 状态。
    active_api: Option<ActiveApiState>,
    /// Monotonic id and seq allocator.
    /// 单调 id 与 seq 分配器。
    id_allocator: IdAllocator,
    /// Revision advanced by every mutation.
    /// 每次 mutation 递增的修订号。
    memory_revision: u64,
    /// Revision confirmed durable by the writer.
    /// 由写入器确认持久化的修订号。
    durable_revision: u64,
    /// Per-file revision at which it became dirty.
    /// 每个文件变为 dirty 时的修订号。
    dirty_since: BTreeMap<DirtyFile, u64>,
    /// Exact event seq values changed by each in-memory revision.
    /// 每个内存修订真实变化的事件 seq。
    changed_event_seqs_by_revision: BTreeMap<u64, Vec<Seq>>,
    /// Revisions handed out by `prepare_flush_candidates` but not yet confirmed
    /// durable. `advance_durable` accepts only revisions found here (plan
    /// 6.2.1: the caller cannot forge a revision the writer never published).
    /// 由 `prepare_flush_candidates` 发出、但尚未确认持久化的修订号。
    /// `advance_durable` 只接受出现在这里的修订号(计划 6.2.1：调用方不能
    /// 伪造写入器从未发布的修订号)。
    issued_revisions: BTreeSet<u64>,
    /// The last compaction pointer the writer confirmed durable; nodes after
    /// it are unpublished candidates that the next record must trim, so a
    /// runtime write failure can never let the next compaction append on top
    /// of unpublished candidates (audit P0#7).
    /// 写入器确认持久化的最后一个压缩指针；其后的节点是未发布候选，下一次
    /// record 必须裁剪，因此 runtime 写失败后下一次压缩绝不可能基于未发布
    /// 候选继续追加(审计 P0#7)。
    published_compaction_id: Option<CompactionId>,
    /// Revision -> compaction pointer that becomes durable if that revision is
    /// confirmed; `advance_durable` uses it to advance the published pointer.
    /// 修订号 -> 该修订号被确认后将变为持久化的压缩指针；`advance_durable`
    /// 用它推进已发布指针。
    pending_compaction_publish: BTreeMap<u64, Option<CompactionId>>,
    /// Complete split map of the current generation, mirroring
    /// `metadata.split_list`. It makes every sealed segment logically
    /// addressable without requiring its content to be resident (plan 6.1.4);
    /// `events` carries the resident stream, which the tail-only loader path
    /// restricts to the active tail (audit P0#2).
    /// 当前 generation 的完整分片映射，镜像 `metadata.split_list`。它使每个
    /// 封口分片在逻辑上可寻址，而无须其内容常驻(计划 6.1.4)；`events` 携带
    /// 常驻流，tail-only 加载路径把它限制为活动尾段(审计 P0#2)。
    segment_catalog: Vec<SegmentInfo>,
    /// Absolute seq of the first event of the resident stream. The resident
    /// stream is 1-based in a fresh document and starts at the active tail's
    /// start seq in the tail-only loader path; every index math and the
    /// rebuild seq-contiguity check are relative to this value.
    /// 常驻流首个事件的绝对 seq。全新文档中为 1，tail-only 加载路径下为
    /// 活动尾段的起始 seq；所有索引计算与 rebuild 的 seq 连续性校验都以该值
    /// 为基准。
    resident_start_seq: u64,
}

impl SessionDocument {
    /// Creates a fresh empty session document without any disk state.
    /// 创建一个不含任何磁盘状态的全新空会话文档。
    pub fn new(
        session_id: impl Into<String>,
        project_tag: impl Into<String>,
        title: impl Into<String>,
        created_at_ms: i64,
    ) -> Self {
        let metadata = MetadataDocument::new(session_id, project_tag, title, created_at_ms);
        Self {
            metadata,
            runtime_lock: RuntimeLock::default(),
            compactions: Vec::new(),
            events: Vec::new(),
            tool_seq_by_id: HashMap::new(),
            active_turn: None,
            active_api: None,
            id_allocator: IdAllocator::new(),
            memory_revision: 0,
            durable_revision: 0,
            dirty_since: BTreeMap::new(),
            changed_event_seqs_by_revision: BTreeMap::new(),
            issued_revisions: BTreeSet::new(),
            published_compaction_id: None,
            pending_compaction_publish: BTreeMap::new(),
            segment_catalog: Vec::new(),
            resident_start_seq: 1,
        }
    }

    /// Returns the authoritative contiguous event stream (the resident stream:
    /// the active tail in the tail-only loader path).
    /// 返回权威连续事件流(常驻流：tail-only 加载路径下即活动尾段)。
    pub fn events(&self) -> &[ConversationEvent] {
        &self.events
    }

    /// Returns the complete split map that makes sealed segments addressable.
    /// 返回使封口分片可寻址的完整分片映射。
    pub fn segment_catalog(&self) -> &[SegmentInfo] {
        &self.segment_catalog
    }

    /// Returns the persisted event-tree generation that owns every resident
    /// event and catalog entry.
    /// 返回拥有全部驻留事件与目录项的持久化事件树代次。
    ///
    /// # Returns
    /// 返回值
    /// Returns the monotonic segment generation published in `metadata.json`.
    /// 返回发布在 `metadata.json` 中的单调分片代次。
    pub fn segment_generation(&self) -> u64 {
        self.metadata.segment_generation
    }

    /// Applies only the physical storage pointers published by one successful
    /// flush while preserving newer logical metadata already mutated in memory.
    /// 仅应用一次成功 flush 发布的物理存储指针，同时保留内存中已经产生的更新
    /// 逻辑 metadata。
    ///
    /// # Parameters
    /// # 参数
    /// `published` is the exact metadata document atomically committed by the
    /// storage writer.
    /// `published` 是存储写入器已经原子提交的精确 metadata 文档。
    ///
    /// # Returns
    /// # 返回值
    /// `Ok(())` when the pointer advance is monotonic and belongs to this
    /// session; otherwise a structured domain error.
    /// 当指针推进单调且属于当前会话时返回 `Ok(())`；否则返回结构化领域错误。
    pub fn apply_published_storage_metadata(
        &mut self,
        published: &MetadataDocument,
    ) -> Result<(), DomainError> {
        if published.session_id != self.metadata.session_id {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "metadata",
                field: "session_id",
            });
        }
        if published.segment_generation < self.metadata.segment_generation {
            return Err(DomainError::SequenceViolation {
                expected: self.metadata.segment_generation,
                actual: published.segment_generation,
            });
        }
        if published.active_context_generation < self.metadata.active_context_generation {
            return Err(DomainError::SequenceViolation {
                expected: self.metadata.active_context_generation,
                actual: published.active_context_generation,
            });
        }
        self.metadata.segment_generation = published.segment_generation;
        self.metadata.active_context_generation = published.active_context_generation;
        self.metadata.split_list = published.split_list.clone();
        self.segment_catalog = published.split_list.clone();
        Ok(())
    }

    /// Returns the resident open-turn state.
    /// 返回常驻开放回合状态。
    pub fn active_turn(&self) -> Option<&ActiveTurnState> {
        self.active_turn.as_ref()
    }

    /// Returns the resident in-flight API state.
    /// 返回常驻在途 API 状态。
    pub fn active_api(&self) -> Option<&ActiveApiState> {
        self.active_api.as_ref()
    }

    /// Returns the current memory revision.
    /// 返回当前内存修订号。
    pub fn memory_revision(&self) -> u64 {
        self.memory_revision
    }

    /// Renames the session in the authoritative metadata document.
    /// 在权威 metadata 文档中重命名会话。
    ///
    /// # Parameters
    /// 参数
    /// `title` is the exact user-visible title and `now_ms` is the semantic
    /// mutation time. Empty titles and negative timestamps are rejected before
    /// any state changes.
    /// `title` 是精确的用户可见标题，`now_ms` 是语义修改时间；空标题与负时间戳
    /// 会在任何状态变化前被拒绝。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when metadata changed.
    /// 仅在 metadata 真实变化时返回 `true`。
    pub fn rename_session(
        &mut self,
        title: impl Into<String>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(DomainError::InvalidSessionTitle);
        }
        validate_timestamp("updated_at_ms", now_ms)?;
        if self.metadata.title == title {
            return Ok(false);
        }
        self.metadata.title = title;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Updates the authoritative archive timestamp without touching the event stream.
    /// 更新权威归档时间戳且不触碰事件流。
    ///
    /// # Parameters
    /// 参数
    /// `archived_at_ms` is `Some` when archiving and `None` when restoring;
    /// `now_ms` is the semantic mutation time.
    /// `archived_at_ms` 在归档时为 `Some`、恢复时为 `None`；`now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when the archive state changed.
    /// 仅在归档状态真实变化时返回 `true`。
    pub fn set_archived_at(
        &mut self,
        archived_at_ms: Option<i64>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if let Some(archived_at_ms) = archived_at_ms {
            validate_timestamp("archived_at_ms", archived_at_ms)?;
            if archived_at_ms < self.metadata.created_at_ms {
                return Err(DomainError::InvalidTimestamp {
                    field: "archived_at_ms",
                    value: archived_at_ms,
                });
            }
        }
        if self.metadata.archived_at_ms == archived_at_ms {
            return Ok(false);
        }
        self.metadata.archived_at_ms = archived_at_ms;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Builds one document from already-loaded parts and strictly rebuilds its
    /// derived state (loader entry point).
    /// 从已加载的部件构建一个文档，并严格重建其派生状态(加载器入口)。
    pub fn from_parts(
        metadata: MetadataDocument,
        runtime_lock: RuntimeLock,
        compactions: Vec<CompactionNode>,
        segment_catalog: Vec<SegmentInfo>,
        events: Vec<ConversationEvent>,
    ) -> Result<Self, DomainError> {
        let published_compaction_id = runtime_lock.current_compaction_id;
        let persisted_revision = metadata.document_revision;
        let resident_start_seq = events
            .first()
            .map(|event| event.seq.value())
            .unwrap_or_else(|| {
                segment_catalog
                    .last()
                    .map(|entry| entry.start_seq.value())
                    .unwrap_or(1)
            });
        let mut document = Self {
            metadata,
            runtime_lock,
            compactions,
            events,
            tool_seq_by_id: HashMap::new(),
            active_turn: None,
            active_api: None,
            id_allocator: IdAllocator::new(),
            memory_revision: persisted_revision,
            durable_revision: persisted_revision,
            dirty_since: BTreeMap::new(),
            changed_event_seqs_by_revision: BTreeMap::new(),
            issued_revisions: BTreeSet::new(),
            published_compaction_id,
            pending_compaction_publish: BTreeMap::new(),
            segment_catalog,
            resident_start_seq,
        };
        document.rebuild_derived_state()?;
        document.validate_runtime_initialization_state()?;
        document.validate_compaction_state()?;
        document.validate_loaded_metadata()?;
        Ok(document)
    }

    /// Returns the last revision confirmed durable.
    /// 返回最近一次被确认持久化的修订号。
    pub fn durable_revision(&self) -> u64 {
        self.durable_revision
    }

    /// Captures the exact event patch recorded for one in-memory revision
    /// without issuing a flush candidate or changing durability state.
    /// 捕获一个内存修订记录的精确事件补丁，且不发出 flush 候选或改变持久化状态。
    ///
    /// # Parameters
    /// 参数
    /// `revision` is the exact revision returned immediately after a successful
    /// domain mutation.
    /// `revision` 是一次成功领域修改后立即返回的精确修订号。
    ///
    /// # Returns
    /// 返回值
    /// Returns the sorted changed seq values and their complete current event
    /// snapshots. Metadata-only revisions return an empty patch.
    /// 返回排序后的变化 seq 及对应完整当前事件快照；仅 metadata 修订返回空补丁。
    pub fn memory_revision_receipt(
        &self,
        revision: u64,
    ) -> Result<MemoryRevisionReceipt, DomainError> {
        let changed_event_seqs = self
            .changed_event_seqs_by_revision
            .get(&revision)
            .cloned()
            .ok_or(DomainError::ReferenceViolation {
                kind: "memory_revision_receipt",
                value: revision,
            })?;
        let changed_events = self.clone_resident_events_at_seqs(&changed_event_seqs)?;
        Ok(MemoryRevisionReceipt {
            revision,
            changed_event_seqs,
            changed_events,
        })
    }

    /// Returns the current dirty file set.
    /// 返回当前 dirty 文件集合。
    pub fn dirty_files(&self) -> impl Iterator<Item = DirtyFile> + '_ {
        self.dirty_since.keys().copied()
    }

    /// Returns the allocator frontier cache for persistence.
    /// 返回供持久化使用的分配器 frontier 缓存。
    pub fn id_frontier(&self) -> IdFrontier {
        let (next_local_id, next_seq) = self.id_allocator.frontier();
        IdFrontier {
            next_local_id,
            next_seq,
        }
    }

    /// Confirms that one candidate revision became durable and clears the
    /// dirty flags fully covered by that revision.
    /// 确认某个候选修订已持久化，并清除该修订完全覆盖的 dirty 标记。
    ///
    /// Only revisions actually issued by [`Self::prepare_flush_candidates`]
    /// are accepted; a future or forged revision is rejected (plan 6.2.1).
    /// A revision already covered by a later confirmation is a no-op, keeping
    /// out-of-order confirmations from falsely failing. Files dirtied after
    /// the candidate was copied keep their dirty flag.
    /// 只接受 [`Self::prepare_flush_candidates`] 实际发出的修订号；未来或被
    /// 伪造的修订号被拒绝(计划 6.2.1)。已被更晚确认覆盖的修订号是无操作，
    /// 避免乱序确认误报失败。候选复制之后再次变脏的文件保持 dirty 标记。
    pub fn advance_durable(&mut self, revision: u64) -> Result<(), DomainError> {
        if revision > self.memory_revision {
            return Err(DomainError::RevisionNeverIssued {
                revision,
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        if revision <= self.durable_revision {
            return Ok(());
        }
        if !self.issued_revisions.contains(&revision) {
            return Err(DomainError::RevisionNeverIssued {
                revision,
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        let covered: Vec<u64> = self.issued_revisions.range(..=revision).copied().collect();
        for issued in covered {
            self.issued_revisions.remove(&issued);
        }
        // Advance the published compaction pointer to the newest confirmed
        // snapshot at or before this revision.
        // 把已发布压缩指针推进到该修订号及之前最新的已确认快照。
        let confirmed: Vec<(u64, Option<CompactionId>)> = self
            .pending_compaction_publish
            .range(..=revision)
            .map(|(rev, pointer)| (*rev, *pointer))
            .collect();
        for (rev, _) in &confirmed {
            self.pending_compaction_publish.remove(rev);
        }
        if let Some((_, pointer)) = confirmed.last() {
            self.published_compaction_id = *pointer;
        }
        self.durable_revision = revision;
        let stale: Vec<DirtyFile> = self
            .dirty_since
            .iter()
            .filter(|(_, since)| **since <= revision)
            .map(|(file, _)| *file)
            .collect();
        for file in stale {
            self.dirty_since.remove(&file);
        }
        let durable_changes: Vec<u64> = self
            .changed_event_seqs_by_revision
            .range(..=revision)
            .map(|(changed_revision, _)| *changed_revision)
            .collect();
        for changed_revision in durable_changes {
            self.changed_event_seqs_by_revision
                .remove(&changed_revision);
        }
        Ok(())
    }

    /// Submits a root user prompt as a fresh turn.
    /// 以全新回合提交一条根用户提示词。
    pub fn submit_root_input(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<TurnId>, DomainError> {
        let ClientInputSubmission {
            client_action_id,
            text,
            resource_ids,
        } = submission;
        if let Some(existing_turn_id) =
            self.existing_root_submission(&client_action_id, &text, &resource_ids)?
        {
            return Ok(IdempotentMutation::Idempotent(existing_turn_id));
        }
        self.require_client_action_unused(&client_action_id)?;
        let text = validate_user_input(text, &resource_ids)?;
        if let Some(turn) = &self.active_turn
            && !turn.closed
        {
            return Err(DomainError::RootInputWhileTurnOpen {
                turn_id: turn.turn_id.value(),
            });
        }
        self.ensure_transition_capacity(1, 2)?;
        let mut staged_allocator = self.id_allocator;
        let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
        let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_events = Vec::with_capacity(1);
        let mut staged_metadata = self.metadata.clone();
        consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                client_action_id,
                input_id,
                input_source: UserInputSource::Root,
                text,
                resource_ids,
            }),
            now_ms,
        )?;
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.active_turn = Some(ActiveTurnState {
            turn_id,
            origin: TurnOrigin::UserRoot,
            start_seq: seq,
            closed: false,
        });
        self.metadata.open_turn_start_seq = Some(seq);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[seq],
        );
        Ok(IdempotentMutation::Committed(turn_id))
    }

    /// Queues one future-turn input into metadata without touching the stream.
    /// 把一条未来回合输入写入 metadata 队列，不触碰主事件流。
    pub fn enqueue_input(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<EnqueueInputReceipt>, DomainError> {
        let ClientInputSubmission {
            client_action_id,
            text,
            resource_ids,
        } = submission;
        if let Some(existing_receipt) =
            self.existing_enqueue_receipt(&client_action_id, &text, &resource_ids)?
        {
            return Ok(IdempotentMutation::Idempotent(existing_receipt));
        }
        self.require_client_action_unused(&client_action_id)?;
        let text = validate_user_input(text, &resource_ids)?;
        validate_timestamp("updated_at_ms", now_ms)?;

        let undispatched_goal_turn = self.active_turn.as_ref().filter(|turn| {
            !turn.closed
                && turn.origin == TurnOrigin::GoalContinuation
                && self.active_api.is_none()
                && !self.events.iter().any(|event| {
                    event.turn_id == Some(turn.turn_id)
                        && matches!(event.event, EventBody::ApiStarted(_))
                })
        });
        if let Some(goal_turn) = undispatched_goal_turn.cloned() {
            if !self.metadata.input_state.queued_inputs.is_empty() {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "undispatched_goal_existing_queue",
                });
            }
            let buckets = self.aggregate_turn_buckets(goal_turn.turn_id, goal_turn.start_seq)?;
            self.ensure_transition_capacity(3, 2)?;
            let mut staged_allocator = self.id_allocator;
            let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
            let scheduled_turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            let mut staged_events = Vec::with_capacity(3);
            let mut staged_metadata = self.metadata.clone();
            consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
            let stopped_turn_stats_seq = self.stage_turn_closure(
                &mut staged_allocator,
                &mut staged_events,
                goal_turn.turn_id,
                goal_turn.start_seq,
                TurnOutcome::Stopped,
                0,
                now_ms,
                buckets,
            )?;
            let input_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(scheduled_turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    client_action_id: client_action_id.clone(),
                    input_id,
                    input_source: UserInputSource::QueuedScheduled,
                    text,
                    resource_ids,
                }),
                now_ms,
            )?;
            let changed_event_seqs = staged_events
                .iter()
                .map(ConversationEvent::seq)
                .collect::<Vec<_>>();
            self.id_allocator = staged_allocator;
            self.metadata = staged_metadata;
            self.events.extend(staged_events);
            self.active_turn = Some(ActiveTurnState {
                turn_id: scheduled_turn_id,
                origin: TurnOrigin::UserRoot,
                start_seq: input_seq,
                closed: false,
            });
            self.metadata.open_turn_start_seq = Some(input_seq);
            self.bump_revision(
                &[DirtyFile::Metadata, DirtyFile::ActiveTail],
                now_ms,
                &changed_event_seqs,
            );
            return Ok(IdempotentMutation::Committed(EnqueueInputReceipt {
                client_action_id,
                input_id,
                disposition: EnqueueInputDisposition::SupersededGoalAndScheduled {
                    stopped_turn_id: goal_turn.turn_id,
                    stopped_turn_stats_seq,
                    scheduled_turn_id,
                    input_seq,
                },
            }));
        }

        self.ensure_transition_capacity(0, 1)?;
        let mut staged_allocator = self.id_allocator;
        let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_metadata = self.metadata.clone();
        consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
        staged_metadata.input_state.queued_inputs.push(QueuedInput {
            client_action_id: client_action_id.clone(),
            input_id,
            text,
            resource_ids,
            created_at_ms: now_ms,
        });
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(EnqueueInputReceipt {
            client_action_id,
            input_id,
            disposition: EnqueueInputDisposition::Queued,
        }))
    }

    /// Replaces the durable composer draft with exact text and ordered resources.
    /// 使用精确正文与有序资源替换持久化输入区草稿。
    ///
    /// # Parameters
    /// 参数
    /// `text` and `resource_ids` are preserved exactly; `now_ms` is the explicit mutation boundary.
    /// `text` 与 `resource_ids` 被原样保留；`now_ms` 是显式修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or already-current draft without manufacturing a revision for a no-op.
    /// 返回已提交或已经是当前值的草稿；no-op 不制造修订号。
    pub fn set_draft(
        &mut self,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<DraftMutationReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let text = text.into();
        if self.metadata.input_state.draft.text == text
            && self.metadata.input_state.draft.resource_ids == resource_ids
        {
            return Ok(IdempotentMutation::Idempotent(DraftMutationReceipt {
                draft: self.metadata.input_state.draft.clone(),
            }));
        }
        let draft = DraftState {
            text,
            resource_ids,
            restored_from_queue: None,
        };
        self.metadata.input_state.draft = draft.clone();
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(DraftMutationReceipt {
            draft,
        }))
    }

    /// Clears the durable composer draft without touching queued inputs.
    /// 清空持久化输入区草稿，且不修改排队输入。
    ///
    /// # Parameters
    /// 参数
    /// `now_ms` is the explicit mutation boundary.
    /// `now_ms` 是显式修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or already-empty draft state.
    /// 返回已提交或已经为空的草稿状态。
    pub fn clear_draft(
        &mut self,
        now_ms: i64,
    ) -> Result<IdempotentMutation<DraftMutationReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let draft = DraftState::default();
        if self.metadata.input_state.draft == draft {
            return Ok(IdempotentMutation::Idempotent(DraftMutationReceipt {
                draft,
            }));
        }
        self.metadata.input_state.draft = draft.clone();
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(DraftMutationReceipt {
            draft,
        }))
    }

    /// Atomically moves one exact queued input into the durable composer draft.
    /// 原子地把一条精确排队输入移动到持久化输入区草稿。
    ///
    /// # Parameters
    /// 参数
    /// `identity` must match both the external action and local input id; `now_ms` is the mutation boundary.
    /// `identity` 必须同时匹配外部动作与本地输入 id；`now_ms` 是修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact restored draft receipt, including an idempotent retry after reload.
    /// 返回精确恢复草稿 receipt，包括重载后的幂等重试。
    pub fn move_queued_input_to_draft(
        &mut self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<IdempotentMutation<QueuedInputDraftReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if let Some(resolution) = self.existing_queued_input_resolution(&identity)? {
            if resolution.resolution != QueuedInputResolutionKind::MovedToDraft {
                return Err(client_action_conflict(&identity.client_action_id));
            }
            return Ok(IdempotentMutation::Idempotent(QueuedInputDraftReceipt {
                identity,
                draft: DraftState {
                    text: resolution.text.clone(),
                    resource_ids: resolution.resource_ids.clone(),
                    restored_from_queue: Some(QueuedInputIdentity {
                        client_action_id: resolution.client_action_id.clone(),
                        input_id: resolution.input_id,
                    }),
                },
            }));
        }
        let index = self.require_queued_input_index(&identity)?;
        let queued = self.metadata.input_state.queued_inputs[index].clone();
        let draft = DraftState {
            text: queued.text.clone(),
            resource_ids: queued.resource_ids.clone(),
            restored_from_queue: Some(identity.clone()),
        };
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let resolution_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            None,
            EventVisibility::UiOnly,
            EventBody::QueuedInputResolution(QueuedInputResolutionEvent {
                client_action_id: queued.client_action_id,
                input_id: queued.input_id,
                resolution: QueuedInputResolutionKind::MovedToDraft,
                text: queued.text,
                resource_ids: queued.resource_ids,
            }),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.input_state.queued_inputs.remove(index);
        staged_metadata.input_state.draft = draft.clone();
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[resolution_seq],
        );
        Ok(IdempotentMutation::Committed(QueuedInputDraftReceipt {
            identity,
            draft,
        }))
    }

    /// Atomically removes one exact queued input from authoritative metadata.
    /// 从权威元数据中原子移除一条精确排队输入。
    ///
    /// # Parameters
    /// 参数
    /// `identity` must match both durable ids and `now_ms` is the mutation boundary.
    /// `identity` 必须同时匹配两个持久化 id；`now_ms` 是修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or previously persisted removal receipt.
    /// 返回已提交或此前已持久化的移除 receipt。
    pub fn remove_queued_input(
        &mut self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<IdempotentMutation<QueuedInputRemovalReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if let Some(resolution) = self.existing_queued_input_resolution(&identity)? {
            if resolution.resolution != QueuedInputResolutionKind::Removed {
                return Err(client_action_conflict(&identity.client_action_id));
            }
            return Ok(IdempotentMutation::Idempotent(QueuedInputRemovalReceipt {
                identity,
            }));
        }
        let index = self.require_queued_input_index(&identity)?;
        let queued = self.metadata.input_state.queued_inputs[index].clone();
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let resolution_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            None,
            EventVisibility::UiOnly,
            EventBody::QueuedInputResolution(QueuedInputResolutionEvent {
                client_action_id: queued.client_action_id,
                input_id: queued.input_id,
                resolution: QueuedInputResolutionKind::Removed,
                text: queued.text,
                resource_ids: queued.resource_ids,
            }),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.input_state.queued_inputs.remove(index);
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[resolution_seq],
        );
        Ok(IdempotentMutation::Committed(QueuedInputRemovalReceipt {
            identity,
        }))
    }

    /// Atomically transfers one queued future input into the currently open logical Turn.
    /// 原子地把一条未来排队输入转移进当前开放逻辑回合。
    ///
    /// # Parameters
    /// 参数
    /// `identity` must match the durable queue item exactly; `now_ms` is the mutation boundary.
    /// `identity` 必须精确匹配持久化队列条目；`now_ms` 是修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or previously persisted transfer receipt without allocating a new input id.
    /// 返回已提交或此前已持久化的转移 receipt，且不会分配新的输入 id。
    pub fn promote_queued_input_to_pending_turn(
        &mut self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<IdempotentMutation<QueuedInputPendingTurnReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if let Some(resolution) = self.existing_queued_input_resolution(&identity)? {
            if resolution.resolution != QueuedInputResolutionKind::PromotedToPendingInTurn {
                return Err(client_action_conflict(&identity.client_action_id));
            }
            let resolution_event = self
                .events
                .iter()
                .find(|event| {
                    matches!(
                        &event.event,
                        EventBody::QueuedInputResolution(candidate)
                            if candidate.client_action_id == identity.client_action_id
                                && candidate.input_id == identity.input_id
                    )
                })
                .ok_or(DomainError::InvalidQueuedInputOrder {
                    field: "promoted_resolution_event_missing",
                })?;
            let turn_id = resolution_event
                .turn_id
                .ok_or(DomainError::InvalidQueuedInputOrder {
                    field: "promoted_resolution_turn_id",
                })?;
            return Ok(IdempotentMutation::Idempotent(
                QueuedInputPendingTurnReceipt {
                    identity,
                    turn_id,
                    resolution_seq: resolution_event.seq,
                },
            ));
        }
        let turn_id = self.require_open_turn()?.turn_id;
        let index = self.require_queued_input_index(&identity)?;
        let queued = self.metadata.input_state.queued_inputs[index].clone();
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let resolution_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::UiOnly,
            EventBody::QueuedInputResolution(QueuedInputResolutionEvent {
                client_action_id: queued.client_action_id.clone(),
                input_id: queued.input_id,
                resolution: QueuedInputResolutionKind::PromotedToPendingInTurn,
                text: queued.text.clone(),
                resource_ids: queued.resource_ids.clone(),
            }),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.input_state.queued_inputs.remove(index);
        staged_metadata
            .input_state
            .pending_in_turn_inputs
            .push(PendingInTurnInput {
                client_action_id: queued.client_action_id,
                input_id: queued.input_id,
                turn_id,
                text: queued.text,
                resource_ids: queued.resource_ids,
                created_at_ms: now_ms,
            });
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[resolution_seq],
        );
        Ok(IdempotentMutation::Committed(
            QueuedInputPendingTurnReceipt {
                identity,
                turn_id,
                resolution_seq,
            },
        ))
    }

    /// Resolves the exact durable queue identity owned by one client action.
    /// 解析一个客户端动作拥有的精确持久化队列身份。
    ///
    /// # Parameters
    /// 参数
    /// `client_action_id` is the browser idempotency identity stored by queue admission.
    /// `client_action_id` 是排队准入保存的浏览器幂等身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the current or terminal queue identity, or `None` when the action never owned a
    /// queue item. Conflicting durable identities are rejected instead of guessed.
    /// 返回当前或已终结的队列身份；若该动作从未拥有队列条目则返回 `None`。持久化身份冲突会被
    /// 显式拒绝，而不是猜测。
    pub fn queued_input_identity_for_action(
        &self,
        client_action_id: &ClientActionId,
    ) -> Result<Option<QueuedInputIdentity>, DomainError> {
        let mut resolved = None;
        for queued in &self.metadata.input_state.queued_inputs {
            if queued.client_action_id == *client_action_id {
                merge_queued_input_identity(
                    &mut resolved,
                    QueuedInputIdentity {
                        client_action_id: queued.client_action_id.clone(),
                        input_id: queued.input_id,
                    },
                )?;
            }
        }
        for event in &self.events {
            let EventBody::QueuedInputResolution(resolution) = &event.event else {
                continue;
            };
            if resolution.client_action_id == *client_action_id {
                merge_queued_input_identity(
                    &mut resolved,
                    QueuedInputIdentity {
                        client_action_id: resolution.client_action_id.clone(),
                        input_id: resolution.input_id,
                    },
                )?;
            }
        }
        if let Some(identity) = self
            .metadata
            .input_state
            .draft
            .restored_from_queue
            .as_ref()
            .filter(|identity| identity.client_action_id == *client_action_id)
        {
            merge_queued_input_identity(&mut resolved, identity.clone())?;
        }
        Ok(resolved)
    }

    /// Replaces the FIFO queue order using one exact complete identity list.
    /// 使用一份精确完整身份列表替换 FIFO 队列顺序。
    ///
    /// # Parameters
    /// 参数
    /// `ordered` must contain every current item exactly once; `now_ms` is the mutation boundary.
    /// `ordered` 必须恰好包含每个当前条目一次；`now_ms` 是修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or already-current complete order.
    /// 返回已提交或已经是当前值的完整顺序。
    pub fn reorder_queued_inputs(
        &mut self,
        ordered: Vec<QueuedInputIdentity>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<QueuedInputOrderReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if ordered.len() != self.metadata.input_state.queued_inputs.len() {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "member_count",
            });
        }
        let current_order = self
            .metadata
            .input_state
            .queued_inputs
            .iter()
            .map(queued_input_identity)
            .collect::<Vec<_>>();
        if current_order == ordered {
            return Ok(IdempotentMutation::Idempotent(QueuedInputOrderReceipt {
                ordered,
            }));
        }
        let mut by_identity = self
            .metadata
            .input_state
            .queued_inputs
            .iter()
            .cloned()
            .map(|queued| (queued_input_identity(&queued), queued))
            .collect::<BTreeMap<_, _>>();
        if by_identity.len() != self.metadata.input_state.queued_inputs.len() {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "duplicate_authoritative_identity",
            });
        }
        let mut reordered = Vec::with_capacity(ordered.len());
        let mut requested = BTreeSet::new();
        for identity in &ordered {
            if !requested.insert(identity.clone()) {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "duplicate_requested_identity",
                });
            }
            let queued =
                by_identity
                    .remove(identity)
                    .ok_or(DomainError::InvalidQueuedInputOrder {
                        field: "unknown_requested_identity",
                    })?;
            reordered.push(queued);
        }
        if !by_identity.is_empty() {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "missing_requested_identity",
            });
        }
        self.metadata.input_state.queued_inputs = reordered;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(QueuedInputOrderReceipt {
            ordered,
        }))
    }

    /// Records one in-turn supplementary input bound to the open turn.
    /// 记录一条绑定开放回合的过程内补充输入。
    pub fn add_pending_in_turn_input(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<PendingInputId>, DomainError> {
        let ClientInputSubmission {
            client_action_id,
            text,
            resource_ids,
        } = submission;
        if let Some(existing_input_id) =
            self.existing_pending_submission(&client_action_id, &text, &resource_ids)?
        {
            return Ok(IdempotentMutation::Idempotent(existing_input_id));
        }
        self.require_client_action_unused(&client_action_id)?;
        let text = validate_user_input(text, &resource_ids)?;
        let turn_id = self.require_open_turn()?.turn_id;
        self.ensure_transition_capacity(0, 1)?;
        let mut staged_allocator = self.id_allocator;
        let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_metadata = self.metadata.clone();
        consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
        staged_metadata
            .input_state
            .pending_in_turn_inputs
            .push(PendingInTurnInput {
                client_action_id,
                input_id,
                turn_id,
                text,
                resource_ids,
                created_at_ms: now_ms,
            });
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(input_id))
    }

    /// Starts one physical Provider API group inside the open turn.
    /// 在开放回合内启动一次物理 Provider API 组。
    pub fn prepare_and_start_api(
        &mut self,
        input: PrepareAndStartApiInput,
    ) -> Result<IdempotentMutation<PreparedApiStart>, DomainError> {
        let PrepareAndStartApiInput {
            purpose,
            request,
            projection,
            request_budget_evidence,
            locked_baseline,
            continuation_action_id,
            now_ms,
        } = input;
        validate_api_continuation_binding_shape(purpose, continuation_action_id.as_ref())?;
        if let Some(action_id) = continuation_action_id.as_ref()
            && let Some(receipt) = self.existing_api_start_for_action(action_id)?
        {
            return Ok(IdempotentMutation::Idempotent(receipt));
        }
        validate_timestamp("api_started.now_ms", now_ms)?;
        validate_api_request_snapshot(&request)?;
        validate_provider_projection_proof(self, &projection)?;
        validate_provider_request_budget_evidence(&request_budget_evidence)?;
        if self.metadata.config.pending_request.is_some() {
            return Err(DomainError::InvalidRequestConfiguration {
                field: "pending_request_not_promoted",
            });
        }
        let requested_active = active_request_from_snapshot(&request);
        let active_is_empty = self.metadata.config.active_request == ActiveRequestConfig::default();
        if !active_is_empty {
            validate_request_against_active_config(
                &self.metadata.config.active_request,
                &requested_active,
            )?;
        } else if self.runtime_lock.initialization_state == RuntimeInitializationState::Initialized
        {
            return Err(DomainError::InvalidRequestConfiguration {
                field: "active_request_uninitialized",
            });
        }
        let turn_id = self.require_open_turn()?.turn_id;
        if let Some(action_id) = continuation_action_id.as_ref() {
            self.validate_continuation_action_for_api(action_id, turn_id)?;
        }
        if self.active_api.is_some() {
            return Err(DomainError::NoActiveApi);
        }
        self.ensure_transition_capacity(1, 1)?;

        let mut staged_runtime_lock = self.runtime_lock.clone();
        let locked_sources = locked_baseline.into_locked_sources();
        let runtime_changed = match staged_runtime_lock.initialization_state {
            RuntimeInitializationState::FreshUninitialized
            | RuntimeInitializationState::BaselineRefreshPending => {
                staged_runtime_lock.initialization_state = RuntimeInitializationState::Initialized;
                staged_runtime_lock.locked_sources = locked_sources;
                true
            }
            RuntimeInitializationState::Initialized => {
                if staged_runtime_lock.locked_sources != locked_sources {
                    return Err(DomainError::RuntimeBaselineAlreadyInitialized);
                }
                false
            }
        };
        let mut staged_metadata = self.metadata.clone();
        if active_is_empty {
            staged_metadata.config.active_request = requested_active;
        }

        let mut staged_allocator = self.id_allocator;
        let api_call_id = ApiCallId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_events = Vec::with_capacity(1);
        let started_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::HostOnly,
            EventBody::ApiStarted(ApiStartedEvent {
                api_call_id,
                purpose,
                request: request.clone(),
                projection: projection.clone(),
                request_budget_evidence,
                continuation_action_id: continuation_action_id.clone(),
            }),
            now_ms,
        )?;
        self.runtime_lock = staged_runtime_lock;
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.active_api = Some(ActiveApiState {
            api_call_id,
            turn_id,
            purpose,
            started_seq,
            request,
            projection: projection.clone(),
            reasoning_seq: None,
            text_seq: None,
            tool_seqs: Vec::new(),
            stats_seq: None,
            provider_source_placed: false,
        });
        let dirty = if runtime_changed {
            vec![DirtyFile::ActiveTail, DirtyFile::Runtime]
        } else {
            vec![DirtyFile::ActiveTail]
        };
        self.bump_revision(&dirty, now_ms, &[started_seq]);
        Ok(IdempotentMutation::Committed(PreparedApiStart {
            api_call_id,
            started_seq,
            projection,
            continuation_action_id,
        }))
    }

    /// Starts one API with an explicit deterministic projection proof for unit fixtures only.
    /// 仅供单元夹具使用显式确定性投影证明启动一次 API。
    #[cfg(test)]
    pub(crate) fn start_api(
        &mut self,
        purpose: ApiPurposeKind,
        request: ApiRequestSnapshot,
        now_ms: i64,
    ) -> Result<ApiCallId, DomainError> {
        let continuation_action_id = if purpose == ApiPurposeKind::Continue {
            self.events
                .iter()
                .rev()
                .find_map(|event| match &event.event {
                    EventBody::ExecutionControl(control)
                        if control.control_kind == ExecutionControlKind::TurnResumed =>
                    {
                        control.client_action_id.clone()
                    }
                    _ => None,
                })
        } else {
            None
        };
        let retained_from_seq = match self.runtime_lock.current_compaction_id {
            Some(compaction_id) => self
                .compactions
                .iter()
                .find(|node| node.compaction_id == compaction_id)
                .map(|node| node.retained_from_seq)
                .ok_or_else(|| {
                    DomainError::reference("current_compaction", compaction_id.value())
                })?,
            None => Seq::try_new(1)?,
        };
        let prepared = self.prepare_and_start_api(PrepareAndStartApiInput {
            purpose,
            request,
            request_budget_evidence: test_provider_request_budget_evidence(),
            projection: ProviderProjectionProof {
                segment_generation: self.metadata.segment_generation,
                document_revision: self.memory_revision,
                retained_from_seq,
                through_seq: self.last_seq()?,
                fingerprint_sha256: "0".repeat(64),
            },
            locked_baseline: LockedBaselineProjection::from_locked_sources(
                self.runtime_lock.locked_sources.clone(),
            )?,
            continuation_action_id,
            now_ms,
        })?;
        Ok(prepared.into_value().api_call_id)
    }

    /// Records that transport accepted one immutable physical API request.
    /// 记录 transport 已接受一份不可变物理 API 请求。
    ///
    /// # Parameters / 参数
    /// `api_call_id` identifies the already durable ApiStarted fact and
    /// `now_ms` is the explicit observation boundary time.
    /// `api_call_id` 标识已经持久化的 ApiStarted 事实，`now_ms` 是显式观察边界时间。
    ///
    /// # Returns / 返回值
    /// Returns a committed receipt or the original idempotent receipt.
    /// 返回已提交 receipt 或原始幂等 receipt。
    pub fn mark_api_transport_observed(
        &mut self,
        api_call_id: ApiCallId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<ApiTransportObservedReceipt>, DomainError> {
        let started_event = self
            .events
            .iter()
            .find(|event| {
                matches!(
                    &event.event,
                    EventBody::ApiStarted(started) if started.api_call_id == api_call_id
                )
            })
            .ok_or_else(|| DomainError::reference("api_started", api_call_id.value()))?;
        let EventBody::ApiStarted(started) = &started_event.event else {
            unreachable!("ApiStarted selected above")
        };
        let started_seq = started_event.seq;
        let started_turn_id = started_event.turn_id;
        let continuation_action_id = started.continuation_action_id.clone();
        let mut existing_observed_seq = None;
        for event in &self.events {
            if let EventBody::ExecutionControl(control) = &event.event
                && control.control_kind == ExecutionControlKind::ApiTransportObserved
                && control.api_call_id == Some(api_call_id)
                && existing_observed_seq.replace(event.seq).is_some()
            {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_api_transport_observed",
                });
            }
        }
        let receipt = |observed_seq| ApiTransportObservedReceipt {
            api_call_id,
            started_seq,
            observed_seq,
            continuation_action_id: continuation_action_id.clone(),
        };
        if let Some(observed_seq) = existing_observed_seq {
            return Ok(IdempotentMutation::Idempotent(receipt(observed_seq)));
        }
        validate_timestamp("api_transport_observed.now_ms", now_ms)?;
        if self.events.iter().any(|event| {
            matches!(
                &event.event,
                EventBody::ApiStats(stats) if stats.api_call_id == api_call_id
            )
        }) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed_after_terminal",
            });
        }
        match &self.active_api {
            Some(active) if active.api_call_id == api_call_id => {}
            _ => return Err(DomainError::NoActiveApi),
        }
        self.ensure_transition_capacity(1, 0)?;
        let turn_id = started_turn_id
            .ok_or_else(|| DomainError::reference("api_started.turn_id", api_call_id.value()))?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let observed_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::HostOnly,
            EventBody::ExecutionControl(ExecutionControlEvent {
                control_kind: ExecutionControlKind::ApiTransportObserved,
                api_call_id: Some(api_call_id),
                client_action_id: None,
            }),
            now_ms,
        )?;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &[observed_seq]);
        Ok(IdempotentMutation::Committed(receipt(observed_seq)))
    }

    /// Reads the durable transport-observed receipt owned by one Continue action.
    /// 读取由一次 Continue 动作拥有的持久化 transport-observed receipt。
    pub fn api_transport_observed_for_action(
        &self,
        action_id: &ClientActionId,
    ) -> Result<Option<ApiTransportObservedReceipt>, DomainError> {
        let Some(prepared) = self.existing_api_start_for_action(action_id)? else {
            return Ok(None);
        };
        let mut observed_seq = None;
        for event in &self.events {
            let EventBody::ExecutionControl(control) = &event.event else {
                continue;
            };
            if control.control_kind != ExecutionControlKind::ApiTransportObserved
                || control.api_call_id != Some(prepared.api_call_id)
            {
                continue;
            }
            if observed_seq.replace(event.seq).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_api_transport_observed",
                });
            }
        }
        Ok(
            observed_seq.map(|observed_seq| ApiTransportObservedReceipt {
                api_call_id: prepared.api_call_id,
                started_seq: prepared.started_seq,
                observed_seq,
                continuation_action_id: prepared.continuation_action_id,
            }),
        )
    }

    /// Reconstructs the exact durable dispatch resolution for one Continue action.
    /// 重建一次 Continue 动作的精确持久化分派结果。
    ///
    /// # Parameters / 参数
    /// `action_id` is the browser idempotency identity already bound to Continue events.
    /// `action_id` 是已经绑定到 Continue 事件的浏览器幂等身份。
    ///
    /// # Returns / 返回值
    /// `None` before ApiStarted, otherwise the prepared, observed, or terminal-before-observed
    /// state. Duplicate terminal facts are rejected instead of being guessed.
    /// ApiStarted 前返回 `None`；否则返回已准备、已观察或观察前终止状态。重复终态事实会被
    /// 明确拒绝，而不是进行猜测。
    pub fn continuation_dispatch_resolution(
        &self,
        action_id: &ClientActionId,
    ) -> Result<Option<ContinuationDispatchResolution>, DomainError> {
        let Some(through_seq) = self.events.last().map(|event| event.seq) else {
            return Ok(None);
        };
        self.continuation_dispatch_resolution_through(action_id, through_seq)
    }

    /// Resolves one Continue dispatch from facts no later than an exact durable event frontier.
    /// 仅使用不晚于精确可靠事件前沿的事实解析一次 Continue 分派。
    ///
    /// # Parameters / 参数
    ///
    /// `action_id` is the persisted client idempotency identity and `through_seq` is the last event
    /// known to be durable by the unique storage owner.
    /// `action_id` 是持久化客户端幂等身份；`through_seq` 是唯一存储 owner 已确认可靠的最后事件。
    ///
    /// # Returns / 返回值
    ///
    /// Returns only the dispatch state derivable from the bounded durable prefix. Later in-memory
    /// facts are deliberately invisible to this query.
    /// 仅返回可由有界可靠前缀推导的分派状态；更晚的内存事实会被该查询明确忽略。
    pub fn continuation_dispatch_resolution_through(
        &self,
        action_id: &ClientActionId,
        through_seq: Seq,
    ) -> Result<Option<ContinuationDispatchResolution>, DomainError> {
        let mut prepared = None;
        for event in self
            .events
            .iter()
            .take_while(|event| event.seq <= through_seq)
        {
            let EventBody::ApiStarted(started) = &event.event else {
                continue;
            };
            if started.continuation_action_id.as_ref() != Some(action_id) {
                continue;
            }
            let receipt = PreparedApiStart {
                api_call_id: started.api_call_id,
                started_seq: event.seq,
                projection: started.projection.clone(),
                continuation_action_id: Some(action_id.clone()),
            };
            if prepared.replace(receipt).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_continue_api_binding",
                });
            }
        }
        let Some(prepared) = prepared else {
            return Ok(None);
        };
        let mut observed_seq = None;
        for event in self
            .events
            .iter()
            .take_while(|event| event.seq <= through_seq)
        {
            let EventBody::ExecutionControl(control) = &event.event else {
                continue;
            };
            if control.control_kind != ExecutionControlKind::ApiTransportObserved
                || control.api_call_id != Some(prepared.api_call_id)
            {
                continue;
            }
            if observed_seq.replace(event.seq).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_api_transport_observed",
                });
            }
        }
        if let Some(observed_seq) = observed_seq {
            let observed = ApiTransportObservedReceipt {
                api_call_id: prepared.api_call_id,
                started_seq: prepared.started_seq,
                observed_seq,
                continuation_action_id: prepared.continuation_action_id,
            };
            return Ok(Some(ContinuationDispatchResolution::TransportObserved(
                observed,
            )));
        }
        let mut terminal_seq = None;
        for event in self
            .events
            .iter()
            .take_while(|event| event.seq <= through_seq)
        {
            let EventBody::ApiStats(stats) = &event.event else {
                continue;
            };
            if stats.api_call_id != prepared.api_call_id {
                continue;
            }
            if terminal_seq.replace(event.seq).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_continue_api_terminal",
                });
            }
        }
        Ok(Some(match terminal_seq {
            Some(terminal_seq) => ContinuationDispatchResolution::TerminalBeforeTransport {
                api_call_id: prepared.api_call_id,
                started_seq: prepared.started_seq,
                terminal_seq,
            },
            None => ContinuationDispatchResolution::AwaitingTransport {
                api_call_id: prepared.api_call_id,
                started_seq: prepared.started_seq,
            },
        }))
    }

    /// Finds the durable API binding of one Continue action.
    /// 查找一次 Continue 动作的持久化 API 绑定。
    fn existing_api_start_for_action(
        &self,
        action_id: &ClientActionId,
    ) -> Result<Option<PreparedApiStart>, DomainError> {
        let mut found = None;
        for event in &self.events {
            let EventBody::ApiStarted(started) = &event.event else {
                continue;
            };
            if started.continuation_action_id.as_ref() != Some(action_id) {
                continue;
            }
            let receipt = PreparedApiStart {
                api_call_id: started.api_call_id,
                started_seq: event.seq,
                projection: started.projection.clone(),
                continuation_action_id: Some(action_id.clone()),
            };
            if found.replace(receipt).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_continue_api_binding",
                });
            }
        }
        Ok(found)
    }

    /// Validates that one unbound Continue action belongs to the open Turn.
    /// 校验一条尚未绑定的 Continue 动作属于当前开放回合。
    fn validate_continuation_action_for_api(
        &self,
        action_id: &ClientActionId,
        turn_id: TurnId,
    ) -> Result<(), DomainError> {
        let mut requested = None;
        let mut resumed = None;
        for event in &self.events {
            let EventBody::ExecutionControl(control) = &event.event else {
                continue;
            };
            if control.client_action_id.as_ref() != Some(action_id) {
                continue;
            }
            match control.control_kind {
                ExecutionControlKind::TurnContinueRequested => {
                    if requested.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_continue_requested",
                        });
                    }
                }
                ExecutionControlKind::TurnResumed if resumed.replace(event).is_some() => {
                    return Err(DomainError::InvalidContinuationHistory {
                        field: "duplicate_turn_resumed",
                    });
                }
                ExecutionControlKind::TurnResumed => {}
                _ => {}
            }
        }
        let requested = requested.ok_or(DomainError::InvalidContinuationHistory {
            field: "continue_api.missing_requested",
        })?;
        let resumed = resumed.ok_or(DomainError::InvalidContinuationHistory {
            field: "continue_api.missing_resumed",
        })?;
        if requested.turn_id != Some(turn_id)
            || resumed.turn_id != Some(turn_id)
            || requested.seq.value() >= resumed.seq.value()
        {
            return Err(DomainError::InvalidContinuationHistory {
                field: "continue_api.turn_or_order",
            });
        }
        Ok(())
    }

    /// Appends or updates the displayable reasoning semantic of one API.
    /// 追加或更新一次 API 的可显示推理语义。
    ///
    /// The first call allocates a new seq; later calls update the same event
    /// in place and never allocate a new seq.
    /// 首次调用分配新 seq；后续调用原位更新同一事件，绝不分配新 seq。
    pub fn append_reasoning(
        &mut self,
        api_call_id: ApiCallId,
        text: impl Into<String>,
        status: AssistantSemanticStatus,
        now_ms: i64,
    ) -> Result<IdempotentMutation<Seq>, DomainError> {
        let mut api = self.require_active_api(api_call_id)?;
        if api.text_seq.is_some() || !api.tool_seqs.is_empty() || api.stats_seq.is_some() {
            return Err(DomainError::SequenceViolation {
                expected: 0,
                actual: api_call_id.value(),
            });
        }
        let text = text.into();
        let mutation = match api.reasoning_seq {
            Some(seq) => self.update_provider_semantic(
                seq,
                now_ms,
                EventBody::AssistantReasoning(AssistantReasoningEvent {
                    api_call_id,
                    text,
                    status,
                    provider_source: None,
                }),
            )?,
            None => {
                let seq = self.append_event(
                    Some(api.turn_id),
                    EventVisibility::Provider,
                    EventBody::AssistantReasoning(AssistantReasoningEvent {
                        api_call_id,
                        text,
                        status,
                        provider_source: None,
                    }),
                    now_ms,
                )?;
                api.reasoning_seq = Some(seq);
                IdempotentMutation::Committed(seq)
            }
        };
        self.save_active_api(api);
        Ok(mutation)
    }

    /// Appends or updates the assistant text semantic of one API.
    /// 追加或更新一次 API 的 assistant 正文语义。
    pub fn append_assistant_text(
        &mut self,
        api_call_id: ApiCallId,
        text: impl Into<String>,
        status: AssistantSemanticStatus,
        now_ms: i64,
    ) -> Result<IdempotentMutation<Seq>, DomainError> {
        let mut api = self.require_active_api(api_call_id)?;
        if !api.tool_seqs.is_empty() || api.stats_seq.is_some() {
            return Err(DomainError::SequenceViolation {
                expected: 0,
                actual: api_call_id.value(),
            });
        }
        let text = text.into();
        let mutation = match api.text_seq {
            Some(seq) => self.update_provider_semantic(
                seq,
                now_ms,
                EventBody::AssistantText(AssistantTextEvent {
                    api_call_id,
                    text,
                    status,
                    provider_source: None,
                }),
            )?,
            None => {
                let seq = self.append_event(
                    Some(api.turn_id),
                    EventVisibility::Provider,
                    EventBody::AssistantText(AssistantTextEvent {
                        api_call_id,
                        text,
                        status,
                        provider_source: None,
                    }),
                    now_ms,
                )?;
                api.text_seq = Some(seq);
                IdempotentMutation::Committed(seq)
            }
        };
        self.save_active_api(api);
        Ok(mutation)
    }

    /// Durably prepares one tool invocation without starting its body.
    /// 持久化准备一条工具调用，但不启动其正文执行。
    pub fn prepare_tool(
        &mut self,
        api_call_id: ApiCallId,
        request_stream_id: RequestStreamId,
        tool_name: impl Into<String>,
        input: ToolPayload,
        provider_tool_call_id: Option<String>,
        now_ms: i64,
    ) -> Result<ToolCallId, DomainError> {
        let prepared = self
            .prepare_tool_batch(
                api_call_id,
                vec![ToolPreparation {
                    request_stream_id,
                    tool_name: tool_name.into(),
                    input,
                    provider_tool_call_id,
                }],
                now_ms,
            )?
            .into_value();
        prepared
            .tools
            .into_iter()
            .next()
            .map(|receipt| receipt.tool_call_id)
            .ok_or_else(|| DomainError::reference("prepared_tool_batch", api_call_id.value()))
    }

    /// Durably prepares one Provider-ordered tool batch and reserves all seqs.
    /// 按 Provider 原始顺序持久化准备一个工具批次并预留全部 seq。
    pub fn prepare_tool_batch(
        &mut self,
        api_call_id: ApiCallId,
        preparations: Vec<ToolPreparation>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<PreparedToolBatchReceipt>, DomainError> {
        self.prepare_tool_batch_internal(api_call_id, preparations, None, now_ms)
    }

    /// Atomically prepares one Provider-ordered tool batch and binds its replay source.
    /// 原子准备一个 Provider 有序工具批次并绑定其重放来源。
    ///
    /// The complete batch and its source are committed as one domain mutation. The source is
    /// always placed on the final newly prepared Provider tool event, while API statistics and
    /// active-API closure remain deferred until every tool reaches a terminal state.
    /// 完整批次及其来源作为一次领域修改提交。来源始终放在最后一条新建的 Provider
    /// Prepared 工具事件上；API 统计与活动 API 闭合仍延迟到全部工具到达终态之后。
    ///
    /// # Parameters
    /// 参数
    /// `api_call_id` identifies the active API, `preparations` preserves Provider order,
    /// `provider_source` carries the typed terminal replay facts, and `now_ms` timestamps the
    /// atomic boundary.
    /// `api_call_id` 标识活动 API，`preparations` 保持 Provider 顺序，`provider_source`
    /// 携带强类型终态重放事实，`now_ms` 为原子边界提供时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns session-local tool identities in Provider order. Any validation failure leaves
    /// events, allocators, indexes, revisions, and active API state unchanged.
    /// 按 Provider 顺序返回会话本地工具身份；任何校验失败都不会修改事件、分配器、索引、
    /// 修订号或活动 API 状态。
    pub fn prepare_tool_batch_with_provider_source(
        &mut self,
        api_call_id: ApiCallId,
        preparations: Vec<ToolPreparation>,
        provider_source: ProviderSourceInput,
        now_ms: i64,
    ) -> Result<IdempotentMutation<PreparedToolBatchReceipt>, DomainError> {
        self.prepare_tool_batch_internal(api_call_id, preparations, Some(provider_source), now_ms)
    }

    /// Implements the transactional Prepared-batch transition with an optional typed source.
    /// 实现带可选强类型来源的事务化 Prepared 批次转换。
    fn prepare_tool_batch_internal(
        &mut self,
        api_call_id: ApiCallId,
        preparations: Vec<ToolPreparation>,
        provider_source: Option<ProviderSourceInput>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<PreparedToolBatchReceipt>, DomainError> {
        let mut api = self.require_active_api(api_call_id)?;
        if api.stats_seq.is_some() {
            return Err(DomainError::SequenceViolation {
                expected: 0,
                actual: api_call_id.value(),
            });
        }
        self.validate_active_api_events(&api)?;
        validate_tool_preparations(&preparations, provider_source.is_some())?;
        if let Some(receipt) =
            self.existing_prepared_tool_batch(api_call_id, &preparations, provider_source.as_ref())?
        {
            return Ok(IdempotentMutation::Idempotent(receipt));
        }
        if let Some(source_input) = provider_source.as_ref() {
            if preparations.is_empty() {
                return Err(DomainError::ReferenceViolation {
                    kind: "provider_tool_batch",
                    value: api_call_id.value(),
                });
            }
            if api.provider_source_placed {
                return Err(DomainError::ProviderSourceAlreadySet {
                    api_call_id: api_call_id.value(),
                });
            }
            validate_provider_source_input(&api, source_input)?;
        }
        let count = u64::try_from(preparations.len())
            .map_err(|_| DomainError::invalid_id("tool_batch_size", u64::MAX))?;
        self.ensure_transition_capacity(count, count)?;

        // Stage every allocation and event before mutating the authoritative document, so source
        // validation or fingerprint failure cannot leave a partial batch.
        // 在修改权威文档前暂存全部分配与事件，使来源校验或指纹失败不会留下半批数据。
        let mut staged_allocator = self.id_allocator;
        let mut tool_receipts = Vec::with_capacity(preparations.len());
        let mut staged_events = Vec::with_capacity(preparations.len());
        for preparation in preparations {
            let tool_call_id = ToolCallId::try_new(staged_allocator.next_local_id()?)?;
            let seq = staged_allocator.next_seq()?;
            let expected = self.resident_start_seq
                + u64::try_from(self.events.len() + staged_events.len())
                    .map_err(|_| DomainError::invalid_id("event_count", u64::MAX))?;
            if seq.value() != expected {
                return Err(DomainError::SequenceViolation {
                    expected,
                    actual: seq.value(),
                });
            }
            staged_events.push(ConversationEvent {
                seq,
                turn_id: Some(api.turn_id),
                created_at_ms: now_ms,
                updated_at_ms: now_ms,
                event_revision: crate::v1::event::EVENT_REVISION_INITIAL,
                visibility: EventVisibility::Provider,
                event: EventBody::ToolCall(ToolCallEvent {
                    api_call_id,
                    tool_call_id,
                    request_stream_id: preparation.request_stream_id.clone(),
                    tool_name: preparation.tool_name,
                    input: preparation.input,
                    output: None,
                    changeset: None,
                    status: ToolCallStatus::Prepared,
                    started_at_ms: None,
                    finished_at_ms: None,
                    provider_tool_call_id: preparation.provider_tool_call_id,
                    external_refs: Vec::new(),
                    generated_resources: Vec::new(),
                    provider_source: None,
                }),
            });
            api.tool_seqs.push(seq);
            tool_receipts.push(PreparedToolReceipt {
                api_call_id,
                tool_call_id,
                request_stream_id: preparation.request_stream_id,
                tool_seq: seq,
            });
        }

        if let Some(source_input) = provider_source.as_ref() {
            let last_provider_seq = staged_events
                .last()
                .map(ConversationEvent::seq)
                .ok_or_else(|| DomainError::ReferenceViolation {
                    kind: "provider_tool_batch",
                    value: api_call_id.value(),
                })?;
            let mut fingerprint_events = self.events.clone();
            fingerprint_events.extend(staged_events.iter().cloned());
            let source =
                build_provider_source(&fingerprint_events, last_provider_seq, source_input)?;
            let Some(ConversationEvent {
                event: EventBody::ToolCall(last_tool),
                ..
            }) = staged_events.last_mut()
            else {
                return Err(DomainError::ReferenceViolation {
                    kind: "provider_tool_batch",
                    value: api_call_id.value(),
                });
            };
            last_tool.provider_source = Some(source);
            api.provider_source_placed = true;
        }

        // Publish only after every fallible operation above has succeeded.
        // 仅在上方全部可失败操作成功后发布。
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.id_allocator = staged_allocator;
        for receipt in &tool_receipts {
            self.tool_seq_by_id
                .insert(receipt.tool_call_id, receipt.tool_seq);
        }
        self.events.extend(staged_events);
        self.save_active_api(api);
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &changed_event_seqs);
        Ok(IdempotentMutation::Committed(PreparedToolBatchReceipt {
            api_call_id,
            tools: tool_receipts,
        }))
    }

    /// Reconstructs an exact prepared-batch receipt for an idempotent retry.
    /// 为一次幂等重试重建精确的已准备批次 receipt。
    fn existing_prepared_tool_batch(
        &self,
        api_call_id: ApiCallId,
        preparations: &[ToolPreparation],
        provider_source: Option<&ProviderSourceInput>,
    ) -> Result<Option<PreparedToolBatchReceipt>, DomainError> {
        let requested_streams: BTreeSet<&str> = preparations
            .iter()
            .map(|preparation| preparation.request_stream_id.as_str())
            .collect();
        let existing: Vec<(&ConversationEvent, &ToolCallEvent)> = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::ToolCall(tool)
                    if tool.api_call_id == api_call_id
                        && requested_streams.contains(tool.request_stream_id.as_str()) =>
                {
                    Some((event, tool))
                }
                _ => None,
            })
            .collect();
        if existing.is_empty() {
            return Ok(None);
        }
        if existing.len() != preparations.len() {
            return Err(DomainError::InvalidToolIdentity {
                field: "prepared_batch_partial_match",
            });
        }
        let mut receipts = Vec::with_capacity(existing.len());
        for (index, ((event, tool), preparation)) in existing.iter().zip(preparations).enumerate() {
            if tool.request_stream_id != preparation.request_stream_id
                || tool.tool_name != preparation.tool_name
                || tool.input != preparation.input
                || tool.provider_tool_call_id != preparation.provider_tool_call_id
            {
                return Err(DomainError::InvalidToolIdentity {
                    field: "prepared_batch_semantic_drift",
                });
            }
            if let Some(source_input) = provider_source {
                let is_last = index + 1 == preparations.len();
                match (is_last, tool.provider_source.as_ref()) {
                    (true, Some(source)) if provider_source_matches_input(source, source_input) => {
                    }
                    (false, None) => {}
                    _ => {
                        return Err(DomainError::InvalidToolIdentity {
                            field: "prepared_batch_provider_source",
                        });
                    }
                }
            }
            receipts.push(PreparedToolReceipt {
                api_call_id,
                tool_call_id: tool.tool_call_id,
                request_stream_id: tool.request_stream_id.clone(),
                tool_seq: event.seq,
            });
        }
        Ok(Some(PreparedToolBatchReceipt {
            api_call_id,
            tools: receipts,
        }))
    }

    /// Starts one previously prepared tool on its existing seq.
    /// 在既有 seq 上启动一条已经准备完成的工具调用。
    pub fn start_prepared_tool(
        &mut self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let current = self
            .require_tool_for_api(api_call_id, tool_call_id)?
            .1
            .status;
        match current {
            ToolCallStatus::Prepared => {
                self.update_tool_in_place(api_call_id, tool_call_id, now_ms, |tool| {
                    tool.status = ToolCallStatus::Running;
                    tool.started_at_ms = Some(now_ms);
                })?;
                Ok(true)
            }
            ToolCallStatus::Running => Ok(false),
            actual => Err(DomainError::ToolStatusMismatch {
                tool_call_id: tool_call_id.value(),
                expected: "prepared_or_running",
                actual: tool_status_name(actual),
            }),
        }
    }

    /// Starts one tool through both lifecycle steps for legacy unit fixtures.
    /// 仅供既有单元测试夹具依次经过两个生命周期步骤启动工具。
    #[cfg(test)]
    pub(crate) fn start_tool(
        &mut self,
        api_call_id: ApiCallId,
        tool_name: impl Into<String>,
        input: ToolPayload,
        provider_tool_call_id: Option<String>,
        now_ms: i64,
    ) -> Result<ToolCallId, DomainError> {
        let tool_name = tool_name.into();
        let request_stream_id = RequestStreamId::try_new(format!(
            "fixture-stream-{}",
            provider_tool_call_id
                .as_deref()
                .unwrap_or(tool_name.as_str())
        ))?;
        let tool_call_id = self.prepare_tool(
            api_call_id,
            request_stream_id,
            tool_name,
            input,
            provider_tool_call_id,
            now_ms,
        )?;
        self.start_prepared_tool(api_call_id, tool_call_id, now_ms)?;
        Ok(tool_call_id)
    }

    /// Reads the authoritative status of one exact resident tool fact.
    /// 读取一条精确常驻工具事实的权威状态。
    ///
    /// # Parameters
    /// 参数
    /// `api_call_id` and `tool_call_id` identify the exact resident tool fact.
    /// `api_call_id` 与 `tool_call_id` 标识精确的常驻工具事实。
    ///
    /// # Returns
    /// 返回值
    /// Returns the authoritative current status, or an explicit identity/ownership error.
    /// 返回权威当前状态；身份或归属不匹配时返回显式错误。
    pub fn tool_status(
        &self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
    ) -> Result<ToolCallStatus, DomainError> {
        self.require_tool_for_api(api_call_id, tool_call_id)
            .map(|(_, tool)| tool.status)
    }

    /// Marks one tool as waiting for a user answer (Ask flow).
    /// 把单个工具标记为等待用户回答(Ask 流程)。
    pub fn mark_tool_waiting_user(
        &mut self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let current = self
            .require_tool_for_api(api_call_id, tool_call_id)?
            .1
            .status;
        match current {
            ToolCallStatus::Running => {
                self.update_tool_in_place(api_call_id, tool_call_id, now_ms, |tool| {
                    tool.status = ToolCallStatus::WaitingUser;
                })?;
                Ok(true)
            }
            ToolCallStatus::WaitingUser => Ok(false),
            actual => Err(DomainError::ToolStatusMismatch {
                tool_call_id: tool_call_id.value(),
                expected: "running_or_waiting_user",
                actual: tool_status_name(actual),
            }),
        }
    }

    /// Replaces the observable output of one running tool on its fixed seq.
    /// 在固定 seq 上替换一个运行中工具的可观察输出。
    ///
    /// This is a progress update, not a terminal transition. Change-sets and
    /// external navigation references remain terminal facts owned by
    /// `finish_tool`.
    /// 这是进度更新而不是终态转换。变更集与外部导航引用仍是由
    /// `finish_tool` 写入的终态事实。
    pub fn update_running_tool_output(
        &mut self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        output: Option<ToolPayload>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let tool = self.require_tool_for_api(api_call_id, tool_call_id)?.1;
        if tool.status != ToolCallStatus::Running {
            return Err(DomainError::ToolStatusMismatch {
                tool_call_id: tool_call_id.value(),
                expected: "running",
                actual: tool_status_name(tool.status),
            });
        }
        if tool.output == output {
            return Ok(false);
        }
        self.update_tool_in_place(api_call_id, tool_call_id, now_ms, |tool| {
            tool.output = output;
        })?;
        Ok(true)
    }

    /// Finishes one tool on the same event without allocating a new seq.
    /// 在同一事件上结束一次工具调用，不分配新 seq。
    pub fn finish_tool(
        &mut self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        completion: ToolCompletion,
    ) -> Result<IdempotentMutation<ToolCompletionReceipt>, DomainError> {
        validate_tool_completion_changeset(completion.changeset.as_ref())?;
        if !matches!(
            completion.status,
            ToolCallStatus::Completed
                | ToolCallStatus::Failed
                | ToolCallStatus::Cancelled
                | ToolCallStatus::Stopped
        ) {
            return Err(DomainError::ToolStatusMismatch {
                tool_call_id: tool_call_id.value(),
                expected: "terminal",
                actual: tool_status_name(completion.status),
            });
        }
        let output =
            canonical_terminal_tool_output(tool_call_id, completion.status, completion.output)?;
        let seq = self
            .tool_seq_by_id
            .get(&tool_call_id)
            .copied()
            .ok_or_else(|| DomainError::reference("tool_call_id", tool_call_id.value()))?;
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        let event = self
            .events
            .get(index)
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let tool = match &event.event {
            EventBody::ToolCall(tool) => tool,
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "tool_call",
                });
            }
        };
        if tool.api_call_id != api_call_id {
            return Err(DomainError::CrossApiGroupUpdate {
                event_seq: seq.value(),
                api_call_id: api_call_id.value(),
            });
        }

        if matches!(
            tool.status,
            ToolCallStatus::Completed
                | ToolCallStatus::Failed
                | ToolCallStatus::Cancelled
                | ToolCallStatus::Stopped
                | ToolCallStatus::Interrupted
        ) {
            validate_tool_lifecycle_shape(tool)?;
            if tool.status != completion.status {
                return Err(DomainError::ToolStatusMismatch {
                    tool_call_id: tool_call_id.value(),
                    expected: tool_status_name(tool.status),
                    actual: tool_status_name(completion.status),
                });
            }
            if tool.output.as_ref() != Some(&output) {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "tool_call_terminal",
                    field: "output",
                });
            }
            if tool.changeset != completion.changeset {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "tool_call_terminal",
                    field: "changeset",
                });
            }
            if tool.external_refs != completion.external_refs {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "tool_call_terminal",
                    field: "external_refs",
                });
            }
            if tool.generated_resources != completion.generated_resources {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "tool_call_terminal",
                    field: "generated_resources",
                });
            }
            return Ok(IdempotentMutation::Idempotent(tool_completion_receipt(
                seq, tool,
            )?));
        }
        if tool.status != ToolCallStatus::Running {
            return Err(DomainError::ToolStatusMismatch {
                tool_call_id: tool_call_id.value(),
                expected: "running_or_terminal",
                actual: tool_status_name(tool.status),
            });
        }
        validate_timestamp("tool.finished_at_ms", completion.finished_at_ms)?;
        if tool
            .started_at_ms
            .is_some_and(|started_at_ms| completion.finished_at_ms < started_at_ms)
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "tool_call",
                field: "lifecycle_timestamps",
            });
        }

        // Stage the complete same-seq transition so event-revision or shape
        // validation cannot leave a partially terminal tool in the document.
        // 先暂存完整的同 seq 转换，确保事件修订号或结构校验失败时不会留下半终态工具。
        let mut staged_event = event.clone();
        staged_event.mutate_in_place(completion.finished_at_ms, |body| match body {
            EventBody::ToolCall(tool) => {
                tool.output = Some(output);
                tool.changeset = completion.changeset;
                tool.status = completion.status;
                tool.finished_at_ms = Some(completion.finished_at_ms);
                tool.external_refs = completion.external_refs;
                tool.generated_resources = completion.generated_resources;
            }
            _ => unreachable!("tool event kind was validated before mutation"),
        })?;
        let staged_tool = match &staged_event.event {
            EventBody::ToolCall(tool) => tool,
            _ => unreachable!("staged event preserves the validated tool kind"),
        };
        validate_tool_lifecycle_shape(staged_tool)?;
        let receipt = tool_completion_receipt(seq, staged_tool)?;
        self.events[index] = staged_event;
        self.bump_revision(&[DirtyFile::ActiveTail], completion.finished_at_ms, &[seq]);
        Ok(IdempotentMutation::Committed(receipt))
    }

    /// Completes a WaitingUser Ask after receiving the user's answer.
    /// 在收到用户回答后完成一条 WaitingUser Ask。
    pub fn answer_waiting_tool(
        &mut self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        completion: ToolCompletion,
    ) -> Result<IdempotentMutation<ToolCompletionReceipt>, DomainError> {
        validate_tool_completion_changeset(completion.changeset.as_ref())?;
        let (seq, current, started_at_ms) = {
            let (seq, tool) = self.require_tool_for_api(api_call_id, tool_call_id)?;
            (seq, tool.status, tool.started_at_ms)
        };
        if is_terminal_tool_status(current) {
            return self.finish_tool(api_call_id, tool_call_id, completion);
        }
        if current != ToolCallStatus::WaitingUser {
            return Err(DomainError::ToolStatusMismatch {
                tool_call_id: tool_call_id.value(),
                expected: "waiting_user_or_terminal",
                actual: tool_status_name(current),
            });
        }
        if !matches!(
            completion.status,
            ToolCallStatus::Completed | ToolCallStatus::Failed
        ) {
            return Err(DomainError::ToolStatusMismatch {
                tool_call_id: tool_call_id.value(),
                expected: "completed_or_failed",
                actual: tool_status_name(completion.status),
            });
        }
        validate_timestamp("tool.finished_at_ms", completion.finished_at_ms)?;
        if started_at_ms.is_some_and(|started_at_ms| completion.finished_at_ms < started_at_ms) {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "tool_call",
                field: "lifecycle_timestamps",
            });
        }
        let output =
            canonical_terminal_tool_output(tool_call_id, completion.status, completion.output)?;
        self.update_tool_in_place(
            api_call_id,
            tool_call_id,
            completion.finished_at_ms,
            |tool| {
                tool.output = Some(output);
                tool.changeset = completion.changeset;
                tool.status = completion.status;
                tool.finished_at_ms = Some(completion.finished_at_ms);
                tool.external_refs = completion.external_refs;
                tool.generated_resources = completion.generated_resources;
            },
        )?;
        Ok(IdempotentMutation::Committed(ToolCompletionReceipt {
            api_call_id,
            tool_call_id,
            tool_seq: seq,
            status: completion.status,
            finished_at_ms: completion.finished_at_ms,
        }))
    }

    /// Cancels a WaitingUser Ask on its existing event.
    /// 在既有事件上取消一条 WaitingUser Ask。
    pub fn cancel_waiting_tool(
        &mut self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        output: Option<ToolPayload>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<ToolCompletionReceipt>, DomainError> {
        let (seq, current, started_at_ms) = {
            let (seq, tool) = self.require_tool_for_api(api_call_id, tool_call_id)?;
            (seq, tool.status, tool.started_at_ms)
        };
        let completion = ToolCompletion {
            output,
            changeset: None,
            status: ToolCallStatus::Cancelled,
            finished_at_ms: now_ms,
            external_refs: Vec::new(),
            generated_resources: Vec::new(),
        };
        if is_terminal_tool_status(current) {
            return self.finish_tool(api_call_id, tool_call_id, completion);
        }
        if current != ToolCallStatus::WaitingUser {
            return Err(DomainError::ToolStatusMismatch {
                tool_call_id: tool_call_id.value(),
                expected: "waiting_user_or_terminal",
                actual: tool_status_name(current),
            });
        }
        validate_timestamp("tool.finished_at_ms", now_ms)?;
        if started_at_ms.is_some_and(|started_at_ms| now_ms < started_at_ms) {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "tool_call",
                field: "lifecycle_timestamps",
            });
        }
        let output = canonical_terminal_tool_output(
            tool_call_id,
            ToolCallStatus::Cancelled,
            completion.output,
        )?;
        self.update_tool_in_place(api_call_id, tool_call_id, now_ms, |tool| {
            tool.output = Some(output);
            tool.status = ToolCallStatus::Cancelled;
            tool.finished_at_ms = Some(now_ms);
        })?;
        Ok(IdempotentMutation::Committed(ToolCompletionReceipt {
            api_call_id,
            tool_call_id,
            tool_seq: seq,
            status: ToolCallStatus::Cancelled,
            finished_at_ms: now_ms,
        }))
    }

    /// Completes one physical API group with its terminal statistic.
    /// 用终态统计闭合一次物理 API 组。
    ///
    /// `provider_source` may only be attached to a group that produced at
    /// least one Provider-sourced event; it is placed exactly once on the last
    /// Provider-sourced event of the group.
    /// `provider_source` 只允许附着到至少产生过一个 Provider 来源事件的组，
    /// 并且恰好放置一次在组内最后一个 Provider 来源事件上。
    pub fn complete_api_group(
        &mut self,
        api_call_id: ApiCallId,
        completion: ApiCompletion,
    ) -> Result<ApiGroupCompletionReceipt, DomainError> {
        let mut terminal_timing = completion.timing.clone();
        if terminal_timing.completed_at_ms.is_none() {
            terminal_timing.completed_at_ms = Some(completion.completed_at_ms);
        }
        let terminal_completed_at_ms = validate_api_terminal_facts(
            completion.status,
            completion.end_reason,
            completion.error.as_ref(),
            completion.continuation_eligibility,
            &terminal_timing,
        )?;
        if terminal_completed_at_ms != completion.completed_at_ms {
            return Err(DomainError::InvalidApiTerminalFacts {
                field: "completed_at_mismatch",
            });
        }
        let mut api = self.require_active_api(api_call_id)?;
        let failure_notice = completion.end_reason.and_then(api_failure_notice);
        let redacted_detail = completion.error.as_ref().map(|error| error.message.clone());
        let pending_inputs = self.metadata.input_state.pending_in_turn_inputs.clone();
        let had_pending_inputs = !pending_inputs.is_empty();
        for pending in &pending_inputs {
            validate_user_input(pending.text.clone(), &pending.resource_ids)?;
            if pending.turn_id != api.turn_id {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "pending_input_turn_mismatch",
                });
            }
        }
        let promotable_count = pending_inputs
            .iter()
            .filter(|pending| !self.input_id_already_streamed(pending.input_id))
            .count();
        let promotable_count = u64::try_from(promotable_count)
            .map_err(|_| DomainError::invalid_id("pending_input_count", u64::MAX))?;
        self.ensure_transition_capacity(
            1 + u64::from(failure_notice.is_some()) + promotable_count,
            0,
        )?;
        self.validate_active_api_events(&api)?;
        ensure_api_request_field(
            "provider",
            &api.request.provider,
            &completion.request.provider,
        )?;
        ensure_api_request_field("profile", &api.request.profile, &completion.request.profile)?;
        ensure_api_request_field("model", &api.request.model, &completion.request.model)?;
        ensure_api_request_field(
            "connection_id",
            &api.request.connection_id,
            &completion.request.connection_id,
        )?;
        ensure_api_request_field(
            "thinking_level",
            &api.request.thinking_level,
            &completion.request.thinking_level,
        )?;
        ensure_api_request_field(
            "temperature",
            &api.request.temperature,
            &completion.request.temperature,
        )?;
        let source_to_attach = if let Some(source_input) = &completion.provider_source {
            validate_provider_source_input(&api, source_input)?;
            let last_provider_seq =
                api.last_provider_sourced_seq()
                    .ok_or_else(|| DomainError::ReferenceViolation {
                        kind: "provider_source",
                        value: api_call_id.value(),
                    })?;
            let expected_source =
                build_provider_source(&self.events, last_provider_seq, source_input)?;
            if api.provider_source_placed {
                let actual_source = provider_source_at_seq(
                    &self.events,
                    self.resident_start_seq,
                    last_provider_seq,
                )?;
                ensure_provider_source_matches(&expected_source, &actual_source)?;
                None
            } else {
                Some((last_provider_seq, expected_source))
            }
        } else if api.provider_source_placed {
            return Err(DomainError::ReferenceViolation {
                kind: "provider_source_terminal",
                value: api_call_id.value(),
            });
        } else {
            None
        };
        let mut staged_event_updates = BTreeMap::new();
        if completion.status != ApiCallStatus::Completed {
            for seq in api.reasoning_seq.iter().copied() {
                self.stage_active_event_interruption(
                    &mut staged_event_updates,
                    seq,
                    api_call_id,
                    completion.completed_at_ms,
                )?;
            }
            for seq in api.text_seq.iter().copied() {
                self.stage_active_event_interruption(
                    &mut staged_event_updates,
                    seq,
                    api_call_id,
                    completion.completed_at_ms,
                )?;
            }
            for seq in api.tool_seqs.iter().copied() {
                self.stage_active_event_interruption(
                    &mut staged_event_updates,
                    seq,
                    api_call_id,
                    completion.completed_at_ms,
                )?;
            }
        }
        self.ensure_staged_active_api_tools_terminal(&api, &staged_event_updates)?;
        if let Some((last_provider_seq, source)) = source_to_attach {
            self.stage_provider_source_attachment(
                &mut staged_event_updates,
                last_provider_seq,
                source,
                completion.completed_at_ms,
            )?;
            api.provider_source_placed = true;
        }
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(
            1 + usize::from(failure_notice.is_some()) + promotable_count as usize,
        );
        let mut staged_metadata = self.metadata.clone();
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(api.turn_id),
            EventVisibility::UiOnly,
            EventBody::ApiStats(ApiStatsEvent {
                api_call_id,
                purpose: api.purpose,
                provider: completion.request.provider.clone(),
                profile: completion.request.profile.clone(),
                model: completion.request.model.clone(),
                connection_id: completion.request.connection_id.clone(),
                thinking_level: completion.request.thinking_level.clone(),
                temperature: completion.request.temperature,
                usage: completion.usage.clone(),
                status: completion.status,
                end_reason: completion.end_reason,
                error: completion.error.clone(),
                continuation_eligibility: completion.continuation_eligibility,
                timing: terminal_timing,
            }),
            completion.completed_at_ms,
        )?;
        api.stats_seq = Some(seq);
        if let Some((notice_kind, severity, reason_code)) = failure_notice {
            self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(api.turn_id),
                EventVisibility::UiOnly,
                EventBody::DisplayNote(DisplayNoteEvent {
                    notice_kind,
                    severity,
                    reason_code: reason_code.to_string(),
                    redacted_detail,
                    api_call_id: Some(api_call_id),
                    retry_count: None,
                    retry_limit: None,
                }),
                completion.completed_at_ms,
            )?;
        }
        let mut promoted_input_seqs = Vec::new();
        for pending in pending_inputs {
            if self.input_id_already_streamed(pending.input_id) {
                continue;
            }
            let promoted_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(api.turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    client_action_id: pending.client_action_id,
                    input_id: pending.input_id,
                    input_source: UserInputSource::PendingInTurn,
                    text: pending.text,
                    resource_ids: pending.resource_ids,
                }),
                completion.completed_at_ms,
            )?;
            promoted_input_seqs.push(promoted_seq);
        }
        staged_metadata.input_state.pending_in_turn_inputs.clear();
        let mut changed_event_seqs: Vec<Seq> = staged_event_updates
            .values()
            .map(ConversationEvent::seq)
            .collect();
        changed_event_seqs.extend(staged_events.iter().map(ConversationEvent::seq));
        for (index, event) in staged_event_updates {
            self.events[index] = event;
        }
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.active_api = None;
        let dirty_files: &[DirtyFile] = if had_pending_inputs {
            &[DirtyFile::Metadata, DirtyFile::ActiveTail]
        } else {
            &[DirtyFile::ActiveTail]
        };
        self.bump_revision(dirty_files, completion.completed_at_ms, &changed_event_seqs);
        Ok(ApiGroupCompletionReceipt {
            api_call_id,
            turn_id: api.turn_id,
            stats_seq: seq,
            promoted_input_seqs,
        })
    }

    /// Interrupts the active physical API while preserving its logical turn.
    /// 中断活动物理 API，同时保留其所属逻辑回合。
    ///
    /// Streaming semantics and every non-terminal tool become interrupted
    /// before the terminal API statistic is published. The method records the
    /// explicit execution-control fact and one terminal
    /// `ApiStats(Interrupted, UserInterrupted)` event, then clears only the
    /// active API. The open turn remains available to `continue_turn`.
    /// 在发布 API 终态统计前，流式语义与每个非终态工具都会转为 interrupted。
    /// 该方法记录显式执行控制事实与一条终态
    /// `ApiStats(Interrupted, UserInterrupted)` 事件，随后只清除活动 API；开放
    /// 回合仍可供 `continue_turn` 继承。
    pub fn interrupt_active_api(&mut self, now_ms: i64) -> Result<Seq, DomainError> {
        let (_, completion) =
            self.interrupt_active_api_state(now_ms, ApiEndReason::UserInterrupted, true)?;
        Ok(completion.stats_seq)
    }

    /// Interrupts an active physical API because its owning process stopped unexpectedly.
    /// 因持有该物理 API 的进程意外停止而中断活动 API。
    ///
    /// # Parameters
    /// 参数
    /// `now_ms` is the exact runtime boundary timestamp used for every interrupted semantic,
    /// tool terminal, and the resulting API statistics event.
    /// `now_ms` 是用于所有被中断语义、工具终态及最终 API 统计事件的精确运行时边界时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns the terminal statistics seq when an active API was interrupted. A prepared
    /// Continue API that transport never observed remains dispatchable and returns `None`.
    /// 当活动 API 被中断时返回终态统计 seq；尚未被 transport 观察的已准备 Continue API
    /// 保持可分派状态并返回 `None`。
    pub fn interrupt_active_api_for_process(
        &mut self,
        now_ms: i64,
    ) -> Result<Option<Seq>, DomainError> {
        let Some(active_api) = self.active_api.as_ref() else {
            return Ok(None);
        };
        if active_api.purpose == ApiPurposeKind::Continue
            && !self.api_transport_was_observed(active_api.api_call_id)
        {
            return Ok(None);
        }
        let (_, completion) =
            self.interrupt_active_api_state(now_ms, ApiEndReason::ProcessInterrupted, false)?;
        Ok(Some(completion.stats_seq))
    }

    /// Atomically records one idempotent Continue action on an unfinished Turn.
    /// 在一个未完成回合上原子记录一次幂等继续动作。
    ///
    /// # Parameters
    /// 参数
    /// `request` carries the external idempotency key and optional supplementary
    /// user payload. `now_ms` is the explicit semantic-boundary timestamp.
    /// `request` 携带外部幂等键与可选补充用户载荷；`now_ms` 是显式语义边界时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns a receipt reconstructible from durable events. An exact retry
    /// returns the original receipt without advancing the document revision.
    /// 返回可从持久化事件重建的 receipt；精确重试返回原 receipt 且不推进文档修订。
    pub fn continue_open_turn(
        &mut self,
        request: ContinuationRequest,
        now_ms: i64,
    ) -> Result<IdempotentMutation<ContinueTurnReceipt>, DomainError> {
        if let Some(receipt) = self.existing_continuation_receipt(&request)? {
            return Ok(IdempotentMutation::Idempotent(receipt));
        }
        validate_timestamp("continue_open_turn.now_ms", now_ms)?;
        let turn_id = self.require_open_turn()?.turn_id;
        self.require_client_action_unused(&request.submission.client_action_id)?;
        if self.active_api.is_some() {
            return Err(DomainError::ContinuationNotEligible {
                reason: "active_api",
            });
        }
        self.validate_continuation_eligibility(turn_id)?;

        let ContinuationRequest {
            submission,
            hidden_model_guidance,
        } = request;
        let ClientInputSubmission {
            client_action_id,
            text,
            resource_ids,
        } = submission;
        let has_input = !text.is_empty() || !resource_ids.is_empty();
        if has_input {
            validate_user_input(text.clone(), &resource_ids)?;
        }
        let hidden_model_guidance = hidden_model_guidance
            .map(validate_system_context_content)
            .transpose()?;
        let event_count = 2 + u64::from(has_input) + u64::from(hidden_model_guidance.is_some());
        let local_id_count = u64::from(has_input);
        self.ensure_transition_capacity(event_count, local_id_count)?;

        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(event_count as usize);
        let mut staged_metadata = self.metadata.clone();
        let draft_consumed =
            has_input && consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
        let (input_id, input_seq) = if has_input {
            let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
            let input_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    client_action_id: client_action_id.clone(),
                    input_id,
                    input_source: UserInputSource::PendingInTurn,
                    text,
                    resource_ids,
                }),
                now_ms,
            )?;
            (Some(input_id), Some(input_seq))
        } else {
            (None, None)
        };
        let guidance_seq = if let Some(content) = hidden_model_guidance {
            Some(self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::SystemContext(SystemContextEvent {
                    context_key: continuation_guidance_key(&client_action_id),
                    operation: SystemContextOperation::Upsert,
                    scope: SystemContextScope::Turn,
                    ttl_turns: Some(1),
                    provider_role_hint: Some(SystemRoleHint::Developer),
                    content,
                }),
                now_ms,
            )?)
        } else {
            None
        };
        let requested_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::HostOnly,
            EventBody::ExecutionControl(ExecutionControlEvent {
                control_kind: ExecutionControlKind::TurnContinueRequested,
                api_call_id: None,
                client_action_id: Some(client_action_id.clone()),
            }),
            now_ms,
        )?;
        let resumed_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::HostOnly,
            EventBody::ExecutionControl(ExecutionControlEvent {
                control_kind: ExecutionControlKind::TurnResumed,
                api_call_id: None,
                client_action_id: Some(client_action_id),
            }),
            now_ms,
        )?;
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        let dirty_files: &[DirtyFile] = if draft_consumed {
            &[DirtyFile::Metadata, DirtyFile::ActiveTail]
        } else {
            &[DirtyFile::ActiveTail]
        };
        self.bump_revision(dirty_files, now_ms, &changed_event_seqs);
        Ok(IdempotentMutation::Committed(ContinueTurnReceipt {
            turn_id,
            input_id,
            input_seq,
            guidance_seq,
            requested_seq,
            resumed_seq,
        }))
    }

    /// Submits a uniquely identified root input for Core unit fixtures only.
    /// 仅为 Core 单元测试夹具提交一条具有唯一身份的根输入。
    #[cfg(test)]
    pub(crate) fn submit_root_input_fixture(
        &mut self,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
        now_ms: i64,
    ) -> Result<TurnId, DomainError> {
        let action_id = self.next_fixture_client_action_id("root", now_ms)?;
        self.submit_root_input(
            ClientInputSubmission::new(action_id, text, resource_ids),
            now_ms,
        )
        .map(IdempotentMutation::into_value)
    }

    /// Queues a uniquely identified input for Core unit fixtures only.
    /// 仅为 Core 单元测试夹具排队一条具有唯一身份的输入。
    #[cfg(test)]
    pub(crate) fn enqueue_input_fixture(
        &mut self,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
        now_ms: i64,
    ) -> Result<PendingInputId, DomainError> {
        let action_id = self.next_fixture_client_action_id("queued", now_ms)?;
        self.enqueue_input(
            ClientInputSubmission::new(action_id, text, resource_ids),
            now_ms,
        )
        .map(|outcome| outcome.into_value().input_id)
    }

    /// Adds a uniquely identified in-turn input for Core unit fixtures only.
    /// 仅为 Core 单元测试夹具添加一条具有唯一身份的回合内输入。
    #[cfg(test)]
    pub(crate) fn add_pending_in_turn_input_fixture(
        &mut self,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
        now_ms: i64,
    ) -> Result<PendingInputId, DomainError> {
        let action_id = self.next_fixture_client_action_id("pending", now_ms)?;
        self.add_pending_in_turn_input(
            ClientInputSubmission::new(action_id, text, resource_ids),
            now_ms,
        )
        .map(IdempotentMutation::into_value)
    }

    /// Continues an eligible Turn with a unique action for Core unit fixtures only.
    /// 仅为 Core 单元测试夹具使用唯一动作继续一个具备资格的回合。
    #[cfg(test)]
    pub(crate) fn continue_turn_fixture(&mut self, now_ms: i64) -> Result<(), DomainError> {
        let action_id = self.next_fixture_client_action_id("continue", now_ms)?;
        self.continue_open_turn(ContinuationRequest::new(action_id, "", Vec::new()), now_ms)
            .map(|_| ())
    }

    /// Builds one deterministic action identity unique inside the fixture document.
    /// 构建一个在夹具文档内唯一的确定性动作身份。
    #[cfg(test)]
    fn next_fixture_client_action_id(
        &self,
        kind: &str,
        now_ms: i64,
    ) -> Result<ClientActionId, DomainError> {
        let (next_local_id, next_seq) = self.id_allocator.frontier();
        ClientActionId::try_new(format!(
            "fixture-{kind}-{next_local_id}-{next_seq}-{now_ms}"
        ))
    }

    /// Reconstructs an existing continuation receipt before current eligibility checks.
    /// 在当前资格检查之前重建一份既有继续 receipt。
    fn existing_continuation_receipt(
        &self,
        request: &ContinuationRequest,
    ) -> Result<Option<ContinueTurnReceipt>, DomainError> {
        let action_id = &request.submission.client_action_id;
        let guidance_key = continuation_guidance_key(action_id);
        let expected_guidance = request
            .hidden_model_guidance
            .clone()
            .map(validate_system_context_content)
            .transpose()?;
        let mut input: Option<(&ConversationEvent, &UserInputEvent)> = None;
        let mut guidance: Option<(&ConversationEvent, &SystemContextEvent)> = None;
        let mut requested: Option<&ConversationEvent> = None;
        let mut resumed: Option<&ConversationEvent> = None;
        for event in &self.events {
            if let EventBody::SystemContext(context) = &event.event
                && context.context_key == guidance_key
            {
                if guidance.replace((event, context)).is_some() {
                    return Err(DomainError::InvalidContinuationHistory {
                        field: "duplicate_continue_guidance",
                    });
                }
                continue;
            }
            if event_client_action_id(event) != Some(action_id) {
                continue;
            }
            match &event.event {
                EventBody::UserInput(value) => {
                    if input.replace((event, value)).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_user_input",
                        });
                    }
                }
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::TurnContinueRequested =>
                {
                    if requested.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_continue_requested",
                        });
                    }
                }
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::TurnResumed =>
                {
                    if resumed.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_turn_resumed",
                        });
                    }
                }
                _ => return Err(client_action_conflict(action_id)),
            }
        }
        if requested.is_none() && resumed.is_none() {
            return if input.is_some() {
                Err(client_action_conflict(action_id))
            } else {
                Ok(None)
            };
        }
        let requested = requested.ok_or(DomainError::InvalidContinuationHistory {
            field: "missing_continue_requested",
        })?;
        let resumed = resumed.ok_or(DomainError::InvalidContinuationHistory {
            field: "missing_turn_resumed",
        })?;
        let turn_id = requested
            .turn_id
            .ok_or(DomainError::InvalidContinuationHistory {
                field: "continue_requested.turn_id",
            })?;
        if resumed.turn_id != Some(turn_id) || requested.seq.value() >= resumed.seq.value() {
            return Err(DomainError::InvalidContinuationHistory {
                field: "continue_pair_order",
            });
        }
        let requested_has_input =
            !request.submission.text.is_empty() || !request.submission.resource_ids.is_empty();
        let (input_id, input_seq) = match input {
            Some((event, value)) => {
                if !requested_has_input
                    || event.turn_id != Some(turn_id)
                    || event.seq.value() >= requested.seq.value()
                    || value.input_source != UserInputSource::PendingInTurn
                    || value.text != request.submission.text
                    || value.resource_ids != request.submission.resource_ids
                {
                    return Err(client_action_conflict(action_id));
                }
                (Some(value.input_id), Some(event.seq))
            }
            None if requested_has_input => return Err(client_action_conflict(action_id)),
            None => (None, None),
        };
        let guidance_seq = match (guidance, expected_guidance) {
            (Some((event, context)), Some(expected_content))
                if event.turn_id == Some(turn_id)
                    && event.seq.value() < requested.seq.value()
                    && context.operation == SystemContextOperation::Upsert
                    && context.scope == SystemContextScope::Turn
                    && context.ttl_turns == Some(1)
                    && context.provider_role_hint == Some(SystemRoleHint::Developer)
                    && context.content == expected_content =>
            {
                Some(event.seq)
            }
            (None, None) => None,
            _ => return Err(client_action_conflict(action_id)),
        };
        Ok(Some(ContinueTurnReceipt {
            turn_id,
            input_id,
            input_seq,
            guidance_seq,
            requested_seq: requested.seq,
            resumed_seq: resumed.seq,
        }))
    }

    /// Validates that the latest physical API terminal permits an explicit Continue.
    /// 校验最近一次物理 API 终态允许显式继续。
    fn validate_continuation_eligibility(&self, turn_id: TurnId) -> Result<(), DomainError> {
        let latest_started = self
            .events
            .iter()
            .rev()
            .find(|event| {
                event.turn_id == Some(turn_id) && matches!(event.event, EventBody::ApiStarted(_))
            })
            .ok_or(DomainError::ContinuationNotEligible {
                reason: "missing_api_started",
            })?;
        let EventBody::ApiStarted(started) = &latest_started.event else {
            unreachable!("latest ApiStarted selected above")
        };
        let mut terminal: Option<(&ConversationEvent, &ApiStatsEvent)> = None;
        for event in self
            .events
            .iter()
            .filter(|event| event.turn_id == Some(turn_id))
        {
            if let EventBody::ApiStats(stats) = &event.event
                && stats.api_call_id == started.api_call_id
                && terminal.replace((event, stats)).is_some()
            {
                return Err(DomainError::ContinuationNotEligible {
                    reason: "duplicate_api_terminal",
                });
            }
        }
        let (terminal_event, terminal) = terminal.ok_or(DomainError::ContinuationNotEligible {
            reason: "missing_api_terminal",
        })?;
        if terminal_event.seq.value() <= latest_started.seq.value() {
            return Err(DomainError::ContinuationNotEligible {
                reason: "api_terminal_order",
            });
        }
        if self.events.iter().any(|event| {
            event.turn_id == Some(turn_id)
                && event.seq.value() > terminal_event.seq.value()
                && matches!(
                    event.event,
                    EventBody::ApiStarted(_)
                        | EventBody::TurnBoundary(_)
                        | EventBody::TurnStats(_)
                        | EventBody::ExecutionControl(ExecutionControlEvent {
                            control_kind: ExecutionControlKind::TurnResumed,
                            ..
                        })
                )
        }) {
            return Err(DomainError::ContinuationNotEligible {
                reason: "terminal_boundary_superseded",
            });
        }
        if terminal.continuation_eligibility == ContinuationEligibility::Allowed {
            Ok(())
        } else {
            Err(DomainError::ContinuationNotEligible {
                reason: "terminal_policy_forbidden",
            })
        }
    }

    /// Starts a Goal auto-continue turn from a persisted goal-continuation
    /// system semantic, asserting exactly-once creation.
    /// 从持久化的 goal-continuation 系统语义启动 Goal 自动续跑回合，并保证
    /// 只创建一次。
    pub fn start_goal_continuation_turn(
        &mut self,
        objective: impl Into<String>,
        now_ms: i64,
    ) -> Result<TurnId, DomainError> {
        if let Some(turn) = &self.active_turn
            && !turn.closed
        {
            return Err(DomainError::GoalContinuationAlreadyActive {
                turn_id: turn.turn_id.value(),
            });
        }
        self.ensure_transition_capacity(1, 1)?;
        let mut staged_allocator = self.id_allocator;
        let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_events = Vec::with_capacity(1);
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::Provider,
            EventBody::SystemContext(SystemContextEvent {
                context_key: "goal_continuation".to_string(),
                operation: SystemContextOperation::Upsert,
                scope: SystemContextScope::Session,
                ttl_turns: None,
                provider_role_hint: None,
                content: objective.into(),
            }),
            now_ms,
        )?;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.active_turn = Some(ActiveTurnState {
            turn_id,
            origin: TurnOrigin::GoalContinuation,
            start_seq: seq,
            closed: false,
        });
        self.metadata.open_turn_start_seq = Some(seq);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[seq],
        );
        Ok(turn_id)
    }

    /// Atomically creates a new active Goal and its first provider-visible semantic.
    /// 原子创建一个新的活动 Goal 及其首条 Provider 可见语义。
    ///
    /// # Parameters
    /// 参数
    /// `objective` and `budget_tokens` are the validated Goal definition, while
    /// `provider_context` is the exact developer semantic rendered for that definition.
    /// `objective` 与 `budget_tokens` 是要持久化的 Goal 定义，`provider_context`
    /// 是为该定义生成的精确 developer 语义。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed context, notice, and optional continuation-turn identities.
    /// 返回已提交的上下文、通知与可选续跑回合身份。
    pub fn create_goal(
        &mut self,
        objective: impl Into<String>,
        budget_tokens: Option<u64>,
        provider_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        let (objective, provider_context) =
            validate_goal_definition(objective.into(), budget_tokens, provider_context.into())?;
        if self.metadata.mode.plan.enabled {
            return Err(DomainError::ModeConflict {
                requested: "goal",
                active: "plan",
            });
        }
        if self.metadata.mode.goal.enabled {
            return Err(DomainError::GoalEnablementMismatch {
                expected_enabled: false,
                actual_enabled: true,
            });
        }
        self.require_no_active_api_for_mode_transition()?;
        let needs_turn = self.active_turn.as_ref().is_none_or(|turn| turn.closed);
        self.ensure_transition_capacity(2, u64::from(needs_turn))?;

        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let mut staged_active_turn = self.active_turn.clone();
        let (turn_id, continuation_turn_id, context_key) = if needs_turn {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            (Some(turn_id), Some(turn_id), "goal_state")
        } else {
            (
                self.active_turn.as_ref().map(|turn| turn.turn_id),
                None,
                "goal_state",
            )
        };
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                context_key,
                SystemContextOperation::Upsert,
                provider_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_saved"),
            now_ms,
        )?;
        if let Some(turn_id) = continuation_turn_id {
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::GoalContinuation,
                start_seq: context_seq,
                closed: false,
            });
        }

        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal = GoalConfig {
            enabled: true,
            objective,
            budget_tokens,
            origin_turn_id: turn_id,
            origin_seq: Some(context_seq),
            state: GoalState::Active,
            budget_cache: None,
        };
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        if continuation_turn_id.is_some() {
            staged_metadata.open_turn_start_seq = Some(context_seq);
        }
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            staged_active_turn,
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id,
        })
    }

    /// Atomically saves a Goal by creating, updating, or replacing the completed generation.
    /// 根据当前状态原子创建、更新 Goal，或替换已完成的 Goal 代际。
    ///
    /// # Parameters
    /// 参数
    /// `objective`, `budget_tokens`, and `provider_context` describe the complete next Goal
    /// definition. `now_ms` is the explicit semantic-boundary timestamp.
    /// `objective`、`budget_tokens` 与 `provider_context` 描述完整的下一 Goal 定义；
    /// `now_ms` 是明确传入的语义边界时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `None` only for an exact update no-op. A completed Goal is always replaced by
    /// a new active generation whose new `origin_seq` is its stable projected Goal identity.
    /// 仅当更新完全无变化时返回 `None`。已完成 Goal 始终替换为新的活动代际，新的
    /// `origin_seq` 即其稳定投影 Goal 身份。
    pub fn save_goal(
        &mut self,
        objective: impl Into<String>,
        budget_tokens: Option<u64>,
        provider_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<Option<GoalModeTransition>, DomainError> {
        let objective = objective.into();
        let provider_context = provider_context.into();
        if !self.metadata.mode.goal.enabled {
            return self
                .create_goal(objective, budget_tokens, provider_context, now_ms)
                .map(Some);
        }
        if self.metadata.mode.goal.state != GoalState::Completed {
            return self.update_goal_definition(objective, budget_tokens, provider_context, now_ms);
        }
        let (objective, provider_context) =
            validate_goal_definition(objective, budget_tokens, provider_context)?;
        self.replace_completed_goal(objective, budget_tokens, provider_context, now_ms)
            .map(Some)
    }

    /// Replaces one completed Goal with a fully staged active generation.
    /// 使用完整暂存事务把一个已完成 Goal 替换为新的活动代际。
    fn replace_completed_goal(
        &mut self,
        objective: String,
        budget_tokens: Option<u64>,
        provider_context: String,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        self.require_goal_enabled(true)?;
        self.require_goal_state(GoalState::Completed, GoalState::Active)?;
        if self.metadata.mode.plan.enabled {
            return Err(DomainError::ModeConflict {
                requested: "goal",
                active: "plan",
            });
        }
        self.require_no_active_api_for_mode_transition()?;
        let needs_turn = self.active_turn.as_ref().is_none_or(|turn| turn.closed);
        self.ensure_transition_capacity(2, u64::from(needs_turn))?;

        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let mut staged_active_turn = self.active_turn.clone();
        let (turn_id, continuation_turn_id, context_key) = if needs_turn {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            (Some(turn_id), Some(turn_id), "goal_state")
        } else {
            (
                self.active_turn.as_ref().map(|turn| turn.turn_id),
                None,
                "goal_state",
            )
        };
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                context_key,
                SystemContextOperation::Upsert,
                provider_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_saved"),
            now_ms,
        )?;
        if let Some(turn_id) = continuation_turn_id {
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::GoalContinuation,
                start_seq: context_seq,
                closed: false,
            });
        }

        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal = GoalConfig {
            enabled: true,
            objective,
            budget_tokens,
            origin_turn_id: turn_id,
            origin_seq: Some(context_seq),
            state: GoalState::Active,
            budget_cache: None,
        };
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        if continuation_turn_id.is_some() {
            staged_metadata.open_turn_start_seq = Some(context_seq);
        }
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            staged_active_turn,
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id,
        })
    }

    /// Atomically updates the objective and budget of one existing Goal.
    /// 原子更新一个已存在 Goal 的目标内容与预算。
    ///
    /// # Returns
    /// 返回值
    /// Returns `None` for an exact no-op; otherwise returns the committed transition.
    /// 完全无变化时返回 `None`，否则返回已提交转换。
    pub fn update_goal_definition(
        &mut self,
        objective: impl Into<String>,
        budget_tokens: Option<u64>,
        provider_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<Option<GoalModeTransition>, DomainError> {
        let (objective, provider_context) =
            validate_goal_definition(objective.into(), budget_tokens, provider_context.into())?;
        self.require_goal_enabled(true)?;
        if self.metadata.mode.goal.state == GoalState::Completed {
            return Err(DomainError::GoalStateTransition {
                current: "completed",
                requested: "updated",
            });
        }
        if self.metadata.mode.goal.objective == objective
            && self.metadata.mode.goal.budget_tokens == budget_tokens
        {
            return Ok(None);
        }
        self.require_no_active_api_for_mode_transition()?;
        self.ensure_transition_capacity(2, 0)?;
        let turn_id = self.active_turn.as_ref().map(|turn| turn.turn_id);
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                "goal_state",
                SystemContextOperation::Upsert,
                provider_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_saved"),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal.objective = objective;
        staged_metadata.mode.goal.budget_tokens = budget_tokens;
        staged_metadata.mode.goal.budget_cache = None;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        if let Some(turn_id) = turn_id {
            staged_metadata.mode.goal.origin_turn_id = Some(turn_id);
            staged_metadata.mode.goal.origin_seq = Some(context_seq);
        }
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            self.active_turn.clone(),
            staged_events,
            now_ms,
        );
        Ok(Some(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id: None,
        }))
    }

    /// Atomically completes the active Goal and withdraws any undispatched continuation turn.
    /// 原子完成活动 Goal，并撤销任何尚未派发的续跑回合。
    ///
    /// # Parameters
    /// 参数
    /// `now_ms` is the explicit semantic-boundary timestamp used by every fact in this
    /// transition.
    /// `now_ms` 是本次转换中所有事实共同使用的显式语义边界时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `None` when the Goal is already complete. Otherwise returns the committed
    /// provider-context and UI-notice identities. Every validation and allocation happens
    /// against staged state, so an error leaves the complete document unchanged.
    /// Goal 已完成时返回 `None`；否则返回已提交的 Provider 上下文与 UI 通知身份。
    /// 所有校验与分配都针对暂存状态执行，因此任何错误都会保持完整文档不变。
    pub fn complete_goal(
        &mut self,
        now_ms: i64,
    ) -> Result<Option<GoalModeTransition>, DomainError> {
        self.require_goal_enabled(true)?;
        if self.metadata.mode.goal.state == GoalState::Completed {
            return Ok(None);
        }

        let undispatched_continuation = self.active_turn.as_ref().filter(|turn| {
            !turn.closed
                && turn.origin == TurnOrigin::GoalContinuation
                && !self.events.iter().any(|event| {
                    event.turn_id == Some(turn.turn_id)
                        && matches!(event.event, EventBody::ApiStarted(_))
                })
        });
        let closure_buckets = undispatched_continuation
            .map(|turn| self.aggregate_turn_buckets(turn.turn_id, turn.start_seq))
            .transpose()?;
        let appended_event_count = if undispatched_continuation.is_some() {
            4
        } else {
            2
        };
        self.ensure_transition_capacity(appended_event_count, 0)?;

        let turn_id = self.active_turn.as_ref().map(|turn| turn.turn_id);
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(appended_event_count as usize);
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                "goal_state",
                SystemContextOperation::Remove,
                String::from(
                    "The active goal is complete; automatic continuation is no longer active.",
                ),
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_completed"),
            now_ms,
        )?;

        let mut staged_active_turn = self.active_turn.clone();
        let mut staged_metadata = self.metadata.clone();
        if let (Some(turn), Some(buckets)) = (undispatched_continuation, closure_buckets) {
            self.stage_turn_closure(
                &mut staged_allocator,
                &mut staged_events,
                turn.turn_id,
                turn.start_seq,
                TurnOutcome::Stopped,
                0,
                now_ms,
                buckets,
            )?;
            let mut closed_turn = turn.clone();
            closed_turn.closed = true;
            staged_active_turn = Some(closed_turn);
            staged_metadata.open_turn_start_seq = None;
        }
        staged_metadata.mode.goal.state = GoalState::Completed;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            staged_active_turn,
            staged_events,
            now_ms,
        );
        Ok(Some(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id: None,
        }))
    }

    /// Atomically clears one existing Goal and records its explicit exit semantic.
    /// 原子清除一个已存在的 Goal，并记录其明确退出语义。
    pub fn clear_goal(
        &mut self,
        removal_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        self.require_goal_enabled(true)?;
        self.require_no_active_api_for_mode_transition()?;
        let removal_context = validate_system_context_content(removal_context.into())?;
        self.ensure_transition_capacity(2, 0)?;
        let turn_id = self.active_turn.as_ref().map(|turn| turn.turn_id);
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                "goal_state",
                SystemContextOperation::Remove,
                removal_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_cleared"),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal = GoalConfig::default();
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            self.active_turn.clone(),
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id: None,
        })
    }

    /// Atomically pauses an active Goal and records its explicit exit semantic.
    /// 原子暂停一个活动 Goal，并记录其明确退出语义。
    pub fn pause_goal(
        &mut self,
        removal_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        self.require_goal_enabled(true)?;
        self.require_goal_state(GoalState::Active, GoalState::Paused)?;
        self.require_no_active_api_for_mode_transition()?;
        let removal_context = validate_system_context_content(removal_context.into())?;
        self.ensure_transition_capacity(2, 0)?;
        let turn_id = self.active_turn.as_ref().map(|turn| turn.turn_id);
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                "goal_state",
                SystemContextOperation::Remove,
                removal_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_paused"),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal.state = GoalState::Paused;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            self.active_turn.clone(),
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id: None,
        })
    }

    /// Atomically resumes a paused or system-stopped Goal with a fresh context semantic.
    /// 使用新的上下文语义原子恢复已暂停或系统停止的 Goal。
    pub fn resume_goal(
        &mut self,
        objective: impl Into<String>,
        budget_tokens: Option<u64>,
        provider_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        let (objective, provider_context) =
            validate_goal_definition(objective.into(), budget_tokens, provider_context.into())?;
        self.require_goal_enabled(true)?;
        if self.metadata.mode.plan.enabled {
            return Err(DomainError::ModeConflict {
                requested: "goal",
                active: "plan",
            });
        }
        if matches!(
            self.metadata.mode.goal.state,
            GoalState::Active | GoalState::Completed
        ) {
            return Err(DomainError::GoalStateTransition {
                current: goal_state_name(self.metadata.mode.goal.state),
                requested: "active",
            });
        }
        self.require_no_active_api_for_mode_transition()?;
        let needs_turn = self.active_turn.as_ref().is_none_or(|turn| turn.closed);
        self.ensure_transition_capacity(2, u64::from(needs_turn))?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let mut staged_active_turn = self.active_turn.clone();
        let (turn_id, continuation_turn_id, context_key) = if needs_turn {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            (Some(turn_id), Some(turn_id), "goal_state")
        } else {
            (
                self.active_turn.as_ref().map(|turn| turn.turn_id),
                None,
                "goal_state",
            )
        };
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                context_key,
                SystemContextOperation::Upsert,
                provider_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_resumed"),
            now_ms,
        )?;
        if let Some(turn_id) = continuation_turn_id {
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::GoalContinuation,
                start_seq: context_seq,
                closed: false,
            });
        }
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal.objective = objective;
        staged_metadata.mode.goal.budget_tokens = budget_tokens;
        staged_metadata.mode.goal.origin_turn_id = turn_id;
        staged_metadata.mode.goal.origin_seq = Some(context_seq);
        staged_metadata.mode.goal.state = GoalState::Active;
        staged_metadata.mode.goal.budget_cache = None;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        if continuation_turn_id.is_some() {
            staged_metadata.open_turn_start_seq = Some(context_seq);
        }
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            staged_active_turn,
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id,
        })
    }

    /// Atomically changes Plan mode and records the matching provider-visible semantic.
    /// 原子切换 Plan 模式，并记录与之匹配的 Provider 可见语义。
    pub fn set_plan_mode(
        &mut self,
        input: PlanModeTransitionInput,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        if input.enabled && self.metadata.mode.goal.enabled {
            return Err(DomainError::ModeConflict {
                requested: "plan",
                active: "goal",
            });
        }
        if self.metadata.mode.plan.enabled == input.enabled {
            return Ok(false);
        }
        self.require_no_active_api_for_mode_transition()?;
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let turn_id = self.active_turn.as_ref().map(|turn| turn.turn_id);
        self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            EventBody::SystemContext(SystemContextEvent {
                context_key: String::from("plan_mode"),
                operation: if input.enabled {
                    SystemContextOperation::Upsert
                } else {
                    SystemContextOperation::Remove
                },
                scope: SystemContextScope::Session,
                ttl_turns: None,
                provider_role_hint: Some(SystemRoleHint::Developer),
                content: input.provider_context,
            }),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.plan.enabled = input.enabled;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
        Ok(true)
    }

    /// Closes the open turn with one boundary followed by the unique terminal
    /// statistic, both committed by the same flush.
    /// 用一个边界与唯一终态统计闭合开放回合，两者由同一次 flush 提交。
    pub fn close_turn(
        &mut self,
        outcome: TurnOutcome,
        duration_ms: i64,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        let turn = self.require_open_turn()?;
        if let Some(api) = self.active_api.as_ref() {
            return Err(DomainError::TurnCompletionWhileApiActive {
                turn_id: turn.turn_id.value(),
                api_call_id: api.api_call_id.value(),
            });
        }
        let turn_id = turn.turn_id;
        let start_seq = turn.start_seq;
        self.ensure_transition_capacity(2, 0)?;
        let buckets = self.aggregate_turn_buckets(turn_id, start_seq)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let final_seq = self.stage_turn_closure(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            start_seq,
            outcome,
            duration_ms,
            now_ms,
            buckets,
        )?;
        let mut staged_active_turn = turn.clone();
        staged_active_turn.closed = true;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.open_turn_start_seq = None;
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.active_turn = Some(staged_active_turn);
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
        Ok(final_seq)
    }

    /// Stages one complete boundary/statistics pair without publishing owner state.
    /// 暂存一组完整的边界/统计事件，不发布 owner 状态。
    #[allow(clippy::too_many_arguments)]
    fn stage_turn_closure(
        &self,
        staged_allocator: &mut IdAllocator,
        staged_events: &mut Vec<ConversationEvent>,
        turn_id: TurnId,
        start_seq: Seq,
        outcome: TurnOutcome,
        duration_ms: i64,
        now_ms: i64,
        buckets: TurnStatBuckets,
    ) -> Result<Seq, DomainError> {
        self.stage_event(
            staged_allocator,
            staged_events,
            Some(turn_id),
            EventVisibility::UiOnly,
            EventBody::TurnBoundary(TurnBoundaryEvent { outcome }),
            now_ms,
        )?;
        // The boundary event just took the last seq; the turn_stats event will
        // take exactly the next one, so the covered end seq includes the
        // statistics event itself (plan 12.21.2 "覆盖的起止 seq").
        // 边界事件刚刚取得最后一个 seq；turn_stats 事件将精确取得下一个，因此
        // 覆盖结束 seq 包含统计事件自身(计划 12.21.2"覆盖的起止 seq")。
        let end_seq = Seq::try_new(staged_allocator.frontier().1)?;
        self.stage_event(
            staged_allocator,
            staged_events,
            Some(turn_id),
            EventVisibility::UiOnly,
            EventBody::TurnStats(TurnStatsEvent {
                start_seq,
                end_seq,
                outcome,
                duration_ms: Some(duration_ms),
                buckets,
            }),
            now_ms,
        )?;
        Ok(end_seq)
    }

    /// Completes the expected open turn and atomically selects its sole queued, Goal, or absent successor.
    /// 完成指定开放回合，并原子选择唯一的排队输入、Goal 或无后继结果。
    ///
    /// `expected_turn_id` is the receipt-free exactly-once key: after a
    /// successful transition the current open turn is either absent or has a
    /// different id, so replaying the same transition cannot close or create a
    /// second turn. All seq/id capacity and ownership checks run before the
    /// first event is appended.
    /// `expected_turn_id` 是无 receipt 的 exactly-once 键：转换成功后，当前开放
    /// 回合要么不存在，要么具有不同 id，因此重放同一转换不能再次闭合或创建
    /// 回合。所有 seq/id 容量与归属校验都在追加首个事件前完成。
    pub fn complete_turn_and_select_successor(
        &mut self,
        expected_turn_id: TurnId,
        now_ms: i64,
    ) -> Result<TurnCompletionTransition, DomainError> {
        let turn = self.require_open_turn()?.clone();
        if turn.turn_id != expected_turn_id {
            return Err(DomainError::TurnIdentityMismatch {
                expected: expected_turn_id.value(),
                actual: turn.turn_id.value(),
            });
        }
        if let Some(api) = self.active_api.as_ref() {
            return Err(DomainError::TurnCompletionWhileApiActive {
                turn_id: expected_turn_id.value(),
                api_call_id: api.api_call_id.value(),
            });
        }
        self.require_final_assistant(expected_turn_id)?;
        let turn_start_event = self
            .events
            .iter()
            .find(|event| event.seq == turn.start_seq)
            .ok_or(DomainError::ReferenceViolation {
                kind: "turn_start_event",
                value: turn.start_seq.value(),
            })?;
        if turn_start_event.turn_id != Some(expected_turn_id) {
            return Err(DomainError::TurnIdentityMismatch {
                expected: expected_turn_id.value(),
                actual: turn_start_event.turn_id.map_or(0, TurnId::value),
            });
        }
        // Logical Turn duration is derived from the authoritative first event so continuation,
        // process restart, and multiple physical APIs never reset the elapsed interval.
        // 逻辑回合耗时从权威首事件推导，使继续、进程重启和多个物理 API 永不重置耗时区间。
        let duration_ms = now_ms.saturating_sub(turn_start_event.created_at_ms).max(0);

        let queued_successor = self.metadata.input_state.queued_inputs.first().cloned();
        if let Some(queued) = queued_successor.as_ref() {
            validate_user_input(queued.text.clone(), &queued.resource_ids)?;
        }

        let goal_budget = self.aggregate_goal_budget(now_ms)?;
        let goal_stats = TurnStatsEvent {
            start_seq: self.metadata.mode.goal.origin_seq.unwrap_or(turn.start_seq),
            end_seq: self.last_seq()?,
            outcome: TurnOutcome::Completed,
            duration_ms: None,
            buckets: goal_budget
                .as_ref()
                .map(|(_, buckets)| buckets.clone())
                .unwrap_or_default(),
        };
        let decision = decide_goal_continuation(&self.metadata.mode.goal, &goal_stats);
        let next_goal_state = match decision.reason {
            GoalStopReason::BudgetExhausted { .. } => Some(GoalState::BudgetExhausted),
            GoalStopReason::UnknownUsage => Some(GoalState::Blocked),
            _ => None,
        };
        let goal_objective = (queued_successor.is_none() && decision.should_continue)
            .then(|| self.metadata.mode.goal.objective.clone());
        let has_successor = queued_successor.is_some() || goal_objective.is_some();
        let appended_event_count = if has_successor { 3 } else { 2 };
        self.ensure_transition_capacity(appended_event_count, u64::from(has_successor))?;
        let buckets = self.aggregate_turn_buckets(expected_turn_id, turn.start_seq)?;
        // Stage the boundary, the unique turn statistics, and the optional successor before
        // publishing any owner state. This keeps final-assistant completion exactly-once and
        // failure-atomic even at allocator exhaustion.
        // 在发布任何 owner 状态前暂存边界、唯一回合统计及可选后继，使最终 assistant
        // 完成即使遇到分配器耗尽也保持 exactly-once 与失败原子性。
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(appended_event_count as usize);
        let completed_turn_final_seq = self.stage_turn_closure(
            &mut staged_allocator,
            &mut staged_events,
            expected_turn_id,
            turn.start_seq,
            TurnOutcome::Completed,
            duration_ms,
            now_ms,
            buckets,
        )?;
        let mut staged_metadata = self.metadata.clone();
        let mut completed_turn = turn;
        completed_turn.closed = true;
        let mut staged_active_turn = Some(completed_turn);
        if staged_metadata.mode.goal.enabled {
            staged_metadata.mode.goal.budget_cache =
                goal_budget.map(|(cache, _)| GoalBudgetCache {
                    covered_until_seq: completed_turn_final_seq,
                    ..cache
                });
            if let Some(state) = next_goal_state {
                staged_metadata.mode.goal.state = state;
            }
        }
        let successor = if let Some(queued) = queued_successor {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            let input_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    client_action_id: queued.client_action_id.clone(),
                    input_id: queued.input_id,
                    input_source: UserInputSource::QueuedScheduled,
                    text: queued.text,
                    resource_ids: queued.resource_ids,
                }),
                now_ms,
            )?;
            staged_metadata.input_state.queued_inputs.remove(0);
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::UserRoot,
                start_seq: input_seq,
                closed: false,
            });
            staged_metadata.open_turn_start_seq = Some(input_seq);
            TurnSuccessor::QueuedUser {
                turn_id,
                input_id: queued.input_id,
                client_action_id: queued.client_action_id,
                input_seq,
            }
        } else if let Some(objective) = goal_objective {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            let start_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::SystemContext(SystemContextEvent {
                    context_key: "goal_state".to_string(),
                    operation: SystemContextOperation::Upsert,
                    scope: SystemContextScope::Session,
                    ttl_turns: None,
                    provider_role_hint: None,
                    content: objective,
                }),
                now_ms,
            )?;
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::GoalContinuation,
                start_seq,
                closed: false,
            });
            staged_metadata.open_turn_start_seq = Some(start_seq);
            TurnSuccessor::GoalContinuation {
                turn_id,
                context_seq: start_seq,
            }
        } else {
            staged_metadata.open_turn_start_seq = None;
            TurnSuccessor::None
        };
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.active_turn = staged_active_turn;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
        Ok(TurnCompletionTransition {
            completed_turn_final_seq,
            successor,
        })
    }

    /// Appends one dynamic system-context semantic.
    /// 追加一条动态系统上下文语义。
    #[allow(clippy::too_many_arguments)]
    pub fn append_system_context(
        &mut self,
        turn_id: Option<TurnId>,
        context_key: impl Into<String>,
        operation: SystemContextOperation,
        scope: SystemContextScope,
        ttl_turns: Option<u64>,
        provider_role_hint: Option<SystemRoleHint>,
        content: impl Into<String>,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        let content = validate_system_context_content(content.into())?;
        self.append_event(
            turn_id,
            EventVisibility::Provider,
            EventBody::SystemContext(SystemContextEvent {
                context_key: context_key.into(),
                operation,
                scope,
                ttl_turns,
                provider_role_hint,
                content,
            }),
            now_ms,
        )
    }

    /// Appends one permanent UI timeline note.
    /// 追加一条永久 UI 时间线通知。
    #[allow(clippy::too_many_arguments)]
    pub fn append_display_note(
        &mut self,
        turn_id: Option<TurnId>,
        notice_kind: DisplayNoticeKind,
        severity: NoticeSeverity,
        reason_code: impl Into<String>,
        redacted_detail: Option<String>,
        api_call_id: Option<ApiCallId>,
        retry_count: Option<u32>,
        retry_limit: Option<u32>,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        self.append_event(
            turn_id,
            EventVisibility::UiOnly,
            EventBody::DisplayNote(DisplayNoteEvent {
                notice_kind,
                severity,
                reason_code: reason_code.into(),
                redacted_detail,
                api_call_id,
                retry_count,
                retry_limit,
            }),
            now_ms,
        )
    }

    /// Appends one Provider-invisible execution control fact.
    /// 追加一条 Provider 不可见执行控制事实。
    pub fn append_execution_control(
        &mut self,
        turn_id: Option<TurnId>,
        control_kind: ExecutionControlKind,
        api_call_id: Option<ApiCallId>,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        self.append_event(
            turn_id,
            EventVisibility::HostOnly,
            EventBody::ExecutionControl(ExecutionControlEvent {
                control_kind,
                api_call_id,
                client_action_id: None,
            }),
            now_ms,
        )
    }

    /// Builds one exact parent-return input from a fully durable child document.
    /// 从一个完全持久化的子会话文档构建精确的父回流输入。
    pub fn build_subagent_return_input(
        &self,
        request: SubagentReturnBuildRequest,
    ) -> Result<SubagentReturnInput, DomainError> {
        if self.memory_revision != self.durable_revision {
            return Err(DomainError::SubagentSourceNotDurable {
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        validate_exact_external_identity("child_session_id", &self.metadata.session_id)?;
        let terminal_stats: Vec<(&ConversationEvent, &TurnStatsEvent)> = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::TurnStats(stats) if event.turn_id == Some(request.final_turn_id) => {
                    Some((event, stats))
                }
                _ => None,
            })
            .collect();
        if terminal_stats.len() != 1 {
            return Err(DomainError::InvalidStatistics {
                field: "child_turn_stats_unique",
            });
        }
        let (stats_event, stats) = terminal_stats[0];
        validate_turn_stat_buckets(&stats.buckets)?;
        if stats.end_seq != stats_event.seq
            || stats.start_seq.value() > stats.end_seq.value()
            || stats.start_seq.value() < self.resident_start_seq
        {
            return Err(DomainError::InvalidStatistics {
                field: "child_turn_stats_range",
            });
        }
        if request.source_assistant_text_seq.value() < stats.start_seq.value()
            || request.source_assistant_text_seq.value() > stats.end_seq.value()
        {
            return Err(DomainError::InvalidStatistics {
                field: "source_assistant_text_seq",
            });
        }
        if self.events.iter().any(|event| {
            (stats.start_seq.value()..=stats.end_seq.value()).contains(&event.seq.value())
                && event.turn_id != Some(request.final_turn_id)
        }) {
            return Err(DomainError::InvalidStatistics {
                field: "child_source_range_turn",
            });
        }
        let source_text = self
            .events
            .iter()
            .find(|event| event.seq == request.source_assistant_text_seq)
            .and_then(|event| match &event.event {
                EventBody::AssistantText(text)
                    if event.turn_id == Some(request.final_turn_id)
                        && text.status == AssistantSemanticStatus::Completed =>
                {
                    Some(text)
                }
                _ => None,
            })
            .ok_or(DomainError::InvalidStatistics {
                field: "source_assistant_text",
            })?;
        let latest_api_stats = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::ApiStats(api_stats)
                    if event.turn_id == Some(request.final_turn_id)
                        && event.seq.value() < stats_event.seq.value() =>
                {
                    Some((event.seq, api_stats))
                }
                _ => None,
            })
            .max_by_key(|(seq, _)| seq.value())
            .map(|(_, api_stats)| api_stats)
            .ok_or(DomainError::InvalidStatistics {
                field: "child_final_api_stats",
            })?;
        if latest_api_stats.status != ApiCallStatus::Completed
            || source_text.api_call_id != latest_api_stats.api_call_id
            || self.events.iter().any(|event| {
                matches!(
                    &event.event,
                    EventBody::ToolCall(tool)
                        if tool.api_call_id == latest_api_stats.api_call_id
                )
            })
        {
            return Err(DomainError::InvalidStatistics {
                field: "child_final_assistant",
            });
        }
        Ok(SubagentReturnInput {
            child_session_id: self.metadata.session_id.clone(),
            child_task_id: request.child_task_id,
            content: source_text.text.clone(),
            source_assistant_text_seq: request.source_assistant_text_seq,
            source_from_seq: stats.start_seq,
            source_to_seq: stats.end_seq,
            child_turn_stats_seq: stats_event.seq,
            totals: stats.buckets.total.clone(),
        })
    }

    /// Atomically appends one final subagent body and its complete inclusive statistics.
    /// 原子追加一份最终子代理正文及其完整包容性统计。
    pub fn append_subagent_return_with_stats(
        &mut self,
        input: SubagentReturnInput,
        now_ms: i64,
    ) -> Result<IdempotentMutation<SubagentReturnReceipt>, DomainError> {
        let parent_turn_id = self.require_open_turn()?.turn_id;
        if let Some(receipt) = self.existing_subagent_return(parent_turn_id, &input)? {
            return Ok(IdempotentMutation::Idempotent(receipt));
        }
        self.ensure_transition_capacity(2, 0)?;
        let returned = SubagentReturnEvent {
            child_session_id: input.child_session_id.clone(),
            child_task_id: input.child_task_id.clone(),
            content: input.content,
            source_assistant_text_seq: input.source_assistant_text_seq,
            source_from_seq: input.source_from_seq,
            source_to_seq: input.source_to_seq,
            child_turn_stats_seq: input.child_turn_stats_seq,
        };
        let stats = SubagentReturnStatsEvent {
            child_session_id: input.child_session_id.clone(),
            child_task_id: input.child_task_id.clone(),
            child_turn_stats_seq: input.child_turn_stats_seq,
            status: SubagentReturnStatus::Final,
            totals: input.totals,
        };
        validate_subagent_return_pair(&returned, &stats)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let return_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(parent_turn_id),
            EventVisibility::Provider,
            EventBody::SubagentReturn(returned),
            now_ms,
        )?;
        let stats_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(parent_turn_id),
            EventVisibility::UiOnly,
            EventBody::SubagentReturnStats(stats),
            now_ms,
        )?;
        let changed_event_seqs = vec![return_seq, stats_seq];
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &changed_event_seqs);
        Ok(IdempotentMutation::Committed(SubagentReturnReceipt {
            parent_turn_id,
            return_seq,
            stats_seq,
            child_session_id: input.child_session_id,
            child_task_id: input.child_task_id,
            child_turn_stats_seq: input.child_turn_stats_seq,
            source_assistant_text_seq: input.source_assistant_text_seq,
        }))
    }

    /// Returns the latest durable subagent return not yet covered by a later physical API.
    /// 返回尚未被后续物理 API 覆盖的最新可靠子代理回流。
    pub fn pending_subagent_return_dispatch(
        &self,
    ) -> Result<Option<SubagentReturnReceipt>, DomainError> {
        let Some(active_turn) = self.active_turn.as_ref().filter(|turn| !turn.closed) else {
            return Ok(None);
        };
        let latest_api_started_seq = self
            .events
            .iter()
            .filter_map(|event| {
                (event.turn_id == Some(active_turn.turn_id)
                    && matches!(&event.event, EventBody::ApiStarted(_)))
                .then_some(event.seq)
            })
            .max_by_key(|seq| seq.value());
        let Some((return_index, return_event, returned)) = self
            .events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match &event.event {
                EventBody::SubagentReturn(returned)
                    if event.turn_id == Some(active_turn.turn_id)
                        && latest_api_started_seq
                            .is_none_or(|started_seq| event.seq.value() > started_seq.value()) =>
                {
                    Some((index, event, returned))
                }
                _ => None,
            })
            .next_back()
        else {
            return Ok(None);
        };
        let stats_event =
            self.events
                .get(return_index + 1)
                .ok_or(DomainError::InvalidStatistics {
                    field: "subagent_return_stats_pair",
                })?;
        let EventBody::SubagentReturnStats(stats) = &stats_event.event else {
            return Err(DomainError::InvalidStatistics {
                field: "subagent_return_stats_pair",
            });
        };
        validate_subagent_return_pair(returned, stats)?;
        if return_event.turn_id != stats_event.turn_id
            || return_event.seq.value().checked_add(1) != Some(stats_event.seq.value())
        {
            return Err(DomainError::InvalidStatistics {
                field: "subagent_return_stats_pair",
            });
        }
        Ok(Some(SubagentReturnReceipt {
            parent_turn_id: active_turn.turn_id,
            return_seq: return_event.seq,
            stats_seq: stats_event.seq,
            child_session_id: returned.child_session_id.clone(),
            child_task_id: returned.child_task_id.clone(),
            child_turn_stats_seq: returned.child_turn_stats_seq,
            source_assistant_text_seq: returned.source_assistant_text_seq,
        }))
    }

    /// Records one compaction node and advances the runtime pointer.
    /// 记录一个压缩节点并推进 runtime 指针。
    ///
    /// Node appends first; the runtime pointer is published separately by the
    /// writer so a crash between the two files keeps the old generation valid.
    /// 节点先追加；runtime 指针由写入器单独发布，使两文件之间的崩溃保持旧
    /// 代次仍然有效。
    pub fn record_compaction(
        &mut self,
        input: CompactionCommitInput,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CompactionId>, DomainError> {
        if self.runtime_lock.initialization_state != RuntimeInitializationState::Initialized {
            return Err(DomainError::RuntimeBaselineUninitialized);
        }
        let CompactionCommitInput {
            api_call_id,
            occurred_at_seq,
            retained_from_seq,
            summary,
            locked_baseline,
        } = input;
        let locked_sources = locked_baseline.into_locked_sources();
        let started = self
            .events
            .iter()
            .find_map(|event| match &event.event {
                EventBody::ApiStarted(started) if started.api_call_id == api_call_id => {
                    Some(started)
                }
                _ => None,
            })
            .ok_or_else(|| DomainError::reference("compaction_api_started", api_call_id.value()))?;
        ensure_api_request_field(
            "compaction.purpose",
            &ApiPurposeKind::Compaction,
            &started.purpose,
        )?;
        let (api_stats_seq, api_stats) = self
            .events
            .iter()
            .find_map(|event| match &event.event {
                EventBody::ApiStats(stats) if stats.api_call_id == api_call_id => {
                    Some((event.seq, stats))
                }
                _ => None,
            })
            .ok_or_else(|| DomainError::reference("compaction_api_stats", api_call_id.value()))?;
        ensure_api_request_field(
            "compaction.status",
            &ApiCallStatus::Completed,
            &api_stats.status,
        )?;
        let last_provider_event = self
            .events
            .iter()
            .filter(|event| {
                group_event_api_id(event) == Some(api_call_id.value())
                    && matches!(
                        &event.event,
                        EventBody::AssistantReasoning(_)
                            | EventBody::AssistantText(_)
                            | EventBody::ToolCall(_)
                    )
            })
            .max_by_key(|event| event.seq.value())
            .ok_or_else(|| DomainError::reference("compaction_summary", api_call_id.value()))?;
        if last_provider_event.seq != occurred_at_seq {
            return Err(DomainError::SequenceViolation {
                expected: last_provider_event.seq.value(),
                actual: occurred_at_seq.value(),
            });
        }
        let persisted_summary = match &last_provider_event.event {
            EventBody::AssistantText(text) if text.status == AssistantSemanticStatus::Completed => {
                &text.text
            }
            EventBody::AssistantReasoning(reasoning)
                if reasoning.status == AssistantSemanticStatus::Completed =>
            {
                &reasoning.text
            }
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "summary_source",
                });
            }
        };
        if persisted_summary != &summary {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "compaction",
                field: "summary",
            });
        }
        if retained_from_seq.value() > occurred_at_seq.value()
            || occurred_at_seq.value() >= api_stats_seq.value()
        {
            return Err(DomainError::SequenceViolation {
                expected: occurred_at_seq.value(),
                actual: retained_from_seq.value().max(api_stats_seq.value()),
            });
        }
        if let Some(existing) = self
            .compactions
            .iter()
            .find(|node| node.api_call_id == api_call_id)
        {
            if existing.api_stats_seq != api_stats_seq
                || existing.occurred_at_seq != occurred_at_seq
                || existing.retained_from_seq != retained_from_seq
                || existing.summary != summary
            {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "existing_node",
                });
            }
            if self.runtime_lock.current_compaction_id != Some(existing.compaction_id) {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "current_compaction_id",
                });
            }
            if self.runtime_lock.locked_sources != locked_sources {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "locked_baseline",
                });
            }
            return Ok(IdempotentMutation::Idempotent(existing.compaction_id));
        }
        // The next formal node write first drops unpublished candidates left
        // over from a failed runtime write, so an in-process failure can never
        // cause the next compaction to append on top of them (plan 6.2/5.8.3,
        // audit P0#7). Only nodes at or before the durably published pointer
        // survive; the loader ignores but never deletes on-disk candidates.
        // 下一次正式节点写入先把 runtime 写失败残留的未发布候选裁剪掉，同进程
        // 失败绝不可能使下一次压缩基于它们继续追加(计划 6.2/5.8.3、审计
        // P0#7)。只有已持久化指针之前(含)的节点保留；加载器只忽略、绝不删除
        // 磁盘候选。
        let mut staged_allocator = self.id_allocator;
        let compaction_id = CompactionId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_compactions =
            trim_unpublished_candidates(&self.compactions, self.published_compaction_id);
        staged_compactions.push(CompactionNode {
            compaction_id,
            api_call_id,
            api_stats_seq,
            occurred_at_seq,
            retained_from_seq,
            summary,
            created_at_ms: now_ms,
        });
        let mut staged_runtime_lock = self.runtime_lock.clone();
        staged_runtime_lock.current_compaction_id = Some(compaction_id);
        staged_runtime_lock.locked_sources = locked_sources;
        self.id_allocator = staged_allocator;
        self.compactions = staged_compactions;
        self.runtime_lock = staged_runtime_lock;
        self.bump_revision(&[DirtyFile::Compactions, DirtyFile::Runtime], now_ms, &[]);
        Ok(IdempotentMutation::Committed(compaction_id))
    }

    /// Replaces the locked sources of the current context generation.
    /// 替换当前上下文代次的锁定来源。
    pub fn initialize_runtime_baseline(
        &mut self,
        baseline: LockedBaselineProjection,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let sources = baseline.into_locked_sources();
        match self.runtime_lock.initialization_state {
            RuntimeInitializationState::FreshUninitialized
            | RuntimeInitializationState::BaselineRefreshPending => {
                if !self.runtime_lock.locked_sources.is_empty() {
                    return Err(DomainError::InvalidRuntimeInitializationState {
                        field: "locked_sources",
                    });
                }
                if self.runtime_lock.initialization_state
                    == RuntimeInitializationState::FreshUninitialized
                    && self.runtime_lock.current_compaction_id.is_some()
                {
                    return Err(DomainError::InvalidRuntimeInitializationState {
                        field: "current_compaction_id",
                    });
                }
                self.runtime_lock.initialization_state = RuntimeInitializationState::Initialized;
                self.runtime_lock.locked_sources = sources;
                self.bump_revision(&[DirtyFile::Runtime], now_ms, &[]);
                Ok(true)
            }
            RuntimeInitializationState::Initialized
                if self.runtime_lock.locked_sources == sources =>
            {
                Ok(false)
            }
            RuntimeInitializationState::Initialized => {
                Err(DomainError::RuntimeBaselineAlreadyInitialized)
            }
        }
    }

    /// Rebuilds all derived state (indexes, active turn/API, allocator
    /// frontier) from the resident events, strictly validating the stream.
    /// 从常驻事件重建全部派生状态(索引、活动回合/API、分配器 frontier)，
    /// 并严格校验事件流。
    ///
    /// This is the loader's single validation entry: seq contiguity, API-group
    /// ordering, `provider_source` uniqueness/placement, and allocator frontier
    /// reconstruction. It never rewrites or repairs anything.
    /// 这是加载器的唯一校验入口：seq 连续性、API 组顺序、`provider_source`
    /// 唯一性与位置、分配器 frontier 重建。它绝不重写或修复任何内容。
    pub fn rebuild_derived_state(&mut self) -> Result<(), DomainError> {
        let mut tool_seq_by_id = HashMap::new();
        let mut max_seq = self.resident_start_seq.saturating_sub(1);
        let prefix_next_event_local_id = self
            .segment_catalog
            .iter()
            .take_while(|entry| entry.start_seq.value() < self.resident_start_seq)
            .map(|entry| entry.next_event_local_id)
            .last()
            .unwrap_or(1);
        if prefix_next_event_local_id == 0 {
            return Err(DomainError::invalid_id(
                "next_event_local_id",
                prefix_next_event_local_id,
            ));
        }
        let mut max_local_id = prefix_next_event_local_id - 1;
        let mut turn_ends: HashMap<u64, u64> = HashMap::new();

        // Non-event documents also own local ids. They are facts, not caches,
        // and therefore participate in allocator restoration.
        // 非事件文档同样持有本地 id；它们是事实而非缓存，因此必须参与分配器恢复。
        for compaction in &self.compactions {
            max_local_id = max_local_id
                .max(compaction.compaction_id.value())
                .max(compaction.api_call_id.value());
        }
        if let Some(compaction_id) = self.runtime_lock.current_compaction_id {
            max_local_id = max_local_id.max(compaction_id.value());
        }
        for pending in &self.metadata.input_state.pending_in_turn_inputs {
            max_local_id = max_local_id
                .max(pending.input_id.value())
                .max(pending.turn_id.value());
        }
        for queued in &self.metadata.input_state.queued_inputs {
            max_local_id = max_local_id.max(queued.input_id.value());
        }
        if let Some(identity) = &self.metadata.input_state.draft.restored_from_queue {
            max_local_id = max_local_id.max(identity.input_id.value());
        }

        for (index, event) in self.events.iter().enumerate() {
            let seq_value = event.seq.value();
            let expected = self.resident_start_seq + index as u64;
            if seq_value != expected {
                return Err(DomainError::SequenceViolation {
                    expected,
                    actual: seq_value,
                });
            }
            if !(crate::v1::event::EVENT_REVISION_INITIAL..=JS_SAFE_INTEGER_MAX)
                .contains(&event.event_revision)
            {
                return Err(DomainError::invalid_id(
                    "event_revision",
                    event.event_revision,
                ));
            }
            max_seq = max_seq.max(seq_value);
            max_local_id = max_local_id.max(event.max_referenced_local_id());
            if let EventBody::SystemContext(context) = &event.event {
                validate_system_context_event(context)?;
            }
            if let EventBody::ExecutionControl(control) = &event.event {
                validate_execution_control_event(control)?;
            }
            if let EventBody::TurnStats(stats) = &event.event {
                validate_turn_stat_buckets(&stats.buckets)?;
            }
            if let Some(turn_id) = event.turn_id
                && let EventBody::TurnBoundary(_) = &event.event
            {
                turn_ends.insert(turn_id.value(), seq_value);
            }
            if let EventBody::ToolCall(tool) = &event.event {
                validate_tool_lifecycle_shape(tool)?;
                if tool_seq_by_id
                    .insert(tool.tool_call_id, event.seq)
                    .is_some()
                {
                    return Err(DomainError::ReferenceViolation {
                        kind: "tool_call_id",
                        value: tool.tool_call_id.value(),
                    });
                }
            }
        }
        validate_subagent_return_history(&self.events)?;
        validate_queued_input_resolution_history(&self.metadata, &self.events)?;
        validate_client_action_history(&self.metadata, &self.events)?;
        validate_api_dispatch_history(&self.events)?;

        // Validate API-group ordering and provider_source placement.
        let mut group_ranks: BTreeMap<u64, Vec<(usize, usize)>> = BTreeMap::new();
        for (index, event) in self.events.iter().enumerate() {
            if let Some(api_id) = group_event_api_id(event) {
                group_ranks
                    .entry(api_id)
                    .or_default()
                    .push((index, group_rank(&event.event)));
            }
        }
        let mut last_open_api: Option<(u64, bool)> = None;
        for (api_id, mut entries) in group_ranks {
            entries.sort_by_key(|(index, _)| *index);
            let mut previous_rank: Option<usize> = None;
            let mut stats_rank = usize::MAX;
            let mut started_count = 0usize;
            let mut last_provider_index: Option<usize> = None;
            let mut provider_source_index: Option<usize> = None;
            for (index, rank) in entries {
                if let Some(before) = previous_rank
                    && rank < before
                {
                    return Err(DomainError::SequenceViolation {
                        expected: before as u64,
                        actual: rank as u64,
                    });
                }
                if rank == STARTED_RANK {
                    started_count += 1;
                } else if rank == STATS_RANK {
                    if stats_rank != usize::MAX {
                        return Err(DomainError::ReferenceViolation {
                            kind: "api_stats",
                            value: api_id,
                        });
                    }
                    stats_rank = rank;
                } else if (REASONING_RANK..=TOOL_RANK).contains(&rank) {
                    last_provider_index = Some(index);
                }
                if let Some(source) = provider_source_of(&self.events[index]) {
                    if rank == STATS_RANK || provider_source_index.replace(index).is_some() {
                        return Err(DomainError::ProviderSourceAlreadySet {
                            api_call_id: api_id,
                        });
                    }
                    // The checkpoint coverage must equal the hosting event's
                    // own seq (plan 5.5.6).
                    // 检查点覆盖必须等于承载事件自身的 seq(计划 5.5.6)。
                    if source.covered_until_seq.value() != self.events[index].seq.value() {
                        return Err(DomainError::SequenceViolation {
                            expected: self.events[index].seq.value(),
                            actual: source.covered_until_seq.value(),
                        });
                    }
                    let expected_fingerprint =
                        provider_visible_fingerprint(&self.events, self.events[index].seq)?;
                    if source.fingerprint_sha256 != expected_fingerprint {
                        return Err(DomainError::ImmutableFieldMutation {
                            entity: "provider_source",
                            field: "fingerprint_sha256",
                        });
                    }
                    if let Some(replay) = &source.replay
                        && !replay.matches_route(
                            &source.provider,
                            source.connection_id.as_deref().unwrap_or_default(),
                            &replay.route_fingerprint,
                            source.profile.as_deref().unwrap_or_default(),
                            &source.model,
                        )
                    {
                        return Err(DomainError::ApiRequestMismatch {
                            field: "provider_source.replay.route",
                            expected: format!(
                                "{}/{}/{}",
                                source.provider,
                                source.profile.as_deref().unwrap_or_default(),
                                source.model
                            ),
                            actual: format!(
                                "{}/{}/{}",
                                replay.provider_id, replay.protocol_profile_id, replay.model_id
                            ),
                        });
                    }
                }
                previous_rank = Some(rank);
            }
            if let Some(source_index) = provider_source_index
                && last_provider_index != Some(source_index)
            {
                return Err(DomainError::ProviderSourcePlacement {
                    api_call_id: api_id,
                    source_seq: self.events[source_index].seq.value(),
                    expected_seq: last_provider_index.map(|index| self.events[index].seq.value()),
                    api_closed: stats_rank != usize::MAX,
                });
            }
            if started_count != 1 {
                return Err(DomainError::ReferenceViolation {
                    kind: "api_started",
                    value: api_id,
                });
            }
            let started_event = self
                .events
                .iter()
                .find(|event| {
                    matches!(
                        &event.event,
                        EventBody::ApiStarted(started)
                            if started.api_call_id.value() == api_id
                    )
                })
                .ok_or_else(|| DomainError::reference("api_started", api_id))?;
            let EventBody::ApiStarted(started) = &started_event.event else {
                unreachable!("api_started event selected above")
            };
            validate_api_request_snapshot(&started.request)?;
            validate_provider_projection_proof_shape(&started.projection)?;
            validate_provider_request_budget_evidence(&started.request_budget_evidence)?;
            if started.projection.through_seq.value() + 1 != started_event.seq.value() {
                return Err(DomainError::InvalidProviderProjection {
                    field: "through_seq",
                });
            }
            if started.projection.segment_generation > self.metadata.segment_generation {
                return Err(DomainError::InvalidProviderProjection {
                    field: "segment_generation",
                });
            }
            if let Some(stats) = self.events.iter().find_map(|event| match &event.event {
                EventBody::ApiStats(stats) if stats.api_call_id.value() == api_id => Some(stats),
                _ => None,
            }) {
                validate_api_terminal_facts(
                    stats.status,
                    stats.end_reason,
                    stats.error.as_ref(),
                    stats.continuation_eligibility,
                    &stats.timing,
                )?;
                ensure_api_request_field("purpose", &started.purpose, &stats.purpose)?;
                ensure_api_request_field("provider", &started.request.provider, &stats.provider)?;
                ensure_api_request_field("profile", &started.request.profile, &stats.profile)?;
                ensure_api_request_field("model", &started.request.model, &stats.model)?;
                ensure_api_request_field(
                    "connection_id",
                    &started.request.connection_id,
                    &stats.connection_id,
                )?;
                ensure_api_request_field(
                    "thinking_level",
                    &started.request.thinking_level,
                    &stats.thinking_level,
                )?;
                ensure_api_request_field(
                    "temperature",
                    &started.request.temperature,
                    &stats.temperature,
                )?;
            }
            for source in self.events.iter().filter_map(|event| {
                (group_event_api_id(event) == Some(api_id))
                    .then(|| provider_source_of(event))
                    .flatten()
            }) {
                ensure_api_request_field(
                    "provider_source.provider",
                    &started.request.provider,
                    &source.provider,
                )?;
                ensure_api_request_field(
                    "provider_source.profile",
                    &started.request.profile,
                    &source.profile,
                )?;
                ensure_api_request_field(
                    "provider_source.model",
                    &started.request.model,
                    &source.model,
                )?;
                ensure_api_request_field(
                    "provider_source.connection_id",
                    &started.request.connection_id,
                    &source.connection_id,
                )?;
            }
            if stats_rank == usize::MAX {
                let has_source = self.events.iter().any(|event| {
                    group_event_api_id(event) == Some(api_id) && provider_source_of(event).is_some()
                });
                last_open_api = Some((api_id, has_source));
            }
        }

        // Strict stream invariants (plan 12.21.1.5, audit P0#5): no group
        // spans turns, no group reopens after its stats, two open groups
        // never interleave, and every turn closure sequence is unique and
        // ordered (boundary strictly before the single stats, no same-turn
        // event after the stats).
        // 严格流不变量(计划 12.21.1.5、审计 P0#5)：组不跨回合、组在其统计后
        // 不得重开、两个开放组不得交错、每个回合的闭合序列唯一且有序(boundary
        // 严格先于唯一 stats，stats 之后无同回合事件)。
        let mut turn_boundaries: BTreeMap<u64, u64> = BTreeMap::new();
        let mut turn_stats_seen: BTreeMap<u64, u64> = BTreeMap::new();
        for event in &self.events {
            let Some(turn_id) = event.turn_id else {
                continue;
            };
            let turn_value = turn_id.value();
            match &event.event {
                EventBody::TurnBoundary(_) => {
                    if turn_boundaries
                        .insert(turn_value, event.seq.value())
                        .is_some()
                    {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                    if turn_stats_seen.contains_key(&turn_value) {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                }
                EventBody::TurnStats(_) => {
                    // Stats without a prior boundary is a mis-ordered closure.
                    if !turn_boundaries.contains_key(&turn_value) {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                    if turn_stats_seen
                        .insert(turn_value, event.seq.value())
                        .is_some()
                    {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                }
                _ => {
                    if turn_stats_seen.contains_key(&turn_value) {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                }
            }
        }

        let mut group_turn: BTreeMap<u64, u64> = BTreeMap::new();
        let mut group_closed: BTreeSet<u64> = BTreeSet::new();
        let mut last_group: Option<u64> = None;
        for event in &self.events {
            let Some(api_id) = group_event_api_id(event) else {
                continue;
            };
            let turn_value = event
                .turn_id
                .ok_or_else(|| DomainError::reference("api_group_turn", api_id))?
                .value();
            match group_turn.get(&api_id) {
                Some(known) => {
                    if *known != turn_value {
                        return Err(DomainError::GroupSpansTurns {
                            api_call_id: api_id,
                        });
                    }
                }
                None => {
                    group_turn.insert(api_id, turn_value);
                }
            }
            if group_closed.contains(&api_id) {
                return Err(DomainError::ClosedGroupReopened {
                    api_call_id: api_id,
                });
            }
            if matches!(&event.event, EventBody::ApiStats(_)) {
                group_closed.insert(api_id);
            }
            // A new group may only open after the previous one is closed.
            // 新组只能在上一组闭合后打开。
            if let Some(previous) = last_group
                && previous != api_id
                && !group_closed.contains(&previous)
            {
                return Err(DomainError::GroupsInterleaved {
                    first: previous,
                    second: api_id,
                });
            }
            last_group = Some(api_id);
        }

        // Rebuild the active turn from turn starts and boundaries.
        let mut active_turn: Option<ActiveTurnState> = None;
        let mut turn_state: BTreeMap<u64, (u64, TurnOrigin)> = BTreeMap::new();
        for event in &self.events {
            match &event.event {
                EventBody::UserInput(_) => {
                    if let Some(turn_id) = event.turn_id {
                        turn_state
                            .entry(turn_id.value())
                            .or_insert((event.seq.value(), TurnOrigin::UserRoot));
                    }
                }
                EventBody::SystemContext(context)
                    if (context.context_key == "goal_continuation"
                        || (context.context_key == "goal_state"
                            && context.operation == SystemContextOperation::Upsert
                            && self.metadata.open_turn_start_seq == Some(event.seq)))
                        && let Some(turn_id) = event.turn_id =>
                {
                    turn_state
                        .entry(turn_id.value())
                        .or_insert((event.seq.value(), TurnOrigin::GoalContinuation));
                }
                _ => {}
            }
        }
        for (turn_id, (start_seq, origin)) in turn_state {
            let closed = turn_ends.contains_key(&turn_id);
            if !closed {
                if active_turn.is_some() {
                    return Err(DomainError::MultipleOpenTurns);
                }
                active_turn = Some(ActiveTurnState {
                    turn_id: TurnId::try_new(turn_id)?,
                    origin,
                    start_seq: Seq::try_new(start_seq)?,
                    closed: false,
                });
            }
        }

        // Rebuild the active API state for the single open group, if any.
        let active_api = match last_open_api {
            Some((api_id, provider_source_placed)) => {
                let api_call_id = ApiCallId::try_new(api_id)?;
                let started = self
                    .events
                    .iter()
                    .find(|event| {
                        matches!(
                            &event.event,
                            EventBody::ApiStarted(value)
                                if value.api_call_id.value() == api_id
                        )
                    })
                    .ok_or_else(|| DomainError::reference("api_started", api_id))?;
                let turn_id = started
                    .turn_id
                    .ok_or_else(|| DomainError::reference("api_group", api_id))?;
                let EventBody::ApiStarted(started_body) = &started.event else {
                    unreachable!("api_started event selected above")
                };
                let reasoning_seq = self.events.iter().find_map(|event| match &event.event {
                    EventBody::AssistantReasoning(reasoning)
                        if reasoning.api_call_id.value() == api_id =>
                    {
                        Some(event.seq)
                    }
                    _ => None,
                });
                let text_seq = self.events.iter().find_map(|event| match &event.event {
                    EventBody::AssistantText(text) if text.api_call_id.value() == api_id => {
                        Some(event.seq)
                    }
                    _ => None,
                });
                let tool_seqs = self
                    .events
                    .iter()
                    .filter_map(|event| match &event.event {
                        EventBody::ToolCall(tool) if tool.api_call_id.value() == api_id => {
                            Some(event.seq)
                        }
                        _ => None,
                    })
                    .collect();
                Some(ActiveApiState {
                    api_call_id,
                    turn_id,
                    purpose: started_body.purpose,
                    started_seq: started.seq,
                    request: started_body.request.clone(),
                    projection: started_body.projection.clone(),
                    reasoning_seq,
                    text_seq,
                    tool_seqs,
                    stats_seq: None,
                    provider_source_placed,
                })
            }
            None => None,
        };
        self.tool_seq_by_id = tool_seq_by_id;
        self.active_turn = active_turn;
        self.active_api = active_api;
        self.id_allocator = IdAllocator::from_frontier(max_local_id + 1, max_seq + 1);
        Ok(())
    }

    /// Validates that persisted Provider API facts never exist under a fresh runtime baseline.
    /// 校验新鲜运行时基线下绝不存在已持久化的 Provider API 事实。
    fn validate_runtime_initialization_state(&self) -> Result<(), DomainError> {
        match self.runtime_lock.initialization_state {
            RuntimeInitializationState::Initialized => Ok(()),
            RuntimeInitializationState::BaselineRefreshPending => {
                if self.runtime_lock.locked_sources.is_empty() {
                    Ok(())
                } else {
                    Err(DomainError::InvalidRuntimeInitializationState {
                        field: "locked_sources",
                    })
                }
            }
            RuntimeInitializationState::FreshUninitialized => {
                if !self.runtime_lock.locked_sources.is_empty() {
                    return Err(DomainError::InvalidRuntimeInitializationState {
                        field: "locked_sources",
                    });
                }
                if self.runtime_lock.current_compaction_id.is_some() {
                    return Err(DomainError::InvalidRuntimeInitializationState {
                        field: "current_compaction_id",
                    });
                }
                if self
                    .events
                    .iter()
                    .any(|event| matches!(event.event, EventBody::ApiStarted(_)))
                {
                    return Err(DomainError::RuntimeBaselineUninitialized);
                }
                Ok(())
            }
        }
    }

    /// Validates the compaction document and its runtime publication pointer.
    /// 校验压缩文档及其 runtime 发布指针。
    fn validate_compaction_state(&self) -> Result<(), DomainError> {
        if !assert_node_order(&self.compactions) {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "compactions",
                field: "node_order",
            });
        }
        if let Some(pointer) = self.runtime_lock.current_compaction_id
            && !self
                .compactions
                .iter()
                .any(|node| node.compaction_id == pointer)
        {
            return Err(DomainError::reference(
                "runtime_compaction_id",
                pointer.value(),
            ));
        }
        for node in &self.compactions {
            if node.retained_from_seq > node.occurred_at_seq
                || node.occurred_at_seq >= node.api_stats_seq
            {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "seq_boundaries",
                });
            }
            if node.occurred_at_seq.value() < self.resident_start_seq {
                continue;
            }
            let started = self.events.iter().find_map(|event| match &event.event {
                EventBody::ApiStarted(started) if started.api_call_id == node.api_call_id => {
                    Some(started)
                }
                _ => None,
            });
            let Some(started) = started else {
                return Err(DomainError::reference(
                    "compaction_api_started",
                    node.api_call_id.value(),
                ));
            };
            if started.purpose != ApiPurposeKind::Compaction {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "api_purpose",
                });
            }
            let stats = self.events.iter().find_map(|event| match &event.event {
                EventBody::ApiStats(stats)
                    if event.seq == node.api_stats_seq && stats.api_call_id == node.api_call_id =>
                {
                    Some(stats)
                }
                _ => None,
            });
            let Some(stats) = stats else {
                return Err(DomainError::reference(
                    "compaction_api_stats",
                    node.api_stats_seq.value(),
                ));
            };
            if stats.status != ApiCallStatus::Completed {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "api_status",
                });
            }
            let summary_matches = self.events.iter().any(|event| {
                if event.seq != node.occurred_at_seq {
                    return false;
                }
                match &event.event {
                    EventBody::AssistantText(text) => {
                        text.api_call_id == node.api_call_id
                            && text.status == AssistantSemanticStatus::Completed
                            && text.text == node.summary
                    }
                    EventBody::AssistantReasoning(reasoning) => {
                        reasoning.api_call_id == node.api_call_id
                            && reasoning.status == AssistantSemanticStatus::Completed
                            && reasoning.text == node.summary
                    }
                    _ => false,
                }
            });
            if !summary_matches {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "summary_source",
                });
            }
        }
        Ok(())
    }

    /// Validates metadata caches against a fully resident authoritative stream.
    /// 依据完整常驻的权威事件流校验 metadata 缓存。
    fn validate_loaded_metadata(&self) -> Result<(), DomainError> {
        validate_active_request_config(&self.metadata.config.active_request)?;
        if let Some(fork_origin) = &self.metadata.fork_origin {
            validate_fork_origin(&self.metadata.session_id, fork_origin)?;
        }
        if let Some(pending) = &self.metadata.config.pending_request {
            validate_timestamp("pending_request.set_at_ms", pending.set_at_ms)?;
            validate_pending_request_config(pending)?;
        }
        validate_knowledge_library_selection_config(
            &self.metadata.config.selection.knowledge_libraries,
        )?;
        validate_provider_tool_selection_config(&self.metadata.config.selection.provider_tools)?;
        if let Some(identity) = &self.metadata.input_state.draft.restored_from_queue {
            validate_user_input(
                self.metadata.input_state.draft.text.clone(),
                &self.metadata.input_state.draft.resource_ids,
            )?;
            let input_id_reused = self
                .metadata
                .input_state
                .pending_in_turn_inputs
                .iter()
                .any(|pending| pending.input_id == identity.input_id)
                || self
                    .metadata
                    .input_state
                    .queued_inputs
                    .iter()
                    .any(|queued| queued.input_id == identity.input_id)
                || self.events.iter().any(|event| {
                    matches!(
                        &event.event,
                        EventBody::UserInput(input) if input.input_id == identity.input_id
                    )
                });
            if input_id_reused {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "draft_input_id_reused",
                });
            }
        }
        if self.resident_start_seq != 1 {
            return Ok(());
        }
        let derived_open_seq = self.active_turn.as_ref().map(|turn| turn.start_seq);
        if self.metadata.open_turn_start_seq != derived_open_seq {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "metadata",
                field: "open_turn_start_seq",
            });
        }
        let goal = &self.metadata.mode.goal;
        if goal.origin_turn_id.is_some() != goal.origin_seq.is_some() {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "goal",
                field: "origin",
            });
        }
        if let (Some(origin_turn_id), Some(origin_seq)) = (goal.origin_turn_id, goal.origin_seq) {
            let origin_matches = self
                .events
                .iter()
                .any(|event| event.seq == origin_seq && event.turn_id == Some(origin_turn_id));
            if !origin_matches {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "goal",
                    field: "origin",
                });
            }
        }
        if goal.enabled
            && goal.state == GoalState::Active
            && goal.origin_seq.is_some()
            && self.active_turn.is_none()
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "goal",
                field: "active_turn",
            });
        }
        if let Some(cache) = &goal.budget_cache {
            let covers_turn_stats = self.events.iter().any(|event| {
                event.seq == cache.covered_until_seq
                    && matches!(&event.event, EventBody::TurnStats(_))
            });
            if !covers_turn_stats {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "goal",
                    field: "budget_cache.covered_until_seq",
                });
            }
        }
        Ok(())
    }

    /// Removes a historical tail by truncating every event at or after the
    /// target seq, then rebuilding derived state (edit/rewrite support).
    /// 通过截断目标 seq 及其后的全部事件删除历史尾部，然后重建派生状态
    /// (编辑/维护重写支持)。
    ///
    /// This is a maintenance rewrite entry point used by the generation
    /// builder; it never leaves seq holes because the retained prefix is
    /// already contiguous.
    /// 这是 generation 构建器使用的维护性重写入口；保留前缀本身连续，因此
    /// 永远不会留下 seq 空洞。
    pub fn truncate_after(
        &mut self,
        keep_until_seq: Seq,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let keep_until = keep_until_seq.value();
        let count = self
            .events
            .iter()
            .filter(|event| event.seq.value() <= keep_until)
            .count();
        if count == self.events.len() {
            return Ok(false);
        }
        let mut staged = self.clone();
        staged.events.truncate(count);
        staged.rebuild_derived_state()?;
        staged.bump_revision(&[DirtyFile::Metadata, DirtyFile::ActiveTail], now_ms, &[]);
        *self = staged;
        Ok(true)
    }

    /// Projects persisted streaming semantics and every non-terminal tool to
    /// `interrupted` after a crash reload (plan 6.5.6), then statically closes
    /// the crashed physical API (plan 12.21.1.5).
    /// 崩溃重载后把已持久化的流式语义与每个非终态工具投影为 `interrupted`
    /// (计划 6.5.6)，随后统计闭合崩溃的物理 API(计划 12.21.1.5)。
    ///
    /// This is a crash-terminal projection, not a repair: statuses are mutated
    /// in place on their existing seq, and one terminal `ApiStats` event closes
    /// any API group that still lacks statistics, so the API no longer occupies
    /// `active_api` and the caller can `continue_turn` then start a fresh
    /// physical API. The interrupted group preserves its immutable request
    /// identity while usage stays all-unknown: a crash snapshot never fabricates
    /// official numbers. The tail is marked dirty; the caller must fix the
    /// transition through one normal explicit flush. Idempotent: a second call
    /// projects nothing.
    /// 这是崩溃终态投影，不是修复：状态在其既有 seq 上原位变更，且一条终态
    /// `ApiStats` 事件闭合任何仍缺统计的 API 组，使该 API 不再占用
    /// `active_api`，调用方可以先 `continue_turn` 再启动全新物理 API。中断组保留
    /// 其不可变请求身份，用量保持全未知：崩溃快照绝不伪造官方数字。尾段被标记
    /// dirty；调用方必须通过一次正常显式 flush 固化该转换。幂等：第二次调用不再
    /// 投影任何内容。
    pub fn project_persisted_interruptions(&mut self) -> Result<u32, DomainError> {
        let Some(active_api) = self.active_api.as_ref() else {
            return Ok(0);
        };
        if active_api.purpose == ApiPurposeKind::Continue
            && !self.api_transport_was_observed(active_api.api_call_id)
        {
            // A Continue that was durably prepared but never observed by
            // transport remains dispatchable under the same client action
            // and ApiCallId after restart. Projecting a terminal here would
            // make the exactly-once binding impossible to resume.
            // 已持久化准备但尚未被 transport 观察到的 Continue，在重启后仍可
            // 使用同一 client action 与 ApiCallId 派发。若在此投影终态，将使
            // exactly-once 绑定无法恢复。
            return Ok(0);
        }
        let now_ms = self
            .events
            .iter()
            .map(|event| event.updated_at_ms)
            .max()
            .unwrap_or(0);
        let (projected, _) =
            self.interrupt_active_api_state(now_ms, ApiEndReason::ProcessInterrupted, false)?;
        Ok(projected)
    }

    /// Returns whether transport durably observed one physical API request.
    /// 返回 transport 是否已可靠观察到指定物理 API 请求。
    ///
    /// # Parameters
    /// 参数
    /// `api_call_id` identifies the exact physical API request.
    /// `api_call_id` 标识精确的物理 API 请求。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when the matching durable execution-control fact exists.
    /// 仅当存在匹配的可靠执行控制事实时返回 `true`。
    fn api_transport_was_observed(&self, api_call_id: ApiCallId) -> bool {
        self.events.iter().any(|event| {
            matches!(
                &event.event,
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::ApiTransportObserved
                        && control.api_call_id == Some(api_call_id)
            )
        })
    }

    /// Selects the persisted context-compaction node for the next safe request
    /// boundary without changing physical segment rollover thresholds.
    /// 为下一个安全请求边界选择持久化上下文压缩节点，不改变物理分片滚段阈值。
    pub fn set_context_compaction_window_node_id(
        &mut self,
        node_id: Option<vulcan_ai_contract::ContextCompactionWindowNodeId>,
        now_ms: i64,
    ) -> bool {
        if self.metadata.config.context_compaction_window_node_id == node_id {
            return false;
        }
        self.metadata.config.context_compaction_window_node_id = node_id;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        true
    }

    /// Copies the complete immutable candidates of every dirty file at one
    /// revision (plan 6.2.1 flush contract).
    /// 在某一个修订号复制全部 dirty 文件的完整不可变候选(计划 6.2.1 flush
    /// 契约)。
    ///
    /// The revision is captured from `memory_revision` inside this call, so
    /// candidate revision, dirty list, and candidate content always come from
    /// one atomic snapshot and no caller can construct a future revision. The
    /// event candidate copies only the active tail; flushed sealed segments
    /// are never rewritten. The caller takes this owned snapshot, releases the
    /// session state lock, and hands it to the single physical writer; the
    /// document keeps mutating freely meanwhile, bumping to higher revisions
    /// that this candidate cannot clear.
    /// 修订号在此调用内部从 `memory_revision` 捕获，因此候选修订号、dirty
    /// 列表与候选内容总是来自同一个原子快照，任何调用方都无法构造未来修订号。
    /// 事件候选只复制活动尾段；已封口分片从不重写。调用方取得这份自有快照、
    /// 释放会话状态锁，并把它交给唯一物理写入器；文档在此期间可以继续自由
    /// 变更，递增到该候选无法清除的更高修订号。
    pub fn prepare_flush_candidates(&mut self) -> Result<Option<FlushCandidate>, DomainError> {
        if self.dirty_since.is_empty() {
            return Ok(None);
        }
        let revision = self.memory_revision;
        let tail_start = self.active_tail_start();
        let tail_index = tail_start
            .value()
            .checked_sub(self.resident_start_seq)
            .ok_or(DomainError::SequenceViolation {
                expected: self.resident_start_seq,
                actual: tail_start.value(),
            })? as usize;
        let tail_start_value = tail_start.value();
        assert!(
            tail_index <= self.events.len(),
            "active tail start {tail_start_value} exceeds the event stream length {}",
            self.events.len()
        );
        let tail_events = self.events[tail_index..].to_vec();
        let changed_event_seqs: Vec<Seq> = self
            .changed_event_seqs_by_revision
            .range((self.durable_revision + 1)..=revision)
            .flat_map(|(_, changed)| changed.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.issued_revisions.insert(revision);
        // Remember which compaction pointer becomes durable if this revision
        // is confirmed: only a runtime write advances the published pointer.
        // 记住该修订号被确认后哪个压缩指针变为持久化：只有 runtime 写入才会
        // 推进已发布指针。
        let published_if_confirmed = if self.dirty_since.contains_key(&DirtyFile::Runtime) {
            self.runtime_lock.current_compaction_id
        } else {
            self.published_compaction_id
        };
        self.pending_compaction_publish
            .insert(revision, published_if_confirmed);
        Ok(Some(FlushCandidate {
            revision,
            dirty: self.dirty_since.keys().copied().collect(),
            changed_event_seqs,
            metadata: self.metadata.clone(),
            runtime_lock: self.runtime_lock.clone(),
            compactions: self.compactions.clone(),
            tail_events,
        }))
    }

    /// Returns the first seq of the active tail from the split map.
    /// 返回分片映射中活动尾段的起始 seq。
    ///
    /// The last split entry is the active tail; an empty split map means the
    /// whole stream is still the tail (plan 5.2.15).
    /// 最后一个分片条目是活动尾段；分片映射为空说明整个流仍是尾段(计划
    /// 5.2.15)。
    fn active_tail_start(&self) -> Seq {
        match self.metadata.split_list.last() {
            Some(last) => last.start_seq,
            None => Seq::try_new(1).expect("the first seq is always valid"),
        }
    }

    /// Clones exact resident events selected by already validated seq values.
    /// 按已经校验的 seq 精确克隆常驻事件。
    fn clone_resident_events_at_seqs(
        &self,
        seqs: &[Seq],
    ) -> Result<Vec<ConversationEvent>, DomainError> {
        seqs.iter()
            .map(|seq| {
                let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
                    DomainError::ReferenceViolation {
                        kind: "resident_event_seq",
                        value: seq.value(),
                    },
                )? as usize;
                self.events
                    .get(index)
                    .filter(|event| event.seq == *seq)
                    .cloned()
                    .ok_or(DomainError::ReferenceViolation {
                        kind: "resident_event_seq",
                        value: seq.value(),
                    })
            })
            .collect()
    }

    /// Sets the request configuration to apply at the next safe API boundary.
    /// 设置将在下一个安全 API 边界生效的请求配置。
    ///
    /// This mutation affects only `metadata.json`; it never appends a timeline
    /// event because no switch has occurred yet.
    /// 此修改只影响 `metadata.json`；配置尚未切换，因此绝不追加时间线事件。
    pub fn set_pending_request(
        &mut self,
        pending: PendingRequestConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("pending_request.set_at_ms", pending.set_at_ms)?;
        validate_timestamp("updated_at_ms", now_ms)?;
        validate_pending_request_config(&pending)?;
        if self
            .metadata
            .config
            .pending_request
            .as_ref()
            .is_some_and(|current| pending_request_effectively_equal(current, &pending))
        {
            return Ok(false);
        }
        self.metadata.config.pending_request = Some(pending);
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Sets the normalized session output-style selection in authoritative metadata.
    /// 在权威 metadata 中设置规范化后的会话输出风格选择。
    ///
    /// # Parameters
    /// 参数
    /// `style_id` is the optional catalog identity; surrounding whitespace is
    /// removed and an empty value means the global/default style. `now_ms` is
    /// the semantic mutation time.
    /// `style_id` 是可选目录身份；首尾空白会被移除，空值表示全局/默认风格；
    /// `now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when the normalized selection changed.
    /// 仅在规范化选择真实变化时返回 `true`。
    pub fn set_output_style(
        &mut self,
        style_id: Option<String>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let style_id = normalize_optional_selection(style_id);
        if self.metadata.config.selection.output_style == style_id {
            return Ok(false);
        }
        self.metadata.config.selection.output_style = style_id;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Commits the complete first-turn selection bundle as one failure-atomic metadata revision.
    /// 把完整首轮选择配置作为一次失败原子的 metadata 修订提交。
    ///
    /// # Parameters
    /// 参数
    /// `input` contains the complete output-style, Provider-tool, knowledge-library, and VMM
    /// selections; `now_ms` is the semantic mutation time.
    /// `input` 包含完整的输出风格、Provider 工具、知识库与 VMM 选择；`now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when at least one exact durable selection changed. Every fallible
    /// validation completes before the document is mutated.
    /// 仅当至少一项精确持久化选择真实变化时返回 `true`；全部可能失败的校验均在修改文档前完成。
    pub fn set_session_selection_bundle(
        &mut self,
        input: SessionSelectionBundleInput,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        validate_provider_tool_selection_config(&input.provider_tools)?;
        validate_knowledge_library_selection_config(&input.knowledge_libraries)?;

        let mut selection = self.metadata.config.selection.clone();
        selection.output_style = normalize_optional_selection(input.output_style);
        selection.provider_tools = input.provider_tools;
        selection.knowledge_libraries = input.knowledge_libraries;
        selection.vmm = input.vmm;
        if self.metadata.config.selection == selection {
            return Ok(false);
        }

        self.metadata.config.selection = selection;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Sets the complete session-owned Provider tool selection in authoritative metadata.
    /// 在权威 metadata 中设置当前会话持有的完整 Provider 工具选择。
    ///
    /// # Parameters
    /// 参数
    /// `config` is the fully materialized, canonically ordered selection; `now_ms` is the semantic
    /// mutation time.
    /// `config` 是已经完整物化并按规范排序的选择；`now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when the exact durable selection changed.
    /// 仅在精确持久化选择真实变化时返回 `true`。
    pub fn set_provider_tool_selection(
        &mut self,
        config: ProviderToolSelectionConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        validate_provider_tool_selection_config(&config)?;
        if self.metadata.config.selection.provider_tools == config {
            return Ok(false);
        }
        self.metadata.config.selection.provider_tools = config;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Sets the complete session-owned knowledge-library selection in authoritative metadata.
    /// 在权威 metadata 中设置当前会话持有的完整知识库选择。
    ///
    /// # Parameters
    /// 参数
    /// `config` is the canonically ordered authorized selection and `now_ms` is the semantic
    /// mutation time.
    /// `config` 是按规范排序且已经授权的选择，`now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when the exact durable selection changed.
    /// 仅在精确持久化选择真实变化时返回 `true`。
    pub fn set_knowledge_library_selection(
        &mut self,
        config: KnowledgeLibrarySelectionConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        validate_knowledge_library_selection_config(&config)?;
        if self.metadata.config.selection.knowledge_libraries == config {
            return Ok(false);
        }
        self.metadata.config.selection.knowledge_libraries = config;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Sets both session-level VMM behavior overrides in authoritative metadata.
    /// 在权威 metadata 中同时设置两项会话级 VMM 行为覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `config` contains the complete tri-state policy pair and `now_ms` is the semantic mutation
    /// time.
    /// `config` 包含完整的三态策略对，`now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when either durable override changed.
    /// 仅在任一持久化覆盖真实变化时返回 `true`。
    pub fn set_vmm_session_policy(
        &mut self,
        config: VmmSessionPolicyConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if self.metadata.config.selection.vmm == config {
            return Ok(false);
        }
        self.metadata.config.selection.vmm = config;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Promotes the pending request configuration at one safe API boundary and
    /// appends a permanent model-switch notice only when the effective config
    /// actually changes (plan 7.6.3).
    /// 在安全 API 边界提升待生效请求配置，并且只在生效配置确实变化时追加永久
    /// 模型切换通知(计划 7.6.3)。
    ///
    /// `turn_id` explicitly scopes the display fact when the boundary belongs
    /// to an open turn; `None` records a session-level boundary. The method
    /// validates every fallible condition before mutating metadata, so active
    /// replacement, pending removal, and the optional display event form one
    /// all-or-nothing document transition.
    /// 当边界属于开放回合时，`turn_id` 显式限定显示事实；`None` 记录会话级边界。
    /// 该方法在修改 metadata 前校验所有可能失败的条件，因此 active 替换、pending
    /// 移除和可选显示事件构成一次全有或全无的文档转换。
    pub fn promote_pending_request(
        &mut self,
        turn_id: Option<TurnId>,
        now_ms: i64,
    ) -> Result<Option<Seq>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let pending = self
            .metadata
            .config
            .pending_request
            .as_ref()
            .cloned()
            .ok_or_else(|| DomainError::reference("pending_request", 0))?;
        if self.active_api.is_some() {
            return Err(DomainError::PromoteWhileApiOpen);
        }
        let next_active = ActiveRequestConfig {
            provider: pending.provider,
            connection_id: pending.connection_id,
            model: pending.model,
            thinking_level: pending.thinking_level,
            temperature: pending.temperature,
        };
        let previous_active = self.metadata.config.active_request.clone();
        let changed = previous_active != next_active;
        if changed {
            if let Some(expected_turn_id) = turn_id {
                let actual_turn_id = self.require_open_turn()?.turn_id;
                if expected_turn_id != actual_turn_id {
                    return Err(DomainError::TurnIdentityMismatch {
                        expected: expected_turn_id.value(),
                        actual: actual_turn_id.value(),
                    });
                }
            }
            self.ensure_transition_capacity(1, 0)?;
        }
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.config.active_request = next_active.clone();
        staged_metadata.config.pending_request = None;
        if !changed {
            self.metadata = staged_metadata;
            self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
            return Ok(None);
        }
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            EventBody::RequestConfigChanged(RequestConfigChangedEvent {
                previous: previous_active,
                current: next_active,
            }),
            now_ms,
        )?;
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[notice_seq],
        );
        Ok(Some(notice_seq))
    }

    /// Updates the Goal terminal state and appends the structured display note
    /// (plan 7.5.6).
    /// 更新 Goal 终态并追加结构化 display-note(计划 7.5.6)。
    pub fn block_goal(
        &mut self,
        reason_code: impl Into<String>,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        self.require_goal_enabled(true)?;
        self.require_goal_state(GoalState::Active, GoalState::Blocked)?;
        let reason_code = reason_code.into();
        let reason_code = reason_code.trim();
        if reason_code.is_empty() {
            return Err(DomainError::InvalidGoalReasonCode);
        }
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            None,
            EventVisibility::UiOnly,
            goal_notice_event(reason_code),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal.state = GoalState::Blocked;
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[notice_seq],
        );
        Ok(notice_seq)
    }

    /// Applies the shared active-API interruption transition.
    /// 应用共享的活动 API 中断转换。
    fn interrupt_active_api_state(
        &mut self,
        now_ms: i64,
        end_reason: ApiEndReason,
        record_execution_control: bool,
    ) -> Result<(u32, ApiGroupCompletionReceipt), DomainError> {
        let api = self.active_api.clone().ok_or(DomainError::NoActiveApi)?;
        let pending_inputs = self.metadata.input_state.pending_in_turn_inputs.clone();
        for pending in &pending_inputs {
            validate_user_input(pending.text.clone(), &pending.resource_ids)?;
        }
        let promotable_count = pending_inputs
            .iter()
            .filter(|pending| !self.input_id_already_streamed(pending.input_id))
            .count();
        let promotable_count = u64::try_from(promotable_count)
            .map_err(|_| DomainError::invalid_id("pending_input_count", u64::MAX))?;
        let appended_event_count = u64::from(if record_execution_control { 2u8 } else { 1u8 })
            .checked_add(promotable_count)
            .ok_or_else(|| DomainError::invalid_id("event_count", u64::MAX))?;
        self.ensure_transition_capacity(appended_event_count, 0)?;
        self.validate_active_api_events(&api)?;

        let mut projected = 0u32;
        let mut staged_event_updates = BTreeMap::new();
        for seq in [api.reasoning_seq, api.text_seq].into_iter().flatten() {
            projected += u32::from(self.stage_active_event_interruption(
                &mut staged_event_updates,
                seq,
                api.api_call_id,
                now_ms,
            )?);
        }
        for seq in api.tool_seqs.iter().copied() {
            projected += u32::from(self.stage_active_event_interruption(
                &mut staged_event_updates,
                seq,
                api.api_call_id,
                now_ms,
            )?);
        }
        self.ensure_staged_active_api_tools_terminal(&api, &staged_event_updates)?;
        let mut staged_allocator = self.id_allocator;
        let staged_capacity = usize::try_from(appended_event_count)
            .map_err(|_| DomainError::invalid_id("event_count", appended_event_count))?;
        let mut staged_events = Vec::with_capacity(staged_capacity);
        if record_execution_control {
            self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(api.turn_id),
                EventVisibility::HostOnly,
                EventBody::ExecutionControl(ExecutionControlEvent {
                    control_kind: ExecutionControlKind::ExecutionInterrupted,
                    api_call_id: Some(api.api_call_id),
                    client_action_id: None,
                }),
                now_ms,
            )?;
        }
        let request = api.request;
        let stats_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(api.turn_id),
            EventVisibility::UiOnly,
            EventBody::ApiStats(ApiStatsEvent {
                api_call_id: api.api_call_id,
                purpose: api.purpose,
                provider: request.provider,
                profile: request.profile,
                model: request.model,
                connection_id: request.connection_id,
                thinking_level: request.thinking_level,
                temperature: request.temperature,
                usage: UsageSnapshot::default(),
                status: ApiCallStatus::Interrupted,
                end_reason: Some(end_reason),
                error: None,
                continuation_eligibility: ContinuationEligibility::Allowed,
                timing: ApiTimings {
                    completed_at_ms: Some(now_ms),
                    ..ApiTimings::default()
                },
            }),
            now_ms,
        )?;
        let mut promoted_input_seqs = Vec::with_capacity(promotable_count as usize);
        for pending in pending_inputs.iter().cloned() {
            if self.input_id_already_streamed(pending.input_id) {
                continue;
            }
            let promoted_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(api.turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    client_action_id: pending.client_action_id,
                    input_id: pending.input_id,
                    input_source: UserInputSource::PendingInTurn,
                    text: pending.text,
                    resource_ids: pending.resource_ids,
                }),
                now_ms,
            )?;
            promoted_input_seqs.push(promoted_seq);
        }
        let had_pending_inputs = !pending_inputs.is_empty();
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.input_state.pending_in_turn_inputs.clear();
        let mut changed_event_seqs: Vec<Seq> = staged_event_updates
            .values()
            .map(ConversationEvent::seq)
            .collect();
        changed_event_seqs.extend(staged_events.iter().map(ConversationEvent::seq));
        for (index, event) in staged_event_updates {
            self.events[index] = event;
        }
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.active_api = None;
        let dirty_files: &[DirtyFile] = if had_pending_inputs {
            &[DirtyFile::Metadata, DirtyFile::ActiveTail]
        } else {
            &[DirtyFile::ActiveTail]
        };
        self.bump_revision(dirty_files, now_ms, &changed_event_seqs);
        let projected_input_count = u32::try_from(promoted_input_seqs.len())
            .map_err(|_| DomainError::invalid_id("pending_input_count", u64::MAX))?;
        Ok((
            projected
                .checked_add(1)
                .and_then(|count| count.checked_add(projected_input_count))
                .ok_or_else(|| DomainError::invalid_id("projection_count", u64::MAX))?,
            ApiGroupCompletionReceipt {
                api_call_id: api.api_call_id,
                turn_id: api.turn_id,
                stats_seq,
                promoted_input_seqs,
            },
        ))
    }

    /// Stages interruption of one active API-owned semantic or running tool.
    /// 暂存一条属于活动 API 的语义或运行中工具中断。
    fn stage_active_event_interruption(
        &self,
        staged_event_updates: &mut BTreeMap<usize, ConversationEvent>,
        seq: Seq,
        api_call_id: ApiCallId,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        let mut event = staged_event_updates
            .get(&index)
            .cloned()
            .or_else(|| self.events.get(index).cloned())
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let changed = match &event.event {
            EventBody::AssistantReasoning(reasoning) => {
                ensure_api_event_owner(seq, api_call_id, reasoning.api_call_id)?;
                reasoning.status == AssistantSemanticStatus::Streaming
            }
            EventBody::AssistantText(text) => {
                ensure_api_event_owner(seq, api_call_id, text.api_call_id)?;
                text.status == AssistantSemanticStatus::Streaming
            }
            EventBody::ToolCall(tool) => {
                ensure_api_event_owner(seq, api_call_id, tool.api_call_id)?;
                matches!(
                    tool.status,
                    ToolCallStatus::Prepared
                        | ToolCallStatus::Running
                        | ToolCallStatus::WaitingUser
                )
            }
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "active_api_event",
                });
            }
        };
        if changed {
            event.mutate_in_place(now_ms, |body| match body {
                EventBody::AssistantReasoning(reasoning) => {
                    reasoning.status = AssistantSemanticStatus::Interrupted;
                }
                EventBody::AssistantText(text) => {
                    text.status = AssistantSemanticStatus::Interrupted;
                }
                EventBody::ToolCall(tool) => {
                    tool.status = ToolCallStatus::Interrupted;
                    tool.output = Some(ToolPayload::terminal_failure(
                        ToolTerminalErrorKind::Interrupted,
                    ));
                    tool.finished_at_ms = Some(now_ms);
                }
                _ => unreachable!("active API event kind was validated before mutation"),
            })?;
            staged_event_updates.insert(index, event);
        }
        Ok(changed)
    }

    /// Stages one Provider source attachment without publishing the target event.
    /// 暂存一次 Provider 来源附着，不发布目标事件。
    fn stage_provider_source_attachment(
        &self,
        staged_event_updates: &mut BTreeMap<usize, ConversationEvent>,
        seq: Seq,
        source: ProviderSource,
        now_ms: i64,
    ) -> Result<(), DomainError> {
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        let mut event = staged_event_updates
            .get(&index)
            .cloned()
            .or_else(|| self.events.get(index).cloned())
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let target = match &event.event {
            EventBody::AssistantReasoning(reasoning) => &reasoning.provider_source,
            EventBody::AssistantText(text) => &text.provider_source,
            EventBody::ToolCall(tool) => &tool.provider_source,
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "provider_source",
                });
            }
        };
        if target.is_some() {
            return Err(DomainError::ProviderSourceAlreadySet {
                api_call_id: source.covered_until_seq.value(),
            });
        }
        event.mutate_in_place(now_ms, |body| match body {
            EventBody::AssistantReasoning(reasoning) => {
                reasoning.provider_source = Some(source);
            }
            EventBody::AssistantText(text) => {
                text.provider_source = Some(source);
            }
            EventBody::ToolCall(tool) => {
                tool.provider_source = Some(source);
            }
            _ => unreachable!("provider source target was validated before mutation"),
        })?;
        staged_event_updates.insert(index, event);
        Ok(())
    }

    /// Requires every staged or persisted tool in one API group to be terminal.
    /// 要求一个 API 组中每个暂存或已持久化工具均为终态。
    fn ensure_staged_active_api_tools_terminal(
        &self,
        api: &ActiveApiState,
        staged_event_updates: &BTreeMap<usize, ConversationEvent>,
    ) -> Result<(), DomainError> {
        for seq in api.tool_seqs.iter().copied() {
            let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
                DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "sealed_segment",
                },
            )? as usize;
            let event = staged_event_updates
                .get(&index)
                .or_else(|| self.events.get(index))
                .ok_or_else(|| DomainError::reference("events", seq.value()))?;
            let EventBody::ToolCall(tool) = &event.event else {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "tool_call",
                });
            };
            if !matches!(
                tool.status,
                ToolCallStatus::Completed
                    | ToolCallStatus::Failed
                    | ToolCallStatus::Cancelled
                    | ToolCallStatus::Stopped
                    | ToolCallStatus::Interrupted
            ) {
                return Err(DomainError::ToolStatusMismatch {
                    tool_call_id: tool.tool_call_id.value(),
                    expected: "terminal",
                    actual: tool_status_name(tool.status),
                });
            }
            if tool.output.is_none() {
                return Err(DomainError::MissingTerminalToolOutput {
                    tool_call_id: tool.tool_call_id.value(),
                    status: tool_status_name(tool.status),
                });
            }
        }
        Ok(())
    }

    /// Validates every resident event referenced by the active API before an
    /// all-or-nothing interruption starts mutating the document.
    /// 在全有或全无中断开始修改文档前，校验活动 API 引用的每条常驻事件。
    fn validate_active_api_events(&self, api: &ActiveApiState) -> Result<(), DomainError> {
        if let Some(seq) = api.reasoning_seq {
            self.validate_active_event(seq, api.api_call_id, ActiveApiEventKind::Reasoning)?;
        }
        if let Some(seq) = api.text_seq {
            self.validate_active_event(seq, api.api_call_id, ActiveApiEventKind::Text)?;
        }
        for seq in api.tool_seqs.iter().copied() {
            self.validate_active_event(seq, api.api_call_id, ActiveApiEventKind::Tool)?;
        }
        Ok(())
    }

    /// Validates the kind and API owner of one resident active event.
    /// 校验一条常驻活动事件的种类与 API 所有者。
    fn validate_active_event(
        &self,
        seq: Seq,
        api_call_id: ApiCallId,
        expected_kind: ActiveApiEventKind,
    ) -> Result<(), DomainError> {
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        let event = self
            .events
            .get(index)
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let actual_api_call_id = match (&event.event, expected_kind) {
            (EventBody::AssistantReasoning(reasoning), ActiveApiEventKind::Reasoning) => {
                reasoning.api_call_id
            }
            (EventBody::AssistantText(text), ActiveApiEventKind::Text) => text.api_call_id,
            (EventBody::ToolCall(tool), ActiveApiEventKind::Tool) => tool.api_call_id,
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "active_api_event_kind",
                });
            }
        };
        ensure_api_event_owner(seq, api_call_id, actual_api_call_id)
    }

    /// Verifies that an all-or-nothing transition can allocate its complete
    /// contiguous seq and local-id range before performing any mutation.
    /// 在执行任何修改前，校验全有或全无转换能够分配完整连续 seq 与本地 id 范围。
    fn ensure_transition_capacity(
        &self,
        event_count: u64,
        local_id_count: u64,
    ) -> Result<(), DomainError> {
        let (next_local_id, next_seq) = self.id_allocator.frontier();
        let resident_len = u64::try_from(self.events.len())
            .map_err(|_| DomainError::invalid_id("resident_event_count", u64::MAX))?;
        let expected_seq = self
            .resident_start_seq
            .checked_add(resident_len)
            .ok_or_else(|| DomainError::invalid_id("seq", u64::MAX))?;
        if next_seq != expected_seq {
            return Err(DomainError::SequenceViolation {
                expected: expected_seq,
                actual: next_seq,
            });
        }
        if event_count > 0 {
            let final_seq = next_seq
                .checked_add(event_count - 1)
                .ok_or_else(|| DomainError::invalid_id("seq", u64::MAX))?;
            if final_seq > JS_SAFE_INTEGER_MAX {
                return Err(DomainError::invalid_id("seq", final_seq));
            }
        }
        if local_id_count > 0 {
            let final_local_id = next_local_id
                .checked_add(local_id_count - 1)
                .ok_or_else(|| DomainError::invalid_id("local_id", u64::MAX))?;
            if !(crate::v1::ids::LOCAL_ID_MIN..=JS_SAFE_INTEGER_MAX).contains(&final_local_id) {
                return Err(DomainError::invalid_id("local_id", final_local_id));
            }
        }
        Ok(())
    }

    /// Returns the last event seq, or an error on an empty stream.
    /// 返回最后一条事件 seq；流为空时返回错误。
    fn last_seq(&self) -> Result<Seq, DomainError> {
        self.events
            .last()
            .map(|event| event.seq)
            .ok_or(DomainError::reference("events", 0))
    }

    /// Returns whether one input id already appeared in the main stream.
    /// 返回某个输入 id 是否已经出现在主事件流中。
    fn input_id_already_streamed(&self, input_id: PendingInputId) -> bool {
        self.events.iter().any(|event| match &event.event {
            EventBody::UserInput(input) => input.input_id == input_id,
            _ => false,
        })
    }

    /// Requires the latest physical API of one turn to carry final assistant text.
    /// 要求一个回合的最新物理 API 携带最终 assistant 正文。
    fn require_final_assistant(&self, turn_id: TurnId) -> Result<(), DomainError> {
        let latest_stats = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::ApiStats(stats) if event.turn_id == Some(turn_id) => {
                    Some((event.seq, stats))
                }
                _ => None,
            })
            .max_by_key(|(seq, _)| seq.value())
            .map(|(_, stats)| stats)
            .ok_or(DomainError::TurnMissingFinalAssistant {
                turn_id: turn_id.value(),
            })?;
        let api_call_id = latest_stats.api_call_id;
        let has_final_text = latest_stats.status == ApiCallStatus::Completed
            && self.events.iter().any(|event| {
                matches!(
                    &event.event,
                    EventBody::AssistantText(text)
                        if text.api_call_id == api_call_id
                            && text.status == AssistantSemanticStatus::Completed
                )
            });
        let has_tool_calls = self.events.iter().any(|event| {
            matches!(
                &event.event,
                EventBody::ToolCall(tool) if tool.api_call_id == api_call_id
            )
        });
        if !has_final_text || has_tool_calls {
            return Err(DomainError::TurnMissingFinalAssistant {
                turn_id: turn_id.value(),
            });
        }
        Ok(())
    }

    /// Aggregates the enabled Goal budget solely from persisted statistics.
    /// 仅从已持久化统计聚合已启用 Goal 的预算。
    fn aggregate_goal_budget(
        &self,
        now_ms: i64,
    ) -> Result<Option<(GoalBudgetCache, TurnStatBuckets)>, DomainError> {
        if !self.metadata.mode.goal.enabled {
            return Ok(None);
        }
        let from_seq = self.metadata.mode.goal.origin_seq.map_or(1, Seq::value);
        let subagent_api_ids: BTreeSet<ApiCallId> = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::ApiStarted(started)
                    if event.seq.value() >= from_seq
                        && started.purpose == ApiPurposeKind::Subagent =>
                {
                    Some(started.api_call_id)
                }
                _ => None,
            })
            .collect();
        let mut direct_usages = Vec::new();
        let mut direct_api_count = 0u64;
        let mut direct_tool_count = 0u64;
        let mut subagent_totals = Vec::new();
        let mut counted_subagent_stats = BTreeSet::new();
        for event in self
            .events
            .iter()
            .filter(|event| event.seq.value() >= from_seq)
        {
            match &event.event {
                EventBody::ApiStats(api) if !subagent_api_ids.contains(&api.api_call_id) => {
                    direct_api_count += 1;
                    direct_usages.push(&api.usage);
                }
                EventBody::ToolCall(tool) if !subagent_api_ids.contains(&tool.api_call_id) => {
                    direct_tool_count += 1;
                }
                EventBody::SubagentReturnStats(returned)
                    if returned.status == SubagentReturnStatus::Final
                        && counted_subagent_stats.insert((
                            returned.child_session_id.clone(),
                            returned.child_turn_stats_seq,
                        )) =>
                {
                    subagent_totals.push(&returned.totals);
                }
                _ => {}
            }
        }
        let direct =
            StatBucket::from_api_usages(direct_usages, direct_api_count, direct_tool_count, 0);
        let subagent = StatBucket::from_subagent_returns(subagent_totals);
        let buckets = TurnStatBuckets::combine(direct, subagent);
        let Some(direct_total_tokens) = budget_tokens_or_zero(&buckets.direct) else {
            return Ok(None);
        };
        let Some(subagent_total_tokens) = budget_tokens_or_zero(&buckets.subagent) else {
            return Ok(None);
        };
        let Some(total_tokens) = budget_tokens_or_zero(&buckets.total) else {
            return Ok(None);
        };
        Ok(Some((
            GoalBudgetCache {
                covered_until_seq: self.last_seq()?,
                direct_total_tokens,
                subagent_total_tokens,
                total_tokens,
                updated_at_ms: now_ms,
            },
            buckets,
        )))
    }

    /// Requires one open (not closed) turn.
    /// 要求存在一个开放(未闭合)回合。
    fn require_open_turn(&self) -> Result<&ActiveTurnState, DomainError> {
        match &self.active_turn {
            Some(turn) if !turn.closed => Ok(turn),
            _ => Err(DomainError::NoActiveTurn),
        }
    }

    /// Reconstructs one exact subagent return receipt without mutating the document.
    /// 在不修改文档的情况下重建一份精确子代理回流 receipt。
    fn existing_subagent_return(
        &self,
        parent_turn_id: TurnId,
        input: &SubagentReturnInput,
    ) -> Result<Option<SubagentReturnReceipt>, DomainError> {
        validate_exact_external_identity("child_session_id", &input.child_session_id)?;
        validate_stat_bucket(&input.totals)?;
        let matching_returns: Vec<&ConversationEvent> = self
            .events
            .iter()
            .filter(|event| {
                event.turn_id == Some(parent_turn_id)
                    && matches!(
                        &event.event,
                        EventBody::SubagentReturn(returned)
                            if returned.child_session_id == input.child_session_id
                                && returned.child_task_id == input.child_task_id
                    )
            })
            .collect();
        if matching_returns.is_empty() {
            return Ok(None);
        }
        if matching_returns.len() != 1 {
            return Err(DomainError::InvalidStatistics {
                field: "subagent_return_identity_unique",
            });
        }
        let return_event = matching_returns[0];
        let stats_event = self
            .events
            .iter()
            .find(|event| event.seq.value() == return_event.seq.value() + 1)
            .ok_or(DomainError::InvalidStatistics {
                field: "subagent_return_stats_pair",
            })?;
        let EventBody::SubagentReturn(returned) = &return_event.event else {
            unreachable!("matching return filter guarantees the event kind")
        };
        let EventBody::SubagentReturnStats(stats) = &stats_event.event else {
            return Err(DomainError::InvalidStatistics {
                field: "subagent_return_stats_pair",
            });
        };
        validate_subagent_return_pair(returned, stats)?;
        if returned.child_turn_stats_seq != input.child_turn_stats_seq
            || returned.source_assistant_text_seq != input.source_assistant_text_seq
            || returned.source_from_seq != input.source_from_seq
            || returned.source_to_seq != input.source_to_seq
            || returned.content != input.content
            || stats.totals != input.totals
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "subagent_return",
                field: "durable_facts",
            });
        }
        Ok(Some(SubagentReturnReceipt {
            parent_turn_id,
            return_seq: return_event.seq,
            stats_seq: stats_event.seq,
            child_session_id: returned.child_session_id.clone(),
            child_task_id: returned.child_task_id.clone(),
            child_turn_stats_seq: returned.child_turn_stats_seq,
            source_assistant_text_seq: returned.source_assistant_text_seq,
        }))
    }

    /// Requires the in-flight API to match and returns its resident state.
    /// 要求当前在途 API 匹配并返回其常驻状态。
    fn require_active_api(&self, api_call_id: ApiCallId) -> Result<ActiveApiState, DomainError> {
        match &self.active_api {
            Some(api) if api.api_call_id == api_call_id => Ok(api.clone()),
            _ => Err(DomainError::CrossApiGroupUpdate {
                event_seq: 0,
                api_call_id: api_call_id.value(),
            }),
        }
    }

    /// Persists the resident active-api state back into the document.
    /// 把常驻活动 API 状态写回文档。
    fn save_active_api(&mut self, api: ActiveApiState) {
        self.active_api = Some(api);
    }

    /// Appends one event at the next contiguous seq and bumps the revision.
    /// 在下一个连续 seq 追加一条事件并递增修订号。
    fn append_event(
        &mut self,
        turn_id: Option<TurnId>,
        visibility: EventVisibility,
        body: EventBody,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            visibility,
            body,
            now_ms,
        )?;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &[seq]);
        Ok(seq)
    }

    /// Stages one event against a copied allocator without mutating the document.
    /// 使用已复制的分配器暂存一条事件，不修改文档。
    fn stage_event(
        &self,
        staged_allocator: &mut IdAllocator,
        staged_events: &mut Vec<ConversationEvent>,
        turn_id: Option<TurnId>,
        visibility: EventVisibility,
        body: EventBody,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        if let EventBody::SystemContext(context) = &body {
            validate_system_context_event(context)?;
        }
        if let EventBody::ExecutionControl(control) = &body {
            validate_execution_control_event(control)?;
        }
        let seq = staged_allocator.next_seq()?;
        let expected = self.resident_start_seq
            + u64::try_from(self.events.len() + staged_events.len())
                .map_err(|_| DomainError::invalid_id("event_count", u64::MAX))?;
        if seq.value() != expected {
            return Err(DomainError::SequenceViolation {
                expected,
                actual: seq.value(),
            });
        }
        staged_events.push(ConversationEvent {
            seq,
            turn_id,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            event_revision: crate::v1::event::EVENT_REVISION_INITIAL,
            visibility,
            event: body,
        });
        Ok(seq)
    }

    /// Returns an already persisted root submission when its action is an exact retry.
    /// 当一个动作是精确重试时返回已持久化的根输入提交。
    fn existing_root_submission(
        &self,
        client_action_id: &ClientActionId,
        text: &str,
        resource_ids: &[ResourceId],
    ) -> Result<Option<TurnId>, DomainError> {
        for event in &self.events {
            let EventBody::UserInput(input) = &event.event else {
                continue;
            };
            if input.client_action_id != *client_action_id {
                continue;
            }
            if input.input_source == UserInputSource::Root
                && input.text == text
                && input.resource_ids == resource_ids
            {
                return event
                    .turn_id
                    .map(Some)
                    .ok_or(DomainError::InvalidContinuationHistory {
                        field: "root_input.turn_id",
                    });
            }
            return Err(client_action_conflict(client_action_id));
        }
        Ok(None)
    }

    /// Returns an already queued submission when its action is an exact retry.
    /// 当一个动作是精确重试时返回已经排队的提交。
    fn existing_enqueue_receipt(
        &self,
        client_action_id: &ClientActionId,
        text: &str,
        resource_ids: &[ResourceId],
    ) -> Result<Option<EnqueueInputReceipt>, DomainError> {
        for queued in &self.metadata.input_state.queued_inputs {
            if queued.client_action_id != *client_action_id {
                continue;
            }
            if queued.text == text && queued.resource_ids == resource_ids {
                return Ok(Some(EnqueueInputReceipt {
                    client_action_id: client_action_id.clone(),
                    input_id: queued.input_id,
                    disposition: EnqueueInputDisposition::Queued,
                }));
            }
            return Err(client_action_conflict(client_action_id));
        }
        for event in &self.events {
            let EventBody::QueuedInputResolution(resolution) = &event.event else {
                continue;
            };
            if resolution.client_action_id != *client_action_id {
                continue;
            }
            if resolution.text == text && resolution.resource_ids == resource_ids {
                return Ok(Some(EnqueueInputReceipt {
                    client_action_id: client_action_id.clone(),
                    input_id: resolution.input_id,
                    disposition: EnqueueInputDisposition::Queued,
                }));
            }
            return Err(client_action_conflict(client_action_id));
        }
        if let Some(identity) = self
            .metadata
            .input_state
            .draft
            .restored_from_queue
            .as_ref()
            .filter(|identity| identity.client_action_id == *client_action_id)
        {
            let draft = &self.metadata.input_state.draft;
            if draft.text == text && draft.resource_ids == resource_ids {
                return Ok(Some(EnqueueInputReceipt {
                    client_action_id: client_action_id.clone(),
                    input_id: identity.input_id,
                    disposition: EnqueueInputDisposition::Queued,
                }));
            }
            return Err(client_action_conflict(client_action_id));
        }
        for (index, event) in self.events.iter().enumerate() {
            let EventBody::UserInput(input) = &event.event else {
                continue;
            };
            if input.client_action_id != *client_action_id {
                continue;
            }
            if input.input_source == UserInputSource::QueuedScheduled
                && input.text == text
                && input.resource_ids == resource_ids
            {
                return Ok(Some(EnqueueInputReceipt {
                    client_action_id: client_action_id.clone(),
                    input_id: input.input_id,
                    disposition: self.enqueue_disposition_from_scheduled_event(index)?,
                }));
            }
            return Err(client_action_conflict(client_action_id));
        }
        Ok(None)
    }

    /// Reconstructs the original enqueue disposition from one exact scheduled input event.
    /// 从一条精确已调度输入事件重建最初的排队处置结果。
    fn enqueue_disposition_from_scheduled_event(
        &self,
        input_index: usize,
    ) -> Result<EnqueueInputDisposition, DomainError> {
        let Some(stats_index) = input_index.checked_sub(1) else {
            return Ok(EnqueueInputDisposition::Queued);
        };
        let Some(boundary_index) = input_index.checked_sub(2) else {
            return Ok(EnqueueInputDisposition::Queued);
        };
        let input_event = &self.events[input_index];
        let stats_event = &self.events[stats_index];
        let boundary_event = &self.events[boundary_index];
        let (
            EventBody::UserInput(_),
            EventBody::TurnStats(stats),
            EventBody::TurnBoundary(boundary),
        ) = (
            &input_event.event,
            &stats_event.event,
            &boundary_event.event,
        )
        else {
            return Ok(EnqueueInputDisposition::Queued);
        };
        let Some(stopped_turn_id) = stats_event.turn_id else {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "superseded_goal_stats_turn_id",
            });
        };
        if boundary_event.turn_id != Some(stopped_turn_id)
            || stats.outcome != TurnOutcome::Stopped
            || boundary.outcome != TurnOutcome::Stopped
            || !self.is_goal_continuation_turn(stopped_turn_id)
        {
            return Ok(EnqueueInputDisposition::Queued);
        }
        let scheduled_turn_id =
            input_event
                .turn_id
                .ok_or(DomainError::InvalidQueuedInputOrder {
                    field: "scheduled_input_turn_id",
                })?;
        Ok(EnqueueInputDisposition::SupersededGoalAndScheduled {
            stopped_turn_id,
            stopped_turn_stats_seq: stats_event.seq,
            scheduled_turn_id,
            input_seq: input_event.seq,
        })
    }

    /// Returns whether one Turn was opened by a Goal-continuation context event.
    /// 返回一个回合是否由 Goal 续跑上下文事件开启。
    fn is_goal_continuation_turn(&self, turn_id: TurnId) -> bool {
        self.events.iter().find_map(|event| {
            if event.turn_id != Some(turn_id) {
                return None;
            }
            Some(matches!(
                &event.event,
                EventBody::SystemContext(context)
                    if context.context_key == "goal_continuation"
                        || (context.context_key == "goal_state"
                            && context.operation == SystemContextOperation::Upsert)
            ))
        }) == Some(true)
    }

    /// Resolves one exact queue identity or reports a structured missing/conflict error.
    /// 解析一个精确队列身份，或报告结构化缺失/冲突错误。
    fn require_queued_input_index(
        &self,
        identity: &QueuedInputIdentity,
    ) -> Result<usize, DomainError> {
        let mut partial_match = false;
        for (index, queued) in self.metadata.input_state.queued_inputs.iter().enumerate() {
            let queued_identity = queued_input_identity(queued);
            if queued_identity == *identity {
                return Ok(index);
            }
            if queued_identity.client_action_id == identity.client_action_id
                || queued_identity.input_id == identity.input_id
            {
                partial_match = true;
            }
        }
        if partial_match {
            return Err(client_action_conflict(&identity.client_action_id));
        }
        Err(DomainError::QueuedInputNotFound {
            input_id: identity.input_id.value(),
        })
    }

    /// Finds one terminal queued-input resolution and rejects partial identity drift.
    /// 查找一条排队输入终结处置，并拒绝部分身份漂移。
    fn existing_queued_input_resolution(
        &self,
        identity: &QueuedInputIdentity,
    ) -> Result<Option<&QueuedInputResolutionEvent>, DomainError> {
        let mut exact = None;
        for event in &self.events {
            let EventBody::QueuedInputResolution(resolution) = &event.event else {
                continue;
            };
            let action_matches = resolution.client_action_id == identity.client_action_id;
            let input_matches = resolution.input_id == identity.input_id;
            if action_matches && input_matches {
                if exact.replace(resolution).is_some() {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "duplicate_resolution_identity",
                    });
                }
            } else if action_matches || input_matches {
                return Err(client_action_conflict(&identity.client_action_id));
            }
        }
        Ok(exact)
    }

    /// Returns an already pending or streamed in-turn submission for an exact retry.
    /// 对精确重试返回已经暂存或已流入回合的过程内提交。
    fn existing_pending_submission(
        &self,
        client_action_id: &ClientActionId,
        text: &str,
        resource_ids: &[ResourceId],
    ) -> Result<Option<PendingInputId>, DomainError> {
        for pending in &self.metadata.input_state.pending_in_turn_inputs {
            if pending.client_action_id != *client_action_id {
                continue;
            }
            if pending.text == text && pending.resource_ids == resource_ids {
                return Ok(Some(pending.input_id));
            }
            return Err(client_action_conflict(client_action_id));
        }
        let mut existing_input_id = None;
        for event in &self.events {
            if event_client_action_id(event) != Some(client_action_id) {
                continue;
            }
            if let EventBody::UserInput(input) = &event.event
                && input.input_source == UserInputSource::PendingInTurn
                && input.text == text
                && input.resource_ids == resource_ids
                && existing_input_id.replace(input.input_id).is_none()
            {
                continue;
            }
            return Err(client_action_conflict(client_action_id));
        }
        Ok(existing_input_id)
    }

    /// Requires one external client action identity to be globally unused.
    /// 要求一个外部客户端动作身份在整个会话中尚未使用。
    fn require_client_action_unused(
        &self,
        client_action_id: &ClientActionId,
    ) -> Result<(), DomainError> {
        let used_by_metadata = self
            .metadata
            .input_state
            .pending_in_turn_inputs
            .iter()
            .any(|pending| pending.client_action_id == *client_action_id)
            || self
                .metadata
                .input_state
                .queued_inputs
                .iter()
                .any(|queued| queued.client_action_id == *client_action_id)
            || self
                .metadata
                .input_state
                .draft
                .restored_from_queue
                .as_ref()
                .is_some_and(|identity| identity.client_action_id == *client_action_id);
        let used_by_event = self.events.iter().any(|event| {
            event_client_action_id(event).is_some_and(|value| value == client_action_id)
                || matches!(
                    &event.event,
                    EventBody::QueuedInputResolution(resolution)
                        if resolution.client_action_id == *client_action_id
                )
        });
        if used_by_metadata || used_by_event {
            return Err(client_action_conflict(client_action_id));
        }
        Ok(())
    }

    /// Requires no physical Provider API to be active before a mode transition.
    /// 要求模式转换前不存在活动的物理 Provider API。
    fn require_no_active_api_for_mode_transition(&self) -> Result<(), DomainError> {
        if let Some(api) = &self.active_api {
            return Err(DomainError::ModeTransitionWhileApiActive {
                api_call_id: api.api_call_id.value(),
            });
        }
        Ok(())
    }

    /// Requires the Goal enablement flag to equal one expected value.
    /// 要求 Goal 启用标记等于一个预期值。
    fn require_goal_enabled(&self, expected_enabled: bool) -> Result<(), DomainError> {
        let actual_enabled = self.metadata.mode.goal.enabled;
        if actual_enabled != expected_enabled {
            return Err(DomainError::GoalEnablementMismatch {
                expected_enabled,
                actual_enabled,
            });
        }
        Ok(())
    }

    /// Requires one exact Goal source state before a requested transition.
    /// 在请求转换前要求一个精确的 Goal 源状态。
    fn require_goal_state(
        &self,
        expected: GoalState,
        requested: GoalState,
    ) -> Result<(), DomainError> {
        let current = self.metadata.mode.goal.state;
        if current != expected {
            return Err(DomainError::GoalStateTransition {
                current: goal_state_name(current),
                requested: goal_state_name(requested),
            });
        }
        Ok(())
    }

    /// Publishes one fully staged Goal transition as a single document revision.
    /// 把一次已完整暂存的 Goal 转换发布为单一文档修订。
    fn commit_goal_transition(
        &mut self,
        metadata: MetadataDocument,
        id_allocator: IdAllocator,
        active_turn: Option<ActiveTurnState>,
        staged_events: Vec<ConversationEvent>,
        now_ms: i64,
    ) {
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.metadata = metadata;
        self.id_allocator = id_allocator;
        self.active_turn = active_turn;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
    }

    /// Replaces the streaming text of an existing provider semantic in place.
    /// 原位替换既有 Provider 语义的流式正文。
    fn update_provider_semantic(
        &mut self,
        seq: Seq,
        now_ms: i64,
        replacement: EventBody,
    ) -> Result<IdempotentMutation<Seq>, DomainError> {
        // Resolve the resident event without mutating it so every validation failure is atomic.
        // 先只读解析常驻事件，确保任何校验失败都不会留下部分修改。
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        // Decide whether the cumulative snapshot is a real monotonic evolution or an exact replay.
        // 判定累计快照是真实单调演进还是完全相同的重放。
        let event = self
            .events
            .get(index)
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let changed = match (&event.event, &replacement) {
            (EventBody::AssistantReasoning(current), EventBody::AssistantReasoning(next)) => {
                ensure_api_event_owner(seq, next.api_call_id, current.api_call_id)?;
                validate_provider_semantic_snapshot_transition(
                    &current.text,
                    current.status,
                    current.provider_source.as_ref(),
                    &next.text,
                    next.status,
                    next.provider_source.as_ref(),
                )?
            }
            (EventBody::AssistantText(current), EventBody::AssistantText(next)) => {
                ensure_api_event_owner(seq, next.api_call_id, current.api_call_id)?;
                validate_provider_semantic_snapshot_transition(
                    &current.text,
                    current.status,
                    current.provider_source.as_ref(),
                    &next.text,
                    next.status,
                    next.provider_source.as_ref(),
                )?
            }
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "streaming_kind",
                });
            }
        };
        if !changed {
            return Ok(IdempotentMutation::Idempotent(seq));
        }
        // Publish the already-validated replacement as one in-place event revision.
        // 将已经通过校验的替换值作为一次原位事件修订发布。
        let event = self
            .events
            .get_mut(index)
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        event.mutate_in_place(now_ms, |body| *body = replacement)?;
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &[seq]);
        Ok(IdempotentMutation::Committed(seq))
    }

    /// Applies one in-place update to a tool event after cross-group checks.
    /// 在跨组校验后对工具事件应用一次原位更新。
    fn update_tool_in_place(
        &mut self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        now_ms: i64,
        update: impl FnOnce(&mut ToolCallEvent),
    ) -> Result<(), DomainError> {
        let seq = self
            .tool_seq_by_id
            .get(&tool_call_id)
            .copied()
            .ok_or_else(|| DomainError::reference("tool_call_id", tool_call_id.value()))?;
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        let event = self
            .events
            .get_mut(index)
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let tool = match &event.event {
            EventBody::ToolCall(tool) => tool,
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "tool_call",
                });
            }
        };
        if tool.api_call_id != api_call_id {
            return Err(DomainError::CrossApiGroupUpdate {
                event_seq: seq.value(),
                api_call_id: api_call_id.value(),
            });
        }
        event.mutate_in_place(now_ms, |body| match body {
            EventBody::ToolCall(tool) => update(tool),
            _ => unreachable!("tool event kind was validated before mutation"),
        })?;
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &[seq]);
        Ok(())
    }

    /// Resolves one resident tool and verifies its API and open-turn ownership.
    /// 解析一条常驻工具事件并校验其 API 与开放回合归属。
    fn require_tool_for_api(
        &self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
    ) -> Result<(Seq, &ToolCallEvent), DomainError> {
        let seq = self
            .tool_seq_by_id
            .get(&tool_call_id)
            .copied()
            .ok_or_else(|| DomainError::reference("tool_call_id", tool_call_id.value()))?;
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        let event = self
            .events
            .get(index)
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let tool = match &event.event {
            EventBody::ToolCall(tool) => tool,
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "tool_call",
                });
            }
        };
        if tool.api_call_id != api_call_id {
            return Err(DomainError::CrossApiGroupUpdate {
                event_seq: seq.value(),
                api_call_id: api_call_id.value(),
            });
        }
        let turn = self.require_open_turn()?;
        if event.turn_id != Some(turn.turn_id) {
            return Err(DomainError::TurnIdentityMismatch {
                expected: event.turn_id.map_or(0, TurnId::value),
                actual: turn.turn_id.value(),
            });
        }
        Ok((seq, tool))
    }

    /// Bumps one real memory revision, touches session metadata monotonically,
    /// and records the exact event seq values changed by that revision.
    /// 递增一次真实内存修订、单调更新会话 metadata，并记录该修订真实变化的事件 seq。
    fn bump_revision(&mut self, files: &[DirtyFile], now_ms: i64, changed_event_seqs: &[Seq]) {
        let id_frontier = self.id_frontier();
        let frontier_changed = self.metadata.id_frontier != id_frontier;
        if frontier_changed {
            self.metadata.id_frontier = id_frontier;
        }
        self.metadata.updated_at_ms = self.metadata.updated_at_ms.max(now_ms);
        self.memory_revision += 1;
        let revision = self.memory_revision;
        self.metadata.document_revision = revision;
        let changed: Vec<Seq> = changed_event_seqs
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.changed_event_seqs_by_revision
            .insert(revision, changed);
        for file in files
            .iter()
            .copied()
            .chain(std::iter::once(DirtyFile::Metadata))
        {
            let entry = self.dirty_since.entry(file).or_insert(revision);
            if revision > *entry {
                *entry = revision;
            }
        }
    }

    /// Aggregates the direct/subagent buckets of one active turn from its resident suffix.
    /// 从活动回合的常驻后缀聚合 direct/subagent 桶。
    ///
    /// `start_seq` is the authoritative first event of `turn_id`. Restricting
    /// aggregation to that suffix prevents every completed turn from rescanning
    /// the immutable history while preserving exact filtering for session-scoped
    /// events that may be interleaved with the active turn.
    /// `start_seq` 是 `turn_id` 的权威首事件。把聚合限制到该后缀，可避免每个已完成
    /// 回合重复扫描不可变历史，同时仍精确过滤可能夹在活动回合中的会话级事件。
    fn aggregate_turn_buckets(
        &self,
        turn_id: TurnId,
        start_seq: Seq,
    ) -> Result<TurnStatBuckets, DomainError> {
        let start_index = start_seq
            .value()
            .checked_sub(self.resident_start_seq)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(DomainError::ReferenceViolation {
                kind: "turn_start_event",
                value: start_seq.value(),
            })?;
        let turn_events =
            self.events
                .get(start_index..)
                .ok_or(DomainError::ReferenceViolation {
                    kind: "turn_start_event",
                    value: start_seq.value(),
                })?;
        let first_event = turn_events.first().ok_or(DomainError::ReferenceViolation {
            kind: "turn_start_event",
            value: start_seq.value(),
        })?;
        if first_event.seq != start_seq || first_event.turn_id != Some(turn_id) {
            return Err(DomainError::TurnIdentityMismatch {
                expected: turn_id.value(),
                actual: first_event.turn_id.map_or(0, TurnId::value),
            });
        }
        let subagent_api_ids: BTreeSet<ApiCallId> = turn_events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::ApiStarted(started)
                    if event.turn_id == Some(turn_id)
                        && started.purpose == ApiPurposeKind::Subagent =>
                {
                    Some(started.api_call_id)
                }
                _ => None,
            })
            .collect();
        let mut direct_usages = Vec::new();
        let mut direct_api_count = 0u64;
        let mut direct_tool_count = 0u64;
        let mut subagent_totals = Vec::new();
        let mut counted_subagent_stats = BTreeSet::new();
        for event in turn_events
            .iter()
            .filter(|event| event.turn_id == Some(turn_id))
        {
            match &event.event {
                EventBody::ApiStats(api) if api.purpose != ApiPurposeKind::Subagent => {
                    direct_api_count += 1;
                    direct_usages.push(&api.usage);
                }
                EventBody::ToolCall(tool) if !subagent_api_ids.contains(&tool.api_call_id) => {
                    direct_tool_count += 1;
                }
                EventBody::SubagentReturnStats(return_stats)
                    if return_stats.status == SubagentReturnStatus::Final
                        && counted_subagent_stats.insert((
                            return_stats.child_session_id.clone(),
                            return_stats.child_turn_stats_seq,
                        )) =>
                {
                    subagent_totals.push(&return_stats.totals);
                }
                _ => {}
            }
        }
        let direct =
            StatBucket::from_api_usages(direct_usages, direct_api_count, direct_tool_count, 0);
        let subagent = StatBucket::from_subagent_returns(subagent_totals);
        Ok(TurnStatBuckets::combine(direct, subagent))
    }
}

/// Validates the immutable source boundary of one forked session.
/// 校验一个分叉会话的不可变来源边界。
fn validate_fork_origin(
    child_session_id: &str,
    fork_origin: &ForkOrigin,
) -> Result<(), DomainError> {
    if fork_origin.source_session_id.trim().is_empty()
        || fork_origin.source_session_id.trim() != fork_origin.source_session_id
    {
        return Err(DomainError::InvalidGenerationInput {
            field: "fork_origin.source_session_id",
        });
    }
    if fork_origin.source_session_id == child_session_id {
        return Err(DomainError::InvalidGenerationInput {
            field: "fork_origin.source_session_id_matches_child",
        });
    }
    if !(1..=JS_SAFE_INTEGER_MAX).contains(&fork_origin.source_segment_generation) {
        return Err(DomainError::InvalidGenerationInput {
            field: "fork_origin.source_segment_generation",
        });
    }
    Ok(())
}

/// Validates one exact external identity without normalization or fallback.
/// 校验一个不做归一化或回退的精确外部身份。
fn validate_exact_external_identity(field: &'static str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() || value.trim() != value {
        return Err(DomainError::InvalidStatistics { field });
    }
    Ok(())
}

/// Validates one complete statistics bucket.
/// 校验一个完整统计桶。
fn validate_stat_bucket(bucket: &StatBucket) -> Result<(), DomainError> {
    if bucket.unknown_usage_api_call_count > bucket.api_call_count {
        return Err(DomainError::InvalidStatistics {
            field: "unknown_usage_api_call_count",
        });
    }
    Ok(())
}

/// Validates direct/subagent/total consistency for one terminal Turn statistic.
/// 校验一条终态回合统计的 direct/subagent/total 一致性。
fn validate_turn_stat_buckets(buckets: &TurnStatBuckets) -> Result<(), DomainError> {
    validate_stat_bucket(&buckets.direct)?;
    validate_stat_bucket(&buckets.subagent)?;
    validate_stat_bucket(&buckets.total)?;
    let expected = TurnStatBuckets::combine(buckets.direct.clone(), buckets.subagent.clone());
    if expected.total != buckets.total {
        return Err(DomainError::InvalidStatistics {
            field: "turn_stats.total",
        });
    }
    Ok(())
}

/// Validates the two adjacent events of one atomic final subagent return.
/// 校验一次原子最终子代理回流的两条相邻事件。
fn validate_subagent_return_pair(
    returned: &SubagentReturnEvent,
    stats: &SubagentReturnStatsEvent,
) -> Result<(), DomainError> {
    validate_exact_external_identity("child_session_id", &returned.child_session_id)?;
    validate_exact_external_identity("child_session_id", &stats.child_session_id)?;
    validate_stat_bucket(&stats.totals)?;
    if returned.child_session_id != stats.child_session_id
        || returned.child_task_id != stats.child_task_id
        || returned.child_turn_stats_seq != stats.child_turn_stats_seq
    {
        return Err(DomainError::InvalidStatistics {
            field: "subagent_return.identity",
        });
    }
    if returned.source_from_seq.value() > returned.source_to_seq.value()
        || returned.source_to_seq != returned.child_turn_stats_seq
        || returned.source_assistant_text_seq.value() < returned.source_from_seq.value()
        || returned.source_assistant_text_seq.value() > returned.source_to_seq.value()
    {
        return Err(DomainError::InvalidStatistics {
            field: "subagent_return.source_range",
        });
    }
    if stats.status != SubagentReturnStatus::Final {
        return Err(DomainError::InvalidStatistics {
            field: "subagent_return.status",
        });
    }
    Ok(())
}

/// Validates that every persisted final subagent return is one adjacent atomic pair.
/// 校验每个持久化最终子代理回流都是一组相邻原子事件。
fn validate_subagent_return_history(events: &[ConversationEvent]) -> Result<(), DomainError> {
    let mut identities = BTreeSet::new();
    for (index, event) in events.iter().enumerate() {
        match &event.event {
            EventBody::SubagentReturn(returned) => {
                let next = events
                    .get(index + 1)
                    .ok_or(DomainError::InvalidStatistics {
                        field: "subagent_return_stats_pair",
                    })?;
                let EventBody::SubagentReturnStats(stats) = &next.event else {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return_stats_pair",
                    });
                };
                if event.turn_id.is_none() || next.turn_id != event.turn_id {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return.parent_turn",
                    });
                }
                if event.visibility != EventVisibility::Provider
                    || next.visibility != EventVisibility::UiOnly
                {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return.visibility",
                    });
                }
                validate_subagent_return_pair(returned, stats)?;
                if !identities.insert((
                    event.turn_id,
                    returned.child_session_id.clone(),
                    returned.child_task_id.clone(),
                )) {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return.identity_unique",
                    });
                }
            }
            EventBody::SubagentReturnStats(_)
                if index > 0
                    && matches!(&events[index - 1].event, EventBody::SubagentReturn(_)) => {}
            EventBody::SubagentReturnStats(_) => {
                return Err(DomainError::InvalidStatistics {
                    field: "subagent_return_stats_pair",
                });
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validates and normalizes one Goal definition before any document mutation.
/// 在任何文档修改前校验并规范化一份 Goal 定义。
fn validate_goal_definition(
    objective: String,
    budget_tokens: Option<u64>,
    provider_context: String,
) -> Result<(String, String), DomainError> {
    let objective = objective.trim().to_string();
    if objective.is_empty() || objective.chars().count() > MAX_GOAL_OBJECTIVE_CHARS {
        return Err(DomainError::InvalidGoalObjective {
            max_chars: MAX_GOAL_OBJECTIVE_CHARS,
        });
    }
    if budget_tokens == Some(0) {
        return Err(DomainError::InvalidGoalBudget);
    }
    let provider_context = validate_system_context_content(provider_context)?;
    Ok((objective, provider_context))
}

/// Validates exact final locked segments without deriving kind, target, identity, or order.
/// 校验精确的最终锁定片段，且不推导其种类、目标层、身份或顺序。
/// Validates one exact request projection against the current frozen document frontier.
/// 针对当前冻结文档前沿校验一份精确请求投影。
fn validate_provider_projection_proof(
    document: &SessionDocument,
    proof: &ProviderProjectionProof,
) -> Result<(), DomainError> {
    validate_provider_projection_proof_shape(proof)?;
    if proof.segment_generation != document.metadata.segment_generation {
        return Err(DomainError::InvalidProviderProjection {
            field: "segment_generation",
        });
    }
    if proof.document_revision != document.memory_revision {
        return Err(DomainError::InvalidProviderProjection {
            field: "document_revision",
        });
    }
    if proof.through_seq != document.last_seq()? {
        return Err(DomainError::InvalidProviderProjection {
            field: "through_seq",
        });
    }
    let expected_retained = match document.runtime_lock.current_compaction_id {
        Some(compaction_id) => document
            .compactions
            .iter()
            .find(|node| node.compaction_id == compaction_id)
            .map(|node| node.retained_from_seq)
            .ok_or_else(|| DomainError::reference("current_compaction", compaction_id.value()))?,
        None => Seq::try_new(1)?,
    };
    if proof.retained_from_seq != expected_retained {
        return Err(DomainError::InvalidProviderProjection {
            field: "retained_from_seq",
        });
    }
    Ok(())
}

/// Validates the storage-independent shape of one persisted projection proof.
/// 校验一份持久化投影证明与存储代次无关的结构。
fn validate_provider_projection_proof_shape(
    proof: &ProviderProjectionProof,
) -> Result<(), DomainError> {
    if proof.segment_generation == 0 || proof.segment_generation > JS_SAFE_INTEGER_MAX {
        return Err(DomainError::InvalidProviderProjection {
            field: "segment_generation",
        });
    }
    if proof.document_revision == 0 || proof.document_revision > JS_SAFE_INTEGER_MAX {
        return Err(DomainError::InvalidProviderProjection {
            field: "document_revision",
        });
    }
    if proof.retained_from_seq.value() > proof.through_seq.value() {
        return Err(DomainError::InvalidProviderProjection {
            field: "retained_from_seq",
        });
    }
    if proof.fingerprint_sha256.len() != 64
        || !proof
            .fingerprint_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DomainError::InvalidProviderProjection {
            field: "fingerprint_sha256",
        });
    }
    Ok(())
}

/// Requires one provider-visible system semantic to carry explicit content.
/// 要求一条 Provider 可见系统语义携带明确内容。
fn validate_system_context_content(content: String) -> Result<String, DomainError> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "system_context",
            field: "empty_content",
        });
    }
    Ok(content)
}

/// Validates one cumulative Provider semantic snapshot and returns whether it changed.
/// 校验一份累计 Provider 语义快照，并返回它是否发生了真实变化。
///
/// # Parameters
/// 参数
/// `current_text` and `current_status` are the durable semantic state, while
/// `next_text` and `next_status` are the full cumulative snapshot being applied.
/// Provider source identity is immutable once attached.
/// `current_text` 与 `current_status` 是持久化语义状态，`next_text` 与
/// `next_status` 是待应用的完整累计快照；Provider 来源身份一经附加即不可变。
///
/// # Returns
/// 返回值
/// Returns `true` only for a monotonic text/status evolution and `false` for an
/// exact replay. Regressions, terminal rewrites, and source drift are rejected.
/// 仅单调正文或状态演进返回 `true`，完全相同的重放返回 `false`；正文回退、
/// 终态改写与来源漂移都会被拒绝。
fn validate_provider_semantic_snapshot_transition(
    current_text: &str,
    current_status: AssistantSemanticStatus,
    current_provider_source: Option<&ProviderSource>,
    next_text: &str,
    next_status: AssistantSemanticStatus,
    next_provider_source: Option<&ProviderSource>,
) -> Result<bool, DomainError> {
    if current_provider_source != next_provider_source {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "assistant_semantic",
            field: "provider_source",
        });
    }
    if current_text == next_text && current_status == next_status {
        return Ok(false);
    }
    if current_status != AssistantSemanticStatus::Streaming {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "assistant_semantic",
            field: "terminal_snapshot",
        });
    }
    if !next_text.starts_with(current_text) {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "assistant_semantic",
            field: "cumulative_text",
        });
    }
    Ok(true)
}

/// Validates that one user input carries model-visible text or at least one resource.
/// 校验一条用户输入至少携带模型可见文本或一个资源。
fn validate_user_input(text: String, resource_ids: &[ResourceId]) -> Result<String, DomainError> {
    if text.trim().is_empty() && resource_ids.is_empty() {
        return Err(DomainError::EmptyUserInput);
    }
    Ok(text)
}

/// Returns the external client action id carried by one event, when any.
/// 返回一条事件携带的外部客户端动作 id（如有）。
fn event_client_action_id(event: &ConversationEvent) -> Option<&ClientActionId> {
    match &event.event {
        EventBody::UserInput(input) => Some(&input.client_action_id),
        EventBody::ExecutionControl(control) => control.client_action_id.as_ref(),
        _ => None,
    }
}

/// Copies the exact durable identity of one queued metadata item.
/// 复制一条排队元数据条目的精确持久化身份。
fn queued_input_identity(queued: &QueuedInput) -> QueuedInputIdentity {
    QueuedInputIdentity {
        client_action_id: queued.client_action_id.clone(),
        input_id: queued.input_id,
    }
}

/// Merges one durable queue identity without accepting identity drift.
/// 合并一条持久化队列身份，且不接受身份漂移。
fn merge_queued_input_identity(
    resolved: &mut Option<QueuedInputIdentity>,
    candidate: QueuedInputIdentity,
) -> Result<(), DomainError> {
    if let Some(existing) = resolved {
        if *existing != candidate {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "queued_action_identity_drift",
            });
        }
        return Ok(());
    }
    *resolved = Some(candidate);
    Ok(())
}

/// Clears the staged draft only when it is the exact payload being admitted.
/// 仅当 staged 草稿与正在准入的载荷完全一致时才清空草稿。
fn consume_matching_draft(
    metadata: &mut MetadataDocument,
    text: &str,
    resource_ids: &[ResourceId],
) -> bool {
    if metadata.input_state.draft.text != text
        || metadata.input_state.draft.resource_ids != resource_ids
    {
        return false;
    }
    metadata.input_state.draft = DraftState::default();
    true
}

/// Builds one stable client-action semantic conflict.
/// 构建一条稳定的客户端动作语义冲突。
fn client_action_conflict(client_action_id: &ClientActionId) -> DomainError {
    DomainError::ClientActionConflict {
        action_id: client_action_id.as_str().to_string(),
    }
}

/// Builds the stable semantic key owned by one Continue guidance action.
/// 构建由一次 Continue 引导动作拥有的稳定语义键。
fn continuation_guidance_key(client_action_id: &ClientActionId) -> String {
    format!("continuation_guidance/{}", client_action_id.as_str())
}

/// Validates the exact purpose-to-Continue-action shape of one API start.
/// 校验一次 API 启动的用途与 Continue 动作之间的精确结构。
fn validate_api_continuation_binding_shape(
    purpose: ApiPurposeKind,
    continuation_action_id: Option<&ClientActionId>,
) -> Result<(), DomainError> {
    match (
        purpose == ApiPurposeKind::Continue,
        continuation_action_id.is_some(),
    ) {
        (true, false) => Err(DomainError::InvalidContinuationHistory {
            field: "api_started.continuation_action_id_missing",
        }),
        (false, true) => Err(DomainError::InvalidContinuationHistory {
            field: "api_started.continuation_action_id_unexpected",
        }),
        _ => Ok(()),
    }
}

/// Validates durable Continue-to-API bindings and transport observations.
/// 校验持久化 Continue 到 API 的绑定及 transport 观察事实。
fn validate_api_dispatch_history(events: &[ConversationEvent]) -> Result<(), DomainError> {
    let mut starts = BTreeMap::new();
    let mut bound_actions = BTreeSet::new();
    for event in events {
        let EventBody::ApiStarted(started) = &event.event else {
            continue;
        };
        validate_api_continuation_binding_shape(
            started.purpose,
            started.continuation_action_id.as_ref(),
        )?;
        if starts
            .insert(started.api_call_id, (event, started))
            .is_some()
        {
            return Err(DomainError::InvalidContinuationHistory {
                field: "duplicate_api_started",
            });
        }
        let Some(action_id) = started.continuation_action_id.as_ref() else {
            continue;
        };
        if !bound_actions.insert(action_id.clone()) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "duplicate_continue_api_binding",
            });
        }
        let mut requested = None;
        let mut resumed = None;
        for candidate in events
            .iter()
            .take_while(|candidate| candidate.seq < event.seq)
        {
            let EventBody::ExecutionControl(control) = &candidate.event else {
                continue;
            };
            if control.client_action_id.as_ref() != Some(action_id) {
                continue;
            }
            match control.control_kind {
                ExecutionControlKind::TurnContinueRequested => requested = Some(candidate),
                ExecutionControlKind::TurnResumed => resumed = Some(candidate),
                _ => {}
            }
        }
        let requested = requested.ok_or(DomainError::InvalidContinuationHistory {
            field: "continue_api.missing_requested",
        })?;
        let resumed = resumed.ok_or(DomainError::InvalidContinuationHistory {
            field: "continue_api.missing_resumed",
        })?;
        if requested.turn_id != event.turn_id
            || resumed.turn_id != event.turn_id
            || requested.seq.value() >= resumed.seq.value()
        {
            return Err(DomainError::InvalidContinuationHistory {
                field: "continue_api.turn_or_order",
            });
        }
    }

    let mut observed_api_ids = BTreeSet::new();
    for event in events {
        let EventBody::ExecutionControl(control) = &event.event else {
            continue;
        };
        if control.control_kind != ExecutionControlKind::ApiTransportObserved {
            continue;
        }
        let api_call_id = control
            .api_call_id
            .ok_or(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed.api_call_id_missing",
            })?;
        if !observed_api_ids.insert(api_call_id) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "duplicate_api_transport_observed",
            });
        }
        let (started_event, _) = starts.get(&api_call_id).ok_or_else(|| {
            DomainError::reference("api_transport_observed.api_started", api_call_id.value())
        })?;
        if event.turn_id != started_event.turn_id || event.seq <= started_event.seq {
            return Err(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed.turn_or_order",
            });
        }
        if events.iter().any(|candidate| {
            candidate.seq < event.seq
                && matches!(
                    &candidate.event,
                    EventBody::ApiStats(stats) if stats.api_call_id == api_call_id
                )
        }) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed_after_terminal",
            });
        }
    }
    Ok(())
}

/// Validates the external identity shape of one execution-control event.
/// 校验一条执行控制事件的外部身份结构。
fn validate_execution_control_event(control: &ExecutionControlEvent) -> Result<(), DomainError> {
    let is_continue = matches!(
        control.control_kind,
        ExecutionControlKind::TurnContinueRequested | ExecutionControlKind::TurnResumed
    );
    let is_transport = control.control_kind == ExecutionControlKind::ApiTransportObserved;
    match (is_continue, control.client_action_id.is_some()) {
        (true, false) => Err(DomainError::InvalidContinuationHistory {
            field: "execution_control.client_action_id_missing",
        }),
        (false, true) => Err(DomainError::InvalidContinuationHistory {
            field: "execution_control.client_action_id_unexpected",
        }),
        _ if is_continue && control.api_call_id.is_some() => {
            Err(DomainError::InvalidContinuationHistory {
                field: "execution_control.api_call_id",
            })
        }
        _ if is_transport && control.api_call_id.is_none() => {
            Err(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed.api_call_id_missing",
            })
        }
        _ => Ok(()),
    }
}

/// Validates global client-action uniqueness and continuation pair structure.
/// 校验全局客户端动作唯一性与继续事件对结构。
fn validate_client_action_history(
    metadata: &MetadataDocument,
    events: &[ConversationEvent],
) -> Result<(), DomainError> {
    let mut metadata_ids = BTreeSet::new();
    for client_action_id in metadata
        .input_state
        .pending_in_turn_inputs
        .iter()
        .map(|pending| &pending.client_action_id)
        .chain(
            metadata
                .input_state
                .queued_inputs
                .iter()
                .map(|queued| &queued.client_action_id),
        )
        .chain(
            metadata
                .input_state
                .draft
                .restored_from_queue
                .iter()
                .map(|identity| &identity.client_action_id),
        )
    {
        if !metadata_ids.insert(client_action_id.clone()) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "duplicate_metadata_client_action_id",
            });
        }
    }

    let mut grouped: BTreeMap<ClientActionId, Vec<&ConversationEvent>> = BTreeMap::new();
    for event in events {
        if let EventBody::UserInput(input) = &event.event {
            validate_user_input(input.text.clone(), &input.resource_ids)?;
        }
        if let Some(client_action_id) = event_client_action_id(event) {
            if metadata_ids.contains(client_action_id) {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "client_action_in_metadata_and_stream",
                });
            }
            grouped
                .entry(client_action_id.clone())
                .or_default()
                .push(event);
        }
    }

    for action_events in grouped.values() {
        let mut input: Option<&ConversationEvent> = None;
        let mut requested: Option<&ConversationEvent> = None;
        let mut resumed: Option<&ConversationEvent> = None;
        for event in action_events {
            match &event.event {
                EventBody::UserInput(_) => {
                    if input.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_stream_client_action_id",
                        });
                    }
                }
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::TurnContinueRequested =>
                {
                    if requested.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_continue_requested",
                        });
                    }
                }
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::TurnResumed =>
                {
                    if resumed.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_turn_resumed",
                        });
                    }
                }
                _ => {
                    return Err(DomainError::InvalidContinuationHistory {
                        field: "unexpected_client_action_event",
                    });
                }
            }
        }
        if requested.is_none() && resumed.is_none() {
            if input.is_none() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "empty_client_action",
                });
            }
            continue;
        }
        let requested = requested.ok_or(DomainError::InvalidContinuationHistory {
            field: "missing_continue_requested",
        })?;
        let resumed = resumed.ok_or(DomainError::InvalidContinuationHistory {
            field: "missing_turn_resumed",
        })?;
        let turn_id = requested
            .turn_id
            .ok_or(DomainError::InvalidContinuationHistory {
                field: "continue_requested.turn_id",
            })?;
        if resumed.turn_id != Some(turn_id) || requested.seq.value() >= resumed.seq.value() {
            return Err(DomainError::InvalidContinuationHistory {
                field: "continue_pair_order",
            });
        }
        if let Some(input) = input
            && (input.turn_id != Some(turn_id) || input.seq.value() >= requested.seq.value())
        {
            return Err(DomainError::InvalidContinuationHistory {
                field: "continue_input_order",
            });
        }
    }
    Ok(())
}

/// Validates durable queued-input terminal receipts against current metadata and stream identities.
/// 校验持久化排队输入终结 receipt 与当前元数据及事件流身份的一致性。
fn validate_queued_input_resolution_history(
    metadata: &MetadataDocument,
    events: &[ConversationEvent],
) -> Result<(), DomainError> {
    let mut actions = BTreeSet::new();
    let mut input_ids = BTreeSet::new();
    for event in events {
        let EventBody::QueuedInputResolution(resolution) = &event.event else {
            continue;
        };
        validate_user_input(resolution.text.clone(), &resolution.resource_ids)?;
        if event.visibility != EventVisibility::UiOnly {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "resolution_event_shape",
            });
        }
        if !actions.insert(resolution.client_action_id.clone())
            || !input_ids.insert(resolution.input_id)
        {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "duplicate_resolution_identity",
            });
        }
        if metadata.input_state.queued_inputs.iter().any(|queued| {
            queued.client_action_id == resolution.client_action_id
                || queued.input_id == resolution.input_id
        }) {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "resolution_identity_still_pending",
            });
        }
        match resolution.resolution {
            QueuedInputResolutionKind::MovedToDraft | QueuedInputResolutionKind::Removed => {
                if event.turn_id.is_some() {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "terminal_resolution_turn_id",
                    });
                }
                if metadata
                    .input_state
                    .pending_in_turn_inputs
                    .iter()
                    .any(|pending| {
                        pending.client_action_id == resolution.client_action_id
                            || pending.input_id == resolution.input_id
                    })
                {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "resolution_identity_still_pending",
                    });
                }
                if events.iter().any(|candidate| {
                    matches!(
                        &candidate.event,
                        EventBody::UserInput(input)
                            if input.client_action_id == resolution.client_action_id
                                || input.input_id == resolution.input_id
                    )
                }) {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "resolution_identity_already_scheduled",
                    });
                }
                if let Some(draft_identity) =
                    metadata.input_state.draft.restored_from_queue.as_ref()
                    && (draft_identity.client_action_id == resolution.client_action_id
                        || draft_identity.input_id == resolution.input_id)
                {
                    let exact_moved_draft = resolution.resolution
                        == QueuedInputResolutionKind::MovedToDraft
                        && draft_identity.client_action_id == resolution.client_action_id
                        && draft_identity.input_id == resolution.input_id
                        && metadata.input_state.draft.text == resolution.text
                        && metadata.input_state.draft.resource_ids == resolution.resource_ids;
                    if !exact_moved_draft {
                        return Err(DomainError::InvalidQueuedInputOrder {
                            field: "resolution_draft_identity_conflict",
                        });
                    }
                }
            }
            QueuedInputResolutionKind::PromotedToPendingInTurn => {
                let turn_id = event.turn_id.ok_or(DomainError::InvalidQueuedInputOrder {
                    field: "promoted_resolution_turn_id",
                })?;
                if metadata
                    .input_state
                    .draft
                    .restored_from_queue
                    .as_ref()
                    .is_some_and(|draft_identity| {
                        draft_identity.client_action_id == resolution.client_action_id
                            || draft_identity.input_id == resolution.input_id
                    })
                {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "promoted_resolution_draft_conflict",
                    });
                }
                let mut exact_pending = false;
                for pending in &metadata.input_state.pending_in_turn_inputs {
                    let action_matches = pending.client_action_id == resolution.client_action_id;
                    let input_matches = pending.input_id == resolution.input_id;
                    if action_matches && input_matches {
                        if pending.turn_id != turn_id
                            || pending.text != resolution.text
                            || pending.resource_ids != resolution.resource_ids
                            || exact_pending
                        {
                            return Err(DomainError::InvalidQueuedInputOrder {
                                field: "promoted_resolution_pending_conflict",
                            });
                        }
                        exact_pending = true;
                    } else if action_matches || input_matches {
                        return Err(DomainError::InvalidQueuedInputOrder {
                            field: "promoted_resolution_pending_conflict",
                        });
                    }
                }
                let mut exact_streamed = false;
                for candidate in events {
                    let EventBody::UserInput(input) = &candidate.event else {
                        continue;
                    };
                    let action_matches = input.client_action_id == resolution.client_action_id;
                    let input_matches = input.input_id == resolution.input_id;
                    if action_matches && input_matches {
                        if candidate.turn_id != Some(turn_id)
                            || input.input_source != UserInputSource::PendingInTurn
                            || input.text != resolution.text
                            || input.resource_ids != resolution.resource_ids
                            || exact_streamed
                        {
                            return Err(DomainError::InvalidQueuedInputOrder {
                                field: "promoted_resolution_stream_conflict",
                            });
                        }
                        exact_streamed = true;
                    } else if action_matches || input_matches {
                        return Err(DomainError::InvalidQueuedInputOrder {
                            field: "promoted_resolution_stream_conflict",
                        });
                    }
                }
                if exact_pending == exact_streamed {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "promoted_resolution_owner_count",
                    });
                }
            }
        }
        if resolution.resolution != QueuedInputResolutionKind::MovedToDraft
            && metadata
                .input_state
                .draft
                .restored_from_queue
                .as_ref()
                .is_some_and(|draft_identity| {
                    draft_identity.client_action_id == resolution.client_action_id
                        || draft_identity.input_id == resolution.input_id
                })
        {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "resolution_draft_identity_conflict",
            });
        }
        if resolution.resolution == QueuedInputResolutionKind::MovedToDraft
            && let Some(draft_identity) = metadata.input_state.draft.restored_from_queue.as_ref()
            && (draft_identity.client_action_id == resolution.client_action_id
                || draft_identity.input_id == resolution.input_id)
            && !(draft_identity.client_action_id == resolution.client_action_id
                && draft_identity.input_id == resolution.input_id
                && metadata.input_state.draft.text == resolution.text
                && metadata.input_state.draft.resource_ids == resolution.resource_ids)
        {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "resolution_draft_identity_conflict",
            });
        }
    }
    Ok(())
}

/// Validates persisted request-budget evidence at both commit and strict reload boundaries.
/// 在提交与严格重载边界同时校验持久化请求预算证据。
fn validate_provider_request_budget_evidence(
    evidence: &ProviderRequestBudgetEvidence,
) -> Result<(), DomainError> {
    validate_provider_budget_fingerprint(
        "request_assembly_fingerprint",
        &evidence.request_assembly_fingerprint,
    )?;
    validate_provider_budget_fingerprint(
        "model_route_fingerprint",
        &evidence.model_route_fingerprint,
    )?;
    validate_provider_budget_fingerprint("toolset_fingerprint", &evidence.toolset_fingerprint)?;
    validate_provider_budget_fingerprint(
        "system_prompt_fingerprint",
        &evidence.system_prompt_fingerprint,
    )?;
    let measurement = &evidence.request_text_measurement;
    for (field, value) in [
        ("character_count", measurement.character_count),
        ("utf8_byte_count", measurement.utf8_byte_count),
        ("structural_units", measurement.structural_units),
        ("history_message_count", measurement.history_message_count),
        (
            "context_breakdown.message_units",
            measurement.context_breakdown.message_units,
        ),
        (
            "context_breakdown.system_tool_units",
            measurement.context_breakdown.system_tool_units,
        ),
        (
            "context_breakdown.mcp_tool_units",
            measurement.context_breakdown.mcp_tool_units,
        ),
        (
            "context_breakdown.skill_units",
            measurement.context_breakdown.skill_units,
        ),
        (
            "context_breakdown.system_prompt_units",
            measurement.context_breakdown.system_prompt_units,
        ),
        (
            "context_breakdown.other_units",
            measurement.context_breakdown.other_units,
        ),
    ] {
        if value > JS_SAFE_INTEGER_MAX {
            return Err(DomainError::InvalidProviderBudgetEvidence { field });
        }
    }
    if measurement.utf8_byte_count < measurement.character_count {
        return Err(DomainError::InvalidProviderBudgetEvidence {
            field: "utf8_byte_count",
        });
    }
    validate_provider_budget_fingerprint("history_fingerprint", &measurement.history_fingerprint)?;
    validate_provider_budget_fingerprint(
        "prefix_invariant_fingerprint",
        &measurement.prefix_invariant_fingerprint,
    )
}

/// Validates one lowercase SHA-256 identity without accepting alternate encodings.
/// 校验一个小写 SHA-256 身份，不接受其他编码形式。
fn validate_provider_budget_fingerprint(
    field: &'static str,
    value: &str,
) -> Result<(), DomainError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(DomainError::InvalidProviderBudgetEvidence { field });
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DomainError::InvalidProviderBudgetEvidence { field });
    }
    Ok(())
}

/// Builds deterministic valid Provider budget evidence for unit fixtures only.
/// 仅为单元测试夹具构建确定且有效的 Provider 预算证据。
#[cfg(test)]
fn test_provider_request_budget_evidence() -> ProviderRequestBudgetEvidence {
    ProviderRequestBudgetEvidence {
        request_assembly_fingerprint: format!("sha256:{}", "0".repeat(64)),
        model_route_fingerprint: format!("sha256:{}", "1".repeat(64)),
        toolset_fingerprint: format!("sha256:{}", "2".repeat(64)),
        system_prompt_fingerprint: format!("sha256:{}", "3".repeat(64)),
        request_text_measurement: crate::v1::budget::ProviderRequestTextMeasurement {
            character_count: 0,
            utf8_byte_count: 0,
            structural_units: 0,
            history_message_count: 0,
            prompt_in_history: false,
            history_fingerprint: format!("sha256:{}", "4".repeat(64)),
            prefix_invariant_fingerprint: format!("sha256:{}", "5".repeat(64)),
            context_breakdown: crate::v1::budget::ProviderRequestContextBreakdown::default(),
        },
    }
}

/// Builds one strongly typed Goal system-context event body.
/// 构建一个强类型 Goal 系统上下文事件体。
fn goal_context_event(
    context_key: &str,
    operation: SystemContextOperation,
    content: String,
) -> EventBody {
    EventBody::SystemContext(SystemContextEvent {
        context_key: context_key.to_string(),
        operation,
        scope: SystemContextScope::Session,
        ttl_turns: None,
        provider_role_hint: Some(SystemRoleHint::Developer),
        content,
    })
}

/// Builds one permanent informational Goal timeline notice.
/// 构建一条永久的信息级 Goal 时间线通知。
fn goal_notice_event(reason_code: &str) -> EventBody {
    EventBody::DisplayNote(DisplayNoteEvent {
        notice_kind: DisplayNoticeKind::GoalStatus,
        severity: NoticeSeverity::Info,
        reason_code: reason_code.to_string(),
        redacted_detail: None,
        api_call_id: None,
        retry_count: None,
        retry_limit: None,
    })
}

/// Returns the stable diagnostic name of one Goal state.
/// 返回一个 Goal 状态的稳定诊断名。
const fn goal_state_name(state: GoalState) -> &'static str {
    match state {
        GoalState::Active => "active",
        GoalState::Paused => "paused",
        GoalState::Completed => "completed",
        GoalState::Blocked => "blocked",
        GoalState::BudgetExhausted => "budget_exhausted",
    }
}

/// Validates one typed Provider source against the immutable active API route.
/// 校验一份强类型 Provider 来源是否匹配不可变活动 API 路由。
fn validate_provider_source_input(
    api: &ActiveApiState,
    source_input: &ProviderSourceInput,
) -> Result<(), DomainError> {
    ensure_api_request_field(
        "provider_source.provider",
        &api.request.provider,
        &source_input.provider,
    )?;
    ensure_api_request_field(
        "provider_source.profile",
        &api.request.profile,
        &source_input.profile,
    )?;
    ensure_api_request_field(
        "provider_source.model",
        &api.request.model,
        &source_input.model,
    )?;
    ensure_api_request_field(
        "provider_source.connection_id",
        &api.request.connection_id,
        &source_input.connection_id,
    )?;
    if let Some(replay) = &source_input.replay {
        ensure_api_request_field(
            "provider_source.replay.provider_id",
            &source_input.provider,
            &replay.provider_id,
        )?;
        ensure_api_request_field(
            "provider_source.replay.connection_id",
            &source_input.connection_id.as_deref().unwrap_or_default(),
            &replay.connection_id.as_str(),
        )?;
        ensure_api_request_field(
            "provider_source.replay.protocol_profile_id",
            &source_input.profile.as_deref().unwrap_or_default(),
            &replay.protocol_profile_id.as_str(),
        )?;
        ensure_api_request_field(
            "provider_source.replay.model_id",
            &source_input.model,
            &replay.model_id,
        )?;
    }
    Ok(())
}

/// Builds the authoritative Provider source for one document-selected coverage boundary.
/// 为文档选定的覆盖边界构建权威 Provider 来源。
fn build_provider_source(
    events: &[ConversationEvent],
    covered_until_seq: Seq,
    source_input: &ProviderSourceInput,
) -> Result<ProviderSource, DomainError> {
    let fingerprint_sha256 = provider_visible_fingerprint(events, covered_until_seq)?;
    Ok(ProviderSource {
        provider: source_input.provider.clone(),
        profile: source_input.profile.clone(),
        model: source_input.model.clone(),
        connection_id: source_input.connection_id.clone(),
        message_id: source_input.message_id.clone(),
        response_id: source_input.response_id.clone(),
        replay: source_input.replay.clone(),
        covered_until_seq,
        fingerprint_sha256,
    })
}

/// Compares one persisted Provider source with its original typed input.
/// 将一条已持久化 Provider 来源与其原始强类型输入进行比较。
fn provider_source_matches_input(source: &ProviderSource, input: &ProviderSourceInput) -> bool {
    source.provider == input.provider
        && source.profile == input.profile
        && source.model == input.model
        && source.connection_id == input.connection_id
        && source.message_id == input.message_id
        && source.response_id == input.response_id
        && source.replay == input.replay
}

/// Returns the exact Provider source hosted by one resident event sequence.
/// 返回一个常驻事件序列承载的精确 Provider 来源。
fn provider_source_at_seq(
    events: &[ConversationEvent],
    resident_start_seq: u64,
    seq: Seq,
) -> Result<ProviderSource, DomainError> {
    let index =
        seq.value()
            .checked_sub(resident_start_seq)
            .ok_or(DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            })? as usize;
    events
        .get(index)
        .and_then(provider_source_of)
        .ok_or_else(|| DomainError::reference("provider_source", seq.value()))
}

/// Requires a previously durable Provider source to equal the terminal typed source completely.
/// 要求此前已可靠落盘的 Provider 来源与终态强类型来源完全一致。
fn ensure_provider_source_matches(
    expected: &ProviderSource,
    actual: &ProviderSource,
) -> Result<(), DomainError> {
    ensure_api_request_field(
        "provider_source.provider",
        &expected.provider,
        &actual.provider,
    )?;
    ensure_api_request_field(
        "provider_source.profile",
        &expected.profile,
        &actual.profile,
    )?;
    ensure_api_request_field("provider_source.model", &expected.model, &actual.model)?;
    ensure_api_request_field(
        "provider_source.connection_id",
        &expected.connection_id,
        &actual.connection_id,
    )?;
    ensure_api_request_field(
        "provider_source.message_id",
        &expected.message_id,
        &actual.message_id,
    )?;
    ensure_api_request_field(
        "provider_source.response_id",
        &expected.response_id,
        &actual.response_id,
    )?;
    ensure_api_request_field("provider_source.replay", &expected.replay, &actual.replay)?;
    ensure_api_request_field(
        "provider_source.covered_until_seq",
        &expected.covered_until_seq,
        &actual.covered_until_seq,
    )?;
    ensure_api_request_field(
        "provider_source.fingerprint_sha256",
        &expected.fingerprint_sha256,
        &actual.fingerprint_sha256,
    )
}

/// Validates exact Provider draft and tool-call identities before batch allocation.
/// 在批次分配前校验精确的 Provider 草稿与工具调用身份。
fn validate_tool_preparations(
    preparations: &[ToolPreparation],
    require_provider_identity: bool,
) -> Result<(), DomainError> {
    if preparations.is_empty() {
        return Err(DomainError::InvalidToolIdentity {
            field: "tool_batch_empty",
        });
    }
    let mut request_stream_ids = BTreeSet::new();
    let mut provider_tool_call_ids = BTreeSet::new();
    for preparation in preparations {
        if !request_stream_ids.insert(preparation.request_stream_id.as_str()) {
            return Err(DomainError::InvalidToolIdentity {
                field: "request_stream_id",
            });
        }
        match preparation.provider_tool_call_id.as_deref() {
            Some(value) if !value.trim().is_empty() => {
                if !provider_tool_call_ids.insert(value) {
                    return Err(DomainError::InvalidToolIdentity {
                        field: "provider_tool_call_id",
                    });
                }
            }
            None if !require_provider_identity => {}
            _ => {
                return Err(DomainError::InvalidToolIdentity {
                    field: "provider_tool_call_id",
                });
            }
        }
    }
    Ok(())
}

/// Requires one completion/source field to equal the immutable API-start snapshot.
/// 要求一个终态或来源字段等于不可变 API 起始快照。
fn ensure_api_request_field<T>(
    field: &'static str,
    expected: &T,
    actual: &T,
) -> Result<(), DomainError>
where
    T: PartialEq + std::fmt::Debug,
{
    if expected == actual {
        return Ok(());
    }
    Err(DomainError::ApiRequestMismatch {
        field,
        expected: format!("{expected:?}"),
        actual: format!("{actual:?}"),
    })
}

/// Returns the stable serialized name of one tool status for diagnostics.
/// 返回一条工具状态用于诊断的稳定序列化名称。
fn tool_status_name(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Prepared => "prepared",
        ToolCallStatus::Running => "running",
        ToolCallStatus::WaitingUser => "waiting_user",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        ToolCallStatus::Cancelled => "cancelled",
        ToolCallStatus::Stopped => "stopped",
        ToolCallStatus::Interrupted => "interrupted",
    }
}

/// Returns whether a tool status is a durable terminal state.
/// 返回工具状态是否为持久化终态。
const fn is_terminal_tool_status(status: ToolCallStatus) -> bool {
    matches!(
        status,
        ToolCallStatus::Completed
            | ToolCallStatus::Failed
            | ToolCallStatus::Cancelled
            | ToolCallStatus::Stopped
            | ToolCallStatus::Interrupted
    )
}

/// Validates a persisted Unix-millisecond timestamp.
/// 校验一个持久化 Unix 毫秒时间戳。
fn validate_timestamp(field: &'static str, value: i64) -> Result<(), DomainError> {
    if value < 0 {
        return Err(DomainError::InvalidTimestamp { field, value });
    }
    Ok(())
}

/// Normalizes one optional catalog or routing selection.
/// 规范化一个可选目录或路由选择值。
fn normalize_optional_selection(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let normalized = value.trim();
        (!normalized.is_empty()).then(|| normalized.to_string())
    })
}

/// Validates one required session-selection component without normalization.
/// 校验一个必需会话选择组件，且不执行规范化。
fn validate_required_selection_component(
    field: &'static str,
    value: &str,
) -> Result<(), DomainError> {
    if value.is_empty() || value.trim() != value {
        return Err(DomainError::InvalidSelectionConfiguration { field });
    }
    Ok(())
}

/// Validates one exact Provider target identity.
/// 校验一个精确 Provider 目标身份。
fn validate_provider_tool_target_config(
    target: &ProviderToolTargetConfig,
) -> Result<(), DomainError> {
    validate_required_selection_component(
        "provider_tools.target.connection_id",
        &target.connection_id,
    )?;
    validate_required_selection_component("provider_tools.target.target_id", &target.target_id)
}

/// Validates one canonically ordered direct capability selection list.
/// 校验一份按规范排序的直接能力选择列表。
fn validate_provider_tool_capability_selections(
    selections: &[ProviderToolCapabilitySelection],
) -> Result<(), DomainError> {
    for selection in selections {
        validate_required_selection_component(
            "provider_tools.selections.capability",
            &selection.capability,
        )?;
        validate_provider_tool_target_config(&selection.target)?;
    }
    if selections
        .windows(2)
        .any(|pair| pair[0].capability >= pair[1].capability)
    {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.selections.order",
        });
    }
    Ok(())
}

/// Validates one canonically ordered media-mode selection list.
/// 校验一份按规范排序的媒体模式选择列表。
fn validate_provider_media_understanding_selections(
    selections: &[ProviderMediaUnderstandingSelection],
) -> Result<(), DomainError> {
    for selection in selections {
        validate_required_selection_component(
            "provider_tools.media_understanding_modes.capability",
            &selection.capability,
        )?;
    }
    if selections
        .windows(2)
        .any(|pair| pair[0].capability >= pair[1].capability)
    {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.media_understanding_modes.order",
        });
    }
    Ok(())
}

/// Validates one canonically ordered native-extension target-set list.
/// 校验一份按规范排序的原生扩展目标集合列表。
fn validate_provider_extension_target_sets(
    target_sets: &[ProviderExtensionTargetSetConfig],
) -> Result<(), DomainError> {
    for target_set in target_sets {
        validate_required_selection_component(
            "provider_tools.extension_target_sets.capability",
            &target_set.capability,
        )?;
        if target_set.targets.is_empty() {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "provider_tools.extension_target_sets.targets",
            });
        }
        for target in &target_set.targets {
            validate_provider_tool_target_config(target)?;
        }
        if target_set.targets.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "provider_tools.extension_target_sets.target_order",
            });
        }
    }
    if target_sets
        .windows(2)
        .any(|pair| pair[0].capability >= pair[1].capability)
    {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.extension_target_sets.order",
        });
    }
    Ok(())
}

/// Validates the complete session-owned Provider tool selection at commit and reload boundaries.
/// 在提交与重载边界校验当前会话持有的完整 Provider 工具选择。
fn validate_provider_tool_selection_config(
    config: &ProviderToolSelectionConfig,
) -> Result<(), DomainError> {
    if !config.initialized && !config.is_empty() {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.initialized",
        });
    }
    for capability in &config.enabled_capabilities {
        validate_required_selection_component("provider_tools.enabled_capabilities", capability)?;
    }
    if config
        .enabled_capabilities
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.enabled_capabilities.order",
        });
    }
    validate_provider_tool_capability_selections(&config.selections)?;
    validate_provider_media_understanding_selections(&config.media_understanding_modes)?;
    validate_provider_extension_target_sets(&config.extension_target_sets)
}

/// Validates one complete knowledge-library selection at commit and reload boundaries.
/// 在提交与重载边界校验一份完整的知识库选择。
fn validate_knowledge_library_selection_config(
    config: &KnowledgeLibrarySelectionConfig,
) -> Result<(), DomainError> {
    if config.mode == KnowledgeLibrarySelectionMode::Inherit && !config.library_ids.is_empty() {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.inherit_ids",
        });
    }
    for library_id in &config.library_ids {
        validate_required_selection_component("knowledge_libraries.library_ids", library_id)?;
    }
    if config.library_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.library_ids.order",
        });
    }
    Ok(())
}

/// Validates one required request-routing selection without normalizing persisted facts.
/// 校验一个必需请求路由选择，且不规范化已持久化事实。
fn validate_required_request_selection(
    field: &'static str,
    value: &str,
) -> Result<(), DomainError> {
    if value.is_empty() || value.trim() != value {
        return Err(DomainError::InvalidRequestConfiguration { field });
    }
    Ok(())
}

/// Validates one optional request-routing selection without treating whitespace as absence.
/// 校验一个可选请求路由选择，且不把空白值当作缺省值。
fn validate_optional_request_selection(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), DomainError> {
    if let Some(value) = value {
        validate_required_request_selection(field, value)?;
    }
    Ok(())
}

/// Validates the provider-agnostic numeric request configuration.
/// 校验与 Provider 无关的数值请求配置。
fn validate_request_temperature(temperature: Option<f64>) -> Result<(), DomainError> {
    if temperature.is_some_and(|temperature| !temperature.is_finite()) {
        return Err(DomainError::InvalidRequestConfiguration {
            field: "temperature",
        });
    }
    Ok(())
}

/// Validates one exact physical API request snapshot at both commit and reload boundaries.
/// 在提交与重载边界同时校验一份精确物理 API 请求快照。
fn validate_api_request_snapshot(request: &ApiRequestSnapshot) -> Result<(), DomainError> {
    validate_required_request_selection("provider", &request.provider)?;
    validate_optional_request_selection("profile", request.profile.as_deref())?;
    validate_required_request_selection("model", &request.model)?;
    validate_optional_request_selection("connection_id", request.connection_id.as_deref())?;
    validate_optional_request_selection("thinking_level", request.thinking_level.as_deref())?;
    validate_request_temperature(request.temperature)
}

/// Validates the authoritative active request configuration stored in metadata.
/// 校验保存在 metadata 中的权威当前请求配置。
fn validate_active_request_config(config: &ActiveRequestConfig) -> Result<(), DomainError> {
    validate_optional_request_selection("active.provider", config.provider.as_deref())?;
    validate_optional_request_selection("active.connection_id", config.connection_id.as_deref())?;
    validate_optional_request_selection("active.model", config.model.as_deref())?;
    validate_optional_request_selection("active.thinking_level", config.thinking_level.as_deref())?;
    validate_request_temperature(config.temperature)
}

/// Validates one pending safe-boundary request configuration without guessing inheritance.
/// 校验一份待安全边界生效的请求配置，且不猜测继承语义。
fn validate_pending_request_config(config: &PendingRequestConfig) -> Result<(), DomainError> {
    validate_optional_request_selection("pending.provider", config.provider.as_deref())?;
    validate_optional_request_selection("pending.connection_id", config.connection_id.as_deref())?;
    validate_optional_request_selection("pending.model", config.model.as_deref())?;
    validate_optional_request_selection(
        "pending.thinking_level",
        config.thinking_level.as_deref(),
    )?;
    validate_request_temperature(config.temperature)
}

/// Converts one exact API request snapshot into the session's active request configuration.
/// 把一份精确 API 请求快照转换为会话当前请求配置。
fn active_request_from_snapshot(request: &ApiRequestSnapshot) -> ActiveRequestConfig {
    ActiveRequestConfig {
        provider: Some(request.provider.clone()),
        connection_id: request.connection_id.clone(),
        model: Some(request.model.clone()),
        thinking_level: request.thinking_level.clone(),
        temperature: request.temperature,
    }
}

/// Requires the dispatched request configuration to equal the current metadata authority.
/// 要求已派发请求配置与当前 metadata 权威完全一致。
fn validate_request_against_active_config(
    active: &ActiveRequestConfig,
    requested: &ActiveRequestConfig,
) -> Result<(), DomainError> {
    ensure_api_request_field("active.provider", &active.provider, &requested.provider)?;
    ensure_api_request_field(
        "active.connection_id",
        &active.connection_id,
        &requested.connection_id,
    )?;
    ensure_api_request_field("active.model", &active.model, &requested.model)?;
    ensure_api_request_field(
        "active.thinking_level",
        &active.thinking_level,
        &requested.thinking_level,
    )?;
    ensure_api_request_field(
        "active.temperature",
        &active.temperature,
        &requested.temperature,
    )?;
    Ok(())
}

/// Validates the single legal terminal status/reason/error/timing contract of one API.
/// 校验一次 API 唯一合法的终态状态、原因、错误与时间契约。
///
/// # Parameters
/// 参数
/// `status`, `end_reason`, and `error` describe the terminal classification; `timing`
/// contains the ordered physical checkpoints persisted in `ApiStats`.
/// `status`、`end_reason` 与 `error` 描述终态分类；`timing` 包含持久化在
/// `ApiStats` 中的有序物理时间点。
///
/// # Returns
/// 返回值
/// Returns the required terminal completion timestamp after every cross-field invariant
/// succeeds.
/// 所有跨字段不变量通过后返回必需的终态完成时间。
fn validate_api_terminal_facts(
    status: ApiCallStatus,
    end_reason: Option<ApiEndReason>,
    error: Option<&RedactedApiError>,
    continuation_eligibility: ContinuationEligibility,
    timing: &ApiTimings,
) -> Result<i64, DomainError> {
    let reason = end_reason.ok_or(DomainError::InvalidApiTerminalFacts {
        field: "end_reason_missing",
    })?;
    match (status, reason, error) {
        (ApiCallStatus::Completed, ApiEndReason::CompletedSuccessfully, None)
        | (ApiCallStatus::Interrupted, ApiEndReason::UserInterrupted, None)
        | (ApiCallStatus::Interrupted, ApiEndReason::ProcessInterrupted, None)
        | (ApiCallStatus::Cancelled, ApiEndReason::Cancelled, None) => {}
        (ApiCallStatus::Failed, ApiEndReason::ProviderError, Some(error))
            if matches!(
                error.category,
                ApiErrorCategory::Provider | ApiErrorCategory::Auth | ApiErrorCategory::Protocol
            ) => {}
        (ApiCallStatus::Failed, ApiEndReason::NetworkError, Some(error))
            if matches!(
                error.category,
                ApiErrorCategory::Network | ApiErrorCategory::Timeout
            ) => {}
        (ApiCallStatus::Failed, ApiEndReason::BalanceInsufficient, Some(error))
            if error.category == ApiErrorCategory::Balance => {}
        (ApiCallStatus::Failed, ApiEndReason::RetryExhausted, Some(_)) => {}
        (ApiCallStatus::Failed, ApiEndReason::Unknown, Some(error))
            if error.category == ApiErrorCategory::Unknown => {}
        (ApiCallStatus::Prepared | ApiCallStatus::Dispatched | ApiCallStatus::InProgress, _, _) => {
            return Err(DomainError::InvalidApiTerminalFacts {
                field: "status_not_terminal",
            });
        }
        _ => {
            return Err(DomainError::InvalidApiTerminalFacts {
                field: "status_reason_error",
            });
        }
    }
    let required_eligibility = match reason {
        ApiEndReason::UserInterrupted
        | ApiEndReason::ProcessInterrupted
        | ApiEndReason::NetworkError
        | ApiEndReason::BalanceInsufficient
        | ApiEndReason::RetryExhausted => Some(ContinuationEligibility::Allowed),
        ApiEndReason::CompletedSuccessfully | ApiEndReason::Cancelled | ApiEndReason::Unknown => {
            Some(ContinuationEligibility::Forbidden)
        }
        ApiEndReason::ProviderError => None,
    };
    if required_eligibility.is_some_and(|required| required != continuation_eligibility) {
        return Err(DomainError::InvalidApiTerminalFacts {
            field: "continuation_eligibility",
        });
    }
    if let Some(error) = error
        && error.message.trim().is_empty()
    {
        return Err(DomainError::InvalidApiTerminalFacts {
            field: "error_message",
        });
    }

    let completed_at_ms = timing
        .completed_at_ms
        .ok_or(DomainError::InvalidApiTerminalFacts {
            field: "timing.completed_at_ms",
        })?;
    let ordered = [
        ("timing.prepared_at_ms", timing.prepared_at_ms),
        ("timing.dispatched_at_ms", timing.dispatched_at_ms),
        ("timing.first_event_at_ms", timing.first_event_at_ms),
        ("timing.first_token_at_ms", timing.first_token_at_ms),
        ("timing.completed_at_ms", Some(completed_at_ms)),
    ];
    let mut previous = None;
    for (field, value) in ordered {
        let Some(value) = value else {
            continue;
        };
        if let Some(previous) = previous
            && value < previous
        {
            return Err(DomainError::InvalidApiTerminalFacts { field });
        }
        previous = Some(value);
    }
    Ok(completed_at_ms)
}

/// Compares pending request facts while ignoring their audit-only set time.
/// 比较待生效请求事实，同时忽略仅用于审计的设置时间。
fn pending_request_effectively_equal(
    left: &PendingRequestConfig,
    right: &PendingRequestConfig,
) -> bool {
    left.provider == right.provider
        && left.connection_id == right.connection_id
        && left.model == right.model
        && left.thinking_level == right.thinking_level
        && left.temperature == right.temperature
}

/// Resolves the one persisted output required by every terminal tool state.
/// 解析每个工具终态都必须持久化的唯一输出。
fn canonical_terminal_tool_output(
    tool_call_id: ToolCallId,
    status: ToolCallStatus,
    host_output: Option<ToolPayload>,
) -> Result<ToolPayload, DomainError> {
    match status {
        ToolCallStatus::Completed => host_output.ok_or(DomainError::MissingTerminalToolOutput {
            tool_call_id: tool_call_id.value(),
            status: tool_status_name(status),
        }),
        ToolCallStatus::Failed => Ok(ToolPayload::terminal_failure(ToolTerminalErrorKind::Failed)),
        ToolCallStatus::Cancelled => Ok(ToolPayload::terminal_failure(
            ToolTerminalErrorKind::Cancelled,
        )),
        ToolCallStatus::Stopped => Ok(ToolPayload::terminal_failure(
            ToolTerminalErrorKind::Stopped,
        )),
        ToolCallStatus::Interrupted => Ok(ToolPayload::terminal_failure(
            ToolTerminalErrorKind::Interrupted,
        )),
        ToolCallStatus::Prepared | ToolCallStatus::Running | ToolCallStatus::WaitingUser => {
            Err(DomainError::ToolStatusMismatch {
                tool_call_id: tool_call_id.value(),
                expected: "terminal",
                actual: tool_status_name(status),
            })
        }
    }
}

/// Builds the authoritative receipt from one validated durable terminal tool event.
/// 从一条已校验的持久化工具终态事件构建权威 receipt。
fn tool_completion_receipt(
    tool_seq: Seq,
    tool: &ToolCallEvent,
) -> Result<ToolCompletionReceipt, DomainError> {
    let finished_at_ms = tool
        .finished_at_ms
        .ok_or(DomainError::ImmutableFieldMutation {
            entity: "tool_call",
            field: "finished_at_ms",
        })?;
    Ok(ToolCompletionReceipt {
        api_call_id: tool.api_call_id,
        tool_call_id: tool.tool_call_id,
        tool_seq,
        status: tool.status,
        finished_at_ms,
    })
}

/// Validates the persisted timestamps implied by one tool lifecycle status.
/// 校验一个工具生命周期状态所蕴含的持久化时间戳形态。
fn validate_tool_lifecycle_shape(tool: &ToolCallEvent) -> Result<(), DomainError> {
    validate_tool_completion_changeset(tool.changeset.as_ref())?;
    let valid = match tool.status {
        ToolCallStatus::Prepared => tool.started_at_ms.is_none() && tool.finished_at_ms.is_none(),
        ToolCallStatus::Running | ToolCallStatus::WaitingUser => {
            tool.started_at_ms.is_some() && tool.finished_at_ms.is_none()
        }
        ToolCallStatus::Completed
        | ToolCallStatus::Failed
        | ToolCallStatus::Cancelled
        | ToolCallStatus::Stopped => tool.started_at_ms.is_some() && tool.finished_at_ms.is_some(),
        // A durable Prepared tool may be interrupted before its body starts, so only the terminal
        // timestamp is mandatory for this explicit shutdown transition.
        // 已可靠 Prepared 的工具可能在主体启动前被中断，因此该显式关闭转换只强制终态时间戳。
        ToolCallStatus::Interrupted => tool.finished_at_ms.is_some(),
    };
    if !valid {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "tool_call",
            field: "lifecycle_timestamps",
        });
    }
    if matches!(
        tool.status,
        ToolCallStatus::Completed
            | ToolCallStatus::Failed
            | ToolCallStatus::Cancelled
            | ToolCallStatus::Stopped
            | ToolCallStatus::Interrupted
    ) && tool.output.is_none()
    {
        return Err(DomainError::MissingTerminalToolOutput {
            tool_call_id: tool.tool_call_id.value(),
            status: tool_status_name(tool.status),
        });
    }
    Ok(())
}

/// Validates one optional persistent change-set through the shared canonical contract.
/// 通过共享规范契约校验一份可选持久化 change-set。
fn validate_tool_completion_changeset(
    changeset: Option<&PersistentChangeSet>,
) -> Result<(), DomainError> {
    let Some(changeset) = changeset else {
        return Ok(());
    };
    validate_persistent_change_set(changeset).map_err(|error| {
        DomainError::InvalidPersistentChangeSet {
            location: error.location,
            detail: error.detail,
        }
    })
}

/// Maps one physical API failure reason to its permanent display notice.
/// 把一次物理 API 失败原因映射为永久显示通知。
fn api_failure_notice(
    reason: ApiEndReason,
) -> Option<(DisplayNoticeKind, NoticeSeverity, &'static str)> {
    match reason {
        ApiEndReason::ProviderError => Some((
            DisplayNoticeKind::ProviderError,
            NoticeSeverity::Error,
            "provider_error",
        )),
        ApiEndReason::NetworkError => Some((
            DisplayNoticeKind::NetworkDisconnected,
            NoticeSeverity::Warning,
            "network_error",
        )),
        ApiEndReason::BalanceInsufficient => Some((
            DisplayNoticeKind::BalanceInsufficient,
            NoticeSeverity::Warning,
            "balance_insufficient",
        )),
        ApiEndReason::RetryExhausted => Some((
            DisplayNoticeKind::RetryExhausted,
            NoticeSeverity::Error,
            "retry_exhausted",
        )),
        ApiEndReason::CompletedSuccessfully
        | ApiEndReason::UserInterrupted
        | ApiEndReason::ProcessInterrupted
        | ApiEndReason::Cancelled
        | ApiEndReason::Unknown => None,
    }
}

/// Returns zero for an empty budget scope and `None` for unknown usage.
/// 对空预算作用域返回零，对未知用量返回 `None`。
fn budget_tokens_or_zero(bucket: &StatBucket) -> Option<u64> {
    if bucket.is_empty() {
        Some(0)
    } else {
        goal_budget_tokens(bucket)
    }
}

/// Requires one active event to belong to the expected physical API.
/// 要求一条活动事件属于预期物理 API。
fn ensure_api_event_owner(
    seq: Seq,
    expected: ApiCallId,
    actual: ApiCallId,
) -> Result<(), DomainError> {
    if expected == actual {
        return Ok(());
    }
    Err(DomainError::CrossApiGroupUpdate {
        event_seq: seq.value(),
        api_call_id: actual.value(),
    })
}

/// Returns the API id of one event belonging to a physical API group.
/// 返回属于某个物理 API 组的事件 API id。
fn group_event_api_id(event: &ConversationEvent) -> Option<u64> {
    match &event.event {
        EventBody::ApiStarted(e) => Some(e.api_call_id.value()),
        EventBody::AssistantReasoning(e) => Some(e.api_call_id.value()),
        EventBody::AssistantText(e) => Some(e.api_call_id.value()),
        EventBody::ToolCall(e) => Some(e.api_call_id.value()),
        EventBody::ApiStats(e) => Some(e.api_call_id.value()),
        _ => None,
    }
}

/// Returns the ordering rank of one event body inside its API group.
/// 返回一个事件体在其 API 组内的顺序等级。
fn group_rank(event: &EventBody) -> usize {
    match event {
        EventBody::ApiStarted(_) => STARTED_RANK,
        EventBody::AssistantReasoning(_) => REASONING_RANK,
        EventBody::AssistantText(_) => TEXT_RANK,
        EventBody::ToolCall(_) => TOOL_RANK,
        EventBody::ApiStats(_) => STATS_RANK,
        _ => usize::MAX,
    }
}

/// Ordering rank of the immutable API-start boundary.
/// 不可变 API 起始边界的顺序等级。
const STARTED_RANK: usize = 0;
/// Ordering rank of the reasoning stage.
/// reasoning 阶段的顺序等级。
const REASONING_RANK: usize = 1;
/// Ordering rank of the assistant text stage.
/// assistant 正文阶段的顺序等级。
const TEXT_RANK: usize = 2;
/// Ordering rank of the tool-call stage.
/// 工具调用阶段的顺序等级。
const TOOL_RANK: usize = 3;
/// Ordering rank of the terminal api-stats stage.
/// 终态 api-stats 阶段的顺序等级。
const STATS_RANK: usize = 4;

/// Returns a clone of the provider source carried by one event, if any.
/// 返回一条事件携带的 provider_source 克隆(如有)。
fn provider_source_of(event: &ConversationEvent) -> Option<ProviderSource> {
    match &event.event {
        EventBody::AssistantReasoning(e) => e.provider_source.clone(),
        EventBody::AssistantText(e) => e.provider_source.clone(),
        EventBody::ToolCall(e) => e.provider_source.clone(),
        _ => None,
    }
}

#[cfg(test)]
mod document_tests;
