//! Unique in-memory V1 session owners and serialized whole-document flushes.
//! V1 会话唯一内存所有者与串行化整文档 flush。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard};
use tokio::time::{Instant, timeout_at};
use uuid::Uuid;

#[cfg(test)]
use vulcan_service_api::ConversationEventPatchDurabilityV1;
use vulcan_service_api::{ConversationEventLiveDeltaV1, ServiceEvent, ServiceEventEmitter};
use vulcan_session_core::v1::config::QueuedInputIdentity;
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::{
    ApiTransportObservedReceipt, ClientInputSubmission, CompactionCommitInput,
    ContinuationDispatchResolution, ContinuationRequest, ContinueTurnReceipt, DraftMutationReceipt,
    EditGenerationInput, EnqueueInputReceipt, FlushCandidate, ForkGenerationInput,
    GenerationBuildOutput, IdempotentMutation, MemoryRevisionReceipt, PrepareAndStartApiInput,
    PreparedApiStart, PreparedToolBatchReceipt, QueuedInputDraftReceipt, QueuedInputOrderReceipt,
    QueuedInputPendingTurnReceipt, QueuedInputRemovalReceipt, SessionDocument, SubagentReturnInput,
    SubagentReturnReceipt, ToolCompletion, ToolCompletionReceipt, ToolPreparation,
    TurnCompletionTransition,
};
use vulcan_session_core::v1::errors::DomainError;
use vulcan_session_core::v1::event::{
    ApiPurposeKind, ApiRequestSnapshot, AssistantSemanticStatus, EventBody,
    ProviderProjectionProof, ProviderSourceInput, ToolPayload,
};
use vulcan_session_core::v1::ids::{
    ApiCallId, ClientActionId, CompactionId, ResourceId, Seq, ToolCallId, TurnId,
};
use vulcan_session_core::v1::projection::{
    ProjectedItem, SourcedProjectedItem, derive_active_system_state,
    project_provider_items_with_sources,
};
use vulcan_session_core::v1::service_dto::ConversationEventWindowBatch;
use vulcan_session_core::v1::{
    ProviderRequestBudgetEvidence, ProviderUsageBudgetAnchor, latest_provider_usage_budget_anchor,
};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    ProviderApiTraceStore, SessionV1FlushResult, SessionV1StorageLocator, SessionV1StoreError,
    flush_session_v1_candidate, load_session_v1_full, publish_edit_session_v1,
};

mod request_preparation;
pub(crate) use request_preparation::SessionPhysicalPreparation;

#[cfg(test)]
use vulcan_session_core::v1::document::RootInputReplacement;
#[cfg(test)]
use vulcan_workspace_store::workspace_store::create_session_v1_storage_fixture;

/// One semantic reason that requires a durable V1 boundary.
/// 要求形成可靠 V1 边界的一种语义原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionFlushReason {
    /// A root or scheduled input became part of the stream.
    /// 根输入或已调度输入进入主事件流。
    InputAccepted,
    /// One completed prompt resource result is durable before the first physical request.
    /// 一项已完成提示词资源结果在首次物理请求之前可靠落盘。
    PromptResourcePrepared,
    /// A physical Provider API start fact must precede network dispatch.
    /// 物理 Provider API 开始事实必须先于网络派发落盘。
    ApiStarted,
    /// A physical Provider request reached the transport after its durable start.
    /// 物理 Provider 请求在可靠起始之后到达了传输层。
    ApiTransportObserved,
    /// A tool entered, checkpointed, or left an externally meaningful state.
    /// 工具进入、检查点更新或离开外部可见状态。
    ToolBoundary,
    /// A physical Provider API reached one terminal result.
    /// 物理 Provider API 到达一个终态结果。
    ApiCompleted,
    /// A durable display-only diagnostic or transition marker was appended.
    /// 追加了需要持久化的仅显示诊断或转换标记。
    DisplayBoundary,
    /// One logical Turn reached its unique terminal statistics.
    /// 一个逻辑回合到达唯一终态统计。
    TurnCompleted,
    /// One durable child return and its complete inclusive statistics entered the parent Turn.
    /// 一条可靠子代理回流及其完整包容性统计进入父回合。
    SubagentReturned,
    /// Runtime and compaction pointers changed atomically.
    /// runtime 与压缩指针发生原子变更。
    ContextChanged,
    /// Session metadata changed outside the event stream.
    /// 会话 metadata 在事件流之外发生变更。
    MetadataChanged,
}

/// Admission class of one in-memory session mutation.
/// 一次内存会话修改的准入类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionMutationAdmission {
    /// Starts a new root Turn and is rejected after shutdown begins.
    /// 启动新根回合，并在关闭开始后被拒绝。
    RootTurn,
    /// Advances or settles work already admitted into an existing Turn.
    /// 推进或结算已经进入现有回合的工作。
    ExistingTurn,
    /// Changes metadata without starting new inference work.
    /// 修改 metadata，但不启动新的推理工作。
    MetadataOnly,
}

/// Receipt returned by every accepted V1 mutation.
/// 每次已接纳 V1 修改返回的回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionMutationReceipt<T> {
    /// Value produced by the domain mutation.
    /// 领域修改产生的值。
    pub(crate) value: T,
    /// Exact memory revision containing the mutation.
    /// 包含该修改的精确内存修订号。
    pub(crate) revision: u64,
    /// Exact event seq values changed by this revision.
    /// 该修订真实改变的事件 seq。
    pub(crate) changed_event_seqs: Vec<Seq>,
    /// Semantic reason associated with the required flush.
    /// 与所需 flush 关联的语义原因。
    pub(crate) flush_reason: SessionFlushReason,
}

/// Strongly typed change decision produced while the document lock is held.
/// 在持有文档锁期间产生的强类型变化判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionMutationChange<T> {
    /// The mutation changed the document and therefore must advance revision.
    /// 修改已改变文档，因此必须推进修订号。
    Changed(T),
    /// The requested state already matched and no revision may be fabricated.
    /// 请求状态已经一致，不得伪造修订号。
    Unchanged(T),
}

/// Maps one Core idempotent result into the registry's revision decision.
/// 把一条 Core 幂等结果映射为注册表的修订判定。
fn session_change_from_idempotent<T>(mutation: IdempotentMutation<T>) -> SessionMutationChange<T> {
    match mutation {
        IdempotentMutation::Committed(value) => SessionMutationChange::Changed(value),
        IdempotentMutation::Idempotent(value) => SessionMutationChange::Unchanged(value),
    }
}

/// Durable receipt for a mutation that may legitimately be idempotent.
/// 可合法幂等的修改所返回的可靠回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionConditionalMutationReceipt<T> {
    /// Value produced by the domain mutation.
    /// 领域修改产生的值。
    pub(crate) value: T,
    /// Whether this invocation changed the authoritative document.
    /// 本次调用是否改变了权威文档。
    pub(crate) changed: bool,
    /// Exact revision observed after the atomic mutation decision.
    /// 原子变化判定完成后观察到的精确修订号。
    pub(crate) revision: u64,
    /// Exact event seq values changed by this invocation.
    /// 本次调用真实改变的事件 seq。
    pub(crate) changed_event_seqs: Vec<Seq>,
    /// Semantic reason associated with any required flush.
    /// 与可能需要的 flush 关联的语义原因。
    pub(crate) flush_reason: SessionFlushReason,
}

/// Atomic choice between closing a successful turn and consuming a newly arrived child result.
/// 关闭成功回合与消费新到达子任务结果之间的原子选择。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SuccessfulTurnSettlement {
    /// The logical turn closed and selected its normal successor.
    /// 逻辑回合已关闭并选择正常后继。
    Completed(TurnCompletionTransition),
    /// A durable return keeps the logical turn open for one more physical request.
    /// 可靠回流保持逻辑回合开放，以便继续一次物理请求。
    SubagentReturn,
}

/// Immutable V1 ledger projection captured by the unique session owner before API start.
/// 在 API 启动前由唯一会话 owner 捕获的不可变 V1 账本投影。
#[derive(Debug, Clone)]
pub(crate) struct SessionProviderProjectionSnapshot {
    /// Exact pending prompt sequences admitted by Core before their first physical API.
    /// 在首次物理API前由Core准入的精确待准备提示词序号。
    pub(crate) preparing_user_prompt_media_seqs: Vec<Seq>,
    /// Persisted segment generation that owns the exact event range.
    /// 拥有精确事件区间的持久化分片代次。
    pub(crate) segment_generation: u64,
    /// Durable document revision from which the request is reconstructed.
    /// 请求重建所基于的可靠文档修订号。
    pub(crate) document_revision: u64,
    /// Inclusive retained context boundary selected by the current compaction pointer.
    /// 当前压缩指针选定的包含式上下文保留边界。
    pub(crate) retained_from_seq: Seq,
    /// Inclusive final event seq covered before `api_started` is appended.
    /// 追加 `api_started` 前覆盖的包含式末尾事件 seq。
    pub(crate) through_seq: Seq,
    /// Provider-visible items projected exactly once from the full durable prefix.
    /// 从完整可靠前缀精确投影一次得到的 Provider 可见条目。
    pub(crate) items: Vec<ProjectedItem>,
    /// The same projection with exact durable source ranges for compaction planning.
    /// 供压缩规划使用、带精确可靠来源范围的同一投影。
    pub(crate) sourced_items: Vec<SourcedProjectedItem>,
    /// Exact project directory resolved by the validated SQLite locator.
    /// 由已校验 SQLite 定位器解析出的精确项目目录。
    pub(crate) project_directory: PathBuf,
    /// Latest completed V1 API pair eligible to calibrate the next exact request.
    /// 可用于校准下一份精确请求的最近一组已完成 V1 API 事实。
    pub(crate) provider_usage_budget_anchor: Option<ProviderUsageBudgetAnchor>,
}

/// Transport payload and projection proof built from one immutable V1 snapshot.
/// 从一份不可变 V1 快照构建的传输载荷与投影证明。
#[derive(Debug)]
pub(crate) struct SessionPreparedPhysicalRequest<T> {
    /// Newly prepared origins to freeze on their exact user prompts with the durable API start.
    /// 随可靠API起始冻结到各自精确用户提示词的新准备原身份。
    pub(crate) user_prompt_media: Vec<vulcan_session_core::v1::UserPromptMediaBinding>,
    /// Exact transport value that may be released only after durable API start.
    /// 仅在 API 起始可靠落盘后才能放行的精确传输值。
    pub(crate) transport: T,
    /// SHA-bound proof of the exact request reconstruction.
    /// 绑定精确请求重建结果的 SHA 证明。
    pub(crate) projection: ProviderProjectionProof,
    /// Non-sensitive evidence derived from the same exact transport request.
    /// 从同一份精确传输请求派生的非敏感证据。
    pub(crate) request_budget_evidence: ProviderRequestBudgetEvidence,
}

/// Durable API-start receipt paired with the exact transport request it authorizes.
/// 可靠 API 起始回执及其授权的精确传输请求。
#[derive(Debug)]
pub(crate) struct SessionPreparedPhysicalApi<T> {
    /// Core receipt proving the physical API start is durable.
    /// 证明物理 API 起始已可靠落盘的 Core 回执。
    pub(crate) core_start: PreparedApiStart,
    /// Exact transport value reconstructed before the Core start transaction.
    /// 在 Core 起始事务前重建的精确传输值。
    pub(crate) transport: T,
}

/// Durable receipt for one in-place history-generation replacement.
/// 一次原位历史 generation 替换的可靠回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionEditGenerationReceipt {
    /// Rewritten root input seq that remains the open Turn boundary.
    /// 保持为开放回合边界的已重写根输入 seq。
    pub(crate) target_seq: Seq,
    /// Newly published event-window generation.
    /// 新发布的事件窗口 generation。
    pub(crate) segment_generation: u64,
    /// Newly published complete document revision.
    /// 新发布的完整文档修订号。
    pub(crate) document_revision: u64,
}

/// Builds deterministic valid Provider budget evidence for cross-module test fixtures.
/// 为跨模块测试夹具构建确定且有效的 Provider 预算证据。
#[cfg(test)]
pub(crate) fn test_provider_request_budget_evidence() -> ProviderRequestBudgetEvidence {
    ProviderRequestBudgetEvidence {
        request_assembly_fingerprint: format!("sha256:{}", "0".repeat(64)),
        model_route_fingerprint: format!("sha256:{}", "1".repeat(64)),
        toolset_fingerprint: format!("sha256:{}", "2".repeat(64)),
        system_prompt_fingerprint: format!("sha256:{}", "3".repeat(64)),
        request_text_measurement: vulcan_session_core::v1::ProviderRequestTextMeasurement {
            character_count: 0,
            utf8_byte_count: 0,
            structural_units: 0,
            history_message_count: 0,
            prompt_in_history: false,
            history_fingerprint: format!("sha256:{}", "4".repeat(64)),
            prefix_invariant_fingerprint: format!("sha256:{}", "5".repeat(64)),
            context_breakdown: vulcan_session_core::v1::ProviderRequestContextBreakdown::default(),
        },
    }
}

/// Process-local ownership token for one session's physical Continue dispatch.
/// 单个会话物理 Continue 分派的进程内所有权令牌。
///
/// The token deliberately contains no durable facts. Dropping it releases only concurrent
/// process admission; Core events remain the sole authority across restart.
/// 该令牌有意不包含任何持久化事实。释放它只解除进程内并发准入；Core 事件在重启前后
/// 始终是唯一权威。
pub(crate) struct ContinuationDispatchGuard {
    /// Owned async mutex guard retained across the detached preparation task.
    /// 跨分离准备任务保留的异步互斥守卫。
    _guard: OwnedMutexGuard<()>,
}

/// Process-local durable frontier and explicit persistence failures for Continue dispatch.
/// Continue 分派的进程内可靠前沿与显式持久化失败状态。
struct ContinuationDispatchDurabilityState {
    /// Last event seq covered by a successfully published complete candidate.
    /// 最近一份成功发布的完整候选所覆盖的最后事件 seq。
    durable_through_seq: Option<Seq>,
    /// Failed candidate affecting each exact Continue action until a covering retry succeeds.
    /// 影响每个精确 Continue 动作的失败候选；覆盖该候选的重试成功前持续保留。
    persistence_failures: BTreeMap<ClientActionId, ContinuationDispatchPersistenceFailure>,
}

/// One non-durable flush failure attached to an exact Continue action.
/// 绑定到一个精确 Continue 动作的一次未可靠落盘失败。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ContinuationDispatchPersistenceFailure {
    /// Complete candidate revision that failed physical publication.
    /// 物理发布失败的完整候选修订号。
    candidate_revision: u64,
    /// Stable storage failure detail retained for the waiting owner.
    /// 为等待 owner 保留的稳定存储失败详情。
    detail: String,
}

/// Process-local ownership token for one session lifecycle projection transaction.
/// 单个会话生命周期投影事务的进程内所有权令牌。
///
/// The token serializes authoritative metadata flushes with their SQLite list projection without
/// imposing one global lifecycle lock on unrelated sessions.
/// 该令牌把权威 metadata flush 与对应 SQLite 列表投影串行化，同时不对无关会话施加全局
/// 生命周期锁。
pub(crate) struct SessionLifecycleTransitionGuard {
    /// Owned async mutex guard retained until metadata and SQLite agree or a retryable error returns.
    /// 保留至 metadata 与 SQLite 一致或返回可重试错误的异步互斥守卫。
    _guard: OwnedMutexGuard<()>,
}

/// Reason one process-local session handle was quarantined from further writes.
/// 一个进程内会话句柄被隔离并禁止继续写入的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionQuarantineReason {
    /// A successful non-conditional mutation did not advance exactly one revision.
    /// 成功的非条件修改没有精确推进一个修订号。
    MutationRevisionMismatch {
        /// Revision observed before the mutation.
        /// 修改前观察到的修订号。
        before_revision: u64,
        /// Revision observed after the mutation.
        /// 修改后观察到的修订号。
        after_revision: u64,
    },
    /// A conditional change decision disagreed with the exact revision effect.
    /// 条件变化判定与精确修订效果不一致。
    ConditionalMutationRevisionMismatch {
        /// Whether the mutation reported a document change.
        /// 修改是否报告文档已变化。
        reported_changed: bool,
        /// Revision observed before the mutation.
        /// 修改前观察到的修订号。
        before_revision: u64,
        /// Revision observed after the mutation.
        /// 修改后观察到的修订号。
        after_revision: u64,
    },
    /// A failed domain mutation changed the authoritative in-memory document.
    /// 失败的领域修改改变了权威内存文档。
    FailedMutationProducedRevision {
        /// Revision observed before the failed mutation.
        /// 失败修改前观察到的修订号。
        before_revision: u64,
        /// Revision observed after the failed mutation.
        /// 失败修改后观察到的修订号。
        after_revision: u64,
        /// Failure returned by the non-atomic domain operation.
        /// 非原子领域操作返回的失败。
        error: DomainError,
    },
    /// The document could not produce the exact receipt for an accepted revision.
    /// 文档无法为已接纳修订生成精确回执。
    RevisionReceiptRejected {
        /// Revision observed before the accepted mutation.
        /// 已接纳修改前观察到的修订号。
        before_revision: u64,
        /// Revision observed after the accepted mutation.
        /// 已接纳修改后观察到的修订号。
        after_revision: u64,
        /// Domain failure raised while constructing the receipt.
        /// 构建回执时产生的领域失败。
        error: DomainError,
    },
    /// Storage accepted one complete candidate, but the resident owner rejected its commit receipt.
    /// 存储已接纳一份完整候选，但常驻 owner 拒绝了其提交回执。
    DurableCommitApplicationRejected {
        /// Candidate revision already accepted by physical storage.
        /// 物理存储已经接纳的候选修订号。
        candidate_revision: u64,
        /// Exact post-commit failure that prevents safe continued ownership.
        /// 阻止 owner 安全继续运行的精确提交后失败。
        detail: String,
    },
    /// A published replacement generation could not be loaded back as the new resident owner.
    /// 已发布的替换 generation 无法重新加载为新的常驻 owner。
    GenerationReloadRejected {
        /// Exact post-commit failure that prevents safe continuation on the old resident generation.
        /// 阻止旧常驻 generation 安全继续运行的精确提交后失败。
        detail: String,
    },
}

impl fmt::Display for SessionQuarantineReason {
    /// Formats one stable quarantine reason for diagnostics and admission errors.
    /// 为诊断与准入错误格式化一条稳定的隔离原因。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MutationRevisionMismatch {
                before_revision,
                after_revision,
            } => write!(
                formatter,
                "successful mutation moved revision from {before_revision} to {after_revision} instead of exactly one step"
            ),
            Self::ConditionalMutationRevisionMismatch {
                reported_changed,
                before_revision,
                after_revision,
            } => write!(
                formatter,
                "conditional mutation reported changed={reported_changed} while revision moved from {before_revision} to {after_revision}"
            ),
            Self::FailedMutationProducedRevision {
                before_revision,
                after_revision,
                error,
            } => write!(
                formatter,
                "failed mutation moved revision from {before_revision} to {after_revision}: {error}"
            ),
            Self::RevisionReceiptRejected {
                before_revision,
                after_revision,
                error,
            } => write!(
                formatter,
                "revision receipt for mutation {before_revision}->{after_revision} was rejected: {error}"
            ),
            Self::DurableCommitApplicationRejected {
                candidate_revision,
                detail,
            } => write!(
                formatter,
                "physical storage accepted candidate revision {candidate_revision}, but the resident owner rejected its commit receipt: {detail}"
            ),
            Self::GenerationReloadRejected { detail } => write!(
                formatter,
                "published replacement generation could not become the resident owner: {detail}"
            ),
        }
    }
}

/// Structured failures produced by the V1 runtime owner.
/// V1 运行时所有者产生的结构化失败。
#[derive(Debug)]
pub(crate) enum SessionRuntimeError {
    /// The registry mutex is poisoned.
    /// 注册表互斥锁已中毒。
    RegistryLockPoisoned,
    /// The session document mutex is poisoned.
    /// 会话文档互斥锁已中毒。
    DocumentLockPoisoned,
    /// The physical writer mutex is poisoned.
    /// 物理写入器互斥锁已中毒。
    WriterLockPoisoned,
    /// The durable live-publisher mutex is poisoned.
    /// 可靠实时发布器互斥锁已中毒。
    LivePublisherLockPoisoned,
    /// The mutation/live ordering mutex is poisoned.
    /// 修改与实时发布顺序互斥锁已中毒。
    MutationPublisherLockPoisoned,
    /// The session quarantine-state mutex is poisoned.
    /// 会话隔离状态互斥锁已中毒。
    QuarantineLockPoisoned,
    /// The Continue durable-frontier state mutex is poisoned.
    /// Continue 可靠前沿状态互斥锁已中毒。
    ContinuationDispatchStateLockPoisoned,
    /// The session handle is quarantined after a domain contract violation.
    /// 会话句柄因领域契约违反而处于隔离状态。
    SessionQuarantined {
        /// Quarantined session identity.
        /// 被隔离的会话身份。
        session_id: SessionId,
        /// First contract violation that quarantined the handle.
        /// 导致句柄隔离的首个契约违反。
        reason: SessionQuarantineReason,
    },
    /// A new root Turn was attempted after shutdown admission closed.
    /// 关闭准入后仍尝试启动新的根回合。
    ShuttingDown,
    /// The registry already owns this exact session.
    /// 注册表已经拥有该精确会话。
    DuplicateSession(SessionId),
    /// Another creator or loader currently owns publication for this session.
    /// 另一个创建器或加载器当前拥有该会话的发布权。
    SessionRegistrationInProgress(SessionId),
    /// A concurrent exact-session load finished without publishing a handle.
    /// 并发精确会话加载结束但未发布句柄。
    ConcurrentSessionLoadFailed(SessionId),
    /// A loader returned a handle for a different session identity.
    /// 加载器返回了另一会话身份的句柄。
    LoadedSessionIdentityMismatch {
        /// Identity requested from the registry.
        /// 注册表请求的身份。
        expected: SessionId,
        /// Identity carried by the loaded handle.
        /// 已加载句柄携带的身份。
        actual: SessionId,
    },
    /// Exact storage returned metadata for a different or malformed session id.
    /// 精确存储返回了属于另一会话或格式错误的会话 ID metadata。
    LoadedSessionDocumentIdentityMismatch {
        /// Identity requested from exact storage.
        /// 向精确存储请求的身份。
        expected: SessionId,
        /// Raw identity carried by the loaded metadata.
        /// 加载 metadata 携带的原始身份。
        actual: String,
    },
    /// The registry does not own the requested session.
    /// 注册表不拥有请求的会话。
    SessionNotRegistered(SessionId),
    /// A Continue request did not reach one durable transport observation before its deadline.
    /// Continue 请求未在截止时间前到达持久化 transport 观察事实。
    ContinuationTransportObservationTimedOut {
        /// Browser idempotency identity of the Continue action.
        /// Continue 动作的浏览器幂等身份。
        client_action_id: ClientActionId,
    },
    /// A Continue API terminated before transport observation became durable.
    /// Continue API 在传输观察可靠落盘前已经终止。
    ContinuationTerminatedBeforeTransport {
        /// Browser idempotency identity of the Continue action.
        /// Continue 动作的浏览器幂等身份。
        client_action_id: ClientActionId,
        /// Physical API identity that terminated before dispatch observation.
        /// 在分派观察前终止的物理 API 身份。
        api_call_id: ApiCallId,
    },
    /// A candidate carrying this Continue action failed physical persistence.
    /// 携带该 Continue 动作的候选发生物理持久化失败。
    ContinuationDispatchPersistenceFailed {
        /// Browser idempotency identity of the affected Continue action.
        /// 受影响 Continue 动作的浏览器幂等身份。
        client_action_id: ClientActionId,
        /// Complete candidate revision that failed publication.
        /// 发布失败的完整候选修订号。
        candidate_revision: u64,
        /// Stable storage failure detail.
        /// 稳定的存储失败详情。
        detail: String,
    },
    /// A mutation returned without issuing a new document revision.
    /// 修改返回时没有产生新的文档修订号。
    MutationProducedNoRevision,
    /// A successful mutation did not advance exactly one revision.
    /// 成功修改没有精确推进一个修订号。
    MutationRevisionMismatch {
        /// Revision before invoking the mutation.
        /// 调用修改前的修订号。
        before_revision: u64,
        /// Revision after invoking the mutation.
        /// 调用修改后的修订号。
        after_revision: u64,
    },
    /// A changed revision could not produce its exact live receipt.
    /// 已变化修订无法生成精确实时回执。
    RevisionReceiptRejected {
        /// Revision before invoking the mutation.
        /// 调用修改前的修订号。
        before_revision: u64,
        /// Revision after invoking the mutation.
        /// 调用修改后的修订号。
        after_revision: u64,
        /// Domain failure raised while constructing the receipt.
        /// 构建回执时产生的领域失败。
        error: Box<DomainError>,
    },
    /// A conditional mutation's typed decision disagreed with its revision effect.
    /// 条件修改的强类型判定与其修订号效果不一致。
    ConditionalMutationRevisionMismatch {
        /// Whether the mutation reported a document change.
        /// 修改是否报告文档已变化。
        reported_changed: bool,
        /// Revision before invoking the mutation.
        /// 调用修改前的修订号。
        before_revision: u64,
        /// Revision after invoking the mutation.
        /// 调用修改后的修订号。
        after_revision: u64,
    },
    /// A domain method returned an error after advancing the document revision.
    /// 领域方法在推进文档修订号后返回了错误。
    FailedMutationProducedRevision {
        /// Revision before invoking the failed mutation.
        /// 调用失败修改前的修订号。
        before_revision: u64,
        /// Revision left behind by the failed mutation.
        /// 失败修改遗留的修订号。
        after_revision: u64,
        /// Domain error returned after the revision changed.
        /// 修订号变化后返回的领域错误。
        error: Box<DomainError>,
    },
    /// The caller targeted a stale physical history generation.
    /// 调用方指定了已经过期的物理历史 generation。
    GenerationSourceChanged {
        /// Generation observed by the caller before starting the action.
        /// 调用方启动动作前观察到的 generation。
        expected: u64,
        /// Generation currently published by the unique session owner.
        /// 唯一会话 owner 当前发布的 generation。
        actual: u64,
    },
    /// The resident owner and its exact durable source disagreed before generation publication.
    /// generation 发布前，常驻 owner 与其精确持久化来源不一致。
    GenerationResidentMismatch {
        /// Resident in-memory document revision.
        /// 常驻内存文档修订号。
        resident_revision: u64,
        /// Strictly loaded durable document revision.
        /// 严格加载的持久化文档修订号。
        durable_revision: u64,
        /// Resident in-memory segment generation.
        /// 常驻内存分片 generation。
        resident_generation: u64,
        /// Strictly loaded durable segment generation.
        /// 严格加载的持久化分片 generation。
        durable_generation: u64,
    },
    /// The domain rejected one semantic mutation.
    /// 领域层拒绝了一次语义修改。
    Domain(Box<DomainError>),
    /// An operation requiring exact physical ownership received a test-only locatorless handle.
    /// 需要精确物理所有权的操作收到仅测试使用且无定位器的句柄。
    StorageLocatorUnavailable,
    /// Provider projection requires the production exact storage locator.
    /// Provider 投影要求生产环境的精确存储定位器。
    ProviderProjectionRequiresStorageLocator,
    /// One exact Provider projection or request reconstruction invariant failed.
    /// 一条精确 Provider 投影或请求重建不变量失败。
    ProviderProjectionInvariant(String),
    /// The physical V1 store rejected or failed one complete candidate.
    /// 物理 V1 存储拒绝或写入失败了一份完整候选。
    Storage(Box<SessionV1StoreError>),
    /// The blocking durability worker could not be joined by the async runtime.
    /// 异步运行时无法汇合阻塞式可靠写入工作线程。
    BlockingTaskJoin(String),
}

