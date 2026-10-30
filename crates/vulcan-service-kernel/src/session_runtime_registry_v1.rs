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
use vulcan_service_api::{
    ConversationEventLiveDeltaV1, ConversationEventTextAppendV1, ConversationSessionAccessV1,
    ServiceEvent, ServiceEventEmitter,
};
use vulcan_session_core::v1::collaboration::{
    CollaborationOperationId, CollaborationRunControl, CollaborationRunControlReceipt,
    SetRunControlInput,
};
use vulcan_session_core::v1::config::QueuedInputIdentity;
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::dedicated::dedicated_provider_events;
use vulcan_session_core::v1::document::{
    ApiTransportObservedReceipt, ClientInputSubmission, CompactionCommitInput,
    ContinuationDispatchResolution, ContinuationRequest, ContinuationTerminalProjection,
    ContinueTurnReceipt, DraftMutationReceipt, EditGenerationInput, EnqueueInputReceipt,
    FlushCandidate, ForkGenerationInput, GenerationBuildOutput, IdempotentMutation,
    MemoryRevisionReceipt, PrepareAndStartApiInput, PreparedApiStart, PreparedToolBatchReceipt,
    QueuedInputDraftReceipt, QueuedInputOrderReceipt, QueuedInputPendingTurnReceipt,
    QueuedInputRemovalReceipt, SessionDocument, SubagentReturnInput, SubagentReturnReceipt,
    ToolCompletion, ToolCompletionReceipt, ToolPreparation, TurnCompletionTransition, TurnOrigin,
    TurnSuccessor,
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
    ProjectedItem, SourcedProjectedItem, project_context_generation_items_with_sources,
};
use vulcan_session_core::v1::service_dto::{
    ConversationEventSummary, ConversationEventWindowBatch,
};
use vulcan_session_core::v1::{
    ProviderRequestBudgetEvidence, ProviderUsageBudgetAnchor, latest_provider_usage_budget_anchor,
};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    ProviderApiTraceStore, SessionV1FlushResult, SessionV1StorageLocator, SessionV1StoreError,
    flush_session_v1_candidate, load_session_v1_full, publish_edit_session_v1,
};

mod compaction_continuation;
mod lifetime_usage;
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
    /// One durable child return without numeric usage entered the parent Turn.
    /// 一条可靠子代理回流，不附带数字用量进入父回合。
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
    /// Persists collaboration control-plane facts without opening a new Turn.
    /// 持久化协作控制面事实，但不创建新的回合。
    ControlPlane,
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
    /// Durable steering or a task result requires continuation of the original business turn.
    /// 可靠方向或任务结果要求继续原业务回合。
    Collaboration,
    /// The logical turn closed and selected its normal successor.
    /// 逻辑回合已关闭并选择正常后继。
    Completed(TurnCompletionTransition),
    /// The assistant output is retained while durable collaboration obligations remain open.
    /// 在可靠协作完成义务仍未结清时保留 assistant 输出，不发布逻辑完成。
    Deferred,
    /// A durable return keeps the logical turn open for one more physical request.
    /// 可靠回流保持逻辑回合开放，以便继续一次物理请求。
    SubagentReturn,
    /// Inputs inserted after the final API were streamed; the turn stays open for one continuation.
    /// 最后一次 API 之后插入的输入已写入事件流；回合保持开放以续跑一次。
    PendingInTurn(Vec<vulcan_session_core::v1::ids::Seq>),
}

/// Determines which immutable frontier owns instruction filtering for a durable projection.
/// 决定可靠投影的指令筛选由哪一个不可变前沿持有。
#[derive(Clone, Copy)]
enum SessionProjectionPurpose {
    /// Ordinary requests preserve the current committed context generation.
    /// 普通请求保留当前已提交上下文代次。
    ProviderRequest,
    /// Summary planning and physical requests exclude instructions inactive at the captured durable frontier.
    /// 摘要规划和物理请求排除在已捕获可靠前沿处已经失效的指令。
    CompactionContext,
}

