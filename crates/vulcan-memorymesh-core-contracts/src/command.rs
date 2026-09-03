//! Public command, result, and lifecycle contracts.
//! 公共命令、结果与生命周期契约。

use crate::{
    CandidateQueryBoundary, CommandId, CommitReceipt, ConfigSnapshotId, CoreError,
    DroppedCandidate, ExportDataset, MemoryCandidateScope, MemoryCategory, MemoryContextEvidence,
    MemoryId, MemoryLevel, MemoryRecord, MemoryScopeClass, MutationIdentity,
    PreCheckReviewCandidate, ProfileBundle, ProfileNode, ProfileNodeId, ProfileView,
    ProfileViewSection, RetentionPlanSet, RetentionPolicy, RetiredProfileNode, ScopeAccess,
    ScopeId, SearchHit, SessionSourceId, SubjectId, TimelineItem, Turn, TurnId, UnixNanos,
    VectorSpaceId,
};
use serde::{Deserialize, Serialize};

/// Selects how aggressively `PreCheck` recalls memory.
/// 选择 `PreCheck` 召回记忆的积极程度。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum RecallMode {
    /// Lets Core decide whether recall is necessary.
    /// 让 Core 决定是否需要召回。
    Auto = 1,
    /// Forces recall unless input validation rejects the command.
    /// 除非输入校验拒绝命令，否则强制召回。
    Force = 2,
    /// Disables recall while preserving local validation.
    /// 禁用召回但保留本地校验。
    Disabled = 3,
}

/// Selects whether `PreCheck` applies the durable compact boundary of the current session.
/// 选择 `PreCheck` 是否应用当前会话的持久压缩边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum SessionRecallMode {
    /// Restricts current-session turn-derived memories to the latest compacted boundary.
    /// 将当前会话中源自轮次的记忆限制在最新压缩边界内。
    CompactAware = 1,
    /// Searches all otherwise-visible memories without a session compact boundary.
    /// 在不应用会话压缩边界的情况下搜索全部其他可见记忆。
    Full = 2,
}

/// Reports one durable session compact-boundary decision.
/// 报告一次持久会话压缩边界决策。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompactResult {
    /// States whether the resolved session acknowledgement was accepted.
    /// 表示已解析会话确认是否被接受。
    pub accepted: bool,
    /// States whether durable boundary state changed.
    /// 表示持久边界状态是否发生变化。
    pub updated: bool,
    /// Identifies the active compact boundary when at least one turn exists.
    /// 当至少存在一个轮次时标识当前压缩边界。
    pub compacted_turn_id: Option<TurnId>,
    /// Carries the durable receipt of the compact-boundary mutation.
    /// 携带压缩边界变更的持久提交收据。
    pub receipt: CommitReceipt,
}

/// Requests memory recall before the host submits a user turn to a model.
/// 请求在宿主将用户轮次提交给模型前召回记忆。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreCheckCommand {
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read and write scope boundaries.
    /// 定义读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Identifies the host session source.
    /// 标识宿主会话来源。
    pub session_source_id: SessionSourceId,
    /// Identifies the embedding vector space used by recall.
    /// 标识召回使用的嵌入向量空间。
    pub vector_space_id: VectorSpaceId,
    /// Carries the current raw user content.
    /// 携带当前原始用户内容。
    pub user_content: String,
    /// Carries recent canonical timeline context.
    /// 携带最近的规范化时间线上下文。
    pub recent_timeline: Vec<TimelineItem>,
    /// Selects recall behavior.
    /// 选择召回行为。
    pub recall_mode: RecallMode,
    /// Selects current-session compact-boundary behavior independently from recall intent.
    /// 独立于召回意图选择当前会话压缩边界行为。
    pub session_recall_mode: SessionRecallMode,
    /// Restricts physical recall to the exact Host-resolved team, space, or project scope.
    /// 将物理召回限制在 Host 精确解析的团队、空间或项目作用域。
    pub candidate_scope: MemoryCandidateScope,
    /// Limits generated search queries.
    /// 限制生成的搜索查询数量。
    pub max_search_queries: u32,
    /// Limits returned memories.
    /// 限制返回的记忆数量。
    pub max_memories: u32,
    /// Carries the business evaluation time that governs read-time decay.
    /// 携带管理读时衰减的业务评估时间。
    pub now_ns: UnixNanos,
    /// Carries the persistent identity authorizing adoption reinforcement writes.
    /// 携带授权采纳强化写入的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Preserves the source request shape before Host flattens it into an ordered timeline.
/// 在 Host 将请求展平成有序时间线前保留来源请求形态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum PostActionInputKind {
    /// Contains a user message and optional final assistant reply, without intermediate messages.
    /// 包含用户消息与可选最终助手回复，不含中间消息。
    SingleTurn = 1,
    /// Contains source intermediate messages and therefore bypasses single-turn Noise gating.
    /// 来源含中间消息，因此跳过单轮 Noise 门控。
    Timeline = 2,
}

/// Requests durable intake of one completed turn with immediate acknowledgement.
/// 请求对一个完整轮次进行持久登记并立即确认。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostActionIntakeCommand {
    /// Preserves source shape; never infer this from a flattened or filtered timeline length.
    /// 保留来源形态；绝不从展平或过滤后的时间线长度推断。
    pub input_kind: PostActionInputKind,
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read and write scope boundaries.
    /// 定义读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Identifies the host session source.
    /// 标识宿主会话来源。
    pub session_source_id: SessionSourceId,
    /// Identifies the embedding vector space used by extracted memories.
    /// 标识提取记忆使用的嵌入向量空间。
    pub vector_space_id: VectorSpaceId,
    /// Carries the complete ordered timeline.
    /// 携带完整有序时间线。
    pub timeline: Vec<TimelineItem>,
    /// Carries the canonical completion time.
    /// 携带规范化完成时间。
    pub completed_at_ns: UnixNanos,
    /// Carries the persistent identity of this intake request.
    /// 携带本次登记请求的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Requests asynchronous analysis of one durably persisted pending turn.