impl fmt::Display for SessionRuntimeError {
    /// Formats one stable operator-facing failure description.
    /// 格式化一条稳定的面向操作者失败描述。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegistryLockPoisoned => formatter.write_str("V1 session registry lock poisoned"),
            Self::DocumentLockPoisoned => formatter.write_str("V1 session document lock poisoned"),
            Self::WriterLockPoisoned => formatter.write_str("V1 session writer lock poisoned"),
            Self::LivePublisherLockPoisoned => {
                formatter.write_str("V1 session live publisher lock poisoned")
            }
            Self::MutationPublisherLockPoisoned => {
                formatter.write_str("V1 session mutation publisher lock poisoned")
            }
            Self::QuarantineLockPoisoned => {
                formatter.write_str("V1 session quarantine lock poisoned")
            }
            Self::ContinuationDispatchStateLockPoisoned => {
                formatter.write_str("V1 Continue durable-frontier state lock poisoned")
            }
            Self::SessionQuarantined { session_id, reason } => write!(
                formatter,
                "V1 session {session_id} is quarantined and rejects further writes: {reason}"
            ),
            Self::ShuttingDown => formatter.write_str("V1 session is shutting down"),
            Self::DuplicateSession(session_id) => {
                write!(formatter, "V1 session {session_id} is already registered")
            }
            Self::SessionRegistrationInProgress(session_id) => write!(
                formatter,
                "V1 session {session_id} registration is already in progress"
            ),
            Self::ConcurrentSessionLoadFailed(session_id) => write!(
                formatter,
                "Concurrent V1 session {session_id} load did not publish a handle"
            ),
            Self::LoadedSessionIdentityMismatch { expected, actual } => write!(
                formatter,
                "V1 session loader returned {actual} while {expected} was requested"
            ),
            Self::LoadedSessionDocumentIdentityMismatch { expected, actual } => write!(
                formatter,
                "V1 session storage returned metadata {actual} while {expected} was requested"
            ),
            Self::SessionNotRegistered(session_id) => {
                write!(formatter, "V1 session {session_id} is not registered")
            }
            Self::ContinuationTransportObservationTimedOut { client_action_id } => write!(
                formatter,
                "V1 Continue action {} did not reach a durable transport observation before its deadline",
                client_action_id.as_str()
            ),
            Self::ContinuationTerminatedBeforeTransport {
                client_action_id,
                api_call_id,
            } => write!(
                formatter,
                "V1 Continue action {} terminated API {} before durable transport observation",
                client_action_id.as_str(),
                api_call_id.value()
            ),
            Self::ContinuationDispatchPersistenceFailed {
                client_action_id,
                candidate_revision,
                detail,
            } => write!(
                formatter,
                "V1 Continue action {} failed to persist candidate revision {}: {}",
                client_action_id.as_str(),
                candidate_revision,
                detail
            ),
            Self::MutationProducedNoRevision => {
                formatter.write_str("V1 session mutation produced no revision")
            }
            Self::MutationRevisionMismatch {
                before_revision,
                after_revision,
            } => write!(
                formatter,
                "V1 session mutation moved revision from {before_revision} to {after_revision} instead of exactly one step"
            ),
            Self::RevisionReceiptRejected {
                before_revision,
                after_revision,
                error,
            } => write!(
                formatter,
                "V1 session revision receipt for mutation {before_revision}->{after_revision} was rejected: {error}"
            ),
            Self::ConditionalMutationRevisionMismatch {
                reported_changed,
                before_revision,
                after_revision,
            } => write!(
                formatter,
                "V1 conditional session mutation reported changed={reported_changed} but revision moved from {before_revision} to {after_revision}"
            ),
            Self::FailedMutationProducedRevision {
                before_revision,
                after_revision,
                error,
            } => write!(
                formatter,
                "V1 session mutation failed after revision moved from {before_revision} to {after_revision}: {error}"
            ),
            Self::GenerationSourceChanged { expected, actual } => write!(
                formatter,
                "V1 history generation changed before publication: expected {expected}, actual {actual}"
            ),
            Self::GenerationResidentMismatch {
                resident_revision,
                durable_revision,
                resident_generation,
                durable_generation,
            } => write!(
                formatter,
                "V1 resident generation disagrees with exact durable storage before publication: resident revision/generation {resident_revision}/{resident_generation}, durable {durable_revision}/{durable_generation}"
            ),
            Self::Domain(error) => write!(formatter, "V1 session domain failure: {error}"),
            Self::StorageLocatorUnavailable => formatter
                .write_str("V1 session operation requires an exact production storage locator"),
            Self::ProviderProjectionRequiresStorageLocator => formatter
                .write_str("V1 Provider projection requires an exact production storage locator"),
            Self::ProviderProjectionInvariant(detail) => {
                write!(
                    formatter,
                    "V1 Provider projection invariant failed: {detail}"
                )
            }
            Self::Storage(error) => write!(formatter, "V1 session storage failure: {error}"),
            Self::BlockingTaskJoin(error) => {
                write!(
                    formatter,
                    "V1 session blocking durability task failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for SessionRuntimeError {}

impl From<DomainError> for SessionRuntimeError {
    /// Boxes one domain failure at the service ownership boundary.
    /// 在服务所有权边界装箱一个领域失败。
    fn from(error: DomainError) -> Self {
        Self::Domain(Box::new(error))
    }
}

impl From<SessionV1StoreError> for SessionRuntimeError {
    /// Boxes one storage failure at the service ownership boundary.
    /// 在服务所有权边界装箱一个存储失败。
    fn from(error: SessionV1StoreError) -> Self {
        Self::Storage(Box::new(error))
    }
}

/// Physical persistence seam used by the unique session writer.
/// 唯一会话写入器使用的物理持久化接口。
pub(crate) trait SessionPersistenceBackend: Send + Sync {
    /// Persists one immutable candidate and returns its exact published pointers.
    /// 持久化一份不可变候选并返回其精确发布指针。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError>;
}

/// Production backend that delegates one complete candidate to the V1 writer.
/// 把一份完整候选交给 V1 写入器的生产后端。
#[derive(Debug)]
pub(crate) struct FileSessionPersistenceBackend {
    /// Opaque indexed storage locator owned by this exact session handle.
    /// 当前精确会话句柄拥有的不透明索引存储定位器。
    locator: SessionV1StorageLocator,
}

impl SessionPersistenceBackend for FileSessionPersistenceBackend {
    /// Persists the candidate without accepting caller-assembled partial state.
    /// 持久化该候选，不接受调用方拼装的局部状态。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        flush_session_v1_candidate(&self.locator, candidate)
    }
}

/// Unique mutable owner and serialized physical writer for one V1 session.
/// 单个 V1 会话的唯一可变所有者与串行物理写入器。
#[derive(Clone)]
struct SessionLivePublisher {
    /// Host service-event multiplexer.
    /// 宿主服务事件复用器。
    emitter: ServiceEventEmitter,
    /// Stable identity of the owning service process.
    /// 所属服务进程的稳定身份。
    service_epoch: String,
}

/// Unique mutable owner and serialized physical writer for one V1 session.
/// 单个 V1 会话的唯一可变所有者与串行物理写入器。
pub(crate) struct SessionHandle {
    /// Stable application session identity.
    /// 稳定的应用会话身份。
    session_id: SessionId,
    /// Sole authoritative mutable session document.
    /// 唯一权威可变会话文档。
    document: Mutex<SessionDocument>,
    /// Same-session physical flush serializer.
    /// 同会话物理 flush 串行器。
    writer: Mutex<()>,
    /// Same-session memory-mutation and volatile-publish ordering serializer.
    /// 同会话内存修改与 volatile 发布顺序串行器。
    mutation_publisher: Mutex<()>,
    /// Single-flight request preparation without holding synchronous mutation or writer guards across awaits.
    /// 请求准备单飞门，不跨异步等待持有同步修改或写入器守卫。
    physical_request_preparation: Arc<AsyncMutex<()>>,
    /// Physical persistence implementation.
    /// 物理持久化实现。
    backend: Arc<dyn SessionPersistenceBackend>,
    /// Exact storage locator retained for full-range Provider projection.
    /// 为完整区间 Provider 投影保留的精确存储定位器。
    locator: Option<SessionV1StorageLocator>,
    /// Sole process-local Provider API trace owner for this session.
    /// 当前会话唯一的进程内 Provider API 追踪 owner。
    provider_api_trace_store: Option<ProviderApiTraceStore>,
    /// Optional process-local publisher installed by the production service host.
    /// 由生产服务宿主安装的可选进程内发布器。
    live_event_publisher: Mutex<Option<SessionLivePublisher>>,
    /// Wakes Continue dispatch waiters after any document revision becomes durable.
    /// 任一文档修订可靠落盘后唤醒 Continue 分派等待者。
    continuation_dispatch_notify: Notify,
    /// Process-local single-flight gate spanning Continue admission through durable transport observation.
    /// 覆盖 Continue 准入至可靠传输观察的进程内单飞门。
    continuation_dispatch_gate: Arc<AsyncMutex<()>>,
    /// Process-local durable Continue frontier and exact transient persistence failures.
    /// 进程内 Continue 可靠前沿与精确瞬态持久化失败。
    continuation_dispatch_durability: Mutex<ContinuationDispatchDurabilityState>,
    /// Whether root-Turn admission has closed for shutdown.
    /// 根回合准入是否因关闭流程而关闭。
    shutting_down: Mutex<bool>,
    /// First domain contract violation that permanently quarantines this owner.
    /// 永久隔离当前所有者的首个领域契约违反。
    quarantine: Mutex<Option<SessionQuarantineReason>>,
}

impl SessionHandle {
    /// Builds one production handle around an already-published V1 document.
    /// 围绕一份已经发布的 V1 文档构建生产句柄。
    pub(crate) fn new(
        session_id: SessionId,
        locator: SessionV1StorageLocator,
        document: SessionDocument,
    ) -> Arc<Self> {
        let durable_through_seq = (document.memory_revision() == document.durable_revision())
            .then(|| document.events().last().map(|event| event.seq))
            .flatten();
        let provider_api_trace_store =
            ProviderApiTraceStore::new(locator.provider_api_trace_directory(), true);
        Arc::new(Self {
            session_id,
            document: Mutex::new(document),
            writer: Mutex::new(()),
            mutation_publisher: Mutex::new(()),
            physical_request_preparation: Arc::new(AsyncMutex::new(())),
            backend: Arc::new(FileSessionPersistenceBackend {
                locator: locator.clone(),
            }),
            locator: Some(locator),
            provider_api_trace_store: Some(provider_api_trace_store),
            live_event_publisher: Mutex::new(None),
            continuation_dispatch_notify: Notify::new(),
            continuation_dispatch_gate: Arc::new(AsyncMutex::new(())),
            continuation_dispatch_durability: Mutex::new(ContinuationDispatchDurabilityState {
                durable_through_seq,
                persistence_failures: BTreeMap::new(),
            }),
            shutting_down: Mutex::new(false),
            quarantine: Mutex::new(None),
        })
    }

    /// Builds one handle with an explicit backend for deterministic tests.
    /// 使用显式后端构建句柄，以支持确定性测试。
    #[cfg(test)]
    pub(crate) fn with_backend(
        session_id: SessionId,
        document: SessionDocument,
        backend: Arc<dyn SessionPersistenceBackend>,
    ) -> Arc<Self> {
        let durable_through_seq = (document.memory_revision() == document.durable_revision())
            .then(|| document.events().last().map(|event| event.seq))
            .flatten();
        Arc::new(Self {
            session_id,
            document: Mutex::new(document),
            writer: Mutex::new(()),
            mutation_publisher: Mutex::new(()),
            physical_request_preparation: Arc::new(AsyncMutex::new(())),
            backend,
            locator: None,
            provider_api_trace_store: None,
            live_event_publisher: Mutex::new(None),
            continuation_dispatch_notify: Notify::new(),
            continuation_dispatch_gate: Arc::new(AsyncMutex::new(())),
            continuation_dispatch_durability: Mutex::new(ContinuationDispatchDurabilityState {
                durable_through_seq,
                persistence_failures: BTreeMap::new(),
            }),
            shutting_down: Mutex::new(false),
            quarantine: Mutex::new(None),
        })
    }

    /// Returns the exact immutable storage locator owned by this production handle.
    /// 返回该生产句柄拥有的精确不可变存储定位器。
    ///
    /// # Returns / 返回值
    ///
    /// A cloned locator safe to use after releasing all document and writer locks.
    /// 一份可在释放全部文档锁与 writer 锁后安全使用的定位器克隆。
    pub(crate) fn storage_locator(&self) -> Result<SessionV1StorageLocator, SessionRuntimeError> {
        self.locator
            .clone()
            .ok_or(SessionRuntimeError::StorageLocatorUnavailable)
    }

    /// Returns the unique session-scoped Provider API trace owner.
    /// 返回唯一的会话级 Provider API 追踪 owner。
    ///
    /// # Returns / 返回值
    ///
    /// Production handles return one cloned owner sharing the same synchronization state; test
    /// handles without a physical locator return `None`.
    /// 生产句柄返回共享同一同步状态的 owner 克隆；没有物理定位器的测试句柄返回 `None`。
    pub(crate) fn provider_api_trace_store(&self) -> Option<ProviderApiTraceStore> {
        self.provider_api_trace_store.clone()
    }

    /// Publishes one edited history generation and replaces the resident document under one owner.
    /// 在同一个 owner 下发布一份编辑后的历史 generation，并替换常驻文档。
    ///
    /// # Parameters / 参数
    /// `expected_segment_generation` is the caller's exact CAS frontier and `input` is the complete
    /// Core edit request.
    /// `expected_segment_generation` 是调用方的精确 CAS 前沿，`input` 是完整 Core 编辑请求。
    ///
    /// # Returns / 返回值
    /// Returns only after metadata published the new generation, a strict full reload succeeded,
    /// and the same process-local handle owns that reloaded document.
    /// 仅在 metadata 发布新 generation、严格全量重载成功且同一进程内句柄接管该重载文档后返回。
    pub(crate) fn publish_edit_generation(
        &self,
        expected_segment_generation: u64,
        input: EditGenerationInput,
    ) -> Result<SessionEditGenerationReceipt, SessionRuntimeError> {
        self.ensure_admitted(SessionMutationAdmission::RootTurn)?;
        let _writer = self
            .writer
            .lock()
            .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
        let _publisher = self
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        let prior_revision = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?
            .memory_revision();
        self.flush_through_with_writer_and_publisher_locked(prior_revision)?;
        self.ensure_not_quarantined()?;
        let locator = self.storage_locator()?;
        let loaded = load_session_v1_full(&locator)?;
        if loaded.metadata.segment_generation != expected_segment_generation {
            return Err(SessionRuntimeError::GenerationSourceChanged {
                expected: expected_segment_generation,
                actual: loaded.metadata.segment_generation,
            });
        }
        {
            let resident = self
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
            if resident.memory_revision() != loaded.document.memory_revision()
                || resident.segment_generation() != loaded.document.segment_generation()
            {
                return Err(SessionRuntimeError::GenerationResidentMismatch {
                    resident_revision: resident.memory_revision(),
                    durable_revision: loaded.document.memory_revision(),
                    resident_generation: resident.segment_generation(),
                    durable_generation: loaded.document.segment_generation(),
                });
            }
        }
        let output = loaded.document.build_edited_generation(input)?;
        let published = publish_edit_session_v1(&locator, &loaded.metadata, &output)?;
        let reloaded = match load_session_v1_full(&locator) {
            Ok(reloaded) => reloaded,
            Err(error) => {
                let reason = SessionQuarantineReason::GenerationReloadRejected {
                    detail: error.to_string(),
                };
                self.enter_quarantine(reason.clone())?;
                return Err(SessionRuntimeError::SessionQuarantined {
                    session_id: self.session_id,
                    reason,
                });
            }
        };
        if reloaded.metadata != published {
            let reason = SessionQuarantineReason::GenerationReloadRejected {
                detail: String::from(
                    "edited generation strict reload disagrees with published metadata",
                ),
            };
            self.enter_quarantine(reason.clone())?;
            return Err(SessionRuntimeError::SessionQuarantined {
                session_id: self.session_id,
                reason,
            });
        }
        let new_revision = reloaded.document.memory_revision();
        let new_generation = reloaded.document.segment_generation();
        let new_durable_through_seq = reloaded.document.events().last().map(|event| event.seq);
        let target_seq = output.history_target_seq;
        let mut resident = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        *resident = reloaded.document;
        drop(resident);
        let mut continuation_durability = self
            .continuation_dispatch_durability
            .lock()
            .map_err(|_| SessionRuntimeError::ContinuationDispatchStateLockPoisoned)?;
        continuation_durability.durable_through_seq = new_durable_through_seq;
        continuation_durability.persistence_failures.clear();
        drop(continuation_durability);
        self.continuation_dispatch_notify.notify_waiters();
        self.publish_generation_rebase_best_effort(prior_revision, new_revision, new_generation);
        Ok(SessionEditGenerationReceipt {
            target_seq,
            segment_generation: new_generation,
            document_revision: new_revision,
        })
    }

    /// Builds one immutable fork generation from the exact fully loaded durable source.
    /// 从精确完整加载的可靠源构建一份不可变分叉 generation。
    ///
    /// # Parameters / 参数
    /// `expected_segment_generation` is the source CAS frontier and `input` owns the child identity.
    /// `expected_segment_generation` 是源 CAS 前沿，`input` 持有子会话身份。
    ///
    /// # Returns / 返回值
    /// Returns a storage-independent Core output without mutating the parent.
    /// 返回不修改父会话且与存储无关的 Core 输出。
    pub(crate) fn build_fork_generation(
        &self,
        expected_segment_generation: u64,
        input: ForkGenerationInput,
    ) -> Result<GenerationBuildOutput, SessionRuntimeError> {
        self.ensure_admitted(SessionMutationAdmission::RootTurn)?;
        let _writer = self
            .writer
            .lock()
            .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
        let _publisher = self
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        let prior_revision = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?
            .memory_revision();
        self.flush_through_with_writer_and_publisher_locked(prior_revision)?;
        self.ensure_not_quarantined()?;
        let loaded = load_session_v1_full(&self.storage_locator()?)?;
        if loaded.metadata.segment_generation != expected_segment_generation {
            return Err(SessionRuntimeError::GenerationSourceChanged {
                expected: expected_segment_generation,
                actual: loaded.metadata.segment_generation,
            });
        }
        {
            let resident = self
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
            if resident.memory_revision() != loaded.document.memory_revision()
                || resident.segment_generation() != loaded.document.segment_generation()
            {
                return Err(SessionRuntimeError::GenerationResidentMismatch {
                    resident_revision: resident.memory_revision(),
                    durable_revision: loaded.document.memory_revision(),
                    resident_generation: resident.segment_generation(),
                    durable_generation: loaded.document.segment_generation(),
                });
            }
        }
        loaded
            .document
            .build_fork_generation(input)
            .map_err(SessionRuntimeError::from)
    }

    /// Installs the sole process-local publisher used by volatile and durable V1 updates.
    /// 安装 V1 volatile 与 durable 更新共同使用的唯一进程内发布器。
    fn install_live_event_publisher(
        &self,
        emitter: ServiceEventEmitter,
        service_epoch: impl Into<String>,
    ) -> Result<(), SessionRuntimeError> {
        let mut live_event_publisher = self
            .live_event_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::LivePublisherLockPoisoned)?;
        *live_event_publisher = Some(SessionLivePublisher {
            emitter,
            service_epoch: service_epoch.into(),
        });
        Ok(())
    }

    /// Installs one deterministic live publisher for a direct handle test.
    /// 为直接句柄测试安装一个确定性的实时发布器。
    #[cfg(test)]
    fn install_live_event_emitter(
        &self,
        emitter: ServiceEventEmitter,
    ) -> Result<(), SessionRuntimeError> {
        self.install_live_event_publisher(emitter, "test-service-epoch")
    }

    /// Returns the stable application session identity.
    /// 返回稳定的应用会话身份。
    pub(crate) fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Starts a root Turn and publishes it before any Provider work begins.
    /// 启动根回合，并在任何 Provider 工作开始前发布它。
    pub(crate) fn submit_root_input(
        &self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<TurnId>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            move |document| {
                document
                    .submit_root_input(submission, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Durably queues one uniquely identified input for a future Turn.
    /// 可靠排队一条具有唯一身份、用于未来回合的输入。
    ///
    /// # Parameters
    /// 参数
    /// `submission` carries the authoritative client action, text, and ordered resource ids;
    /// `now_ms` is the single admission timestamp.
    /// `submission` 携带权威客户端动作、正文与有序资源 ID；`now_ms` 是唯一准入时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact pending-input identity and whether the client action committed a new
    /// revision or matched an already durable submission.
    /// 返回精确的待处理输入身份，以及该客户端动作是提交新修订还是命中已可靠提交。
    pub(crate) fn enqueue_input(
        &self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<EnqueueInputReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            move |document| {
                document
                    .enqueue_input(submission, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Durably replaces the existing-session composer draft with one exact payload.
    /// 使用一份精确载荷可靠替换既有会话输入区草稿。
    pub(crate) fn set_draft(
        &self,
        text: String,
        resource_ids: Vec<ResourceId>,
        staged_attachment_ids: Vec<Option<String>>,
        resource_sources: Vec<Option<vulcan_session_core::v1::UserPromptResourceSource>>,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<DraftMutationReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document
                    .set_draft_with_resource_sources(
                        text,
                        resource_ids,
                        staged_attachment_ids,
                        resource_sources,
                        now_ms,
                    )
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Durably clears the existing-session composer draft.
    /// 可靠清空既有会话输入区草稿。
    pub(crate) fn clear_draft(
        &self,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<DraftMutationReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document
                    .clear_draft(now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Durably moves one exact queued input into the composer draft.
    /// 可靠地把一条精确排队输入移动到输入区草稿。
    pub(crate) fn move_queued_input_to_draft(
        &self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<QueuedInputDraftReceipt>, SessionRuntimeError>
    {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document
                    .move_queued_input_to_draft(identity, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Durably removes one exact queued input.
    /// 可靠移除一条精确排队输入。
    pub(crate) fn remove_queued_input(
        &self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<QueuedInputRemovalReceipt>, SessionRuntimeError>
    {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document
                    .remove_queued_input(identity, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Durably transfers one exact queued input into the currently open logical Turn.
    /// 可靠地把一条精确排队输入转移进当前开放逻辑回合。
    pub(crate) fn promote_queued_input_to_pending_turn(
        &self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<QueuedInputPendingTurnReceipt>, SessionRuntimeError>
    {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::InputAccepted,
            move |document| {
                document
                    .promote_queued_input_to_pending_turn(identity, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Durably publishes one complete exact FIFO queue order.
    /// 可靠发布一份完整且精确的 FIFO 队列顺序。
    pub(crate) fn reorder_queued_inputs(
        &self,
        ordered: Vec<QueuedInputIdentity>,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<QueuedInputOrderReceipt>, SessionRuntimeError>
    {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document
                    .reorder_queued_inputs(ordered, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Durably commits one fully projected physical API start before network dispatch.
    /// 在网络派发前可靠提交一份已经完整投影的物理 API 起始事实。
    ///
    /// # Parameters
    /// 参数
    /// `input` contains the exact request configuration, projection proof, and typed locked
    /// baseline produced by the unique Provider preparation owner.
    /// `input` 包含唯一 Provider 准备所有者产生的精确请求配置、投影证明与强类型锁定基线。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact durable API identity and projection receipt. Callers must never build
    /// this input from legacy conversation state or infer baseline provenance from labels.
    /// 返回精确的可靠 API 身份与投影回执；调用方不得从旧会话状态构造该输入，也不得从标签推断基线来源。
    #[cfg(test)]
    pub(crate) fn prepare_and_start_api_durable(
        &self,
        input: PrepareAndStartApiInput,
    ) -> Result<SessionConditionalMutationReceipt<PreparedApiStart>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiStarted,
            move |document| {
                document
                    .prepare_and_start_api(input)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Reconstructs and durably commits one physical Provider request under the sole session owner.
    /// 在唯一会话 owner 下重建并可靠提交一次物理 Provider 请求。
    ///
    /// # Parameters
    /// 参数
    /// `purpose`, `request`, `locked_baseline`, and `continuation_action_id` are typed request
    /// facts; `build` consumes the immutable durable V1 projection and its scoped completion authority,
    /// returning the only transport value that may be released or an explicit stale-result rejection.
    /// `purpose`、`request`、`locked_baseline` 与 `continuation_action_id` 是强类型请求事实；
    /// `build`消费不可变可靠V1投影及其作用域完成权限，返回唯一允许放行的传输值或明确陈旧结果拒绝。
    ///
    /// # Returns
    /// 返回值
    /// Returns a receipt only after prior state, `api_started`, and any fresh baseline are durable.
    /// 仅在先前状态、`api_started` 与首次基线全部可靠落盘后返回回执。
    /// Returns `None` when pending selection or an external document frontier changed before commit; own completion writes require real durable recapture.
    /// 待生效选择或外部文档前沿在提交之前变化时返回`None`；自身完成写入要求真实可靠重捕获。
    /// The asynchronous preparation gate serializes builders; writer/publisher guards cover capture and commit only.
    /// 异步准备门串行化构建者；写入器与发布器守卫仅覆盖捕获和提交。
    pub(crate) async fn prepare_physical_api_durable<T, BuildFuture>(
        self: &Arc<Self>,
        purpose: ApiPurposeKind,
        request: ApiRequestSnapshot,
        locked_baseline: LockedBaselineProjection,
        continuation_action_id: Option<ClientActionId>,
        now_ms: i64,
        build: impl FnOnce(
            SessionProviderProjectionSnapshot,
            Arc<SessionPhysicalPreparation>,
        ) -> BuildFuture,
    ) -> Result<
        Option<SessionConditionalMutationReceipt<SessionPreparedPhysicalApi<T>>>,
        SessionRuntimeError,
    >
    where
        BuildFuture: std::future::Future<
                Output = Result<Option<SessionPreparedPhysicalRequest<T>>, SessionRuntimeError>,
            >,
    {
        // Serialize whole preparations without holding the synchronous document or persistence locks across I/O.
        // 串行化完整准备，不跨输入输出持有同步文档或持久化锁。
        let lease = Arc::new(
            Arc::clone(&self.physical_request_preparation)
                .lock_owned()
                .await,
        );
        self.ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
        // Phase one captures a durable frontier while the existing writer and publisher preserve ordering.
        // 第一阶段捕获可靠前沿，既有写入器与发布器保持顺序。
        let snapshot = {
            let _writer = self
                .writer
                .lock()
                .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
            let _publisher = self
                .mutation_publisher
                .lock()
                .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
            let prior_revision = self
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?
                .memory_revision();
            self.flush_through_with_writer_and_publisher_locked(prior_revision)?;
            {
                // Reject an absent turn or an already running API before expensive resource preparation begins.
                // 在昂贵资源准备开始之前，拒绝缺失回合或已经运行中的 API。
                let document = self
                    .document
                    .lock()
                    .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
                if document.active_turn().is_none_or(|turn| turn.closed) {
                    return Err(DomainError::NoActiveTurn.into());
                }
                if document.active_api().is_some() {
                    return Err(DomainError::NoActiveApi.into());
                }
            }
            // A picker write may arrive after construction. Rebuild before promotion, without a failed API or a false switch record.
            // 选择器写入可能晚于请求构建；必须在提升前重建，不制造失败 API 或虚假切换记录。
            let selection_changed = self
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?
                .metadata
                .config
                .pending_request
                .as_ref()
                .is_some_and(|pending| {
                    pending.provider.as_deref() != Some(request.provider.as_str())
                        || pending.connection_id != request.connection_id
                        || pending.model.as_deref() != Some(request.model.as_str())
                        || vulcan_session_core::v1::config::canonical_request_thinking_level(
                            pending.thinking_level.as_deref(),
                        ) != vulcan_session_core::v1::config::canonical_request_thinking_level(
                            request.thinking_level.as_deref(),
                        )
                        || pending.temperature != request.temperature
                });
            if selection_changed {
                return Ok(None);
            }
            // pendingPromotion is owned by the same serialized Provider permit boundary that captures
            // the projection and starts the next physical API. No legacy runtime document participates.
            // pendingPromotion 由捕获投影并启动下一次物理 API 的同一串行 Provider permit 边界拥有；
            // 旧运行态文档不再参与该事务。
            let pending_promotion = self.mutate_in_memory_if_changed_with_publisher_locked(
                SessionFlushReason::ApiStarted,
                |document| {
                    if document.metadata.config.pending_request.is_none() {
                        return Ok(SessionMutationChange::Unchanged(()));
                    }
                    let turn_id = document.active_turn().map(|active| active.turn_id);
                    document.promote_pending_request(turn_id, now_ms)?;
                    Ok(SessionMutationChange::Changed(()))
                },
            )?;
            self.flush_through_with_writer_and_publisher_locked(pending_promotion.revision)?;
            self.capture_provider_projection_snapshot_with_publisher_locked()?
        };
        // Completed-result workers retain this same lease if the asynchronous builder is dropped.
        // 异步构建者被丢弃时，已完成结果工作线程保留同一租约。
        let preparation = Arc::new(SessionPhysicalPreparation::new(
            Arc::clone(self),
            lease,
            snapshot.clone(),
        ));
        // Phase two may read media, upload or encode asynchronously after both synchronous guards are gone.
        // 第二阶段仅在两个同步守卫释放后，才可异步读取媒体、上传或编码。
        // Keep the potentially large media/encoding future off every enclosing async frame without spawning detached work.
        // 将可能较大的媒体及编码Future移出各层外围异步帧，不启动分离工作。
        let Some(prepared) = Box::pin(build(snapshot, Arc::clone(&preparation))).await? else {
            return Ok(None);
        };
        // Only the official durable recapture can advance request proof after this preparation's own writes.
        // 本准备自身写入之后，只有正式可靠重捕获可以推进请求证明。
        let snapshot = preparation.last_snapshot()?;
        if prepared.projection.segment_generation != snapshot.segment_generation
            || prepared.projection.document_revision != snapshot.document_revision
            || prepared.projection.retained_from_seq != snapshot.retained_from_seq
            || prepared.projection.through_seq != snapshot.through_seq
        {
            return Err(SessionRuntimeError::ProviderProjectionInvariant(
                String::from("request proof does not match the captured durable V1 frontier"),
            ));
        }
        // Reacquire the same owner's locks before accepting any prepared result.
        // 接受任何准备结果之前，重新取得同一所有者的锁。
        self.ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
        let _writer = self
            .writer
            .lock()
            .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
        let _publisher = self
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        if preparation.invalidated()? {
            return Ok(None);
        }
        {
            // Any intervening generation or revision change invalidates the prepared transport, even without a picker change.
            // 任何期间发生的代际或修订变化都会使准备传输失效，即使选择器没有变化。
            let document = self
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
            if document.segment_generation() != snapshot.segment_generation
                || document.memory_revision() != snapshot.document_revision
                || document.durable_revision() != snapshot.document_revision
            {
                return Ok(None);
            }
        }
        let expected_request = request.clone();
        let expected_projection = prepared.projection.clone();
        let expected_request_budget_evidence = prepared.request_budget_evidence.clone();
        let receipt = self.mutate_in_memory_if_changed_with_publisher_locked(
            SessionFlushReason::ApiStarted,
            move |document| {
                let mutation = document.prepare_and_start_api_with_user_prompt_media(
                    PrepareAndStartApiInput {
                        purpose,
                        request,
                        request_budget_evidence: prepared.request_budget_evidence,
                        projection: prepared.projection,
                        locked_baseline,
                        continuation_action_id,
                        now_ms,
                    },
                    prepared.user_prompt_media,
                )?;
                match mutation {
                    IdempotentMutation::Committed(core_start) => {
                        Ok(SessionMutationChange::Changed(SessionPreparedPhysicalApi {
                            core_start,
                            transport: prepared.transport,
                        }))
                    }
                    IdempotentMutation::Idempotent(core_start) => {
                        let existing = document.events().iter().find(|event| {
                            event.seq() == core_start.started_seq
                                && matches!(&event.event, EventBody::ApiStarted(_))
                        });
                        let Some(existing) = existing else {
                            return Err(DomainError::ImmutableFieldMutation {
                                entity: "api_started",
                                field: "idempotent_source_missing",
                            });
                        };
                        let EventBody::ApiStarted(existing) = &existing.event else {
                            unreachable!("the predicate accepted only api_started events")
                        };
                        if existing.request != expected_request
                            || existing.request_budget_evidence != expected_request_budget_evidence
                            || existing.projection.fingerprint_sha256
                                != expected_projection.fingerprint_sha256
                        {
                            return Err(DomainError::ImmutableFieldMutation {
                                entity: "api_started",
                                field: "idempotent_request_drift",
                            });
                        }
                        Ok(SessionMutationChange::Unchanged(
                            SessionPreparedPhysicalApi {
                                core_start,
                                transport: prepared.transport,
                            },
                        ))
                    }
                }
            },
        )?;
        self.flush_through_with_writer_and_publisher_locked(receipt.revision)?;
        Ok(Some(receipt))
    }

    /// Captures one durable Provider projection for context-compaction planning.
    /// 为上下文压缩规划捕获一份可靠 Provider 投影。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact flushed projection frontier and source ranges while the session publisher
    /// remains serialized; no transcript or runtime-state fallback participates.
    /// 在会话发布器保持串行时返回精确已 flush 的投影前沿及来源范围；不参与任何 transcript 或
    /// runtime-state 回退。
    pub(crate) fn capture_context_compaction_planning_snapshot(
        &self,
    ) -> Result<SessionProviderProjectionSnapshot, SessionRuntimeError> {
        self.ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
        let _writer = self
            .writer
            .lock()
            .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
        let _publisher = self
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        let revision = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?
            .memory_revision();
        self.flush_through_with_writer_and_publisher_locked(revision)?;
        self.capture_provider_projection_snapshot_with_publisher_locked()
    }

    /// Captures the exact durable V1 Provider projection while publication is serialized.
    /// 在发布已串行化时捕获精确可靠的 V1 Provider 投影。
    fn capture_provider_projection_snapshot_with_publisher_locked(
        &self,
    ) -> Result<SessionProviderProjectionSnapshot, SessionRuntimeError> {
        let locator = self
            .locator
            .as_ref()
            .ok_or(SessionRuntimeError::ProviderProjectionRequiresStorageLocator)?;
        let (
            segment_generation,
            document_revision,
            retained_from_seq,
            through_seq,
            preparing_user_prompt_media_seqs,
        ) = {
            let document = self
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
            if document.memory_revision() != document.durable_revision() {
                return Err(SessionRuntimeError::ProviderProjectionInvariant(
                    String::from("document remained dirty after the pre-projection flush"),
                ));
            }
            let through_value = document
                .metadata
                .id_frontier
                .next_seq
                .checked_sub(1)
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    SessionRuntimeError::ProviderProjectionInvariant(String::from(
                        "an API cannot start before the first durable event",
                    ))
                })?;
            let retained_from_seq = match document.runtime_lock.current_compaction_id {
                Some(compaction_id) => document
                    .compactions
                    .iter()
                    .find(|node| node.compaction_id == compaction_id)
                    .map(|node| node.retained_from_seq)
                    .ok_or_else(|| {
                        SessionRuntimeError::ProviderProjectionInvariant(String::from(
                            "runtime compaction pointer has no matching durable node",
                        ))
                    })?,
                None => Seq::try_new(1)?,
            };
            (
                document.segment_generation(),
                document.memory_revision(),
                retained_from_seq,
                Seq::try_new(through_value)?,
                document.preparing_user_prompt_media_seqs(),
            )
        };
        let range = locator.load_event_range(Seq::try_new(1)?, through_seq)?;
        if range.state.segment_generation != segment_generation
            || range.state.document_revision != document_revision
            || range.state.next_seq != through_seq.value().saturating_add(1)
        {
            return Err(SessionRuntimeError::ProviderProjectionInvariant(
                String::from("exact event range frontier changed during Provider projection"),
            ));
        }
        let backfill_system = if retained_from_seq.value() > 1 {
            derive_active_system_state(&range.events, Seq::try_new(retained_from_seq.value() - 1)?)?
        } else {
            Vec::new()
        };
        let sourced_items = project_provider_items_with_sources(
            &range.events,
            Some(retained_from_seq),
            &backfill_system,
        )?;
        let items = sourced_items
            .iter()
            .map(|projected| projected.item.clone())
            .collect();
        Ok(SessionProviderProjectionSnapshot {
            preparing_user_prompt_media_seqs,
            segment_generation,
            document_revision,
            retained_from_seq,
            through_seq,
            items,
            sourced_items,
            project_directory: locator.project_directory(),
            provider_usage_budget_anchor: latest_provider_usage_budget_anchor(&range.events),
        })
    }

    /// Durably records that one prepared physical API crossed the transport boundary.
    /// 可靠记录一次已准备物理 API 已经越过传输边界。
    ///
    /// # Parameters
    /// 参数
    /// `api_call_id` is the exact local API identity returned by prepare, and `now_ms` is the
    /// transport observation time.
    /// `api_call_id` 是 prepare 返回的精确本地 API 身份，`now_ms` 是传输观测时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or idempotent Core receipt only after its observed revision is
    /// durable. A repeated observation never creates a second revision.
    /// 仅在所观察修订可靠落盘后返回 Core 的已提交或幂等回执；重复观测绝不创建第二个修订。
    pub(crate) async fn mark_api_transport_observed_durable(
        self: &Arc<Self>,
        api_call_id: ApiCallId,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<ApiTransportObservedReceipt>, SessionRuntimeError>
    {
        let receipt = Arc::clone(self)
            .mutate_durable_if_changed_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiTransportObserved,
                move |document| {
                    document
                        .mark_api_transport_observed(api_call_id, now_ms)
                        .map(session_change_from_idempotent)
                },
            )
            .await?;
        Ok(receipt)
    }

    /// Waits until one Continue action owns a durably observed physical API request.
    /// 等待一次 Continue 动作拥有一个已可靠观察的物理 API 请求。
    ///
    /// # Parameters
    /// 参数
    /// `action_id` is the exact browser idempotency identity and `wait` is the maximum wait.
    /// `action_id` 是精确的浏览器幂等身份，`wait` 是最大等待时长。
    ///
    /// # Returns
    /// 返回值
    /// Returns the persisted Core receipt. The query never infers dispatch from revisions or time.
    /// 返回持久化 Core 回执；查询绝不从修订号或时间推断派发事实。
    pub(crate) async fn wait_for_continuation_transport_observed(
        &self,
        action_id: &ClientActionId,
        wait: Duration,
    ) -> Result<ApiTransportObservedReceipt, SessionRuntimeError> {
        let deadline = Instant::now() + wait;
        loop {
            let notified = self.continuation_dispatch_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            self.ensure_not_quarantined()?;
            self.ensure_no_continuation_dispatch_persistence_failure(action_id)?;
            match self.durable_continuation_dispatch_resolution(action_id)? {
                Some(ContinuationDispatchResolution::TransportObserved(receipt)) => {
                    return Ok(receipt);
                }
                Some(ContinuationDispatchResolution::TerminalBeforeTransport {
                    api_call_id,
                    ..
                }) => {
                    return Err(SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                        client_action_id: action_id.clone(),
                        api_call_id,
                    });
                }
                Some(ContinuationDispatchResolution::AwaitingTransport { .. }) | None => {}
            }
            if timeout_at(deadline, notified).await.is_err() {
                return Err(
                    SessionRuntimeError::ContinuationTransportObservationTimedOut {
                        client_action_id: action_id.clone(),
                    },
                );
            }
        }
    }

    /// Waits without a client deadline until a detached Continue dispatch is observed or terminates.
    /// 不受客户端截止时间影响地等待分离 Continue 分派被观察或终止。
    ///
    /// # Parameters / 参数
    /// `action_id` is the exact browser idempotency identity retained by the detached owner.
    /// `action_id` 是分离所有者保留的精确浏览器幂等身份。
    ///
    /// # Returns / 返回值
    /// The durable transport receipt, or an explicit terminal-before-transport failure.
    /// 返回可靠传输回执，或明确的传输前终止失败。
    pub(crate) async fn wait_until_continuation_dispatch_settled(
        &self,
        action_id: &ClientActionId,
    ) -> Result<ApiTransportObservedReceipt, SessionRuntimeError> {
        loop {
            let notified = self.continuation_dispatch_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            self.ensure_not_quarantined()?;
            self.ensure_no_continuation_dispatch_persistence_failure(action_id)?;
            match self.durable_continuation_dispatch_resolution(action_id)? {
                Some(ContinuationDispatchResolution::TransportObserved(receipt)) => {
                    return Ok(receipt);
                }
                Some(ContinuationDispatchResolution::TerminalBeforeTransport {
                    api_call_id,
                    ..
                }) => {
                    return Err(SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                        client_action_id: action_id.clone(),
                        api_call_id,
                    });
                }
                Some(ContinuationDispatchResolution::AwaitingTransport { .. }) | None => {
                    notified.await;
                }
            }
        }
    }

    /// Resolves one Continue action only through the process-local durable event frontier.
    /// 仅通过进程内可靠事件前沿解析一次 Continue 动作。
    ///
    /// # Parameters / 参数
    ///
    /// `action_id` is the exact persisted browser idempotency identity.
    /// `action_id` 是精确持久化的浏览器幂等身份。
    ///
    /// # Returns / 返回值
    ///
    /// Returns no state before any event is durable, otherwise delegates to the Core bounded-prefix
    /// projection. It never exposes a newer in-memory-only terminal or transport observation.
    /// 尚无可靠事件时返回空；否则委托 Core 的有界前缀投影。绝不暴露更新但仅存在于内存的终态或
    /// 传输观察事实。
    fn durable_continuation_dispatch_resolution(
        &self,
        action_id: &ClientActionId,
    ) -> Result<Option<ContinuationDispatchResolution>, SessionRuntimeError> {
        let through_seq = self
            .continuation_dispatch_durability
            .lock()
            .map_err(|_| SessionRuntimeError::ContinuationDispatchStateLockPoisoned)?
            .durable_through_seq;
        let Some(through_seq) = through_seq else {
            return Ok(None);
        };
        self.read(|document| {
            document.continuation_dispatch_resolution_through(action_id, through_seq)
        })?
        .map_err(SessionRuntimeError::from)
    }

    /// Rejects one active waiter when its exact Continue candidate failed persistence.
    /// 当精确 Continue 候选持久化失败时拒绝对应的活动等待者。
    ///
    /// # Parameters / 参数
    ///
    /// `action_id` is the exact action currently awaiting a durable dispatch decision.
    /// `action_id` 是当前等待可靠分派判定的精确动作。
    ///
    /// # Returns / 返回值
    ///
    /// Returns success when no failed candidate is pending, otherwise returns the retained
    /// structured persistence failure so the detached owner releases its single-flight gate.
    /// 没有待处理失败候选时返回成功；否则返回保留的结构化持久化失败，使分离 owner 释放单飞门。
    fn ensure_no_continuation_dispatch_persistence_failure(
        &self,
        action_id: &ClientActionId,
    ) -> Result<(), SessionRuntimeError> {
        let state = self
            .continuation_dispatch_durability
            .lock()
            .map_err(|_| SessionRuntimeError::ContinuationDispatchStateLockPoisoned)?;
        let Some(failure) = state.persistence_failures.get(action_id) else {
            return Ok(());
        };
        Err(SessionRuntimeError::ContinuationDispatchPersistenceFailed {
            client_action_id: action_id.clone(),
            candidate_revision: failure.candidate_revision,
            detail: failure.detail.clone(),
        })
    }

    /// Acquires the sole process-local Continue dispatch owner for this session.
    /// 获取当前会话唯一的进程内 Continue 分派所有者。
    ///
    /// # Returns / 返回值
    /// A guard held from durable Continue admission until durable transport observation. The guard
    /// coordinates concurrent callers only; the V1 event stream remains the durable authority.
    /// 返回从可靠 Continue 准入持有至可靠传输观察的守卫。该守卫只协调并发调用；V1 事件流
    /// 仍是持久化权威。
    pub(crate) async fn acquire_continuation_dispatch(&self) -> ContinuationDispatchGuard {
        ContinuationDispatchGuard {
            _guard: Arc::clone(&self.continuation_dispatch_gate)
                .lock_owned()
                .await,
        }
    }

    /// Reads whether one Continue action already owns a durably observed physical request.
    /// 读取一次 Continue 动作是否已经拥有可靠观察的物理请求。
    ///
    /// # Parameters / 参数
    /// `action_id` is the exact browser-generated idempotency identity.
    /// `action_id` 是浏览器生成的精确幂等身份。
    ///
    /// # Returns / 返回值
    /// The persisted receipt when observed, `None` before observation, or a strict history error.
    /// 已观察时返回持久化回执；观察前返回 `None`；历史不合法时返回严格错误。
    #[cfg(test)]
    pub(crate) fn continuation_transport_observed_for_action(
        &self,
        action_id: &ClientActionId,
    ) -> Result<Option<ApiTransportObservedReceipt>, SessionRuntimeError> {
        self.ensure_not_quarantined()?;
        match self.durable_continuation_dispatch_resolution(action_id)? {
            Some(ContinuationDispatchResolution::TransportObserved(receipt)) => Ok(Some(receipt)),
            Some(ContinuationDispatchResolution::TerminalBeforeTransport {
                api_call_id, ..
            }) => Err(SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                client_action_id: action_id.clone(),
                api_call_id,
            }),
            Some(ContinuationDispatchResolution::AwaitingTransport { .. }) | None => Ok(None),
        }
    }

    /// Reconciles one Continue action's dirty dispatch facts before allowing another physical API.
    /// 在允许另一次物理 API 前，协调一次 Continue 动作尚未落盘的分派事实。
    ///
    /// # Parameters / 参数
    /// `action_id` is the exact browser-generated idempotency identity protected by the caller's
    /// per-session Continue dispatch guard.
    /// `action_id` 是由调用方会话级 Continue 分派守卫保护的精确浏览器幂等身份。
    ///
    /// # Returns / 返回值
    /// Returns the original durable transport receipt when the physical request was already
    /// observed, `None` only when the action may still start its first physical request, or a
    /// strict terminal/persistence error. Any dirty `ApiStarted`, transport observation, or API
    /// terminal fact is flushed and re-read under the sole writer and publisher boundary first.
    /// 若物理请求已经被观察，则返回原始可靠传输回执；仅当该动作仍可启动首次物理请求时返回
    /// `None`；终态或持久化失败则返回严格错误。任何尚未落盘的 `ApiStarted`、传输观察或 API
    /// 终态事实都会先在唯一 writer 与 publisher 边界内完成 flush 并重新读取。
    pub(crate) fn reconcile_continuation_dispatch_for_action(
        &self,
        action_id: &ClientActionId,
    ) -> Result<Option<ApiTransportObservedReceipt>, SessionRuntimeError> {
        self.ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
        let _writer = self
            .writer
            .lock()
            .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
        let _publisher = self
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        let required_revision = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?
            .memory_revision();
        self.flush_through_with_writer_and_publisher_locked(required_revision)?;
        self.ensure_not_quarantined()?;
        self.ensure_no_continuation_dispatch_persistence_failure(action_id)?;
        match self.durable_continuation_dispatch_resolution(action_id)? {
            Some(ContinuationDispatchResolution::TransportObserved(receipt)) => Ok(Some(receipt)),
            Some(ContinuationDispatchResolution::TerminalBeforeTransport {
                api_call_id, ..
            }) => Err(SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                client_action_id: action_id.clone(),
                api_call_id,
            }),
            Some(ContinuationDispatchResolution::AwaitingTransport { .. }) | None => Ok(None),
        }
    }

    /// Atomically prepares one ordered Provider tool batch and durably publishes its local ids.
    /// 原子准备一批有序 Provider 工具，并可靠发布其本地身份。
    ///
    /// # Parameters
    /// 参数
    /// `api_call_id` identifies the owning physical API, `preparations` preserve Provider order,
    /// `provider_source` binds replay provenance, and `now_ms` is the preparation time.
    /// `api_call_id` 标识所属物理 API，`preparations` 保留 Provider 顺序，`provider_source`
    /// 绑定重放来源，`now_ms` 是准备时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact ordered Core receipt after durability. Replaying the same batch returns
    /// the original receipt without a second revision or live edge.
    /// 在可靠落盘后返回精确有序 Core 回执；重放同一批次时返回原回执，不产生第二修订或实时边。
    pub(crate) async fn prepare_tool_batch_with_provider_source_durable(
        self: &Arc<Self>,
        api_call_id: ApiCallId,
        preparations: Vec<ToolPreparation>,
        provider_source: ProviderSourceInput,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<PreparedToolBatchReceipt>, SessionRuntimeError>
    {
        Arc::clone(self)
            .mutate_durable_if_changed_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                move |document| {
                    document
                        .prepare_tool_batch_with_provider_source(
                            api_call_id,
                            preparations,
                            provider_source,
                            now_ms,
                        )
                        .map(session_change_from_idempotent)
                },
            )
            .await
    }

    /// Durably closes the current successful Turn and atomically selects its sole successor.
    /// 可靠关闭当前成功回合，并原子选择其唯一后继。
    ///
    /// # Parameters
    /// 参数
    /// `now_ms` is the final assistant observation time.
    /// `now_ms` 是最终 assistant 观测时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns a pending-child decision without closure, or one durable closure and successor receipt.
    /// 返回不闭合回合的待处理子任务决策，或一份可靠回合闭合及后继回执。
    pub(crate) async fn complete_successful_turn(
        self: &Arc<Self>,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<SuccessfulTurnSettlement>, SessionRuntimeError>
    {
        self.clone()
            .mutate_durable_if_changed_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::TurnCompleted,
                move |document| {
                    // Check under the same writer lock as closure, so no return can land between the two.
                    // 在关闭事务的同一写锁下检查，防止回流落在检查与关闭之间。
                    if document.pending_subagent_return_dispatch()?.is_some() {
                        return Ok(SessionMutationChange::Unchanged(
                            SuccessfulTurnSettlement::SubagentReturn,
                        ));
                    }
                    let expected_turn_id = document
                        .active_turn()
                        .map(|turn| turn.turn_id)
                        .ok_or(DomainError::NoActiveTurn)?;
                    document
                        .complete_turn_and_select_successor(expected_turn_id, now_ms)
                        .map(|transition| {
                            SessionMutationChange::Changed(SuccessfulTurnSettlement::Completed(
                                transition,
                            ))
                        })
                },
            )
            .await
    }

    /// Durably appends one exact child return and its complete inclusive statistics to the parent.
    /// 把一条精确子代理回流及其完整包容性统计可靠追加到父会话。
    ///
    /// # Parameters / 参数
    ///
    /// - `input`: Child-domain output built only from a fully durable child document.
    ///   `input`：仅从完全可靠子文档构建的子领域输出。
    /// - `now_ms`: Exact parent observation timestamp.
    ///   `now_ms`：父级精确观测时间戳。
    ///
    /// # Returns / 返回值
    ///
    /// The committed or idempotently replayed parent receipt after durable publication.
    /// 可靠发布后已提交或幂等重放的父级回执。
    pub(crate) fn append_subagent_return_with_stats(
        &self,
        input: SubagentReturnInput,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<SubagentReturnReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::SubagentReturned,
            move |document| {
                document
                    .append_subagent_return_with_stats(input, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Updates streaming reasoning in memory without forcing per-token I/O.
    /// 仅在内存中更新流式思考，不强制逐 token I/O。
    pub(crate) fn update_reasoning(
        &self,
        api_call_id: ApiCallId,
        text: impl Into<String>,
        status: AssistantSemanticStatus,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<Seq>, SessionRuntimeError> {
        let text = text.into();
        self.mutate_in_memory_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document
                    .append_reasoning(api_call_id, text, status, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Updates streaming assistant text in memory without per-token I/O.
    /// 仅在内存中更新流式 assistant 正文，不执行逐 token I/O。
    pub(crate) fn update_assistant_text(
        &self,
        api_call_id: ApiCallId,
        text: impl Into<String>,
        status: AssistantSemanticStatus,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<Seq>, SessionRuntimeError> {
        let text = text.into();
        self.mutate_in_memory_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document
                    .append_assistant_text(api_call_id, text, status, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Publishes the durable Ask waiting checkpoint of one tool.
    /// 发布一个工具的可靠 Ask 等待检查点。
    pub(crate) fn mark_tool_waiting_user(
        &self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<()>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            move |document| {
                document
                    .mark_tool_waiting_user(api_call_id, tool_call_id, now_ms)
                    .map(|changed| {
                        if changed {
                            SessionMutationChange::Changed(())
                        } else {
                            SessionMutationChange::Unchanged(())
                        }
                    })
            },
        )
    }

    /// Durably completes one exact WaitingUser Ask with the user's typed answer result.
    /// 使用用户的强类型回答结果可靠完成一条精确的 WaitingUser Ask。
    ///
    /// # Parameters
    /// 参数
    /// `api_call_id` and `tool_call_id` identify the existing tool event; `completion` owns the
    /// exact terminal output and timestamp.
    /// `api_call_id` 与 `tool_call_id` 标识既有工具事件；`completion` 持有精确终态输出与时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns only after the in-place event revision is durably published.
    /// 仅在原位事件修订可靠发布后返回。
    pub(crate) fn answer_waiting_tool(
        &self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        completion: ToolCompletion,
    ) -> Result<SessionConditionalMutationReceipt<ToolCompletionReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            move |document| {
                document
                    .answer_waiting_tool(api_call_id, tool_call_id, completion)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Durably cancels one exact WaitingUser Ask on its original event sequence.
    /// 在原始事件序号上可靠取消一条精确的 WaitingUser Ask。
    ///
    /// # Parameters
    /// 参数
    /// `api_call_id` and `tool_call_id` identify the waiting tool, `output` is the optional
    /// producer-owned terminal payload, and `now_ms` is the terminal observation time.
    /// `api_call_id` 与 `tool_call_id` 标识等待工具，`output` 是可选的生产者终态载荷，`now_ms` 是终态观测时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns only after the exact in-place cancellation is durable.
    /// 仅在精确原位取消可靠落盘后返回。
    pub(crate) fn cancel_waiting_tool(
        &self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        output: Option<ToolPayload>,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<ToolCompletionReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            move |document| {
                document
                    .cancel_waiting_tool(api_call_id, tool_call_id, output, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Publishes the output, ChangeSet, references, and terminal tool status.
    /// 发布工具输出、ChangeSet、引用与终态状态。
    pub(crate) fn finish_tool(
        &self,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        completion: ToolCompletion,
    ) -> Result<SessionConditionalMutationReceipt<ToolCompletionReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            move |document| {
                document
                    .finish_tool(api_call_id, tool_call_id, completion)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Publishes one context generation from the exact durable compaction API facts.
    /// 从精确可靠的压缩 API 事实发布一个上下文代际。
    pub(crate) fn record_compaction(
        &self,
        input: CompactionCommitInput,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<CompactionId>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ContextChanged,
            move |document| {
                document
                    .record_compaction(input, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Records an explicit Continue on the existing open Turn durably.
    /// 在既有开放回合上可靠记录一次显式继续。
    pub(crate) fn continue_turn(
        &self,
        request: ContinuationRequest,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<ContinueTurnReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            move |document| {
                document
                    .continue_open_turn(request, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Reads one value from the authoritative document without cloning history.
    /// 从权威文档读取一个值，且不克隆历史。
    pub(crate) fn read<T>(
        &self,
        reader: impl FnOnce(&SessionDocument) -> T,
    ) -> Result<T, SessionRuntimeError> {
        let document = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        Ok(reader(&document))
    }

    /// Mutates only memory and returns the exact new revision for a later
    /// semantic flush boundary.
    /// 只修改内存，并返回精确的新修订号，供后续语义 flush 边界使用。
    #[cfg(test)]
    pub(crate) fn mutate_in_memory<T>(
        &self,
        admission: SessionMutationAdmission,
        flush_reason: SessionFlushReason,
        mutation: impl FnOnce(&mut SessionDocument) -> Result<T, DomainError>,
    ) -> Result<SessionMutationReceipt<T>, SessionRuntimeError> {
        self.ensure_admitted(admission)?;
        let _publisher = self
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        self.mutate_in_memory_with_publisher_locked(flush_reason, mutation)
    }

    /// Applies one idempotent memory-only mutation and publishes a volatile edge only when changed.
    /// 应用一次幂等的纯内存修改，并且仅在真实变化时发布 volatile 边。
    ///
    /// # Parameters
    /// 参数
    /// `admission` selects the lifecycle gate, `flush_reason` labels the later
    /// semantic flush boundary, and `mutation` returns the Core-authored
    /// changed decision while the authoritative document lock is held.
    /// `admission` 选择生命周期门，`flush_reason` 标记后续语义 flush 边界，
    /// `mutation` 在持有权威文档锁时返回 Core 产生的变化判定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact current revision without flushing. Exact replays do
    /// not publish a live edge, allocate a revision, or touch persistence.
    /// 返回当前精确修订号但不执行 flush；完全重放不会发布 live 边、分配修订号
    /// 或触碰持久化。
    pub(crate) fn mutate_in_memory_if_changed<T>(
        &self,
        admission: SessionMutationAdmission,
        flush_reason: SessionFlushReason,
        mutation: impl FnOnce(&mut SessionDocument) -> Result<SessionMutationChange<T>, DomainError>,
    ) -> Result<SessionConditionalMutationReceipt<T>, SessionRuntimeError> {
        self.ensure_admitted(admission)?;
        // Serialize the changed decision and volatile publication under one owner lock.
        // 在同一个 owner 锁内串行变化判定与 volatile 发布。
        let _publisher = self
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        self.mutate_in_memory_if_changed_with_publisher_locked(flush_reason, mutation)
    }

    /// Mutates the document and makes that exact semantic boundary durable.
    /// 修改文档，并使该精确语义边界可靠落盘。
    pub(crate) fn mutate_durable<T>(
        &self,
        admission: SessionMutationAdmission,
        flush_reason: SessionFlushReason,
        mutation: impl FnOnce(&mut SessionDocument) -> Result<T, DomainError>,
    ) -> Result<SessionMutationReceipt<T>, SessionRuntimeError> {
        self.ensure_admitted(admission)?;
        let receipt = {
            let _publisher = self
                .mutation_publisher
                .lock()
                .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
            self.mutate_in_memory_with_publisher_locked(flush_reason, mutation)?
        };
        self.flush_through(receipt.revision)?;
        Ok(receipt)
    }

    /// Applies one ordinary mutation while the caller holds the mutation-publisher lock.
    /// 在调用方持有修改发布锁时应用一次普通修改。
    ///
    /// # Parameters
    /// 参数
    /// `flush_reason` labels the semantic boundary and `mutation` must be one
    /// failure-atomic Core operation that advances exactly one revision on success.
    /// `flush_reason` 标记语义边界，`mutation` 必须是一个具备失败原子性且成功时
    /// 精确推进一个修订号的 Core 操作。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact revision receipt after its volatile edge is published.
    /// If the Core contract is violated, the handle is quarantined before the
    /// structured error returns and no candidate is flushed.
    /// 在 volatile 边发布后返回精确修订回执；若 Core 契约被破坏，则先隔离
    /// 句柄再返回结构化错误，并且不会 flush 候选。
    fn mutate_in_memory_with_publisher_locked<T>(
        &self,
        flush_reason: SessionFlushReason,
        mutation: impl FnOnce(&mut SessionDocument) -> Result<T, DomainError>,
    ) -> Result<SessionMutationReceipt<T>, SessionRuntimeError> {
        self.ensure_not_quarantined()?;
        let mut document = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        let before_revision = document.memory_revision();
        let mutation_result = mutation(&mut document);
        let after_revision = document.memory_revision();
        let value = match mutation_result {
            Ok(value) => value,
            Err(error) if after_revision == before_revision => {
                return Err(SessionRuntimeError::from(error));
            }
            Err(error) => {
                let reason = SessionQuarantineReason::FailedMutationProducedRevision {
                    before_revision,
                    after_revision,
                    error: error.clone(),
                };
                drop(document);
                self.enter_quarantine(reason)?;
                return Err(SessionRuntimeError::FailedMutationProducedRevision {
                    before_revision,
                    after_revision,
                    error: Box::new(error),
                });
            }
        };
        if before_revision.checked_add(1) != Some(after_revision) {
            let reason = SessionQuarantineReason::MutationRevisionMismatch {
                before_revision,
                after_revision,
            };
            drop(document);
            self.enter_quarantine(reason)?;
            return Err(SessionRuntimeError::MutationRevisionMismatch {
                before_revision,
                after_revision,
            });
        }
        let revision_receipt = match document.memory_revision_receipt(after_revision) {
            Ok(receipt) => receipt,
            Err(error) => {
                let reason = SessionQuarantineReason::RevisionReceiptRejected {
                    before_revision,
                    after_revision,
                    error: error.clone(),
                };
                drop(document);
                self.enter_quarantine(reason)?;
                return Err(SessionRuntimeError::RevisionReceiptRejected {
                    before_revision,
                    after_revision,
                    error: Box::new(error),
                });
            }
        };
        let window_generation = document.segment_generation();
        drop(document);
        self.publish_volatile_event_patch_best_effort(
            before_revision,
            &revision_receipt,
            window_generation,
        );
        Ok(SessionMutationReceipt {
            value,
            revision: revision_receipt.revision,
            changed_event_seqs: revision_receipt.changed_event_seqs,
            flush_reason,
        })
    }

    /// Applies one atomic mutation that may be idempotent, then makes the
    /// observed revision durable without fabricating a write.
    /// 原子应用一次可幂等修改，随后使观察到的修订号可靠落盘，且不伪造写入。
    ///
    /// # Parameters
    /// 参数
    /// `admission` selects the lifecycle gate, `flush_reason` identifies the
    /// semantic boundary, and `mutation` must directly map one failure-atomic
    /// Core method's returned `changed` value to `Changed` or `Unchanged` while
    /// the authoritative document lock remains held. Service callers must not
    /// pre-read or hand-author the change decision.
    /// `admission` 选择生命周期准入，`flush_reason` 标识语义边界，`mutation`
    /// 必须在权威文档锁保持持有时，把单个具备失败原子性的 Core 方法返回的
    /// `changed` 值直接映射为 `Changed` 或 `Unchanged`；服务调用方不得预读或
    /// 手工编写变化判定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the typed decision only after the observed revision is durable.
    /// Even an unchanged invocation flushes a pre-existing dirty revision. Any
    /// mismatch between the typed decision, error result, and revision effect
    /// quarantines the handle without publishing or persisting that mutation.
    /// 仅在观察到的修订号可靠落盘后返回强类型判定；即使本次未变化，也会
    /// flush 调用前已经存在的脏修订号。强类型判定、错误结果与修订效果之间
    /// 的任何不一致都会隔离句柄，且不发布、不持久化该修改。
    pub(crate) fn mutate_durable_if_changed<T>(
        &self,
        admission: SessionMutationAdmission,
        flush_reason: SessionFlushReason,
        mutation: impl FnOnce(&mut SessionDocument) -> Result<SessionMutationChange<T>, DomainError>,
    ) -> Result<SessionConditionalMutationReceipt<T>, SessionRuntimeError> {
        self.ensure_admitted(admission)?;
        // Serialize the changed decision and volatile edge, then release publication ownership
        // before disk I/O so later in-memory revisions can continue while the writer is blocked.
        // 串行变化判定与 volatile 边，然后在磁盘 I/O 前释放发布所有权，使 writer 阻塞时后续
        // 内存修订仍可继续增长。
        let receipt = {
            let _publisher = self
                .mutation_publisher
                .lock()
                .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
            self.mutate_in_memory_if_changed_with_publisher_locked(flush_reason, mutation)?
        };
        self.flush_through(receipt.revision)?;
        Ok(receipt)
    }

    /// Applies one conditional mutation while the caller owns publication serialization.
    /// 在调用方持有发布串行锁时应用一次条件式修改。
    ///
    /// # Parameters
    /// 参数
    /// `flush_reason` labels the future persistence boundary and `mutation`
    /// must report the Core-authored changed result for the same locked update.
    /// `flush_reason` 标记未来的持久化边界，`mutation` 必须为同一次锁内修改
    /// 返回 Core 产生的变化结果。
    ///
    /// # Returns
    /// 返回值
    /// Returns after validating revision effects and publishing any exact volatile patch.
    /// 在校验修订效果并发布任何精确 volatile patch 后返回。
    fn mutate_in_memory_if_changed_with_publisher_locked<T>(
        &self,
        flush_reason: SessionFlushReason,
        mutation: impl FnOnce(&mut SessionDocument) -> Result<SessionMutationChange<T>, DomainError>,
    ) -> Result<SessionConditionalMutationReceipt<T>, SessionRuntimeError> {
        self.ensure_not_quarantined()?;
        // Hold the authoritative document lock across the mutation and changed decision.
        // 在修改与变化判定期间持续持有权威文档锁。
        let mut document = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        let before_revision = document.memory_revision();
        let change_result = mutation(&mut document);
        let after_revision = document.memory_revision();
        let change = match change_result {
            Ok(change) => change,
            Err(error) if after_revision == before_revision => {
                return Err(SessionRuntimeError::from(error));
            }
            Err(error) => {
                let reason = SessionQuarantineReason::FailedMutationProducedRevision {
                    before_revision,
                    after_revision,
                    error: error.clone(),
                };
                drop(document);
                self.enter_quarantine(reason)?;
                return Err(SessionRuntimeError::FailedMutationProducedRevision {
                    before_revision,
                    after_revision,
                    error: Box::new(error),
                });
            }
        };
        // Preserve the domain value while retaining the explicit changed decision.
        // 保留领域返回值，同时保留显式变化判定。
        let (value, changed) = match change {
            SessionMutationChange::Changed(value) => (value, true),
            SessionMutationChange::Unchanged(value) => (value, false),
        };
        let revision_matches = if changed {
            before_revision.checked_add(1) == Some(after_revision)
        } else {
            before_revision == after_revision
        };
        if !revision_matches {
            let reason = SessionQuarantineReason::ConditionalMutationRevisionMismatch {
                reported_changed: changed,
                before_revision,
                after_revision,
            };
            drop(document);
            self.enter_quarantine(reason)?;
            return Err(SessionRuntimeError::ConditionalMutationRevisionMismatch {
                reported_changed: changed,
                before_revision,
                after_revision,
            });
        }
        // Capture one sparse receipt only for a real semantic revision.
        // 仅为真实语义修订捕获一份稀疏回执。
        let revision_receipt = if changed {
            match document.memory_revision_receipt(after_revision) {
                Ok(receipt) => Some(receipt),
                Err(error) => {
                    let reason = SessionQuarantineReason::RevisionReceiptRejected {
                        before_revision,
                        after_revision,
                        error: error.clone(),
                    };
                    drop(document);
                    self.enter_quarantine(reason)?;
                    return Err(SessionRuntimeError::RevisionReceiptRejected {
                        before_revision,
                        after_revision,
                        error: Box::new(error),
                    });
                }
            }
        } else {
            None
        };
        let window_generation = document.segment_generation();
        drop(document);
        if let Some(revision_receipt) = revision_receipt.as_ref() {
            self.publish_volatile_event_patch_best_effort(
                before_revision,
                revision_receipt,
                window_generation,
            );
        }
        Ok(SessionConditionalMutationReceipt {
            value,
            changed,
            revision: after_revision,
            changed_event_seqs: revision_receipt
                .map(|receipt| receipt.changed_event_seqs)
                .unwrap_or_default(),
            flush_reason,
        })
    }

    /// Runs the existing durable mutation path on Tokio's blocking pool.
    /// 在 Tokio 阻塞线程池上运行既有可靠修改路径。
    ///
    /// # Parameters
    /// 参数
    /// `admission` and `flush_reason` retain the exact synchronous mutation contract, while
    /// `mutation` owns the strongly typed document update executed by the blocking worker.
    /// `admission` 与 `flush_reason` 保留精确同步修改契约，`mutation` 持有由阻塞工作线程执行的强类型文档更新。
    ///
    /// # Returns
    /// 返回值
    /// Returns only after the same durable revision required by `mutate_durable` has published.
    /// 仅在 `mutate_durable` 所要求的同一可靠修订已经发布后返回。
    pub(crate) async fn mutate_durable_async<T, Mutation>(
        self: Arc<Self>,
        admission: SessionMutationAdmission,
        flush_reason: SessionFlushReason,
        mutation: Mutation,
    ) -> Result<SessionMutationReceipt<T>, SessionRuntimeError>
    where
        T: Send + 'static,
        Mutation: FnOnce(&mut SessionDocument) -> Result<T, DomainError> + Send + 'static,
    {
        tokio::task::spawn_blocking(move || self.mutate_durable(admission, flush_reason, mutation))
            .await
            .map_err(|error| SessionRuntimeError::BlockingTaskJoin(error.to_string()))?
    }

    /// Runs the conditional durable mutation path on Tokio's blocking pool.
    /// 在 Tokio 阻塞线程池上运行条件式可靠修改路径。
    ///
    /// # Parameters
    /// 参数
    /// `admission` and `flush_reason` preserve the synchronous owner contract, while `mutation`
    /// returns the Core-authored changed decision under the document lock.
    /// `admission` 与 `flush_reason` 保留同步 owner 契约，`mutation` 在文档锁内返回 Core
    /// 产生的变化判定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact conditional receipt after the observed revision is durable.
    /// 在所观察修订可靠落盘后返回精确条件式回执。
    pub(crate) async fn mutate_durable_if_changed_async<T, Mutation>(
        self: Arc<Self>,
        admission: SessionMutationAdmission,
        flush_reason: SessionFlushReason,
        mutation: Mutation,
    ) -> Result<SessionConditionalMutationReceipt<T>, SessionRuntimeError>
    where
        T: Send + 'static,
        Mutation: FnOnce(&mut SessionDocument) -> Result<SessionMutationChange<T>, DomainError>
            + Send
            + 'static,
    {
        tokio::task::spawn_blocking(move || {
            self.mutate_durable_if_changed(admission, flush_reason, mutation)
        })
        .await
        .map_err(|error| SessionRuntimeError::BlockingTaskJoin(error.to_string()))?
    }

    /// Flushes through one revision without retaining mutation publication ownership during I/O.
    /// 在 I/O 期间不保留修改发布所有权地 flush 到指定修订。
    ///
    /// # Parameters
    /// 参数
    /// `required_revision` is the minimum durable document revision required by the caller.
    /// `required_revision` 是调用方要求的最小可靠文档修订。
    ///
    /// # Returns
    /// 返回值
    /// Returns after the writer persisted at least the requested revision. Durable acknowledgements
    /// are serialized independently from the disk write.
    /// 在 writer 至少持久化请求修订后返回；durable 确认与磁盘写入分别串行化。
    fn flush_through(&self, required_revision: u64) -> Result<(), SessionRuntimeError> {
        self.ensure_not_quarantined()?;
        let _writer = self
            .writer
            .lock()
            .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
        self.ensure_not_quarantined()?;
        loop {
            // Acquire locks in writer -> publisher -> document order. Release the publisher before
            // physical I/O so another semantic mutation can publish while this candidate writes.
            // 按 writer -> publisher -> document 顺序加锁；在物理 I/O 前释放 publisher，使该
            // 候选写入期间另一语义修改仍可发布。
            let candidate = {
                let _publisher = self
                    .mutation_publisher
                    .lock()
                    .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
                self.capture_flush_candidate(required_revision)?
            };
            let Some((candidate, base_durable_revision)) = candidate else {
                return Ok(());
            };
            self.persist_flush_candidate_with_writer_locked(&candidate, base_durable_revision)?;
        }
    }

    /// Flushes through one revision while the caller owns writer and publication serialization.
    /// 在调用方拥有 writer 与发布串行权时 flush 到指定修订。
    ///
    /// # Parameters
    /// 参数
    /// `required_revision` is the minimum durable document revision required by the caller.
    /// `required_revision` 是调用方要求的最小可靠文档修订。
    ///
    /// # Returns
    /// 返回值
    /// Returns after the writer persisted at least the requested revision.
    /// 在 writer 至少持久化请求修订后返回。
    fn flush_through_with_writer_and_publisher_locked(
        &self,
        required_revision: u64,
    ) -> Result<(), SessionRuntimeError> {
        self.ensure_not_quarantined()?;
        loop {
            let Some((candidate, base_durable_revision)) =
                self.capture_flush_candidate(required_revision)?
            else {
                return Ok(());
            };
            self.persist_flush_candidate_with_writer_locked(&candidate, base_durable_revision)?;
        }
    }

    /// Captures the newest complete candidate while mutation publication is serialized.
    /// 在修改发布已串行化时捕获最新完整候选。
    ///
    /// # Parameters
    /// 参数
    /// `required_revision` is the minimum frontier that must become durable.
    /// `required_revision` 是必须可靠落盘的最小前沿。
    ///
    /// # Returns
    /// 返回值
    /// Returns `None` when the requested frontier is already durable, otherwise the exact newest
    /// candidate and its durable predecessor.
    /// 请求前沿已可靠时返回 `None`；否则返回精确最新候选及其可靠前驱。
    fn capture_flush_candidate(
        &self,
        required_revision: u64,
    ) -> Result<Option<(FlushCandidate, u64)>, SessionRuntimeError> {
        let mut document = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        if document.durable_revision() >= required_revision {
            return Ok(None);
        };
        let base_durable_revision = document.durable_revision();
        let candidate = document
            .prepare_flush_candidates()?
            .ok_or(SessionRuntimeError::MutationProducedNoRevision)?;
        Ok(Some((candidate, base_durable_revision)))
    }

    /// Clears transient failures covered by the complete candidate before one physical retry.
    /// 在一次物理重试前清除被完整候选覆盖的瞬态失败。
    ///
    /// # Parameters / 参数
    ///
    /// `candidate_revision` is the complete document revision about to be published.
    /// `candidate_revision` 是即将发布的完整文档修订号。
    ///
    /// # Returns / 返回值
    ///
    /// Returns after all failures at or before the covering revision are removed. A failed retry
    /// records the exact failure again before notifying waiters.
    /// 删除不晚于覆盖修订的全部失败后返回；重试若失败，会在通知等待者前重新记录精确失败。
    fn begin_continuation_dispatch_flush_attempt(
        &self,
        candidate_revision: u64,
    ) -> Result<(), SessionRuntimeError> {
        let mut state = self
            .continuation_dispatch_durability
            .lock()
            .map_err(|_| SessionRuntimeError::ContinuationDispatchStateLockPoisoned)?;
        state
            .persistence_failures
            .retain(|_, failure| failure.candidate_revision > candidate_revision);
        Ok(())
    }

    /// Finds exact Continue actions whose dispatch facts changed in one failed candidate.
    /// 查找一份失败候选中分派事实发生变化的精确 Continue 动作。
    ///
    /// # Parameters / 参数
    ///
    /// `candidate` supplies the exact changed seq set captured with the failed full state.
    /// `candidate` 提供与失败完整状态一同捕获的精确已变化 seq 集合。
    ///
    /// # Returns / 返回值
    ///
    /// Returns unique action ids derived from typed ApiStarted, ApiStats, and ExecutionControl
    /// facts. The full document is scanned only on the exceptional storage-failure path.
    /// 返回从强类型 ApiStarted、ApiStats 与 ExecutionControl 事实推导的唯一动作 id；完整文档
    /// 仅在异常存储失败路径扫描。
    fn continuation_actions_changed_by_candidate(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<Vec<ClientActionId>, SessionRuntimeError> {
        let changed_seqs = candidate
            .changed_event_seqs
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let document = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        let mut action_by_api = BTreeMap::new();
        for event in document.events() {
            let EventBody::ApiStarted(started) = &event.event else {
                continue;
            };
            if let Some(action_id) = &started.continuation_action_id {
                action_by_api.insert(started.api_call_id, action_id.clone());
            }
        }
        let mut actions = BTreeSet::new();
        for event in document.events() {
            if !changed_seqs.contains(&event.seq) {
                continue;
            }
            match &event.event {
                EventBody::ApiStarted(started) => {
                    if let Some(action_id) = &started.continuation_action_id {
                        actions.insert(action_id.clone());
                    }
                }
                EventBody::ApiStats(stats) => {
                    if let Some(action_id) = action_by_api.get(&stats.api_call_id) {
                        actions.insert(action_id.clone());
                    }
                }
                EventBody::ExecutionControl(control) => {
                    if let Some(action_id) = &control.client_action_id {
                        actions.insert(action_id.clone());
                    }
                    // Transport observations intentionally carry only the local API identity, so
                    // recover the owning Continue action from the immutable ApiStarted binding.
                    // 传输观察按契约只携带本地 API 身份，因此必须从不可变 ApiStarted 绑定恢复所属
                    // Continue 动作。
                    if let Some(api_call_id) = control.api_call_id
                        && let Some(action_id) = action_by_api.get(&api_call_id)
                    {
                        actions.insert(action_id.clone());
                    }
                }
                _ => {}
            }
        }
        Ok(actions.into_iter().collect())
    }

    /// Records one failed complete candidate for every affected Continue action.
    /// 为每个受影响 Continue 动作记录一份失败的完整候选。
    ///
    /// # Parameters / 参数
    ///
    /// `action_ids` are exact typed identities, `candidate_revision` is the failed revision, and
    /// `detail` is the stable storage diagnostic.
    /// `action_ids` 是精确强类型身份；`candidate_revision` 是失败修订；`detail` 是稳定存储诊断。
    ///
    /// # Returns / 返回值
    ///
    /// Returns after replacing any older failure for the same action.
    /// 替换同一动作的任何更早失败后返回。
    fn record_continuation_dispatch_persistence_failure(
        &self,
        action_ids: &[ClientActionId],
        candidate_revision: u64,
        detail: &str,
    ) -> Result<(), SessionRuntimeError> {
        let mut state = self
            .continuation_dispatch_durability
            .lock()
            .map_err(|_| SessionRuntimeError::ContinuationDispatchStateLockPoisoned)?;
        for action_id in action_ids {
            state.persistence_failures.insert(
                action_id.clone(),
                ContinuationDispatchPersistenceFailure {
                    candidate_revision,
                    detail: detail.to_owned(),
                },
            );
        }
        Ok(())
    }

    /// Advances the process-local durable Continue event frontier after physical publication.
    /// 在物理发布后推进进程内 Continue 可靠事件前沿。
    ///
    /// # Parameters / 参数
    ///
    /// `candidate` is the exact immutable state accepted by storage.
    /// `candidate` 是存储已接纳的精确不可变状态。
    ///
    /// # Returns / 返回值
    ///
    /// Returns after publishing the candidate's last active-tail seq when present.
    /// 候选存在活动尾段事件时，发布其最后 seq 后返回。
    fn advance_continuation_dispatch_durable_frontier(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<(), SessionRuntimeError> {
        let Some(through_seq) = candidate.tail_events.last().map(|event| event.seq) else {
            return Ok(());
        };
        let mut state = self
            .continuation_dispatch_durability
            .lock()
            .map_err(|_| SessionRuntimeError::ContinuationDispatchStateLockPoisoned)?;
        state.durable_through_seq = Some(through_seq);
        Ok(())
    }

    /// Persists one captured candidate while the caller owns the session writer.
    /// 在调用方拥有会话 writer 时持久化一份已捕获候选。
    ///
    /// # Parameters
    /// 参数
    /// `candidate` is the complete captured document state and `base_durable_revision` is its
    /// exact durable predecessor.
    /// `candidate` 是完整捕获的文档状态；`base_durable_revision` 是其精确可靠前驱。
    ///
    /// # Returns
    /// 返回值
    /// Returns after storage, durable frontier advancement, and best-effort live acknowledgement.
    /// 在存储、可靠前沿推进和最佳努力实时确认后返回。
    fn persist_flush_candidate_with_writer_locked(
        &self,
        candidate: &FlushCandidate,
        base_durable_revision: u64,
    ) -> Result<(), SessionRuntimeError> {
        self.begin_continuation_dispatch_flush_attempt(candidate.revision)?;
        let outcome = match self.backend.flush(candidate) {
            Ok(outcome) => outcome,
            Err(error) => {
                let action_ids = match self.continuation_actions_changed_by_candidate(candidate) {
                    Ok(action_ids) => action_ids,
                    Err(action_error) => {
                        self.continuation_dispatch_notify.notify_waiters();
                        return Err(action_error);
                    }
                };
                let record_result = self.record_continuation_dispatch_persistence_failure(
                    &action_ids,
                    candidate.revision,
                    &error.to_string(),
                );
                // A failed publication is not a durable settlement, but it is a terminal
                // process-local outcome for every waiter owned by this failed attempt. Wake them
                // so they can return the exact persistence failure and release the session gate.
                // 发布失败不是可靠结算，但它是本次失败尝试所拥有的每个等待者的进程内终态；唤醒
                // 它们，使其返回精确持久化失败并释放会话门。
                self.continuation_dispatch_notify.notify_waiters();
                record_result?;
                return Err(error.into());
            }
        };
        let post_commit_result: Result<u64, SessionRuntimeError> = (|| {
            let mut document = self
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
            if let Some(published) = &outcome.published_metadata {
                document.apply_published_storage_metadata(published)?;
            }
            document.advance_durable(candidate.revision)?;
            let window_generation = document.segment_generation();
            drop(document);
            self.advance_continuation_dispatch_durable_frontier(candidate)?;
            Ok(window_generation)
        })();
        let window_generation = match post_commit_result {
            Ok(window_generation) => window_generation,
            Err(error) => {
                // Storage has already committed the candidate, so continuing with an unconfirmed
                // resident frontier would create two authorities. Quarantine the owner and wake
                // every Continue waiter instead of retrying or exposing memory as durable state.
                // 存储已经提交候选，若继续使用未确认的常驻前沿就会产生两个权威。必须隔离 owner
                // 并唤醒全部 Continue 等待者，而不是继续重试或把内存状态冒充为可靠状态。
                let reason = SessionQuarantineReason::DurableCommitApplicationRejected {
                    candidate_revision: candidate.revision,
                    detail: error.to_string(),
                };
                self.enter_quarantine(reason)?;
                return Err(error);
            }
        };
        // Continue settlement is a condition over the durable ledger, so every successful durable
        // frontier advancement must wake waiters. Restricting this signal to transport observation
        // loses the terminal-before-transport edge and can park the detached owner forever.
        // Continue 结算是可靠账本上的条件，因此每次可靠前沿推进成功都必须唤醒等待者；若只在传输
        // 观测时发信号，就会漏掉“传输前终止”边，并可能让分离 owner 永久停驻。
        self.continuation_dispatch_notify.notify_waiters();
        self.publish_durable_event_ack_best_effort(
            candidate,
            window_generation,
            base_durable_revision,
        );
        Ok(())
    }

    /// Publishes one exact in-memory revision patch without changing durability state.
    /// 发布一个精确内存修订补丁，且不改变持久化状态。
    fn publish_volatile_event_patch_best_effort(
        &self,
        base_memory_revision: u64,
        receipt: &MemoryRevisionReceipt,
        window_generation: u64,
    ) {
        let Some(service_epoch) = self.live_service_epoch_best_effort() else {
            return;
        };
        let batch = ConversationEventWindowBatch::from_events(
            self.session_id.to_string(),
            window_generation,
            &receipt.changed_events,
        );
        match ConversationEventLiveDeltaV1::volatile(
            batch,
            service_epoch,
            base_memory_revision,
            receipt.revision,
        ) {
            Ok(delta) => self.publish_live_event_delta_best_effort(delta),
            Err(error) => eprintln!(
                "[session-v1-live] rejected invalid volatile revision edge: session={} error={error}",
                self.session_id
            ),
        }
    }

    /// Emits one live delta without allowing a presentation failure to roll back domain state.
    /// 发出一份实时增量，且绝不允许显示层失败回滚领域状态。
    fn publish_live_event_delta_best_effort(&self, delta: ConversationEventLiveDeltaV1) {
        let publisher = match self.live_event_publisher.lock() {
            Ok(publisher) => publisher.clone(),
            Err(_) => {
                eprintln!(
                    "[session-v1-live] publisher lock poisoned after domain mutation: session={}",
                    self.session_id
                );
                return;
            }
        };
        let Some(publisher) = publisher else {
            return;
        };
        if delta.service_epoch != publisher.service_epoch {
            eprintln!(
                "[session-v1-live] rejected stale service epoch: session={} expected={} actual={}",
                self.session_id, publisher.service_epoch, delta.service_epoch
            );
            return;
        }
        publisher
            .emitter
            .emit(ServiceEvent::ConversationEventDeltaV1(Box::new(delta)));
    }

    /// Reads the service epoch bound atomically with the current live publisher.
    /// 读取与当前实时发布器原子绑定的服务代次。
    fn live_service_epoch_best_effort(&self) -> Option<String> {
        match self.live_event_publisher.lock() {
            Ok(publisher) => publisher
                .as_ref()
                .map(|publisher| publisher.service_epoch.clone()),
            Err(_) => {
                eprintln!(
                    "[session-v1-live] publisher lock poisoned while reading service epoch: session={}",
                    self.session_id
                );
                None
            }
        }
    }

    /// Publishes one generation-rebase signal after storage and the resident owner agree.
    /// 在存储与常驻 owner 一致后发布一次 generation rebase 信号。
    ///
    /// The empty durable edge intentionally carries the new generation. WebUI must rehydrate that
    /// generation before consuming further sparse revision patches; presentation failure never
    /// rolls back the already committed domain generation.
    /// 空 durable 边有意携带新的 generation。WebUI 必须先重新水合该 generation，之后才能消费后续
    /// 稀疏修订补丁；显示层发布失败绝不回滚已经提交的领域 generation。
    fn publish_generation_rebase_best_effort(
        &self,
        base_durable_revision: u64,
        durable_revision: u64,
        window_generation: u64,
    ) {
        let Some(service_epoch) = self.live_service_epoch_best_effort() else {
            return;
        };
        let delta = ConversationEventLiveDeltaV1::durable_ack(
            self.session_id.to_string(),
            window_generation,
            service_epoch,
            base_durable_revision,
            durable_revision,
        );
        match delta {
            Ok(delta) => self.publish_live_event_delta_best_effort(delta),
            Err(error) => eprintln!(
                "[session-v1-live] rejected generation rebase edge: session={} generation={} error={error}",
                self.session_id, window_generation
            ),
        }
    }

    /// Publishes an event-free durable acknowledgement for one committed revision.
    /// 为一个已提交修订发布不含事件的 durable 确认。
    ///
    /// # Parameters
    /// 参数
    /// `candidate` is the exact candidate accepted by storage and
    /// `window_generation` is the generation after published metadata is applied.
    /// `candidate` 是存储已接受的精确候选，`window_generation` 是已应用发布
    /// metadata 后的代次。
    ///
    /// # Returns
    /// 返回值
    /// Returns after the best-effort publisher was attempted. Publisher failure
    /// is diagnosed and never changes the already durable domain result.
    /// 尝试最佳努力发布后返回；发布器失败会被诊断，且绝不改变已经持久化的领域结果。
    fn publish_durable_event_ack_best_effort(
        &self,
        candidate: &FlushCandidate,
        window_generation: u64,
        base_durable_revision: u64,
    ) {
        let Some(service_epoch) = self.live_service_epoch_best_effort() else {
            return;
        };
        let delta = ConversationEventLiveDeltaV1::durable_ack(
            self.session_id.to_string(),
            window_generation,
            service_epoch,
            base_durable_revision,
            candidate.revision,
        );
        match delta {
            Ok(delta) => self.publish_live_event_delta_best_effort(delta),
            Err(error) => eprintln!(
                "[session-v1-live] rejected invalid durable revision edge: session={} error={error}",
                self.session_id
            ),
        }
    }

    /// Flushes every dirty revision currently owned by this session.
    /// flush 当前会话拥有的全部脏修订。
    pub(crate) fn flush_all(&self) -> Result<(), SessionRuntimeError> {
        self.ensure_not_quarantined()?;
        let target = self.read(SessionDocument::memory_revision)?;
        let durable = self.read(SessionDocument::durable_revision)?;
        if durable >= target {
            return Ok(());
        }
        self.flush_through(target)
    }

    /// Closes new-root admission while admitted work settles terminal facts.
    /// 关闭新根回合准入，同时允许已接纳工作结算终态事实。
    pub(crate) fn begin_shutdown(&self) -> Result<(), SessionRuntimeError> {
        let mut shutting_down = self
            .shutting_down
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        *shutting_down = true;
        Ok(())
    }

    /// Reopens root admission after the host cancels a failed shutdown.
    /// 宿主取消失败关闭后重新开放根回合准入。
    pub(crate) fn resume_after_failed_shutdown(&self) -> Result<(), SessionRuntimeError> {
        let mut shutting_down = self
            .shutting_down
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        *shutting_down = false;
        Ok(())
    }

    /// Verifies one mutation remains admissible at the current shutdown phase.
    /// 校验一次修改在当前关闭阶段仍可准入。
    fn ensure_admitted(
        &self,
        admission: SessionMutationAdmission,
    ) -> Result<(), SessionRuntimeError> {
        self.ensure_not_quarantined()?;
        let shutting_down = self
            .shutting_down
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        if *shutting_down && admission == SessionMutationAdmission::RootTurn {
            return Err(SessionRuntimeError::ShuttingDown);
        }
        Ok(())
    }

    /// Rejects every write-side operation after the handle entered quarantine.
    /// 在句柄进入隔离状态后拒绝所有写侧操作。
    ///
    /// # Returns
    /// 返回值
    /// Returns `Ok` only while no domain contract violation has contaminated
    /// process-local memory; otherwise returns the first stable quarantine reason.
    /// 仅当进程内内存尚未被领域契约违反污染时返回 `Ok`；否则返回首个稳定隔离原因。
    fn ensure_not_quarantined(&self) -> Result<(), SessionRuntimeError> {
        let quarantine = self
            .quarantine
            .lock()
            .map_err(|_| SessionRuntimeError::QuarantineLockPoisoned)?;
        match quarantine.as_ref() {
            Some(reason) => Err(SessionRuntimeError::SessionQuarantined {
                session_id: self.session_id,
                reason: reason.clone(),
            }),
            None => Ok(()),
        }
    }

    /// Permanently quarantines this process-local owner without publishing its dirty memory.
    /// 永久隔离当前进程内所有者，且不发布其脏内存。
    ///
    /// # Parameters
    /// 参数
    /// `reason` is the first observed Core mutation contract violation.
    /// `reason` 是首次观察到的 Core 修改契约违反。
    ///
    /// # Returns
    /// 返回值
    /// Returns after preserving the first reason; repeated calls never replace it.
    /// 保留首个原因后返回；重复调用绝不会替换该原因。
    fn enter_quarantine(&self, reason: SessionQuarantineReason) -> Result<(), SessionRuntimeError> {
        let mut quarantine = self
            .quarantine
            .lock()
            .map_err(|_| SessionRuntimeError::QuarantineLockPoisoned)?;
        let newly_quarantined = quarantine.is_none();
        if newly_quarantined {
            *quarantine = Some(reason);
        }
        drop(quarantine);
        if newly_quarantined {
            // Quarantine permanently ends every in-flight write-side owner. A registered Continue
            // waiter must observe that state transition instead of retaining the session gate.
            // 隔离会永久终止每个在途写侧 owner；已注册的 Continue 等待者必须观察该状态变化，
            // 而不能继续持有会话门。
            self.continuation_dispatch_notify.notify_waiters();
        }
        Ok(())
    }
}

/// Process-local registry that owns exactly one handle per active V1 session.
/// 进程内注册表，为每个活动 V1 会话精确拥有一个句柄。
pub(crate) struct SessionRuntimeRegistry {
    /// Stable identity regenerated once for every service process.
    /// 每个服务进程只重新生成一次的稳定身份。
    service_epoch: String,
    /// Process-local registry state protected independently of service state.
    /// 独立于服务状态保护的进程内注册表状态。
    state: Mutex<SessionRuntimeRegistryState>,
}

/// Mutable registry state serialized by the registry-owned mutex.
/// 由注册表自有互斥锁串行化的可变注册表状态。
#[derive(Default)]
struct SessionRuntimeRegistryState {
    /// Registered handles keyed by stable application session id.
    /// 按稳定应用会话 id 索引的已注册句柄。
    handles: BTreeMap<SessionId, Arc<SessionHandle>>,
    /// Exact sessions currently owned by one physical loader or creator.
    /// 当前由一个物理加载器或创建器拥有的精确会话。
    loading: BTreeMap<SessionId, Arc<SessionLoadGate>>,
    /// Per-session lifecycle serializers retained independently of loaded handles.
    /// 独立于已加载句柄保留的单会话生命周期串行器。
    lifecycle_gates: BTreeMap<SessionId, Arc<AsyncMutex<()>>>,
    /// Whether registry-wide root admission is closed.
    /// 注册表级根回合准入是否关闭。
    shutting_down: bool,
    /// Publisher inherited by every current and future registered handle.
    /// 由当前及未来每个已注册句柄继承的发布器。
    live_event_emitter: Option<ServiceEventEmitter>,
}

/// Wait gate shared by callers racing to load one exact session.
/// 竞争加载同一精确会话的调用方共享的等待门。
#[derive(Default)]
struct SessionLoadGate {
    /// Configuration-only reservations publish no handle; waiting loaders must retry admission.
    /// 纯配置预留不发布句柄；等待的加载器必须重新申请准入。
    retry_after_completion: bool,
    /// Whether the unique loader has finished its publication attempt.
    /// 唯一加载器是否已完成发布尝试。
    completed: Mutex<bool>,
    /// Notification emitted after the publication attempt finishes.
    /// 发布尝试结束后发出的通知。
    changed: Condvar,
}

/// Exclusive per-session admission held across asynchronous V1 creation.
/// 跨异步 V1 创建过程持有的单会话独占准入。
pub(crate) struct SessionCreationReservation<'registry> {
    /// Registry that owns the exact initialization slot.
    /// 拥有精确初始化槽的注册表。
    registry: &'registry SessionRuntimeRegistry,
    /// Session identity protected by this reservation.
    /// 由该预留保护的会话身份。
    session_id: SessionId,
    /// Shared gate reused by concurrent exact loaders.
    /// 并发精确加载器复用的共享门。
    gate: Arc<SessionLoadGate>,
    /// Whether publication already consumed this reservation.
    /// 发布是否已经消费该预留。
    published: bool,
}

impl SessionCreationReservation<'_> {
    /// Atomically replaces this reservation with one fully published handle.
    /// 用一个完整发布的句柄原子替换该预留。
    pub(crate) fn publish(
        mut self,
        handle: Arc<SessionHandle>,
    ) -> Result<Arc<SessionHandle>, SessionRuntimeError> {
        let actual = handle.session_id();
        if actual != self.session_id {
            return Err(SessionRuntimeError::LoadedSessionIdentityMismatch {
                expected: self.session_id,
                actual,
            });
        }
        let mut state = self
            .registry
            .state
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
        let owns_slot = state
            .loading
            .get(&self.session_id)
            .is_some_and(|gate| Arc::ptr_eq(gate, &self.gate));
        if !owns_slot {
            return Err(SessionRuntimeError::SessionRegistrationInProgress(
                self.session_id,
            ));
        }
        if state.handles.contains_key(&self.session_id) {
            return Err(SessionRuntimeError::DuplicateSession(self.session_id));
        }
        let mut completed = self
            .gate
            .completed
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
        if let Some(emitter) = state.live_event_emitter.clone() {
            handle.install_live_event_publisher(emitter, &self.registry.service_epoch)?;
        }
        if state.shutting_down {
            handle.begin_shutdown()?;
        }
        state.loading.remove(&self.session_id);
        state.handles.insert(self.session_id, Arc::clone(&handle));
        *completed = true;
        self.gate.changed.notify_all();
        self.published = true;
        Ok(handle)
    }
}

impl Drop for SessionCreationReservation<'_> {
    /// Releases a failed or cancelled creation slot without publishing state.
    /// 释放失败或取消的创建槽，不发布任何状态。
    fn drop(&mut self) {
        if self.published {
            return;
        }
        if let Ok(mut state) = self.registry.state.lock() {
            let owns_slot = state
                .loading
                .get(&self.session_id)
                .is_some_and(|gate| Arc::ptr_eq(gate, &self.gate));
            if owns_slot {
                state.loading.remove(&self.session_id);
            }
        }
        if let Ok(mut completed) = self.gate.completed.lock() {
            *completed = true;
            self.gate.changed.notify_all();
        }
    }
}

impl Default for SessionRuntimeRegistry {
    /// Builds one empty registry with root admission open.
    /// 构建一个根回合准入开放的空注册表。
    fn default() -> Self {
        Self {
            service_epoch: Uuid::new_v4().to_string(),
            state: Mutex::new(SessionRuntimeRegistryState::default()),
        }
    }
}

impl SessionRuntimeRegistry {
    /// Returns the stable epoch shared by hydration, live pushes, and reconnect.
    /// 返回 hydration、实时推送与重连共享的稳定代次。
    pub(crate) fn service_epoch(&self) -> &str {
        &self.service_epoch
    }

    /// Acquires the unique process-local lifecycle transaction slot for one session.
    /// 获取一个会话唯一的进程内生命周期事务槽。
    ///
    /// # Parameters / 参数
    ///
    /// - `session_id`: Exact session whose metadata and SQLite projection will change.
    ///   `session_id`：即将修改 metadata 与 SQLite 投影的精确会话。
    ///
    /// # Returns / 返回值
    ///
    /// An owned guard that may cross asynchronous SQLite calls and never blocks unrelated sessions.
    /// 一个可跨异步 SQLite 调用且绝不阻塞无关会话的所有权守卫。
    pub(crate) async fn acquire_lifecycle_transition(
        &self,
        session_id: SessionId,
    ) -> Result<SessionLifecycleTransitionGuard, SessionRuntimeError> {
        let gate = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
            Arc::clone(
                state
                    .lifecycle_gates
                    .entry(session_id)
                    .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
            )
        };
        Ok(SessionLifecycleTransitionGuard {
            _guard: gate.lock_owned().await,
        })
    }

    /// Removes one registered root owner after an archive or delete projection commits.
    /// 在归档或删除投影提交后移除一个已注册根 owner。
    ///
    /// # Parameters / 参数
    ///
    /// - `session_id`: Exact root session no longer eligible for active dispatch.
    ///   `session_id`：不再具备活动分派资格的精确根会话。
    ///
    /// # Returns / 返回值
    ///
    /// The removed handle when present; it first closes root admission so stale references cannot
    /// create new Turns and the lifecycle caller can perform one final flush before deletion.
    /// 存在时返回已移除句柄；它会先关闭根准入，避免陈旧引用创建新回合，并允许生命周期
    /// 调用方在删除前执行一次最终 flush。
    pub(crate) fn unregister_lifecycle_session(
        &self,
        session_id: SessionId,
    ) -> Result<Option<Arc<SessionHandle>>, SessionRuntimeError> {
        let removed = self
            .state
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?
            .handles
            .remove(&session_id);
        if let Some(handle) = removed.as_ref() {
            handle.begin_shutdown()?;
        }
        Ok(removed)
    }

    /// Installs one publisher for every current and future V1 session handle.
    /// 为当前及未来每个 V1 会话句柄安装同一个发布器。
    ///
    /// # Parameters
    /// 参数
    /// `emitter` is the production service-event multiplexer owned by the host.
    /// `emitter` 是宿主持有的生产服务事件复用器。
    ///
    /// # Returns
    /// 返回值
    /// Returns after the registry and every currently registered handle share
    /// the same publisher.
    /// 在注册表与每个当前已注册句柄共享同一发布器后返回。
    pub(crate) fn install_live_event_emitter(
        &self,
        emitter: ServiceEventEmitter,
    ) -> Result<(), SessionRuntimeError> {
        let handles = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
            state.live_event_emitter = Some(emitter.clone());
            state.handles.values().cloned().collect::<Vec<_>>()
        };
        for handle in handles {
            handle.install_live_event_publisher(emitter.clone(), &self.service_epoch)?;
        }
        Ok(())
    }

    /// Reserves the exact per-session initialization slot before any file I/O.
    /// 在任何文件 I/O 前预留精确的单会话初始化槽。
    pub(crate) fn reserve_creation(
        &self,
        session_id: SessionId,
    ) -> Result<SessionCreationReservation<'_>, SessionRuntimeError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
        if state.shutting_down {
            return Err(SessionRuntimeError::ShuttingDown);
        }
        if state.handles.contains_key(&session_id) {
            return Err(SessionRuntimeError::DuplicateSession(session_id));
        }
        if state.loading.contains_key(&session_id) {
            return Err(SessionRuntimeError::SessionRegistrationInProgress(
                session_id,
            ));
        }
        let gate = Arc::new(SessionLoadGate::default());
        state.loading.insert(session_id, Arc::clone(&gate));
        Ok(SessionCreationReservation {
            registry: self,
            session_id,
            gate,
            published: false,
        })
    }

    /// Registers an already-published and indexed session handle exactly once.
    /// 精确注册一次已经发布并建立索引的会话句柄。
    #[cfg(test)]
    pub(crate) fn register(&self, handle: Arc<SessionHandle>) -> Result<(), SessionRuntimeError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
        if state.shutting_down {
            return Err(SessionRuntimeError::ShuttingDown);
        }
        let session_id = handle.session_id();
        if state.loading.contains_key(&session_id) {
            return Err(SessionRuntimeError::SessionRegistrationInProgress(
                session_id,
            ));
        }
        if state.handles.contains_key(&session_id) {
            return Err(SessionRuntimeError::DuplicateSession(session_id));
        }
        if let Some(emitter) = state.live_event_emitter.clone() {
            handle.install_live_event_publisher(emitter, &self.service_epoch)?;
        }
        state.handles.insert(session_id, handle);
        Ok(())
    }

    /// Returns the sole handle for one registered session.
    /// 返回一个已注册会话的唯一句柄。
    pub(crate) fn get(
        &self,
        session_id: SessionId,
    ) -> Result<Arc<SessionHandle>, SessionRuntimeError> {
        self.state
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?
            .handles
            .get(&session_id)
            .cloned()
            .ok_or(SessionRuntimeError::SessionNotRegistered(session_id))
    }

    /// Returns the unique handle or performs exactly one physical load for all
    /// callers racing on the same session id.
    /// 返回唯一句柄，或为竞争同一会话 id 的全部调用方只执行一次物理加载。
    pub(crate) fn get_or_load(
        &self,
        session_id: SessionId,
        loader: impl FnOnce() -> Result<Arc<SessionHandle>, SessionRuntimeError>,
    ) -> Result<Arc<SessionHandle>, SessionRuntimeError> {
        let gate = loop {
            let (gate, owns_load) = {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
                if let Some(handle) = state.handles.get(&session_id) {
                    return Ok(Arc::clone(handle));
                }
                if state.shutting_down {
                    return Err(SessionRuntimeError::ShuttingDown);
                }
                match state.loading.get(&session_id) {
                    Some(gate) => (Arc::clone(gate), false),
                    None => {
                        let gate = Arc::new(SessionLoadGate::default());
                        state.loading.insert(session_id, Arc::clone(&gate));
                        (gate, true)
                    }
                }
            };

            if owns_load {
                break gate;
            }
            let mut completed = gate
                .completed
                .lock()
                .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
            while !*completed {
                completed = gate
                    .changed
                    .wait(completed)
                    .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
            }
            drop(completed);
            if gate.retry_after_completion {
                continue;
            }
            return self
                .get(session_id)
                .map_err(|_| SessionRuntimeError::ConcurrentSessionLoadFailed(session_id));
        };

        let loaded = loader().and_then(|handle| {
            let actual = handle.session_id();
            if actual != session_id {
                return Err(SessionRuntimeError::LoadedSessionIdentityMismatch {
                    expected: session_id,
                    actual,
                });
            }
            Ok(handle)
        });
        let publication = (|| {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
            state.loading.remove(&session_id);
            if let Ok(handle) = &loaded {
                if let Some(emitter) = state.live_event_emitter.clone() {
                    handle.install_live_event_publisher(emitter, &self.service_epoch)?;
                }
                if state.shutting_down {
                    handle.begin_shutdown()?;
                }
                state.handles.insert(session_id, Arc::clone(handle));
            }
            Ok::<(), SessionRuntimeError>(())
        })();
        let mut completed = gate
            .completed
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
        *completed = true;
        gate.changed.notify_all();
        drop(completed);
        publication.and(loaded)
    }

    /// Executes configuration access through the resident owner or an exclusive unloaded slot.
    /// 通过常驻 owner 或独占的未加载槽执行配置访问。
    /// `session_id` selects one exact owner; `resident` and `unloaded` return the same result type.
    /// `session_id` 选择精确 owner；`resident` 与 `unloaded` 返回相同结果类型。
    /// Returns the callback result without invoking a loader or propagating Turn restoration errors.
    /// 返回回调结果，不调用加载器，也不传播回合恢复错误。
    pub(crate) fn with_configuration_access<T>(
        &self,
        session_id: SessionId,
        resident: impl FnOnce(&SessionHandle) -> Result<T, String>,
        unloaded: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        loop {
            // Admission shares the creation/load map so a cold write cannot race a new owner.
            // 准入共用创建/加载映射，确保冷写入不会与新 owner 竞争。
            let mut state = self
                .state
                .lock()
                .map_err(|_| SessionRuntimeError::RegistryLockPoisoned.to_string())?;
            if state.shutting_down {
                return Err(SessionRuntimeError::ShuttingDown.to_string());
            }
            if let Some(handle) = state.handles.get(&session_id).cloned() {
                drop(state);
                return resident(&handle);
            }
            if let Some(gate) = state.loading.get(&session_id).cloned() {
                drop(state);
                // Failed runtime loads do not invalidate independent configuration metadata.
                // 运行时加载失败并不使独立配置元数据失效。
                let mut completed = gate
                    .completed
                    .lock()
                    .map_err(|_| SessionRuntimeError::RegistryLockPoisoned.to_string())?;
                while !*completed {
                    completed = gate
                        .changed
                        .wait(completed)
                        .map_err(|_| SessionRuntimeError::RegistryLockPoisoned.to_string())?;
                }
                continue;
            }
            // The RAII reservation wakes waiters even if configuration I/O fails.
            // 即使配置 I/O 失败，RAII 预留也会唤醒等待方。
            let gate = Arc::new(SessionLoadGate {
                retry_after_completion: true,
                ..Default::default()
            });
            state.loading.insert(session_id, Arc::clone(&gate));
            let reservation = SessionCreationReservation {
                registry: self,
                session_id,
                gate,
                published: false,
            };
            drop(state);
            let result = unloaded();
            drop(reservation);
            return result;
        }
    }

    /// Closes root admission and returns a stable handle snapshot for shutdown.
    /// 关闭根回合准入，并返回供关闭流程使用的稳定句柄快照。
    pub(crate) fn begin_shutdown(&self) -> Result<Vec<Arc<SessionHandle>>, SessionRuntimeError> {
        let handles = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
            state.shutting_down = true;
            state.handles.values().cloned().collect::<Vec<_>>()
        };
        for handle in &handles {
            handle.begin_shutdown()?;
        }
        Ok(handles)
    }

    /// Reopens root admission after a failed host shutdown.
    /// 宿主关闭失败后重新开放根回合准入。
    pub(crate) fn resume_after_failed_shutdown(&self) -> Result<(), SessionRuntimeError> {
        let handles = self
            .state
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?
            .handles
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for handle in handles {
            handle.resume_after_failed_shutdown()?;
        }
        self.state
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?
            .shutting_down = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    mod request_preparation;
    use super::*;
    use std::path::PathBuf;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread;

    use vulcan_service_api::ServiceEventSink;
    use vulcan_session_core::v1::config::{ActiveRequestConfig, PendingRequestConfig};
    use vulcan_session_core::v1::context::LockedBaselineProjection;
    use vulcan_session_core::v1::document::ApiCompletion;
    use vulcan_session_core::v1::event::{
        ApiCallStatus, ApiEndReason, ApiErrorCategory, ApiPurposeKind, ApiRequestSnapshot,
        ApiTimings, ContinuationEligibility, DisplayNoticeKind, EventBody, NoticeSeverity,
        ProviderProjectionProof, RedactedApiError, ToolCallStatus,
    };
    use vulcan_session_core::v1::ids::{ClientActionId, RequestStreamId};
    use vulcan_session_core::v1::projection::provider_visible_fingerprint;
    use vulcan_session_core::v1::stats::UsageSnapshot;
    use vulcan_workspace_store::workspace_store::SessionV1StorageError;

    /// In-memory sink that captures the exact service events delivered by the publisher.
    /// 捕获发布器投递的精确服务事件的内存 sink。
    #[derive(Default)]
    struct RecordingEventSink {
        /// Service events in synchronous delivery order.
        /// 按同步投递顺序保存的服务事件。
        events: Mutex<Vec<ServiceEvent>>,
    }

    impl RecordingEventSink {
        /// Returns one defensive copy of all delivered service events.
        /// 返回全部已投递服务事件的一份防御性副本。
        fn events(&self) -> Vec<ServiceEvent> {
            self.events.lock().unwrap().clone()
        }

        /// Removes all previously delivered events while preserving the installed sink.
        /// 清除此前投递的全部事件，同时保留已经安装的 sink。
        fn clear(&self) {
            self.events.lock().unwrap().clear();
        }
    }

    impl ServiceEventSink for RecordingEventSink {
        /// Captures one delivered service event without transforming its payload.
        /// 在不转换载荷的情况下捕获一条已投递服务事件。
        fn emit_service_event(&self, event: ServiceEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    /// In-memory backend that captures the latest complete candidate.
    /// 捕获最新完整候选的内存后端。
    #[derive(Default)]
    struct RecordingPersistenceBackend {
        /// Latest candidate accepted as durable.
        /// 最近一次被接纳为可靠状态的候选。
        latest: Mutex<Option<FlushCandidate>>,
        /// Number of physical flush calls accepted by this backend.
        /// 该后端接纳的物理 flush 调用次数。
        flush_count: AtomicUsize,
    }

    impl RecordingPersistenceBackend {
        /// Returns the exact event tail retained by the latest durable candidate.
        /// 返回最近可靠候选保留的精确事件尾段。
        fn persisted_events(&self) -> Vec<vulcan_session_core::v1::event::ConversationEvent> {
            self.latest
                .lock()
                .unwrap()
                .as_ref()
                .map(|candidate| candidate.tail_events.clone())
                .unwrap_or_default()
        }

        /// Returns the number of physical flushes observed by the backend.
        /// 返回后端观察到的物理 flush 次数。
        fn flush_count(&self) -> usize {
            self.flush_count.load(Ordering::SeqCst)
        }
    }

    impl SessionPersistenceBackend for RecordingPersistenceBackend {
        /// Captures one immutable candidate and publishes its exact metadata.
        /// 捕获一份不可变候选并发布其精确 metadata。
        fn flush(
            &self,
            candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            self.flush_count.fetch_add(1, Ordering::SeqCst);
            *self.latest.lock().unwrap() = Some(candidate.clone());
            Ok(SessionV1FlushResult {
                written_file_count: candidate.dirty.len(),
                published_metadata: Some(candidate.metadata.clone()),
            })
        }
    }

    /// Backend that fails a fixed number of calls before delegating to disk.
    /// 在委托磁盘写入前固定失败若干次的后端。
    struct FailThenPersistBackend {
        /// Remaining injected failures.
        /// 剩余注入失败次数。
        remaining_failures: AtomicUsize,
        /// Recording backend used after failures are exhausted.
        /// 失败耗尽后使用的记录后端。
        delegate: Arc<RecordingPersistenceBackend>,
    }

    impl SessionPersistenceBackend for FailThenPersistBackend {
        /// Fails deterministically, then writes the latest complete candidate.
        /// 确定性失败，随后写入最新完整候选。
        fn flush(
            &self,
            candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            let remaining = self.remaining_failures.load(Ordering::SeqCst);
            if remaining > 0 {
                self.remaining_failures.fetch_sub(1, Ordering::SeqCst);
                return Err(SessionV1StoreError::Storage(SessionV1StorageError::io(
                    PathBuf::from("injected-session-v1.json"),
                    "injected registry writer failure",
                )));
            }
            self.delegate.flush(candidate)
        }
    }

    /// Backend that commits the candidate but corrupts one returned publication receipt.
    /// 提交候选后破坏一次返回发布回执的后端。
    struct CorruptPublishedMetadataBackend {
        /// Whether the next successful publication receipt must carry a foreign session id.
        /// 下一份成功发布回执是否必须携带外部会话 id。
        corrupt_next: AtomicBool,
        /// Recording backend that proves the candidate was physically accepted first.
        /// 用于证明候选已经先被物理接纳的记录后端。
        delegate: Arc<RecordingPersistenceBackend>,
    }

    impl SessionPersistenceBackend for CorruptPublishedMetadataBackend {
        /// Persists first, then injects one impossible post-commit metadata identity.
        /// 先持久化，再注入一份不可能成立的提交后 metadata 身份。
        fn flush(
            &self,
            candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            let mut outcome = self.delegate.flush(candidate)?;
            if self.corrupt_next.swap(false, Ordering::SeqCst)
                && let Some(metadata) = outcome.published_metadata.as_mut()
            {
                metadata.session_id = String::from("foreign-session");
            }
            Ok(outcome)
        }
    }

    /// Backend that pauses its first candidate after capture and before publication.
    /// 在首份候选捕获后、发布前暂停的后端。
    struct BlockingPersistBackend {
        /// Shared `(entered, released)` state for deterministic coordination.
        /// 用于确定性协调的共享 `(已进入, 已释放)` 状态。
        gate: Arc<(Mutex<(bool, bool)>, Condvar)>,
        /// Recording backend used after the test releases publication.
        /// 测试释放发布后使用的记录后端。
        delegate: Arc<RecordingPersistenceBackend>,
    }

    impl SessionPersistenceBackend for BlockingPersistBackend {
        /// Blocks publication without retaining the session document mutex.
        /// 在不持有会话文档互斥锁的情况下阻塞发布。
        fn flush(
            &self,
            candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            let (state, changed) = &*self.gate;
            let mut state = state.lock().unwrap();
            state.0 = true;
            changed.notify_all();
            while !state.1 {
                state = changed.wait(state).unwrap();
            }
            drop(state);
            self.delegate.flush(candidate)
        }
    }

    /// Creates one empty V1 session around an explicit deterministic backend.
    /// 围绕显式确定性后端创建一个空 V1 会话。
    fn published_handle(backend: Arc<dyn SessionPersistenceBackend>) -> Arc<SessionHandle> {
        let session_id = SessionId::new_v4();
        let document = SessionDocument::new(session_id.to_string(), "project-1", "title", 1);
        SessionHandle::with_backend(session_id, document, backend)
    }

    /// Creates one recording backend and its session handle.
    /// 创建一个记录后端及其会话句柄。
    fn recording_handle() -> (Arc<RecordingPersistenceBackend>, Arc<SessionHandle>) {
        let backend = Arc::new(RecordingPersistenceBackend::default());
        let handle = published_handle(backend.clone());
        (backend, handle)
    }

    /// Builds one explicit client submission with a stable test idempotency identity.
    /// 构建一条带稳定测试幂等身份的显式客户端输入。
    fn test_client_input(action_id: &str, text: &str) -> ClientInputSubmission {
        ClientInputSubmission::new(
            ClientActionId::try_new(action_id).expect("valid test client action id"),
            text,
            Vec::new(),
        )
    }

    /// Queue and draft wrappers publish each committed Core revision exactly once and preserve no-op receipts.
    /// 队列与草稿包装器会把每个已提交的 Core 修订精确发布一次，并保留空操作回执。
    #[test]
    fn queue_and_draft_wrappers_flush_only_committed_revisions() {
        let (recording, handle) = recording_handle();
        let first = handle
            .enqueue_input(test_client_input("registry-queue-first", "first"), 2)
            .expect("enqueue first input");
        let second = handle
            .enqueue_input(test_client_input("registry-queue-second", "second"), 3)
            .expect("enqueue second input");
        let first_identity = QueuedInputIdentity {
            client_action_id: first.value.client_action_id,
            input_id: first.value.input_id,
        };
        let second_identity = QueuedInputIdentity {
            client_action_id: second.value.client_action_id,
            input_id: second.value.input_id,
        };
        assert_eq!(recording.flush_count(), 2);

        let reordered = handle
            .reorder_queued_inputs(vec![second_identity.clone(), first_identity.clone()], 4)
            .expect("reorder queued inputs");
        assert!(reordered.changed);
        let repeated_order = handle
            .reorder_queued_inputs(vec![second_identity.clone(), first_identity.clone()], 5)
            .expect("repeat exact queue order");
        assert!(!repeated_order.changed);
        assert_eq!(recording.flush_count(), 3);

        let moved = handle
            .move_queued_input_to_draft(second_identity.clone(), 6)
            .expect("move queued input to draft");
        assert!(moved.changed);
        let repeated_move = handle
            .move_queued_input_to_draft(second_identity, 7)
            .expect("repeat exact move");
        assert!(!repeated_move.changed);
        assert_eq!(repeated_move.value, moved.value);
        assert_eq!(recording.flush_count(), 4);

        let removed = handle
            .remove_queued_input(first_identity.clone(), 8)
            .expect("remove queued input");
        assert!(removed.changed);
        let repeated_remove = handle
            .remove_queued_input(first_identity, 9)
            .expect("repeat exact removal");
        assert!(!repeated_remove.changed);
        assert_eq!(repeated_remove.value, removed.value);
        assert_eq!(recording.flush_count(), 5);
        let (memory_revision, durable_revision, queue_len, draft_text) = handle
            .read(|document| {
                (
                    document.memory_revision(),
                    document.durable_revision(),
                    document.metadata.input_state.queued_inputs.len(),
                    document.metadata.input_state.draft.text.clone(),
                )
            })
            .expect("read final queue and draft state");
        assert_eq!(memory_revision, durable_revision);
        assert_eq!(queue_len, 0);
        assert_eq!(draft_text, "second");
    }

    /// Builds one exact Provider preparation input from the current authoritative document.
    /// 从当前权威文档构建一份精确 Provider 准备输入。
    fn test_prepare_api_input(
        document: &SessionDocument,
        purpose: ApiPurposeKind,
        request: ApiRequestSnapshot,
        now_ms: i64,
    ) -> PrepareAndStartApiInput {
        let through_seq = document
            .events()
            .last()
            .map(|event| event.seq)
            .expect("test provider projection requires one root event");
        PrepareAndStartApiInput {
            purpose,
            request,
            request_budget_evidence: test_provider_request_budget_evidence(),
            projection: ProviderProjectionProof {
                segment_generation: document.segment_generation(),
                document_revision: document.memory_revision(),
                retained_from_seq: Seq::try_new(1).expect("valid retained seq"),
                through_seq,
                fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                    .expect("valid provider-visible fingerprint"),
            },
            locked_baseline: LockedBaselineProjection::empty(),
            continuation_action_id: None,
            now_ms,
        }
    }

    /// Creates the explicit initialized-empty test baseline and starts one physical API.
    /// 创建显式“已初始化但来源为空”的测试基线，并启动一次物理 API。
    fn start_initialized_test_api(handle: &SessionHandle) -> ApiCallId {
        handle
            .submit_root_input(test_client_input("registry-start-api-root", "root"), 2)
            .expect("submit test root input");
        let input = handle
            .read(|document| {
                test_prepare_api_input(
                    document,
                    ApiPurposeKind::MainGeneration,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    4,
                )
            })
            .expect("build test API preparation");
        handle
            .prepare_and_start_api_durable(input)
            .expect("start initialized test API")
            .value
            .api_call_id
    }

    /// The physical Provider permit promotes the exact pending request before request capture.
    /// 物理 Provider 许可会在捕获请求前提升精确待生效配置。
    #[tokio::test]
    async fn physical_provider_permit_promotes_pending_thinking_before_api_start() {
        // Both historical encodings are accepted as the same setting, not a perpetual route change.
        // 两种历史编码均作为同一设置接受，不能造成永久路由切换循环。
        for (stored_level, request_level) in [
            ("max", Some("max")),
            ("off", None),
            ("xhigh", Some("x-high")),
            ("x-high", Some("xhigh")),
        ] {
            let temporary = tempfile::tempdir().expect("temporary session root");
            let session_id = SessionId::new_v4();
            let locator =
                create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1)
                    .expect("create exact V1 session fixture");
            let loaded = load_session_v1_full(&locator).expect("load complete initial session");
            let handle = SessionHandle::new(session_id, locator, loaded.document);
            handle
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        document.metadata.config.active_request = ActiveRequestConfig {
                            provider: Some(String::from("provider-1")),
                            connection_id: Some(String::from("connection-1")),
                            model: Some(String::from("model-1")),
                            thinking_level: Some(String::from("high")),
                            temperature: Some(0.2),
                        };
                        let changed = document.set_pending_request(
                            PendingRequestConfig {
                                provider: Some(String::from("provider-1")),
                                connection_id: Some(String::from("connection-1")),
                                model: Some(String::from("model-1")),
                                thinking_level: Some(String::from(stored_level)),
                                temperature: Some(0.2),
                                set_at_ms: 2,
                            },
                            2,
                        )?;
                        assert!(changed, "pending request fixture must change metadata");
                        Ok(SessionMutationChange::Changed(()))
                    },
                )
                .expect("publish active and pending request fixture");
            handle
                .submit_root_input(
                    test_client_input("registry-pending-thinking-root", "root"),
                    3,
                )
                .expect("submit root input");
            let fingerprint_sha256 = handle
                .read(|document| {
                    let through_seq = document.events().last().expect("root event").seq();
                    provider_visible_fingerprint(document.events(), through_seq)
                })
                .expect("read provider projection")
                .expect("fingerprint provider projection");
            let request = ApiRequestSnapshot {
                provider: String::from("provider-1"),
                profile: None,
                model: String::from("model-1"),
                connection_id: Some(String::from("connection-1")),
                thinking_level: request_level.map(String::from),
                temperature: Some(0.2),
            };

            // Simulate a picker write overtaking construction for every request control.
            // 模拟选择器写入超越请求构建，覆盖每一种请求控制项。
            let revision_before = handle
                .read(|document| document.memory_revision())
                .expect("read pre-preparation revision");
            for field in 0..5 {
                let mut stale = request.clone();
                match field {
                    0 => stale.provider = String::from("old-provider"),
                    1 => stale.connection_id = Some(String::from("old-connection")),
                    2 => stale.model = String::from("old-model"),
                    3 => stale.thinking_level = Some(String::from("high")),
                    _ => stale.temperature = Some(0.1),
                }
                let receipt = handle
                    .prepare_physical_api_durable::<(), _>(
                        ApiPurposeKind::MainGeneration,
                        stale,
                        LockedBaselineProjection::empty(),
                        None,
                        4,
                        |_, _preparation| async {
                            panic!("stale candidate must be rebuilt before projection or promotion")
                        },
                    )
                    .await
                    .expect("stale selection returns a rebuild without starting an API");
                assert!(receipt.is_none());
                handle
                    .read(|document| {
                        assert_eq!(document.memory_revision(), revision_before);
                        assert!(document.metadata.config.pending_request.is_some());
                        assert!(document.active_api().is_none());
                        assert!(!document.events().iter().any(|event| matches!(
                            &event.event,
                            EventBody::RequestConfigChanged(_) | EventBody::DisplayNote(_)
                        )));
                    })
                    .expect("verify unchanged stale-request state");
            }

            let receipt = handle
                .prepare_physical_api_durable(
                    ApiPurposeKind::MainGeneration,
                    request.clone(),
                    LockedBaselineProjection::empty(),
                    None,
                    4,
                    move |snapshot, _preparation| async move {
                        Ok(Some(SessionPreparedPhysicalRequest {
                            user_prompt_media: Vec::new(),
                            transport: (),
                            projection: ProviderProjectionProof {
                                segment_generation: snapshot.segment_generation,
                                document_revision: snapshot.document_revision,
                                retained_from_seq: snapshot.retained_from_seq,
                                through_seq: snapshot.through_seq,
                                fingerprint_sha256,
                            },
                            request_budget_evidence: test_provider_request_budget_evidence(),
                        }))
                    },
                )
                .await
                .expect("prepare physical API after pending promotion")
                .expect("candidate matches pending selection");

            assert!(receipt.changed);
            handle
                .read(|document| {
                    assert!(document.metadata.config.pending_request.is_none());
                    assert_eq!(
                        document
                            .metadata
                            .config
                            .active_request
                            .thinking_level
                            .as_deref(),
                        Some(stored_level)
                    );
                    assert!(document.events().iter().any(|event| {
                        matches!(
                            &event.event,
                            EventBody::ApiStarted(started)
                                if started.api_call_id == receipt.value.core_start.api_call_id
                                    && started.request == request
                        )
                    }));
                })
                .expect("read promoted request facts");
            assert_eq!(
                handle
                    .read(|document| document.memory_revision() == document.durable_revision())
                    .expect("read durable revision"),
                true
            );
        }
    }

    /// Builds one retryable network terminal that can end before transport observation.
    /// 构建一份可重试的网络终态，用于表达传输观测前结束。
    fn retryable_network_completion(now_ms: i64) -> ApiCompletion {
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Failed,
            end_reason: Some(ApiEndReason::NetworkError),
            error: Some(RedactedApiError {
                category: ApiErrorCategory::Network,
                error_code: None,
                message: String::from("redacted pre-wire network failure"),
            }),
            continuation_eligibility: ContinuationEligibility::Allowed,
            timing: ApiTimings {
                completed_at_ms: Some(now_ms),
                ..ApiTimings::default()
            },
            provider_source: None,
            completed_at_ms: now_ms,
        }
    }

    /// Admits one Continue action after a retryable terminal on the initial physical API.
    /// 在初始物理 API 的可重试终态之后准入一个 Continue 动作。
    async fn admit_test_continue_action(
        handle: &Arc<SessionHandle>,
        action_id: &str,
    ) -> ClientActionId {
        let initial_api_call_id = start_initialized_test_api(handle);
        Arc::clone(handle)
            .mutate_durable_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                move |document| {
                    document
                        .complete_api_group(initial_api_call_id, retryable_network_completion(5))
                },
            )
            .await
            .expect("complete initial API before Continue");
        let action_id =
            ClientActionId::try_new(action_id).expect("valid Continue action id for test");
        handle
            .continue_turn(
                ContinuationRequest::new(action_id.clone(), "", Vec::new()),
                6,
            )
            .expect("admit Continue action");
        action_id
    }

    /// Builds one exact Continue API preparation from the current authoritative test document.
    /// 从当前权威测试文档构建一份精确 Continue API 准备输入。
    fn test_continue_prepare_input(
        handle: &SessionHandle,
        action_id: &ClientActionId,
    ) -> PrepareAndStartApiInput {
        let mut input = handle
            .read(|document| {
                test_prepare_api_input(
                    document,
                    ApiPurposeKind::Continue,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    7,
                )
            })
            .expect("build Continue API preparation");
        input.continuation_action_id = Some(action_id.clone());
        input
    }

    /// Async durable mutation returns only after the captured candidate is published.
    /// 异步可靠修改仅在捕获候选完成发布后返回。
    #[tokio::test]
    async fn async_durable_mutation_publishes_before_return() {
        let (recording, handle) = recording_handle();
        Arc::clone(&handle)
            .mutate_durable_async(
                SessionMutationAdmission::RootTurn,
                SessionFlushReason::InputAccepted,
                |document| {
                    document.submit_root_input(test_client_input("registry-root", "root"), 2)
                },
            )
            .await
            .expect("async durable mutation");

        let (memory_revision, durable_revision) = handle
            .read(|document| (document.memory_revision(), document.durable_revision()))
            .expect("read revisions");
        assert_eq!(memory_revision, durable_revision);
        assert!(
            recording
                .persisted_events()
                .iter()
                .any(|event| matches!(&event.event, EventBody::UserInput(_)))
        );
    }

    /// The process-local Continue owner excludes a second physical dispatch until release.
    /// 进程内 Continue 所有者在释放前排除第二次物理分派。
    #[tokio::test]
    async fn continuation_dispatch_guard_is_single_flight_per_session() {
        let (_recording, handle) = recording_handle();
        let first = handle.acquire_continuation_dispatch().await;
        let acquired = Arc::new(AtomicUsize::new(0));
        let follower_handle = Arc::clone(&handle);
        let follower_acquired = Arc::clone(&acquired);
        let follower = tokio::spawn(async move {
            let _guard = follower_handle.acquire_continuation_dispatch().await;
            follower_acquired.store(1, Ordering::SeqCst);
        });
        tokio::task::yield_now().await;
        assert_eq!(acquired.load(Ordering::SeqCst), 0);

        drop(first);
        follower.await.expect("follower dispatch owner");
        assert_eq!(acquired.load(Ordering::SeqCst), 1);
    }

    /// A durable pre-wire terminal wakes the detached Continue owner and releases its gate.
    /// 可靠的传输前终态会唤醒分离 Continue owner，并释放其门锁。
    #[tokio::test]
    async fn terminal_before_transport_wakes_continue_owner_and_releases_gate() {
        let (_recording, handle) = recording_handle();
        let initial_api_call_id = start_initialized_test_api(&handle);
        Arc::clone(&handle)
            .mutate_durable_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                move |document| {
                    document
                        .complete_api_group(initial_api_call_id, retryable_network_completion(5))
                },
            )
            .await
            .expect("complete initial API before Continue");

        let action_id = ClientActionId::try_new("continue-pre-wire-terminal")
            .expect("valid Continue action id");
        handle
            .continue_turn(
                ContinuationRequest::new(action_id.clone(), "", Vec::new()),
                6,
            )
            .expect("admit Continue action");
        let mut prepare_input = handle
            .read(|document| {
                test_prepare_api_input(
                    document,
                    ApiPurposeKind::Continue,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    7,
                )
            })
            .expect("build Continue API preparation");
        prepare_input.continuation_action_id = Some(action_id.clone());
        let continued_api_call_id = handle
            .prepare_and_start_api_durable(prepare_input)
            .expect("prepare Continue API")
            .value
            .api_call_id;

        let durable_edge = handle.continuation_dispatch_notify.notified();
        tokio::pin!(durable_edge);
        durable_edge.as_mut().enable();
        let waiter_handle = Arc::clone(&handle);
        let waiter_action_id = action_id.clone();
        let waiter_started = Arc::new(Notify::new());
        let waiter_started_signal = Arc::clone(&waiter_started);
        let waiter = tokio::spawn(async move {
            let _guard = waiter_handle.acquire_continuation_dispatch().await;
            waiter_started_signal.notify_one();
            waiter_handle
                .wait_until_continuation_dispatch_settled(&waiter_action_id)
                .await
        });
        waiter_started.notified().await;
        tokio::task::yield_now().await;

        Arc::clone(&handle)
            .mutate_durable_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                move |document| {
                    document
                        .complete_api_group(continued_api_call_id, retryable_network_completion(8))
                },
            )
            .await
            .expect("persist terminal before transport observation");

        tokio::time::timeout(Duration::from_secs(1), durable_edge)
            .await
            .expect("durable terminal must notify registered waiters");
        let waiter_error = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("detached Continue waiter must settle")
            .expect("detached Continue waiter task must join")
            .expect_err("pre-wire terminal must not report transport observation");
        assert!(matches!(
            waiter_error,
            SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                client_action_id,
                api_call_id,
            } if client_action_id == action_id && api_call_id == continued_api_call_id
        ));

        let _next_owner = tokio::time::timeout(
            Duration::from_secs(1),
            handle.acquire_continuation_dispatch(),
        )
        .await
        .expect("terminal waiter must release the per-session Continue gate");
    }

    /// An in-memory-only terminal stays invisible until its exact candidate becomes durable.
    /// 仅存在于内存的终态在其精确候选可靠落盘前保持不可见。
    #[tokio::test]
    async fn continuation_waiter_never_exposes_in_memory_only_terminal() {
        let (_recording, handle) = recording_handle();
        let initial_api_call_id = start_initialized_test_api(&handle);
        Arc::clone(&handle)
            .mutate_durable_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                move |document| {
                    document
                        .complete_api_group(initial_api_call_id, retryable_network_completion(5))
                },
            )
            .await
            .expect("complete initial API before Continue");
        let action_id = ClientActionId::try_new("continue-in-memory-terminal")
            .expect("valid Continue action id");
        handle
            .continue_turn(
                ContinuationRequest::new(action_id.clone(), "", Vec::new()),
                6,
            )
            .expect("admit Continue action");
        let mut prepare_input = handle
            .read(|document| {
                test_prepare_api_input(
                    document,
                    ApiPurposeKind::Continue,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    7,
                )
            })
            .expect("build Continue API preparation");
        prepare_input.continuation_action_id = Some(action_id.clone());
        let continued_api_call_id = handle
            .prepare_and_start_api_durable(prepare_input)
            .expect("prepare Continue API")
            .value
            .api_call_id;
        handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                move |document| {
                    document
                        .complete_api_group(continued_api_call_id, retryable_network_completion(8))
                },
            )
            .expect("append terminal without physical flush");

        let before_flush = handle
            .wait_for_continuation_transport_observed(&action_id, Duration::from_millis(25))
            .await
            .expect_err("in-memory-only terminal must remain invisible");
        assert!(matches!(
            before_flush,
            SessionRuntimeError::ContinuationTransportObservationTimedOut {
                client_action_id,
            } if client_action_id == action_id
        ));

        handle
            .flush_all()
            .expect("publish exact terminal candidate");
        let after_flush = handle
            .wait_until_continuation_dispatch_settled(&action_id)
            .await
            .expect_err("durable terminal must settle without transport observation");
        assert!(matches!(
            after_flush,
            SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                client_action_id,
                api_call_id,
            } if client_action_id == action_id && api_call_id == continued_api_call_id
        ));
    }

    /// A failed Continue terminal flush releases waiters, while admission retry exposes terminal.
    /// Continue 终态 flush 失败会释放等待者，而准入重试会暴露该终态。
    #[tokio::test]
    async fn continuation_terminal_flush_failure_releases_gate_and_retry_clears_failure() {
        let recording = Arc::new(RecordingPersistenceBackend::default());
        let backend = Arc::new(FailThenPersistBackend {
            remaining_failures: AtomicUsize::new(0),
            delegate: Arc::clone(&recording),
        });
        let handle = published_handle(backend.clone());
        let initial_api_call_id = start_initialized_test_api(&handle);
        Arc::clone(&handle)
            .mutate_durable_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                move |document| {
                    document
                        .complete_api_group(initial_api_call_id, retryable_network_completion(5))
                },
            )
            .await
            .expect("complete initial API before Continue");
        let action_id = ClientActionId::try_new("continue-terminal-flush-failure")
            .expect("valid Continue action id");
        handle
            .continue_turn(
                ContinuationRequest::new(action_id.clone(), "", Vec::new()),
                6,
            )
            .expect("admit Continue action");
        let mut prepare_input = handle
            .read(|document| {
                test_prepare_api_input(
                    document,
                    ApiPurposeKind::Continue,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    7,
                )
            })
            .expect("build Continue API preparation");
        prepare_input.continuation_action_id = Some(action_id.clone());
        let continued_api_call_id = handle
            .prepare_and_start_api_durable(prepare_input)
            .expect("prepare Continue API")
            .value
            .api_call_id;

        let waiter_handle = Arc::clone(&handle);
        let waiter_action_id = action_id.clone();
        let waiter_started = Arc::new(Notify::new());
        let waiter_started_signal = Arc::clone(&waiter_started);
        let waiter = tokio::spawn(async move {
            let _guard = waiter_handle.acquire_continuation_dispatch().await;
            waiter_started_signal.notify_one();
            waiter_handle
                .wait_until_continuation_dispatch_settled(&waiter_action_id)
                .await
        });
        waiter_started.notified().await;
        backend.remaining_failures.store(1, Ordering::SeqCst);
        let flush_error = Arc::clone(&handle)
            .mutate_durable_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                move |document| {
                    document
                        .complete_api_group(continued_api_call_id, retryable_network_completion(8))
                },
            )
            .await
            .expect_err("injected terminal flush must fail");
        assert!(matches!(flush_error, SessionRuntimeError::Storage(_)));
        let failed_revision = handle
            .read(SessionDocument::memory_revision)
            .expect("read failed candidate revision");
        let waiter_error = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("failed persistence must wake detached waiter")
            .expect("detached waiter task must join")
            .expect_err("failed persistence cannot report a durable settlement");
        assert!(matches!(
            waiter_error,
            SessionRuntimeError::ContinuationDispatchPersistenceFailed {
                client_action_id,
                candidate_revision,
                ..
            } if client_action_id == action_id && candidate_revision == failed_revision
        ));
        let bounded_error = handle
            .wait_for_continuation_transport_observed(&action_id, Duration::from_secs(1))
            .await
            .expect_err("bounded waiter must see the same failed attempt");
        assert!(matches!(
            bounded_error,
            SessionRuntimeError::ContinuationDispatchPersistenceFailed {
                client_action_id,
                candidate_revision,
                ..
            } if client_action_id == action_id && candidate_revision == failed_revision
        ));
        let next_owner = tokio::time::timeout(
            Duration::from_secs(1),
            handle.acquire_continuation_dispatch(),
        )
        .await
        .expect("failed waiter must release the per-session Continue gate");
        drop(next_owner);

        let durable_error = handle
            .reconcile_continuation_dispatch_for_action(&action_id)
            .expect_err("admission retry must expose durable terminal before dispatch");
        assert!(matches!(
            durable_error,
            SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                client_action_id,
                api_call_id,
            } if client_action_id == action_id && api_call_id == continued_api_call_id
        ));
        assert!(recording.flush_count() >= 6);
    }

    /// Quarantine wakes one detached Continue waiter and releases its session gate.
    /// 隔离会唤醒一个分离 Continue 等待者并释放其会话门。
    #[tokio::test]
    async fn quarantine_wakes_continue_waiter_and_releases_gate() {
        let (_recording, handle) = recording_handle();
        let action_id =
            ClientActionId::try_new("continue-quarantine").expect("valid Continue action id");
        let quarantine_edge = handle.continuation_dispatch_notify.notified();
        tokio::pin!(quarantine_edge);
        quarantine_edge.as_mut().enable();
        let waiter_handle = Arc::clone(&handle);
        let waiter_action_id = action_id.clone();
        let waiter = tokio::spawn(async move {
            let _guard = waiter_handle.acquire_continuation_dispatch().await;
            waiter_handle
                .wait_until_continuation_dispatch_settled(&waiter_action_id)
                .await
        });
        let reason = SessionQuarantineReason::MutationRevisionMismatch {
            before_revision: 3,
            after_revision: 5,
        };
        handle
            .enter_quarantine(reason.clone())
            .expect("enter deterministic quarantine");
        tokio::time::timeout(Duration::from_secs(1), quarantine_edge)
            .await
            .expect("quarantine transition must notify registered waiters");
        let waiter_error = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("quarantine must settle detached waiter")
            .expect("detached waiter task must join")
            .expect_err("quarantine cannot produce a dispatch receipt");
        assert!(matches!(
            waiter_error,
            SessionRuntimeError::SessionQuarantined {
                session_id,
                reason: actual_reason,
            } if session_id == handle.session_id && actual_reason == reason
        ));
        let _next_owner = tokio::time::timeout(
            Duration::from_secs(1),
            handle.acquire_continuation_dispatch(),
        )
        .await
        .expect("quarantined waiter must release the per-session Continue gate");
    }

    /// A rejected post-commit receipt quarantines the resident owner and releases Continue waiters.
    /// 被拒绝的提交后回执会隔离常驻 owner，并释放 Continue 等待者。
    #[tokio::test]
    async fn post_commit_receipt_rejection_quarantines_and_releases_continue_gate() {
        let recording = Arc::new(RecordingPersistenceBackend::default());
        let backend = Arc::new(CorruptPublishedMetadataBackend {
            corrupt_next: AtomicBool::new(false),
            delegate: Arc::clone(&recording),
        });
        let handle = published_handle(backend.clone());
        let action_id =
            admit_test_continue_action(&handle, "continue-post-commit-receipt-rejected").await;
        let continued_api_call_id = handle
            .prepare_and_start_api_durable(test_continue_prepare_input(&handle, &action_id))
            .expect("prepare Continue API")
            .value
            .api_call_id;
        let waiter_handle = Arc::clone(&handle);
        let waiter_action_id = action_id.clone();
        let waiter_started = Arc::new(Notify::new());
        let waiter_started_signal = Arc::clone(&waiter_started);
        let waiter = tokio::spawn(async move {
            let _guard = waiter_handle.acquire_continuation_dispatch().await;
            waiter_started_signal.notify_one();
            waiter_handle
                .wait_until_continuation_dispatch_settled(&waiter_action_id)
                .await
        });
        waiter_started.notified().await;
        tokio::task::yield_now().await;

        backend.corrupt_next.store(true, Ordering::SeqCst);
        let completion_error = Arc::clone(&handle)
            .mutate_durable_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                move |document| {
                    document
                        .complete_api_group(continued_api_call_id, retryable_network_completion(8))
                },
            )
            .await
            .expect_err("foreign post-commit metadata must reject the resident owner");
        assert!(matches!(
            completion_error,
            SessionRuntimeError::Domain(error)
                if matches!(*error, DomainError::ImmutableFieldMutation {
                    entity: "metadata",
                    field: "session_id",
                })
        ));
        assert!(
            recording.persisted_events().iter().any(|event| matches!(
                &event.event,
                EventBody::ApiStats(stats) if stats.api_call_id == continued_api_call_id
            )),
            "the injected failure must happen after physical candidate acceptance"
        );

        let waiter_error = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("post-commit quarantine must settle detached waiter")
            .expect("detached waiter task must join")
            .expect_err("quarantined owner cannot return a dispatch receipt");
        assert!(matches!(
            waiter_error,
            SessionRuntimeError::SessionQuarantined {
                session_id,
                reason: SessionQuarantineReason::DurableCommitApplicationRejected {
                    candidate_revision: _,
                    detail: _,
                },
            } if session_id == handle.session_id
        ));
        let _next_owner = tokio::time::timeout(
            Duration::from_secs(1),
            handle.acquire_continuation_dispatch(),
        )
        .await
        .expect("post-commit quarantine must release the per-session Continue gate");
    }

    /// A failed ApiStarted publication is reconciled before the first physical request is allowed.
    /// ApiStarted 发布失败会在允许首次物理请求前被协调落盘。
    #[tokio::test]
    async fn continuation_api_start_flush_failure_is_retained_until_covering_retry() {
        let recording = Arc::new(RecordingPersistenceBackend::default());
        let backend = Arc::new(FailThenPersistBackend {
            remaining_failures: AtomicUsize::new(0),
            delegate: Arc::clone(&recording),
        });
        let handle = published_handle(backend.clone());
        let action_id =
            admit_test_continue_action(&handle, "continue-api-start-flush-failure").await;
        let prepare_input = test_continue_prepare_input(&handle, &action_id);
        backend.remaining_failures.store(1, Ordering::SeqCst);
        let prepare_error = handle
            .prepare_and_start_api_durable(prepare_input)
            .expect_err("injected ApiStarted flush must fail");
        assert!(matches!(prepare_error, SessionRuntimeError::Storage(_)));
        assert!(
            handle
                .continuation_transport_observed_for_action(&action_id)
                .expect("query only the durable prefix")
                .is_none(),
            "failed ApiStarted candidate must not appear durable"
        );
        let wait_error = handle
            .wait_for_continuation_transport_observed(&action_id, Duration::from_secs(1))
            .await
            .expect_err("late waiter must receive retained persistence failure");
        assert!(matches!(
            wait_error,
            SessionRuntimeError::ContinuationDispatchPersistenceFailed {
                client_action_id,
                ..
            } if client_action_id == action_id
        ));

        let observed = handle
            .reconcile_continuation_dispatch_for_action(&action_id)
            .expect("retry must reconcile the exact ApiStarted candidate");
        assert!(
            observed.is_none(),
            "an ApiStarted-only retry must still allow the first physical dispatch"
        );
        let after_retry = handle
            .wait_for_continuation_transport_observed(&action_id, Duration::from_millis(25))
            .await
            .expect_err("retried ApiStarted still awaits transport observation");
        assert!(matches!(
            after_retry,
            SessionRuntimeError::ContinuationTransportObservationTimedOut {
                client_action_id,
            } if client_action_id == action_id
        ));
        assert!(recording.flush_count() >= 5);
    }

    /// A failed transport-observation flush is reconciled before retry can dispatch again.
    /// 传输观察 flush 失败会在重试能够再次分派前被协调落盘。
    #[tokio::test]
    async fn continuation_transport_observation_flush_failure_suppresses_second_dispatch() {
        let recording = Arc::new(RecordingPersistenceBackend::default());
        let backend = Arc::new(FailThenPersistBackend {
            remaining_failures: AtomicUsize::new(0),
            delegate: Arc::clone(&recording),
        });
        let handle = published_handle(backend.clone());
        let action_id =
            admit_test_continue_action(&handle, "continue-transport-observation-flush-failure")
                .await;
        let continued_api_call_id = handle
            .prepare_and_start_api_durable(test_continue_prepare_input(&handle, &action_id))
            .expect("prepare Continue API")
            .value
            .api_call_id;
        let failed_edge = handle.continuation_dispatch_notify.notified();
        tokio::pin!(failed_edge);
        failed_edge.as_mut().enable();
        backend.remaining_failures.store(1, Ordering::SeqCst);
        let observation_error = handle
            .mark_api_transport_observed_durable(continued_api_call_id, 8)
            .await
            .expect_err("injected transport-observation flush must fail");
        assert!(matches!(observation_error, SessionRuntimeError::Storage(_)));
        tokio::time::timeout(Duration::from_secs(1), failed_edge)
            .await
            .expect("failed transport observation must notify registered waiters");
        let waiter_error = handle
            .wait_until_continuation_dispatch_settled(&action_id)
            .await
            .expect_err("failed transport observation cannot return a durable receipt");
        assert!(matches!(
            waiter_error,
            SessionRuntimeError::ContinuationDispatchPersistenceFailed {
                client_action_id,
                ..
            } if client_action_id == action_id
        ));

        let observed = handle
            .reconcile_continuation_dispatch_for_action(&action_id)
            .expect("admission retry must reconcile the exact transport observation")
            .expect("reconciled observation must suppress a second physical dispatch");
        assert_eq!(observed.api_call_id, continued_api_call_id);
        assert_eq!(observed.continuation_action_id, Some(action_id));
        assert!(recording.flush_count() >= 6);
    }

    /// An unchanged mutation on a clean document succeeds without physical I/O.
    /// 干净文档上的未变化修改会成功，且不产生物理 I/O。
    #[test]
    fn idempotent_durable_mutation_does_not_fabricate_revision_or_flush() {
        let (recording, handle) = recording_handle();
        let receipt = handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |_document| Ok(SessionMutationChange::Unchanged("already-set")),
            )
            .expect("unchanged durable mutation");

        assert_eq!(receipt.value, "already-set");
        assert!(!receipt.changed);
        assert_eq!(receipt.revision, 0);
        assert_eq!(receipt.flush_reason, SessionFlushReason::MetadataChanged);
        assert_eq!(recording.flush_count(), 0);
    }

    /// A context-window change persists once, while repeating the same node is a clean no-op.
    /// 上下文窗口变更只持久化一次，重复选择同一节点是干净的幂等操作。
    #[test]
    fn context_compaction_window_change_is_durable_and_idempotent() {
        let (recording, handle) = recording_handle();
        let node_id = "compression-window-4"
            .parse::<vulcan_context::ContextCompactionWindowNodeId>()
            .expect("valid context-window node");
        let first = handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                move |document| {
                    let changed = document.set_context_compaction_window_node_id(Some(node_id), 2);
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                },
            )
            .expect("first context-window change");
        let revision_after_first = first.revision;
        assert!(first.changed);
        assert_eq!(recording.flush_count(), 1);

        let repeated = handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                move |document| {
                    let changed = document.set_context_compaction_window_node_id(Some(node_id), 3);
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                },
            )
            .expect("repeated context-window selection");

        assert!(!repeated.changed);
        assert_eq!(repeated.revision, revision_after_first);
        assert_eq!(recording.flush_count(), 1);
        assert_eq!(
            handle
                .read(|document| document.metadata.config.context_compaction_window_node_id)
                .expect("read context-window node"),
            Some(node_id)
        );
    }

    /// An output-style change persists once, while the normalized repeated selection is a no-op.
    /// 输出风格变更只持久化一次，重复选择同一规范值时保持幂等。
    #[test]
    fn output_style_change_is_durable_and_idempotent() {
        let (recording, handle) = recording_handle();
        let first = handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| {
                    let changed = document.set_output_style(Some(String::from(" concise ")), 2)?;
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                },
            )
            .expect("first output-style change");
        let revision_after_first = first.revision;
        assert!(first.changed);
        assert_eq!(recording.flush_count(), 1);

        let repeated = handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| {
                    let changed = document.set_output_style(Some(String::from("concise")), 3)?;
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                },
            )
            .expect("repeated output-style selection");

        assert!(!repeated.changed);
        assert_eq!(repeated.revision, revision_after_first);
        assert_eq!(recording.flush_count(), 1);
        assert_eq!(
            handle
                .read(|document| document.metadata.config.selection.output_style.clone())
                .expect("read output-style selection")
                .as_deref(),
            Some("concise")
        );
    }

    /// An unchanged invocation still publishes a revision dirtied beforehand.
    /// 未变化调用仍会发布调用前已经变脏的修订号。
    #[test]
    fn idempotent_durable_mutation_flushes_preexisting_dirty_revision() {
        let (recording, handle) = recording_handle();
        let dirty = handle
            .mutate_in_memory(
                SessionMutationAdmission::RootTurn,
                SessionFlushReason::InputAccepted,
                |document| {
                    document.submit_root_input(test_client_input("registry-root", "root"), 2)
                },
            )
            .expect("create dirty revision");
        assert_eq!(recording.flush_count(), 0);

        let receipt = handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |_document| Ok(SessionMutationChange::Unchanged(())),
            )
            .expect("flush existing dirty revision");
        let (memory_revision, durable_revision) = handle
            .read(|document| (document.memory_revision(), document.durable_revision()))
            .expect("read durable revisions");

        assert!(!receipt.changed);
        assert_eq!(receipt.revision, dirty.revision);
        assert_eq!(memory_revision, durable_revision);
        assert_eq!(recording.flush_count(), 1);
    }

    /// Concurrent unchanged requests share one serialized publication of prior dirt.
    /// 并发未变化请求共享一次对既有脏状态的串行发布。
    #[test]
    fn concurrent_idempotent_mutations_publish_preexisting_dirt_once() {
        let (recording, handle) = recording_handle();
        handle
            .mutate_in_memory(
                SessionMutationAdmission::RootTurn,
                SessionFlushReason::InputAccepted,
                |document| {
                    document.submit_root_input(test_client_input("registry-root", "root"), 2)
                },
            )
            .expect("create shared dirty revision");
        let barrier = Arc::new(Barrier::new(8));
        let mut workers = Vec::new();
        for _ in 0..8 {
            let handle = Arc::clone(&handle);
            let barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                barrier.wait();
                handle.mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    |_document| Ok(SessionMutationChange::Unchanged(())),
                )
            }));
        }

        for worker in workers {
            let receipt = worker
                .join()
                .expect("join idempotent worker")
                .expect("idempotent worker mutation");
            assert!(!receipt.changed);
        }
        let (memory_revision, durable_revision) = handle
            .read(|document| (document.memory_revision(), document.durable_revision()))
            .expect("read concurrent durable revisions");
        assert_eq!(memory_revision, durable_revision);
        assert_eq!(recording.flush_count(), 1);
    }

    /// The typed change decision must exactly match the revision effect.
    /// 强类型变化判定必须与修订号效果精确一致。
    #[test]
    fn conditional_mutation_rejects_false_change_claims() {
        let (recording, handle) = recording_handle();
        let result = handle.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |_document| Ok(SessionMutationChange::Changed(())),
        );

        assert!(matches!(
            result,
            Err(SessionRuntimeError::ConditionalMutationRevisionMismatch {
                reported_changed: true,
                before_revision: 0,
                after_revision: 0,
            })
        ));
        assert_eq!(recording.flush_count(), 0);
        assert!(matches!(
            handle.flush_all(),
            Err(SessionRuntimeError::SessionQuarantined { .. })
        ));
    }

    /// A false unchanged claim quarantines memory without persisting the leaked revision.
    /// 错误的未变化声明会隔离内存，且不持久化泄漏的修订号。
    #[test]
    fn conditional_mutation_quarantines_revision_leaked_by_false_unchanged_claim() {
        let (recording, handle) = recording_handle();
        let result = handle.mutate_durable_if_changed(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                document.submit_root_input(
                    test_client_input("registry-leaked-root", "leaked root"),
                    2,
                )?;
                Ok(SessionMutationChange::Unchanged(()))
            },
        );
        let (memory_revision, durable_revision) = handle
            .read(|document| (document.memory_revision(), document.durable_revision()))
            .expect("read leaked revision durability");

        assert!(matches!(
            result,
            Err(SessionRuntimeError::ConditionalMutationRevisionMismatch {
                reported_changed: false,
                before_revision: 0,
                after_revision: 1,
            })
        ));
        assert_eq!(memory_revision, 1);
        assert_eq!(durable_revision, 0);
        assert_eq!(recording.flush_count(), 0);
        assert!(matches!(
            handle.mutate_in_memory(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |_document| Ok(())
            ),
            Err(SessionRuntimeError::SessionQuarantined { .. })
        ));
    }

    /// A failure-atomic Core error leaves both revision and storage untouched.
    /// 具备失败原子性的 Core 错误不会改变修订号或存储。
    #[test]
    fn conditional_mutation_preserves_failure_atomic_domain_errors() {
        let (recording, handle) = recording_handle();
        let result = handle.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |_document| Err::<SessionMutationChange<()>, _>(DomainError::NoActiveApi),
        );
        let (memory_revision, durable_revision) = handle
            .read(|document| (document.memory_revision(), document.durable_revision()))
            .expect("read failure-atomic revisions");

        assert!(matches!(
            result,
            Err(SessionRuntimeError::Domain(error)) if *error == DomainError::NoActiveApi
        ));
        assert_eq!(memory_revision, 0);
        assert_eq!(durable_revision, 0);
        assert_eq!(recording.flush_count(), 0);
    }

    /// A malformed failing mutation quarantines its changed revision without persisting it.
    /// 错误实现的失败修改会隔离其变化修订，且不将其持久化。
    #[test]
    fn conditional_mutation_quarantines_revision_leaked_before_domain_error() {
        let (recording, handle) = recording_handle();
        let result = handle.mutate_durable_if_changed(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                document.submit_root_input(
                    test_client_input("registry-leaked-root", "leaked root"),
                    2,
                )?;
                Err::<SessionMutationChange<()>, _>(DomainError::NoActiveApi)
            },
        );
        let (memory_revision, durable_revision) = handle
            .read(|document| (document.memory_revision(), document.durable_revision()))
            .expect("read failed mutation durability");

        assert!(matches!(
            result,
            Err(SessionRuntimeError::FailedMutationProducedRevision {
                before_revision: 0,
                after_revision: 1,
                error,
            }) if *error == DomainError::NoActiveApi
        ));
        assert_eq!(memory_revision, 1);
        assert_eq!(durable_revision, 0);
        assert_eq!(recording.flush_count(), 0);
        assert!(matches!(
            handle.flush_all(),
            Err(SessionRuntimeError::SessionQuarantined { .. })
        ));
    }

    /// A successful ordinary mutation without one revision quarantines the handle.
    /// 成功但未精确推进一个修订的普通修改会隔离句柄。
    #[test]
    fn ordinary_durable_mutation_quarantines_success_without_revision() {
        let (recording, handle) = recording_handle();
        let result = handle.mutate_durable(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |_document| Ok(()),
        );

        assert!(matches!(
            result,
            Err(SessionRuntimeError::MutationRevisionMismatch {
                before_revision: 0,
                after_revision: 0,
            })
        ));
        assert_eq!(recording.flush_count(), 0);
        assert!(matches!(
            handle.flush_all(),
            Err(SessionRuntimeError::SessionQuarantined { .. })
        ));
    }

    /// An ordinary error that leaks a revision quarantines without publishing or flushing.
    /// 泄漏修订号的普通错误会隔离句柄，且不发布、不 flush。
    #[test]
    fn ordinary_in_memory_error_quarantines_without_publish_or_flush() {
        let (recording, handle) = recording_handle();
        let sink = Arc::new(RecordingEventSink::default());
        handle
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .unwrap();
        let result = handle.mutate_in_memory(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                document.submit_root_input(
                    test_client_input("registry-leaked-root", "leaked root"),
                    2,
                )?;
                Err::<(), _>(DomainError::NoActiveApi)
            },
        );

        assert!(matches!(
            result,
            Err(SessionRuntimeError::FailedMutationProducedRevision {
                before_revision: 0,
                after_revision: 1,
                error,
            }) if *error == DomainError::NoActiveApi
        ));
        assert_eq!(recording.flush_count(), 0);
        assert!(sink.events().is_empty());
        assert!(matches!(
            handle.flush_all(),
            Err(SessionRuntimeError::SessionQuarantined { .. })
        ));
    }

    /// A failure-atomic ordinary domain error does not quarantine the handle.
    /// 具备失败原子性的普通领域错误不会隔离句柄。
    #[test]
    fn ordinary_in_memory_failure_atomic_error_keeps_handle_admissible() {
        let (recording, handle) = recording_handle();
        let result = handle.mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |_document| Err::<(), _>(DomainError::NoActiveApi),
        );

        assert!(matches!(
            result,
            Err(SessionRuntimeError::Domain(error)) if *error == DomainError::NoActiveApi
        ));
        let root = handle
            .submit_root_input(test_client_input("registry-root", "root"), 2)
            .expect("failure-atomic error must not quarantine handle");
        assert_eq!(root.revision, 1);
        assert_eq!(recording.flush_count(), 1);
    }

    /// One durable mutation emits a volatile batch and an idempotent durable confirmation.
    /// 一次可靠修改先发出 volatile 批次，再发出可幂等的 durable 确认。
    #[test]
    fn durable_flush_publishes_canonical_live_delta_after_storage_success() {
        let (recording, handle) = recording_handle();
        let sink = Arc::new(RecordingEventSink::default());
        let registry = SessionRuntimeRegistry::default();
        registry
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .unwrap();
        registry.register(Arc::clone(&handle)).unwrap();

        handle
            .submit_root_input(test_client_input("registry-root", "root"), 2)
            .unwrap();

        let events = sink.events();
        let (volatile, durable) = match events.as_slice() {
            [
                ServiceEvent::ConversationEventDeltaV1(volatile),
                ServiceEvent::ConversationEventDeltaV1(durable),
            ] => (volatile, durable),
            actual => panic!("expected volatile and durable V1 deltas, got {actual:?}"),
        };
        let generation = handle
            .read(SessionDocument::segment_generation)
            .expect("read segment generation");
        assert_eq!(
            volatile.batch,
            ConversationEventWindowBatch::from_events(
                handle.session_id().to_string(),
                generation,
                &recording.persisted_events(),
            )
        );
        assert_eq!(
            volatile.durability,
            ConversationEventPatchDurabilityV1::Volatile
        );
        assert_eq!(
            durable.durability,
            ConversationEventPatchDurabilityV1::Durable
        );
        assert_eq!(durable.revision, volatile.revision);
        assert_eq!(durable.batch.session_id, volatile.batch.session_id);
        assert_eq!(
            durable.batch.window_generation,
            volatile.batch.window_generation
        );
        assert!(durable.batch.events.is_empty());
    }

    /// One edit generation atomically replaces the resident owner and publishes a rebase signal.
    /// 一次编辑 generation 会原子替换常驻 owner，并发布 rebase 信号。
    #[test]
    fn edit_generation_replaces_resident_owner_and_publishes_rebase() {
        let temporary = tempfile::tempdir().expect("temporary session root");
        let session_id = SessionId::new_v4();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1)
                .expect("create exact V1 session fixture");
        let loaded = load_session_v1_full(&locator).expect("load complete initial session");
        let handle = SessionHandle::new(session_id, locator.clone(), loaded.document);
        let sink = Arc::new(RecordingEventSink::default());
        handle
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .expect("install live event emitter");
        handle
            .submit_root_input(test_client_input("edit-root", "before edit"), 2)
            .expect("publish durable root input");
        let target_seq = handle
            .read(|document| {
                document
                    .events()
                    .iter()
                    .find(|event| matches!(&event.event, EventBody::UserInput(_)))
                    .map(|event| event.seq)
                    .expect("root input seq")
            })
            .expect("read root input seq");
        let prior_revision = handle
            .read(SessionDocument::memory_revision)
            .expect("read prior revision");
        sink.clear();

        let receipt = handle
            .publish_edit_generation(
                1,
                EditGenerationInput {
                    target_seq,
                    replacement: RootInputReplacement::new("after edit", Vec::new()),
                    now_ms: 3,
                },
            )
            .expect("publish edited generation");

        assert_eq!(receipt.target_seq, target_seq);
        assert_eq!(receipt.segment_generation, 2);
        assert_eq!(receipt.document_revision, prior_revision + 1);
        handle
            .read(|document| {
                assert_eq!(document.segment_generation(), 2);
                assert_eq!(document.memory_revision(), receipt.document_revision);
                assert_eq!(document.durable_revision(), receipt.document_revision);
                assert_eq!(
                    document.runtime_lock.initialization_state,
                    vulcan_session_core::v1::context::RuntimeInitializationState::BaselineRefreshPending
                );
                let EventBody::UserInput(input) =
                    &document.events()[target_seq.value() as usize - 1].event
                else {
                    panic!("target seq must remain a root user input");
                };
                assert_eq!(input.text, "after edit");
            })
            .expect("inspect replaced resident generation");
        let durable = load_session_v1_full(&locator).expect("strict reload edited generation");
        assert_eq!(durable.document.segment_generation(), 2);
        assert_eq!(
            durable.document.memory_revision(),
            receipt.document_revision
        );
        let events = sink.events();
        let delta = match events.as_slice() {
            [ServiceEvent::ConversationEventDeltaV1(delta)] => delta,
            actual => panic!("expected one generation rebase delta, got {actual:?}"),
        };
        assert_eq!(delta.batch.window_generation, 2);
        assert_eq!(delta.base_durable_revision, Some(prior_revision));
        assert_eq!(delta.revision, receipt.document_revision);
        assert_eq!(
            delta.durability,
            ConversationEventPatchDurabilityV1::Durable
        );
        assert!(delta.batch.events.is_empty());

        sink.clear();
        let error = handle
            .publish_edit_generation(
                1,
                EditGenerationInput {
                    target_seq,
                    replacement: RootInputReplacement::new("stale retry", Vec::new()),
                    now_ms: 4,
                },
            )
            .expect_err("stale generation must be rejected");
        assert!(matches!(
            error,
            SessionRuntimeError::GenerationSourceChanged {
                expected: 1,
                actual: 2
            }
        ));
        assert!(sink.events().is_empty());
        assert_eq!(
            handle
                .read(SessionDocument::segment_generation)
                .expect("read unchanged generation"),
            2
        );
    }

    /// Metadata-only revisions publish empty edges so the next event patch has an exact base.
    /// 仅 metadata 修订会发布空边，使下一份事件补丁拥有精确前驱。
    #[test]
    fn metadata_revision_publishes_empty_edges_before_next_event_patch() {
        let (_recording, handle) = recording_handle();
        let sink = Arc::new(RecordingEventSink::default());
        handle
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .unwrap();
        let api_call_id = start_initialized_test_api(&handle);
        let prior_revision = handle
            .read(SessionDocument::memory_revision)
            .expect("read revision before metadata mutation");
        sink.clear();

        let metadata = handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| {
                    let changed = document.set_output_style(Some("concise".to_string()), 5)?;
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                },
            )
            .expect("publish metadata-only revision");
        let semantic = handle
            .update_reasoning(
                api_call_id,
                "next semantic event",
                AssistantSemanticStatus::Streaming,
                6,
            )
            .expect("publish next event revision");

        assert_eq!(metadata.revision, prior_revision + 1);
        assert_eq!(semantic.revision, prior_revision + 2);
        let deltas = sink
            .events()
            .into_iter()
            .map(|event| match event {
                ServiceEvent::ConversationEventDeltaV1(delta) => delta,
                actual => panic!("expected V1 live delta, got {actual:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(deltas.len(), 3);
        assert_eq!(deltas[0].base_memory_revision, Some(prior_revision));
        assert_eq!(deltas[0].revision, prior_revision + 1);
        assert!(deltas[0].batch.events.is_empty());
        assert_eq!(
            deltas[0].durability,
            ConversationEventPatchDurabilityV1::Volatile
        );
        assert_eq!(deltas[1].revision, prior_revision + 1);
        assert!(deltas[1].batch.events.is_empty());
        assert_eq!(
            deltas[1].durability,
            ConversationEventPatchDurabilityV1::Durable
        );
        assert_eq!(deltas[2].base_memory_revision, Some(prior_revision + 1));
        assert_eq!(deltas[2].revision, prior_revision + 2);
        assert!(!deltas[2].batch.events.is_empty());
    }

    /// A failed metadata flush preserves its empty volatile edge for a durable retry.
    /// metadata flush 失败时保留其空 volatile 边，供后续可靠重试确认。
    #[test]
    fn failed_metadata_flush_retries_with_empty_durable_ack() {
        let recording = Arc::new(RecordingPersistenceBackend::default());
        let backend = Arc::new(FailThenPersistBackend {
            remaining_failures: AtomicUsize::new(0),
            delegate: Arc::clone(&recording),
        });
        let handle = published_handle(backend.clone());
        let sink = Arc::new(RecordingEventSink::default());
        handle
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .unwrap();
        start_initialized_test_api(&handle);
        let prior_revision = handle
            .read(SessionDocument::memory_revision)
            .expect("read revision before failed metadata mutation");
        sink.clear();
        backend.remaining_failures.store(1, Ordering::SeqCst);

        assert!(matches!(
            handle.mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| {
                    let changed = document.set_output_style(Some("compact".to_string()), 5)?;
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                },
            ),
            Err(SessionRuntimeError::Storage(_))
        ));
        let volatile_only = sink.events();
        assert_eq!(volatile_only.len(), 1);
        assert!(matches!(
            volatile_only.as_slice(),
            [ServiceEvent::ConversationEventDeltaV1(delta)]
                if delta.revision == prior_revision + 1
                    && delta.base_memory_revision == Some(prior_revision)
                    && delta.batch.events.is_empty()
                    && delta.durability == ConversationEventPatchDurabilityV1::Volatile
        ));

        handle.flush_all().expect("retry metadata flush");
        let retried = sink.events();
        assert_eq!(retried.len(), 2);
        assert!(matches!(
            retried.last(),
            Some(ServiceEvent::ConversationEventDeltaV1(delta))
                if delta.revision == prior_revision + 1
                    && delta.batch.events.is_empty()
                    && delta.durability == ConversationEventPatchDurabilityV1::Durable
        ));
    }

    /// Repeating an already durable metadata no-op emits no fabricated empty edge.
    /// 重复已经可靠落盘的 metadata 空操作不会伪造空边。
    #[test]
    fn repeated_metadata_noop_emits_no_duplicate_empty_edge() {
        let (_recording, handle) = recording_handle();
        let sink = Arc::new(RecordingEventSink::default());
        handle
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .unwrap();
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| {
                    let changed = document.set_output_style(Some("concise".to_string()), 2)?;
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                },
            )
            .expect("publish first metadata revision");
        sink.clear();

        let repeated = handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| {
                    let changed = document.set_output_style(Some("concise".to_string()), 3)?;
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                },
            )
            .expect("accept metadata no-op");
        handle.flush_all().expect("flush already durable no-op");

        assert!(!repeated.changed);
        assert!(sink.events().is_empty());
    }

    /// A failed physical flush preserves the volatile frame and later confirms it durably.
    /// 物理 flush 失败时保留 volatile 帧，并在稍后以 durable 方式确认。
    #[test]
    fn failed_flush_keeps_volatile_delta_before_retry_succeeds() {
        let recording = Arc::new(RecordingPersistenceBackend::default());
        let backend = Arc::new(FailThenPersistBackend {
            remaining_failures: AtomicUsize::new(1),
            delegate: Arc::clone(&recording),
        });
        let handle = published_handle(backend);
        let sink = Arc::new(RecordingEventSink::default());
        handle
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .unwrap();

        assert!(matches!(
            handle.submit_root_input(test_client_input("registry-root", "root"), 2),
            Err(SessionRuntimeError::Storage(_))
        ));
        assert!(matches!(
            sink.events().as_slice(),
            [ServiceEvent::ConversationEventDeltaV1(_)]
        ));

        handle.flush_all().unwrap();
        let events = sink.events();
        let (volatile, durable) = match events.as_slice() {
            [
                ServiceEvent::ConversationEventDeltaV1(volatile),
                ServiceEvent::ConversationEventDeltaV1(durable),
            ] => (volatile, durable),
            actual => panic!("expected volatile and durable retry deltas, got {actual:?}"),
        };
        assert_eq!(
            volatile.durability,
            ConversationEventPatchDurabilityV1::Volatile
        );
        assert_eq!(
            durable.durability,
            ConversationEventPatchDurabilityV1::Durable
        );
        assert_eq!(durable.revision, volatile.revision);
        assert!(!volatile.batch.events.is_empty());
        assert!(durable.batch.events.is_empty());
    }

    /// Multiple reasoning deltas publish exact same-seq volatile patches without token I/O.
    /// 多次思考增量会发布精确的同 seq volatile 补丁，且不产生逐 token I/O。
    #[test]
    fn reasoning_deltas_publish_exact_volatile_same_seq_patches() {
        let (recording, handle) = recording_handle();
        let sink = Arc::new(RecordingEventSink::default());
        handle
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .unwrap();
        let api_call_id = start_initialized_test_api(&handle);
        sink.clear();
        let flush_count_before_stream = recording.flush_count();

        let first = handle
            .update_reasoning(api_call_id, "first", AssistantSemanticStatus::Streaming, 5)
            .expect("publish first reasoning delta");
        // Replay the exact cumulative snapshot and require a true memory no-op.
        // 完全重放同一累计快照，并要求它成为真实的内存 no-op。
        let duplicate = handle
            .update_reasoning(api_call_id, "first", AssistantSemanticStatus::Streaming, 6)
            .expect("accept exact reasoning replay");
        let second = handle
            .update_reasoning(
                api_call_id,
                "first second",
                AssistantSemanticStatus::Completed,
                7,
            )
            .expect("publish completed reasoning delta");

        assert_eq!(first.value, second.value);
        assert_eq!(duplicate.value, first.value);
        assert!(!duplicate.changed);
        assert_eq!(duplicate.revision, first.revision);
        assert!(duplicate.changed_event_seqs.is_empty());
        assert_eq!(recording.flush_count(), flush_count_before_stream);
        let deltas = sink
            .events()
            .into_iter()
            .map(|event| match event {
                ServiceEvent::ConversationEventDeltaV1(delta) => delta,
                actual => panic!("expected V1 live delta, got {actual:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(deltas.len(), 2);
        assert!(
            deltas
                .iter()
                .all(|delta| delta.durability == ConversationEventPatchDurabilityV1::Volatile)
        );
        assert_eq!(deltas[0].batch.events[0].seq, deltas[1].batch.events[0].seq);
        assert_eq!(deltas[0].batch.events[0].event_revision, 1);
        assert_eq!(deltas[1].batch.events[0].event_revision, 2);
        assert!(matches!(
            &deltas[0].batch.events[0].event,
            EventBody::AssistantReasoning(reasoning) if reasoning.text == "first"
        ));
        assert!(matches!(
            &deltas[1].batch.events[0].event,
            EventBody::AssistantReasoning(reasoning) if reasoning.text == "first second"
        ));
    }

    /// An exact assistant-text replay does not publish or flush a synthetic revision.
    /// 完全相同的 assistant 正文重放不会发布或 flush 伪修订。
    #[test]
    fn assistant_text_exact_replay_is_a_memory_noop() {
        // Create one live owner and isolate events emitted after the API start.
        // 创建一个 live owner，并隔离 API 启动之后发布的事件。
        let (recording, handle) = recording_handle();
        let sink = Arc::new(RecordingEventSink::default());
        handle
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .unwrap();
        let api_call_id = start_initialized_test_api(&handle);
        sink.clear();
        let flush_count_before_stream = recording.flush_count();

        // Commit one cumulative snapshot and replay it byte-for-byte.
        // 提交一份累计快照，并逐字节重放它。
        let first = handle
            .update_assistant_text(api_call_id, "answer", AssistantSemanticStatus::Streaming, 5)
            .expect("publish first assistant snapshot");
        let duplicate = handle
            .update_assistant_text(api_call_id, "answer", AssistantSemanticStatus::Streaming, 6)
            .expect("accept exact assistant replay");

        assert!(first.changed);
        assert!(!duplicate.changed);
        assert_eq!(duplicate.value, first.value);
        assert_eq!(duplicate.revision, first.revision);
        assert!(duplicate.changed_event_seqs.is_empty());
        assert_eq!(recording.flush_count(), flush_count_before_stream);
        assert_eq!(sink.events().len(), 1);
    }

    /// A running tool grows from no output to structured output on its original seq.
    /// 运行中工具会在原始 seq 上从无输出增长为结构化输出。
    #[test]
    fn running_tool_output_growth_publishes_same_seq_patch() {
        let (_recording, handle) = recording_handle();
        let sink = Arc::new(RecordingEventSink::default());
        handle
            .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
            .unwrap();
        let api_call_id = start_initialized_test_api(&handle);
        sink.clear();
        let tool_call_id = handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                |document| {
                    document.prepare_tool(
                        api_call_id,
                        RequestStreamId::try_new("registry-running-tool-stream")?,
                        "file",
                        ToolPayload::empty(),
                        Some("provider-tool".to_string()),
                        5,
                    )
                },
            )
            .expect("prepare tool")
            .value;
        handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                |document| document.start_prepared_tool(api_call_id, tool_call_id, 5),
            )
            .expect("start prepared tool");
        let grown_output = ToolPayload::try_new(serde_json::json!({"bytes": 64})).unwrap();
        handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                |document| {
                    document.update_running_tool_output(
                        api_call_id,
                        tool_call_id,
                        Some(grown_output.clone()),
                        6,
                    )
                },
            )
            .expect("publish running tool output growth");

        let tool_patches = sink
            .events()
            .into_iter()
            .filter_map(|event| match event {
                ServiceEvent::ConversationEventDeltaV1(delta)
                    if delta.durability == ConversationEventPatchDurabilityV1::Volatile
                        && matches!(
                            delta.batch.events.first().map(|event| &event.event),
                            Some(EventBody::ToolCall(_))
                        ) =>
                {
                    Some(delta)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(tool_patches.len(), 3);
        assert!(
            tool_patches
                .iter()
                .all(|patch| { patch.batch.events[0].seq == tool_patches[0].batch.events[0].seq })
        );
        assert!(matches!(
            &tool_patches[0].batch.events[0].event,
            EventBody::ToolCall(tool) if tool.output.is_none()
        ));
        assert!(matches!(
            &tool_patches[2].batch.events[0].event,
            EventBody::ToolCall(tool) if tool.output.as_ref() == Some(&grown_output)
        ));
    }

    /// Repeating one exact tool terminal returns the persisted Core receipt without a new flush.
    /// 重复同一精确工具终态时返回已持久化的 Core 回执，且不产生新的 flush。
    #[test]
    fn repeated_exact_tool_finish_is_idempotent_at_the_session_handle() {
        let (recording, handle) = recording_handle();
        let api_call_id = start_initialized_test_api(&handle);
        let tool_call_id = handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                |document| {
                    document.prepare_tool(
                        api_call_id,
                        RequestStreamId::try_new("registry-idempotent-finish-stream")?,
                        "ask",
                        ToolPayload::try_new(serde_json::json!({"question": "confirm"}))?,
                        None,
                        5,
                    )
                },
            )
            .expect("prepare tool")
            .value;
        handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                |document| document.start_prepared_tool(api_call_id, tool_call_id, 5),
            )
            .expect("start prepared tool");
        let output = ToolPayload::try_new(serde_json::json!({"answer": "yes"})).unwrap();
        let first = handle
            .finish_tool(
                api_call_id,
                tool_call_id,
                ToolCompletion {
                    output: Some(output.clone()),
                    changeset: None,
                    status: ToolCallStatus::Completed,
                    finished_at_ms: 6,
                    external_refs: Vec::new(),
                    generated_resources: Vec::new(),
                },
            )
            .expect("commit first terminal tool result");
        let flushes_after_first = recording.flush_count();
        let repeated = handle
            .finish_tool(
                api_call_id,
                tool_call_id,
                ToolCompletion {
                    output: Some(output),
                    changeset: None,
                    status: ToolCallStatus::Completed,
                    finished_at_ms: 99,
                    external_refs: Vec::new(),
                    generated_resources: Vec::new(),
                },
            )
            .expect("repeat exact terminal tool result");

        assert!(first.changed);
        assert!(!repeated.changed);
        assert_eq!(repeated.value, first.value);
        assert_eq!(repeated.value.finished_at_ms, 6);
        assert_eq!(recording.flush_count(), flushes_after_first);
    }

    /// Volatile reasoning remains outside the durable candidate used by a restart.
    /// volatile 思考不会进入重启所读取的最后一份可靠候选。
    #[test]
    fn restart_projection_excludes_unflushed_volatile_reasoning() {
        let (recording, handle) = recording_handle();
        let api_call_id = start_initialized_test_api(&handle);
        handle
            .update_reasoning(
                api_call_id,
                "not durable",
                AssistantSemanticStatus::Streaming,
                5,
            )
            .expect("append volatile reasoning");

        assert!(
            handle
                .read(|document| {
                    document.events().iter().any(|event| {
                    matches!(
                        &event.event,
                        EventBody::AssistantReasoning(reasoning) if reasoning.text == "not durable"
                    )
                })
                })
                .unwrap()
        );
        assert!(!recording.persisted_events().iter().any(|event| matches!(
            &event.event,
            EventBody::AssistantReasoning(reasoning) if reasoning.text == "not durable"
        )));
    }

    /// Two failed flushes retain memory; the third persists the newest state.
    /// 两次 flush 失败保留内存；第三次把最新状态持久化。
    #[test]
    fn failed_flush_retries_latest_complete_candidate() {
        let recording = Arc::new(RecordingPersistenceBackend::default());
        let backend = Arc::new(FailThenPersistBackend {
            remaining_failures: AtomicUsize::new(2),
            delegate: Arc::clone(&recording),
        });
        let handle = published_handle(backend);
        let first = handle.mutate_durable(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| document.submit_root_input(test_client_input("registry-root", "root"), 2),
        );
        assert!(matches!(first, Err(SessionRuntimeError::Storage(_))));
        handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::DisplayBoundary,
                |document| {
                    let turn_id = document.active_turn().map(|turn| turn.turn_id);
                    document.append_display_note(
                        turn_id,
                        DisplayNoticeKind::SystemError,
                        NoticeSeverity::Info,
                        "latest",
                        None,
                        None,
                        None,
                        None,
                        3,
                    )
                },
            )
            .unwrap();
        assert!(matches!(
            handle.flush_all(),
            Err(SessionRuntimeError::Storage(_))
        ));
        handle.flush_all().unwrap();
        let memory_events = handle.read(|document| document.events().to_vec()).unwrap();
        assert_eq!(recording.persisted_events(), memory_events);
    }

    /// A newer mutation remains dirty when an older captured candidate publishes.
    /// 旧候选发布时，更新的内存修改仍保持脏状态。
    #[test]
    fn mutation_during_flush_remains_dirty_until_next_publication() {
        let gate = Arc::new((Mutex::new((false, false)), Condvar::new()));
        let recording = Arc::new(RecordingPersistenceBackend::default());
        let backend = Arc::new(BlockingPersistBackend {
            gate: Arc::clone(&gate),
            delegate: Arc::clone(&recording),
        });
        let handle = published_handle(backend);
        let writer_handle = Arc::clone(&handle);
        let writer = thread::spawn(move || {
            writer_handle.mutate_durable(
                SessionMutationAdmission::RootTurn,
                SessionFlushReason::InputAccepted,
                |document| {
                    document.submit_root_input(test_client_input("registry-root", "root"), 2)
                },
            )
        });

        let (state, changed) = &*gate;
        let mut state = state.lock().unwrap();
        while !state.0 {
            state = changed.wait(state).unwrap();
        }
        drop(state);

        handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::DisplayBoundary,
                |document| {
                    let turn_id = document.active_turn().map(|turn| turn.turn_id);
                    document.append_display_note(
                        turn_id,
                        DisplayNoticeKind::SystemError,
                        NoticeSeverity::Info,
                        "arrived during flush",
                        None,
                        None,
                        None,
                        None,
                        3,
                    )
                },
            )
            .unwrap();

        let mut state = state_for_release(&gate);
        state.1 = true;
        changed.notify_all();
        drop(state);
        writer.join().unwrap().unwrap();

        let (memory_revision, durable_revision) = handle
            .read(|document| (document.memory_revision(), document.durable_revision()))
            .unwrap();
        assert!(memory_revision > durable_revision);
        handle.flush_all().unwrap();
        let memory_events = handle.read(|document| document.events().to_vec()).unwrap();
        assert_eq!(recording.persisted_events(), memory_events);
    }