/// Immutable V1 ledger projection captured by the unique session owner before API start.
/// 在 API 启动前由唯一会话 owner 捕获的不可变 V1 账本投影。
#[derive(Debug, Clone)]
pub(crate) struct SessionProviderProjectionSnapshot {
    /// Exact open logical turn captured with the durable source; closed turns have no execution owner.
    /// 与可靠来源一同捕获的精确开放逻辑回合；已关闭回合没有执行归属。
    pub(crate) open_turn_id: Option<TurnId>,
    /// Frozen child business execution identity; regular sessions have no child profile.
    /// 冻结的子业务执行身份；普通会话没有子执行档案。
    pub(crate) child_business_execution_id: Option<String>,
    /// Exact durable compaction identity captured with this projection's retained boundary.
    /// 与本投影保留边界一同捕获的准确持久压缩身份。
    pub(crate) current_compaction_id: Option<CompactionId>,
    /// Exact pending prompt sequences admitted by Core before their first physical API.
    /// 在首次物理API前由Core准入的精确待准备提示词序号。
    pub(crate) preparing_user_prompt_media_seqs: Vec<Seq>,
    /// Committed frontier of the current compaction; user media at or before it is delivered as an address only.
    /// 当前压缩的已提交前沿；该前沿及之前的用户媒体仅以地址投递。
    pub(crate) compaction_frontier_seq: Option<Seq>,
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
    /// Persisted initialized baseline for this exact generation; `None` permits first initialization.
    /// 本精确 generation 已持久化的初始化基线；`None` 允许首次初始化。
    pub(crate) initialized_locked_baseline: Option<LockedBaselineProjection>,
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
    /// Exact baseline committed with this physical API start.
    /// 与本次物理 API 起始一同提交的精确基线。
    pub(crate) locked_baseline: LockedBaselineProjection,
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
    /// The session is sealed for lifecycle deletion and rejects every new mutation.
    /// 会话已进入生命周期删除封存状态并拒绝所有新修改。
    SessionSealed(SessionId),
    /// Deletion sealing found durable collaboration work that still needs settlement.
    /// 删除封存检查发现仍有需要收敛的持久协作工作。
    UnsettledCollaboration(SessionId),
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
    /// A Continue-owned Turn terminated before any physical API was prepared.
    /// Continue 所属回合在任何物理 API 准备前已经终止。
    ContinuationTerminatedBeforeApi {
        /// Browser idempotency identity of the Continue action.
        /// Continue 动作的浏览器幂等身份。
        client_action_id: ClientActionId,
        /// Durable UI projection associated with the terminal Turn boundary.
        /// 与终态回合边界关联的可靠 UI 投影。
        projection: ContinuationTerminalProjection,
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
        /// Durable UI projection associated with the terminal API fact.
        /// 与 API 终态事实关联的可靠 UI 投影。
        projection: ContinuationTerminalProjection,
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
    /// Final request accounting requires compaction before a physical API can be committed.
    /// 最终请求记账要求在提交物理 API 前执行压缩。
    ProviderContextCompactionRequired(String),
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
            Self::SessionSealed(session_id) => write!(
                formatter,
                "V1 session {session_id} is sealed for lifecycle deletion"
            ),
            Self::UnsettledCollaboration(session_id) => write!(
                formatter,
                "V1 session {session_id} still has unsettled collaboration work"
            ),
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
            Self::ContinuationTerminatedBeforeApi {
                client_action_id, ..
            } => write!(
                formatter,
                "V1 Continue action {} terminated before a physical API was prepared",
                client_action_id.as_str()
            ),
            Self::ContinuationTerminatedBeforeTransport {
                client_action_id,
                api_call_id,
                ..
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
            Self::ProviderContextCompactionRequired(reason) => formatter.write_str(reason),
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

/// Tracks the final deletion fence independently from ordinary shutdown draining.
/// 独立记录最终删除封存状态，避免与普通关闭排空混淆。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionDeletionState {
    /// The handle accepts mutations according to its ordinary admission class.
    /// 句柄按普通准入类别接受修改。
    Open,
    /// Deletion has reserved the handle and all new mutations must stop.
    /// 删除已预留该句柄，所有新修改必须停止。
    Sealing,
    /// Deletion has completed the final write-side fence.
    /// 删除已完成最终写入侧封存。
    Sealed,
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

/// Streaming text events of one window generation exactly as last published.
/// 某一窗口代次中流式正文事件上次发布时的精确状态。
///
/// Volatile patches form a gap-free revision chain, so a client that accepts a patch holds the
/// event state at its base revision, which for these seqs is exactly the state recorded here.
/// volatile 补丁构成无缺口修订链，接受补丁的客户端持有其基线修订处的事件状态，对于这些 seq
/// 即此处记录的状态。
#[derive(Default)]
struct PublishedStreamingText {
    /// Generation the recorded events belong to.
    /// 记录的事件所属代次。
    window_generation: Option<u64>,
    /// Last published summary of each event that is still streaming text.
    /// 每条仍为流式正文的事件上次发布的摘要。
    events: BTreeMap<Seq, ConversationEventSummary>,
}

/// Unique mutable owner and serialized physical writer for one V1 session.
/// 单个 V1 会话的唯一可变所有者与串行物理写入器。
pub(crate) struct SessionHandle {
    /// Root-shared aggregate initialized only during the session loading lifecycle.
    /// 仅在会话加载生命周期初始化的根共享累计。
    lifetime_usage: std::sync::OnceLock<Result<Arc<lifetime_usage::SessionUsageOwner>, String>>,
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
    /// Streaming text as last published; read and written only in volatile-publish order.
    /// 上次发布的流式正文；仅按 volatile 发布顺序读写。
    published_streaming_text: Mutex<PublishedStreamingText>,
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
    /// Whether this handle is sealed or sealing for final lifecycle deletion.
    /// 当前句柄是否正在进行或已经完成最终生命周期删除封存。
    sealed_for_deletion: Mutex<SessionDeletionState>,
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
            published_streaming_text: Mutex::new(PublishedStreamingText::default()),
            physical_request_preparation: Arc::new(AsyncMutex::new(())),
            backend: Arc::new(FileSessionPersistenceBackend {
                locator: locator.clone(),
            }),
            locator: Some(locator),
            lifetime_usage: std::sync::OnceLock::new(),
            provider_api_trace_store: Some(provider_api_trace_store),
            live_event_publisher: Mutex::new(None),
            continuation_dispatch_notify: Notify::new(),
            continuation_dispatch_gate: Arc::new(AsyncMutex::new(())),
            continuation_dispatch_durability: Mutex::new(ContinuationDispatchDurabilityState {
                durable_through_seq,
                persistence_failures: BTreeMap::new(),
            }),
            shutting_down: Mutex::new(false),
            sealed_for_deletion: Mutex::new(SessionDeletionState::Open),
            quarantine: Mutex::new(None),
        })
    }