/// 请求对一个已持久化的待处理轮次进行异步分析。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostActionAnalyzePendingTurnCommand {
    /// Separates replacement candidate visibility and supersede authority from the source turn scope.
    /// 将替代候选可见性与替代授权从来源轮次作用域中分离。
    pub replacement_scope: crate::MemoryReplacementScope,
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read and write scope boundaries.
    /// 定义读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Binds each generic profile role emitted by analysis to one exact writable scope.
    /// 将分析产出的每个通用画像角色绑定到一个精确可写作用域。
    pub profile_sections: Vec<ProfileViewSection>,
    /// Identifies the host session source.
    /// 标识宿主会话来源。
    pub session_source_id: SessionSourceId,
    /// Identifies the embedding vector space used by extracted memories.
    /// 标识提取记忆使用的嵌入向量空间。
    pub vector_space_id: VectorSpaceId,
    /// Identifies the durable pending turn under analysis.
    /// 标识正在分析的持久待处理轮次。
    pub turn_id: TurnId,
    /// Carries the explicit analysis generation supplied by the host scheduler.
    /// 携带由宿主调度器提供的显式分析代次。
    pub analysis_generation: u64,
    /// Carries the immutable business-time cutoff for this analysis generation.
    /// 携带本次分析代次不可变的业务时间截止点。
    pub analysis_cutoff_ns: UnixNanos,
    /// Carries the durable session watermark after the last completed extraction.
    /// 携带上一次完成提炼后的持久会话水位。
    pub direct_write_observed_after_ns: UnixNanos,
    /// Carries the persistent identity of this analysis attempt.
    /// 携带本次分析尝试的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Requests atomically advancing one resolved session compact boundary.
/// 请求原子推进一个已解析会话的压缩边界。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompactCommand {
    /// Identifies the resolved subject owner.
    /// 标识已解析主体所有者。
    pub subject_id: SubjectId,
    /// Defines the exact session read and write boundary.
    /// 定义精确的会话读写边界。
    pub scope_access: ScopeAccess,
    /// Identifies the host session source.
    /// 标识宿主会话来源。
    pub session_source_id: SessionSourceId,
    /// Carries the caller-observed compaction time in Unix nanoseconds.
    /// 携带调用方观察到的纳秒级Unix压缩时间。
    pub compacted_at_ns: UnixNanos,
    /// Carries the persistent identity of this boundary mutation.
    /// 携带本次边界变更的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Requests semantic memory search.
/// 请求语义记忆搜索。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySearchCommand {
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read and write scope boundaries.
    /// 定义读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Carries the natural-language query.
    /// 携带自然语言查询。
    pub query: String,
    /// Limits returned hits; zero selects the frozen snapshot default.
    /// 限制返回命中数量；零表示选择冻结快照默认值。
    pub limit: u32,
    /// Identifies the expected vector space.
    /// 标识期望的向量空间。
    pub vector_space_id: VectorSpaceId,
    /// Carries optional explicit scope and session boundaries for this public search.
    /// 携带本次公共搜索的可选显式作用域及会话边界。
    pub boundary: CandidateQueryBoundary,
    /// Carries the business evaluation time that governs read-time decay.
    /// 携带管理读时衰减的业务评估时间。
    pub now_ns: UnixNanos,
}

/// Requests details for one visible memory.
/// 请求一条可见记忆的详情。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDetailsCommand {
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read scope boundaries.
    /// 定义读取作用域边界。
    pub scope_access: ScopeAccess,
    /// Identifies the requested memory.
    /// 标识请求的记忆。
    pub memory_id: MemoryId,
}

/// Requests direct creation of one durable memory.
/// 请求直接创建一条持久记忆。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteCommand {
    /// Defines replacement search and existing-record supersede grants independently of the new record target.
    /// 独立于新记录目标定义替代检索与既有记录替代授权。
    pub replacement_scope: crate::MemoryReplacementScope,
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read and write scope boundaries.
    /// 定义读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Identifies the host session source for direct-write exclusion during `PostAction`.
    /// 标识供 `PostAction` 主动写入排除使用的宿主会话来源。
    pub session_source_id: SessionSourceId,
    /// Identifies the embedding vector space used by the memory.
    /// 标识记忆使用的嵌入向量空间。
    pub vector_space_id: VectorSpaceId,
    /// Carries the complete direct-write item using the same semantics as batch input.
    /// 使用与批量输入相同的语义携带完整主动写入项。
    pub item: MemoryWriteBatchItem,
    /// Carries the canonical event time.
    /// 携带规范化事件时间。
    pub occurred_at_ns: UnixNanos,
    /// Carries the persistent identity of this write.
    /// 携带本次写入的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Reports one direct memory created or semantically deduplicated by Core.
/// 报告一条由 Core 创建或完成语义去重的主动记忆。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryWriteResult {
    /// Carries the committed memory with its host-assigned identity and final version.
    /// 携带带有宿主分配身份与最终版本的已提交记忆。
    pub memory: MemoryRecord,
    /// Reports whether an existing durable memory was reused.
    /// 报告是否复用了既有持久记忆。
    pub deduplicated: bool,
    /// Carries the durable receipt for a created memory, or none for deduplication reuse.
    /// 携带新建记忆的持久提交收据；复用去重时为空。
    pub receipt: Option<CommitReceipt>,
}

