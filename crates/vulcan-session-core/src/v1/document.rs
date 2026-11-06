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

use crate::v1::config::{IdFrontier, MetadataDocument, SegmentInfo};
use crate::v1::context::{CompactionNode, RuntimeLock};
use crate::v1::errors::DomainError;
use crate::v1::event::*;
use crate::v1::ids::{
    ApiCallId, ClientActionId, CompactionId, IdAllocator, JS_SAFE_INTEGER_MAX, Seq, ToolCallId,
    TurnId,
};
use crate::v1::projection::validate_system_context_event;

// Preserve the existing test fixture namespace without production-only unused imports.
// 保留现有测试夹具命名空间，避免在生产构建中引入仅供测试使用的导入。
#[cfg(test)]
use crate::v1::budget::ProviderRequestBudgetEvidence;
#[cfg(test)]
use crate::v1::config::{
    DraftState, GoalConfig, KnowledgeLibrarySelectionConfig, KnowledgeLibrarySelectionMode,
    VmmSessionPolicyConfig,
};
#[cfg(test)]
use crate::v1::context::{LockedBaselineProjection, RuntimeInitializationState};
#[cfg(test)]
use crate::v1::ids::{PendingInputId, RequestStreamId, ResourceId, SubagentTaskId};
#[cfg(test)]
use crate::v1::stats::TurnStatBuckets;
#[cfg(test)]
use vulcan_change_set::PersistentChangeSet;

mod answer_mode;
mod api;
mod business_hook;
mod child_creation;
mod child_execution;
mod collaboration;
mod compaction;
mod dedicated;
mod dynamic_context;
mod generation;
mod hydration;
mod input;
mod input_handoff;
mod modes;
mod permission_mode;
mod persistence;
mod provider_source;
mod recovery;
mod resident_events;
mod selection;
mod shell_job_continuation;
mod subagent;
mod terminal_settlement;
mod tools;
mod turns;
mod user_prompt;

pub use answer_mode::ANSWER_MODE_CONTEXT_KEY;
pub use api::{
    ApiCompletion, ApiGroupCompletionReceipt, ApiTransportObservedReceipt,
    ContinuationDispatchResolution, ContinuationTerminalProjection, PrepareAndStartApiInput,
    PreparedApiStart,
};
pub use child_execution::ChildContextMaintenanceSource;
pub use collaboration::ChildTaskNonSuccessInput;
pub use compaction::CompactionCommitInput;
pub use dedicated::{DEDICATED_MEMORY_LOCKED_REASON, DEDICATED_MEMORY_UNLOCKED_REASON};
pub use input::{
    ClientInputSubmission, DraftMutationReceipt, EnqueueInputDisposition, EnqueueInputReceipt,
    QueuedInputDraftReceipt, QueuedInputOrderReceipt, QueuedInputPendingTurnReceipt,
    QueuedInputRemovalReceipt,
};
pub use modes::{GoalModeTransition, PlanModeTransitionInput};
pub use persistence::{DirtyFile, FlushCandidate, MemoryRevisionReceipt};
pub use selection::SessionSelectionBundleInput;
pub use shell_job_continuation::{SHELL_JOB_CONTINUATION_CONTEXT_KEY, ShellJobContinuationStart};
pub use subagent::{SubagentReturnBuildRequest, SubagentReturnInput, SubagentReturnReceipt};
pub use tools::{
    PreparedToolBatchReceipt, PreparedToolReceipt, ToolCompletion, ToolCompletionReceipt,
    ToolPreparation,
};
pub use turns::{
    ContinuationRequest, ContinueTurnReceipt, TurnCompletionTransition, TurnSuccessor,
};

#[cfg(test)]
use api::validate_api_terminal_facts;
use api::validate_execution_control_event;
use input::validate_user_input;
use resident_events::ResidentEvents;
pub(super) use selection::validate_provider_tool_selection_config;
#[cfg(test)]
use tools::tool_status_name;