    /// Builds a file-backed owner with an injected writer for storage-failure verification.
    /// 使用注入写入器构建文件所有者，以验证存储失败。
    /// `locator` and `document` retain real loading authority; `backend` controls only physical writes.
    /// `locator` 与 `document` 保留实际加载权威；`backend` 仅控制物理写入。
    /// Returns an unpublished-to-registry owner with the same production loading behavior.
    /// 返回尚未注册且保持相同生产加载行为的所有者。
    #[cfg(test)]
    pub(crate) fn with_storage_backend(
        session_id: SessionId,
        locator: SessionV1StorageLocator,
        document: SessionDocument,
        backend: Arc<dyn SessionPersistenceBackend>,
    ) -> Arc<Self> {
        // No other owner can observe this handle before its first registration.
        // 首次注册前，其他所有者无法观察此句柄。
        let mut handle = Self::new(session_id, locator, document);
        Arc::get_mut(&mut handle)
            .expect("new handle has one owner")
            .backend = backend;
        handle
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
            published_streaming_text: Mutex::new(PublishedStreamingText::default()),
            physical_request_preparation: Arc::new(AsyncMutex::new(())),
            backend,
            locator: None,
            lifetime_usage: lifetime_usage::test_usage_owner(session_id),
            provider_api_trace_store: None,
            live_event_publisher: Mutex::new(None),
            continuation_dispatch_notify: Notify::new(),
            continuation_dispatch_gate: Arc::new(AsyncMutex::new(())),
            continuation_dispatch_durability: Mutex::new(ContinuationDispatchDurabilityState {
                durable_through_seq,
                persistence_failures: BTreeMap::new(),
            }),
            shutting_down: Mutex::new(false),
            sealed_for_deletion: Mutex::new(SessionDeletionState::Open),
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
        self.ensure_admitted(SessionMutationAdmission::RootTurn)?;
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
        self.ensure_admitted(SessionMutationAdmission::RootTurn)?;
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

    /// Starts a strict fixture root without service-level idle reconciliation.
    /// 不进行服务级空闲对账，启动严格测试夹具根回合。
    #[cfg(test)]
    pub(crate) fn submit_root_input(
        &self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<TurnId>, SessionRuntimeError> {
        self.submit_root_input_with_idle_authority(submission, now_ms, false)
    }

    /// Publishes a new root after the caller excludes every physical execution owner.
    /// 调用方排除全部物理执行所有者后，发布新根输入。
    /// `submission` and `now_ms` retain input identity; the receipt covers old closure and new admission.
    /// `submission` 与 `now_ms` 保留输入身份；回执覆盖旧回合闭合与新准入。
    pub(crate) fn submit_root_input_after_execution_ended(
        &self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<TurnId>, SessionRuntimeError> {
        self.submit_root_input_with_idle_authority(submission, now_ms, true)
    }

    /// Atomically publishes input and Running control, optionally settling a proven idle predecessor.
    /// 原子发布输入与运行控制，可选结算已证实空闲的前序回合。
    /// `execution_ended` requires caller-held dispatch exclusion; returns one durable receipt.
    /// `execution_ended` 要求调用方持有分发互斥；返回一次可靠回执。
    fn submit_root_input_with_idle_authority(
        &self,
        submission: ClientInputSubmission,
        now_ms: i64,
        execution_ended: bool,
    ) -> Result<SessionConditionalMutationReceipt<TurnId>, SessionRuntimeError> {
        let running_operation_id = CollaborationOperationId::try_new(format!(
            "root-input-running:{}:{}",
            self.session_id,
            submission.client_action_id.as_str()
        ))
        .map_err(SessionRuntimeError::from)?;
        let receipt = self.mutate_durable_if_changed(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            move |document| {
                document.apply_atomic_transition(move |document| {
                    let turn_mutation = if execution_ended {
                        document.submit_root_input_after_execution_ended(submission, now_ms)?
                    } else {
                        document.submit_root_input(submission, now_ms)?
                    };
                    match turn_mutation {
                        IdempotentMutation::Idempotent(turn_id) => {
                            Ok(SessionMutationChange::Unchanged((turn_id, None)))
                        }
                        IdempotentMutation::Committed(turn_id) => {
                            let control_mutation =
                                document.set_collaboration_run_control(SetRunControlInput {
                                    operation_id: running_operation_id,
                                    state: CollaborationRunControl::Running,
                                    changed_at_ms: now_ms,
                                })?;
                            let control_receipt = match control_mutation {
                                IdempotentMutation::Committed(receipt)
                                | IdempotentMutation::Idempotent(receipt) => receipt,
                            };
                            Ok(SessionMutationChange::Changed((
                                turn_id,
                                Some(control_receipt),
                            )))
                        }
                    }
                })
            },
        )?;
        // A fresh root input and its Running control are committed as one durable admission; an idempotent replay preserves a prior stop state.
        // 新根输入与 Running 控制在同一次可靠准入中提交；幂等重放保留之前的停止状态。
        Ok(SessionConditionalMutationReceipt {
            value: receipt.value.0,
            changed: receipt.changed,
            revision: receipt.revision,
            changed_event_seqs: receipt.changed_event_seqs,
            flush_reason: receipt.flush_reason,
        })
    }

    /// Persists one typed collaboration run-control transition under the session owner.
    /// 在会话 owner 下持久化一条类型化协作运行控制转换。
    pub(crate) fn set_collaboration_run_control_durable(
        &self,
        input: SetRunControlInput,
    ) -> Result<
        SessionConditionalMutationReceipt<CollaborationRunControlReceipt>,
        SessionRuntimeError,
    > {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document
                    .set_collaboration_run_control(input)
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
        // Only a newly scheduled user successor may activate control; queued work and retries do not.
        // 只有新调度的用户后继可以激活控制；普通排队及重试不能激活。
        let running_operation_id = CollaborationOperationId::try_new(format!(
            "queued-input-running:{}:{}",
            self.session_id,
            submission.client_action_id.as_str()
        ))
        .map_err(SessionRuntimeError::from)?;
        self.mutate_durable_if_changed(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            move |document| {
                document.apply_atomic_transition(move |document| {
                    match document.enqueue_input(submission, now_ms)? {
                        IdempotentMutation::Idempotent(receipt) => {
                            Ok(SessionMutationChange::Unchanged(receipt))
                        }
                        IdempotentMutation::Committed(receipt) => {
                            if matches!(
                                receipt.disposition,
                                vulcan_session_core::v1::document::EnqueueInputDisposition::SupersededGoalAndScheduled { .. }
                            ) {
                                // Publish the new business turn and its run permission in the same revision.
                                // 在同一修订发布新业务回合及其运行许可。
                                document.set_collaboration_run_control(SetRunControlInput {
                                    operation_id: running_operation_id,
                                    state: CollaborationRunControl::Running,
                                    changed_at_ms: now_ms,
                                })?;
                            }
                            Ok(SessionMutationChange::Changed(receipt))
                        }
                    }
                })
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

    /// Durably acknowledges one exact queue attachment release after index cleanup succeeds.
    /// 在索引清理成功后可靠确认一条精确的队列附件释放。
    ///
    /// # Parameters
    /// 参数
    /// `client_action_id` selects the persisted release intent and `now_ms` dates the mutation.
    /// `client_action_id` 选择持久释放意图，`now_ms` 记录变更时间。
    ///
    /// # Returns
    /// 返回值
    /// The changed or idempotent durable receipt, or an explicit storage error.
    /// 返回已变化或幂等的可靠回执，或明确的存储错误。
    pub(crate) fn acknowledge_queued_staging_release(
        &self,
        client_action_id: ClientActionId,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<()>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document
                    .acknowledge_queued_staging_release(&client_action_id, now_ms)
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
    /// `purpose`, `request`, `candidate_locked_baseline`, and `continuation_action_id` are typed request
    /// facts; `build` consumes the immutable durable V1 projection and its scoped completion authority,
    /// returning the only transport value that may be released or an explicit stale-result rejection.
    /// `purpose`、`request`、`candidate_locked_baseline` 与 `continuation_action_id` 是强类型请求事实；
    /// `build`消费不可变可靠V1投影及其作用域完成权限，返回唯一允许放行的传输值或明确陈旧结果拒绝。
    ///
    /// # Returns
    /// 返回值
    /// Returns a receipt only after prior state, `api_started`, and any fresh baseline are durable.
    /// 仅在先前状态、`api_started` 与首次基线全部可靠落盘后返回回执。
    /// Returns `None` when the candidate differs from the active selection or an external document
    /// frontier changed before commit; own completion writes require real durable recapture.
    /// 候选与活动选择不一致，或外部文档前沿在提交之前变化时返回 `None`；自身完成写入要求真实可靠重捕获。
    /// The asynchronous preparation gate serializes builders; writer/publisher guards cover capture and commit only.
    /// 异步准备门串行化构建者；写入器与发布器守卫仅覆盖捕获和提交。
    pub(crate) async fn prepare_physical_api_durable<T, BuildFuture>(
        self: &Arc<Self>,
        purpose: ApiPurposeKind,
        request: ApiRequestSnapshot,
        candidate_locked_baseline: LockedBaselineProjection,
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
        // The physical API purpose also owns every later recapture, not only the initial summary plan.
        // 物理 API 用途同时决定每次后续重捕获，不能只在最初摘要规划中生效。
        let projection_purpose = if purpose == ApiPurposeKind::Compaction {
            SessionProjectionPurpose::CompactionContext
        } else {
            SessionProjectionPurpose::ProviderRequest
        };
        let snapshot = {
            let _writer = self
                .writer
                .lock()
                .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
            let _publisher = self
                .mutation_publisher
                .lock()
                .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
            self.ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
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
                let active = &document.metadata.config.active_request;
                let active_route_initialized = active.provider.is_some() || active.model.is_some();
                if active_route_initialized
                    && (active.provider.as_deref() != Some(request.provider.as_str())
                        || active.connection_id != request.connection_id
                        || active.model.as_deref() != Some(request.model.as_str())
                        || vulcan_session_core::v1::config::canonical_request_thinking_level(
                            active.thinking_level.as_deref(),
                        ) != vulcan_session_core::v1::config::canonical_request_thinking_level(
                            request.thinking_level.as_deref(),
                        )
                        || active.temperature != request.temperature)
                {
                    return Ok(None);
                }
            }
            // Provider preparation consumes only the already active request configuration. Picker
            // writes remain pending until an explicit user-input or Continue boundary activates them.
            // Provider 准备仅消费已经活动的请求配置。选择器写入保持待生效，直到明确的用户输入或
            // Continue 边界激活它们。
            self.capture_provider_projection_snapshot_with_publisher_locked(projection_purpose)?
        };
        // Completed-result workers retain this same lease if the asynchronous builder is dropped.
        // 异步构建者被丢弃时，已完成结果工作线程保留同一租约。
        let preparation = Arc::new(SessionPhysicalPreparation::new(
            Arc::clone(self),
            lease,
            snapshot.clone(),
            projection_purpose,
        ));
        // Phase two may read media, upload or encode asynchronously after both synchronous guards are gone.
        // 第二阶段仅在两个同步守卫释放后，才可异步读取媒体、上传或编码。
        // Keep the potentially large media/encoding future off every enclosing async frame without spawning detached work.
        // 将可能较大的媒体及编码Future移出各层外围异步帧，不启动分离工作。
        // Soft budget control must pass the same frontier validation as a permit; stale routes never trigger compaction.
        // 软预算控制必须通过与许可相同的前沿校验；陈旧路由绝不能触发压缩。
        let prepared = match Box::pin(build(snapshot, Arc::clone(&preparation))).await {
            Ok(Some(prepared)) => Ok(prepared),
            Ok(None) => return Ok(None),
            Err(error @ SessionRuntimeError::ProviderContextCompactionRequired(_)) => Err(error),
            Err(error) => return Err(error),
        };
        // Only the official durable recapture can advance request proof after this preparation's own writes.
        // 本准备自身写入之后，只有正式可靠重捕获可以推进请求证明。
        let snapshot = preparation.last_snapshot()?;
        if let Ok(prepared) = &prepared
            && (prepared.projection.segment_generation != snapshot.segment_generation
                || prepared.projection.document_revision != snapshot.document_revision
                || prepared.projection.retained_from_seq != snapshot.retained_from_seq
                || prepared.projection.through_seq != snapshot.through_seq)
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
        self.ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
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
        // Only a current final-budget refusal may leave the preparation owner as host control flow.
        // 只有仍为当前状态的最终预算拒绝才能作为宿主控制流离开准备所有者。
        let prepared = prepared?;
        let expected_request = request.clone();
        let expected_projection = prepared.projection.clone();
        let expected_request_budget_evidence = prepared.request_budget_evidence.clone();
        // An initialized generation owns its persisted baseline. The newly assembled candidate is
        // authoritative only while the generation is fresh or explicitly refresh-pending.
        // 已初始化 generation 以其持久化基线为准；新组装候选只在 generation 为全新或明确待刷新时权威。
        let locked_baseline = snapshot
            .initialized_locked_baseline
            .unwrap_or(candidate_locked_baseline);
        let receipt_locked_baseline = locked_baseline.clone();
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
                            locked_baseline: receipt_locked_baseline,
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
                                locked_baseline: receipt_locked_baseline,
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
        self.capture_durable_projection(SessionProjectionPurpose::CompactionContext)
    }

    /// Captures a flushed snapshot for the specified projection purpose under the existing writer and publisher locks.
    /// 在既有写入与发布锁内，为指定投影用途捕获已落盘快照。
    /// Returns the exact source projection, or an admission, persistence, or projection error.
    /// 返回精确来源投影，或准入、持久化、投影错误。
    fn capture_durable_projection(
        &self,
        purpose: SessionProjectionPurpose,
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
        self.ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
        let revision = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?
            .memory_revision();
        self.flush_through_with_writer_and_publisher_locked(revision)?;
        self.capture_provider_projection_snapshot_with_publisher_locked(purpose)
    }

    /// Captures the complete durable Provider history used by a Continue-owned controller run.
    /// 捕获 Continue 所属控制器执行使用的完整可靠 Provider 历史。
    ///
    /// # Returns
    /// 返回值
    /// Returns the same serialized V1 projection frontier used by physical request preparation.
    /// 返回与物理请求准备使用的相同串行化 V1 投影前沿。
    pub(crate) fn capture_controller_projection_snapshot(
        &self,
    ) -> Result<SessionProviderProjectionSnapshot, SessionRuntimeError> {
        self.capture_durable_projection(SessionProjectionPurpose::ProviderRequest)
    }

    /// Captures the exact durable V1 Provider projection while publication is serialized.
    /// 在发布已串行化时捕获精确可靠的 V1 Provider 投影。
    fn capture_provider_projection_snapshot_with_publisher_locked(
        &self,
        purpose: SessionProjectionPurpose,
    ) -> Result<SessionProviderProjectionSnapshot, SessionRuntimeError> {
        let locator = self
            .locator
            .as_ref()
            .ok_or(SessionRuntimeError::ProviderProjectionRequiresStorageLocator)?;
        let (
            segment_generation,
            document_revision,
            retained_from_seq,
            context_frozen_at,
            remote_checkpoint,
            summary_compaction,
            through_seq,
            preparing_user_prompt_media_seqs,
            initialized_locked_baseline,
            current_compaction_id,
            open_turn_id,
            child_business_execution_id,
            dedicated_profile,
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
            // Both identities come from the same committed node; later requests never advance this frontier.
            // 两种身份来自同一已提交节点；后续请求绝不推进此前沿。
            let (retained_from_seq, context_frozen_at, remote_checkpoint, summary_compaction) =
                match document.runtime_lock.current_compaction_id {
                    Some(compaction_id) => document
                        .compactions
                        .iter()
                        .find(|node| node.compaction_id == compaction_id)
                        .map(|node| {
                            // The published node owns its opaque projection; no assistant event is synthesized.
                            // 已发布节点持有不透明投影；不合成助手事件。
                            let checkpoint = match &node.content {
                                vulcan_session_core::v1::context::CompactionContent::Summary { .. } => None,
                                vulcan_session_core::v1::context::CompactionContent::RemoteResponses { checkpoint } => Some(SourcedProjectedItem {
                                    user_prompt_parameters: None,
                                    source_from_seq: node.api_stats_seq,
                                    source_to_seq: node.api_stats_seq,
                                    item: ProjectedItem::RemoteCompactionCheckpoint {
                                        compaction_id: node.compaction_id,
                                        checkpoint: Box::new(checkpoint.clone()),
                                    },
                                }),
                            };
                            // Only a committed local summary owns a protocol-neutral continuation control.
                            // 只有已提交的本地摘要拥有与协议无关的续接控制消息。
                            let summary = matches!(
                                &node.content,
                                vulcan_session_core::v1::context::CompactionContent::Summary { .. }
                            ).then(|| node.clone());
                            (node.retained_from_seq, Some(node.api_stats_seq), checkpoint, summary)
                        })
                        .ok_or_else(|| {
                            SessionRuntimeError::ProviderProjectionInvariant(String::from(
                                "runtime compaction pointer has no matching durable node",
                            ))
                        })?,
                    None => (Seq::try_new(1)?, None, None, None),
                };
            (
                document.segment_generation(),
                document.memory_revision(),
                retained_from_seq,
                context_frozen_at,
                remote_checkpoint,
                summary_compaction,
                Seq::try_new(through_value)?,
                document.preparing_user_prompt_media_seqs(),
                LockedBaselineProjection::from_initialized_runtime_lock(&document.runtime_lock)?,
                document.runtime_lock.current_compaction_id,
                document
                    .active_turn()
                    .filter(|turn| !turn.closed)
                    .map(|turn| turn.turn_id),
                document
                    .metadata
                    .child_execution_profile
                    .as_ref()
                    .map(|profile| profile.business_execution_id.clone()),
                document.dedicated_profile().cloned(),
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
        // Planning may prune at the current frontier; ordinary continuation must reuse the committed frontier.
        // 规划可以在当前前沿裁剪；普通继续必须复用已提交前沿。
        let instruction_frontier = match purpose {
            SessionProjectionPurpose::ProviderRequest => context_frozen_at,
            SessionProjectionPurpose::CompactionContext => Some(through_seq),
        };
        // A dedicated session forgets closed turns started while its memory was locked.
        // 专职会话遗忘在记忆锁定期间开始且已闭合的回合。
        let dedicated_events = dedicated_profile
            .as_ref()
            .map(|profile| dedicated_provider_events(profile, &range.events, open_turn_id));
        let mut sourced_items = project_context_generation_items_with_sources(
            dedicated_events.as_deref().unwrap_or(&range.events),
            retained_from_seq,
            instruction_frontier,
        )?;
        // Derive the same fixed control for planning, controller restore and physical request preparation.
        // 为规划、控制器恢复与物理请求准备派生同一条固定控制消息。
        if let Some(summary) = summary_compaction {
            compaction_continuation::insert_summary_continuation(&mut sourced_items, &summary)?;
        }
        // The checkpoint replaces the discarded prefix and therefore precedes all retained history.
        // 检查点替代已丢弃前缀，因此位于全部保留历史之前。
        if let Some(checkpoint) = remote_checkpoint {
            sourced_items.insert(0, checkpoint);
        }
        let items = sourced_items
            .iter()
            .map(|projected| projected.item.clone())
            .collect();
        Ok(SessionProviderProjectionSnapshot {
            open_turn_id,
            child_business_execution_id,
            current_compaction_id,
            preparing_user_prompt_media_seqs,
            compaction_frontier_seq: context_frozen_at,
            segment_generation,
            document_revision,
            retained_from_seq,
            through_seq,
            items,
            sourced_items,
            project_directory: locator.project_directory(),
            provider_usage_budget_anchor: latest_provider_usage_budget_anchor(&range.events),
            initialized_locked_baseline,
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
                Some(ContinuationDispatchResolution::TerminalBeforeApi { projection, .. }) => {
                    return Err(SessionRuntimeError::ContinuationTerminatedBeforeApi {
                        client_action_id: action_id.clone(),
                        projection,
                    });
                }
                Some(ContinuationDispatchResolution::TerminalBeforeTransport {
                    api_call_id,
                    projection,
                    ..
                }) => {
                    return Err(SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                        client_action_id: action_id.clone(),
                        api_call_id,
                        projection,
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
                Some(ContinuationDispatchResolution::TerminalBeforeApi { projection, .. }) => {
                    return Err(SessionRuntimeError::ContinuationTerminatedBeforeApi {
                        client_action_id: action_id.clone(),
                        projection,
                    });
                }
                Some(ContinuationDispatchResolution::TerminalBeforeTransport {
                    api_call_id,
                    projection,
                    ..
                }) => {
                    return Err(SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                        client_action_id: action_id.clone(),
                        api_call_id,
                        projection,
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
            Some(ContinuationDispatchResolution::TerminalBeforeApi { projection, .. }) => {
                Err(SessionRuntimeError::ContinuationTerminatedBeforeApi {
                    client_action_id: action_id.clone(),
                    projection,
                })
            }
            Some(ContinuationDispatchResolution::TerminalBeforeTransport {
                api_call_id,
                projection,
                ..
            }) => Err(SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                client_action_id: action_id.clone(),
                api_call_id,
                projection,
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
        self.ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
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
            Some(ContinuationDispatchResolution::TerminalBeforeApi { projection, .. }) => {
                Err(SessionRuntimeError::ContinuationTerminatedBeforeApi {
                    client_action_id: action_id.clone(),
                    projection,
                })
            }
            Some(ContinuationDispatchResolution::TerminalBeforeTransport {
                api_call_id,
                projection,
                ..
            }) => Err(SessionRuntimeError::ContinuationTerminatedBeforeTransport {
                client_action_id: action_id.clone(),
                api_call_id,
                projection,
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
        // Read accounting inside the document transaction; accounting never locks documents.
        // 在文档事务中读取统计；统计模块永不获取文档锁。
        let usage_handle = self.clone();
        let session_id = self.session_id;
        let completion_receipt = self
            .clone()
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
                    let active_turn_origin = document
                        .active_turn()
                        .map(|turn| turn.origin)
                        .ok_or(DomainError::NoActiveTurn)?;
                    // Question executions and explicit stop states never create a second business completion.
                    // 问题执行与明确停止状态绝不创建第二次业务完成。
                    if active_turn_origin == TurnOrigin::QuestionExecution
                        || !matches!(
                            document.collaboration_run_control(),
                            CollaborationRunControl::Running
                        )
                    {
                        return Ok(SessionMutationChange::Unchanged(
                            SuccessfulTurnSettlement::Deferred,
                        ));
                    }
                    // Resume accepted business guidance through its own durable dispatch authority.
                    // 通过独立可靠分发权威继续已接纳业务方向。
                    if document.pending_collaboration_continuation().is_some() {
                        return Ok(SessionMutationChange::Unchanged(
                            SuccessfulTurnSettlement::Collaboration,
                        ));
                    }
                    // Keep the logical Turn open while Core still owns a reserved or active child task.
                    // 当 Core 仍持有预留或活动子任务时保持逻辑回合开启。
                    if !document
                        .completion_obligations(expected_turn_id)
                        .open_child_task_ids
                        .is_empty()
                    {
                        return Ok(SessionMutationChange::Unchanged(
                            SuccessfulTurnSettlement::Deferred,
                        ));
                    }
                    document.apply_atomic_transition(move |document| {
                        // An insertion that missed the final API terminal still reaches the model.
                        // 错过最后一次 API 终态的插入仍须送达模型。
                        let promoted = document.promote_pending_in_turn_inputs_after_api(now_ms)?;
                        if !promoted.is_empty() {
                            return Ok(SessionMutationChange::Changed(
                                SuccessfulTurnSettlement::PendingInTurn(promoted),
                            ));
                        }
                        let transition = document.complete_turn_with_lifetime_usage(
                            expected_turn_id,
                            now_ms,
                            usage_handle.goal_usage_snapshot().ok().as_ref(),
                        )?;
                        if matches!(&transition.successor, TurnSuccessor::None) {
                            // Natural idle is recorded in the same document mutation as Turn closure.
                            // 自然空闲与 Turn 闭合在同一次文档修改中记录。
                            let natural_idle_operation_id =
                                CollaborationOperationId::try_new(format!(
                                    "natural-idle:{}:{}",
                                    session_id,
                                    transition.completed_turn_final_seq.value()
                                ))?;
                            document.set_collaboration_run_control(SetRunControlInput {
                                operation_id: natural_idle_operation_id,
                                state: CollaborationRunControl::NaturallyIdle,
                                changed_at_ms: now_ms,
                            })?;
                        }
                        Ok(SessionMutationChange::Changed(
                            SuccessfulTurnSettlement::Completed(transition),
                        ))
                    })
                },
            )
            .await?;
        Ok(completion_receipt)
    }

    /// Durably appends one exact child return without numeric usage to the parent.
    /// 把一条精确子代理回流，不附带数字用量可靠追加到父会话。
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
    #[cfg(test)]
    pub(crate) fn append_subagent_return(
        &self,
        input: SubagentReturnInput,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<SubagentReturnReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::SubagentReturned,
            move |document| {
                document
                    .append_subagent_return(input, now_ms)
                    .map(session_change_from_idempotent)
            },
        )
    }

    /// Commits child content and its owned execution terminal fact under one publication lock.
    /// 在同一发布锁下提交子正文及其所属执行终态事实。
    /// `execution_id` is supplied by the detached listener; `input` is validated child content.
    /// `execution_id` 由分离监听器提供；`input` 是已校验子正文。
    /// Returns only after the combined revision is durable, or an explicit domain/storage error.
    /// 仅在组合修订落盘后返回，否则返回明确的领域或存储错误。
    pub(crate) fn append_execution_subagent_return(
        &self,
        execution_id: vulcan_session_core::v1::collaboration::CollaborationExecutionId,
        input: SubagentReturnInput,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<SubagentReturnReceipt>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::SubagentReturned,
            move |document| {
                // Native owned executions must publish both facts; unowned task calls keep content-only returns.
                // 原生所属执行必须发布两个事实；无所属预留的任务调用保持仅正文回流。
                if document
                    .collaboration_state()
                    .child_tasks
                    .contains_key(&execution_id)
                {
                    document.complete_child_task_with_return(execution_id, input, now_ms)
                } else {
                    document.append_subagent_return(input, now_ms)
                }
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

    /// Seals assistant text in memory together with its answer-time `ui-code` snapshots.
    /// 在内存中封口 assistant 正文，并附着其回答当时的 `ui-code` 快照。
    pub(crate) fn seal_assistant_text(
        &self,
        api_call_id: ApiCallId,
        text: impl Into<String>,
        status: AssistantSemanticStatus,
        code_snapshots: Vec<vulcan_session_core::v1::answer_components::AnswerCodeSnapshot>,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<Seq>, SessionRuntimeError> {
        let text = text.into();
        self.mutate_in_memory_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document
                    .append_assistant_text_with_code_snapshots(
                        api_call_id,
                        text,
                        status,
                        code_snapshots,
                        now_ms,
                    )
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

    /// Publishes the durable return of one approval-gated tool from WaitingUser to Running.
    /// 发布一个受审批管控的工具从 WaitingUser 可靠恢复为 Running。
    pub(crate) fn resume_waiting_tool(
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
                    .resume_waiting_tool(api_call_id, tool_call_id, now_ms)
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

    /// Activates the initial request intent staged after a newly created session root.
    /// 激活在新建会话根输入之后暂存的首个请求意图。
    ///
    /// # Parameters / 参数
    /// `now_ms` is the exact admission timestamp used by the request-switch fact.
    /// `now_ms` 是请求切换事实使用的精确准入时间戳。
    ///
    /// # Returns / 返回值
    /// Returns whether a pending request was consumed. Absence is an idempotent no-op.
    /// 返回是否消费了待生效请求；不存在待生效请求时为幂等 no-op。
    pub(crate) fn activate_pending_request_for_new_session_root(
        &self,
        now_ms: i64,
    ) -> Result<SessionConditionalMutationReceipt<Option<Seq>>, SessionRuntimeError> {
        self.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            move |document| {
                if document.metadata.config.pending_request.is_none() {
                    return Ok(SessionMutationChange::Unchanged(None));
                }
                let turn_id = document
                    .active_turn()
                    .map(|turn| turn.turn_id)
                    .ok_or(DomainError::NoActiveTurn)?;
                document
                    .promote_pending_request(Some(turn_id), now_ms)
                    .map(SessionMutationChange::Changed)
            },
        )
    }

    /// Reads complete durable history at a pinned writer frontier for cross-turn accounting.
    /// 在固定写入前沿读取完整可靠历史，用于跨回合统计。
    /// `reader` projects one value; returns storage or owner-consistency failures explicitly.
    /// `reader` 投影一个值；显式返回存储或所有者一致性错误。
    pub(crate) fn read_full_durable<T>(
        &self,
        reader: impl FnOnce(&SessionDocument) -> T,
    ) -> Result<T, SessionRuntimeError> {
        // Pin the storage writer before the publisher, matching mutation lock order.
        // 先固定存储写入器再固定发布器，与修改锁顺序一致。
        let _writer = self
            .writer
            .lock()
            .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
        // Prevent publication from advancing the frontier during the full read.
        // 防止完整读取期间发布推进前沿。
        let _publisher = self
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        // Flush exactly the resident revision before loading older segments.
        // 加载旧分段前精确刷写常驻修订。
        let revision = self
            .document
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?
            .memory_revision();
        self.flush_through_with_writer_and_publisher_locked(revision)?;
        self.ensure_not_quarantined()?;
        // Rebuild all durable segments, including history evicted from memory.
        // 重建全部可靠分段，包含已移出内存的历史。
        let loaded = load_session_v1_full(&self.storage_locator()?)?;
        // Reject any mismatch rather than mixing resident and stored generations.
        // 拒绝任何不一致，避免混用常驻与存储代际。
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
        Ok(reader(&loaded.document))
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
        self.ensure_admitted(admission)?;
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
        self.ensure_admitted(admission)?;
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
            self.ensure_admitted(admission)?;
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
            self.ensure_admitted(admission)?;
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
        let mut batch = ConversationEventWindowBatch::from_events(
            self.session_id.to_string(),
            window_generation,
            &receipt.changed_events,
        );
        // The published record advances even without a publisher, so it always equals the state
        // at the latest revision and a later append never builds on a skipped one.
        // 即使没有发布器，发布记录也会推进，使其始终等于最新修订处的状态，后续追加绝不建立在
        // 被跳过的修订之上。
        let text_appends = self.split_streaming_text_appends(window_generation, &mut batch.events);
        let Some(service_epoch) = self.live_service_epoch_best_effort() else {
            return;
        };
        match ConversationEventLiveDeltaV1::volatile_with_text_appends(
            batch,
            text_appends,
            service_epoch,
            base_memory_revision,
            receipt.revision,
        ) {
            Ok(delta) => self.publish_live_event_delta_best_effort(delta),
            Err(error) => tracing::warn!(
                "[session-v1-live] rejected invalid volatile revision edge: session={} error={error}",
                self.session_id
            ),
        }
    }

    /// Moves pure streaming-text extensions out of `events` and returns them as appends.
    /// 把纯流式正文延长从 `events` 中移出，并以追加形式返回。
    ///
    /// # Parameters
    /// 参数
    /// `window_generation` owns `events`, the complete changed events of one revision in seq order.
    /// `window_generation` 拥有 `events`，即单个修订按 seq 排序的完整变更事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns the appends in seq order; `events` keeps every event that must be sent complete.
    /// 按 seq 顺序返回追加；`events` 保留所有必须完整发送的事件。
    fn split_streaming_text_appends(
        &self,
        window_generation: u64,
        events: &mut Vec<ConversationEventSummary>,
    ) -> Vec<ConversationEventTextAppendV1> {
        let mut published = match self.published_streaming_text.lock() {
            Ok(published) => published,
            Err(_) => {
                tracing::warn!(
                    "[session-v1-live] streaming text record poisoned; publishing complete events: session={}",
                    self.session_id
                );
                return Vec::new();
            }
        };
        if published.window_generation != Some(window_generation) {
            published.window_generation = Some(window_generation);
            published.events.clear();
        }
        let mut text_appends = Vec::new();
        let mut complete = Vec::with_capacity(events.len());
        for event in events.drain(..) {
            let append = published
                .events
                .get(&event.seq)
                .and_then(|previous| ConversationEventTextAppendV1::between(previous, &event));
            if ConversationEventTextAppendV1::can_extend(&event) {
                published.events.insert(event.seq, event.clone());
            } else {
                published.events.remove(&event.seq);
            }
            match append {
                Some(append) => text_appends.push(append),
                None => complete.push(event),
            }
        }
        *events = complete;
        text_appends
    }

    /// Emits one live delta without allowing a presentation failure to roll back domain state.
    /// 发出一份实时增量，且绝不允许显示层失败回滚领域状态。
    fn publish_live_event_delta_best_effort(&self, delta: ConversationEventLiveDeltaV1) {
        let publisher = match self.live_event_publisher.lock() {
            Ok(publisher) => publisher.clone(),
            Err(_) => {
                tracing::warn!(
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
            tracing::warn!(
                "[session-v1-live] rejected stale service epoch: session={} expected={} actual={}",
                self.session_id,
                publisher.service_epoch,
                delta.service_epoch
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
                tracing::warn!(
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
            Err(error) => tracing::warn!(
                "[session-v1-live] rejected generation rebase edge: session={} generation={} error={error}",
                self.session_id,
                window_generation
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
            Ok(mut delta) => {
                // Use the flushed candidate, never a potentially newer in-memory answer mode.
                // 使用已落盘候选，绝不使用可能更新的内存回答模式。
                delta.answer_mode = Some(Box::new(candidate.metadata.config.answer_mode.clone()));
                // Access travels the same way, so a permission change reaches every open window.
                // 授权以相同方式下发，使权限修改到达每个打开的窗口。
                delta.access = Some(Box::new(ConversationSessionAccessV1::from_access(
                    &candidate.metadata.mode.access,
                )));
                self.publish_live_event_delta_best_effort(delta);
            }
            Err(error) => tracing::warn!(
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

    /// Reserves and completes the final write-side fence before physical lifecycle deletion.
    /// 在物理生命周期删除前预留并完成最终写入侧封存。
    ///
    /// The reservation closes new admissions before waiting for the writer and publisher locks.
    /// Existing mutations that already crossed the publisher boundary are allowed to finish, and
    /// the authoritative document is checked again while both serialization locks are held.
    /// 预留会在等待写入器和发布器锁前关闭新准入；已经越过发布器边界的修改允许完成，
    /// 并在同时持有两个串行锁时再次检查权威文档。
    pub(crate) fn seal_for_deletion(&self) -> Result<(), SessionRuntimeError> {
        {
            let mut state = self
                .sealed_for_deletion
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
            if *state != SessionDeletionState::Open {
                return Err(SessionRuntimeError::SessionSealed(self.session_id));
            }
            *state = SessionDeletionState::Sealing;
        }

        let result = (|| {
            self.ensure_not_quarantined()?;
            let _writer = self
                .writer
                .lock()
                .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
            let _publisher = self
                .mutation_publisher
                .lock()
                .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
            self.ensure_not_quarantined()?;
            let unsettled = self
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?
                .has_unsettled_collaboration_for_edit_or_delete();
            if unsettled {
                return Err(SessionRuntimeError::UnsettledCollaboration(self.session_id));
            }
            let mut state = self
                .sealed_for_deletion
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
            if *state != SessionDeletionState::Sealing {
                return Err(SessionRuntimeError::SessionSealed(self.session_id));
            }
            *state = SessionDeletionState::Sealed;
            Ok(())
        })();

        if result.is_err()
            && let Ok(mut state) = self.sealed_for_deletion.lock()
            && *state == SessionDeletionState::Sealing
        {
            *state = SessionDeletionState::Open;
        }
        result
    }

    /// Reopens a deletion-reserved handle after physical deletion failed.
    /// 物理删除失败后重新打开已预留删除的句柄。
    pub(crate) fn reopen_after_failed_deletion(&self) -> Result<(), SessionRuntimeError> {
        let mut state = self
            .sealed_for_deletion
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        if *state != SessionDeletionState::Sealed {
            return Err(SessionRuntimeError::SessionSealed(self.session_id));
        }
        *state = SessionDeletionState::Open;
        Ok(())
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
        let deletion_state = self
            .sealed_for_deletion
            .lock()
            .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
        if *deletion_state != SessionDeletionState::Open {
            return Err(SessionRuntimeError::SessionSealed(self.session_id));
        }
        drop(deletion_state);
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

/// Exclusive process-local recovery ownership, released on completion, cancellation or unwind.
/// 独占进程内恢复所有权，在完成、取消或栈展开时释放。
pub(crate) struct SessionCollaborationRecoveryGuard {
    /// Exact registry instance, independent of a later service registry replacement.
    /// 精确注册表实例，独立于后续服务注册表替换。
    registry: Arc<SessionRuntimeRegistry>,
}

impl Drop for SessionCollaborationRecoveryGuard {
    /// Releases only this registry's singleton worker slot without touching canonical documents.
    /// 只释放本注册表的单实例任务槽，不触碰规范文档。
    fn drop(&mut self) {
        match self.registry.state.lock() {
            Ok(mut state) => state.collaboration_recovery_running = false,
            Err(error) => {
                tracing::error!(%error, "Collaboration recovery ownership could not be released")
            }
        }
    }
}

/// Mutable registry state serialized by the registry-owned mutex.
/// 由注册表自有互斥锁串行化的可变注册表状态。
#[derive(Default)]
struct SessionRuntimeRegistryState {
    /// One retained collaboration recovery worker per canonical registry instance.
    /// 每个规范注册表实例保留唯一协作恢复任务。
    collaboration_recovery_running: bool,
    /// Registered handles keyed by stable application session id.
    /// 按稳定应用会话 id 索引的已注册句柄。
    handles: BTreeMap<SessionId, Arc<SessionHandle>>,
    /// Exact sessions currently owned by one physical loader or creator.
    /// 当前由一个物理加载器或创建器拥有的精确会话。
    loading: BTreeMap<SessionId, Arc<SessionLoadGate>>,
    /// Per-session lifecycle serializers retained independently of loaded handles.
    /// 独立于已加载句柄保留的单会话生命周期串行器。
    lifecycle_gates: BTreeMap<SessionId, Arc<AsyncMutex<()>>>,
    /// Sessions reserved for final deletion; no new canonical owner may load them.
    /// 已预留最终删除的会话；不得再加载新的规范 owner。
    deleting: BTreeSet<SessionId>,
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
        if state.deleting.contains(&self.session_id) {
            return Err(SessionRuntimeError::SessionSealed(self.session_id));
        }
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
    /// Reserves the only recovery worker for this registry until its returned guard is dropped.
    /// 在返回的守卫释放前，为本注册表预留唯一恢复任务。
    /// No work is started here; concurrent callers receive no guard and cannot replace its owner.
    /// 此处不启动工作；并发调用方不会取得守卫，也不能替换其所有者。
    pub(crate) fn try_reserve_collaboration_recovery(
        self: &Arc<Self>,
    ) -> Result<Option<SessionCollaborationRecoveryGuard>, SessionRuntimeError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
        if state.collaboration_recovery_running {
            return Ok(None);
        }
        state.collaboration_recovery_running = true;
        Ok(Some(SessionCollaborationRecoveryGuard {
            registry: Arc::clone(self),
        }))
    }
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

    /// Reserves one canonical handle for final deletion before unregistering it.
    /// 在注销规范句柄前为最终删除预留一个规范 owner。
    ///
    /// The registry tombstone blocks cold loads while the handle seal waits for already admitted
    /// writes. The caller must hold the session lifecycle transition guard.
    /// 注册表墓碑会在句柄封存等待已准入写入时阻止冷加载；调用方必须持有会话生命周期转换守卫。
    pub(crate) fn begin_lifecycle_deletion(
        &self,
        session_id: SessionId,
        expected_handle: &Arc<SessionHandle>,
    ) -> Result<(), SessionRuntimeError> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
            let Some(handle) = state.handles.get(&session_id) else {
                return Err(SessionRuntimeError::SessionNotRegistered(session_id));
            };
            if !Arc::ptr_eq(handle, expected_handle) {
                return Err(SessionRuntimeError::SessionNotRegistered(session_id));
            }
            if !state.deleting.insert(session_id) {
                return Err(SessionRuntimeError::SessionSealed(session_id));
            }
        }

        match expected_handle.seal_for_deletion() {
            Ok(()) => Ok(()),
            Err(error) => {
                if let Ok(mut state) = self.state.lock() {
                    state.deleting.remove(&session_id);
                }
                Err(error)
            }
        }
    }

    /// Restores one deletion-reserved handle after physical deletion failed.
    /// 物理删除失败后恢复一个已预留删除的句柄。
    pub(crate) fn abort_lifecycle_deletion(
        &self,
        session_id: SessionId,
        handle: Arc<SessionHandle>,
    ) -> Result<(), SessionRuntimeError> {
        let should_resume_root = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
            state.deleting.remove(&session_id);
            if let Some(existing) = state.handles.get(&session_id) {
                // An unregister failure may happen before removal, so restoring the same canonical
                // owner is already complete and must not be reported as a duplicate failure.
                // 注销失败可能发生在移除前；若仍是同一规范 owner，恢复动作已完成，不应误报重复会话。
                if !Arc::ptr_eq(existing, &handle) {
                    return Err(SessionRuntimeError::DuplicateSession(session_id));
                }
            } else {
                state.handles.insert(session_id, Arc::clone(&handle));
            }
            !state.shutting_down
        };
        handle.reopen_after_failed_deletion()?;
        if should_resume_root {
            handle.resume_after_failed_shutdown()?;
        }
        Ok(())
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
        if state.deleting.contains(&session_id) {
            return Err(SessionRuntimeError::SessionSealed(session_id));
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
        let session_id = handle.session_id();
        if state.deleting.contains(&session_id) {
            return Err(SessionRuntimeError::SessionSealed(session_id));
        }
        if state.shutting_down {
            return Err(SessionRuntimeError::ShuttingDown);
        }
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
        let state = self
            .state
            .lock()
            .map_err(|_| SessionRuntimeError::RegistryLockPoisoned)?;
        if state.deleting.contains(&session_id) {
            return Err(SessionRuntimeError::SessionSealed(session_id));
        }
        state
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
                if state.deleting.contains(&session_id) {
                    return Err(SessionRuntimeError::SessionSealed(session_id));
                }
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
            if state.deleting.contains(&session_id) {
                return Err(SessionRuntimeError::SessionSealed(session_id));
            }
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
            if state.deleting.contains(&session_id) {
                return Err(SessionRuntimeError::SessionSealed(session_id).to_string());
            }
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

    /// Reports whether registry-wide root admission is currently closed.
    /// 报告注册表级根回合准入当前是否关闭。
    ///
    /// # Returns
    /// 返回值
    /// `true` after shutdown begins and before a failed shutdown is resumed.
    /// 关闭开始后、失败关闭恢复前返回 `true`。
    pub(crate) fn shutdown_admission_is_closed(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.shutting_down)
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests;