/// Requests deletion of one memory inside the explicit write scope.
/// 请求删除显式写入作用域内的一条记忆。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDeleteCommand {
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read and write scope boundaries.
    /// 定义读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Identifies the memory to delete.
    /// 标识待删除记忆。
    pub memory_id: MemoryId,
    /// Carries the expected optimistic version.
    /// 携带期望的乐观并发版本。
    pub expected_version: Option<u64>,
    /// Carries the persistent identity of this deletion.
    /// 携带本次删除的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Requests the visible profile bundle rendered through one host-resolved view.
/// 请求通过一个宿主解析视图渲染的可见画像包。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileQueryCommand {
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read scope boundaries.
    /// 定义读取作用域边界。
    pub scope_access: ScopeAccess,
    /// Carries the host-resolved ordered profile view.
    /// 携带由宿主解析的有序画像视图。
    pub view: ProfileView,
    /// Carries the deterministic lifecycle evaluation time.
    /// 携带确定性的生命周期评估时间。
    pub evaluated_at_ns: UnixNanos,
}

/// Requests applying one natural-language profile instruction.
/// 请求应用一条自然语言画像指令。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileInstructionCommand {
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read and write scope boundaries.
    /// 定义读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Identifies the one host-resolved profile section this instruction may inspect and mutate.
    /// 标识本次指令唯一允许读取和变更的宿主已解析画像分区。
    pub target_section: ProfileViewSection,
    /// Carries the instruction text.
    /// 携带指令文本。
    pub instruction: String,
    /// Carries the canonical event time.
    /// 携带规范化事件时间。
    pub occurred_at_ns: UnixNanos,
    /// Carries the persistent identity of this instruction.
    /// 携带本次指令的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Requests deterministic retention planning against one complete policy.
/// 请求针对一个完整策略的确定性保留规划。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionCommand {
    /// Identifies the subject.
    /// 标识主体。
    pub subject_id: SubjectId,
    /// Defines read and write scope boundaries.
    /// 定义读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Carries the policy evaluation time.
    /// 携带策略评估时间。
    pub now_ns: UnixNanos,
    /// Carries the complete retention policy snapshot.
    /// 携带完整的保留策略快照。
    pub policy: RetentionPolicy,
    /// Carries the persistent identity authorizing recycle and purge mutations.
    /// 携带授权回收与清除变更的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Requests one grouped multi-query memory search with a per-query result limit.
/// 请求一次带每查询结果限制的分组多查询记忆搜索。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySearchBatchCommand {
    /// Identifies the resolved subject.
    /// 标识已解析主体。
    pub subject_id: SubjectId,
    /// Defines exact read and write scope boundaries.
    /// 定义精确读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Carries complete natural-language queries in caller order.
    /// 按调用方顺序携带完整自然语言查询。
    pub queries: Vec<String>,
    /// Limits results independently for every query group; zero selects the snapshot default.
    /// 独立限制每个查询分组的结果数；零表示选择快照默认值。
    pub top_k: u32,
    /// Identifies the required embedding vector space.
    /// 标识所需的嵌入向量空间。
    pub vector_space_id: VectorSpaceId,
    /// Carries optional explicit scope and session boundaries shared by every query group.
    /// 携带全部查询分组共享的可选显式作用域及会话边界。
    pub boundary: CandidateQueryBoundary,
    /// Carries the business evaluation time that governs read-time decay.
    /// 携带管理读时衰减的业务评估时间。
    pub now_ns: UnixNanos,
}

/// Carries one grouped multi-query result in caller query order.
/// 按调用方查询顺序携带一个分组多查询结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemorySearchGroup {
    /// Carries the zero-based caller query index.
    /// 携带从零开始的调用方查询索引。
    pub query_index: u32,
    /// Carries the normalized query.
    /// 携带规范化查询。
    pub query: String,
    /// Carries final ordered hits for this query only.
    /// 仅携带该查询的最终有序命中。
    pub hits: Vec<SearchHit>,
}

/// Requests batch memory and turn details without per-record host-effect round trips.
/// 请求批量记忆与轮次详情，不产生逐记录宿主效应往返。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDetailsBatchCommand {
    /// Identifies the resolved subject.
    /// 标识已解析主体。
    pub subject_id: SubjectId,
    /// Defines exact visible scope boundaries.
    /// 定义精确可见作用域边界。
    pub scope_access: ScopeAccess,
    /// Carries requested memory identities in caller order.
    /// 按调用方顺序携带请求的记忆标识。
    pub memory_ids: Vec<MemoryId>,
    /// Carries requested turn identities in caller order.
    /// 按调用方顺序携带请求的轮次标识。
    pub turn_ids: Vec<TurnId>,
}

/// Carries caller-ordered batch detail results with explicit missing entries.
/// 携带按调用方顺序排列且显式表示缺失项的批量详情结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryDetailsBatchResult {
    /// Carries one optional record for each requested memory identity.
    /// 为每个请求的记忆标识携带一条可选记录。
    pub memories: Vec<Option<MemoryRecord>>,
    /// Carries one optional canonical turn for each requested turn identity.
    /// 为每个请求的轮次标识携带一个可选规范轮次。
    pub turns: Vec<Option<Turn>>,
}