pub use generation::{
    EditGenerationInput, ForkGenerationInput, GenerationBuildOutput, RootInputReplacement,
};

/// Where one turn came from.
/// 一个回合的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOrigin {
    /// A root user prompt opened the turn.
    /// 根用户提示词开启回合。
    UserRoot,
    /// A durably accepted agent task opened its own business turn.
    /// 已可靠接纳的代理任务开启自己的业务回合。
    AgentTask,
    /// Goal auto-continue opened the turn.
    /// Goal 自动续跑开启回合。
    GoalContinuation,
    /// Background shell jobs finished while the session was idle and woke it.
    /// 会话空闲时后台 shell 任务结束并唤醒了会话。
    ShellJobContinuation,
    /// A durable child result opened a follow-up turn after the parent had finished.
    /// 父回合结束后，可靠子任务结果开启后续处理回合。
    SubagentReturn,
    /// A dedicated auxiliary turn answered a temporary collaboration question.
    /// 专用辅助回合回答临时协作问题。
    QuestionExecution,
    /// An independent auxiliary execution maintains an idle root or completed child's context.
    /// 独立辅助执行维护空闲主会话或已完成子任务的上下文。
    ContextMaintenance,
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
    events: ResidentEvents,
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
    /// Private revision reserved by an unpublished composite mutation; never serialized or issued to a writer.
    /// 未发布复合修改预留的私有修订，绝不序列化或交给写入器。
    staged_commit_revision: Option<u64>,
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
            events: ResidentEvents::default(),
            tool_seq_by_id: HashMap::new(),
            active_turn: None,
            active_api: None,
            id_allocator: IdAllocator::new(),
            memory_revision: 0,
            staged_commit_revision: None,
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
        let mut document =
            Self::assemble_parts(metadata, runtime_lock, compactions, segment_catalog, events);
        document.rebuild_derived_state()?;
        document.validate_assembled_parts()?;
        Ok(document)
    }

    /// Builds one document read from storage, quarantining replay checkpoints whose fingerprint no
    /// longer verifies instead of refusing the whole session.
    /// 从存储读取的部件构建文档；指纹已无法校验的重放检查点被隔离，而不是拒绝整个会话。
    ///
    /// A checkpoint only carries Provider-native continuation data (`provider_source`); the events
    /// themselves are untouched. A quarantined checkpoint is dropped from the resident copy, so its
    /// API group is replayed as plain projected content. Every other validation stays strict.
    /// 检查点只携带 Provider 原生续接数据(`provider_source`)，事件本身不受影响。被隔离的检查点从
    /// 常驻副本中移除，其 API 组改以普通投影内容重放；其他校验保持严格。
    ///
    /// # Returns
    /// 返回值
    /// The document and the hosting seqs of every quarantined checkpoint, in discovery order.
    /// 返回文档以及每个被隔离检查点的承载 seq(按发现顺序)。
    pub fn from_stored_parts(
        metadata: MetadataDocument,
        runtime_lock: RuntimeLock,
        compactions: Vec<CompactionNode>,
        segment_catalog: Vec<SegmentInfo>,
        events: Vec<ConversationEvent>,
    ) -> Result<(Self, Vec<u64>), DomainError> {
        let mut document =
            Self::assemble_parts(metadata, runtime_lock, compactions, segment_catalog, events);
        let mut quarantined = Vec::new();
        loop {
            match document.rebuild_derived_state() {
                Ok(()) => break,
                Err(error @ DomainError::ProviderSourceFingerprintMismatch { seq, .. }) => {
                    // Each pass removes one checkpoint, so the loop ends after at most one pass per checkpoint.
                    // 每轮移除一个检查点，因此循环至多按检查点数量执行。
                    if !document.drop_resident_provider_source(seq) {
                        return Err(error);
                    }
                    quarantined.push(seq);
                }
                Err(error) => return Err(error),
            }
        }
        document.validate_assembled_parts()?;
        Ok((document, quarantined))
    }

    /// Removes the replay checkpoint hosted by one resident event.
    /// 移除一个常驻事件承载的重放检查点。
    /// Returns whether the event existed and carried a checkpoint.
    /// 返回该事件是否存在且带有检查点。
    fn drop_resident_provider_source(&mut self, seq: u64) -> bool {
        let Some(index) = self
            .events
            .iter()
            .position(|event| event.seq.value() == seq)
        else {
            return false;
        };
        let Some(event) = self.events.get_mut(index) else {
            return false;
        };
        let source = match &mut event.event {
            EventBody::AssistantReasoning(reasoning) => &mut reasoning.provider_source,
            EventBody::AssistantText(text) => &mut text.provider_source,
            EventBody::ToolCall(tool) => &mut tool.provider_source,
            _ => return false,
        };
        source.take().is_some()
    }

    /// Wraps loaded parts into a document whose derived state is not yet rebuilt.
    /// 把已加载部件包装为尚未重建派生状态的文档。
    fn assemble_parts(
        metadata: MetadataDocument,
        runtime_lock: RuntimeLock,
        compactions: Vec<CompactionNode>,
        segment_catalog: Vec<SegmentInfo>,
        events: Vec<ConversationEvent>,
    ) -> Self {
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
        Self {
            metadata,
            runtime_lock,
            compactions,
            events: ResidentEvents::new(events),
            tool_seq_by_id: HashMap::new(),
            active_turn: None,
            active_api: None,
            id_allocator: IdAllocator::new(),
            memory_revision: persisted_revision,
            staged_commit_revision: None,
            durable_revision: persisted_revision,
            dirty_since: BTreeMap::new(),
            changed_event_seqs_by_revision: BTreeMap::new(),
            issued_revisions: BTreeSet::new(),
            published_compaction_id,
            pending_compaction_publish: BTreeMap::new(),
            segment_catalog,
            resident_start_seq,
        }
    }

    /// Runs the whole-document validations that follow a derived-state rebuild of loaded parts.
    /// 在已加载部件的派生状态重建之后执行整文档校验。
    fn validate_assembled_parts(&self) -> Result<(), DomainError> {
        self.validate_collaboration_state()?;
        self.validate_child_execution_profile()?;
        self.validate_child_creation_intents()?;
        self.validate_business_hook_binding()?;
        if let Some(record) = self.terminal_settlement_record() {
            self.terminal_settlement_has_terminal_fact(record)?;
        }
        dynamic_context::validate_dynamic_context_map(&self.metadata.dynamic_context)?;
        self.validate_runtime_initialization_state()?;
        self.validate_compaction_state()?;
        self.validate_loaded_metadata()?;
        Ok(())
    }

    /// Returns the last revision confirmed durable.
    /// 返回最近一次被确认持久化的修订号。
    pub fn durable_revision(&self) -> u64 {
        self.durable_revision
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

    /// Records the memory recall of the active turn at most once.
    /// 为活动回合最多记录一次记忆召回。
    /// Returns None when the turn already carries its recall record.
    /// 回合已带有召回记录时返回 None。
    pub fn append_memory_recall(
        &mut self,
        turn_id: TurnId,
        recall: MemoryRecallEvent,
        now_ms: i64,
    ) -> Result<Option<Seq>, DomainError> {
        let recorded = self.events.iter().any(|event| {
            event.turn_id == Some(turn_id) && matches!(event.event, EventBody::MemoryRecall(_))
        });
        if recorded {
            return Ok(None);
        }
        self.append_event(
            Some(turn_id),
            EventVisibility::UiOnly,
            EventBody::MemoryRecall(recall),
            now_ms,
        )
        .map(Some)
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

    /// Requires one open (not closed) turn.
    /// 要求存在一个开放(未闭合)回合。
    fn require_open_turn(&self) -> Result<&ActiveTurnState, DomainError> {
        match &self.active_turn {
            Some(turn) if !turn.closed => Ok(turn),
            _ => Err(DomainError::NoActiveTurn),
        }
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
        // Generic append never creates Turns; only the staging APIs may allocate a new owner.
        // 通用追加绝不创建回合；只有暂存 API 才能分配新的 owner。
        if let Some(turn_id) = turn_id
            && self
                .active_turn
                .as_ref()
                .is_none_or(|turn| turn.closed || turn.turn_id != turn_id)
        {
            return Err(DomainError::TurnClosureMismatch {
                turn_id: turn_id.value(),
            });
        }
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
        // IDs are monotonic: a previous or closed current Turn cannot own a new event.
        // ID 单调递增：更早回合或已结束的当前回合不能拥有新事件。
        if let Some(turn_id) = turn_id
            && let Some(current) = &self.active_turn
            && (turn_id < current.turn_id || (turn_id == current.turn_id && current.closed))
        {
            return Err(DomainError::TurnClosureMismatch {
                turn_id: turn_id.value(),
            });
        }
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

    /// Returns the revision to be published by the current mutation, including nested draft operations.
    /// 返回当前修改将发布的修订，包含嵌套草稿操作。
    /// No state changes; overflow is rejected before any event or metadata mutation.
    /// 不修改状态；在修改事件或元数据之前拒绝溢出。
    fn next_mutation_revision(&self) -> Result<u64, DomainError> {
        match self.staged_commit_revision {
            Some(revision) => Ok(revision),
            None => self
                .memory_revision
                .checked_add(1)
                .ok_or_else(|| DomainError::invalid_id("document_revision", self.memory_revision)),
        }
    }

    /// Executes related domain transitions as one failure-atomic, single-revision publication.
    /// 将相关领域转换作为一次失败原子、单修订的发布执行。
    /// `transition` may call document mutations but must not issue flushes or acknowledge persistence.
    /// `transition` 可调用文档修改，但不得发出 flush 或确认持久化。
    /// Returns the domain result after installing the draft; any error leaves the live owner unchanged.
    /// 安装草稿后返回领域结果；任何错误都保持当前所有者不变。
    pub fn apply_atomic_transition<T>(
        &mut self,
        transition: impl FnOnce(&mut Self) -> Result<T, DomainError>,
    ) -> Result<T, DomainError> {
        let entry_memory_revision = self.memory_revision;
        let entry_durable_revision = self.durable_revision;
        let entry_issued_revisions = self.issued_revisions.clone();
        // Draft revisions and embedded receipts use the same reserved publication identity.
        // 草稿修订及其内嵌回执使用同一个预留发布身份。
        let mut staged = self.begin_atomic_mutation()?;
        // The result cannot escape until every domain transition succeeds.
        // 只有全部领域转换成功后，结果才可离开草稿。
        let result = transition(&mut staged)?;
        if staged.memory_revision != entry_memory_revision
            && Some(staged.memory_revision) != staged.staged_commit_revision
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "atomic_transition",
                field: "document_revision",
            });
        }
        if staged.durable_revision != entry_durable_revision
            || staged.issued_revisions != entry_issued_revisions
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "atomic_transition",
                field: "persistence_boundary",
            });
        }
        staged.commit();
        Ok(result)
    }

    /// Opens a composite in-place draft whose suboperations share one future publication revision.
    /// 打开原地复合草稿，其子操作共享一个待发布修订。
    /// Returns the guard or an overflow error; dropping the guard uncommitted rolls back every staged suboperation.
    /// 返回守卫或溢出错误；未提交即丢弃守卫会回滚全部暂存子操作。
    fn begin_atomic_mutation(&mut self) -> Result<AtomicMutation<'_>, DomainError> {
        // Nested drafts inherit the original publication identity, including receipt-embedded revisions.
        // 嵌套草稿继承原始发布身份，包括回执内部的修订字段。
        let revision = self.next_mutation_revision()?;
        // The snapshot excludes the event stream, which journals its own minimal undo frame.
        // 快照不含事件流；事件流自行记录最小撤销帧。
        let mut events = std::mem::take(&mut self.events);
        let rollback = Box::new(self.clone());
        events.open_frame();
        self.events = events;
        self.staged_commit_revision = Some(revision);
        Ok(AtomicMutation {
            document: self,
            rollback: Some(rollback),
        })
    }
}