    /// Returns the locked gate state used to release a blocked publication.
    /// 返回用于释放阻塞发布的已加锁门状态。
    fn state_for_release(
        gate: &Arc<(Mutex<(bool, bool)>, Condvar)>,
    ) -> std::sync::MutexGuard<'_, (bool, bool)> {
        gate.0.lock().unwrap()
    }

    /// Concurrent tool completions update their original seq without reordering it.
    /// 并发工具完成只更新原 seq，不改变其顺序。
    #[test]
    fn concurrent_tool_completions_preserve_original_seq_order() {
        const TOOL_COUNT: usize = 12;
        let (_recording, handle) = recording_handle();
        handle
            .submit_root_input(test_client_input("registry-root", "root"), 2)
            .expect("submit root input");
        let preparation = handle
            .read(|document| {
                test_prepare_api_input(
                    document,
                    ApiPurposeKind::MainGeneration,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    3,
                )
            })
            .expect("build API preparation");
        let api_call_id = handle
            .prepare_and_start_api_durable(preparation)
            .unwrap()
            .value
            .api_call_id;
        let mut tool_call_ids = Vec::with_capacity(TOOL_COUNT);
        for index in 0..TOOL_COUNT {
            let tool_call_id = handle
                .mutate_in_memory(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::ToolBoundary,
                    |document| {
                        document.prepare_tool(
                            api_call_id,
                            RequestStreamId::try_new(format!("registry-concurrent-tool-{index}"))?,
                            format!("tool-{index}"),
                            ToolPayload::try_new(serde_json::json!({"index": index}))?,
                            Some(format!("provider-{index}")),
                            4 + index as i64,
                        )
                    },
                )
                .unwrap()
                .value;
            handle
                .mutate_in_memory(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::ToolBoundary,
                    |document| {
                        document.start_prepared_tool(api_call_id, tool_call_id, 4 + index as i64)
                    },
                )
                .unwrap();
            tool_call_ids.push(tool_call_id);
        }
        handle.flush_all().unwrap();
        let barrier = Arc::new(Barrier::new(TOOL_COUNT));
        let workers = tool_call_ids
            .iter()
            .copied()
            .enumerate()
            .map(|(index, tool_call_id)| {
                let handle = Arc::clone(&handle);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    handle.finish_tool(
                        api_call_id,
                        tool_call_id,
                        ToolCompletion {
                            output: Some(
                                ToolPayload::try_new(serde_json::json!({"completed": index}))
                                    .expect("valid concurrent tool output"),
                            ),
                            changeset: None,
                            status: ToolCallStatus::Completed,
                            finished_at_ms: 100 + index as i64,
                            external_refs: Vec::new(),
                            generated_resources: Vec::new(),
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        let persisted_tool_facts = handle
            .read(|document| {
                document
                    .events()
                    .iter()
                    .filter_map(|event| match &event.event {
                        EventBody::ToolCall(tool) => {
                            Some((event.seq, tool.tool_call_id, tool.status))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap();
        assert_eq!(persisted_tool_facts.len(), TOOL_COUNT);
        assert_eq!(
            persisted_tool_facts
                .iter()
                .map(|(_, tool_call_id, _)| *tool_call_id)
                .collect::<Vec<_>>(),
            tool_call_ids
        );
        assert!(
            persisted_tool_facts
                .iter()
                .all(|(_, _, status)| *status == ToolCallStatus::Completed)
        );
        assert!(
            persisted_tool_facts
                .windows(2)
                .all(|pair| pair[0].0.value() + 1 == pair[1].0.value())
        );
    }

    /// Shutdown rejects new roots and reopens admission after a failed close.
    /// 关闭拒绝新根输入，并在关闭失败后重新开放准入。
    #[test]
    fn shutdown_admission_is_explicit_and_reversible() {
        let (_recording, handle) = recording_handle();
        handle.begin_shutdown().unwrap();
        let rejected = handle.mutate_in_memory(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                document.submit_root_input(test_client_input("registry-blocked", "blocked"), 2)
            },
        );
        assert!(matches!(rejected, Err(SessionRuntimeError::ShuttingDown)));
        handle.resume_after_failed_shutdown().unwrap();
        handle
            .mutate_durable(
                SessionMutationAdmission::RootTurn,
                SessionFlushReason::InputAccepted,
                |document| {
                    document
                        .submit_root_input(test_client_input("registry-accepted", "accepted"), 3)
                },
            )
            .unwrap();
    }

    /// The registry rejects duplicate owners and returns the exact Arc.
    /// 注册表拒绝重复所有者，并返回精确的 Arc。
    #[test]
    fn registry_owns_exactly_one_handle_per_session() {
        let (_recording, handle) = recording_handle();
        let session_id = handle.session_id();
        let registry = SessionRuntimeRegistry::default();
        registry.register(Arc::clone(&handle)).unwrap();
        assert!(Arc::ptr_eq(&registry.get(session_id).unwrap(), &handle));
        assert!(matches!(
            registry.register(handle),
            Err(SessionRuntimeError::DuplicateSession(id)) if id == session_id
        ));
    }

    /// Concurrent exact-session loads execute one loader and return one Arc.
    /// 并发精确会话加载只执行一个加载器并返回同一个 Arc。
    #[test]
    fn concurrent_get_or_load_has_one_physical_owner() {
        const CALLER_COUNT: usize = 16;
        let (_recording, handle) = recording_handle();
        let session_id = handle.session_id();
        let registry = Arc::new(SessionRuntimeRegistry::default());
        let load_count = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(CALLER_COUNT));
        let callers = (0..CALLER_COUNT)
            .map(|_| {
                let registry = Arc::clone(&registry);
                let handle = Arc::clone(&handle);
                let load_count = Arc::clone(&load_count);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    registry
                        .get_or_load(session_id, || {
                            load_count.fetch_add(1, Ordering::SeqCst);
                            Ok(handle)
                        })
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        for caller in callers {
            assert!(Arc::ptr_eq(&caller.join().unwrap(), &handle));
        }
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
    }

    /// A waiting physical loader resumes after metadata-only publication without a false load failure.
    /// 等待的物理加载器在纯元数据发布后继续，不产生虚假的加载失败。
    #[test]
    fn capability_metadata_serializes_cold_save_and_physical_load() {
        // Channels hold the metadata writer at a deterministic pre-publication boundary.
        // 通道将元数据写入器固定在确定性的发布前边界。
        let (_, handle) = recording_handle();
        let session_id = handle.session_id();
        let registry = Arc::new(SessionRuntimeRegistry::default());
        let published = Arc::new(AtomicUsize::new(0));
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer_registry = registry.clone();
        let writer_published = published.clone();
        let writer = thread::spawn(move || {
            writer_registry.with_configuration_access(
                session_id,
                |_| panic!("cold configuration must not have a resident owner"),
                || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    writer_published.store(1, Ordering::SeqCst);
                    Ok(())
                },
            )
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let loader_registry = registry.clone();
        let loader_handle = handle.clone();
        let loader = thread::spawn(move || {
            loader_registry.get_or_load(session_id, || {
                assert_eq!(published.load(Ordering::SeqCst), 1);
                Ok(loader_handle)
            })
        });
        // Three references prove the loader acquired the same gate before it is released.
        // 三个引用证明加载器在门控释放前已取得同一门控。
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if Arc::strong_count(&registry.state.lock().unwrap().loading[&session_id]) >= 3 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "loader did not reach configuration gate"
            );
            thread::yield_now();
        }
        release_tx.send(()).unwrap();
        writer.join().unwrap().unwrap();
        assert!(Arc::ptr_eq(&loader.join().unwrap().unwrap(), &handle));
        assert!(registry.state.lock().unwrap().loading.is_empty());
    }

    /// Failed metadata publication releases admission and never poisons the session registry.
    /// 元数据发布失败会释放准入，绝不污染会话注册表。
    #[test]
    fn capability_metadata_failure_releases_unloaded_slot() {
        let (_, handle) = recording_handle();
        let session_id = handle.session_id();
        let registry = SessionRuntimeRegistry::default();
        assert!(
            registry
                .with_configuration_access::<()>(
                    session_id,
                    |_| panic!("unexpected resident"),
                    || Err(String::from("atomic write failed"))
                )
                .is_err()
        );
        assert!(registry.state.lock().unwrap().loading.is_empty());
        assert_eq!(
            registry
                .with_configuration_access(session_id, |_| panic!("unexpected resident"), || Ok(42))
                .unwrap(),
            42
        );
        assert!(Arc::ptr_eq(
            &registry
                .get_or_load(session_id, || Ok(handle.clone()))
                .unwrap(),
            &handle
        ));
        assert_eq!(
            registry
                .with_configuration_access(
                    session_id,
                    |_| Ok(43),
                    || panic!("must use resident owner")
                )
                .unwrap(),
            43
        );
    }

    /// The physical API start is recoverable before any Provider output exists.
    /// 在任何 Provider 输出出现前，物理 API 起始事实即可恢复。
    #[test]
    fn api_start_is_durable_before_zero_output_crash() {
        let (recording, handle) = recording_handle();
        handle
            .submit_root_input(test_client_input("registry-root", "root"), 2)
            .unwrap();
        let preparation = handle
            .read(|document| {
                test_prepare_api_input(
                    document,
                    ApiPurposeKind::MainGeneration,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    3,
                )
            })
            .expect("build API preparation");
        let api_call_id = handle
            .prepare_and_start_api_durable(preparation)
            .unwrap()
            .value
            .api_call_id;

        let (memory_revision, durable_revision) = handle
            .read(|document| (document.memory_revision(), document.durable_revision()))
            .unwrap();
        assert_eq!(memory_revision, durable_revision);
        assert!(recording.persisted_events().iter().any(|event| {
            matches!(
                &event.event,
                EventBody::ApiStarted(started) if started.api_call_id == api_call_id
            )
        }));
    }

    /// Repeated shutdown keeps one handle, permits terminal settlement, and
    /// preserves dirty memory across a failed close flush.
    /// 重复关闭保持同一句柄、允许终态收束，并在关闭 flush 失败时保留脏内存。
    #[test]
    fn repeated_shutdown_with_active_tool_has_deterministic_recovery() {
        let recording = Arc::new(RecordingPersistenceBackend::default());
        let backend = Arc::new(FailThenPersistBackend {
            remaining_failures: AtomicUsize::new(1),
            delegate: Arc::clone(&recording),
        });
        let handle = published_handle(backend);
        let session_id = handle.session_id();
        handle
            .mutate_in_memory(
                SessionMutationAdmission::RootTurn,
                SessionFlushReason::InputAccepted,
                |document| {
                    document.submit_root_input(test_client_input("registry-root", "root"), 2)
                },
            )
            .expect("submit root input");
        let api_call_id = handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiStarted,
                |document| {
                    document.prepare_and_start_api(test_prepare_api_input(
                        document,
                        ApiPurposeKind::MainGeneration,
                        ApiRequestSnapshot::new("openai", "gpt-5"),
                        3,
                    ))
                },
            )
            .expect("prepare API")
            .value
            .into_value()
            .api_call_id;
        let tool_call_id = handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                |document| {
                    document.prepare_tool(
                        api_call_id,
                        RequestStreamId::try_new("registry-shutdown-tool-stream")?,
                        "shell_command",
                        ToolPayload::try_new(serde_json::json!({"command": "echo ok"}))?,
                        Some("provider-tool".to_string()),
                        4,
                    )
                },
            )
            .expect("prepare tool")
            .value;
        handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                |document| document.start_prepared_tool(api_call_id, tool_call_id, 4),
            )
            .expect("start prepared tool");
        let registry = SessionRuntimeRegistry::default();
        registry.register(Arc::clone(&handle)).unwrap();
        let first_snapshot = registry.begin_shutdown().unwrap();
        let second_snapshot = registry.begin_shutdown().unwrap();
        assert_eq!(first_snapshot.len(), 1);
        assert!(Arc::ptr_eq(&first_snapshot[0], &second_snapshot[0]));

        let failed_terminal_flush = handle.mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.finish_tool(
                    api_call_id,
                    tool_call_id,
                    ToolCompletion {
                        output: Some(ToolPayload::try_new(
                            serde_json::json!({"status": "completed"}),
                        )?),
                        changeset: None,
                        status: ToolCallStatus::Completed,
                        finished_at_ms: 5,
                        external_refs: Vec::new(),
                        generated_resources: Vec::new(),
                    },
                )
            },
        );
        assert!(matches!(
            failed_terminal_flush,
            Err(SessionRuntimeError::Storage(_))
        ));
        assert!(matches!(
            handle.mutate_in_memory(
                SessionMutationAdmission::RootTurn,
                SessionFlushReason::InputAccepted,
                |document| document
                    .submit_root_input(test_client_input("registry-blocked-second", "blocked"), 6),
            ),
            Err(SessionRuntimeError::ShuttingDown)
        ));
        handle.flush_all().unwrap();
        assert_eq!(
            recording.persisted_events(),
            handle.read(|document| document.events().to_vec()).unwrap()
        );

        registry.resume_after_failed_shutdown().unwrap();
        assert!(matches!(
            registry.get(session_id),
            Ok(registered) if Arc::ptr_eq(&registered, &handle)
        ));
    }
}