/// Describes one direct memory item inside an atomic batch write.
/// 描述原子批量写入中的一条主动记忆项。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteBatchItem {
    /// Identifies the exact Host-resolved scope that owns this item.
    /// 标识拥有该项目的精确 Host 已解析作用域。
    pub target_scope_id: ScopeId,
    /// Classifies the generic ownership horizon used by Core lifecycle policy.
    /// 对 Core 生命周期策略使用的通用归属跨度进行分类。
    pub scope_class: MemoryScopeClass,
    /// Classifies the memory domain meaning.
    /// 对记忆领域含义分类。
    pub category: MemoryCategory,
    /// Carries a compact summary used by search and UI adapters.
    /// 携带供搜索与 UI 适配器使用的紧凑摘要。
    pub summary: String,
    /// Carries complete durable details.
    /// 携带完整持久详情。
    pub details: String,
    /// Carries optional normalized priority; Core applies the frozen P2 default when absent.
    /// 携带可选规范化优先级；缺失时 Core 应用冻结的 P2 默认值。
    pub priority: Option<u32>,
    /// Carries the optional lifecycle level; Core derives the frozen scope default when absent.
    /// 携带可选生命周期等级；缺失时 Core 按冻结的作用域默认值推导。
    pub memory_level: Option<MemoryLevel>,
    /// Carries an optional absolute expiration time.
    /// 携带可选的绝对过期时间。
    pub expires_at_ns: Option<UnixNanos>,
    /// Carries searchable labels.
    /// 携带可搜索标签。
    pub tags: Vec<String>,
}

/// Requests one atomic direct-memory batch write with host durable operation identity.
/// 请求一次带宿主持久操作身份的原子主动记忆批量写入。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteBatchCommand {
    /// Defines replacement search and supersede grants independently of the batch creation grants.
    /// 独立于批次新建授权定义替代检索与替代授权。
    pub replacement_scope: crate::MemoryReplacementScope,
    /// Identifies the resolved subject.
    /// 标识已解析主体。
    pub subject_id: SubjectId,
    /// Defines exact read and write scope boundaries.
    /// 定义精确读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Carries every exact scope this atomic command may create records in; supersede grants are separate.
    /// 携带本原子命令获准新建记录的每个精确作用域；替代授权独立提供。
    pub authorized_write_scopes: Vec<ScopeId>,
    /// Identifies the host session source for durable attribution.
    /// 标识用于持久归因的宿主会话来源。
    pub session_source_id: SessionSourceId,
    /// Identifies the embedding vector space.
    /// 标识嵌入向量空间。
    pub vector_space_id: VectorSpaceId,
    /// Carries canonical event time for all items.
    /// 携带全部项的规范事件时间。
    pub occurred_at_ns: UnixNanos,
    /// Carries ordered direct-write items.
    /// 携带有序主动写入项。
    pub items: Vec<MemoryWriteBatchItem>,
    /// Carries the persistent identity of this batch write.
    /// 携带本次批量写入的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Reports one ordered direct-memory write result.
/// 报告一个有序主动记忆写入结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryWriteBatchResult {
    /// Carries written or idempotently reused records in input order.
    /// 按输入顺序携带已写入或幂等复用的记录。
    pub memories: Vec<MemoryRecord>,
    /// Reports whether each aligned result reused an existing or earlier batch record.
    /// 报告每个对齐结果是否复用了既有记录或批次中更早的记录。
    pub deduplicated: Vec<bool>,
    /// Carries the durable commit receipt.
    /// 携带持久提交收据。
    pub receipt: Option<CommitReceipt>,
}

/// Describes one explicitly versioned memory deletion.
/// 描述一次显式带版本的记忆删除。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDeleteBatchItem {
    /// Identifies the memory to delete.
    /// 标识待删除记忆。
    pub memory_id: MemoryId,
    /// Carries an optional optimistic version.
    /// 携带可选乐观版本。
    pub expected_version: Option<u64>,
}

/// Requests one atomic scoped memory deletion batch.
/// 请求一次原子作用域内记忆删除批次。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDeleteBatchCommand {
    /// Identifies the resolved subject.
    /// 标识已解析主体。
    pub subject_id: SubjectId,
    /// Defines exact read and write scope boundaries.
    /// 定义精确读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Carries a nonempty audit reason.
    /// 携带非空审计原因。
    pub reason: String,
    /// Carries ordered deletion items.
    /// 携带有序删除项。
    pub items: Vec<MemoryDeleteBatchItem>,
    /// Carries the persistent identity of this batch deletion.
    /// 携带本次批量删除的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Reports the ordered outcome of one atomic deletion batch.
/// 报告一次原子删除批次的有序结果。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDeleteBatchResult {
    /// Carries identities confirmed deleted in request order.
    /// 按请求顺序携带已确认删除标识。
    pub deleted_memory_ids: Vec<MemoryId>,
    /// Carries missing, inactive, or out-of-write-scope identities in request order.
    /// 按请求顺序携带缺失、非活跃或超出写入作用域的标识。
    pub not_found_memory_ids: Vec<MemoryId>,
    /// Carries the durable receipt when at least one memory was deleted.
    /// 在至少删除一条记忆时携带持久提交收据。
    pub receipt: Option<CommitReceipt>,
}

/// Reports one committed direct-memory deletion.
/// 报告一次已提交的主动记忆删除。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDeleteResult {
    /// States whether the requested memory was deleted.
    /// 表示请求的记忆是否已删除。
    pub deleted: bool,
    /// Carries the durable receipt of the deletion mutation.
    /// 携带删除变更的持久提交收据。
    pub receipt: CommitReceipt,
}

/// Reports one reviewed profile-instruction outcome.
/// 报告一次已审核的画像指令结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileInstructionResult {
    /// Carries newly accepted active nodes.
    /// 携带新接受的活跃节点。
    pub accepted_nodes: Vec<ProfileNode>,
    /// Carries atomically retired nodes with explicit reasons.
    /// 携带带显式原因的原子退役节点。
    pub retired_nodes: Vec<RetiredProfileNode>,
    /// Carries the normalized review rationale.
    /// 携带规范化审核理由。
    pub review_reason: String,
    /// Carries the durable commit receipt when mutations were emitted.
    /// 在发出变更时携带持久提交收据。
    pub receipt: Option<CommitReceipt>,
}