/// Exclusive in-place draft of one failure-atomic composite mutation.
/// 一次失败原子复合修改的独占原地草稿。
/// Mutations go straight to the live document; `commit` keeps them, while dropping the guard
/// uncommitted (an error return or an unwind) restores the exact entry state.
/// 修改直接作用于当前文档；`commit` 保留修改，未提交即丢弃守卫(错误返回或展开)会恢复精确入口状态。
struct AtomicMutation<'a> {
    /// The live owner being mutated in place.
    /// 正在原地修改的当前所有者。
    document: &'a mut SessionDocument,
    /// Entry state of every field except the self-journaling event stream; `None` once committed.
    /// 除自记录事件流外全部字段的入口状态；提交后为 `None`。
    rollback: Option<Box<SessionDocument>>,
}

impl AtomicMutation<'_> {
    /// Keeps every staged suboperation and restores the enclosing mutation's revision reservation.
    /// 保留全部暂存子操作，并恢复外层修改的修订预留。
    fn commit(mut self) {
        let rollback = self
            .rollback
            .take()
            .expect("an uncommitted atomic mutation keeps its rollback state");
        self.document.staged_commit_revision = rollback.staged_commit_revision;
        self.document.events.commit_frame();
    }
}

impl std::ops::Deref for AtomicMutation<'_> {
    type Target = SessionDocument;

    fn deref(&self) -> &SessionDocument {
        self.document
    }
}