/// Identifies one memory and its required optimistic version for maintenance.
/// 标识一条记忆及其维护所需的乐观版本。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedMemoryRef {
    /// Identifies the target memory.
    /// 标识目标记忆。
    pub memory_id: MemoryId,
    /// Carries the required current version.
    /// 携带必需当前版本。
    pub expected_version: u64,
}

/// Selects one explicit maintenance operation with no implicit fallback.
/// 选择一种不含隐式兜底的显式维护操作。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaintenanceAction {
    /// Restores recycled memories to active state.
    /// 将已回收记忆恢复为活跃状态。
    RestoreRecycled(Vec<VersionedMemoryRef>),
    /// Physically purges recycled memories and dependent evidence.
    /// 物理清除已回收记忆及其依赖证据。
    PurgeRecycled(Vec<VersionedMemoryRef>),
    /// Rebuilds active memory vectors in the requested vector space.
    /// 在请求的向量空间中重建活跃记忆向量。
    RebuildVectors {
        /// Carries exact target memory identities in caller order.
        /// 按调用方顺序携带精确目标记忆标识。
        memory_ids: Vec<MemoryId>,
        /// Identifies the new vector space.
        /// 标识新向量空间。
        vector_space_id: VectorSpaceId,
    },
}

/// Requests one host-identity-idempotent atomic maintenance operation.
/// 请求一次宿主持久身份幂等的原子维护操作。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceCommand {
    /// Identifies the resolved subject.
    /// 标识已解析主体。
    pub subject_id: SubjectId,
    /// Defines exact read and write scope boundaries.
    /// 定义精确读写作用域边界。
    pub scope_access: ScopeAccess,
    /// Carries canonical maintenance event time.
    /// 携带规范维护事件时间。
    pub occurred_at_ns: UnixNanos,
    /// Selects the exact maintenance action.
    /// 选择精确维护操作。
    pub action: MaintenanceAction,
    /// Carries the persistent identity of this maintenance operation.
    /// 携带本次维护操作的持久身份。
    pub mutation_identity: MutationIdentity,
}

/// Reports one committed maintenance operation.
/// 报告一次已提交维护操作。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceResult {
    /// Carries affected memory identities in caller order.
    /// 按调用方顺序携带受影响记忆标识。
    pub affected_memory_ids: Vec<MemoryId>,
    /// Counts vectors rebuilt by this operation.
    /// 统计本次操作重建的向量数。
    pub rebuilt_vector_count: u32,
    /// Carries the durable receipt of the maintenance mutation.
    /// 携带维护变更的持久提交收据。
    pub receipt: CommitReceipt,
}

/// Enumerates every public resumable Core command.
/// 枚举所有公共可恢复 Core 命令。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreCommand {
    /// Runs `PreCheck` recall.
    /// 运行 `PreCheck` 召回。
    PreCheck(PreCheckCommand),
    /// Runs durable `PostAction` intake with immediate acknowledgement.
    /// 运行持久 `PostAction` 登记并立即确认。
    PostActionIntake(PostActionIntakeCommand),
    /// Runs asynchronous analysis of one persisted pending turn.
    /// 运行对一个已持久化待处理轮次的异步分析。
    PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand),
    /// Compacts chat history.
    /// 压缩聊天历史。
    ChatCompact(ChatCompactCommand),
    /// Searches memories.
    /// 搜索记忆。
    MemorySearch(MemorySearchCommand),
    /// Reads memory details.
    /// 读取记忆详情。
    MemoryDetails(MemoryDetailsCommand),
    /// Writes one memory.
    /// 写入一条记忆。
    MemoryWrite(MemoryWriteCommand),
    /// Deletes one memory.
    /// 删除一条记忆。
    MemoryDelete(MemoryDeleteCommand),
    /// Reads a rendered profile bundle.
    /// 读取已渲染的画像包。
    ProfileQuery(ProfileQueryCommand),
    /// Applies a profile instruction.
    /// 应用画像指令。
    ProfileInstruction(ProfileInstructionCommand),
    /// Produces deterministic retention plans.
    /// 产生确定性保留计划。
    Retention(RetentionCommand),
    /// Searches multiple query groups in one resumable command.
    /// 在一个可恢复命令中搜索多个查询分组。
    MemorySearchBatch(MemorySearchBatchCommand),
    /// Reads memory and turn details in one batch.
    /// 在一个批次中读取记忆与轮次详情。
    MemoryDetailsBatch(MemoryDetailsBatchCommand),
    /// Writes multiple direct memories atomically.
    /// 原子写入多条主动记忆。
    MemoryWriteBatch(MemoryWriteBatchCommand),
    /// Deletes multiple memories atomically.
    /// 原子删除多条记忆。
    MemoryDeleteBatch(MemoryDeleteBatchCommand),
    /// Runs one explicit lifecycle or vector maintenance operation.
    /// 运行一次显式生命周期或向量维护操作。
    Maintenance(MaintenanceCommand),
}

impl CoreCommand {
    /// Returns the stable wire and fingerprint discriminant of this command.
    /// 返回该命令的稳定线格式与指纹判别值。
    #[must_use]
    pub const fn kind(&self) -> u32 {
        match self {
            Self::PreCheck(_) => 1,
            Self::PostActionIntake(_) => 2,
            Self::PostActionAnalyzePendingTurn(_) => 3,
            Self::ChatCompact(_) => 4,
            Self::MemorySearch(_) => 5,
            Self::MemoryDetails(_) => 6,
            Self::MemoryWrite(_) => 7,
            Self::MemoryDelete(_) => 8,
            Self::ProfileQuery(_) => 9,
            Self::ProfileInstruction(_) => 10,
            Self::Retention(_) => 11,
            Self::MemorySearchBatch(_) => 12,
            Self::MemoryDetailsBatch(_) => 13,
            Self::MemoryWriteBatch(_) => 14,
            Self::MemoryDeleteBatch(_) => 15,
            Self::Maintenance(_) => 16,
        }
    }

    /// Returns the persistent mutation identity of every mutating command.
    /// 返回每个变更命令的持久变更身份。
    #[must_use]
    pub const fn mutation_identity(&self) -> Option<&MutationIdentity> {
        match self {
            Self::PreCheck(command) => Some(&command.mutation_identity),
            Self::PostActionIntake(command) => Some(&command.mutation_identity),
            Self::PostActionAnalyzePendingTurn(command) => Some(&command.mutation_identity),
            Self::ChatCompact(command) => Some(&command.mutation_identity),
            Self::MemoryWrite(command) => Some(&command.mutation_identity),
            Self::MemoryDelete(command) => Some(&command.mutation_identity),
            Self::ProfileInstruction(command) => Some(&command.mutation_identity),
            Self::Retention(command) => Some(&command.mutation_identity),
            Self::MemoryWriteBatch(command) => Some(&command.mutation_identity),
            Self::MemoryDeleteBatch(command) => Some(&command.mutation_identity),
            Self::Maintenance(command) => Some(&command.mutation_identity),
            Self::MemorySearch(_)
            | Self::MemoryDetails(_)
            | Self::ProfileQuery(_)
            | Self::MemorySearchBatch(_)
            | Self::MemoryDetailsBatch(_) => None,
        }
    }

    /// Reports whether this command may produce durable side effects.
    /// 报告该命令是否可能产生持久副作用。
    #[must_use]
    pub const fn is_mutating(&self) -> bool {
        self.mutation_identity().is_some()
    }

    /// Borrows the exact subject identity for scope validation and fingerprinting.
    /// 借用精确主体标识用于作用域校验与指纹计算。
    #[must_use]
    pub const fn subject_id(&self) -> &SubjectId {
        match self {
            Self::PreCheck(command) => &command.subject_id,
            Self::PostActionIntake(command) => &command.subject_id,
            Self::PostActionAnalyzePendingTurn(command) => &command.subject_id,
            Self::ChatCompact(command) => &command.subject_id,
            Self::MemorySearch(command) => &command.subject_id,
            Self::MemoryDetails(command) => &command.subject_id,
            Self::MemoryWrite(command) => &command.subject_id,
            Self::MemoryDelete(command) => &command.subject_id,
            Self::ProfileQuery(command) => &command.subject_id,
            Self::ProfileInstruction(command) => &command.subject_id,
            Self::Retention(command) => &command.subject_id,
            Self::MemorySearchBatch(command) => &command.subject_id,
            Self::MemoryDetailsBatch(command) => &command.subject_id,
            Self::MemoryWriteBatch(command) => &command.subject_id,
            Self::MemoryDeleteBatch(command) => &command.subject_id,
            Self::Maintenance(command) => &command.subject_id,
        }
    }

    /// Borrows the exact scope access for scope validation and fingerprinting.
    /// 借用精确作用域访问用于作用域校验与指纹计算。
    #[must_use]
    pub const fn scope_access(&self) -> &ScopeAccess {
        match self {
            Self::PreCheck(command) => &command.scope_access,
            Self::PostActionIntake(command) => &command.scope_access,
            Self::PostActionAnalyzePendingTurn(command) => &command.scope_access,
            Self::ChatCompact(command) => &command.scope_access,
            Self::MemorySearch(command) => &command.scope_access,
            Self::MemoryDetails(command) => &command.scope_access,
            Self::MemoryWrite(command) => &command.scope_access,
            Self::MemoryDelete(command) => &command.scope_access,
            Self::ProfileQuery(command) => &command.scope_access,
            Self::ProfileInstruction(command) => &command.scope_access,
            Self::Retention(command) => &command.scope_access,
            Self::MemorySearchBatch(command) => &command.scope_access,
            Self::MemoryDetailsBatch(command) => &command.scope_access,
            Self::MemoryWriteBatch(command) => &command.scope_access,
            Self::MemoryDeleteBatch(command) => &command.scope_access,
            Self::Maintenance(command) => &command.scope_access,
        }
    }
}

/// Reports the durable adoption reinforcement outcome of one completed `PreCheck`.
/// 报告一次已完成 `PreCheck` 的持久采纳强化结果。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReinforcementResult {
    /// Carries every accepted memory identity whose adoption was reinforced.
    /// 携带每个已采纳且被强化的记忆标识。
    pub reinforced_memory_ids: Vec<MemoryId>,
    /// States whether the reinforcement write failed or remained uncommitted.
    /// 表示强化写入是否失败或未提交。
    pub failed: bool,
    /// Carries the durable receipt when reinforcement committed successfully.
    /// 在强化成功提交时携带持久提交收据。
    pub receipt: Option<CommitReceipt>,
}

/// Reports the final `PreCheck` decision and assembled context.
/// 报告最终 `PreCheck` 决策与组装上下文。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreCheckResult {
    /// States whether memory context should be injected.
    /// 表示是否应注入记忆上下文。
    pub should_inject: bool,
    /// Carries the assembled context text.
    /// 携带组装后的上下文文本。
    pub context_text: String,
    /// Carries accepted redacted context views, never mutable copies of storage records.
    /// 携带已接受的脱敏上下文视图，不携带存储记录的可变副本。
    pub context_items: Vec<PreCheckReviewCandidate>,
    /// Carries the post-pipeline review candidates, including when L2 review fails.
    /// 携带检索流水线处理后的审核候选，L2 审核失败时也保留；不是原始物理通道全集。
    pub candidates: Vec<PreCheckReviewCandidate>,
    /// States whether a recoverable host effect degraded the result.
    /// 表示可恢复宿主 Effect 是否使结果降级。
    pub degraded: bool,
    /// Carries the deterministic search queries used by the command.
    /// 携带命令使用的确定性搜索查询。
    pub queries: Vec<String>,
    /// Carries the durable adoption reinforcement outcome when recall accepted memories.
    /// 在召回接受记忆时携带持久采纳强化结果。
    pub reinforcement: Option<ReinforcementResult>,
}