impl std::ops::DerefMut for AtomicMutation<'_> {
    fn deref_mut(&mut self) -> &mut SessionDocument {
        self.document
    }
}

impl Drop for AtomicMutation<'_> {
    fn drop(&mut self) {
        if let Some(rollback) = self.rollback.take() {
            let mut events = std::mem::take(&mut self.document.events);
            events.rollback_frame();
            *self.document = *rollback;
            self.document.events = events;
        }
    }
}

impl SessionDocument {
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
        self.memory_revision = match self.staged_commit_revision {
            Some(revision) => revision,
            None => self.memory_revision + 1,
        };
        let revision = self.memory_revision;
        self.metadata.document_revision = revision;
        let changed: Vec<Seq> = changed_event_seqs
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        // Every staged suboperation contributes its exact changed events to the one public receipt.
        // 每个暂存子操作将精确变化事件汇入同一份公开回执。
        let receipt = self
            .changed_event_seqs_by_revision
            .entry(revision)
            .or_default();
        receipt.extend(changed);
        receipt.sort_unstable();
        receipt.dedup();
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
}

/// Validates one exact external identity without normalization or fallback.
/// 校验一个不做归一化或回退的精确外部身份。
fn validate_exact_external_identity(field: &'static str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() || value.trim() != value {
        return Err(DomainError::InvalidStatistics { field });
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

/// Builds one stable client-action semantic conflict.
/// 构建一条稳定的客户端动作语义冲突。
fn client_action_conflict(client_action_id: &ClientActionId) -> DomainError {
    DomainError::ClientActionConflict {
        action_id: client_action_id.as_str().to_string(),
    }
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

/// Validates a persisted Unix-millisecond timestamp.
/// 校验一个持久化 Unix 毫秒时间戳。
fn validate_timestamp(field: &'static str, value: i64) -> Result<(), DomainError> {
    if value < 0 {
        return Err(DomainError::InvalidTimestamp { field, value });
    }
    Ok(())
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