/// Identifies which Core-owned admission path produced the final Noise decision.
/// 标识产生最终 Noise 决策的 Core 自有准入路径。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum NoiseDecisionSource {
    /// The snapshot disabled Noise entirely.
    /// 快照完全禁用了 Noise。
    Disabled = 1,
    /// Source intermediate messages bypassed single-turn admission.
    /// 来源中间消息跳过了单轮准入。
    TimelineBypass = 2,
    /// Deterministic regex rules made the decision without runtime semantic scoring.
    /// 确定性正则规则作出决策，未进行运行时语义评分。
    Regex = 3,
    /// Available semantic prototypes participated in the final decision.
    /// 可用语义原型参与了最终决策。
    Semantic = 4,
    /// Semantic capability failure left only the already-passed deterministic rules.
    /// 语义能力失败，仅保留已经通过的确定性规则。
    RegexOnlyFallback = 5,
}

/// Preserves auditable Noise facts after command handles and internal continuations are released.
/// 在命令句柄和内部续体释放后保留可审计的 Noise 事实。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoiseAudit {
    /// Binds the exact immutable policy and resource snapshot used by this decision.
    /// 绑定本次决策使用的精确不可变策略与资源快照。
    pub config_snapshot_id: ConfigSnapshotId,
    /// Identifies the validated rule file schema.
    /// 标识已经校验的规则文件 Schema。
    pub rule_schema_version: String,
    /// Binds the resolved common and locale rule bytes.
    /// 绑定解析后的 common 与区域规则字节。
    pub rules_digest: String,
    /// Explains whether the decision used rules, semantics, a bypass, or failure fallback.
    /// 说明决策使用规则、语义、跳过还是故障回退。
    pub source: NoiseDecisionSource,
    /// Identifies the role associated with the reported category and confidence, when evaluated.
    /// 在已评估时标识所报告类别与置信度对应的角色。
    pub role: Option<crate::Role>,
    /// Reports the matched or best observed semantic category when present.
    /// 在存在时报告命中类别或最佳已观测语义类别。
    pub category: Option<String>,
    /// Reports the finite nonnegative source dot score; rounding may exceed one.
    /// 报告有限非负来源点积分数；舍入可能略大于一。
    pub confidence: f64,
    /// Distinguishes successful capability execution from degraded admission.
    /// 区分能力完整执行与降级准入。
    pub degraded: bool,
    /// Names category embeddings that were unavailable instead of silently dropping them.
    /// 命名不可用的类别嵌入，不将其静默丢弃。
    pub unavailable_categories: Vec<String>,
}

/// Reports one durable `PostAction` intake acknowledgement.
/// 报告一次持久 `PostAction` 登记确认。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PostActionAcceptedResult {
    /// Preserves the exact Noise decision, including successful bypass and failure fallback.
    /// 保留精确 Noise 决策，包括正常跳过与故障回退。
    pub noise: NoiseAudit,
    /// Identifies the persisted pending turn when the intake passed noise gating.
    /// 在登记通过 Noise 门控时标识已持久化的待处理轮次。
    pub turn_id: Option<TurnId>,
    /// States whether the turn was rejected as pure noise before persistence.
    /// 表示轮次是否在持久化前作为纯噪声被拒绝。
    pub ignored_as_noise: bool,
    /// Carries the durable commit receipt when the pending turn was written.
    /// 在写入待处理轮次时携带持久提交收据。
    pub receipt: Option<CommitReceipt>,
}

/// Reports one completed `PostAction` analysis with its full derived outcome.
/// 报告一次完成的 `PostAction` 分析及其完整派生结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PostActionAnalysisResult {
    /// Identifies the analyzed turn.
    /// 标识被分析的轮次。
    pub turn_id: TurnId,
    /// Carries accepted memory identities bound at commit time in candidate order.
    /// 按候选顺序携带提交时绑定的已接受记忆标识。
    pub accepted_memory_ids: Vec<MemoryId>,
    /// Carries dropped candidates with deduplication sources.
    /// 携带带去重来源的被丢弃候选。
    pub dropped_candidates: Vec<DroppedCandidate>,
    /// Audits provider-confirmed invalid persistence inputs in admitted-candidate coordinates.
    /// 在准入候选坐标中审计 Provider 已确认非法的持久化输入。
    pub embedding_dropped_candidates: Vec<crate::EmbeddingDroppedInput>,
    /// Carries existing memory identities superseded by accepted candidates.
    /// 携带被已接受候选替代的既有记忆标识。
    pub superseded_memory_ids: Vec<MemoryId>,
    /// Carries newly persisted nodes; state distinguishes reviewed active facts from unreviewed pending evidence.
    /// 携带新持久化节点；状态区分已评审活跃事实与未评审待定证据，不表示全部节点均获L2批准。
    pub accepted_profile_nodes: Vec<ProfileNode>,
    /// Carries atomically retired profile node identities.
    /// 携带原子退役的画像节点标识。
    pub retired_profile_node_ids: Vec<ProfileNodeId>,
    /// Carries context evidence rows attached to this analysis in stable order.
    /// 以稳定顺序携带附着于本次分析的情境证据行。
    pub context_evidence: Vec<MemoryContextEvidence>,
    /// Carries the durable commit receipt of the derived mutation batch.
    /// 携带派生变更批次的持久提交收据。
    pub receipt: Option<CommitReceipt>,
}

/// Reports one import preflight plan.
/// 报告一份导入预检计划。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPlan {
    /// Reports the decoded schema version.
    /// 报告解码后的 Schema 版本。
    pub schema_version: u32,
    /// Reports the source resource digest.
    /// 报告来源资源摘要。
    pub resource_digest: String,
    /// Reports the source vector space.
    /// 报告来源向量空间。
    pub vector_space_id: Option<VectorSpaceId>,
    /// Reports the source embedding dimension paired with the vector space.
    /// 报告与来源向量空间成对的嵌入维度。
    pub embedding_dimension: Option<u32>,
    /// Counts turns that would be imported.
    /// 统计将导入的对话轮次。
    pub turn_count: u64,
    /// Counts memories that would be imported.
    /// 统计将导入的记忆。
    pub memory_count: u64,
    /// Counts profile nodes that would be imported.
    /// 统计将导入的画像节点。
    pub profile_node_count: u64,
    /// Counts context evidence rows that would be imported.
    /// 统计将导入的情境证据行。
    pub context_evidence_count: u64,
    /// Carries explicit conflicts detected during preflight.
    /// 携带预检期间检测到的显式冲突。
    pub conflicts: Vec<String>,
    /// States whether vector rebuild is required.
    /// 表示是否需要重建向量。
    pub requires_vector_rebuild: bool,
}

/// Enumerates final strongly typed command results.
/// 枚举最终强类型命令结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CommandResult {
    /// Carries a `PreCheck` result.
    /// 携带 `PreCheck` 结果。
    PreCheck(PreCheckResult),
    /// Carries a `PostAction` intake acknowledgement.
    /// 携带 `PostAction` 登记确认。
    PostActionAccepted(PostActionAcceptedResult),
    /// Carries a completed `PostAction` analysis outcome.
    /// 携带完成的 `PostAction` 分析结果。
    PostActionAnalysis(PostActionAnalysisResult),
    /// Carries the durable chat compact-boundary decision.
    /// 携带持久聊天压缩边界决策。
    ChatCompact(ChatCompactResult),
    /// Carries memory search hits.
    /// 携带记忆搜索命中。
    MemorySearch(Vec<SearchHit>),
    /// Carries optional memory details.
    /// 携带可选记忆详情。
    MemoryDetails(Option<MemoryRecord>),
    /// Carries the written memory.
    /// 携带已写入的记忆。
    MemoryWrite(MemoryWriteResult),
    /// Carries one committed memory deletion result.
    /// 携带一次已提交的记忆删除结果。
    MemoryDelete(MemoryDeleteResult),
    /// Carries a profile bundle.
    /// 携带画像包。
    ProfileQuery(ProfileBundle),
    /// Carries profile nodes changed by an instruction.
    /// 携带画像指令修改的节点。
    ProfileInstruction(ProfileInstructionResult),
    /// Carries the complete retention plan set.
    /// 携带完整的保留计划集合。
    Retention(RetentionPlanSet),
    /// Carries grouped multi-query search results.
    /// 携带分组多查询搜索结果。
    MemorySearchBatch(Vec<MemorySearchGroup>),
    /// Carries batch memory and turn details.
    /// 携带批量记忆与轮次详情。
    MemoryDetailsBatch(MemoryDetailsBatchResult),
    /// Carries atomic batch-write results.
    /// 携带原子批量写入结果。
    MemoryWriteBatch(MemoryWriteBatchResult),
    /// Carries atomic batch-delete results.
    /// 携带原子批量删除结果。
    MemoryDeleteBatch(MemoryDeleteBatchResult),
    /// Carries one maintenance result.
    /// 携带一次维护结果。
    Maintenance(MaintenanceResult),
    /// Carries a portable decoded dataset for import application.
    /// 携带用于导入应用的可移植解码数据集。
    ImportDataset(ExportDataset),
}

/// Describes the observable lifecycle of one command.
/// 描述一个命令的可观察生命周期。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum CommandStatus {
    /// Indicates the command is waiting for host effects.
    /// 表示命令正在等待宿主 Effect。
    Waiting = 1,
    /// Indicates the command completed successfully.
    /// 表示命令成功完成。
    Completed = 2,
    /// Indicates the command was cancelled.
    /// 表示命令已取消。
    Cancelled = 3,
    /// Indicates a storage outcome must be reconciled by idempotency key.
    /// 表示必须通过幂等键对账存储结果。
    ResultUncertain = 4,
    /// Indicates the command failed with a stable Core error.
    /// 表示命令因稳定 Core 错误而失败。
    Failed = 5,
}

/// Identifies one command and its frozen configuration snapshot.
/// 标识一个命令及其冻结配置快照。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandHandle {
    /// Identifies the command.
    /// 标识命令。
    pub command_id: CommandId,
    /// Identifies the immutable configuration snapshot.
    /// 标识不可变配置快照。
    pub config_snapshot_id: ConfigSnapshotId,
}

/// Reports command lifecycle and diagnostic state without exposing engine internals.
/// 报告命令生命周期与诊断状态，同时不暴露引擎内部实现。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDiagnostics {
    /// Carries the public command handle.
    /// 携带公共命令句柄。
    pub handle: CommandHandle,
    /// Carries the current lifecycle status.
    /// 携带当前生命周期状态。
    pub status: CommandStatus,
    /// Identifies the pending effect when waiting.
    /// 在等待时标识待处理 Effect。
    pub pending_effect_id: Option<crate::EffectId>,
    /// Carries the final stable error when failed or uncertain.
    /// 在失败或结果不确定时携带最终稳定错误。
    pub last_error: Option<CoreError>,
}
