//! Shared resumable command state machine and Core-owned retrieval execution.
//! 共享可恢复命令状态机与 Core 自有检索执行。

#[cfg(test)]
#[path = "observation_tests.rs"]
mod observation_tests;
#[cfg(test)]
#[path = "separate_fallback_tests.rs"]
mod separate_fallback_tests;

#[cfg(test)]
#[path = "review_query_tests.rs"]
mod review_query_tests;

#[path = "review_query_checkpoint.rs"]
mod review_query_checkpoint;

use crate::algorithms::{normalize_hash_text, normalize_text, simple_lowercase};
use crate::precheck::{ReviewAccumulator, ReviewCandidateState};
use crate::query_batch::QueryBatchPlan;
use crate::retrieval_kernel;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use vulcan_memorymesh_core_config::ConfigSnapshot;
use vulcan_memorymesh_core_contracts::{
    AcceptedCandidate, AcceptedProfileCandidate, Admission, AnalysisAttemptIdentity,
    CandidateQueryBoundary, ChatCompactCommand, ChatCompactResult, CommandCheckpoint,
    CommandDiagnostics, CommandHandle, CommandId, CommandResult, CommandStatus, CommandStep,
    CommitReceipt, ContextEvidenceCandidate, ContextEvidenceGroup, ContextEvidenceRelation,
    CoreCommand, CoreError, DirectWriteDedupeLookup, DroppedCandidate, EffectDeadlineClass,
    EffectFailureKind, EffectId, EffectRequest, EffectResult, EmbeddingEffect, EntityKind,
    EntityRef, EntitySlotId, ErrorCode, ErrorStage, EvidenceRef, FailureClass, HostEffect,
    HostEffectResult, IdempotencyKey, LlmEffect, LlmInput, LlmOutput, LlmPurpose,
    MAX_DOMAIN_COLLECTION_ITEMS, MAX_DOMAIN_TEXT_BYTES, MAX_MEMORY_TAG_BYTES, MAX_MEMORY_TAGS,
    MaintenanceAction, MaintenanceCommand, MaintenanceResult, MemoryAccessClass,
    MemoryAccessIdentity, MemoryCandidate, MemoryContextEvidence, MemoryDeleteBatchCommand,
    MemoryDeleteBatchResult, MemoryDeleteCommand, MemoryDeleteResult, MemoryDetailsBatchCommand,
    MemoryDetailsBatchResult, MemoryDetailsCommand, MemoryId, MemoryLevel, MemoryRecord,
    MemoryReviewCandidate, MemoryScopeClass, MemorySearchBatchCommand, MemorySearchCommand,
    MemorySearchGroup, MemorySourceKind, MemoryStability, MemoryState, MemoryWriteBatchCommand,
    MemoryWriteBatchItem, MemoryWriteBatchResult, MemoryWriteCommand, MemoryWriteResult, Mutation,
    MutationBatch, MutationIdentity, MutationOutcome, NoiseAudit, NoiseDecisionSource,
    NoisePrototype, NoisePrototypeCacheIdentity, PhysicalCandidate, PlannedColdTurnJobRef,
    PlannedMemoryRef, PlannedSessionRef, PlannedTrashBatchRef, PlannedVectorGcRef,
    PostActionAcceptedResult, PostActionAnalysisResult, PostActionAnalyzePendingTurnCommand,
    PostActionDirectWrite, PostActionIntakeCommand, PostActionReferenceTurn, PreCheckCommand,
    PreCheckResult, PreCheckReviewCandidate, ProfileBundle, ProfileCandidate,
    ProfileInstructionCommand, ProfileInstructionResult, ProfileLevel, ProfileNode, ProfileNodeId,
    ProfileNodeState, ProfileQueryCommand, ProfileSectionBundle, ProfileSourceId,
    ProfileSourceKind, ProfileViewSection, RecallMode, ReinforcementResult, RerankEffect,
    RetentionCommand, RetentionPlanSet, RetentionReasonCode, RetiredProfileNode, Role, ScopeAccess,
    ScopeId, SearchHit, SectionRole, SessionRecallBoundary, StorageQuery, StorageQueryResult,
    StructuredOutputContractId, SubjectId, TimelineItem, Turn, TurnId, TurnStatus, VectorSpaceId,
};

/// Defines the authoritative VMM recent direct-write soft-idempotency window.
/// 定义 VMM 权威的近期主动写入软幂等窗口。
/// Carries the fixed direct-write dedupe window in nanoseconds.
/// 以纳秒保存固定的主动写入去重窗口。
const DIRECT_WRITE_DEDUPE_WINDOW_NS: i64 = 24 * 60 * 60 * 1_000_000_000;

/// Converts one public millisecond duration into a representable signed nanosecond duration.
/// 把一个公共毫秒时长转换为可表示的有符号纳秒时长。
fn checked_duration_ns(duration_millis: u64, field: &str) -> Result<i64, CoreError> {
    let converted_nanos = i128::from(duration_millis) * 1_000_000;
    i64::try_from(converted_nanos).map_err(|_| {
        validation_error(format!(
            "{field} millisecond duration exceeds signed nanosecond range"
        ))
    })
}

/// Limits one private continuation or received effect summary to a recoverable binary size.
/// 将单个私有续体或已接收 Effect 摘要限制在可恢复的二进制大小内。
const MAX_PRIVATE_CONTINUATION_BYTES: usize = 16_777_216;
/// Limits Host-owned job and batch identifiers that are not yet dedicated ID newtypes.
/// 限制尚未使用专用 ID 新类型的宿主任务与批次标识。
const MAX_HOST_IDENTIFIER_BYTES: usize = 512;

/// Drives every public command through the same typed Effect/Resume state machine.
/// 通过同一强类型 Effect/Resume 状态机驱动每个公共命令。
#[derive(Default)]
pub struct CommandEngine {
    /// Retains command records for status, cancellation, resume, and checkpoint.
    /// 保留用于状态、取消、恢复与检查点的命令记录。
    records: BTreeMap<CommandId, CommandRecord>,
    /// Generates process-local stable command identifiers used only for handle lookup.
    /// 生成仅用于句柄查找的进程内稳定命令标识。
    next_command_sequence: u64,
    /// Retains every handle identity issued during this engine instance, including released commands.
    /// 保留该引擎实例已签发的全部句柄身份，包括已释放命令。
    issued_command_ids: BTreeSet<CommandId>,
}

/// Retains one command's frozen configuration, host identity, and internal flow state.
/// 保留一个命令的冻结配置、宿主身份与内部流程状态。
struct CommandRecord {
    /// Carries the public handle.
    /// 携带公共句柄。
    handle: CommandHandle,
    /// Retains the exact immutable snapshot for the whole command lifetime.
    /// 在命令完整生命周期内保留精确不可变快照。
    snapshot: Arc<ConfigSnapshot>,
    /// Carries the public lifecycle status.
    /// 携带公共生命周期状态。
    status: CommandStatus,
    /// Carries the private typed flow state.
    /// 携带私有强类型流程状态。
    state: FlowState,
    /// Identifies the only accepted next effect result.
    /// 标识唯一接受的下一个 Effect 结果。
    pending_effect_id: Option<EffectId>,
    /// Retains the exact pending effect request for checkpoint export and resume.
    /// 为检查点导出与恢复保留精确待处理 Effect 请求。
    pending_effect: Option<EffectRequest>,
    /// Retains the exact durable key of the pending mutation for reconciliation.
    /// 保留待处理变更的精确持久幂等键以供对账。
    pending_mutation_idempotency_key: Option<IdempotencyKey>,
    /// Generates stable effect identities within the command.
    /// 在命令内生成稳定 Effect 标识。
    next_effect_sequence: u32,
    /// Retains the final failure or uncertain diagnostic.
    /// 保留最终失败或结果不确定诊断。
    last_error: Option<CoreError>,
    /// Carries the host-provided persistent operation identity for mutating commands.
    /// 为变更命令携带由宿主提供的持久操作身份。
    mutation_identity: Option<MutationIdentity>,
    /// Records already-received effect result summaries for checkpoint exports.
    /// 为检查点导出记录已接收 Effect 结果摘要。
    received_effects: Vec<String>,
    /// Retains the canonical command input for checkpoint export and reconstruction.
    /// 为检查点导出与重建保留规范命令输入。
    checkpoint_command_input: Option<Vec<u8>>,
    /// Records whether any optional host stage degraded during execution.
    /// 记录执行期间是否有可选宿主阶段发生降级。
    degraded: bool,
}

/// Enumerates the shared retrieval pipeline stages.
/// 枚举共享检索流水线阶段。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum PipelineStage {
    /// Awaits channels and their complete records from one indivisible observation.
    /// 等待同次不可分割观察中的通道及完整记录。
    Observation,
    /// Awaits channel query embeddings.
    /// 等待通道查询嵌入。
    Embedding,
    /// Awaits physical vector candidates.
    /// 等待物理向量候选。
    Vector,
    /// Awaits physical lexical candidates.
    /// 等待物理词法候选。
    Lexical,
    /// Awaits memory record facts.
    /// 等待记忆记录事实。
    Records,
    /// Awaits context evidence facts.
    /// 等待情境证据事实。
    Evidence,
    /// Awaits one provider rerank result.
    /// 等待一次 Provider 重排结果。
    Rerank,
}

/// Describes what the shared retrieval pipeline does once it finishes.
/// 描述共享检索流水线完成后的去向。
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum PipelineFollowUp {
    /// Completes one `MemorySearch` command.
    /// 完成一个 `MemorySearch` 命令。
    MemorySearch,
    /// Advances one `MemorySearchBatch` group.
    /// 推进一个 `MemorySearchBatch` 分组。
    MemorySearchBatchGroup {
        /// Retains first successful observations for this one source Search invocation.
        /// 为本次来源Search保留首次成功观察，不与其他命令共享。
        evidence_cache: ContextEvidenceCache,
        /// Retains the validated command.
        /// 保留已校验命令。
        command: MemorySearchBatchCommand,
        /// Retains unique representatives and their caller-position mapping.
        /// 保留唯一代表及其调用位置映射。
        query_plan: QueryBatchPlan,
        /// Retains one batch of embeddings aligned to unique representatives.
        /// 保留与唯一代表对齐的一批嵌入。
        embeddings: Vec<Vec<f32>>,
        /// Retains the current group cursor.
        /// 保留当前分组游标。
        group: usize,
        /// Retains completed groups.
        /// 保留已完成分组。
        completed: Vec<MemorySearchGroup>,
    },
    /// Continues independent `PreCheck` query groups before constructing the combined reviewer view.
    /// 在构造合并审核视图前继续独立 `PreCheck` 查询分组。
    PreCheckGroup(Box<PendingPreCheckSearch>),
    /// Continues `Analyze` with the L2 review.
    /// 以 L2 审核继续 `Analyze`。
    AnalyzeReview(Box<PendingAnalyzeSearch>),
    /// Continues one candidate-local direct-write semantic review.
    /// 继续一条候选局部的主动写入语义审核。
    DirectWriteReview {
        /// Retains the complete caller-ordered review state.
        /// 保留完整的调用方有序审核状态。
        pending: Box<PendingDirectWriteReview>,
        /// Identifies the caller item whose retrieval pipeline is running.
        /// 标识当前运行检索流水线的调用方项目。
        item_index: usize,
    },
}

/// Retains only successful per-identity evidence observations within one source Search call, including empty rows.
/// 仅在一次来源Search调用内保留逐身份成功证据观察，包含成功空行集；不缓存匹配分或读取失败。
type ContextEvidenceCache =
    BTreeMap<MemoryId, Vec<vulcan_memorymesh_core_contracts::MemoryContextEvidence>>;

/// Accumulates the shared retrieval pipeline state across candidate channel effects.
/// 跨候选通道 Effect 累加共享检索流水线状态。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SearchWork {
    /// Restricts physical candidates before truncation, independently of current-session compact visibility.
    /// 在截断前限制物理候选，独立于当前会话压缩可见性。
    candidate_scope: Option<vulcan_memorymesh_core_contracts::MemoryCandidateScope>,
    /// Identifies the exact subject.
    /// 标识精确主体。
    subject_id: vulcan_memorymesh_core_contracts::SubjectId,
    /// Carries the resolved scope contract.
    /// 携带已解析作用域契约。
    scope_access: vulcan_memorymesh_core_contracts::ScopeAccess,
    /// Carries normalized queries.
    /// 携带规范化查询。
    queries: Vec<String>,
    /// Carries one embedding per query.
    /// 每条查询携带一个嵌入。
    embeddings: Vec<Vec<f32>>,
    /// Identifies the vector space.
    /// 标识向量空间。
    vector_space_id: VectorSpaceId,
    /// Carries the optional exact-session boundary.
    /// 携带可选精确会话边界。
    boundary: Option<SessionRecallBoundary>,
    /// Limits final public results.
    /// 限制最终公共结果数。
    limit: u32,
    /// Expands only replacement retrieval and survives every checkpoint/resume boundary.
    /// 仅扩容更替检索，且跨每个检查点与恢复边界保留，不从作用域形状推断。
    enable_hard_dedupe_pool: bool,
    /// Carries the business evaluation time for read-time decay.
    /// 携带用于读时衰减的业务评估时间。
    now_ns: i64,
    /// Records that an allowed complete-observation failure activated the configured split-read path.
    /// 记录允许的完整观察失败已激活配置的分离读取路径；该事实跨检查点保持。
    separate_fallback_active: bool,
    /// Carries physical vector candidates.
    /// 携带物理向量候选。
    vector: Vec<PhysicalCandidate>,
    /// Carries physical lexical candidates.
    /// 携带物理词法候选。
    lexical: Vec<PhysicalCandidate>,
    /// Carries fetched memory records.
    /// 携带已获取记忆记录。
    records: BTreeMap<MemoryId, MemoryRecord>,
    /// Carries fetched context evidence groups.
    /// 携带已获取情境证据分组。
    evidence: Vec<ContextEvidenceGroup>,
}

/// Runs the Core-owned retrieval pipeline as one typed state machine continuation.
/// 将 Core 自有检索流水线作为一个强类型状态机续体运行。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SearchPipeline {
    /// Carries the current stage.
    /// 携带当前阶段。
    stage: PipelineStage,
    /// Carries the accumulated search work.
    /// 携带累计检索工作。
    work: SearchWork,
    /// Describes the completion continuation.
    /// 描述完成续体。
    follow_up: PipelineFollowUp,
    /// Carries fused hits awaiting rerank, or decayed hits awaiting live context evidence.
    /// 携带等待重排的融合命中，或等待实时情境证据的已衰减命中。
    fused: Vec<SearchHit>,
    /// Carries the fused-hit indexes represented by provider-facing rerank documents.
    /// 携带由面向 Provider 的重排文档表示的融合命中索引。
    rerank_indices: Vec<usize>,
}

/// Retains independent query state until `PreCheck` can prepare its reviewer set.
/// 在 `PreCheck` 构造审核集合之前保存独立查询状态，避免恢复时混淆查询归属。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PendingPreCheckSearch {
    /// Shares successful observations across this one intent-query Search invocation.
    /// 在本次意图查询Search调用内共享成功观察。
    evidence_cache: ContextEvidenceCache,
    /// Retains the validated command and final accepted-memory bound.
    /// 保留已验证命令及最终采纳数量上限。
    command: PreCheckCommand,
    /// Retains the already scrubbed current user prose.
    /// 保留已经脱敏的当前用户正文。
    content: String,
    /// Retains normalized queries in exact request order.
    /// 按精确请求顺序保留规范化查询。
    queries: Vec<String>,
    /// Retains the single accepted embedding batch aligned with those queries.
    /// 保留与查询对齐、已一次性接受的嵌入批次。
    embeddings: Vec<Vec<f32>>,
    /// Retains the next query's zero-based position.
    /// 保留下一个查询的零基位置。
    group: usize,
    /// Retains admitted unique memories and their private source versions, without raw duplicate query groups.
    /// 保留已准入唯一记忆及其私有来源版本，不保存重复的原始查询组。
    completed: ReviewAccumulator,
    /// Retains L1 reasoning, absent only for explicit forced recall.
    /// 保留 L1 理由，仅在明确强制召回时缺省。
    intent_reason: Option<String>,
}

/// Carries a `PreCheck` review outcome awaiting durable reinforcement.
/// 携带等待持久强化的 `PreCheck` 审核结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PendingPreCheck {
    /// Carries the queries used.
    /// 携带使用查询。
    queries: Vec<String>,
    /// Carries the candidate hits.
    /// 携带候选命中。
    candidates: Vec<ReviewCandidateState>,
    /// Carries accepted hits.
    /// 携带已接受命中。
    accepted: Vec<ReviewCandidateState>,
}

/// Retains maintenance facts until the Host commit receipt is available.
/// 在宿主提交收据可用前保留维护结果事实。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PendingMaintenanceResult {
    /// Carries affected memory identities in caller order.
    /// 按调用方顺序携带受影响记忆标识。
    affected_memory_ids: Vec<MemoryId>,
    /// Counts vectors rebuilt by this operation.
    /// 统计本次操作重建的向量数。
    rebuilt_vector_count: u32,
}

/// Carries an `Analyze` review outcome awaiting the derived atomic mutation.
/// 携带等待派生原子变更的 `Analyze` 审核结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PendingAnalyze {
    /// Distinguishes an authoritative L1 fallback from successful L2 after optional retrieval degradation.
    /// 区分权威L1回退与可选检索降级后成功的L2，不能用命令全局降级标志推断画像状态。
    review_fallback_to_l1: bool,
    /// Retains the validated analysis command.
    /// 保留已校验分析命令。
    command: PostActionAnalyzePendingTurnCommand,
    /// Retains the persisted pending turn.
    /// 保留已持久化待处理轮次。
    turn: Turn,
    /// Retains the validated dominant user act extracted by L1.
    /// 保留由 L1 提取并已校验的主要用户行为。
    user_input_kind: vulcan_memorymesh_core_contracts::TurnInputKind,
    /// Retains the validated compact turn details extracted by L1.
    /// 保留由 L1 提取并已校验的紧凑轮次详情。
    analysis_details: String,
    /// Retains memory candidates.
    /// 保留记忆候选。
    memories: Vec<MemoryCandidate>,
    /// Retains candidate embeddings.
    /// 保留候选嵌入。
    embeddings: Vec<Vec<f32>>,
    /// Retains profile candidates.
    /// 保留画像候选。
    profiles: Vec<ProfileCandidate>,
    /// Retains context evidence observations.
    /// 保留情境证据观测。
    context_evidence: Vec<ContextEvidenceCandidate>,
    /// Retains each memory candidate with only its own similar stored memories.
    /// 保留每条记忆候选及仅属于它的相似已存记忆。
    memory_reviews: Vec<MemoryReviewCandidate>,
    /// Maps dense L2 memory positions to the admitted candidate positions.
    /// 将稠密 L2 记忆位置映射到准入候选位置。
    reviewer_to_candidate: Vec<u32>,
    /// Retains authoritative hard-dedupe outcomes that L2 must not override.
    /// 保留 L2 不得覆盖的权威硬去重结果。
    hard_dropped: Vec<vulcan_memorymesh_core_contracts::DroppedCandidate>,
    /// Retains the exact visible active profile nodes eligible for reviewed retirement.
    /// 保留可由审核退役的精确可见活跃画像节点。
    current_profile_nodes: Vec<ProfileNode>,
    /// Retains the L2 review output.
    /// 保留 L2 审核输出。
    review: LlmOutput,
    /// Audits permanently invalid persistence inputs in original candidate coordinates.
    /// 在原始候选坐标中审计永久非法的持久化输入。
    embedding_dropped: Vec<vulcan_memorymesh_core_contracts::EmbeddingDroppedInput>,
}

/// Retains one profile-instruction result template and the slots that require receipt binding.
/// 保留一份画像指令结果模板及需要提交收据绑定的槽位。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PendingProfileInstruction {
    /// Carries accepted placeholder nodes, all retired nodes, and the review rationale.
    /// 携带已接受占位节点、全部退役节点与审核理由。
    outcome: ProfileInstructionResult,
    /// Carries accepted-node slots in exact outcome order.
    /// 按结果精确顺序携带已接受节点槽位。
    accepted_slots: Vec<EntitySlotId>,
}

/// Retains one direct memory placeholder and its command-local creation slot.
/// 保留一条主动记忆占位记录及其命令局部创建槽位。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PendingMemoryWrite {
    /// Carries the record whose identity and version are replaced from the commit receipt.
    /// 携带将由提交收据替换身份与版本的记录。
    memory: MemoryRecord,
    /// Identifies the new-memory slot expected in the commit receipt.
    /// 标识提交收据中预期的新记忆槽位。
    slot: EntitySlotId,
}

/// Describes how one batch result row obtains its final durable memory record.
/// 描述一个批量结果行如何取得其最终持久记忆记录。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum PendingBatchMemory {
    /// Creates one new record through a command-local slot.
    /// 通过一个命令局部槽位创建新记录。
    New {
        /// Identifies the slot bound by the host commit.
        /// 标识由宿主提交绑定的槽位。
        slot: EntitySlotId,
        /// Carries the placeholder record awaiting binding.
        /// 携带等待绑定的占位记录。
        memory: MemoryRecord,
    },
    /// Reuses one exact existing durable record.
    /// 复用一条精确既有持久记录。
    Existing(MemoryRecord),
    /// Reuses one earlier result row from the same atomic batch.
    /// 复用同一原子批次中更早的结果行。
    Earlier(usize),
}

/// Retains batch result bindings while an atomic direct-memory commit is pending.
/// 在原子主动记忆提交等待期间保留批量结果绑定。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PendingMemoryWriteBatch {
    /// Carries one resolution source per caller-ordered item.
    /// 为每个按调用方顺序排列的项目携带一个解析来源。
    entries: Vec<PendingBatchMemory>,
    /// Reports whether each aligned item reused an existing or earlier record.
    /// 报告每个对齐项目是否复用了既有记录或更早记录。
    deduplicated: Vec<bool>,
}

/// Retains one normalized direct write while recent durable soft deduplication is queried.
/// 在查询近期持久化软去重期间保留一条已规范化主动写入。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PendingMemoryWriteDedupe {
    /// Retains the complete normalized command.
    /// 保留完整的已规范化命令。
    command: MemoryWriteCommand,
    /// Carries the versioned Core-owned semantic hash.
    /// 携带版本化 Core 自有语义哈希。
    dedupe_hash: String,
}

/// Retains one normalized atomic batch while recent durable soft deduplication is queried.
/// 在查询近期持久化软去重期间保留一批已规范化原子写入。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PendingMemoryWriteBatchDedupe {
    /// Retains the complete normalized command.
    /// 保留完整的已规范化命令。
    command: MemoryWriteBatchCommand,
    /// Carries one versioned semantic hash per caller item.
    /// 为每个调用方项目携带一个版本化语义哈希。
    dedupe_hashes: Vec<String>,
}

/// Retains one batch and its pre-resolved durable reuse entries while embeddings are requested.
/// 在请求嵌入期间保留一批写入及其已预解析持久复用项。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PendingMemoryWriteBatchEmbedding {
    /// Retains the complete normalized command.
    /// 保留完整的已规范化命令。
    command: MemoryWriteBatchCommand,
    /// Carries one semantic hash per caller item.
    /// 为每个调用方项目携带一个语义哈希。
    dedupe_hashes: Vec<String>,
    /// Carries one optional durable soft-deduplication hit per caller item.
    /// 为每个调用方项目携带一个可选持久软去重命中。
    existing: Vec<Option<MemoryRecord>>,
    /// Retains exact-item collapse separately from query-text aliases, before any embedding is requested.
    /// 在请求嵌入前保留精确项目折叠，独立于查询文本别名。
    queries: DirectQueryPlan,
}

/// Separates soft hits, exact-item aliases and textual query aliases for one direct-write batch.
/// 为一次主动写入批次区分软命中、精确项目别名与文本查询别名，不扩大任何候选的写授权。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DirectQueryPlan {
    /// Maps compact review candidates to unique query representatives.
    /// 将压缩后的评审候选映射至唯一查询代表。
    query_plan: QueryBatchPlan,
    /// Maps each compact review candidate to its original caller item.
    /// 将每个压缩评审候选映射回原调用项。
    review_to_item: Vec<usize>,
    /// Identifies an earlier soft-miss representative for an exact duplicate, not a soft-hit row.
    /// 为精确重复项标记更早的软未命中代表，不能指向已软命中的记录。
    earlier: Vec<Option<usize>>,
}

/// Selects the public result shape produced by the shared direct-write review flow.
/// 选择共享主动写入审核流程生成的公共结果形态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum DirectWriteResultMode {
    /// Produces one `MemoryWrite` result.
    /// 生成一个 `MemoryWrite` 结果。
    Single,
    /// Produces one caller-ordered `MemoryWriteBatch` result.
    /// 生成一个按调用方顺序排列的 `MemoryWriteBatch` 结果。
    Batch,
}

/// Retains complete query facts before any candidate-specific category or review decision is applied.
/// 在应用候选专属类别或评审决定之前保留完整查询事实；别名只复用这些事实，不复用最终决定。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ReviewQueryFacts {
    /// Carries the completed public hit sequence, including provider order and score explanations.
    /// 携带完整命中序列，包括提供方顺序与评分解释。
    hits: Vec<SearchHit>,
    /// Carries the independent post-score, pre-MMR records for category-local hard deduplication.
    /// 携带独立的评分后、MMR前记录，供各类别独立硬去重。
    hard_dedupe_pool: Vec<MemoryRecord>,
    /// Retains the contract-bound vector threshold used by this exact query path.
    /// 保留本条精确查询路径使用的向量契约阈值，避免组合与降级通道混用。
    hard_dedupe_threshold: f64,
    /// Identifies the vector producer whose threshold was retained for this query.
    /// 标识本条查询保留阈值所属的向量生产者，供检查点按冻结配置复核。
    hard_dedupe_contract: vulcan_memorymesh_core_contracts::ScoreContractRef,
}

/// Retains one Analyze source Search invocation, including the profile snapshot read before it began.
/// 保留一次Analyze来源Search调用，包括查询开始前已读取的画像快照。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PendingAnalyzeSearch {
    /// Retains the admitted L1 facts and the already observed profile snapshot.
    /// 保留已准入L1事实及已经观察的画像快照。
    pending: PendingAnalyze,
    /// Maps each admitted candidate to its first unique query representative.
    /// 将各准入候选映射至首个唯一查询代表。
    query_plan: QueryBatchPlan,
    /// Stores strict vectors once per unique query, never persistence vectors.
    /// 按唯一查询保存严格向量，不存储持久化向量。
    embeddings: Vec<Vec<f32>>,
    /// Stores completed unique queries in order; length is the next query cursor.
    /// 按序保存已完成唯一查询，长度即下一个查询游标。
    completed: Vec<ReviewQueryFacts>,
    /// Shares successful context observations only within this source Search call.
    /// 仅在本次来源Search内共享成功情境观察。
    evidence_cache: ContextEvidenceCache,
}

/// Borrows the fixed command-owned provenance needed to construct a direct-write record.
/// 借用构造主动写入记录所需的固定命令来源，不从已有命中或候选结果推断归属。
struct DirectRecordSource<'a> {
    /// Identifies the original command subject.
    /// 标识原命令主体。
    subject_id: &'a SubjectId,
    /// Identifies the original command session.
    /// 标识原命令会话。
    session_source_id: &'a vulcan_memorymesh_core_contracts::SessionSourceId,
    /// Identifies the original command vector space.
    /// 标识原命令向量空间。
    vector_space_id: &'a VectorSpaceId,
    /// Retains the original write timestamp for defaults and durable provenance.
    /// 为默认值及持久来源保留原写入时间戳。
    occurred_at_ns: i64,
}

/// Retains candidate-local direct-write review state across retrieval and LLM effects.
/// 在检索与 LLM Effect 之间保留候选局部的主动写入审核状态。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PendingDirectWriteReview {
    /// Shares successful observations across the candidate batch of this direct-write review.
    /// 在本次主动写入审核的候选批次间共享成功观察。
    evidence_cache: ContextEvidenceCache,
    /// Retains the pre-query item partition and immutable representative mapping.
    /// 保留查询前项目划分及不可变代表映射。
    queries: DirectQueryPlan,
    /// Retains completed unique-query facts; the length is the next unique query cursor.
    /// 保留已完成唯一查询事实，长度即下一唯一查询游标。
    query_facts: Vec<ReviewQueryFacts>,
    /// Retains the original write time independently of deduped or filtered candidates.
    /// 独立于去重或过滤后的候选保留原始写命令时间。
    occurred_at_ns: i64,
    /// Retains the explicit replacement filter across all candidate-local queries and checkpoints.
    /// 在全部候选局部查询及检查点中保留显式替代过滤。
    candidate_scope: vulcan_memorymesh_core_contracts::MemoryCandidateScope,
    /// Selects the public result shape.
    /// 选择公共结果形态。
    mode: DirectWriteResultMode,
    /// Retains proposed records in caller order.
    /// 按调用方顺序保留拟写入记录。
    proposed: Vec<MemoryRecord>,
    /// Retains embeddings aligned to proposed records.
    /// 保留与拟写入记录对齐的嵌入。
    embeddings: Vec<Vec<f32>>,
    /// Retains one resolved result source per caller item.
    /// 为每个调用方项目保留一个已解析结果来源。
    entries: Vec<Option<PendingBatchMemory>>,
    /// Maps reviewer-local indexes back to caller item indexes.
    /// 将审核器局部索引映射回调用方项目索引。
    reviewer_to_item: Vec<usize>,
    /// Carries candidate-local reviewer inputs after hard deduplication.
    /// 携带硬去重后的候选局部审核器输入。
    memory_reviews: Vec<MemoryReviewCandidate>,
    /// Carries the exact scopes this command may mutate through supersede decisions.
    /// 携带本命令可通过替代决策变更的精确作用域。
    authorized_supersede_scopes: Vec<ScopeId>,
    /// Carries the original source Search access, independently of each candidate's write target.
    /// 携带原来源Search访问范围，独立于各候选的写入目标。
    scope_access: ScopeAccess,
}

/// Carries the semantic noise evaluation working state.
/// 携带语义 Noise 评估工作状态。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct IntakeNoiseContext {
    /// Retains role, resource, and partial-failure audit facts across every continuation.
    /// 在每个续体中保留角色、资源及部分故障审计事实。
    noise: NoiseAudit,
    /// Retains stable category batches so one provider failure does not disable other categories.
    /// 保留稳定类别批次，使单个提供方故障不会禁用其他类别。
    prototype_categories: Vec<String>,
    /// Identifies the next category batch, including across checkpoint recovery.
    /// 标识下一个类别批次，并在检查点恢复时保留。
    prototype_category_index: usize,
    /// Accumulates only complete successful category batches.
    /// 仅累积完整成功的类别批次。
    resolved_prototypes: Vec<NoisePrototype>,
    /// Preserves user then assistant query roles and their separate scrubbed texts.
    /// 保留用户在前、助手在后的查询角色及各自脱敏正文。
    queries: Vec<(Role, String)>,
    /// Carries phrase targets awaiting embedding as (category, phrase, threshold).
    /// 以（类别、短语、阈值）形式携带等待嵌入的短语目标。
    phrase_targets: Vec<(String, String, f64)>,
    /// Retains the validated intake command.
    /// 保留已校验登记命令。
    command: PostActionIntakeCommand,
}

/// Enumerates every private flow continuation without dynamic payloads.
/// 枚举每个私有流程续体且不使用动态载荷。
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum FlowState {
    /// Indicates no further transition is valid.
    /// 表示不再允许任何转换。
    Terminal,
    /// Waits for `PreCheck` L1 intent.
    /// 等待 `PreCheck` L1 意图。
    PreCheckIntent {
        /// Retains the validated command.
        /// 保留已校验命令。
        command: PreCheckCommand,
        /// Retains scrubbed normalized user content.
        /// 保留已脱敏规范化用户内容。
        content: String,
    },
    /// Waits for `PreCheck` query embeddings.
    /// 等待 `PreCheck` 查询嵌入。
    PreCheckEmbedding {
        /// Retains the validated command.
        /// 保留已校验命令。
        command: PreCheckCommand,
        /// Retains scrubbed normalized user content.
        /// 保留已脱敏规范化用户内容。
        content: String,
        /// Retains complete search queries.
        /// 保留完整搜索查询。
        queries: Vec<String>,
        /// Retains L1 reasoning without inventing a rationale for forced recall.
        /// 保留 L1 理由，不为强制召回编造理由。
        intent_reason: Option<String>,
    },
    /// Runs the shared retrieval pipeline.
    /// 运行共享检索流水线。
    SearchPipeline(Box<SearchPipeline>),
    /// Waits for `PreCheck` L2 candidate review.
    /// 等待 `PreCheck` L2 候选审核。
    PreCheckReview {
        /// Retains the validated command.
        /// 保留已校验命令。
        command: PreCheckCommand,
        /// Retains the queries used.
        /// 保留使用的查询。
        queries: Vec<String>,
        /// Retains the candidate hits.
        /// 保留候选命中。
        candidates: Vec<ReviewCandidateState>,
    },
    /// Waits for `PreCheck` adoption reinforcement mutation.
    /// 等待 `PreCheck` 采纳强化变更。
    PreCheckReinforce(Box<PendingPreCheck>),
    /// Waits for one semantic noise prototype bundle query.
    /// 等待一次语义 Noise 原型束查询。
    IntakeNoiseQuery(Box<IntakeNoiseContext>),
    /// Waits for phrase prototype embeddings.
    /// 等待短语原型嵌入。
    IntakeNoiseEmbed(Box<IntakeNoiseContext>),
    /// Waits for one prototype bundle cache mutation.
    /// 等待一次原型束缓存变更。
    IntakeNoiseStore {
        /// Retains the noise context awaiting cache write.
        /// 保留等待缓存写入的 Noise 上下文。
        context: Box<IntakeNoiseContext>,
        /// Carries freshly built prototype rows.
        /// 携带新构建的原型行。
        prototypes: Vec<NoisePrototype>,
    },
    /// Waits for one turn-text embedding for semantic noise scoring.
    /// 等待一个用于语义 Noise 评分的轮次文本嵌入。
    IntakeNoiseScore {
        /// Retains the noise context with resolved prototypes.
        /// 保留带已解析原型的 Noise 上下文。
        context: Box<IntakeNoiseContext>,
        /// Carries the resolved prototype rows.
        /// 携带已解析原型行。
        prototypes: Vec<NoisePrototype>,
    },
    /// Waits for one durable `PostAction` intake turn mutation.
    /// 等待一次持久 `PostAction` 登记轮次变更。
    IntakeTurn {
        /// Preserves the completed admission decision while its turn commit is pending.
        /// 在轮次提交待定时保留已经完成的准入决策。
        noise: NoiseAudit,
        /// Identifies the pending turn slot bound at commit.
        /// 标识提交时绑定的待处理轮次槽位。
        turn_slot: EntitySlotId,
    },
    /// Waits for one durable analysis attempt identity mutation.
    /// 等待一次持久分析尝试身份变更。
    AnalyzeAttempt {
        /// Retains the validated analysis command.
        /// 保留已校验分析命令。
        command: PostActionAnalyzePendingTurnCommand,
    },
    /// Waits for one pending-turn details load.
    /// 等待一次待处理轮次详情加载。
    AnalyzeLoadTurn {
        /// Retains the validated analysis command.
        /// 保留已校验分析命令。
        command: PostActionAnalyzePendingTurnCommand,
    },
    /// Waits for `Analyze` L1 analysis.
    /// 等待 `Analyze` L1 分析。
    AnalyzeAnalysis {
        /// Retains the validated analysis command.
        /// 保留已校验分析命令。
        command: PostActionAnalyzePendingTurnCommand,
        /// Retains the persisted pending turn.
        /// 保留已持久化待处理轮次。
        turn: Turn,
    },
    /// Waits for `Analyze` candidate embeddings.
    /// 等待 `Analyze` 候选嵌入。
    AnalyzeEmbedding(Box<PendingAnalyze>),
    /// Waits for exact visible profile nodes before the unified L2 review.
    /// 在统一 L2 审核前等待精确可见画像节点。
    AnalyzeProfileQuery(Box<PendingAnalyze>),
    /// Waits for `Analyze` L2 candidate review.
    /// 等待 `Analyze` L2 候选审核。
    AnalyzeReview(Box<PendingAnalyze>),
    /// Waits for accepted summary embeddings before any derived mutation is emitted.
    /// 在发出任何派生变更前等待已接受摘要嵌入。
    AnalyzePersistenceEmbedding(Box<PendingAnalyze>),
    /// Waits for the `Analyze` derived atomic mutation.
    /// 等待 `Analyze` 派生原子变更。
    AnalyzeDerived(Box<PendingAnalyze>),
    /// Waits for one atomic `ChatCompact` boundary mutation.
    /// 等待一次原子 `ChatCompact` 边界变更。
    ChatCompactMutation,
    /// Waits for `MemoryDetails` storage query.
    /// 等待 `MemoryDetails` 存储查询。
    MemoryDetails(MemoryDetailsCommand),
    /// Waits for one batch details storage query.
    /// 等待一次批量详情存储查询。
    MemoryDetailsBatch(MemoryDetailsBatchCommand),
    /// Waits for one recent durable direct-write soft-deduplication query.
    /// 等待一次近期持久主动写入软去重查询。
    MemoryWriteDedupe(PendingMemoryWriteDedupe),
    /// Waits for direct memory embedding after a soft-deduplication miss.
    /// 在软去重未命中后等待主动记忆嵌入。
    MemoryWriteEmbedding(PendingMemoryWriteDedupe),
    /// Waits for one durable memory write mutation.
    /// 等待一次持久记忆写入变更。
    MemoryWriteMutation(PendingMemoryWrite),
    /// Waits for one caller-aligned recent durable batch soft-deduplication query.
    /// 等待一次与调用方对齐的近期持久批量软去重查询。
    MemoryWriteBatchDedupe(PendingMemoryWriteBatchDedupe),
    /// Waits for one atomic direct-memory batch embedding call.
    /// 等待一次原子主动记忆批量嵌入调用。
    MemoryWriteBatchEmbedding(PendingMemoryWriteBatchEmbedding),
    /// Waits for one atomic batch-write mutation.
    /// 等待一次原子批量写入变更。
    MemoryWriteBatchMutation(PendingMemoryWriteBatch),
    /// Waits for the shared `PostAction` L2 reviewer to decide direct-write candidates.
    /// 等待共享 `PostAction` L2 审核器决定主动写入候选。
    MemoryWriteReview(Box<PendingDirectWriteReview>),
    /// Waits for strict summary embeddings only for records actually being created.
    /// 仅为实际创建的记录等待严格摘要嵌入。
    MemoryWritePersistenceEmbedding {
        /// Retains caller-aligned review decisions and reuse entries.
        /// 保留调用方对齐的审核决策与复用项。
        pending: Box<PendingDirectWriteReview>,
        /// Retains exact versioned supersede targets until all embeddings succeed.
        /// 在全部嵌入成功前保留精确带版本的替代目标。
        supersede_by_item: BTreeMap<usize, Vec<MemoryRecord>>,
    },
    /// Waits for direct-memory access-identity preflight.
    /// 等待主动记忆删除的访问身份预检。
    MemoryDeletePreflight(MemoryDeleteCommand),
    /// Waits for direct memory deletion.
    /// 等待直接记忆删除。
    MemoryDeleteMutation,
    /// Waits for batch-delete existence preflight.
    /// 等待批量删除存在性预检。
    MemoryDeleteBatchPreflight(MemoryDeleteBatchCommand),
    /// Retains the partial public outcome while an atomic found-item delete executes.
    /// 在已找到项目的原子删除执行期间保留部分公共结果。
    MemoryDeleteBatchMutation(MemoryDeleteBatchResult),
    /// Waits for profile node query.
    /// 等待画像节点查询。
    ProfileQuery(ProfileQueryCommand),
    /// Waits for profile nodes before instruction review.
    /// 等待指令审核前的画像节点。
    ProfileInstructionQuery(ProfileInstructionCommand),
    /// Waits for profile instruction inference.
    /// 等待画像指令推理。
    ProfileInstructionLlm {
        /// Retains the validated instruction command.
        /// 保留已校验画像指令命令。
        command: ProfileInstructionCommand,
        /// Retains current visible nodes for exact retirement validation.
        /// 保留当前可见节点以进行精确退役校验。
        current_nodes: Vec<ProfileNode>,
    },
    /// Waits for profile instruction mutation.
    /// 等待画像指令变更。
    ProfileInstructionMutation(Box<PendingProfileInstruction>),
    /// Waits for retention scan facts.
    /// 等待保留扫描事实。
    RetentionQuery(RetentionCommand),
    /// Waits for exact rebuild target details.
    /// 等待精确重建目标详情。
    MaintenanceRebuildQuery(MaintenanceCommand),
    /// Waits for exact lifecycle-maintenance target details.
    /// 等待精确生命周期维护目标详情。
    MaintenanceLifecyclePreflight(MaintenanceCommand),
    /// Waits for rebuilt vector embeddings.
    /// 等待重建向量嵌入。
    MaintenanceRebuildEmbedding {
        /// Retains the validated maintenance command.
        /// 保留已校验维护命令。
        command: MaintenanceCommand,
        /// Retains exact active records in caller order.
        /// 按调用方顺序保留精确活跃记录。
        memories: Vec<MemoryRecord>,
    },
    /// Waits for one atomic maintenance mutation.
    /// 等待一次原子维护变更。
    MaintenanceMutation(PendingMaintenanceResult),
}

impl CommandRecord {
    /// Builds one atomic mutation batch with a bounded, role-separated derived identity.
    /// 构建一个使用有界且按角色隔离的派生身份的原子变更批次。
    fn mutation_batch(
        &self,
        role: &str,
        mutations: Vec<Mutation>,
    ) -> Result<MutationBatch, CoreError> {
        let identity = self.mutation_identity.clone().ok_or_else(|| {
            command_error(
                ErrorCode::Internal,
                "mutating flow lost its host operation identity",
            )
        })?;
        Ok(MutationBatch {
            identity: derive_batch_identity(&identity, role)?,
            mutations,
        })
    }
}

/// Derives one deterministic sub-operation identity from the host operation identity.
/// 从宿主操作身份派生一个确定性的子操作身份。
fn derive_batch_identity(
    identity: &MutationIdentity,
    role: &str,
) -> Result<MutationIdentity, CoreError> {
    vulcan_memorymesh_core_contracts::derive_mutation_identity(identity, role)
        .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))
}

impl CommandEngine {
    /// Creates an empty command engine.
    /// 创建空命令引擎。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts one command against an immutable configuration snapshot.
    /// 针对一个不可变配置快照启动命令。
    ///
    /// # Errors
    /// Returns a stable validation, identity, or configuration error before publishing a handle.
    /// 在发布句柄前返回稳定校验、身份或配置错误。
    pub fn start(
        &mut self,
        command: CoreCommand,
        snapshot: Arc<ConfigSnapshot>,
    ) -> Result<(CommandHandle, CommandStep), CoreError> {
        validate_command(&command, &snapshot)?;
        // Verify the host-provided persistent identity before any durable effect is emitted.
        // 在任何持久 Effect 发出前校验宿主提供的持久身份。
        let mutation_identity = verify_identity(&command, &snapshot)?;
        // Allocate a process-local handle without colliding with checkpoints restored into this
        // instance; the identifier never acts as durable operation or business identity.
        // 分配一个不会与本实例已恢复检查点冲突的进程内句柄；该标识绝不充当持久操作或业务身份。
        let command_id = loop {
            self.next_command_sequence =
                self.next_command_sequence.checked_add(1).ok_or_else(|| {
                    command_error(
                        ErrorCode::Internal,
                        "command identifier sequence is exhausted",
                    )
                })?;
            let candidate = CommandId::new(format!("command-{:016x}", self.next_command_sequence))
                .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
            if !self.issued_command_ids.contains(&candidate) {
                break candidate;
            }
        };
        let handle = CommandHandle {
            command_id: command_id.clone(),
            config_snapshot_id: snapshot.id.clone(),
        };
        let mut record = CommandRecord {
            handle: handle.clone(),
            snapshot,
            status: CommandStatus::Waiting,
            state: FlowState::Terminal,
            pending_effect_id: None,
            pending_effect: None,
            pending_mutation_idempotency_key: None,
            next_effect_sequence: 0,
            last_error: None,
            mutation_identity,
            received_effects: Vec::new(),
            checkpoint_command_input: Some(vulcan_memorymesh_core_contracts::encode_command(
                &command,
            )),
            degraded: false,
        };
        let step = initialize(&mut record, command)?;
        self.issued_command_ids.insert(command_id.clone());
        self.records.insert(command_id, record);
        Ok((handle, step))
    }

    /// Resumes one waiting command with exactly one matching effect result.
    /// 使用恰好一个匹配的 Effect 结果恢复等待中的命令。
    ///
    /// # Errors
    /// Returns a stable error for unknown handles, lifecycle violations, effect mismatch, or failed flows.
    /// 当句柄未知、生命周期违规、Effect 不匹配或流程失败时返回稳定错误。
    pub fn resume(
        &mut self,
        handle: &CommandHandle,
        result: EffectResult,
    ) -> Result<CommandStep, CoreError> {
        let mut record = self.records.remove(&handle.command_id).ok_or_else(|| {
            command_error(
                ErrorCode::CommandNotFound,
                format!("command not found: {}", handle.command_id),
            )
        })?;
        if record.handle.config_snapshot_id != handle.config_snapshot_id {
            self.records
                .insert(record.handle.command_id.clone(), record);
            return Err(command_error(
                ErrorCode::CommandNotFound,
                "command snapshot identity does not match the handle",
            ));
        }
        match record.status {
            CommandStatus::Waiting => {}
            CommandStatus::Cancelled => {
                self.records
                    .insert(record.handle.command_id.clone(), record);
                return Err(command_error(
                    ErrorCode::CommandCancelled,
                    "command was cancelled",
                ));
            }
            CommandStatus::ResultUncertain => {
                let error = record.last_error.clone().unwrap_or_else(|| {
                    command_error(
                        ErrorCode::StorageResultUncertain,
                        "command storage result is uncertain",
                    )
                });
                self.records
                    .insert(record.handle.command_id.clone(), record);
                return Err(error);
            }
            CommandStatus::Completed | CommandStatus::Failed => {
                let error = record.last_error.clone().unwrap_or_else(|| {
                    command_error(ErrorCode::CommandCompleted, "command is already terminal")
                });
                self.records
                    .insert(record.handle.command_id.clone(), record);
                return Err(error);
            }
        }
        if record.pending_effect_id.as_ref() != Some(&result.effect_id) {
            self.records
                .insert(record.handle.command_id.clone(), record);
            return Err(command_error(
                ErrorCode::EffectMismatch,
                format!(
                    "effect result does not match pending effect: {}",
                    result.effect_id
                ),
            ));
        }
        let received_summary = match effect_result_summary(&result.result) {
            Ok(summary) => summary,
            Err(error) => {
                self.records
                    .insert(record.handle.command_id.clone(), record);
                return Err(error);
            }
        };
        // Only a typed rejection proves non-commit; malformed acknowledgements may follow a write.
        // 仅强类型拒绝能够证明未提交；格式错误的确认也可能出现在写入之后。
        let unresolved_mutation = !matches!(
            &result.result,
            HostEffectResult::StorageMutation(MutationOutcome::Rejected { .. })
        ) && record
            .pending_effect
            .as_ref()
            .is_some_and(|request| matches!(&request.effect, HostEffect::StorageMutation(_)));
        let retained_state = unresolved_mutation.then(|| record.state.clone());
        record.pending_effect_id = None;
        record.received_effects.push(received_summary);
        let transition = advance(&mut record, result.result);
        let transition = preserve_invalid_mutation_outcome(
            &mut record,
            unresolved_mutation,
            retained_state,
            transition,
        );
        finalize_transition(&mut record, &transition);
        self.records
            .insert(record.handle.command_id.clone(), record);
        transition
    }

    /// Reconciles one uncertain storage mutation with the host's durable outcome lookup.
    /// 使用宿主持久结果查询来对账一个结果不确定的存储变更。
    ///
    /// # Errors
    /// Returns an error when the handle is unknown, the command is not uncertain, or the outcome
    /// remains uncertain or cannot complete the retained flow.
    /// 当句柄未知、命令并非结果不确定、结果仍不确定或无法完成保留流程时返回错误。
    pub fn reconcile(
        &mut self,
        handle: &CommandHandle,
        outcome: MutationOutcome,
    ) -> Result<CommandStep, CoreError> {
        let mut record = self.records.remove(&handle.command_id).ok_or_else(|| {
            command_error(
                ErrorCode::CommandNotFound,
                format!("command not found: {}", handle.command_id),
            )
        })?;
        let validation = if record.handle.config_snapshot_id != handle.config_snapshot_id {
            Err(command_error(
                ErrorCode::CommandNotFound,
                "command snapshot identity does not match the handle",
            ))
        } else if record.status != CommandStatus::ResultUncertain {
            Err(command_error(
                ErrorCode::InvalidArgument,
                "only a result-uncertain command may be reconciled",
            ))
        } else if matches!(outcome, MutationOutcome::Uncertain) {
            Err(command_error(
                ErrorCode::InvalidArgument,
                "reconciliation must resolve to committed or rejected",
            ))
        } else {
            Ok(())
        };
        if let Err(error) = validation {
            self.records
                .insert(record.handle.command_id.clone(), record);
            return Err(error);
        }
        let reconciled_result = HostEffectResult::StorageMutation(outcome);
        let received_summary = match effect_result_summary(&reconciled_result) {
            Ok(summary) => summary,
            Err(error) => {
                self.records
                    .insert(record.handle.command_id.clone(), record);
                return Err(error);
            }
        };
        // Validation failure must leave the original uncertainty and its error identity untouched.
        // 校验失败必须保持原有不确定状态及错误身份不变。
        record.status = CommandStatus::Waiting;
        record.last_error = None;
        let committed_mutation = matches!(
            &reconciled_result,
            HostEffectResult::StorageMutation(MutationOutcome::Committed { .. })
        ) && record
            .pending_effect
            .as_ref()
            .is_some_and(|request| matches!(&request.effect, HostEffect::StorageMutation(_)));
        let retained_state = committed_mutation.then(|| record.state.clone());
        record.received_effects.push(received_summary);
        let transition = advance(&mut record, reconciled_result);
        let transition = preserve_invalid_mutation_outcome(
            &mut record,
            committed_mutation,
            retained_state,
            transition,
        );
        finalize_transition(&mut record, &transition);
        self.records
            .insert(record.handle.command_id.clone(), record);
        transition
    }

    /// Cancels one non-terminal command without replaying or compensating effects.
    /// 取消一个非终态命令，且不重放或补偿 Effect。
    ///
    /// # Errors
    /// Returns an error when the handle is unknown or already terminal.
    /// 当句柄未知或已经终止时返回错误。
    pub fn cancel(&mut self, handle: &CommandHandle) -> Result<(), CoreError> {
        let record = self.records.get_mut(&handle.command_id).ok_or_else(|| {
            command_error(
                ErrorCode::CommandNotFound,
                format!("command not found: {}", handle.command_id),
            )
        })?;
        if record.handle.config_snapshot_id != handle.config_snapshot_id {
            return Err(command_error(
                ErrorCode::CommandNotFound,
                "command snapshot identity does not match the handle",
            ));
        }
        if record.status != CommandStatus::Waiting {
            return Err(command_error(
                ErrorCode::CommandCompleted,
                "only waiting commands may be cancelled",
            ));
        }
        record.status = CommandStatus::Cancelled;
        record.pending_effect_id = None;
        record.pending_effect = None;
        record.pending_mutation_idempotency_key = None;
        record.state = FlowState::Terminal;
        Ok(())
    }

    /// Releases one terminal command record after the caller no longer needs diagnostics.
    /// 在调用方不再需要诊断后释放一个终态命令记录。
    ///
    /// # Errors
    /// Returns an error when the handle is unknown, mismatched, or still waiting.
    /// 当句柄未知、不匹配或仍在等待时返回错误。
    pub fn release(&mut self, handle: &CommandHandle) -> Result<(), CoreError> {
        let record = self.records.get(&handle.command_id).ok_or_else(|| {
            command_error(
                ErrorCode::CommandNotFound,
                format!("command not found: {}", handle.command_id),
            )
        })?;
        if record.handle.config_snapshot_id != handle.config_snapshot_id {
            return Err(command_error(
                ErrorCode::CommandNotFound,
                "command snapshot identity does not match the handle",
            ));
        }
        if matches!(
            record.status,
            CommandStatus::Waiting | CommandStatus::ResultUncertain
        ) {
            return Err(command_error(
                ErrorCode::InvalidArgument,
                "waiting or result-uncertain command must be completed, reconciled, or cancelled before release",
            ));
        }
        self.records.remove(&handle.command_id);
        Ok(())
    }

    /// Returns stable lifecycle diagnostics for one command.
    /// 返回一个命令的稳定生命周期诊断。
    ///
    /// # Errors
    /// Returns an error when the handle is unknown or has a mismatched snapshot identity.
    /// 当句柄未知或快照标识不匹配时返回错误。
    pub fn diagnostics(&self, handle: &CommandHandle) -> Result<CommandDiagnostics, CoreError> {
        let record = self.records.get(&handle.command_id).ok_or_else(|| {
            command_error(
                ErrorCode::CommandNotFound,
                format!("command not found: {}", handle.command_id),
            )
        })?;
        if record.handle.config_snapshot_id != handle.config_snapshot_id {
            return Err(command_error(
                ErrorCode::CommandNotFound,
                "command snapshot identity does not match the handle",
            ));
        }
        Ok(CommandDiagnostics {
            handle: record.handle.clone(),
            status: record.status,
            pending_effect_id: record.pending_effect_id.clone(),
            last_error: record.last_error.clone(),
        })
    }

    /// Exports one versioned binary command checkpoint bound to its exact snapshot and contracts.
    /// 导出一个绑定精确快照与契约的带版本二进制命令检查点。
    ///
    /// # Errors
    /// Returns an error when the handle is unknown or mismatched, or when the command is
    /// terminal, cancelled, or failed and therefore cannot resume.
    /// 当句柄未知或不匹配，或命令已终态、已取消、已失败而无法恢复时返回错误。
    #[allow(clippy::redundant_clone)]
    pub fn export_checkpoint(&self, handle: &CommandHandle) -> Result<Vec<u8>, CoreError> {
        let record = self.records.get(&handle.command_id).ok_or_else(|| {
            command_error(
                ErrorCode::CommandNotFound,
                format!("command not found: {}", handle.command_id),
            )
        })?;
        if record.handle.config_snapshot_id != handle.config_snapshot_id {
            return Err(command_error(
                ErrorCode::CommandNotFound,
                "command snapshot identity does not match the handle",
            ));
        }
        // Terminal, cancelled, and failed commands cannot resume and must refuse export.
        // 已终态、已取消与已失败命令无法恢复，必须拒绝导出。
        match record.status {
            CommandStatus::Completed => {
                return Err(command_error(
                    ErrorCode::CommandCompleted,
                    "completed commands cannot be checkpointed",
                ));
            }
            CommandStatus::Failed => {
                return Err(command_error(
                    ErrorCode::CommandCompleted,
                    "failed commands cannot be checkpointed",
                ));
            }
            CommandStatus::Cancelled => {
                return Err(command_error(
                    ErrorCode::CommandCancelled,
                    "cancelled commands cannot be checkpointed",
                ));
            }
            CommandStatus::Waiting | CommandStatus::ResultUncertain => {}
        }
        if record.status == CommandStatus::Waiting && record.pending_effect.is_none() {
            return Err(command_error(
                ErrorCode::CheckpointIncompatible,
                "waiting command has no resumable pending effect",
            ));
        }
        if record.status == CommandStatus::ResultUncertain
            && record.pending_mutation_idempotency_key.is_none()
        {
            return Err(command_error(
                ErrorCode::CheckpointIncompatible,
                "result-uncertain command has no durable reconciliation key",
            ));
        }
        let state = checkpoint_state(record);
        let command_input = record.checkpoint_command_input.clone().unwrap_or_default();
        let command = vulcan_memorymesh_core_contracts::decode_command(&command_input)?;
        let contracts = checkpoint_contracts(&command, &record.snapshot);
        // Serialize the full private flow state and the exact pending effect request so a
        // new process can resume mid-flow instead of restarting from the first effect.
        // 序列化完整私有流程状态与精确待处理 Effect 请求，使新进程可在中间流程
        // 恢复而不是从首个 Effect 重启。
        let flow_state = encode_checkpoint_continuation(&record.state, "flow state")?;
        let pending_effect = record
            .pending_effect
            .as_ref()
            .map(|request| encode_checkpoint_continuation(request, "pending effect"))
            .transpose()?;
        let continuation = Some(vulcan_memorymesh_core_contracts::CheckpointContinuation {
            flow_state,
            pending_effect,
            next_effect_sequence: record.next_effect_sequence,
            pending_mutation_idempotency_key: record.pending_mutation_idempotency_key.clone(),
            degraded: record.degraded,
        });
        let mut checkpoint = CommandCheckpoint {
            format_version: vulcan_memorymesh_core_contracts::CHECKPOINT_FORMAT_VERSION,
            handle: record.handle.clone(),
            command_input,
            mutation_identity: record.mutation_identity.clone(),
            state,
            contracts,
            received_summary: record.received_effects.clone(),
            continuation,
            payload_digest: String::new(),
        };
        checkpoint.payload_digest =
            vulcan_memorymesh_core_contracts::checkpoint_payload_digest(&checkpoint)?;
        vulcan_memorymesh_core_contracts::encode_checkpoint(&checkpoint)
    }

    /// Restores one waiting command from a versioned checkpoint against the exact snapshot identity.
    /// 针对精确快照身份从带版本检查点恢复一个等待命令。
    ///
    /// # Errors
    /// Returns a stable checkpoint error when the format is unsupported, integrity fails, or the
    /// bound snapshot is unavailable.
    /// 当格式不支持、完整性失败或绑定快照不可用时返回稳定检查点错误。
    pub fn restore_checkpoint(
        &mut self,
        bytes: &[u8],
        available_snapshots: &BTreeMap<String, Arc<ConfigSnapshot>>,
    ) -> Result<(CommandHandle, CommandStep), CoreError> {
        let checkpoint = vulcan_memorymesh_core_contracts::decode_checkpoint(bytes)?;
        verify_checkpoint_digest(&checkpoint)?;
        if self
            .issued_command_ids
            .contains(&checkpoint.handle.command_id)
        {
            return Err(command_error(
                ErrorCode::CheckpointIncompatible,
                "checkpoint command identity was already issued in this Core instance",
            ));
        }
        let command = vulcan_memorymesh_core_contracts::decode_command(&checkpoint.command_input)?;
        if command.mutation_identity() != checkpoint.mutation_identity.as_ref() {
            return Err(command_error(
                ErrorCode::CheckpointIncompatible,
                "checkpoint mutation identity does not match its command input",
            ));
        }
        let snapshot = available_snapshots
            .values()
            .find(|candidate| candidate.id == checkpoint.handle.config_snapshot_id)
            .cloned()
            .ok_or_else(|| {
                CoreError::new(
                    ErrorCode::CheckpointSnapshotUnavailable,
                    ErrorStage::Command,
                    "the exact checkpoint-bound configuration snapshot is unavailable",
                )
            })?;
        validate_command(&command, &snapshot)?;
        if verify_identity(&command, &snapshot)? != checkpoint.mutation_identity {
            return Err(command_error(
                ErrorCode::CheckpointIncompatible,
                "checkpoint command identity or canonical fingerprint is invalid",
            ));
        }
        let Some(continuation) = &checkpoint.continuation else {
            return Err(command_error(
                ErrorCode::CheckpointIncompatible,
                "resumable checkpoint is missing its exact continuation",
            ));
        };
        // Deserialize the full private flow state captured at export time.
        // 反序列化导出时捕获的完整私有流程状态。
        let state: FlowState =
            decode_checkpoint_continuation(&continuation.flow_state, "flow state")?;
        let pending_effect = match &continuation.pending_effect {
            Some(payload) => Some(decode_checkpoint_continuation(payload, "pending effect")?),
            None => None,
        };
        validate_checkpoint_continuation(&checkpoint, &state, pending_effect.as_ref(), &command)?;
        validate_pipeline_checkpoint(&state, pending_effect.as_ref(), &command, &snapshot)?;
        let expected_contracts = checkpoint_contracts(&command, &snapshot);
        if checkpoint.contracts != expected_contracts {
            return Err(command_error(
                ErrorCode::CheckpointIncompatible,
                "checkpoint contract set does not match its exact snapshot and command",
            ));
        }
        let handle = checkpoint.handle.clone();
        // Uncertainty diagnostics are derivable authority, not optional history lost during restart.
        // 不确定诊断是可派生的权威事实，不是重启后可以丢弃的可选历史。
        let (status, last_error) = match &checkpoint.state {
            vulcan_memorymesh_core_contracts::CheckpointState::AwaitingReconciliation {
                idempotency_key,
            } => (
                CommandStatus::ResultUncertain,
                Some(uncertain_storage_error(&handle, idempotency_key.clone())),
            ),
            _ => (CommandStatus::Waiting, None),
        };
        let record = CommandRecord {
            handle: handle.clone(),
            snapshot,
            status,
            state,
            pending_effect_id: (status == CommandStatus::Waiting)
                .then(|| {
                    pending_effect
                        .as_ref()
                        .map(|request: &EffectRequest| request.effect_id.clone())
                })
                .flatten(),
            pending_effect,
            pending_mutation_idempotency_key: continuation.pending_mutation_idempotency_key.clone(),
            next_effect_sequence: continuation.next_effect_sequence,
            last_error,
            mutation_identity: checkpoint.mutation_identity.clone(),
            received_effects: checkpoint.received_summary.clone(),
            checkpoint_command_input: Some(checkpoint.command_input.clone()),
            degraded: continuation.degraded,
        };
        // An uncertain command retains its mutation key and must be reconciled by the host
        // together with the original batch used to validate the eventual receipt. It must
        // never replay that retained effect.
        // 结果不确定命令保留其变更键及用于校验最终收据的原始批次，绝不能重放该 Effect。
        if status == CommandStatus::ResultUncertain {
            self.issued_command_ids.insert(handle.command_id.clone());
            self.records.insert(handle.command_id.clone(), record);
            return Ok((handle, CommandStep::Effects(Vec::new())));
        }
        // Re-issue the exact pending effect with its original identity so the host result
        // continues the flow from the precise mid-flight position.
        // 以原始身份重新发出精确待处理 Effect，使宿主结果从精确飞行中间位置继续流程。
        if let Some(request) = record.pending_effect.clone() {
            self.issued_command_ids.insert(handle.command_id.clone());
            self.records.insert(handle.command_id.clone(), record);
            return Ok((handle, CommandStep::Effects(vec![request])));
        }
        Err(command_error(
            ErrorCode::CheckpointIncompatible,
            "checkpoint continuation has neither a pending effect nor a reconciliation key",
        ))
    }
}

/// Initializes one validated command into its first typed transition.
/// 将一个已校验命令初始化为首个强类型转换。
fn initialize(record: &mut CommandRecord, command: CoreCommand) -> Result<CommandStep, CoreError> {
    match command {
        CoreCommand::PreCheck(command) => initialize_precheck(record, command),
        CoreCommand::PostActionIntake(command) => initialize_intake(record, command),
        CoreCommand::PostActionAnalyzePendingTurn(command) => initialize_analyze(record, command),
        CoreCommand::ChatCompact(command) => initialize_chat_compact(record, command),
        CoreCommand::MemorySearch(command) => initialize_memory_search(record, command),
        CoreCommand::MemoryDetails(command) => initialize_memory_details(record, command),
        CoreCommand::MemoryWrite(command) => initialize_memory_write(record, command),
        CoreCommand::MemoryDelete(command) => initialize_memory_delete(record, command),
        CoreCommand::ProfileQuery(command) => initialize_profile_query(record, command),
        CoreCommand::ProfileInstruction(command) => initialize_profile_instruction(record, command),
        CoreCommand::Retention(command) => initialize_retention(record, command),
        CoreCommand::MemorySearchBatch(command) => initialize_memory_search_batch(record, command),
        CoreCommand::MemoryDetailsBatch(command) => {
            initialize_memory_details_batch(record, command)
        }
        CoreCommand::MemoryWriteBatch(command) => initialize_memory_write_batch(record, command),
        CoreCommand::MemoryDeleteBatch(command) => initialize_memory_delete_batch(record, command),
        CoreCommand::Maintenance(command) => initialize_maintenance(record, command),
    }
}

/// Advances exactly one private flow state with one typed host result.
/// 使用一个强类型宿主结果精确推进一个私有流程状态。
#[allow(clippy::too_many_lines)]
fn advance(record: &mut CommandRecord, result: HostEffectResult) -> Result<CommandStep, CoreError> {
    validate_host_result_envelope(&result)?;
    // An uncertain mutation must retain the pending flow state so reconciliation can replay it.
    // 结果不确定的变更必须保留待处理流程状态，以便对账时重放。
    if matches!(
        &result,
        HostEffectResult::StorageMutation(MutationOutcome::Uncertain)
    ) {
        let idempotency_key = record
            .pending_mutation_idempotency_key
            .clone()
            .ok_or_else(|| {
                command_error(
                    ErrorCode::Internal,
                    "pending mutation idempotency key is missing",
                )
            })?;
        return mark_uncertain(record, idempotency_key);
    }
    if matches!(&result, HostEffectResult::Failure(_)) {
        return degrade(record, &result);
    }
    let state = std::mem::replace(&mut record.state, FlowState::Terminal);
    match state {
        FlowState::Terminal => Err(command_error(
            ErrorCode::CommandCompleted,
            "terminal command cannot be resumed",
        )),
        FlowState::PreCheckIntent { command, content } => {
            advance_precheck_intent(record, command, content, result)
        }
        FlowState::PreCheckEmbedding {
            command,
            content,
            queries,
            intent_reason,
        } => advance_precheck_embedding(record, command, content, queries, intent_reason, result),
        FlowState::SearchPipeline(pipeline) => advance_search_pipeline(record, *pipeline, result),
        FlowState::PreCheckReview {
            command,
            queries,
            candidates,
        } => advance_precheck_review(record, command, queries, candidates, result),
        FlowState::PreCheckReinforce(pending) => {
            let accepted_ids = pending
                .accepted
                .iter()
                .map(|hit| hit.source.memory.id.clone())
                .collect();
            let degraded = record.degraded;
            advance_reconcilable_with_receipt(record, result, move |receipt| {
                let (context_items, context_text) =
                    crate::precheck::assemble_context(&pending.accepted);
                Ok(CommandResult::PreCheck(PreCheckResult {
                    should_inject: !context_items.is_empty(),
                    context_text,
                    context_items,
                    candidates: review_candidate_views(&pending.candidates),
                    degraded,
                    queries: pending.queries.clone(),
                    reinforcement: Some(ReinforcementResult {
                        reinforced_memory_ids: accepted_ids,
                        failed: false,
                        receipt: Some(receipt),
                    }),
                }))
            })
        }
        FlowState::IntakeNoiseQuery(context) => {
            advance_intake_noise_query(record, *context, result)
        }
        FlowState::IntakeNoiseEmbed(context) => {
            advance_intake_noise_embed(record, *context, result)
        }
        FlowState::IntakeNoiseStore {
            context,
            prototypes,
        } => advance_intake_noise_store(record, *context, prototypes, result),
        FlowState::IntakeNoiseScore {
            context,
            prototypes,
        } => advance_intake_noise_score(record, *context, prototypes, result),
        FlowState::IntakeTurn { turn_slot, noise } => {
            advance_reconcilable_with_receipt(record, result, |receipt| {
                let turn_id = receipt
                    .entity_bindings
                    .iter()
                    .find(|binding| binding.entity_slot_id == turn_slot)
                    .map(|binding| TurnId::new(binding.domain_id.clone()))
                    .transpose()
                    .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?
                    .ok_or_else(|| {
                        command_error(
                            ErrorCode::Internal,
                            "pending turn slot binding is missing from the commit receipt",
                        )
                    })?;
                Ok(CommandResult::PostActionAccepted(
                    PostActionAcceptedResult {
                        noise,
                        turn_id: Some(turn_id),
                        ignored_as_noise: false,
                        receipt: Some(receipt),
                    },
                ))
            })
        }
        FlowState::AnalyzeAttempt { command } => advance_analyze_attempt(record, command, result),
        FlowState::AnalyzeLoadTurn { command } => {
            advance_analyze_load_turn(record, command, result)
        }
        FlowState::AnalyzeAnalysis { command, turn } => {
            advance_analyze_analysis(record, command, turn, result)
        }
        FlowState::AnalyzeEmbedding(pending) => advance_analyze_embedding(record, *pending, result),
        FlowState::AnalyzeProfileQuery(pending) => {
            advance_analyze_profile_query(record, *pending, result)
        }
        FlowState::AnalyzeReview(pending) => advance_analyze_review(record, *pending, result),
        FlowState::AnalyzePersistenceEmbedding(pending) => {
            advance_analyze_persistence_embedding(record, *pending, result)
        }
        FlowState::AnalyzeDerived(pending) => advance_analyze_derived(record, *pending, result),
        FlowState::MemoryDetails(command) => match expect_storage_query(result)? {
            StorageQueryResult::MemoryDetails(record_result) => {
                validate_memory_details_result(&command, record_result.as_ref().as_ref())?;
                complete(record, CommandResult::MemoryDetails(*record_result))
            }
            _ => Err(effect_type_error("MemoryDetails storage result")),
        },
        FlowState::MemoryDetailsBatch(command) => match expect_storage_query(result)? {
            StorageQueryResult::DetailsBatch { memories, turns } => {
                validate_memory_details_batch_result(&command, &memories, &turns)?;
                complete(
                    record,
                    CommandResult::MemoryDetailsBatch(MemoryDetailsBatchResult { memories, turns }),
                )
            }
            _ => Err(effect_type_error("MemoryDetailsBatch storage result")),
        },
        FlowState::ChatCompactMutation => {
            if matches!(
                &result,
                HostEffectResult::StorageMutation(MutationOutcome::Uncertain)
            ) {
                record.state = FlowState::ChatCompactMutation;
            }
            advance_chat_compact(record, result)
        }
        FlowState::MemoryWriteDedupe(pending) => {
            advance_memory_write_dedupe(record, pending, result)
        }
        FlowState::MemoryWriteEmbedding(pending) => {
            advance_memory_write_embedding(record, pending, result)
        }
        FlowState::MemoryWriteMutation(pending) => {
            advance_reconcilable_with_receipt(record, result, |receipt| {
                validate_direct_memory_receipt(&receipt, &BTreeSet::from([pending.slot.clone()]))?;
                let memory =
                    bind_memory_from_receipt(pending.memory.clone(), &pending.slot, &receipt)?;
                Ok(CommandResult::MemoryWrite(MemoryWriteResult {
                    memory,
                    deduplicated: false,
                    receipt: Some(receipt),
                }))
            })
        }
        FlowState::MemoryWriteBatchDedupe(pending) => {
            advance_memory_write_batch_dedupe(record, pending, result)
        }
        FlowState::MemoryWriteBatchEmbedding(pending) => {
            advance_memory_write_batch_embedding(record, pending, result)
        }
        FlowState::MemoryWriteBatchMutation(pending) => {
            advance_reconcilable_with_receipt(record, result, |receipt| {
                let memories = resolve_batch_memories(&pending.entries, Some(&receipt))?;
                Ok(CommandResult::MemoryWriteBatch(MemoryWriteBatchResult {
                    memories,
                    deduplicated: pending.deduplicated.clone(),
                    receipt: Some(receipt),
                }))
            })
        }
        FlowState::MemoryWriteReview(pending) => {
            advance_direct_write_review(record, *pending, result)
        }
        FlowState::MemoryWritePersistenceEmbedding {
            pending,
            supersede_by_item,
        } => advance_direct_persistence_embedding(record, *pending, &supersede_by_item, result),
        FlowState::MemoryDeletePreflight(command) => {
            advance_memory_delete_preflight(record, command, result)
        }
        FlowState::MemoryDeleteMutation => {
            advance_reconcilable_with_receipt(record, result, |receipt| {
                Ok(CommandResult::MemoryDelete(MemoryDeleteResult {
                    deleted: true,
                    receipt,
                }))
            })
        }
        FlowState::MemoryDeleteBatchPreflight(command) => {
            advance_memory_delete_batch_preflight(record, command, result)
        }
        FlowState::MemoryDeleteBatchMutation(outcome) => {
            advance_reconcilable_with_receipt(record, result, |receipt| {
                let mut outcome = outcome.clone();
                outcome.receipt = Some(receipt);
                Ok(CommandResult::MemoryDeleteBatch(outcome))
            })
        }
        FlowState::ProfileQuery(command) => advance_profile_query(record, command, result),
        FlowState::ProfileInstructionQuery(command) => {
            advance_profile_instruction_query(record, command, result)
        }
        FlowState::ProfileInstructionLlm {
            command,
            current_nodes,
        } => advance_profile_instruction_llm(record, command, current_nodes, result),
        FlowState::ProfileInstructionMutation(pending) => {
            advance_reconcilable_with_receipt(record, result, |receipt| {
                let expected_bindings = pending
                    .accepted_slots
                    .iter()
                    .cloned()
                    .map(|slot| (slot, EntityKind::ProfileNode))
                    .collect::<BTreeMap<_, _>>();
                validate_expected_bindings(&receipt, &expected_bindings)?;
                let mut accepted_nodes = pending.outcome.accepted_nodes.clone();
                if accepted_nodes.len() != pending.accepted_slots.len() {
                    return Err(command_error(
                        ErrorCode::Internal,
                        "profile instruction slot count does not match accepted node count",
                    ));
                }
                for (node, slot) in accepted_nodes.iter_mut().zip(&pending.accepted_slots) {
                    let binding = receipt
                        .entity_bindings
                        .iter()
                        .find(|binding| binding.entity_slot_id == *slot)
                        .ok_or_else(|| {
                            command_error(
                                ErrorCode::Internal,
                                format!(
                                    "profile instruction commit receipt is missing slot {}",
                                    slot.as_str()
                                ),
                            )
                        })?;
                    if binding.entity_kind != EntityKind::ProfileNode {
                        return Err(command_error(
                            ErrorCode::Internal,
                            "profile instruction slot was bound to the wrong entity kind",
                        ));
                    }
                    node.id = ProfileNodeId::new(binding.domain_id.clone())
                        .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
                    node.version = receipt
                        .committed_versions
                        .iter()
                        .find(|version| {
                            version.entity_kind == EntityKind::ProfileNode
                                && version.domain_id == binding.domain_id
                        })
                        .map(|version| version.version)
                        .ok_or_else(|| {
                            command_error(
                                ErrorCode::Internal,
                                "profile instruction commit receipt is missing the created version",
                            )
                        })?;
                }
                Ok(CommandResult::ProfileInstruction(
                    ProfileInstructionResult {
                        accepted_nodes,
                        retired_nodes: pending.outcome.retired_nodes.clone(),
                        review_reason: pending.outcome.review_reason.clone(),
                        receipt: Some(receipt),
                    },
                ))
            })
        }
        FlowState::RetentionQuery(command) => advance_retention_query(record, command, result),
        FlowState::MaintenanceRebuildQuery(command) => {
            advance_maintenance_rebuild_query(record, command, result)
        }
        FlowState::MaintenanceLifecyclePreflight(command) => {
            advance_maintenance_lifecycle_preflight(record, &command, result)
        }
        FlowState::MaintenanceRebuildEmbedding { command, memories } => {
            advance_maintenance_rebuild_embedding(record, command, memories, result)
        }
        FlowState::MaintenanceMutation(outcome) => {
            advance_reconcilable_with_receipt(record, result, |receipt| {
                Ok(CommandResult::Maintenance(MaintenanceResult {
                    affected_memory_ids: outcome.affected_memory_ids.clone(),
                    rebuilt_vector_count: outcome.rebuilt_vector_count,
                    receipt,
                }))
            })
        }
    }
}

/// Applies one mutation outcome with a receipt-consuming success continuation.
/// 应用一个变更结果并使用消费收据的成功续体。
fn advance_reconcilable_with_receipt(
    record: &mut CommandRecord,
    result: HostEffectResult,
    success: impl FnOnce(CommitReceipt) -> Result<CommandResult, CoreError>,
) -> Result<CommandStep, CoreError> {
    if matches!(
        &result,
        HostEffectResult::StorageMutation(MutationOutcome::Uncertain)
    ) {
        retain_uncertain_state(record)?;
    }
    match expect_mutation(result)? {
        MutationOutcome::Committed { receipt, .. } => {
            validate_commit_receipt_identity(record, &receipt)?;
            complete(record, success(receipt)?)
        }
        MutationOutcome::Rejected { reason } => Err(CoreError::new(
            ErrorCode::StorageConflict,
            ErrorStage::Storage,
            reason,
        )),
        MutationOutcome::Uncertain => {
            let idempotency_key =
                record
                    .pending_mutation_idempotency_key
                    .clone()
                    .ok_or_else(|| {
                        command_error(
                            ErrorCode::Internal,
                            "pending mutation idempotency key is missing",
                        )
                    })?;
            mark_uncertain(record, idempotency_key)
        }
    }
}

/// Verifies that a committed receipt belongs to the exact pending derived mutation identity.
/// 校验一份已提交收据属于精确的待处理派生变更身份。
fn validate_commit_receipt_identity(
    record: &CommandRecord,
    receipt: &CommitReceipt,
) -> Result<(), CoreError> {
    // The exact retained batch is authority; opaque identities must never be parsed for suffixes.
    // 精确保留的批次是收据权威；不得通过解析不透明身份后缀来重建预期值。
    let Some(HostEffect::StorageMutation(batch)) = record
        .pending_effect
        .as_ref()
        .map(|request| &request.effect)
    else {
        return Err(command_error(
            ErrorCode::Internal,
            "receipt-bearing flow lost its pending mutation batch",
        ));
    };
    if record.pending_mutation_idempotency_key.as_ref() != Some(&batch.identity.idempotency_key) {
        return Err(command_error(
            ErrorCode::Internal,
            "pending batch reconciliation identity drifted",
        ));
    }
    if receipt.operation_id != batch.identity.operation_id
        || receipt.idempotency_key != batch.identity.idempotency_key
        || receipt.request_fingerprint != batch.identity.request_fingerprint
    {
        return Err(command_error(
            ErrorCode::StorageConflict,
            "commit receipt identity does not match the pending mutation batch",
        ));
    }
    validate_commit_receipt_shape(receipt)?;
    validate_commit_receipt_against_pending_mutation(record, receipt)?;
    Ok(())
}

/// Validates the host-independent structural invariants of one committed receipt.
/// 校验一份已提交收据中与宿主实现无关的结构不变量。
fn validate_commit_receipt_shape(receipt: &CommitReceipt) -> Result<(), CoreError> {
    let mut slots = BTreeSet::new();
    let mut bindings = BTreeSet::new();
    for binding in &receipt.entity_bindings {
        let identity = (binding.entity_kind.as_u32(), binding.domain_id.clone());
        if binding.domain_id.is_empty()
            || !slots.insert(binding.entity_slot_id.clone())
            || !bindings.insert(identity)
        {
            return Err(command_error(
                ErrorCode::StorageConflict,
                "commit receipt contains an empty or duplicate entity binding",
            ));
        }
    }
    let mut versions = BTreeSet::new();
    for versioned in &receipt.committed_versions {
        let identity = (versioned.entity_kind.as_u32(), versioned.domain_id.clone());
        if versioned.domain_id.is_empty() || versioned.version == 0 || !versions.insert(identity) {
            return Err(command_error(
                ErrorCode::StorageConflict,
                "commit receipt contains an empty, zero, or duplicate final entity version",
            ));
        }
    }
    if !bindings.is_subset(&versions) {
        return Err(command_error(
            ErrorCode::StorageConflict,
            "commit receipt is missing a final version for one bound entity",
        ));
    }
    if receipt.side_effect_summary.len() > 8_192 || receipt.side_effect_summary.contains('\0') {
        return Err(command_error(
            ErrorCode::StorageConflict,
            "commit receipt side-effect summary violates its bounded text contract",
        ));
    }
    Ok(())
}

/// Validates that a commit receipt reports exactly the entities mutated by the pending batch.
/// 校验提交收据精确报告待处理批次实际变更的实体。
#[allow(clippy::too_many_lines)]
fn validate_commit_receipt_against_pending_mutation(
    record: &CommandRecord,
    receipt: &CommitReceipt,
) -> Result<(), CoreError> {
    let Some(HostEffect::StorageMutation(batch)) = record
        .pending_effect
        .as_ref()
        .map(|request| &request.effect)
    else {
        return Err(command_error(
            ErrorCode::Internal,
            "commit receipt has no retained pending mutation batch",
        ));
    };

    // Freeze the exact slot set from creation mutations. References to those slots in later
    // mutations do not create additional bindings.
    // 从创建变更中冻结精确槽位集合；后续变更对这些槽位的引用不会创建额外绑定。
    let mut expected_bindings = BTreeMap::new();
    for mutation in &batch.mutations {
        let created = match mutation {
            Mutation::PutTurn {
                entity: EntityRef::New(slot),
                ..
            } => Some((slot, EntityKind::Turn)),
            Mutation::PutMemory {
                entity: EntityRef::New(slot),
                ..
            } => Some((slot, EntityKind::Memory)),
            Mutation::PutProfileNode {
                entity: EntityRef::New(slot),
                ..
            } => Some((slot, EntityKind::ProfileNode)),
            _ => None,
        };
        if let Some((slot, kind)) = created
            && expected_bindings.insert(slot.clone(), kind).is_some()
        {
            return Err(command_error(
                ErrorCode::Internal,
                "pending mutation batch creates one entity slot more than once",
            ));
        }
    }
    validate_expected_bindings(receipt, &expected_bindings)?;
    let receipt_bindings = receipt
        .entity_bindings
        .iter()
        .map(|binding| (binding.entity_slot_id.clone(), binding))
        .collect::<BTreeMap<_, _>>();

    // Collect the final version identity for every versioned mutation target. Physical
    // deletions and metadata-only mutations intentionally produce no entity version.
    // 收集每个带版本变更目标的最终版本身份；物理删除与仅元数据变更有意不产生实体版本。
    let mut expected_versions = BTreeSet::new();
    for mutation in &batch.mutations {
        match mutation {
            Mutation::PutTurn { entity, .. } => {
                insert_entity_version(
                    &mut expected_versions,
                    EntityKind::Turn,
                    resolve_receipt_entity(entity, EntityKind::Turn, &receipt_bindings)?,
                );
            }
            Mutation::RestoreImportedMemory(memory) => {
                insert_entity_version(
                    &mut expected_versions,
                    EntityKind::Memory,
                    memory.id.as_str().to_owned(),
                );
            }
            Mutation::RestoreImportedProfileNode(node) => {
                insert_entity_version(
                    &mut expected_versions,
                    EntityKind::ProfileNode,
                    node.id.as_str().to_owned(),
                );
            }
            Mutation::RestoreImportedContextEvidence(evidence) => {
                insert_entity_version(
                    &mut expected_versions,
                    EntityKind::ContextEvidence,
                    vulcan_memorymesh_core_contracts::context_evidence_domain_id(
                        &evidence.memory_id,
                        &evidence.context_key,
                        &evidence.context_value,
                    ),
                );
            }
            Mutation::PutMemory { entity, .. } => {
                insert_entity_version(
                    &mut expected_versions,
                    EntityKind::Memory,
                    resolve_receipt_entity(entity, EntityKind::Memory, &receipt_bindings)?,
                );
            }
            Mutation::ReinforceMemory { memory_id, .. }
            | Mutation::SupersedeMemory { memory_id, .. }
            | Mutation::RecycleMemory { memory_id, .. }
            | Mutation::RestoreMemory { memory_id, .. } => {
                insert_entity_version(
                    &mut expected_versions,
                    EntityKind::Memory,
                    memory_id.as_str().to_owned(),
                );
            }
            Mutation::AggregateContextEvidence {
                memory,
                context_key,
                context_value,
                ..
            } => {
                let memory_id = MemoryId::new(resolve_receipt_entity(
                    memory,
                    EntityKind::Memory,
                    &receipt_bindings,
                )?)
                .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
                insert_entity_version(
                    &mut expected_versions,
                    EntityKind::ContextEvidence,
                    vulcan_memorymesh_core_contracts::context_evidence_domain_id(
                        &memory_id,
                        context_key,
                        context_value,
                    ),
                );
            }
            Mutation::PutProfileNode { entity, .. } => {
                insert_entity_version(
                    &mut expected_versions,
                    EntityKind::ProfileNode,
                    resolve_receipt_entity(entity, EntityKind::ProfileNode, &receipt_bindings)?,
                );
            }
            Mutation::RetireProfileNode {
                profile_node_id, ..
            } => {
                insert_entity_version(
                    &mut expected_versions,
                    EntityKind::ProfileNode,
                    profile_node_id.as_str().to_owned(),
                );
            }
            Mutation::TransitionTurnStatus { .. }
            | Mutation::AdvanceChatCompactBoundary { .. }
            | Mutation::PutChatCompactBoundary(_)
            | Mutation::DeleteMemory { .. }
            | Mutation::PutAnalysisAttempt(_)
            | Mutation::PutNoisePrototypeBundle { .. }
            | Mutation::PurgeMemory { .. } => {}
        }
    }
    let actual_versions = receipt
        .committed_versions
        .iter()
        .map(|version| (version.entity_kind.as_u32(), version.domain_id.clone()))
        .collect::<BTreeSet<_>>();
    if actual_versions != expected_versions {
        return Err(command_error(
            ErrorCode::StorageConflict,
            "commit receipt version set does not match the pending mutation batch",
        ));
    }
    Ok(())
}

/// Resolves one existing or same-batch entity reference to its committed domain identity.
/// 将一个既有或同批次实体引用解析为其已提交领域身份。
fn resolve_receipt_entity(
    entity: &EntityRef,
    expected_kind: EntityKind,
    bindings: &BTreeMap<EntitySlotId, &vulcan_memorymesh_core_contracts::EntityBinding>,
) -> Result<String, CoreError> {
    match entity {
        EntityRef::Existing { domain_id, .. } if !domain_id.is_empty() => Ok(domain_id.clone()),
        EntityRef::New(slot) => bindings
            .get(slot)
            .filter(|binding| binding.entity_kind == expected_kind)
            .map(|binding| binding.domain_id.clone())
            .ok_or_else(|| {
                command_error(
                    ErrorCode::StorageConflict,
                    "commit receipt cannot resolve a pending mutation entity slot",
                )
            }),
        EntityRef::Existing { .. } => Err(command_error(
            ErrorCode::Internal,
            "pending mutation contains an empty existing entity identity",
        )),
    }
}

/// Inserts one exact versioned entity identity into the expected receipt set.
/// 将一个精确带版本实体身份插入预期收据集合。
fn insert_entity_version(
    versions: &mut BTreeSet<(u32, String)>,
    kind: EntityKind,
    domain_id: String,
) {
    versions.insert((kind.as_u32(), domain_id));
}

/// Validates the exact slot kinds expected from one derived mutation batch.
/// 校验一个派生变更批次所期望的精确槽位类别。
fn validate_expected_bindings(
    receipt: &CommitReceipt,
    expected: &BTreeMap<EntitySlotId, EntityKind>,
) -> Result<(), CoreError> {
    if receipt.entity_bindings.len() != expected.len() {
        return Err(command_error(
            ErrorCode::StorageConflict,
            "commit receipt entity binding count does not match the derived mutation batch",
        ));
    }
    for binding in &receipt.entity_bindings {
        if expected.get(&binding.entity_slot_id) != Some(&binding.entity_kind) {
            return Err(command_error(
                ErrorCode::StorageConflict,
                "commit receipt entity binding does not match the derived mutation batch",
            ));
        }
    }
    Ok(())
}

/// Validates one direct-write receipt against the exact set of created memory slots.
/// 根据创建记忆槽位的精确集合校验一份主动写入提交收据。
fn validate_direct_memory_receipt(
    receipt: &CommitReceipt,
    expected_slots: &BTreeSet<EntitySlotId>,
) -> Result<(), CoreError> {
    if receipt.entity_bindings.len() != expected_slots.len()
        || receipt.committed_versions.len() < expected_slots.len()
    {
        return Err(command_error(
            ErrorCode::Internal,
            "direct memory commit receipt has an unexpected binding or version count",
        ));
    }
    let mut bound_slots = BTreeSet::new();
    let mut bound_ids = BTreeSet::new();
    for binding in &receipt.entity_bindings {
        if binding.entity_kind != EntityKind::Memory
            || !expected_slots.contains(&binding.entity_slot_id)
            || !bound_slots.insert(binding.entity_slot_id.clone())
            || !bound_ids.insert(binding.domain_id.clone())
        {
            return Err(command_error(
                ErrorCode::Internal,
                "direct memory commit receipt contains an invalid or duplicate binding",
            ));
        }
    }
    let mut versioned_ids = BTreeSet::new();
    for version in &receipt.committed_versions {
        if version.entity_kind != EntityKind::Memory
            || version.version == 0
            || !versioned_ids.insert(version.domain_id.clone())
        {
            return Err(command_error(
                ErrorCode::Internal,
                "direct memory commit receipt contains an invalid or duplicate version",
            ));
        }
    }
    if bound_slots != *expected_slots || !bound_ids.is_subset(&versioned_ids) {
        return Err(command_error(
            ErrorCode::Internal,
            "direct memory commit receipt is missing a required slot or version",
        ));
    }
    Ok(())
}

/// Replaces one placeholder memory identity and version from an already validated receipt.
/// 从一份已校验提交收据中替换一条占位记忆的身份与版本。
fn bind_memory_from_receipt(
    mut memory: MemoryRecord,
    slot: &EntitySlotId,
    receipt: &CommitReceipt,
) -> Result<MemoryRecord, CoreError> {
    let binding = receipt
        .entity_bindings
        .iter()
        .find(|binding| binding.entity_slot_id == *slot)
        .ok_or_else(|| {
            command_error(
                ErrorCode::Internal,
                format!("direct memory commit receipt is missing slot {slot}"),
            )
        })?;
    memory.id = MemoryId::new(binding.domain_id.clone())
        .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
    memory.version = receipt
        .committed_versions
        .iter()
        .find(|version| {
            version.entity_kind == EntityKind::Memory && version.domain_id == binding.domain_id
        })
        .map(|version| version.version)
        .ok_or_else(|| {
            command_error(
                ErrorCode::Internal,
                "direct memory commit receipt is missing the created version",
            )
        })?;
    Ok(memory)
}

/// Resolves every caller-ordered batch row to the exact persisted memory record.
/// 将每个按调用方顺序排列的批量行解析为精确持久记忆记录。
fn resolve_batch_memories(
    entries: &[PendingBatchMemory],
    receipt: Option<&CommitReceipt>,
) -> Result<Vec<MemoryRecord>, CoreError> {
    let expected_slots = entries
        .iter()
        .filter_map(|entry| match entry {
            PendingBatchMemory::New { slot, .. } => Some(slot.clone()),
            PendingBatchMemory::Existing(_) | PendingBatchMemory::Earlier(_) => None,
        })
        .collect::<BTreeSet<_>>();
    if expected_slots.is_empty() {
        if receipt.is_some() {
            return Err(command_error(
                ErrorCode::Internal,
                "no-op direct memory batch unexpectedly returned a commit receipt",
            ));
        }
    } else {
        let receipt = receipt.ok_or_else(|| {
            command_error(
                ErrorCode::Internal,
                "direct memory batch is missing its commit receipt",
            )
        })?;
        validate_direct_memory_receipt(receipt, &expected_slots)?;
    }
    let mut resolved = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let memory = match entry {
            PendingBatchMemory::New { slot, memory } => bind_memory_from_receipt(
                memory.clone(),
                slot,
                receipt.ok_or_else(|| {
                    command_error(
                        ErrorCode::Internal,
                        "new direct memory is missing its receipt",
                    )
                })?,
            )?,
            PendingBatchMemory::Existing(memory) => {
                // A reused row can also be superseded by another item in this batch.
                // 复用行也可能被本批次的另一项替代。
                let mut memory = memory.clone();
                if let Some(version) = receipt.and_then(|receipt| {
                    receipt.committed_versions.iter().find(|version| {
                        version.entity_kind == EntityKind::Memory
                            && version.domain_id == memory.id.as_str()
                    })
                }) {
                    memory.version = version.version;
                }
                memory
            }
            PendingBatchMemory::Earlier(source_index) => {
                if *source_index >= index {
                    return Err(command_error(
                        ErrorCode::Internal,
                        "direct memory batch alias does not reference an earlier item",
                    ));
                }
                resolved.get(*source_index).cloned().ok_or_else(|| {
                    command_error(
                        ErrorCode::Internal,
                        "direct memory batch alias source is unavailable",
                    )
                })?
            }
        };
        resolved.push(memory);
    }
    Ok(resolved)
}

/// Applies the command-specific fallback after an explicitly degradable typed Host failure.
/// 在明确允许降级的强类型Host失败后应用命令专属回退；参数为当前记录和失败确认，返回后续步骤或错误。
///
/// Optional stages (lexical candidates, context evidence, rerank, semantic noise, and
/// the PostAction/PreCheck L2 reviews) fall back to their deterministic substitutes and
/// mark the command degraded; mandatory stages fail closed with a stable error.
/// 可选阶段（词法候选、情境证据、重排、语义 Noise、PostAction/PreCheck L2 审核）
/// 回退到其确定性替代并标记命令降级；必需阶段封闭失败并返回稳定错误。
/// Analyze review-chain query failures restore the admitted L1 snapshot; direct writes, persistence and contract faults do not inherit this permission.
/// Analyze评审链查询失败恢复已准入L1快照；主动写入、持久化及契约错误不继承这一回退权限。
#[allow(clippy::too_many_lines)]
fn degrade(
    record: &mut CommandRecord,
    result: &HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let HostEffectResult::Failure(failure) = result else {
        return Err(command_error(
            ErrorCode::EffectMismatch,
            "degrade requires a structured host failure",
        ));
    };
    // A structured-contract violation is never degraded: it reflects a host contract
    // defect that must surface to the caller instead of being masked.
    // 结构化契约违规绝不降级：它反映宿主契约缺陷，必须向调用方暴露而非掩盖。
    if failure.class == FailureClass::StructuredContract {
        return Err(command_error(
            ErrorCode::EffectDegraded,
            format!(
                "structured contract failure is not degradable: {}",
                failure.diagnostic
            ),
        ));
    }
    if !failure.allow_degrade {
        return Err(command_error(
            ErrorCode::EffectDegraded,
            format!(
                "host effect failure does not permit degradation: {}",
                failure.diagnostic
            ),
        ));
    }
    // Validate the reported surface before changing state. A non-retryable storage-mutation
    // failure cannot prove that no commit occurred, so it must retain the original batch and
    // enter reconciliation instead of being reported as a successful degradation.
    // 在改变状态前校验报告的失败表面。不可安全重试的存储变更失败无法证明提交未发生，
    // 因此必须保留原始批次并进入对账，而不能被报告为成功降级。
    ensure_failure_kind(&record.state, failure.kind)?;
    if failure.kind == EffectFailureKind::StorageMutation && !failure.retryable {
        let idempotency_key = record
            .pending_mutation_idempotency_key
            .clone()
            .ok_or_else(|| {
                command_error(
                    ErrorCode::Internal,
                    "failed storage mutation is missing its reconciliation identity",
                )
            })?;
        return mark_uncertain(record, idempotency_key);
    }
    record.degraded = true;
    let state = std::mem::replace(&mut record.state, FlowState::Terminal);
    match state {
        // Lexical and context-evidence failures degrade to empty deterministic channels.
        // 词法与情境证据失败降级为空确定性通道。
        FlowState::SearchPipeline(pipeline) => {
            let pipeline = *pipeline;
            match pipeline.stage {
                PipelineStage::Lexical => advance_search_pipeline(
                    record,
                    pipeline,
                    HostEffectResult::StorageQuery(StorageQueryResult::LexicalCandidates(
                        vulcan_memorymesh_core_contracts::CandidateChannelResult {
                            candidates: Vec::new(),
                            memories: Vec::new(),
                        },
                    )),
                ),
                PipelineStage::Evidence => {
                    // Failure skips all context for this query but cannot become a cached empty success.
                    // 失败跳过当前查询的全部情境，但不能变成已缓存的成功空观察。
                    finish_pipeline_evidence(record, pipeline, None)
                }
                PipelineStage::Rerank => start_pipeline_evidence(record, pipeline),
                PipelineStage::Observation
                    if record
                        .snapshot
                        .memory_policy
                        .retrieval_policy
                        .separate_channel_fallback
                        .is_some() =>
                {
                    activate_separate_channel_fallback(record, pipeline)
                }
                PipelineStage::Vector | PipelineStage::Records | PipelineStage::Observation => {
                    let PipelineFollowUp::AnalyzeReview(search) = pipeline.follow_up else {
                        return Err(effect_type_error(
                            "mandatory retrieval cannot degrade for this command",
                        ));
                    };
                    begin_analyze_l1_fallback(record, search.pending)
                }
                PipelineStage::Embedding => Err(command_error(
                    ErrorCode::EffectDegraded,
                    format!(
                        "host effect failed at a mandatory pipeline stage: {}",
                        failure.diagnostic
                    ),
                )),
            }
        }
        // PostAction L2 review transport/provider failure falls back to the L1 admitted
        // candidates, exactly as VMM does; structured-contract failures never degrade.
        // PostAction L2 审核传输/提供方失败回退到 L1 已准入候选，与 VMM 一致；
        // 结构化契约失败绝不降级。
        FlowState::AnalyzeReview(pending) | FlowState::AnalyzeProfileQuery(pending) => {
            begin_analyze_l1_fallback(record, *pending)
        }
        FlowState::AnalyzeEmbedding(pending) => begin_analyze_l1_fallback(record, *pending),
        // Direct-write review is authoritative in VMM: a reviewer failure aborts the write
        // instead of silently creating a record without semantic dedupe or supersede checks.
        // 主动写入审核在 VMM 中具有权威性：审核失败会中止写入，而不是绕过语义去重或替代
        // 检查后静默创建记录。
        FlowState::MemoryWriteReview(pending) => {
            record.state = FlowState::MemoryWriteReview(pending);
            Err(command_error(
                ErrorCode::EffectDegraded,
                format!(
                    "direct-write semantic review is mandatory: {}",
                    failure.diagnostic
                ),
            ))
        }
        // PreCheck L1 intent/embedding or L2 review failure returns an empty non-injecting
        // result; VMM sets selectedCandidates to nil and marks the result degraded instead of
        // injecting every candidate.
        // PreCheck L1 意图/嵌入或 L2 审核失败返回空的不注入结果；VMM 将 selectedCandidates
        // 置空并标记结果降级，而非注入全部候选。
        FlowState::PreCheckIntent { .. } => {
            complete_precheck_without_adoption(record, Vec::new(), Vec::new())
        }
        FlowState::PreCheckEmbedding { queries, .. } => {
            complete_precheck_without_adoption(record, queries, Vec::new())
        }
        FlowState::PreCheckReview {
            queries,
            candidates,
            ..
        } => complete_precheck_without_adoption(record, queries, candidates),
        // PreCheck reinforcement failure keeps the selected context but reports the
        // reinforcement write as failed instead of losing the recalled context.
        // PreCheck 强化失败保留已选上下文，但将强化写入报告为失败而非丢失召回上下文。
        FlowState::PreCheckReinforce(pending) => {
            let accepted_ids = pending
                .accepted
                .iter()
                .map(|hit| hit.source.memory.id.clone())
                .collect();
            let (context_items, context_text) =
                crate::precheck::assemble_context(&pending.accepted);
            complete(
                record,
                CommandResult::PreCheck(PreCheckResult {
                    should_inject: !context_items.is_empty(),
                    context_text,
                    context_items,
                    candidates: review_candidate_views(&pending.candidates),
                    degraded: true,
                    queries: pending.queries.clone(),
                    reinforcement: Some(ReinforcementResult {
                        reinforced_memory_ids: accepted_ids,
                        failed: true,
                        receipt: None,
                    }),
                }),
            )
        }
        // Semantic-noise cache query failure degrades to live phrase embedding so the
        // semantic decision still runs with freshly generated prototypes.
        // 语义 Noise 缓存查询失败降级为实时短语嵌入，使语义决策仍以新生成原型运行。
        FlowState::IntakeNoiseQuery(context) => begin_intake_noise_embedding(record, *context),
        // Semantic-noise phrase or text embedding failure degrades to regex-only gating,
        // preserving the already-rejected rows while skipping the semantic prototype check.
        // 语义 Noise 短语或文本嵌入失败降级为仅正则门控，保留已拒绝行并跳过语义原型检查。
        FlowState::IntakeNoiseEmbed(context) => {
            let mut context = *context;
            let failed_category = context
                .prototype_categories
                .get(context.prototype_category_index)
                .ok_or_else(|| effect_type_error("pending Noise category"))?;
            context
                .noise
                .unavailable_categories
                .push(failed_category.clone());
            context
                .phrase_targets
                .retain(|(category, _, _)| category != failed_category);
            context.prototype_category_index += 1;
            begin_intake_noise_embedding(record, context)
        }
        FlowState::IntakeNoiseScore { context, .. } => {
            let mut context = *context;
            context.noise.source = NoiseDecisionSource::RegexOnlyFallback;
            emit_intake_turn(record, context.command, context.noise)
        }
        // Semantic-noise cache write failure continues with the freshly built prototypes.
        // 语义 Noise 缓存写入失败继续使用新构建的原型。
        FlowState::IntakeNoiseStore {
            context,
            prototypes,
        } => {
            validate_noise_prototypes(record, &context, &prototypes)?;
            let rows = prototypes_to_rows(&context, &prototypes)?;
            advance_intake_semantic(record, *context, rows)
        }
        _ => Err(command_error(
            ErrorCode::EffectDegraded,
            format!(
                "host effect failed at a mandatory stage: {}",
                failure.diagnostic
            ),
        )),
    }
}

/// Activates the frozen split-read recovery after an allowed complete-observation failure.
/// 在允许的完整观察失败后激活冻结的分离读取恢复；清空不可复用事实并发出向量读取。
fn activate_separate_channel_fallback(
    record: &mut CommandRecord,
    mut pipeline: SearchPipeline,
) -> Result<CommandStep, CoreError> {
    if pipeline.work.separate_fallback_active {
        return Err(command_error(
            ErrorCode::Internal,
            "separate-channel fallback cannot be activated twice",
        ));
    }
    pipeline.work.separate_fallback_active = true;
    // The failed observation yielded no accepted fact. Recovery starts from a clean split-read ledger.
    // 失败观察未产生任何已接收事实；恢复必须从干净的分离读取账本开始。
    pipeline.work.vector.clear();
    pipeline.work.lexical.clear();
    pipeline.work.records.clear();
    pipeline.work.evidence.clear();
    pipeline.fused.clear();
    pipeline.rerank_indices.clear();
    effective_retrieval_policy(
        &pipeline.work,
        &record.snapshot.memory_policy.retrieval_policy,
    )?;
    pipeline.stage = PipelineStage::Vector;
    record.state = FlowState::SearchPipeline(Box::new(pipeline));
    start_pipeline(record)
}

/// Validates that one reported failure kind matches the effect the current flow awaits.
/// 校验一个报告的失败类别与当前流程等待的 Effect 一致。
fn ensure_failure_kind(state: &FlowState, kind: EffectFailureKind) -> Result<(), CoreError> {
    let expected = match state {
        FlowState::PreCheckIntent { .. }
        | FlowState::PreCheckReview { .. }
        | FlowState::AnalyzeReview(_)
        | FlowState::MemoryWriteReview(_) => EffectFailureKind::Llm,
        FlowState::PreCheckEmbedding { .. }
        | FlowState::AnalyzeEmbedding(_)
        | FlowState::IntakeNoiseEmbed(_)
        | FlowState::IntakeNoiseScore { .. } => EffectFailureKind::Embedding,
        FlowState::PreCheckReinforce(_) | FlowState::IntakeNoiseStore { .. } => {
            EffectFailureKind::StorageMutation
        }
        FlowState::IntakeNoiseQuery(_) | FlowState::AnalyzeProfileQuery(_) => {
            EffectFailureKind::StorageQuery
        }
        FlowState::SearchPipeline(pipeline) => match pipeline.stage {
            PipelineStage::Lexical | PipelineStage::Evidence => EffectFailureKind::StorageQuery,
            PipelineStage::Rerank => EffectFailureKind::Rerank,
            PipelineStage::Observation => {
                return if kind == EffectFailureKind::StorageQuery {
                    Ok(())
                } else {
                    Err(effect_type_error("candidate observation failure kind"))
                };
            }
            PipelineStage::Vector | PipelineStage::Records => {
                if matches!(pipeline.follow_up, PipelineFollowUp::AnalyzeReview(_)) {
                    return if kind == EffectFailureKind::StorageQuery {
                        Ok(())
                    } else {
                        Err(effect_type_error("Analyze retrieval failure kind"))
                    };
                }
                return Err(command_error(
                    ErrorCode::EffectMismatch,
                    "mandatory pipeline stage has no degradation path",
                ));
            }
            PipelineStage::Embedding => {
                return Err(command_error(
                    ErrorCode::EffectMismatch,
                    "mandatory query embedding has no degradation path",
                ));
            }
        },
        _ => {
            return Err(command_error(
                ErrorCode::EffectMismatch,
                "flow state has no degradation path",
            ));
        }
    };
    if expected == kind {
        Ok(())
    } else {
        Err(command_error(
            ErrorCode::EffectMismatch,
            format!(
                "reported failure kind {kind:?} does not match the pending effect {expected:?}"
            ),
        ))
    }
}

/// Retains the current flow state across an uncertain mutation by re-deriving the pending continuation.
/// 通过重新派生待处理续体在结果不确定的变更期间保留当前流程状态。
#[allow(clippy::unnecessary_wraps)]
fn retain_uncertain_state(record: &mut CommandRecord) -> Result<(), CoreError> {
    // The uncertain outcome is consumed without advancing; the flow continuation is
    // reconstructed by replaying the same advance path during reconciliation.
    // 不确定结果被消费而不推进；对账期间通过重放同一推进路径重建流程续体。
    let state = std::mem::replace(&mut record.state, FlowState::Terminal);
    record.state = state;
    Ok(())
}

/// Marks one command result-uncertain and returns its stable reconciliation error.
/// 将一个命令标记为结果不确定并返回其稳定对账错误。
fn mark_uncertain(
    record: &mut CommandRecord,
    idempotency_key: IdempotencyKey,
) -> Result<CommandStep, CoreError> {
    record.status = CommandStatus::ResultUncertain;
    let error = uncertain_storage_error(&record.handle, idempotency_key);
    record.last_error = Some(error.clone());
    Err(error)
}

/// Reconstructs the same no-replay diagnostic for live and restored uncertain mutations.
/// 为正在运行和已恢复的不确定变更构造相同的禁止重放诊断。
fn uncertain_storage_error(handle: &CommandHandle, idempotency_key: IdempotencyKey) -> CoreError {
    CoreError::new(
        ErrorCode::StorageResultUncertain,
        ErrorStage::Storage,
        "storage outcome is uncertain; reconcile by idempotency key and do not replay",
    )
    .with_uncertain_side_effect(idempotency_key)
    .with_command(handle.command_id.clone())
    .with_snapshot(handle.config_snapshot_id.clone())
}

/// Preserves reconciliation authority when an acknowledgement cannot establish the write outcome.
/// 当确认无法确立写入结果时保留对账权威。
fn preserve_invalid_mutation_outcome(
    record: &mut CommandRecord,
    unresolved_mutation: bool,
    retained_state: Option<FlowState>,
    transition: Result<CommandStep, CoreError>,
) -> Result<CommandStep, CoreError> {
    let Err(cause) = transition else {
        return transition;
    };
    if !unresolved_mutation || record.status != CommandStatus::Waiting {
        return Err(cause);
    }
    let state = retained_state.ok_or_else(|| {
        command_error(
            ErrorCode::Internal,
            "mutation acknowledgement validation lost its retained flow state",
        )
    })?;
    let idempotency_key = record
        .pending_mutation_idempotency_key
        .clone()
        .ok_or_else(|| {
            command_error(
                ErrorCode::Internal,
                "mutation acknowledgement validation lost its reconciliation key",
            )
        })?;
    record.state = state;
    record.status = CommandStatus::ResultUncertain;
    let error = CoreError::new(
        ErrorCode::StorageResultUncertain,
        ErrorStage::Storage,
        format!(
            "storage acknowledgement failed Core validation; reconcile by idempotency key: {}",
            cause.diagnostic
        ),
    )
    .with_uncertain_side_effect(idempotency_key)
    .with_command(record.handle.command_id.clone())
    .with_snapshot(record.handle.config_snapshot_id.clone());
    record.last_error = Some(error.clone());
    Err(error)
}

/// Applies the transition result to the record lifecycle.
/// 将转换结果应用到记录生命周期。
fn finalize_transition(record: &mut CommandRecord, transition: &Result<CommandStep, CoreError>) {
    if let Err(error) = transition
        && record.status == CommandStatus::Waiting
    {
        record.status = CommandStatus::Failed;
        record.pending_effect_id = None;
        record.pending_effect = None;
        record.pending_mutation_idempotency_key = None;
        record.last_error = Some(
            error
                .clone()
                .with_command(record.handle.command_id.clone())
                .with_snapshot(record.handle.config_snapshot_id.clone()),
        );
    }
}

/// Initializes `PreCheck` with local PII handling and the configured recall mode.
/// 使用本地 PII 处理与已配置召回模式初始化 `PreCheck`。
fn initialize_precheck(
    record: &mut CommandRecord,
    mut command: PreCheckCommand,
) -> Result<CommandStep, CoreError> {
    // Scrub user prose without collapsing layout; query normalization belongs to retrieval.
    // 脱敏用户正文但不折叠排版；查询归一化属于检索阶段。
    let content = record
        .snapshot
        .pii_scrubber
        .scrub(&command.user_content)?
        .text;
    command.user_content.clone_from(&content);
    command.recent_timeline = normalize_timeline(&record.snapshot, &command.recent_timeline)?;
    match command.recall_mode {
        RecallMode::Disabled => complete(
            record,
            CommandResult::PreCheck(PreCheckResult {
                should_inject: false,
                context_text: String::new(),
                context_items: Vec::new(),
                candidates: Vec::new(),
                degraded: record.degraded,
                queries: Vec::new(),
                reinforcement: None,
            }),
        ),
        RecallMode::Force => {
            // Use the complete normalized user sentence as the deterministic forced query.
            // 使用完整规范化用户句子作为确定性的强制查询。
            let queries = vec![normalize_text(&content)];
            record.state = FlowState::PreCheckEmbedding {
                command: command.clone(),
                content,
                queries: queries.clone(),
                intent_reason: None,
            };
            emit_effect(
                record,
                HostEffect::Embedding(EmbeddingEffect {
                    allow_partial_invalid_inputs: false,
                    vector_space_id: command.vector_space_id,
                    expected_dimension: record.snapshot.embedding_dimension,
                    inputs: queries,
                }),
            )
        }
        RecallMode::Auto => {
            // Load the authoritative L1 prompt from the frozen snapshot.
            // 从冻结快照加载权威 L1 Prompt。
            let prompt = record.snapshot.prompt("precheck_l1_main.md")?.to_owned();
            record.state = FlowState::PreCheckIntent {
                command: command.clone(),
                content: content.clone(),
            };
            emit_llm_effect(
                record,
                LlmPurpose::PreCheckIntent,
                prompt,
                LlmInput::PreCheckIntent {
                    recent_timeline: command.recent_timeline.clone(),
                    user_content: content,
                    max_search_queries: command.max_search_queries,
                },
            )
        }
    }
}

/// Advances `PreCheck` from L1 intent to query embeddings or immediate completion.
/// 将 `PreCheck` 从 L1 意图推进到查询嵌入或即时完成。
fn advance_precheck_intent(
    record: &mut CommandRecord,
    command: PreCheckCommand,
    content: String,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let output = expect_llm(result)?;
    vulcan_memorymesh_core_contracts::validate_llm_output(LlmPurpose::PreCheckIntent, &output)?;
    match output {
        LlmOutput::PreCheckIntent {
            reason,
            need_memory,
            mut queries,
        } => {
            if !need_memory {
                return complete(
                    record,
                    CommandResult::PreCheck(PreCheckResult {
                        should_inject: false,
                        context_text: String::new(),
                        context_items: Vec::new(),
                        candidates: Vec::new(),
                        degraded: record.degraded,
                        queries: Vec::new(),
                        reinforcement: None,
                    }),
                );
            }
            // Apply VMM's explicit deictic-query policy before retrieval; Force bypasses L1.
            // 检索前应用 VMM 明确的指代查询规则；Force 不经过 L1。
            queries = crate::precheck::normalize_intent_queries(
                queries,
                &content,
                !command.recent_timeline.is_empty(),
                command.max_search_queries as usize,
            );
            if queries.is_empty() {
                return complete_precheck_without_adoption(record, queries, Vec::new());
            }
            record.state = FlowState::PreCheckEmbedding {
                command: command.clone(),
                content,
                queries: queries.clone(),
                intent_reason: Some(normalize_text(&reason)),
            };
            emit_effect(
                record,
                HostEffect::Embedding(EmbeddingEffect {
                    allow_partial_invalid_inputs: false,
                    vector_space_id: command.vector_space_id,
                    expected_dimension: record.snapshot.embedding_dimension,
                    inputs: queries,
                }),
            )
        }
        _ => Err(effect_type_error("PreCheck intent LLM output")),
    }
}

/// Advances `PreCheck` from embeddings into the shared retrieval pipeline.
/// 将 `PreCheck` 从嵌入推进到共享检索流水线。
#[allow(clippy::needless_pass_by_value, clippy::redundant_clone)]
fn advance_precheck_embedding(
    record: &mut CommandRecord,
    command: PreCheckCommand,
    content: String,
    queries: Vec<String>,
    intent_reason: Option<String>,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let embeddings = expect_embeddings(result, queries.len(), record.snapshot.embedding_dimension)?;
    start_precheck_query_group(
        record,
        PendingPreCheckSearch {
            evidence_cache: ContextEvidenceCache::new(),
            command,
            content,
            queries,
            embeddings,
            group: 0,
            completed: ReviewAccumulator::default(),
            intent_reason,
        },
    )
}

/// Starts exactly one query using its previously accepted embedding and the independent recall budget.
/// 使用先前已接受的对应嵌入及独立召回预算，只启动一个查询，不跨组融合物理候选。
fn start_precheck_query_group(
    record: &mut CommandRecord,
    pending: PendingPreCheckSearch,
) -> Result<CommandStep, CoreError> {
    let query = pending
        .queries
        .get(pending.group)
        .ok_or_else(|| validation_error("PreCheck query group cursor is invalid"))?
        .clone();
    let embedding = pending
        .embeddings
        .get(pending.group)
        .ok_or_else(|| validation_error("PreCheck query embedding alignment is invalid"))?
        .clone();
    if pending.completed.processed_groups != pending.group
        || pending.queries.len() != pending.embeddings.len()
    {
        return Err(validation_error(
            "PreCheck completed groups or embeddings are misaligned",
        ));
    }
    let command = &pending.command;
    let work = SearchWork {
        subject_id: command.subject_id.clone(),
        scope_access: command.scope_access.clone(),
        queries: vec![query],
        embeddings: vec![embedding],
        vector_space_id: command.vector_space_id.clone(),
        boundary: Some(SessionRecallBoundary {
            session_source_id: command.session_source_id.clone(),
            mode: command.session_recall_mode,
        }),
        candidate_scope: Some(command.candidate_scope.clone()),
        limit: record.snapshot.memory_policy.precheck_policy.recall_top_k,
        enable_hard_dedupe_pool: false,
        now_ns: command.now_ns,
        separate_fallback_active: false,
        vector: Vec::new(),
        lexical: Vec::new(),
        records: BTreeMap::new(),
        evidence: Vec::new(),
    };
    let pipeline = SearchPipeline {
        stage: PipelineStage::Vector,
        work,
        follow_up: PipelineFollowUp::PreCheckGroup(Box::new(pending)),
        fused: Vec::new(),
        rerank_indices: Vec::new(),
    };
    record.state = FlowState::SearchPipeline(Box::new(pipeline));
    start_pipeline(record)
}

/// Advances `PreCheck` from fused hits to the L2 review.
/// 将 `PreCheck` 从融合命中推进到 L2 审核。
#[allow(clippy::needless_pass_by_value, clippy::redundant_clone)]
fn advance_pipeline_to_precheck(
    record: &mut CommandRecord,
    command: PreCheckCommand,
    content: String,
    queries: Vec<String>,
    intent_reason: Option<String>,
    groups: ReviewAccumulator,
) -> Result<CommandStep, CoreError> {
    let hits = crate::precheck::finish_review_candidates(
        groups,
        &record.snapshot.memory_policy.precheck_policy,
        &record.snapshot.pii_scrubber,
    )?;
    if hits.is_empty() {
        return complete_precheck_without_adoption(record, queries, Vec::new());
    }
    // Load the authoritative L2 review prompt from the frozen snapshot.
    // 从冻结快照加载权威 L2 审核 Prompt。
    let prompt = record.snapshot.prompt("precheck_l2_main.md")?.to_owned();
    record.state = FlowState::PreCheckReview {
        command: command.clone(),
        queries: queries.clone(),
        candidates: hits.clone(),
    };
    emit_llm_effect(
        record,
        LlmPurpose::PreCheckReview,
        prompt,
        LlmInput::PreCheckReview {
            current_timestamp_ns: command.now_ns,
            search_queries: queries,
            intent_reason,
            user_content: content,
            candidates: review_candidate_views(&hits),
        },
    )
}

/// Finishes without injection while retaining already observed query and candidate facts.
/// 在不注入上下文的同时保留已观察到的查询与候选事实，避免降级抹去审计来源。
fn complete_precheck_without_adoption(
    record: &mut CommandRecord,
    queries: Vec<String>,
    candidates: Vec<ReviewCandidateState>,
) -> Result<CommandStep, CoreError> {
    complete(
        record,
        CommandResult::PreCheck(PreCheckResult {
            should_inject: false,
            context_text: String::new(),
            context_items: Vec::new(),
            candidates: candidates
                .into_iter()
                .map(|candidate| candidate.view)
                .collect(),
            degraded: record.degraded,
            queries,
            reinforcement: None,
        }),
    )
}

/// Completes `PreCheck` from accepted L2 memory identities.
/// 从 L2 接受的记忆标识完成 `PreCheck`。
#[allow(clippy::needless_pass_by_value)]
fn advance_precheck_review(
    record: &mut CommandRecord,
    command: PreCheckCommand,
    queries: Vec<String>,
    candidates: Vec<ReviewCandidateState>,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let output = expect_llm(result)?;
    vulcan_memorymesh_core_contracts::validate_llm_output(LlmPurpose::PreCheckReview, &output)?;
    match output {
        LlmOutput::PreCheckReview {
            accepted_memory_ids,
        } => {
            // Reject duplicate accepted identities because they would make host behavior ambiguous.
            // 拒绝重复的接受标识，因为它们会使宿主行为不明确。
            let accepted_set = accepted_memory_ids.iter().collect::<BTreeSet<_>>();
            if accepted_set.len() != accepted_memory_ids.len() {
                return Err(effect_type_error(
                    "PreCheck review returned duplicate memory identities",
                ));
            }
            // Preserve review order while requiring every accepted identity to exist in candidates.
            // 保持审核顺序，同时要求每个接受标识都存在于候选中。
            // Validate the complete provider output before applying the caller limit. A
            // trailing foreign identity must not escape validation merely because enough
            // earlier candidates have already filled the result window.
            // 在应用调用方限制前校验输出的完整集合。即使前面的候选已填满结果窗口，
            // 尾部的外来标识也不得逃过校验。
            if accepted_memory_ids.iter().any(|memory_id| {
                !candidates
                    .iter()
                    .any(|hit| hit.view.memory_id == *memory_id)
            }) {
                return Err(effect_type_error(
                    "PreCheck review accepted an identity outside candidate set",
                ));
            }
            // Preserve review order while truncating only the already validated set.
            // 保持审核顺序，仅对已完整校验的集合执行截断。
            let mut accepted = Vec::new();
            for memory_id in accepted_memory_ids
                .into_iter()
                .take(command.max_memories as usize)
            {
                // Resolve the exact candidate once by stable memory identity.
                // 通过稳定记忆标识精确解析一次候选。
                let hit = candidates
                    .iter()
                    .find(|hit| hit.view.memory_id == memory_id)
                    .cloned()
                    .ok_or_else(|| {
                        effect_type_error(
                            "PreCheck review accepted an identity outside candidate set",
                        )
                    })?;
                accepted.push(hit);
            }
            if accepted.is_empty() {
                return complete(
                    record,
                    CommandResult::PreCheck(PreCheckResult {
                        should_inject: false,
                        context_text: String::new(),
                        context_items: Vec::new(),
                        candidates: review_candidate_views(&candidates),
                        degraded: record.degraded,
                        queries,
                        reinforcement: None,
                    }),
                );
            }
            // Emit the durable adoption reinforcement mutation for every accepted memory.
            // 为每个已接受记忆发出持久采纳强化变更。
            let mutations = accepted
                .iter()
                .map(|hit| Mutation::ReinforceMemory {
                    actor_subject_id: command.subject_id.clone(),
                    scope_id: hit.source.memory.scope_id.clone(),
                    expected_access: MemoryAccessIdentity::from(&hit.source.memory),
                    memory_id: hit.source.memory.id.clone(),
                    expected_version: hit.source.memory.version,
                    recalled_at_ns: command.now_ns,
                    source_session_id: Some(command.session_source_id.clone()),
                })
                .collect();
            let batch = record.mutation_batch("reinforce", mutations)?;
            record.state = FlowState::PreCheckReinforce(Box::new(PendingPreCheck {
                queries,
                candidates,
                accepted,
            }));
            emit_effect(record, HostEffect::StorageMutation(batch))
        }
        _ => Err(effect_type_error("PreCheck review LLM output")),
    }
}

/// Copies only approved redacted views across public L2 and result boundaries.
/// 只将已处理的脱敏视图复制到公共 L2 及结果边界，不泄露引擎私有原记录。
fn review_candidate_views(candidates: &[ReviewCandidateState]) -> Vec<PreCheckReviewCandidate> {
    candidates
        .iter()
        .map(|candidate| candidate.view.clone())
        .collect()
}

/// Initializes `MemorySearch` with one normalized query embedding.
/// 使用一次规范化查询嵌入初始化 `MemorySearch`。
fn initialize_memory_search(
    record: &mut CommandRecord,
    mut command: MemorySearchCommand,
) -> Result<CommandStep, CoreError> {
    // Normalize the query before requesting its embedding.
    // 在请求嵌入前规范化查询。
    command.query = normalize_text(&command.query);
    if command.limit == 0 {
        command.limit = record.snapshot.memory_policy.retrieval_policy.top_k_default;
    }
    // The pipeline needs one embedding first; request it as its own stage.
    // 流水线首先需要一次嵌入；将其作为独立阶段请求。
    record.state = FlowState::SearchPipeline(Box::new(SearchPipeline {
        stage: PipelineStage::Embedding,
        work: SearchWork {
            subject_id: command.subject_id.clone(),
            scope_access: command.scope_access.clone(),
            queries: vec![command.query.clone()],
            embeddings: Vec::new(),
            vector_space_id: command.vector_space_id.clone(),
            boundary: command.boundary.session_recall_boundary.clone(),
            candidate_scope: command.boundary.candidate_scope.clone(),
            limit: command.limit,
            enable_hard_dedupe_pool: false,
            now_ns: command.now_ns,
            separate_fallback_active: false,
            vector: Vec::new(),
            lexical: Vec::new(),
            records: BTreeMap::new(),
            evidence: Vec::new(),
        },
        follow_up: PipelineFollowUp::MemorySearch,
        fused: Vec::new(),
        rerank_indices: Vec::new(),
    }));
    // Emit the query embedding and mark the pipeline to consume it through the pending embedding path.
    // 发出查询嵌入并将流水线标记为通过待处理嵌入路径消费它。
    emit_embedding_for_pipeline(record, vec![command.query])
}

/// Initializes grouped multi-query memory search.
/// 初始化分组多查询记忆搜索。
#[allow(clippy::redundant_clone)]
fn initialize_memory_search_batch(
    record: &mut CommandRecord,
    mut command: MemorySearchBatchCommand,
) -> Result<CommandStep, CoreError> {
    command.queries = command
        .queries
        .iter()
        .map(|query| normalize_text(query))
        .collect();
    if command.top_k == 0 {
        command.top_k = record.snapshot.memory_policy.retrieval_policy.top_k_default;
    }
    // Prepare all unique embeddings before any physical query, then keep the physical pipeline single-query.
    // 在任何物理查询之前准备全部唯一嵌入，随后保持物理流水线为单查询。
    let query_plan = QueryBatchPlan::new(&command.queries);
    let inputs = query_plan.unique_queries.clone();
    let vector_space_id = command.vector_space_id.clone();
    record.state = FlowState::SearchPipeline(Box::new(SearchPipeline {
        stage: PipelineStage::Embedding,
        work: SearchWork {
            subject_id: command.subject_id.clone(),
            scope_access: command.scope_access.clone(),
            queries: inputs.clone(),
            embeddings: Vec::new(),
            vector_space_id,
            boundary: command.boundary.session_recall_boundary.clone(),
            candidate_scope: command.boundary.candidate_scope.clone(),
            limit: command.top_k,
            enable_hard_dedupe_pool: false,
            now_ns: command.now_ns,
            separate_fallback_active: false,
            vector: Vec::new(),
            lexical: Vec::new(),
            records: BTreeMap::new(),
            evidence: Vec::new(),
        },
        follow_up: PipelineFollowUp::MemorySearchBatchGroup {
            evidence_cache: ContextEvidenceCache::new(),
            command: command.clone(),
            query_plan,
            embeddings: Vec::new(),
            group: 0,
            completed: Vec::new(),
        },
        fused: Vec::new(),
        rerank_indices: Vec::new(),
    }));
    emit_embedding_for_pipeline(record, inputs)
}

/// Initializes one scoped memory details query.
/// 初始化一次有作用域约束的记忆详情查询。
fn initialize_memory_details(
    record: &mut CommandRecord,
    command: MemoryDetailsCommand,
) -> Result<CommandStep, CoreError> {
    let retained_command = command.clone();
    record.state = FlowState::Terminal;
    let step = emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::MemoryDetails {
            subject_id: command.subject_id,
            scope_access: command.scope_access,
            memory_id: command.memory_id,
        }),
    )?;
    if let CommandStep::Effects(_) = &step {
        record.state = FlowState::MemoryDetails(retained_command);
    }
    Ok(step)
}

/// Initializes one caller-ordered batch details lookup.
/// 初始化一次按调用方顺序排列的批量详情查询。
fn initialize_memory_details_batch(
    record: &mut CommandRecord,
    command: MemoryDetailsBatchCommand,
) -> Result<CommandStep, CoreError> {
    let retained_command = command.clone();
    record.state = FlowState::Terminal;
    let step = emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::DetailsBatch {
            subject_id: command.subject_id,
            scope_access: command.scope_access,
            memory_ids: command.memory_ids,
            turn_ids: command.turn_ids,
        }),
    )?;
    if let CommandStep::Effects(_) = &step {
        record.state = FlowState::MemoryDetailsBatch(retained_command);
    }
    Ok(step)
}

/// Initializes direct memory writing with a batch embedding effect.
/// 使用批量嵌入 Effect 初始化直接记忆写入。
fn initialize_memory_write(
    record: &mut CommandRecord,
    mut command: MemoryWriteCommand,
) -> Result<CommandStep, CoreError> {
    // Trim and scrub direct prose without destroying newlines, indentation, or compatibility forms.
    // 去除主动写入正文的首尾空白并脱敏，不破坏换行、缩进及兼容字符。
    command.item.summary = record
        .snapshot
        .pii_scrubber
        .scrub(command.item.summary.trim())?
        .text;
    command.item.details = record
        .snapshot
        .pii_scrubber
        .scrub(command.item.details.trim())?
        .text;
    let dedupe_hash =
        direct_write_dedupe_hash(&record.snapshot, &command.item, &command.session_source_id);
    let pending = PendingMemoryWriteDedupe {
        command: command.clone(),
        dedupe_hash: dedupe_hash.clone(),
    };
    record.state = FlowState::MemoryWriteDedupe(pending);
    emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::RecentDirectWriteDedupe {
            subject_id: command.subject_id,
            scope_access: command.scope_access,
            session_source_id: command.session_source_id,
            lookups: vec![DirectWriteDedupeLookup {
                target_scope_id: command.item.target_scope_id,
                scope_class: command.item.scope_class,
                dedupe_hash,
            }],
            created_not_before_ns: command
                .occurred_at_ns
                .saturating_sub(DIRECT_WRITE_DEDUPE_WINDOW_NS),
            evaluated_at_ns: command.occurred_at_ns,
        }),
    )
}

/// Initializes direct memory deletion as one atomic mutation.
/// 将直接记忆删除初始化为一个原子变更。
fn initialize_memory_delete(
    record: &mut CommandRecord,
    command: MemoryDeleteCommand,
) -> Result<CommandStep, CoreError> {
    record.state = FlowState::MemoryDeletePreflight(command.clone());
    emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::MemoryDetails {
            subject_id: command.subject_id,
            scope_access: command.scope_access,
            memory_id: command.memory_id,
        }),
    )
}

/// Initializes one atomic direct-memory batch write with one embedding call.
/// 使用一次嵌入调用初始化原子主动记忆批量写入。
fn initialize_memory_write_batch(
    record: &mut CommandRecord,
    mut command: MemoryWriteBatchCommand,
) -> Result<CommandStep, CoreError> {
    // Scrub both public summaries and durable details before either leaves Core.
    // 在公共摘要与持久详情离开 Core 前统一完成脱敏。
    for item in &mut command.items {
        item.summary = record
            .snapshot
            .pii_scrubber
            .scrub(item.summary.trim())?
            .text;
        item.details = record
            .snapshot
            .pii_scrubber
            .scrub(item.details.trim())?
            .text;
    }
    let dedupe_hashes = command
        .items
        .iter()
        .map(|item| direct_write_dedupe_hash(&record.snapshot, item, &command.session_source_id))
        .collect::<Vec<_>>();
    let lookups = command
        .items
        .iter()
        .zip(&dedupe_hashes)
        .map(|(item, dedupe_hash)| DirectWriteDedupeLookup {
            target_scope_id: item.target_scope_id.clone(),
            scope_class: item.scope_class,
            dedupe_hash: dedupe_hash.clone(),
        })
        .collect();
    record.state = FlowState::MemoryWriteBatchDedupe(PendingMemoryWriteBatchDedupe {
        command: command.clone(),
        dedupe_hashes,
    });
    emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::RecentDirectWriteDedupe {
            subject_id: command.subject_id,
            scope_access: command.scope_access,
            session_source_id: command.session_source_id,
            lookups,
            created_not_before_ns: command
                .occurred_at_ns
                .saturating_sub(DIRECT_WRITE_DEDUPE_WINDOW_NS),
            evaluated_at_ns: command.occurred_at_ns,
        }),
    )
}

/// Initializes one caller-idempotent atomic memory deletion batch.
/// 初始化一次调用方幂等的原子记忆删除批次。
fn initialize_memory_delete_batch(
    record: &mut CommandRecord,
    command: MemoryDeleteBatchCommand,
) -> Result<CommandStep, CoreError> {
    let memory_ids = command
        .items
        .iter()
        .map(|item| item.memory_id.clone())
        .collect();
    record.state = FlowState::MemoryDeleteBatchPreflight(command.clone());
    emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::DetailsBatch {
            subject_id: command.subject_id,
            scope_access: command.scope_access,
            memory_ids,
            turn_ids: Vec::new(),
        }),
    )
}

/// Initializes `ChatCompact` as one atomic latest-turn boundary mutation.
/// 将 `ChatCompact` 初始化为一次原子最新轮次边界变更。
fn initialize_chat_compact(
    record: &mut CommandRecord,
    command: ChatCompactCommand,
) -> Result<CommandStep, CoreError> {
    let mutation = Mutation::AdvanceChatCompactBoundary {
        subject_id: command.subject_id,
        scope_id: command.scope_access.write_scope,
        session_source_id: command.session_source_id,
        compacted_at_ns: command.compacted_at_ns,
    };
    let batch = record.mutation_batch("compact", vec![mutation])?;
    record.state = FlowState::ChatCompactMutation;
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Initializes one profile query through the host-resolved view.
/// 通过宿主解析视图初始化一次画像查询。
fn initialize_profile_query(
    record: &mut CommandRecord,
    command: ProfileQueryCommand,
) -> Result<CommandStep, CoreError> {
    record.state = FlowState::ProfileQuery(command.clone());
    emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::ProfileNodes {
            subject_id: command.subject_id,
            scope_access: command.scope_access,
            sections: command.view.sections,
            evaluated_at_ns: command.evaluated_at_ns,
        }),
    )
}

/// Initializes one profile instruction by first reading current visible nodes.
/// 通过先读取当前可见节点初始化一条画像指令。
fn initialize_profile_instruction(
    record: &mut CommandRecord,
    mut command: ProfileInstructionCommand,
) -> Result<CommandStep, CoreError> {
    // Normalize and scrub instruction text before storage or inference effects.
    // 在存储或推理 Effect 前规范化并脱敏指令文本。
    command.instruction = record
        .snapshot
        .pii_scrubber
        .scrub(command.instruction.trim())?
        .text;
    record.state = FlowState::ProfileInstructionQuery(command.clone());
    emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::ProfileNodes {
            subject_id: command.subject_id,
            scope_access: command.scope_access,
            sections: vec![command.target_section],
            evaluated_at_ns: command.occurred_at_ns,
        }),
    )
}

/// Initializes retention planning by reading raw scan facts.
/// 通过读取原始扫描事实初始化保留规划。
fn initialize_retention(
    record: &mut CommandRecord,
    command: RetentionCommand,
) -> Result<CommandStep, CoreError> {
    record.state = FlowState::RetentionQuery(command.clone());
    emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::RetentionScan {
            subject_id: command.subject_id,
            scope_access: command.scope_access,
            now_ns: command.now_ns,
            policy: command.policy,
        }),
    )
}

/// Initializes one explicit lifecycle or vector maintenance operation.
/// 初始化一次显式生命周期或向量维护操作。
#[allow(clippy::needless_pass_by_value)]
fn initialize_maintenance(
    record: &mut CommandRecord,
    command: MaintenanceCommand,
) -> Result<CommandStep, CoreError> {
    match &command.action {
        MaintenanceAction::RestoreRecycled(items) | MaintenanceAction::PurgeRecycled(items) => {
            let memory_ids = items.iter().map(|item| item.memory_id.clone()).collect();
            record.state = FlowState::MaintenanceLifecyclePreflight(command.clone());
            emit_effect(
                record,
                HostEffect::StorageQuery(StorageQuery::DetailsBatch {
                    subject_id: command.subject_id,
                    scope_access: command.scope_access,
                    memory_ids,
                    turn_ids: Vec::new(),
                }),
            )
        }
        MaintenanceAction::RebuildVectors { memory_ids, .. } => {
            record.state = FlowState::MaintenanceRebuildQuery(command.clone());
            emit_effect(
                record,
                HostEffect::StorageQuery(StorageQuery::DetailsBatch {
                    subject_id: command.subject_id.clone(),
                    scope_access: command.scope_access.clone(),
                    memory_ids: memory_ids.clone(),
                    turn_ids: Vec::new(),
                }),
            )
        }
    }
}

/// Requests the first embedding of one retrieval pipeline.
/// 请求一个检索流水线的首个嵌入。
fn emit_embedding_for_pipeline(
    record: &mut CommandRecord,
    inputs: Vec<String>,
) -> Result<CommandStep, CoreError> {
    let vector_space_id = match &record.state {
        FlowState::SearchPipeline(pipeline) => pipeline.work.vector_space_id.clone(),
        _ => {
            return Err(command_error(
                ErrorCode::Internal,
                "pipeline embedding requested outside a search pipeline",
            ));
        }
    };
    emit_effect(
        record,
        HostEffect::Embedding(EmbeddingEffect {
            vector_space_id,
            allow_partial_invalid_inputs: false,
            expected_dimension: record.snapshot.embedding_dimension,
            inputs,
        }),
    )
}

/// Starts or advances the shared retrieval pipeline by emitting its next stage effect.
/// 通过发出下一阶段 Effect 启动或推进共享检索流水线。
fn start_pipeline(record: &mut CommandRecord) -> Result<CommandStep, CoreError> {
    // Select IO consistency from the frozen policy before emitting any physical read.
    // 在发出物理读取前按冻结策略选择IO一致性，不按融合方式或后端名称猜测。
    if let FlowState::SearchPipeline(pipeline) = &mut record.state
        && pipeline.stage == PipelineStage::Vector
        && effective_retrieval_policy(
            &pipeline.work,
            &record.snapshot.memory_policy.retrieval_policy,
        )?
        .candidate_read_mode
            == vulcan_memorymesh_core_contracts::CandidateReadMode::SingleObservation
    {
        pipeline.stage = PipelineStage::Observation;
    }
    let FlowState::SearchPipeline(pipeline) = &record.state else {
        return Err(command_error(
            ErrorCode::Internal,
            "pipeline state is missing",
        ));
    };
    let stage_effect = pipeline_effect(pipeline, &record.snapshot)?;
    emit_effect(record, stage_effect)
}

/// Derives the exact pending effect from frozen pipeline facts for both live execution and restore validation.
/// 根据冻结流水线事实推导精确待处理效果，供实时执行及恢复校验共用；参数为流水线和快照，返回效果或错误。
fn pipeline_effect(
    pipeline: &SearchPipeline,
    snapshot: &ConfigSnapshot,
) -> Result<HostEffect, CoreError> {
    let work = &pipeline.work;
    let policy = effective_retrieval_policy(work, &snapshot.memory_policy.retrieval_policy)?;
    Ok(match pipeline.stage {
        PipelineStage::Observation => HostEffect::StorageQuery(StorageQuery::CandidateObservation(
            candidate_observation_query(work, snapshot)?,
        )),
        PipelineStage::Embedding => HostEffect::Embedding(EmbeddingEffect {
            allow_partial_invalid_inputs: false,
            vector_space_id: work.vector_space_id.clone(),
            expected_dimension: snapshot.embedding_dimension,
            inputs: work.queries.clone(),
        }),
        PipelineStage::Vector => HostEffect::StorageQuery(StorageQuery::VectorCandidates {
            subject_id: work.subject_id.clone(),
            scope_access: work.scope_access.clone(),
            queries: work.queries.clone(),
            query_embeddings: work.embeddings.clone(),
            vector_space_id: work.vector_space_id.clone(),
            boundary: CandidateQueryBoundary {
                candidate_scope: work.candidate_scope.clone(),
                session_recall_boundary: work.boundary.clone(),
            },
            limit: candidate_pool_limit(work, &snapshot.memory_policy),
            expected_contract: policy.vector_contract.contract,
            now_ns: work.now_ns,
        }),
        PipelineStage::Lexical => HostEffect::StorageQuery(StorageQuery::LexicalCandidates {
            subject_id: work.subject_id.clone(),
            scope_access: work.scope_access.clone(),
            queries: work.queries.clone(),
            vector_space_id: work.vector_space_id.clone(),
            boundary: CandidateQueryBoundary {
                candidate_scope: work.candidate_scope.clone(),
                session_recall_boundary: work.boundary.clone(),
            },
            limit: candidate_pool_limit(work, &snapshot.memory_policy),
            expected_contract: policy.lexical_contract.contract,
            now_ns: work.now_ns,
        }),
        PipelineStage::Records => HostEffect::StorageQuery(StorageQuery::MemoryRecordBatch {
            subject_id: work.subject_id.clone(),
            scope_access: work.scope_access.clone(),
            memory_ids: candidate_missing_record_ids(&pipeline.work)?,
        }),
        PipelineStage::Evidence => HostEffect::StorageQuery(StorageQuery::ContextEvidenceBatch {
            subject_id: work.subject_id.clone(),
            scope_access: work.scope_access.clone(),
            memory_ids: pipeline_missing_evidence_ids(pipeline),
        }),
        PipelineStage::Rerank => HostEffect::Rerank(RerankEffect {
            query: work.queries.first().cloned().unwrap_or_default(),
            documents: pipeline
                .rerank_indices
                .iter()
                .filter_map(|index| pipeline.fused.get(*index))
                .map(|hit| rerank_document_text(&hit.memory))
                .collect(),
        }),
    })
}

/// Derives one complete physical query from frozen work and policy for both emission and restore validation.
/// 从冻结工作及策略派生完整物理查询，供发出和恢复校验共用；返回唯一对齐的查询或错误。
fn candidate_observation_query(
    work: &SearchWork,
    snapshot: &ConfigSnapshot,
) -> Result<vulcan_memorymesh_core_contracts::CandidateObservationQuery, CoreError> {
    let ([query], [embedding]) = (work.queries.as_slice(), work.embeddings.as_slice()) else {
        return Err(effect_type_error(
            "single-observation query and embedding must be exactly aligned to one query",
        ));
    };
    let policy = &snapshot.memory_policy.retrieval_policy;
    Ok(
        vulcan_memorymesh_core_contracts::CandidateObservationQuery {
            subject_id: work.subject_id.clone(),
            scope_access: work.scope_access.clone(),
            query: query.clone(),
            query_embedding: embedding.clone(),
            vector_space_id: work.vector_space_id.clone(),
            boundary: CandidateQueryBoundary {
                candidate_scope: work.candidate_scope.clone(),
                session_recall_boundary: work.boundary.clone(),
            },
            limit: candidate_pool_limit(work, &snapshot.memory_policy),
            vector_contract: policy.vector_contract.contract.clone(),
            lexical_contract: policy
                .hybrid_enabled
                .then(|| policy.lexical_contract.contract.clone()),
            now_ns: work.now_ns,
        },
    )
}

/// Collects the deduplicated candidate memory identities for fact queries.
/// 为事实查询收集去重后的候选记忆标识。
fn candidate_memory_ids(work: &SearchWork) -> Result<Vec<MemoryId>, CoreError> {
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for candidate in work.vector.iter().chain(&work.lexical) {
        if seen.insert(candidate.memory_id.clone()) {
            ids.push(
                MemoryId::new(candidate.memory_id.clone())
                    .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?,
            );
        }
    }
    Ok(ids)
}

/// Collects candidate identities that were not materialized by their source channel.
/// 收集尚未由来源通道物化的候选身份，保持向量优先及词法补缺顺序。
fn candidate_missing_record_ids(work: &SearchWork) -> Result<Vec<MemoryId>, CoreError> {
    Ok(candidate_memory_ids(work)?
        .into_iter()
        .filter(|id| !work.records.contains_key(id))
        .collect())
}

/// Computes the bounded candidate pool limit for one retrieval window.
/// 计算一个检索窗口的有界候选池限制。
fn candidate_pool_limit(
    work: &SearchWork,
    policy: &vulcan_memorymesh_core_config::MemoryPolicy,
) -> u32 {
    retrieval_kernel::candidate_pool_limit(
        work.limit,
        work.enable_hard_dedupe_pool,
        &policy.retrieval_policy,
    )
}

/// Resolves the one retrieval policy that applies to the retained pipeline path.
/// 根据流水线保留的路径解析唯一生效的检索策略；缺少已激活配置时返回错误。
fn effective_retrieval_policy(
    work: &SearchWork,
    policy: &vulcan_memorymesh_core_contracts::RetrievalPolicy,
) -> Result<vulcan_memorymesh_core_contracts::RetrievalPolicy, CoreError> {
    if !work.separate_fallback_active {
        return Ok(policy.clone());
    }
    let fallback = policy.separate_channel_fallback.as_ref().ok_or_else(|| {
        command_error(
            ErrorCode::Internal,
            "active separate-channel fallback is missing its frozen policy",
        )
    })?;
    let mut effective = policy.clone();
    effective.candidate_read_mode =
        vulcan_memorymesh_core_contracts::CandidateReadMode::SeparateChannels;
    effective.rrf_score_mode = vulcan_memorymesh_core_contracts::RrfScoreMode::RankNormalized;
    effective.vector_contract = fallback.vector_contract.clone();
    effective.lexical_contract = fallback.lexical_contract.clone();
    effective.rrf_math_profile = fallback.rrf_math_profile;
    effective.dual_channel_bonus = fallback.dual_channel_bonus;
    effective.separate_channel_fallback = None;
    Ok(effective)
}

/// Rejects a physical candidate channel that exceeds the exact requested storage bound.
/// 拒绝超过精确存储查询上限的物理候选通道。
fn validate_candidate_result_bound(
    candidates: &[PhysicalCandidate],
    limit: u32,
    channel: &str,
) -> Result<(), CoreError> {
    if candidates.len() <= limit as usize {
        Ok(())
    } else {
        Err(effect_type_error(format!(
            "{channel} candidate result exceeds the requested bound"
        )))
    }
}

/// Validates caller-aligned candidate records and their exact visibility and vector-space facts.
/// 校验按调用方对齐的候选记录及其精确可见性与向量空间事实。
fn validate_candidate_records(
    work: &SearchWork,
    requested_ids: &[MemoryId],
    records: &[Option<MemoryRecord>],
    expected_dimension: u32,
) -> Result<(), CoreError> {
    if records.len() != requested_ids.len() {
        return Err(effect_type_error(
            "candidate memory records are not aligned with requested identities",
        ));
    }
    for (requested_id, record) in requested_ids.iter().zip(records) {
        let Some(record) = record else { continue };
        validate_candidate_memory_record(work, requested_id, record, expected_dimension)?;
    }
    Ok(())
}

/// Validates one exact materialized candidate for both separate and indivisible read paths.
/// 为分离及不可分割读取路径校验一条精确物化候选；参数为查询、预期身份、记录和维数，返回错误或成功。
fn validate_candidate_memory_record(
    work: &SearchWork,
    requested_id: &MemoryId,
    record: &MemoryRecord,
    expected_dimension: u32,
) -> Result<(), CoreError> {
    validate_host_memory_record(record, Some((&work.vector_space_id, expected_dimension)))?;
    if record.id != *requested_id
        || !record.is_visible_to_subject(&work.subject_id)
        || !work.scope_access.visible_scopes.contains(&record.scope_id)
        || work
            .candidate_scope
            .as_ref()
            .is_some_and(|scope| !scope.matches(record))
        || record.state != MemoryState::Active
        || record
            .expires_at_ns
            .is_some_and(|expires_at_ns| expires_at_ns <= work.now_ns)
        || record.vector_space_id.as_ref() != Some(&work.vector_space_id)
    {
        return Err(effect_type_error(
            "candidate memory record violates identity, visibility, lifecycle, or vector-space bounds",
        ));
    }
    Ok(())
}

/// Validates records carried by one split candidate result without accepting partial closure.
/// 校验一条分离候选结果携带的源记录，不接受部分闭包或错位身份。
fn validate_candidate_channel_memories(
    work: &SearchWork,
    candidates: &[PhysicalCandidate],
    memories: &[MemoryRecord],
    expected_dimension: u32,
) -> Result<(), CoreError> {
    if memories.is_empty() {
        return Ok(());
    }
    if memories.len() != candidates.len() {
        return Err(effect_type_error(
            "candidate channel records do not exactly cover its candidates",
        ));
    }
    for (candidate, memory) in candidates.iter().zip(memories) {
        let requested_id = MemoryId::new(candidate.memory_id.clone())
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
        validate_candidate_memory_record(work, &requested_id, memory, expected_dimension)?;
    }
    Ok(())
}

/// Retains source records while preserving the established vector-first merge rule.
/// 保留来源记录，并维持既有的向量优先、词法仅补缺合并规则。
fn merge_candidate_channel_memories(work: &mut SearchWork, memories: Vec<MemoryRecord>) {
    for memory in memories {
        work.records.entry(memory.id.clone()).or_insert(memory);
    }
}

/// Revalidates all retained records against a bounded candidate identity set.
/// 针对有界候选身份集合重新校验全部已保留记录，并拒绝额外身份。
fn validate_retained_candidate_records(
    work: &SearchWork,
    allowed_ids: &[MemoryId],
    expected_dimension: u32,
) -> Result<(), CoreError> {
    for (id, memory) in &work.records {
        if !allowed_ids.contains(id) {
            return Err(effect_type_error(
                "candidate continuation contains an unrequested memory record",
            ));
        }
        validate_candidate_memory_record(work, id, memory, expected_dimension)?;
    }
    Ok(())
}

/// Accepts a complete observation only after validating channel bounds, presence, provenance and record closure.
/// 仅在通道预算、存在性、来源与记录闭包全部校验后接受完整观察；成功保存事实，失败不提交状态。
fn accept_candidate_observation(
    work: &mut SearchWork,
    observation: vulcan_memorymesh_core_contracts::CandidateObservation,
    policy: &vulcan_memorymesh_core_config::MemoryPolicy,
    expected_dimension: u32,
) -> Result<(), CoreError> {
    let retrieval = &policy.retrieval_policy;
    if observation.lexical_channel.is_some() != retrieval.hybrid_enabled {
        return Err(effect_type_error(
            "observation lexical channel presence differs from the requested mode",
        ));
    }
    let limit = candidate_pool_limit(work, policy);
    for (channel, descriptor, name) in std::iter::once((
        &observation.vector_channel,
        &retrieval.vector_contract,
        "vector",
    ))
    .chain(
        observation
            .lexical_channel
            .as_ref()
            .map(|channel| (channel, &retrieval.lexical_contract, "lexical")),
    ) {
        validate_candidate_result_bound(&channel.candidates, limit, name)?;
        vulcan_memorymesh_core_contracts::validate_candidate_channel(&channel.candidates)
            .map_err(|error| effect_type_error(error.diagnostic))?;
        vulcan_memorymesh_core_contracts::validate_candidate_channel_contract(
            &channel.candidates,
            descriptor,
        )
        .map_err(|error| effect_type_error(error.diagnostic))?;
    }
    // Stage both channels locally to derive their exact unique union; no positional record guessing is allowed.
    // 在局部续体暂存两通道以求精确唯一并集，不按记录位置猜测归属。
    work.vector = observation.vector_channel.candidates;
    work.lexical = observation
        .lexical_channel
        .map_or_else(Vec::new, |channel| channel.candidates);
    let requested_ids = candidate_memory_ids(work)?;
    if observation.memories.len() != requested_ids.len() {
        return Err(effect_type_error(
            "observation records do not exactly cover the candidate union",
        ));
    }
    let mut records = BTreeMap::new();
    for memory in observation.memories {
        if records.insert(memory.id.clone(), memory).is_some() {
            return Err(effect_type_error(
                "observation contains a duplicate record identity",
            ));
        }
    }
    for requested_id in &requested_ids {
        let memory = records
            .get(requested_id)
            .ok_or_else(|| effect_type_error("observation candidate record is missing"))?;
        validate_candidate_memory_record(work, requested_id, memory, expected_dimension)?;
    }
    work.records = records;
    Ok(())
}

/// Validates caller-aligned context evidence groups and stable unique row ordering.
/// 校验按调用方对齐的情境证据分组以及稳定且唯一的行顺序。
fn validate_context_evidence_groups(
    requested_ids: &[MemoryId],
    groups: &[vulcan_memorymesh_core_contracts::ContextEvidenceGroup],
) -> Result<(), CoreError> {
    if groups.len() != requested_ids.len() {
        return Err(effect_type_error(
            "context evidence groups are not aligned with requested identities",
        ));
    }
    for (requested_id, group) in requested_ids.iter().zip(groups) {
        if group.memory_id != *requested_id || group.evidence.len() > MAX_DOMAIN_COLLECTION_ITEMS {
            return Err(effect_type_error(
                "context evidence group identity does not match its requested slot",
            ));
        }
        let mut previous: Option<(&str, &str)> = None;
        for row in &group.evidence {
            let current = (row.context_key.as_str(), row.context_value.as_str());
            if row.memory_id != *requested_id
                || row.context_key.trim().is_empty()
                || row.context_value.trim().is_empty()
                || row.context_key.len() > 4 * 1024 * 1024
                || row.context_value.len() > 4 * 1024 * 1024
                || row.context_key.contains('\0')
                || row.context_value.contains('\0')
                || row.version == 0
                || row.support_count == 0 && row.rebuttal_count == 0
                || (row.support_count == 0) != row.last_supported_at_ns.is_none()
                || (row.rebuttal_count == 0) != row.last_rebutted_at_ns.is_none()
                || previous.is_some_and(|value| value >= current)
            {
                return Err(effect_type_error(
                    "context evidence rows violate identity, value, version, count, or stable-order bounds",
                ));
            }
            previous = Some(current);
        }
    }
    Ok(())
}

/// Advances the shared retrieval pipeline with one stage result.
/// 使用一个阶段结果推进共享检索流水线。
#[allow(clippy::manual_flatten, clippy::too_many_lines)]
fn advance_search_pipeline(
    record: &mut CommandRecord,
    mut pipeline: SearchPipeline,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    match pipeline.stage {
        PipelineStage::Observation => {
            let StorageQueryResult::CandidateObservation(observation) =
                expect_storage_query(result)?
            else {
                return Err(effect_type_error(
                    "complete candidate observation storage result",
                ));
            };
            accept_candidate_observation(
                &mut pipeline.work,
                observation,
                &record.snapshot.memory_policy,
                record.snapshot.embedding_dimension,
            )?;
            start_pipeline_scoring(record, pipeline)
        }
        PipelineStage::Embedding => {
            let embeddings = expect_embeddings(
                result,
                pipeline.work.queries.len(),
                record.snapshot.embedding_dimension,
            )?;
            if let PipelineFollowUp::MemorySearchBatchGroup {
                query_plan,
                embeddings: prepared,
                ..
            } = &mut pipeline.follow_up
            {
                // Keep the full accepted batch in the continuation, exposing only the first query to storage.
                // 续体保留完整已接受批次，仅向存储暴露首条查询。
                pipeline.work.queries = vec![query_plan.unique_queries[0].clone()];
                pipeline.work.embeddings = vec![embeddings[0].clone()];
                *prepared = embeddings;
            } else {
                pipeline.work.embeddings = embeddings;
            }
            pipeline.stage = PipelineStage::Vector;
            record.state = FlowState::SearchPipeline(Box::new(pipeline));
            start_pipeline(record)
        }
        PipelineStage::Vector => {
            let StorageQueryResult::VectorCandidates(channel) = expect_storage_query(result)?
            else {
                return Err(effect_type_error("vector candidates storage result"));
            };
            let policy = effective_retrieval_policy(
                &pipeline.work,
                &record.snapshot.memory_policy.retrieval_policy,
            )?;
            validate_candidate_result_bound(
                &channel.candidates,
                candidate_pool_limit(&pipeline.work, &record.snapshot.memory_policy),
                "vector",
            )?;
            vulcan_memorymesh_core_contracts::validate_candidate_channel(&channel.candidates)
                .map_err(|error| effect_type_error(error.diagnostic))?;
            // Every returned candidate must be scored by the exact contract this stage
            // requested; a mismatched space would corrupt the fused ranking.
            // 每个返回候选必须由本阶段请求的精确契约评分；契约空间不匹配会破坏融合排序。
            vulcan_memorymesh_core_contracts::validate_candidate_channel_contract(
                &channel.candidates,
                &policy.vector_contract,
            )
            .map_err(|error| effect_type_error(error.diagnostic))?;
            validate_candidate_channel_memories(
                &pipeline.work,
                &channel.candidates,
                &channel.memories,
                record.snapshot.embedding_dimension,
            )?;
            pipeline.work.vector = channel.candidates;
            merge_candidate_channel_memories(&mut pipeline.work, channel.memories);
            // When the hybrid switch is off, the pipeline runs vector-only and skips the
            // lexical channel entirely; the fusion then degenerates to the vector ranking.
            // 混合开关关闭时，流水线仅运行向量通道并完全跳过词法通道；
            // 融合随之退化为纯向量排序。
            pipeline.stage = if policy.hybrid_enabled {
                PipelineStage::Lexical
            } else {
                PipelineStage::Records
            };
            if pipeline.stage == PipelineStage::Records
                && candidate_missing_record_ids(&pipeline.work)?.is_empty()
            {
                start_pipeline_scoring(record, pipeline)
            } else {
                record.state = FlowState::SearchPipeline(Box::new(pipeline));
                start_pipeline(record)
            }
        }
        PipelineStage::Lexical => {
            let StorageQueryResult::LexicalCandidates(channel) = expect_storage_query(result)?
            else {
                return Err(effect_type_error("lexical candidates storage result"));
            };
            let policy = effective_retrieval_policy(
                &pipeline.work,
                &record.snapshot.memory_policy.retrieval_policy,
            )?;
            validate_candidate_result_bound(
                &channel.candidates,
                candidate_pool_limit(&pipeline.work, &record.snapshot.memory_policy),
                "lexical",
            )?;
            vulcan_memorymesh_core_contracts::validate_candidate_channel(&channel.candidates)
                .map_err(|error| effect_type_error(error.diagnostic))?;
            vulcan_memorymesh_core_contracts::validate_candidate_channel_contract(
                &channel.candidates,
                &policy.lexical_contract,
            )
            .map_err(|error| effect_type_error(error.diagnostic))?;
            validate_candidate_channel_memories(
                &pipeline.work,
                &channel.candidates,
                &channel.memories,
                record.snapshot.embedding_dimension,
            )?;
            pipeline.work.lexical = channel.candidates;
            merge_candidate_channel_memories(&mut pipeline.work, channel.memories);
            if candidate_missing_record_ids(&pipeline.work)?.is_empty() {
                start_pipeline_scoring(record, pipeline)
            } else {
                pipeline.stage = PipelineStage::Records;
                record.state = FlowState::SearchPipeline(Box::new(pipeline));
                start_pipeline(record)
            }
        }
        PipelineStage::Records => {
            let StorageQueryResult::MemoryRecordBatch(records) = expect_storage_query(result)?
            else {
                return Err(effect_type_error("memory record batch storage result"));
            };
            let requested_ids = candidate_missing_record_ids(&pipeline.work)?;
            validate_candidate_records(
                &pipeline.work,
                &requested_ids,
                &records,
                record.snapshot.embedding_dimension,
            )?;
            for record_value in records.into_iter().flatten() {
                pipeline
                    .work
                    .records
                    .insert(record_value.id.clone(), record_value);
            }
            start_pipeline_scoring(record, pipeline)
        }
        PipelineStage::Evidence => {
            let StorageQueryResult::ContextEvidenceBatch(groups) = expect_storage_query(result)?
            else {
                return Err(effect_type_error("context evidence storage result"));
            };
            finish_pipeline_evidence(record, pipeline, Some(groups))
        }
        PipelineStage::Rerank => {
            // Entries retain provider order and address the exact non-empty document list; missing entries stay untouched.
            // 条目保留提供方顺序，并寻址精确的非空文档列表；未返回的条目保持原样。
            let score_count = pipeline.rerank_indices.len();
            let scores = expect_rerank(result, score_count)?;
            pipeline.fused = apply_rerank_scores(
                std::mem::take(&mut pipeline.fused),
                &pipeline.rerank_indices,
                scores,
            )?;
            start_pipeline_evidence(record, pipeline)
        }
    }
}

/// Fuses confirmed records and emits optional reranking before any live evidence read.
/// 对已确认记录执行融合，并在实时证据读取前发出可选重排；参数是当前命令及续体，返回下一步或校验错误。
fn start_pipeline_scoring(
    record: &mut CommandRecord,
    mut pipeline: SearchPipeline,
) -> Result<CommandStep, CoreError> {
    let policy = effective_retrieval_policy(
        &pipeline.work,
        &record.snapshot.memory_policy.retrieval_policy,
    )?;
    pipeline.fused = build_fused_hits(&pipeline.work, &policy)?;
    if pipeline.fused.is_empty() {
        return finish_pipeline(record, pipeline, Vec::new(), Vec::new());
    }
    pipeline.rerank_indices =
        rerank_candidate_indices(&pipeline.fused, policy.rerank_top_n as usize);
    if pipeline.rerank_indices.len() > 1 {
        pipeline.stage = PipelineStage::Rerank;
        record.state = FlowState::SearchPipeline(Box::new(pipeline));
        start_pipeline(record)
    } else {
        start_pipeline_evidence(record, pipeline)
    }
}

/// Applies decay once and reads evidence only for surviving hits in their source-defined order.
/// 对命令续体仅衰减一次，再按来源定义顺序读取存活命中的证据；返回等待或完成，不重跑已经确认的重排。
fn start_pipeline_evidence(
    record: &mut CommandRecord,
    mut pipeline: SearchPipeline,
) -> Result<CommandStep, CoreError> {
    let policy = &record.snapshot.memory_policy.retrieval_policy;
    apply_lifecycle_scores(&pipeline.work, policy, &mut pipeline.fused);
    pipeline.rerank_indices.clear();
    // Disabled context and signal-free queries cannot affect scoring and must not add an IO failure path.
    // 禁用情境或没有可匹配短语的查询不能影响评分，也不得增加额外的IO失败路径。
    if !policy.context.enabled
        || retrieval_kernel::normalize_context_value(&pipeline.work.queries.join(" ")).is_empty()
    {
        let fused = std::mem::take(&mut pipeline.fused);
        return finish_scored_pipeline(record, pipeline, fused);
    }
    if pipeline_missing_evidence_ids(&pipeline).is_empty() {
        return finish_pipeline_evidence(record, pipeline, Some(Vec::new()));
    }
    pipeline.stage = PipelineStage::Evidence;
    record.state = FlowState::SearchPipeline(Box::new(pipeline));
    start_pipeline(record)
}

/// Returns evidence identities from the post-fusion, post-rerank, post-decay hit list.
/// 从融合、重排、衰减后的命中列表返回证据身份；输入已校验续体，不查询已淘汰的物理候选。
fn pipeline_evidence_ids(pipeline: &SearchPipeline) -> Vec<MemoryId> {
    // A provider may return repeated occurrences; storage identities remain unique in first-occurrence order.
    // 提供方可返回重复命中，但存储身份仍按首次出现顺序去重，不能把重复命中作为重复查询槽位。
    let mut seen = BTreeSet::new();
    pipeline
        .fused
        .iter()
        .filter(|hit| seen.insert(hit.memory.id.clone()))
        .map(|hit| hit.memory.id.clone())
        .collect()
}

/// Locates the cache whose lifetime matches the concrete source Search invocation, or none for a single query.
/// 根据明确来源Search调用定位缓存；单查询返回无缓存，参数为已绑定的检索续体。
fn pipeline_evidence_cache(pipeline: &SearchPipeline) -> Option<&ContextEvidenceCache> {
    match &pipeline.follow_up {
        PipelineFollowUp::MemorySearch => None,
        PipelineFollowUp::MemorySearchBatchGroup { evidence_cache, .. } => Some(evidence_cache),
        PipelineFollowUp::AnalyzeReview(search) => Some(&search.evidence_cache),
        PipelineFollowUp::PreCheckGroup(pending) => Some(&pending.evidence_cache),
        PipelineFollowUp::DirectWriteReview { pending, .. } => Some(&pending.evidence_cache),
    }
}

/// Locates the same invocation-owned cache for accepting a validated successful observation.
/// 为接收已校验成功观察定位同一调用所拥有的可变缓存；单查询返回无缓存。
fn pipeline_evidence_cache_mut(pipeline: &mut SearchPipeline) -> Option<&mut ContextEvidenceCache> {
    match &mut pipeline.follow_up {
        PipelineFollowUp::MemorySearch => None,
        PipelineFollowUp::MemorySearchBatchGroup { evidence_cache, .. } => Some(evidence_cache),
        PipelineFollowUp::AnalyzeReview(search) => Some(&mut search.evidence_cache),
        PipelineFollowUp::PreCheckGroup(pending) => Some(&mut pending.evidence_cache),
        PipelineFollowUp::DirectWriteReview { pending, .. } => Some(&mut pending.evidence_cache),
    }
}

/// Returns only identities not yet successfully observed in this source Search call.
/// 返回本次来源Search尚未成功观察的身份；保留首次出现顺序，成功空观察同样命中缓存。
fn pipeline_missing_evidence_ids(pipeline: &SearchPipeline) -> Vec<MemoryId> {
    let cache = pipeline_evidence_cache(pipeline);
    pipeline_evidence_ids(pipeline)
        .into_iter()
        .filter(|id| cache.is_none_or(|cache| !cache.contains_key(id)))
        .collect()
}

/// Applies one successful incremental observation or skips all context after a failed read without poisoning the cache.
/// 应用一次成功增量观察，或在读取失败后跳过全部情境而不污染缓存；参数None仅表示明确失败，返回下一命令步骤或契约错误。
fn finish_pipeline_evidence(
    record: &mut CommandRecord,
    mut pipeline: SearchPipeline,
    observation: Option<Vec<ContextEvidenceGroup>>,
) -> Result<CommandStep, CoreError> {
    if let Some(groups) = observation {
        let requested_ids = pipeline_missing_evidence_ids(&pipeline);
        // A live observation may be newer than command start; identity and row contracts remain mandatory.
        // 实时观察可以晚于命令开始时间；身份与行契约仍必须成立。
        validate_context_evidence_groups(&requested_ids, &groups)?;
        if let Some(cache) = pipeline_evidence_cache_mut(&mut pipeline) {
            for group in groups {
                cache.insert(group.memory_id, group.evidence);
            }
            let ids = pipeline_evidence_ids(&pipeline);
            let cache = pipeline_evidence_cache(&pipeline).ok_or_else(|| {
                command_error(ErrorCode::Internal, "invocation evidence cache disappeared")
            })?;
            pipeline.work.evidence = ids
                .into_iter()
                .map(|memory_id| {
                    let evidence = cache
                        .get(&memory_id)
                        .ok_or_else(|| {
                            effect_type_error("successful evidence observation is incomplete")
                        })?
                        .clone();
                    Ok(ContextEvidenceGroup {
                        memory_id,
                        evidence,
                    })
                })
                .collect::<Result<_, CoreError>>()?;
        } else {
            pipeline.work.evidence = groups;
        }
    } else {
        // VMM returns the entire query's unadjusted hits if any requested miss fails, even when other IDs are cached.
        // VMM在任一缺失读取失败时返回整条查询未经情境调整的命中，即使其他身份已有缓存。
        pipeline.work.evidence.clear();
    }
    let fused = std::mem::take(&mut pipeline.fused);
    finish_scored_pipeline(record, pipeline, fused)
}

/// Builds fused ordered hits with complete staged score explanations.
/// 构建带完整分阶段评分解释的融合有序命中。
#[allow(clippy::map_unwrap_or)]
fn build_fused_hits(
    work: &SearchWork,
    policy: &vulcan_memorymesh_core_contracts::RetrievalPolicy,
) -> Result<Vec<SearchHit>, CoreError> {
    // Fuse both physical channels with rank-based RRF.
    // 使用基于排名的 RRF 融合两个物理通道。
    let pool_limit =
        retrieval_kernel::candidate_pool_limit(work.limit, work.enable_hard_dedupe_pool, policy);
    // Split retrieval materializes before fusion; missing rows must not consume its ranks or window.
    // 分离检索在融合前物化；缺失行不能占据名次或窗口。组合模式保留物理融合语义，末端仍跳过缺行。
    let filter_before_fusion = !policy.hybrid_enabled
        || policy.rrf_score_mode == vulcan_memorymesh_core_contracts::RrfScoreMode::RankNormalized;
    let available = work
        .records
        .keys()
        .map(MemoryId::as_str)
        .collect::<BTreeSet<_>>();
    let filter_channel = |channel: &[PhysicalCandidate]| {
        channel
            .iter()
            .filter(|candidate| {
                !filter_before_fusion || available.contains(candidate.memory_id.as_str())
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let vector = filter_channel(&work.vector);
    let lexical = filter_channel(&work.lexical);
    let fused = retrieval_kernel::rrf_fusion(
        &vector,
        &lexical,
        policy,
        pool_limit,
        !work.lexical.is_empty(),
    )?;
    let mut hits = Vec::with_capacity(fused.len());
    for (memory_id, mut explanation) in fused {
        let Ok(id) = MemoryId::new(memory_id) else {
            continue;
        };
        let Some(record) = work.records.get(&id) else {
            continue;
        };
        // Gate each channel decision score against its contract-bound threshold.
        // 针对绑定契约的阈值校验每个通道的决策分值。
        if !channel_passes_threshold(&work.vector, &id, policy)?
            && !channel_passes_threshold(&work.lexical, &id, policy)?
        {
            continue;
        }
        let base = explanation
            .base_decision_score
            .ok_or_else(|| effect_type_error("fused business decision score"))?
            .clamp(0.0, 1.0);
        explanation.final_score = base;
        hits.push(SearchHit {
            memory: record.clone(),
            search_score: base,
            rerank_score: None,
            final_score: base,
            explanation: Some(explanation),
        });
    }
    Ok(hits)
}

/// Reports whether one candidate passes the contract-bound threshold inside its channel.
/// 报告一个候选是否通过其通道内绑定契约的阈值。
fn channel_passes_threshold(
    channel: &[PhysicalCandidate],
    memory_id: &MemoryId,
    policy: &vulcan_memorymesh_core_contracts::RetrievalPolicy,
) -> Result<bool, CoreError> {
    if policy.min_similarity.is_empty() {
        return Ok(true);
    }
    let Some(candidate) = channel
        .iter()
        .find(|candidate| candidate.memory_id == memory_id.as_str())
    else {
        return Ok(false);
    };
    // An unbound channel has no physical threshold; rank-only values must not be synthesized.
    // 未绑定阈值的通道无需物理决策分，不能为仅排名通道合成数值。
    if !policy
        .min_similarity
        .iter()
        .any(|binding| binding.contract == candidate.score.contract)
    {
        return Ok(true);
    }
    Ok(retrieval_kernel::passes_threshold(
        candidate.score.require_decision_score()?,
        &candidate.score.contract,
        &policy.min_similarity,
    ))
}

/// Completes the shared retrieval pipeline and dispatches its follow-up.
/// 完成共享检索流水线并分发其后续动作。
#[allow(
    clippy::cast_possible_truncation,
    clippy::redundant_clone,
    clippy::too_many_lines
)]
fn finish_pipeline(
    record: &mut CommandRecord,
    pipeline: SearchPipeline,
    hits: Vec<SearchHit>,
    hard_dedupe_pool: Vec<MemoryRecord>,
) -> Result<CommandStep, CoreError> {
    let effective_policy = effective_retrieval_policy(
        &pipeline.work,
        &record.snapshot.memory_policy.retrieval_policy,
    )?;
    let query_hard_dedupe_threshold = hard_dedupe_threshold(&effective_policy);
    let query_hard_dedupe_contract = effective_policy.vector_contract.contract.clone();
    match pipeline.follow_up {
        PipelineFollowUp::MemorySearch => complete(record, CommandResult::MemorySearch(hits)),
        PipelineFollowUp::MemorySearchBatchGroup {
            evidence_cache,
            command,
            query_plan,
            embeddings,
            group,
            mut completed,
        } => {
            let query = query_plan.unique_queries[group].clone();
            completed.push(MemorySearchGroup {
                query_index: group as u32,
                query,
                hits,
            });
            let next_group = group + 1;
            if next_group == query_plan.unique_queries.len() {
                let groups = command
                    .queries
                    .iter()
                    .enumerate()
                    .map(|(index, query)| MemorySearchGroup {
                        query_index: index as u32,
                        query: query.clone(),
                        hits: completed[query_plan.caller_to_unique[index]].hits.clone(),
                    })
                    .collect();
                return complete(record, CommandResult::MemorySearchBatch(groups));
            }
            // Continue the next group through the same pipeline stages.
            // 通过相同流水线阶段继续下一分组。
            let work = SearchWork {
                subject_id: command.subject_id.clone(),
                scope_access: command.scope_access.clone(),
                queries: vec![query_plan.unique_queries[next_group].clone()],
                embeddings: vec![embeddings[next_group].clone()],
                vector_space_id: command.vector_space_id.clone(),
                boundary: command.boundary.session_recall_boundary.clone(),
                candidate_scope: command.boundary.candidate_scope.clone(),
                limit: command.top_k,
                enable_hard_dedupe_pool: false,
                now_ns: command.now_ns,
                separate_fallback_active: false,
                vector: Vec::new(),
                lexical: Vec::new(),
                records: BTreeMap::new(),
                evidence: Vec::new(),
            };
            record.state = FlowState::SearchPipeline(Box::new(SearchPipeline {
                stage: PipelineStage::Vector,
                work,
                follow_up: PipelineFollowUp::MemorySearchBatchGroup {
                    evidence_cache,
                    command,
                    query_plan,
                    embeddings,
                    group: next_group,
                    completed,
                },
                fused: Vec::new(),
                rerank_indices: Vec::new(),
            }));
            start_pipeline(record)
        }
        PipelineFollowUp::PreCheckGroup(mut pending) => {
            crate::precheck::accumulate_review_group(
                &mut pending.completed,
                hits,
                &record.snapshot.memory_policy.precheck_policy,
            )?;
            pending.group += 1;
            if pending.group == pending.queries.len() {
                advance_pipeline_to_precheck(
                    record,
                    pending.command,
                    pending.content,
                    pending.queries,
                    pending.intent_reason,
                    pending.completed,
                )
            } else {
                start_precheck_query_group(record, *pending)
            }
        }
        PipelineFollowUp::DirectWriteReview {
            pending,
            item_index,
        } => finish_direct_write_candidate(
            record,
            *pending,
            item_index,
            hits,
            hard_dedupe_pool,
            query_hard_dedupe_threshold,
            query_hard_dedupe_contract,
        ),
        PipelineFollowUp::AnalyzeReview(mut search) => {
            search.completed.push(ReviewQueryFacts {
                hits,
                hard_dedupe_pool,
                hard_dedupe_threshold: query_hard_dedupe_threshold,
                hard_dedupe_contract: query_hard_dedupe_contract,
            });
            if search.completed.len() < search.query_plan.unique_queries.len() {
                return start_analyze_candidate_search(record, *search);
            }
            // Complete the one Search invocation before making independent candidate decisions.
            // 先完成一次Search调用，再独立决定各候选；同查询不同类别不能复用硬去重结论。
            let policy = &record.snapshot.memory_policy.retrieval_policy;
            for (index, candidate) in search.pending.memories.iter().enumerate() {
                let group = search.query_plan.caller_to_unique[index];
                let facts = &search.completed[group];
                if let Some(existing) = select_hard_duplicate(
                    &facts.hard_dedupe_pool,
                    candidate.category,
                    &search.embeddings[group],
                    facts.hard_dedupe_threshold,
                ) {
                    search.pending.hard_dropped.push(DroppedCandidate {
                        candidate_index: index as u32,
                        dedupe_memory_id: Some(existing.id.to_string()),
                        reason: Some("local hard cosine duplicate".to_owned()),
                    });
                } else {
                    search.pending.reviewer_to_candidate.push(index as u32);
                }
                search.pending.memory_reviews.push(MemoryReviewCandidate {
                    candidate: candidate.clone(),
                    similar_memories: review_similar_memories(facts.hits.clone(), policy),
                });
            }
            finish_analyze_preparation(record, search.pending)
        }
    }
}

/// Initializes durable `PostAction` intake with deterministic and semantic noise gating.
/// 使用确定性与语义 Noise 门控初始化持久 `PostAction` 登记。
#[allow(clippy::redundant_clone)]
fn initialize_intake(
    record: &mut CommandRecord,
    mut command: PostActionIntakeCommand,
) -> Result<CommandStep, CoreError> {
    // Normalize and scrub the complete timeline under the frozen snapshot.
    // 在冻结快照下规范化并脱敏完整时间线。
    command.timeline = normalize_timeline(&record.snapshot, &command.timeline)?;
    let mut noise = NoiseAudit {
        config_snapshot_id: record.snapshot.id.clone(),
        rule_schema_version: vulcan_memorymesh_core_config::NOISE_RULE_SCHEMA_VERSION.to_owned(),
        rules_digest: record.snapshot.noise_digest.clone(),
        source: if record.snapshot.noise_policy.enabled {
            NoiseDecisionSource::Regex
        } else {
            NoiseDecisionSource::Disabled
        },
        role: None,
        category: None,
        confidence: 0.0,
        degraded: false,
        unavailable_categories: Vec::new(),
    };
    // VMM only gates simple turns; source intermediate messages explicitly bypass Noise.
    // VMM 仅对简单轮次门控；来源中间消息显式跳过 Noise。
    if command.input_kind == vulcan_memorymesh_core_contracts::PostActionInputKind::Timeline {
        noise.source = NoiseDecisionSource::TimelineBypass;
        return emit_intake_turn(record, command, noise);
    }
    // Single-turn shape validation guarantees user then optional assistant ordering.
    // 单轮形态校验保证用户在前、可选助手在后的顺序。
    if let Some((role, decision)) = command.timeline.iter().find_map(|item| {
        let decision = record
            .snapshot
            .noise_detector
            .detect(item.role, &item.content);
        decision.is_noise.then_some((item.role, decision))
    }) {
        noise.role = Some(role);
        noise.category = decision.category;
        noise.confidence = decision.confidence;
        return complete(
            record,
            CommandResult::PostActionAccepted(PostActionAcceptedResult {
                noise,
                turn_id: None,
                ignored_as_noise: true,
                receipt: None,
            }),
        );
    }
    // Semantic noise requires embedding support; query the prototype cache first.
    // 语义 Noise 需要嵌入支持；首先查询原型缓存。
    // Cache all role targets under one rules identity, even when this request lacks an assistant.
    // 在同一规则身份下缓存所有角色目标，即使本次请求没有助手回复。
    let mut phrase_targets = Vec::new();
    let mut seen = BTreeSet::new();
    let mut queries = Vec::new();
    for role in [Role::User, Role::Assistant] {
        let targets = record.snapshot.noise_detector.semantic_phrase_targets(role);
        if !targets.is_empty() {
            for item in command.timeline.iter().filter(|item| item.role == role) {
                queries.push((role, item.content.clone()));
            }
        }
        for target in targets {
            if seen.insert((target.category.clone(), target.phrase.clone())) {
                phrase_targets.push((target.category, target.phrase, target.threshold));
            }
        }
    }
    if queries.is_empty() {
        return emit_intake_turn(record, command, noise);
    }
    let identity = noise_cache_identity(record, &command);
    let mut seen_categories = BTreeSet::new();
    let prototype_categories = phrase_targets
        .iter()
        .filter(|(category, _, _)| seen_categories.insert(category.clone()))
        .map(|(category, _, _)| category.clone())
        .collect();
    let context = IntakeNoiseContext {
        noise,
        prototype_categories,
        prototype_category_index: 0,
        resolved_prototypes: Vec::new(),
        queries,
        phrase_targets,
        command: command.clone(),
    };
    record.state = FlowState::IntakeNoiseQuery(Box::new(context));
    emit_effect(
        record,
        HostEffect::StorageQuery(StorageQuery::NoisePrototypeBundle { identity }),
    )
}

/// Builds the complete semantic noise cache identity for one intake command.
/// 为一个登记命令构建完整的语义 Noise 缓存身份。
fn noise_cache_identity(
    record: &CommandRecord,
    command: &PostActionIntakeCommand,
) -> NoisePrototypeCacheIdentity {
    NoisePrototypeCacheIdentity {
        scope_id: command.scope_access.write_scope.clone(),
        language: record.snapshot.noise_language.clone(),
        model: record.snapshot.vector_space_fingerprint.clone(),
        dimension: record.snapshot.embedding_dimension,
        rules_hash: record.snapshot.noise_digest.clone(),
    }
}

/// Continues intake after one prototype bundle lookup.
/// 在一次原型束查询后继续登记。
fn advance_intake_noise_query(
    record: &mut CommandRecord,
    context: IntakeNoiseContext,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::NoisePrototypes {
        identity,
        prototypes,
    } = expect_storage_query(result)?
    else {
        return Err(effect_type_error("noise prototype storage result"));
    };
    if identity != noise_cache_identity(record, &context.command) {
        return Err(effect_type_error("noise prototype cache identity"));
    }
    if prototypes.is_empty() || validate_noise_prototypes(record, &context, &prototypes).is_err() {
        // A correctly identified but incomplete/stale derived cache is regenerated, never used as policy.
        // 身份正确但不完整或陈旧的派生缓存应重新生成，绝不作为策略使用。
        record.degraded |= !prototypes.is_empty();
        begin_intake_noise_embedding(record, context)
    } else {
        // VMM normalizes persisted prototypes when loading its cache, not when reusing fresh rows.
        // VMM在加载持久缓存时再次归一化原型，不在复用新生成行时重复处理。
        let prototypes: Vec<_> = prototypes
            .into_iter()
            .map(|mut prototype| {
                prototype.vector = normalize_vector(prototype.vector);
                prototype
            })
            .collect();
        let rows = prototypes_to_rows(&context, &prototypes)?;
        advance_intake_semantic(record, context, rows)
    }
}

/// Emits the next category embedding or persists the completed usable prototype subset.
/// 发出下一类别嵌入，或持久化已经完成的可用原型子集。
fn begin_intake_noise_embedding(
    record: &mut CommandRecord,
    mut context: IntakeNoiseContext,
) -> Result<CommandStep, CoreError> {
    if context.prototype_category_index > context.prototype_categories.len() {
        return Err(effect_type_error("Noise prototype category cursor"));
    }
    if let Some(category) = context
        .prototype_categories
        .get(context.prototype_category_index)
    {
        let inputs: Vec<String> = context
            .phrase_targets
            .iter()
            .filter(|(name, _, _)| name == category)
            .map(|(_, phrase, _)| phrase.clone())
            .collect();
        if inputs.is_empty() {
            return Err(effect_type_error(
                "Noise prototype category has no phrase inputs",
            ));
        }
        let vector_space_id = context.command.vector_space_id.clone();
        record.state = FlowState::IntakeNoiseEmbed(Box::new(context));
        return emit_effect(
            record,
            HostEffect::Embedding(EmbeddingEffect {
                vector_space_id,
                allow_partial_invalid_inputs: false,
                expected_dimension: record.snapshot.embedding_dimension,
                inputs,
            }),
        );
    }
    let prototypes = std::mem::take(&mut context.resolved_prototypes);
    if prototypes.is_empty() {
        context.noise.source = NoiseDecisionSource::RegexOnlyFallback;
        return emit_intake_turn(record, context.command, context.noise);
    }
    validate_noise_prototypes(record, &context, &prototypes)?;
    let identity = noise_cache_identity(record, &context.command);
    let batch = record.mutation_batch(
        "noise-cache",
        vec![Mutation::PutNoisePrototypeBundle {
            identity,
            prototypes: prototypes.clone(),
        }],
    )?;
    record.state = FlowState::IntakeNoiseStore {
        context: Box::new(context),
        prototypes,
    };
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Builds prototype rows from cached vectors with thresholds aligned to phrase targets.
/// 从缓存向量构建原型行，并将阈值与短语目标对齐。
fn prototypes_to_rows(
    context: &IntakeNoiseContext,
    prototypes: &[NoisePrototype],
) -> Result<Vec<vulcan_memorymesh_core_config::NoisePrototypeRow>, CoreError> {
    let thresholds = context
        .phrase_targets
        .iter()
        .map(|(category, phrase, threshold)| ((category.clone(), phrase.clone()), *threshold))
        .collect::<std::collections::BTreeMap<_, _>>();
    prototypes
        .iter()
        .map(|row| {
            let threshold = thresholds
                .get(&(row.category.clone(), row.phrase.clone()))
                .copied()
                .ok_or_else(|| effect_type_error("noise prototype phrase identity"))?;
            Ok(vulcan_memorymesh_core_config::NoisePrototypeRow {
                category: row.category.clone(),
                phrase: row.phrase.clone(),
                vector: row.vector.clone(),
                threshold,
            })
        })
        .collect()
}

/// Validates cached semantic-noise prototypes against the frozen rule and embedding boundary.
/// 根据冻结的规则与嵌入边界校验缓存的语义 Noise 原型。
fn validate_noise_prototypes(
    record: &CommandRecord,
    context: &IntakeNoiseContext,
    prototypes: &[NoisePrototype],
) -> Result<(), CoreError> {
    if prototypes.len() != context.phrase_targets.len() {
        return Err(effect_type_error("noise prototype row count"));
    }
    let expected = context
        .phrase_targets
        .iter()
        .map(|(category, phrase, _)| (category.clone(), phrase.clone()))
        .collect::<BTreeSet<_>>();
    let expected_dimension = usize::try_from(record.snapshot.embedding_dimension)
        .map_err(|_| effect_type_error("noise prototype dimension"))?;
    let mut actual = BTreeSet::new();
    for prototype in prototypes {
        if !actual.insert((prototype.category.clone(), prototype.phrase.clone()))
            || prototype.vector.len() != expected_dimension
            || prototype.vector.iter().any(|value| !value.is_finite())
        {
            return Err(effect_type_error(
                "noise prototype identity, dimension, or vector values",
            ));
        }
    }
    if actual != expected {
        return Err(effect_type_error("noise prototype phrase set"));
    }
    Ok(())
}

/// Continues intake after one phrase-prototype embedding batch.
/// 在一次短语原型嵌入批次后继续登记。
fn advance_intake_noise_embed(
    record: &mut CommandRecord,
    mut context: IntakeNoiseContext,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let category = context
        .prototype_categories
        .get(context.prototype_category_index)
        .ok_or_else(|| effect_type_error("pending Noise category"))?;
    let targets: Vec<_> = context
        .phrase_targets
        .iter()
        .filter(|(name, _, _)| name == category)
        .collect();
    let vectors = expect_embeddings(result, targets.len(), record.snapshot.embedding_dimension)?;
    // Build L2-normalized prototype rows from the host embedding output.
    // 从宿主嵌入输出构建 L2 规范化原型行。
    let prototypes: Vec<NoisePrototype> = targets
        .into_iter()
        .zip(vectors)
        .map(|((category, phrase, _), vector)| NoisePrototype {
            category: category.clone(),
            phrase: phrase.clone(),
            vector: normalize_vector(vector),
        })
        .collect();
    context.resolved_prototypes.extend(prototypes);
    context.prototype_category_index += 1;
    begin_intake_noise_embedding(record, context)
}

/// Continues intake after one prototype bundle cache write.
/// 在一次原型束缓存写入后继续登记。
#[allow(clippy::needless_pass_by_value)]
fn advance_intake_noise_store(
    record: &mut CommandRecord,
    context: IntakeNoiseContext,
    prototypes: Vec<NoisePrototype>,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    validate_noise_prototypes(record, &context, &prototypes)?;
    let rows = prototypes_to_rows(&context, &prototypes)?;
    match expect_mutation(result)? {
        MutationOutcome::Committed { receipt, .. } => {
            validate_commit_receipt_identity(record, &receipt)?;
            advance_intake_semantic(record, context, rows)
        }
        MutationOutcome::Rejected { .. } => {
            // A definitive cache rejection is not a rejected turn; use the fresh local prototypes.
            // 确定的缓存拒绝不是轮次拒绝；继续使用新生成的本地原型。
            record.degraded = true;
            advance_intake_semantic(record, context, rows)
        }
        MutationOutcome::Uncertain => {
            let idempotency_key =
                record
                    .pending_mutation_idempotency_key
                    .clone()
                    .ok_or_else(|| {
                        command_error(
                            ErrorCode::Internal,
                            "pending mutation idempotency key is missing",
                        )
                    })?;
            mark_uncertain(record, idempotency_key)
        }
    }
}

/// Continues intake with one turn-text embedding for semantic scoring.
/// 使用一次轮次文本嵌入继续登记以进行语义评分。
#[allow(clippy::needless_pass_by_value)]
fn advance_intake_semantic(
    record: &mut CommandRecord,
    mut context: IntakeNoiseContext,
    rows: Vec<vulcan_memorymesh_core_config::NoisePrototypeRow>,
) -> Result<CommandStep, CoreError> {
    // Only query roles that still have at least one successfully embedded category.
    // 仅查询仍至少拥有一个成功嵌入类别的角色。
    context.queries.retain(|(role, _)| {
        record
            .snapshot
            .noise_detector
            .semantic_phrase_targets(*role)
            .iter()
            .any(|target| {
                rows.iter()
                    .any(|row| row.category == target.category && row.phrase == target.phrase)
            })
    });
    if context.queries.is_empty() {
        context.noise.source = NoiseDecisionSource::RegexOnlyFallback;
        return emit_intake_turn(record, context.command, context.noise);
    }
    let vector_space_id = context.command.vector_space_id.clone();
    let inputs = context
        .queries
        .iter()
        .map(|(_, text)| text.clone())
        .collect();
    let prototypes: Vec<NoisePrototype> = rows
        .iter()
        .map(|row| NoisePrototype {
            category: row.category.clone(),
            phrase: row.phrase.clone(),
            vector: row.vector.clone(),
        })
        .collect();
    record.state = FlowState::IntakeNoiseScore {
        context: Box::new(context),
        prototypes,
    };
    emit_effect(
        record,
        HostEffect::Embedding(EmbeddingEffect {
            vector_space_id,
            allow_partial_invalid_inputs: false,
            expected_dimension: record.snapshot.embedding_dimension,
            inputs,
        }),
    )
}

/// Completes intake noise gating and emits the pending turn mutation or noise rejection.
/// 完成登记 Noise 门控并发出待处理轮次变更或噪声拒绝。
#[allow(clippy::needless_pass_by_value)]
fn advance_intake_noise_score(
    record: &mut CommandRecord,
    mut context: IntakeNoiseContext,
    prototypes: Vec<NoisePrototype>,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let vectors = expect_embeddings(
        result,
        context.queries.len(),
        record.snapshot.embedding_dimension,
    )?;
    let rows = prototypes_to_rows(&context, &prototypes)?;
    context.noise.source = NoiseDecisionSource::Semantic;
    context.noise.degraded |= record.degraded;
    let mut ignored = false;
    for ((role, _), vector) in context.queries.iter().zip(vectors) {
        // Normalize query observations exactly once; restored prototype state is already normalized.
        // 查询观察只归一化一次；恢复后的原型状态已经完成归一化。
        let vector = normalize_vector(vector);
        let decision = record
            .snapshot
            .noise_detector
            .semantic_decision(*role, &vector, &rows);
        context.noise.role = Some(*role);
        context.noise.category = decision.category;
        context.noise.confidence = decision.confidence;
        if decision.is_noise {
            ignored = true;
            break;
        }
    }
    if ignored {
        return complete(
            record,
            CommandResult::PostActionAccepted(PostActionAcceptedResult {
                noise: context.noise,
                turn_id: None,
                ignored_as_noise: true,
                receipt: None,
            }),
        );
    }
    emit_intake_turn(record, context.command, context.noise)
}

/// Emits one durable pending-turn intake mutation with a command-local slot.
/// 发出一次带命令局部槽位的持久待处理轮次登记变更。
#[allow(clippy::needless_pass_by_value)]
fn emit_intake_turn(
    record: &mut CommandRecord,
    command: PostActionIntakeCommand,
    mut noise: NoiseAudit,
) -> Result<CommandStep, CoreError> {
    let turn_slot = EntitySlotId::new("pending-turn")
        .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
    let turn = Turn {
        id: TurnId::new("__pending__")
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?,
        subject_id: command.subject_id.clone(),
        scope_id: command.scope_access.write_scope.clone(),
        session_source_id: command.session_source_id.clone(),
        timeline: command.timeline.clone(),
        previous_turn_ids: Vec::new(),
        next_turn_ids: Vec::new(),
        created_at_ns: command.completed_at_ns,
        status: TurnStatus::Pending,
        user_input_kind: None,
        analysis_details: None,
    };
    let batch = record.mutation_batch(
        "intake",
        vec![Mutation::PutTurn {
            entity: EntityRef::New(turn_slot.clone()),
            turn,
        }],
    )?;
    noise.degraded |= record.degraded;
    record.state = FlowState::IntakeTurn { turn_slot, noise };
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Initializes asynchronous analysis by first persisting the analysis attempt identity.
/// 通过首先持久化分析尝试身份来初始化异步分析。
fn initialize_analyze(
    record: &mut CommandRecord,
    command: PostActionAnalyzePendingTurnCommand,
) -> Result<CommandStep, CoreError> {
    // The first external effect persists the analysis attempt identity before any model or
    // embedding work, which is the authoritative crash-recovery anchor for this generation.
    // 首个外部 Effect 在任何模型或嵌入工作前持久化分析尝试身份，这是该代次的权威崩溃恢复锚点。
    let attempt = AnalysisAttemptIdentity {
        turn_id: command.turn_id.clone(),
        analysis_generation: command.analysis_generation,
        mutation_identity: record.mutation_identity.clone().ok_or_else(|| {
            command_error(
                ErrorCode::Internal,
                "analyze command lost its host operation identity",
            )
        })?,
        config_snapshot_id: record.handle.config_snapshot_id.clone(),
        structured_output_contracts: vec![
            StructuredOutputContractId::PostActionAnalysis,
            StructuredOutputContractId::PostActionReview,
        ],
        // Bind the attempt to the configured retrieval contracts so downstream score
        // validation compares against the same identities the host actually used.
        // 将尝试绑定到配置的检索契约，使下游评分校验与实际使用的宿主身份一致。
        score_contracts: {
            let retrieval = &record.snapshot.memory_policy.retrieval_policy;
            vec![
                retrieval.vector_contract.contract.clone(),
                retrieval.lexical_contract.contract.clone(),
            ]
        },
    };
    let batch = record.mutation_batch("attempt", vec![Mutation::PutAnalysisAttempt(attempt)])?;
    record.state = FlowState::AnalyzeAttempt { command };
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Continues analysis after one attempt-identity persistence by loading the pending turn.
/// 在一次尝试身份持久化后通过加载待处理轮次继续分析。
#[allow(clippy::needless_pass_by_value, clippy::redundant_clone)]
fn advance_analyze_attempt(
    record: &mut CommandRecord,
    command: PostActionAnalyzePendingTurnCommand,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    match expect_mutation(result)? {
        MutationOutcome::Committed { receipt, .. } => {
            validate_commit_receipt_identity(record, &receipt)?;
            record.state = FlowState::AnalyzeLoadTurn {
                command: command.clone(),
            };
            let policy = &record.snapshot.memory_policy;
            emit_effect(
                record,
                HostEffect::StorageQuery(StorageQuery::PostActionContext {
                    subject_id: command.subject_id.clone(),
                    scope_access: command.scope_access.clone(),
                    session_source_id: command.session_source_id.clone(),
                    target_turn_id: command.turn_id.clone(),
                    reference_turn_limit: policy.postaction_history_turn_limit,
                    direct_write_observed_after_ns: command.direct_write_observed_after_ns,
                    analysis_cutoff_ns: command.analysis_cutoff_ns,
                }),
            )
        }
        MutationOutcome::Rejected { reason } => Err(CoreError::new(
            ErrorCode::StorageConflict,
            ErrorStage::Storage,
            reason,
        )),
        MutationOutcome::Uncertain => {
            let idempotency_key =
                record
                    .pending_mutation_idempotency_key
                    .clone()
                    .ok_or_else(|| {
                        command_error(
                            ErrorCode::Internal,
                            "pending mutation idempotency key is missing",
                        )
                    })?;
            mark_uncertain(record, idempotency_key)
        }
    }
}

/// Continues analysis after one pending-turn load with the authoritative L1 prompt.
/// 在一次待处理轮次加载后使用权威 L1 Prompt 继续分析。
#[allow(clippy::needless_pass_by_value, clippy::redundant_clone)]
fn advance_analyze_load_turn(
    record: &mut CommandRecord,
    command: PostActionAnalyzePendingTurnCommand,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::PostActionContext {
        target_turn,
        reference_turns,
        recent_direct_writes,
    } = expect_storage_query(result)?
    else {
        return Err(effect_type_error("analyze pending turn context result"));
    };
    let loaded = *target_turn;
    if loaded.id != command.turn_id || loaded.status != TurnStatus::Pending {
        return Err(effect_type_error("analyze requires one pending turn"));
    }
    validate_postaction_context(
        &command,
        &loaded,
        &reference_turns,
        &recent_direct_writes,
        record.snapshot.memory_policy.postaction_history_turn_limit,
    )?;
    let reference_turns = trim_postaction_history(
        &loaded,
        reference_turns,
        record.snapshot.memory_policy.postaction_max_input_tokens,
    );
    // Load the authoritative L1 analysis prompt and emit the analysis effect.
    // 加载权威 L1 分析 Prompt 并发出分析 Effect。
    let prompt = record.snapshot.prompt("postaction_l1_main.md")?.to_owned();
    record.state = FlowState::AnalyzeAnalysis {
        command: command.clone(),
        turn: loaded.clone(),
    };
    emit_llm_effect(
        record,
        LlmPurpose::PostActionAnalysis,
        prompt,
        LlmInput::PostActionAnalysis {
            current_timestamp_ns: command.analysis_cutoff_ns,
            reference_turns,
            target_turn: Box::new(loaded.clone()),
            recent_direct_writes,
        },
    )
}

/// Validates all host-supplied `PostAction` context facts against the frozen command boundary.
/// 根据冻结的命令边界校验宿主提供的全部 `PostAction` 上下文事实。
fn validate_postaction_context(
    command: &PostActionAnalyzePendingTurnCommand,
    target: &Turn,
    references: &[PostActionReferenceTurn],
    direct_writes: &[PostActionDirectWrite],
    reference_turn_limit: u32,
) -> Result<(), CoreError> {
    validate_host_turn(target)?;
    if target.subject_id != command.subject_id
        || target.scope_id != command.scope_access.write_scope
        || target.session_source_id != command.session_source_id
        || target.created_at_ns > command.analysis_cutoff_ns
        || target.status != TurnStatus::Pending
        || target.user_input_kind.is_some()
        || target.analysis_details.is_some()
    {
        return Err(effect_type_error(
            "PostAction target violates subject, scope, session, timeline, or cutoff bounds",
        ));
    }
    if references.len() > usize::try_from(reference_turn_limit).unwrap_or(usize::MAX) {
        return Err(effect_type_error(
            "PostAction reference turn count exceeds the requested bound",
        ));
    }
    let mut reference_ids = BTreeSet::new();
    let mut previous_order: Option<(i64, &TurnId)> = None;
    let target_turn_id = &target.id;
    for reference in references {
        let order = (reference.created_at_ns, &reference.turn_id);
        if &reference.turn_id == target_turn_id
            || reference.subject_id != command.subject_id
            || reference.scope_id != command.scope_access.write_scope
            || reference.session_source_id != command.session_source_id
            || invalid_required_host_text(&reference.details, MAX_DOMAIN_TEXT_BYTES)
            || reference.created_at_ns > target.created_at_ns
            || !reference_ids.insert(reference.turn_id.clone())
            || previous_order.is_some_and(|previous| previous >= order)
        {
            return Err(effect_type_error(
                "PostAction reference facts violate identity, details, time, or order bounds",
            ));
        }
        previous_order = Some(order);
    }
    if direct_writes.len() > MAX_DOMAIN_COLLECTION_ITEMS {
        return Err(effect_type_error(
            "PostAction direct-write facts exceed the public collection bound",
        ));
    }
    let visible_scopes = command
        .scope_access
        .visible_scopes
        .iter()
        .collect::<BTreeSet<_>>();
    let mut direct_ids = BTreeSet::new();
    let mut previous_direct_order: Option<(i64, &MemoryId)> = None;
    for direct in direct_writes {
        let order = (direct.created_at_ns, &direct.memory_id);
        if direct.session_source_id != command.session_source_id
            || !visible_scopes.contains(&direct.scope_id)
            || invalid_required_host_text(&direct.summary, MAX_DOMAIN_TEXT_BYTES)
            || invalid_required_host_text(&direct.content, MAX_DOMAIN_TEXT_BYTES)
            || direct.created_at_ns <= command.direct_write_observed_after_ns
            || direct.created_at_ns > command.analysis_cutoff_ns
            || !direct_ids.insert(direct.memory_id.clone())
            || previous_direct_order.is_some_and(|previous| previous >= order)
        {
            return Err(effect_type_error(
                "PostAction direct-write facts violate session, scope, content, time, or order bounds",
            ));
        }
        previous_direct_order = Some(order);
    }
    Ok(())
}

/// Trims refined history with the VMM-compatible target-first heuristic token budget.
/// 使用与 VMM 兼容的目标优先启发式 Token 预算裁剪已提炼历史。
fn trim_postaction_history(
    target: &Turn,
    references: Vec<PostActionReferenceTurn>,
    max_tokens: u32,
) -> Vec<PostActionReferenceTurn> {
    if max_tokens == 0 {
        return references;
    }
    let target_budget = target
        .timeline
        .iter()
        .map(|item| estimate_postaction_tokens(&item.content))
        .sum::<u32>();
    let mut remaining = max_tokens.saturating_sub(target_budget);
    let mut selected = Vec::new();
    for reference in references.into_iter().rev() {
        let budget = estimate_postaction_tokens(&reference.details);
        if !selected.is_empty() && budget > remaining {
            break;
        }
        remaining = remaining.saturating_sub(budget);
        selected.push(reference);
    }
    selected.reverse();
    selected
}

/// Estimates text tokens with the frozen VMM domestic-language coefficient profile.
/// 使用冻结的 VMM 国内语言系数配置估算文本 Token 数。
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn estimate_postaction_tokens(text: &str) -> u32 {
    let mut total = 0.0_f64;
    let mut latin_len = 0_u32;
    let mut latin_transitions = 0_u32;
    let mut previous_latin: Option<char> = None;
    let flush_latin = |total: &mut f64,
                       latin_len: &mut u32,
                       latin_transitions: &mut u32,
                       previous_latin: &mut Option<char>| {
        if *latin_len > 0 {
            *total += 0.88;
            if *latin_len > 5 {
                *total += f64::from(*latin_len - 5) / 4.5;
            }
            *total = f64::from(*latin_transitions).mul_add(0.10, *total);
            *latin_len = 0;
            *latin_transitions = 0;
            *previous_latin = None;
        }
    };
    for character in text.chars() {
        if character.is_ascii_alphabetic() {
            if previous_latin.is_some_and(char::is_lowercase) && character.is_uppercase() {
                latin_transitions = latin_transitions.saturating_add(1);
            }
            latin_len = latin_len.saturating_add(1);
            previous_latin = Some(character);
            continue;
        }
        flush_latin(
            &mut total,
            &mut latin_len,
            &mut latin_transitions,
            &mut previous_latin,
        );
        total += if is_cjk_character(character) {
            1.18
        } else if character.is_numeric() {
            0.50
        } else if character.is_whitespace() {
            0.08
        } else if character.is_alphabetic() {
            1.10
        } else {
            0.82
        };
    }
    flush_latin(
        &mut total,
        &mut latin_len,
        &mut latin_transitions,
        &mut previous_latin,
    );
    if total == 0.0 {
        0
    } else {
        (total * 1.06).ceil().min(f64::from(u32::MAX)) as u32
    }
}

/// Reports whether one Unicode scalar belongs to the CJK families used by VMM.
/// 报告一个 Unicode 标量是否属于 VMM 使用的 CJK 字符族。
const fn is_cjk_character(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x3040..=0x30FF
            | 0xAC00..=0xD7AF
    )
}

/// Continues analysis from L1 candidates into one embedding batch for similarity recall.
/// 将分析从 L1 候选推进到一次用于相似召回的嵌入批次。
#[allow(clippy::cast_possible_truncation, clippy::redundant_clone)]
#[allow(clippy::too_many_lines)]
fn advance_analyze_analysis(
    record: &mut CommandRecord,
    command: PostActionAnalyzePendingTurnCommand,
    turn: Turn,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    match expect_llm(result)? {
        LlmOutput::PostActionAnalysis {
            user_input_kind,
            turn_id,
            details,
            memories: candidates,
            profiles: profile_candidates,
            context_evidence: evidence_candidates,
        } => {
            // Enforce the closed contract on the decoded output before any side effect.
            // 在任何副作用前对解码输出强制执行封闭契约。
            vulcan_memorymesh_core_contracts::validate_llm_output(
                LlmPurpose::PostActionAnalysis,
                &LlmOutput::PostActionAnalysis {
                    user_input_kind,
                    turn_id: turn_id.clone(),
                    details: details.clone(),
                    memories: candidates.clone(),
                    profiles: profile_candidates.clone(),
                    context_evidence: evidence_candidates.clone(),
                },
            )?;
            if turn_id != turn.id {
                return Err(effect_type_error(
                    "PostAction analysis turn identity does not match the pending turn",
                ));
            }
            // Validate all observations before filtering admission, then count each normalized
            // candidate/key/value/relation tuple only once, as the source analyzer does.
            // 在准入过滤前校验全部观测，按来源分析器语义让每个规范化候选/键/值/关系仅计一次。
            let evidence_candidates = normalize_analysis_evidence(evidence_candidates)?;
            // Keep only admitted candidates and omit evidence bound to dropped candidates.
            // 仅保留已准入候选，并移除绑定到被丢弃候选的证据。
            let mut admitted = Vec::new();
            let mut admitted_indexes = BTreeMap::new();
            for (index, candidate) in candidates.into_iter().enumerate() {
                if candidate.admission == Admission::Accept {
                    admitted_indexes.insert(index as u32, admitted.len() as u32);
                    admitted.push(candidate);
                }
            }
            // Remap evidence rows from the original candidate space into the compressed
            // admitted space so later slot binding and result assembly stay aligned even
            // when dropped candidates precede accepted ones.
            // 将证据行从原始候选空间重映射到压缩准入空间，使后续槽位绑定与结果组装
            // 在存在前置被丢弃候选时仍保持对齐。
            let evidence = evidence_candidates
                .into_iter()
                .filter_map(|mut row| {
                    admitted_indexes
                        .get(&row.candidate_index)
                        .map(|compressed| {
                            row.candidate_index = *compressed;
                            row
                        })
                })
                .collect();
            let admitted_profiles = profile_candidates
                .iter()
                .filter(|candidate| candidate.admission == Admission::Accept)
                .cloned()
                .collect::<Vec<_>>();
            let pending = pending_analyze_l1(
                command,
                turn,
                user_input_kind,
                details.trim().to_owned(),
                admitted,
                admitted_profiles,
                evidence,
            );
            begin_analyze_profile_query(record, pending)
        }
        _ => Err(effect_type_error("PostAction analysis LLM output")),
    }
}

/// Continues analysis from candidate embeddings into the shared retrieval pipeline.
/// 将分析从候选嵌入推进到共享检索流水线。
#[allow(clippy::too_many_arguments)]
fn advance_analyze_embedding(
    record: &mut CommandRecord,
    pending: PendingAnalyze,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let query_plan = analyze_query_plan(&pending);
    let embeddings = expect_embeddings(
        result,
        query_plan.unique_queries.len(),
        record.snapshot.embedding_dimension,
    )?;
    start_analyze_candidate_search(
        record,
        PendingAnalyzeSearch {
            pending,
            query_plan,
            embeddings,
            completed: Vec::new(),
            evidence_cache: ContextEvidenceCache::new(),
        },
    )
}

/// Constructs the immutable query mapping from admitted candidates, without using candidate categories as query keys.
/// 从准入候选构造不可变查询映射，不把类别用作查询键；输入L1续体，返回首代表和候选映射。
fn analyze_query_plan(pending: &PendingAnalyze) -> QueryBatchPlan {
    QueryBatchPlan::new(
        &pending
            .memories
            .iter()
            .map(|candidate| memory_review_query(&candidate.summary, &candidate.content))
            .collect::<Vec<_>>(),
    )
}

/// Begins the source Search after profile observation and validates the original query budget before deduplication.
/// 在画像观察后开始来源Search，并在去重前校验原查询预算；输入L1续体，返回嵌入、评审或明确回退步骤。
fn begin_analyze_candidate_queries(
    record: &mut CommandRecord,
    pending: PendingAnalyze,
) -> Result<CommandStep, CoreError> {
    if pending.memories.len()
        > record
            .snapshot
            .memory_policy
            .retrieval_policy
            .max_query_items as usize
    {
        return begin_analyze_l1_fallback(record, pending);
    }
    if pending.memories.is_empty() {
        return finish_analyze_preparation(record, pending);
    }
    let effect = EmbeddingEffect {
        allow_partial_invalid_inputs: false,
        vector_space_id: pending.command.vector_space_id.clone(),
        expected_dimension: record.snapshot.embedding_dimension,
        inputs: analyze_query_plan(&pending).unique_queries,
    };
    record.state = FlowState::AnalyzeEmbedding(Box::new(pending));
    emit_effect(record, HostEffect::Embedding(effect))
}

/// Builds one unique Analyze query work item, shared by execution and checkpoint validation.
/// 构建一条唯一Analyze查询工作项，供执行和恢复校验共用；输入查询续体及快照，返回精确检索工作或错误。
fn analyze_query_work(
    search: &PendingAnalyzeSearch,
    snapshot: &ConfigSnapshot,
) -> Result<SearchWork, CoreError> {
    let index = search.completed.len();
    let command = &search.pending.command;
    Ok(SearchWork {
        subject_id: command.subject_id.clone(),
        scope_access: command.scope_access.clone(),
        queries: vec![
            search
                .query_plan
                .unique_queries
                .get(index)
                .cloned()
                .ok_or_else(|| effect_type_error("Analyze query cursor"))?,
        ],
        embeddings: vec![
            search
                .embeddings
                .get(index)
                .cloned()
                .ok_or_else(|| effect_type_error("Analyze unique embedding cursor"))?,
        ],
        vector_space_id: command.vector_space_id.clone(),
        boundary: None,
        candidate_scope: Some(command.replacement_scope.candidates.clone()),
        limit: snapshot.memory_policy.retrieval_policy.replacement_top_k,
        enable_hard_dedupe_pool: true,
        now_ns: command.analysis_cutoff_ns,
        separate_fallback_active: false,
        vector: Vec::new(),
        lexical: Vec::new(),
        records: BTreeMap::new(),
        evidence: Vec::new(),
    })
}

/// Runs the next unique query while retaining completed facts and the pre-query profile snapshot.
/// 保留已完成事实及查询前画像快照，执行下一条唯一查询；返回首个物理效果或错误。
fn start_analyze_candidate_search(
    record: &mut CommandRecord,
    search: PendingAnalyzeSearch,
) -> Result<CommandStep, CoreError> {
    let work = analyze_query_work(&search, &record.snapshot)?;
    record.state = FlowState::SearchPipeline(Box::new(SearchPipeline {
        stage: PipelineStage::Vector,
        work,
        follow_up: PipelineFollowUp::AnalyzeReview(Box::new(search)),
        fused: Vec::new(),
        rerank_indices: Vec::new(),
    }));
    start_pipeline(record)
}

/// Queries the exact visible profile-node set before the unified `PostAction` L2 review.
/// 在统一 `PostAction` L2 审核前查询精确可见画像节点集合。
fn begin_analyze_profile_query(
    record: &mut CommandRecord,
    pending: PendingAnalyze,
) -> Result<CommandStep, CoreError> {
    if pending.profiles.is_empty() {
        return begin_analyze_candidate_queries(record, pending);
    }
    let effect = StorageQuery::ProfileNodes {
        subject_id: pending.command.subject_id.clone(),
        scope_access: pending.command.scope_access.clone(),
        sections: pending.command.profile_sections.clone(),
        evaluated_at_ns: pending.command.analysis_cutoff_ns,
    };
    record.state = FlowState::AnalyzeProfileQuery(Box::new(pending));
    emit_effect(record, HostEffect::StorageQuery(effect))
}

/// Enters L2 only after all unique query facts have been bound independently to admitted candidates.
/// 仅在全部唯一查询事实独立绑定到准入候选后进入L2；全部硬去重且无画像时直接持久化明确结果。
fn finish_analyze_preparation(
    record: &mut CommandRecord,
    mut pending: PendingAnalyze,
) -> Result<CommandStep, CoreError> {
    if pending.reviewer_to_candidate.is_empty() && pending.profiles.is_empty() {
        pending.review = LlmOutput::PostActionReview {
            accepted_candidates: Vec::new(),
            dropped_candidates: pending.hard_dropped.clone(),
            accepted_profile_candidates: Vec::new(),
            invalid_profile_candidate_indexes: Vec::new(),
            retired_profile_nodes: Vec::new(),
            reason: Some("all candidates resolved before L2".to_owned()),
        };
        return begin_analyze_persistence_embedding(record, pending);
    }
    emit_analyze_review(record, pending)
}

/// Continues the unified `PostAction` review from exact visible profile-node facts.
/// 从精确可见画像节点事实继续统一 `PostAction` 审核。
fn advance_analyze_profile_query(
    record: &mut CommandRecord,
    mut pending: PendingAnalyze,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::ProfileNodes(nodes) = expect_storage_query(result)? else {
        return Err(effect_type_error("Analyze profile-node storage result"));
    };
    if nodes.len() > MAX_DOMAIN_COLLECTION_ITEMS {
        return Err(effect_type_error(
            "Analyze profile-node facts exceed the public collection bound",
        ));
    }
    let nodes = validate_and_filter_profile_nodes(
        nodes,
        &pending.command.subject_id,
        &pending.command.profile_sections,
        pending.command.analysis_cutoff_ns,
    )?;
    pending.current_profile_nodes.clone_from(&nodes);
    begin_analyze_candidate_queries(record, pending)
}

/// Emits L2 from the explicitly prepared candidate and profile facts, without inventing a profile read.
/// 从明确准备的候选及画像事实发出L2，不伪造画像读取；参数为评审续体，返回模型效果。
fn emit_analyze_review(
    record: &mut CommandRecord,
    pending: PendingAnalyze,
) -> Result<CommandStep, CoreError> {
    let prompt = record.snapshot.prompt("postaction_l2_main.md")?.to_owned();
    record.state = FlowState::AnalyzeReview(Box::new(pending.clone()));
    emit_llm_effect(
        record,
        LlmPurpose::PostActionReview,
        prompt,
        LlmInput::PostActionReview {
            context: vulcan_memorymesh_core_contracts::PostActionReviewContext::Turn {
                target_turn: Box::new(pending.turn),
                user_input_kind: pending.user_input_kind,
                profile_sections: pending.command.profile_sections,
            },
            memory_candidates: pending
                .reviewer_to_candidate
                .iter()
                .map(|index| pending.memory_reviews[*index as usize].clone())
                .collect(),
            profile_candidates: pending.profiles,
            current_profile_nodes: pending.current_profile_nodes,
        },
    )
}

/// Continues analysis from fused similarity hits to the L2 review.
/// 将分析从融合相似命中推进到 L2 审核。
fn advance_analyze_review(
    record: &mut CommandRecord,
    pending: PendingAnalyze,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    match expect_llm(result)? {
        mut review @ LlmOutput::PostActionReview { .. } => {
            vulcan_memorymesh_core_contracts::validate_llm_output(
                LlmPurpose::PostActionReview,
                &review,
            )?;
            remap_analyze_review(&pending, &mut review)?;
            validate_postaction_review_inputs(&pending, &review)?;
            let mut pending = pending;
            pending.review = review;
            begin_analyze_persistence_embedding(record, pending)
        }
        _ => Err(effect_type_error("PostAction review LLM output")),
    }
}

/// Restores admitted-candidate coordinates after L2 reviewed only the non-hard-duplicate subset.
/// 在 L2 仅审核非硬重复子集后恢复准入候选坐标。
fn remap_analyze_review(pending: &PendingAnalyze, review: &mut LlmOutput) -> Result<(), CoreError> {
    let LlmOutput::PostActionReview {
        accepted_candidates,
        dropped_candidates,
        ..
    } = review
    else {
        return Err(effect_type_error("analyze reviewer output"));
    };
    let remap = |index: u32| {
        pending
            .reviewer_to_candidate
            .get(index as usize)
            .copied()
            .ok_or_else(|| effect_type_error("analyze reviewer index outside supplied subset"))
    };
    for accepted in accepted_candidates.iter_mut() {
        accepted.candidate_index = remap(accepted.candidate_index)?;
    }
    for dropped in dropped_candidates.iter_mut() {
        dropped.candidate_index = remap(dropped.candidate_index)?;
    }
    dropped_candidates.extend(pending.hard_dropped.clone());
    accepted_candidates.sort_by_key(|accepted| accepted.candidate_index);
    dropped_candidates.sort_by_key(|dropped| dropped.candidate_index);
    Ok(())
}

/// Validates review indexes and exact durable references against the supplied L2 facts.
/// 根据提供给 L2 的事实校验审核索引与精确持久引用。
#[allow(clippy::too_many_lines)]
fn validate_postaction_review_inputs(
    pending: &PendingAnalyze,
    review: &LlmOutput,
) -> Result<(), CoreError> {
    let LlmOutput::PostActionReview {
        accepted_candidates,
        dropped_candidates,
        accepted_profile_candidates,
        invalid_profile_candidate_indexes,
        retired_profile_nodes,
        ..
    } = review
    else {
        return Err(effect_type_error("PostAction review output"));
    };
    let memory_coverage = accepted_candidates
        .iter()
        .map(|item| item.candidate_index)
        .chain(dropped_candidates.iter().map(|item| item.candidate_index))
        .collect::<BTreeSet<_>>();
    if accepted_candidates.len() + dropped_candidates.len() != pending.memories.len()
        || memory_coverage.len() != pending.memories.len()
        || memory_coverage
            .iter()
            .any(|index| (*index as usize) >= pending.memories.len())
    {
        return Err(effect_type_error(
            "PostAction review must cover every memory candidate exactly once",
        ));
    }
    let profile_coverage = accepted_profile_candidates
        .iter()
        .map(|item| item.candidate_index)
        .chain(invalid_profile_candidate_indexes.iter().copied())
        .collect::<BTreeSet<_>>();
    if accepted_profile_candidates.len() + invalid_profile_candidate_indexes.len()
        != pending.profiles.len()
        || profile_coverage.len() != pending.profiles.len()
        || profile_coverage
            .iter()
            .any(|index| (*index as usize) >= pending.profiles.len())
    {
        return Err(effect_type_error(
            "PostAction review must cover every profile candidate exactly once",
        ));
    }
    let active_by_id = pending
        .current_profile_nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let mut superseded_memories = BTreeSet::new();
    for accepted in accepted_candidates {
        let similar_by_id = pending
            .memory_reviews
            .get(accepted.candidate_index as usize)
            .ok_or_else(|| effect_type_error("PostAction memory candidate index"))?
            .similar_memories
            .iter()
            .map(|hit| (hit.memory.id.as_str(), &hit.memory))
            .collect::<BTreeMap<_, _>>();
        for memory_id in &accepted.supersede_memory_ids {
            let Some(memory) = similar_by_id.get(memory_id.as_str()) else {
                return Err(effect_type_error(
                    "PostAction memory supersede target is absent from supplied similar facts",
                ));
            };
            if !pending
                .command
                .replacement_scope
                .authorized_supersede_scopes
                .contains(&memory.scope_id)
                || memory.state != MemoryState::Active
                || !superseded_memories.insert(memory_id.as_str())
            {
                return Err(effect_type_error(
                    "PostAction memory supersede target is read-only, inactive, or used more than once",
                ));
            }
        }
    }
    for dropped in dropped_candidates {
        if pending.hard_dropped.iter().any(|local| local == dropped) {
            continue;
        }
        let similar_by_id = pending
            .memory_reviews
            .get(dropped.candidate_index as usize)
            .ok_or_else(|| effect_type_error("PostAction memory candidate index"))?
            .similar_memories
            .iter()
            .map(|hit| (hit.memory.id.as_str(), &hit.memory))
            .collect::<BTreeMap<_, _>>();
        if dropped
            .dedupe_memory_id
            .as_ref()
            .is_some_and(|identity| !similar_by_id.contains_key(identity.as_str()))
        {
            return Err(effect_type_error(
                "PostAction dropped candidate dedupe target is absent from supplied similar facts",
            ));
        }
    }
    let mut retired = BTreeSet::new();
    for accepted in accepted_profile_candidates {
        let candidate = pending
            .profiles
            .get(accepted.candidate_index as usize)
            .ok_or_else(|| effect_type_error("PostAction profile candidate index"))?;
        let writable_scope = profile_scope_for_role(&pending.command, candidate.section_role)?;
        for target in &accepted.supersede_nodes {
            let Some(node) = active_by_id.get(&target.node_id) else {
                return Err(effect_type_error(
                    "PostAction profile supersede target is absent or retired more than once",
                ));
            };
            if node.scope_id != *writable_scope || !retired.insert(target.node_id.clone()) {
                return Err(effect_type_error(
                    "PostAction profile supersede target is read-only or retired more than once",
                ));
            }
        }
    }
    let writable_profile_scopes = pending
        .command
        .profile_sections
        .iter()
        .map(|section| &section.scope_id)
        .collect::<BTreeSet<_>>();
    for target in retired_profile_nodes {
        let Some(node) = active_by_id.get(&target.node_id) else {
            return Err(effect_type_error(
                "PostAction retire-only profile target is absent or retired more than once",
            ));
        };
        if !writable_profile_scopes.contains(&node.scope_id)
            || !retired.insert(target.node_id.clone())
        {
            return Err(effect_type_error(
                "PostAction retire-only profile target is read-only or retired more than once",
            ));
        }
    }
    Ok(())
}

/// Builds VMM's review query without confusing it with the durable summary embedding text.
/// 构建 VMM 审核查询，不将其与持久摘要嵌入文本混淆。
fn memory_review_query(summary: &str, details: &str) -> String {
    let summary = summary.trim();
    let details = details.trim();
    let query = if summary.is_empty() {
        details.to_owned()
    } else if details.is_empty() || crate::query_case::simple_fold_equal(summary, details) {
        summary.to_owned()
    } else if simple_lowercase(details).contains(&simple_lowercase(summary)) {
        details.to_owned()
    } else {
        format!("{summary}\n{details}")
    };
    // VMM passes the composite review query through Search before embedding it.
    // VMM 将组合审核查询交给 Search 后才嵌入，因此此处仅归一化查询表示。
    normalize_text(&query)
}

/// Requests fresh persistence vectors only after L2 selected the actual memory creations.
/// 仅在 L2 选定实际创建的记忆后请求新的持久化向量。
fn begin_analyze_persistence_embedding(
    record: &mut CommandRecord,
    mut pending: PendingAnalyze,
) -> Result<CommandStep, CoreError> {
    let indexes = accepted_candidate_indexes(&pending.review)?;
    // Clear query vectors even when no memory survives; they must never reach durable storage.
    // 即使没有记忆存活也清除查询向量；查询向量绝不能进入持久存储。
    pending.embeddings = vec![Vec::new(); pending.memories.len()];
    if indexes.is_empty() {
        return emit_analyze_derived(record, pending);
    }
    let inputs = indexes
        .iter()
        .map(|index| {
            pending
                .memories
                .get(*index as usize)
                .map(|candidate| candidate.summary.trim().to_owned())
                .ok_or_else(|| effect_type_error("persistence embedding candidate index"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let effect = EmbeddingEffect {
        vector_space_id: pending.command.vector_space_id.clone(),
        expected_dimension: record.snapshot.embedding_dimension,
        inputs,
        allow_partial_invalid_inputs: true,
    };
    record.state = FlowState::AnalyzePersistenceEmbedding(Box::new(pending));
    emit_effect(record, HostEffect::Embedding(effect))
}

/// Reconstructs only admitted L1 facts when a review-chain read fails before L2 continuation exists.
/// 当L2续体生成前的评审链读取失败时，仅重建已准入L1事实；参数为已验证L1上下文，返回无评审决定的续体。
#[allow(clippy::too_many_arguments)]
const fn pending_analyze_l1(
    command: PostActionAnalyzePendingTurnCommand,
    turn: Turn,
    user_input_kind: vulcan_memorymesh_core_contracts::TurnInputKind,
    analysis_details: String,
    memories: Vec<MemoryCandidate>,
    profiles: Vec<ProfileCandidate>,
    context_evidence: Vec<ContextEvidenceCandidate>,
) -> PendingAnalyze {
    PendingAnalyze {
        review_fallback_to_l1: false,
        command,
        turn,
        user_input_kind,
        analysis_details,
        memories,
        profiles,
        context_evidence,
        embeddings: Vec::new(),
        memory_reviews: Vec::new(),
        reviewer_to_candidate: Vec::new(),
        hard_dropped: Vec::new(),
        current_profile_nodes: Vec::new(),
        review: LlmOutput::PostActionReview {
            accepted_candidates: Vec::new(),
            dropped_candidates: Vec::new(),
            accepted_profile_candidates: Vec::new(),
            invalid_profile_candidate_indexes: Vec::new(),
            retired_profile_nodes: Vec::new(),
            reason: None,
        },
        embedding_dropped: Vec::new(),
    }
}

/// Restores the complete admitted L1 snapshot, never carrying partial dedupe, replacement or profile activation forward.
/// 恢复完整已准入L1快照，绝不继承局部去重、替代或画像激活；参数为原续体，返回持久嵌入或派生写入步骤。
#[allow(clippy::cast_possible_truncation)]
fn begin_analyze_l1_fallback(
    record: &mut CommandRecord,
    mut pending: PendingAnalyze,
) -> Result<CommandStep, CoreError> {
    record.degraded = true;
    pending.review_fallback_to_l1 = true;
    pending.hard_dropped.clear();
    pending.current_profile_nodes.clear();
    pending.memory_reviews.clear();
    pending.reviewer_to_candidate.clear();
    pending.review = LlmOutput::PostActionReview {
        accepted_candidates: (0..pending.memories.len() as u32)
            .map(|index| AcceptedCandidate {
                candidate_index: index,
                supersede_memory_ids: Vec::new(),
            })
            .collect(),
        dropped_candidates: Vec::new(),
        // Source L1 zero-valued lifecycle fields mean P0 and transient, but no review has set an expiry or source ID.
        // 来源L1零值生命周期字段对应P0和瞬态级别，但评审尚未设置有效期或来源编号。
        accepted_profile_candidates: pending
            .profiles
            .iter()
            .enumerate()
            .map(|(index, profile)| AcceptedProfileCandidate {
                candidate_index: index as u32,
                normalized_content: profile.content.clone(),
                priority: 100,
                profile_level: ProfileLevel::Transient,
                level_reason: None,
                supersede_nodes: Vec::new(),
            })
            .collect(),
        invalid_profile_candidate_indexes: Vec::new(),
        retired_profile_nodes: Vec::new(),
        reason: Some("degraded L1 fallback after review failure".to_owned()),
    };
    begin_analyze_persistence_embedding(record, pending)
}

/// Validates a complete request partition and remaps partial embedding results before mutation.
/// 在变更前校验完整请求划分并重映射部分嵌入结果。
fn advance_analyze_persistence_embedding(
    record: &mut CommandRecord,
    mut pending: PendingAnalyze,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let indexes = accepted_candidate_indexes(&pending.review)?;
    let batch =
        expect_partial_embeddings(result, indexes.len(), record.snapshot.embedding_dimension)?;
    for (input_index, vector) in batch.result_indices.into_iter().zip(batch.vectors) {
        pending.embeddings[indexes[input_index as usize] as usize] = vector;
    }
    let dropped = batch
        .dropped
        .into_iter()
        .map(|mut drop| {
            drop.input_index = indexes[drop.input_index as usize];
            drop
        })
        .collect::<Vec<_>>();
    let rejected = dropped
        .iter()
        .map(|drop| drop.input_index)
        .collect::<BTreeSet<_>>();
    let LlmOutput::PostActionReview {
        accepted_candidates,
        ..
    } = &mut pending.review
    else {
        return Err(effect_type_error("persistence embedding review output"));
    };
    // Remove the entire accepted decision, including its supersede targets. Evidence is later
    // attached only to surviving slots, so a rejected vector can never retire an existing row.
    // 移除整个接受决策及其替代目标。证据后续仅附着于存活槽位，拒绝向量绝不会退役既有记录。
    accepted_candidates.retain(|accepted| !rejected.contains(&accepted.candidate_index));
    pending.embedding_dropped = dropped;
    emit_analyze_derived(record, pending)
}

/// Validates indexed partial results; no missing index, duplicate index, or implicit drop is legal.
/// 校验索引化部分结果；缺失索引、重复索引与隐式丢弃均不合法。
fn expect_partial_embeddings(
    result: HostEffectResult,
    count: usize,
    dimension: u32,
) -> Result<vulcan_memorymesh_core_contracts::EmbeddingBatchResult, CoreError> {
    let HostEffectResult::Embedding(mut batch) = result else {
        return Err(effect_type_error("partial embedding result"));
    };
    if batch.result_indices.is_empty() && batch.dropped.is_empty() && batch.vectors.len() == count {
        batch.result_indices = (0..u32::try_from(count)
            .map_err(|_| effect_type_error("embedding input bound"))?)
            .collect();
    }
    if batch.result_indices.len() != batch.vectors.len()
        || batch.vectors.len().saturating_add(batch.dropped.len()) != count
        || batch.vectors.iter().any(|vector| {
            vector.len() != dimension as usize || vector.iter().any(|value| !value.is_finite())
        })
    {
        return Err(effect_type_error(
            "partial embedding shape or vector values",
        ));
    }
    let mut seen = BTreeSet::new();
    for index in batch
        .result_indices
        .iter()
        .copied()
        .chain(batch.dropped.iter().map(|drop| drop.input_index))
    {
        if index as usize >= count || !seen.insert(index) {
            return Err(effect_type_error("partial embedding partition index"));
        }
    }
    Ok(batch)
}

/// Emits the derived atomic mutation batch for one analyzed pending turn.
/// 为一个已分析待处理轮次发出派生原子变更批次。
#[allow(clippy::too_many_lines)]
fn emit_analyze_derived(
    record: &mut CommandRecord,
    pending: PendingAnalyze,
) -> Result<CommandStep, CoreError> {
    // Map accepted candidate indexes to deterministic command-local memory slots.
    // 将已接受候选索引映射到确定性的命令局部记忆槽位。
    let LlmOutput::PostActionReview {
        accepted_candidates,
        accepted_profile_candidates,
        retired_profile_nodes,
        ..
    } = &pending.review
    else {
        return Err(effect_type_error("analyze derived review output"));
    };
    let mut slots = Vec::new();
    let mut mutations = Vec::new();
    for accepted in accepted_candidates {
        let index = accepted.candidate_index as usize;
        let Some(candidate) = pending.memories.get(index) else {
            return Err(effect_type_error("review index outside candidate set"));
        };
        let slot = EntitySlotId::new(format!("memory-{index}"))
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
        let embedding = Some(
            pending
                .embeddings
                .get(index)
                .filter(|vector| !vector.is_empty())
                .cloned()
                .ok_or_else(|| effect_type_error("accepted memory has no persistence embedding"))?,
        );
        let placeholder_id = MemoryId::new(slot.as_str().to_owned())
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
        slots.push((accepted.candidate_index, slot.clone()));
        mutations.push(Mutation::PutMemory {
            entity: EntityRef::New(slot.clone()),
            record: build_memory_record(
                &record.snapshot,
                &pending,
                candidate,
                embedding,
                placeholder_id,
            )?,
        });
        for superseded_id in &accepted.supersede_memory_ids {
            let target = reviewed_memory_for(&pending, accepted.candidate_index, superseded_id)?;
            mutations.push(Mutation::SupersedeMemory {
                actor_subject_id: pending.command.subject_id.clone(),
                scope_id: target.scope_id.clone(),
                expected_access: MemoryAccessIdentity::from(target),
                memory_id: target.id.clone(),
                expected_version: target.version,
                superseded_by: EntityRef::New(slot.clone()),
                status_reason: Some("superseded-by-review".to_owned()),
            });
        }
    }
    let active_profile_by_id = pending
        .current_profile_nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    // Successful L2 creates active nodes; a restored L1 snapshot retains pending evidence without synthetic review fields.
    // 成功L2创建活跃节点；恢复的L1快照保留待评审证据，不能生成虚假的评审字段。
    for accepted in accepted_profile_candidates {
        let index = accepted.candidate_index as usize;
        let candidate = pending
            .profiles
            .get(index)
            .ok_or_else(|| effect_type_error("profile review index outside candidate set"))?;
        let profile_scope =
            profile_scope_for_role(&pending.command, candidate.section_role)?.clone();
        let slot = EntitySlotId::new(format!("profile-{index}"))
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
        let node = build_analyze_profile_node(
            &pending,
            accepted,
            &active_profile_by_id,
            ProfileNodeId::new(slot.as_str().to_owned())
                .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?,
            0,
        )?;
        mutations.push(Mutation::PutProfileNode {
            entity: EntityRef::New(slot.clone()),
            node,
        });
        // Bind each reviewed supersede target to the new same-batch profile slot.
        // 将每个经审核替代目标绑定到同批次的新画像槽位。
        for retired in &accepted.supersede_nodes {
            let existing = active_profile_by_id.get(&retired.node_id).ok_or_else(|| {
                effect_type_error("profile supersede target is missing from active facts")
            })?;
            mutations.push(Mutation::RetireProfileNode {
                subject_id: existing.subject_id.clone(),
                scope_id: profile_scope.clone(),
                profile_node_id: retired.node_id.clone(),
                expected_version: existing.version,
                superseded_by: Some(EntityRef::New(slot.clone())),
                status_reason: Some(retired.reason.trim().to_owned()),
            });
        }
    }
    // Apply retire-only directives without fabricating a replacement identity.
    // 应用仅退役指令且不伪造替代身份。
    for retired in retired_profile_nodes {
        let existing = active_profile_by_id.get(&retired.node_id).ok_or_else(|| {
            effect_type_error("retire-only profile target is missing from active facts")
        })?;
        mutations.push(Mutation::RetireProfileNode {
            subject_id: existing.subject_id.clone(),
            scope_id: existing.scope_id.clone(),
            profile_node_id: retired.node_id.clone(),
            expected_version: existing.version,
            superseded_by: None,
            status_reason: Some(retired.reason.trim().to_owned()),
        });
    }
    // Aggregate context evidence onto accepted memory slots.
    // 将情境证据聚合到已接受记忆槽位。
    for row in &pending.context_evidence {
        let Some((_, slot)) = slots
            .iter()
            .find(|(index, _)| *index == row.candidate_index)
        else {
            continue;
        };
        mutations.push(Mutation::AggregateContextEvidence {
            memory: EntityRef::New(slot.clone()),
            context_key: retrieval_kernel::normalize_context_key(&row.context_key),
            context_value: retrieval_kernel::normalize_context_value(&row.context_value),
            relation: row.relation,
            occurred_at_ns: pending.turn.created_at_ns,
        });
    }
    // Transition the analyzed turn from pending to completed in the same atomic batch.
    // 在同一原子批次中将已分析轮次从待处理迁移为已完成。
    mutations.push(Mutation::TransitionTurnStatus {
        turn_id: pending.turn.id.clone(),
        from: TurnStatus::Pending,
        to: TurnStatus::Completed,
        user_input_kind: Some(pending.user_input_kind),
        analysis_details: Some(pending.analysis_details.clone()),
    });
    let batch = record.mutation_batch("derive", mutations)?;
    record.state = FlowState::AnalyzeDerived(Box::new(pending));
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Builds the same profile facts for a pending mutation and its committed result; only the supplied identity and version differ.
/// 为待提交变更及其已提交结果构造相同画像事实；输入续体、接纳项、既有画像、身份和版本，返回节点或明确校验错误。
fn build_analyze_profile_node(
    pending: &PendingAnalyze,
    accepted: &AcceptedProfileCandidate,
    active_profile_by_id: &BTreeMap<ProfileNodeId, &ProfileNode>,
    id: ProfileNodeId,
    version: u64,
) -> Result<ProfileNode, CoreError> {
    let candidate = pending
        .profiles
        .get(accepted.candidate_index as usize)
        .ok_or_else(|| effect_type_error("profile review index outside candidate set"))?;
    let refresh_weight = accepted
        .supersede_nodes
        .iter()
        .filter_map(|target| active_profile_by_id.get(&target.node_id))
        .map(|node| node.refresh_weight)
        .max()
        .map_or(0, |weight| weight.saturating_add(1));
    // A pre-review L1 snapshot has neither review authority nor an invented source ID or expiry.
    // 评审前L1快照没有评审授权，不能虚构来源编号或过期时间；两条输出路径必须共用这一判断。
    let (state, source_id, expires_at_ns) = if pending.review_fallback_to_l1 {
        (ProfileNodeState::Pending, None, None)
    } else {
        (
            ProfileNodeState::Active,
            Some(
                ProfileSourceId::new(pending.turn.id.as_str().to_owned())
                    .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?,
            ),
            profile_expiry_ns(
                pending.turn.created_at_ns,
                accepted.profile_level,
                refresh_weight,
            )?,
        )
    };
    Ok(ProfileNode {
        id,
        subject_id: Some(pending.command.subject_id.clone()),
        scope_id: profile_scope_for_role(&pending.command, candidate.section_role)?.clone(),
        content: accepted.normalized_content.trim().to_owned(),
        confidence: Some(0.9),
        state,
        source_turn_ids: vec![pending.turn.id.clone()],
        source_kind: ProfileSourceKind::TurnExtract,
        source_id,
        evidence_source: (!candidate.evidence_source.trim().is_empty())
            .then(|| candidate.evidence_source.trim().to_owned()),
        priority: accepted.priority,
        profile_level: accepted.profile_level,
        level_reason: accepted.level_reason.clone(),
        refresh_weight,
        effective_date_ns: Some(pending.turn.created_at_ns),
        expires_at_ns,
        status_reason: None,
        superseded_by: None,
        version,
        // Keep the existing cutoff binding explicit; the VMM post-model materialization clock remains a separate integration gap.
        // 明确保留当前截止时间绑定；VMM模型返回后的真实物化时钟仍是另一个待补接入差距。
        created_at_ns: pending.command.analysis_cutoff_ns,
        updated_at_ns: pending.command.analysis_cutoff_ns,
    })
}

/// Builds one complete durable memory record from one accepted candidate.
/// 从一个已接受候选构建一条完整持久记忆记录。
fn build_memory_record(
    snapshot: &ConfigSnapshot,
    pending: &PendingAnalyze,
    candidate: &MemoryCandidate,
    embedding: Option<Vec<f32>>,
    placeholder_id: MemoryId,
) -> Result<MemoryRecord, CoreError> {
    // L1 only extracts semantic facts. Core applies the frozen VMM turn-extraction
    // lifecycle defaults instead of asking the model or database adapter to invent them.
    // L1 只提取语义事实。Core 应用冻结的 VMM 轮次提炼生命周期默认值，
    // 而不是要求模型或数据库适配器自行生成这些值。
    let priority = snapshot.memory_policy.postaction_memory_priority;
    let importance = priority_to_importance(priority);
    let expires_at_ns = checked_expiry_from_millis(
        pending.turn.created_at_ns,
        snapshot.memory_policy.postaction_memory_expiry_ms,
    )?;
    // The storage layer replaces this placeholder with the slot-bound domain identity.
    // 存储层会用槽位绑定的领域身份替换该占位标识。
    Ok(MemoryRecord {
        id: placeholder_id,
        access_class: MemoryAccessClass::SubjectPrivate,
        source_subject_id: Some(pending.command.subject_id.clone()),
        scope_id: pending.command.scope_access.write_scope.clone(),
        scope_class: MemoryScopeClass::Environment,
        category: candidate.category,
        summary: candidate.summary.trim().to_owned(),
        content: candidate.content.trim().to_owned(),
        source_kind: MemorySourceKind::Turn,
        origin_session_source_id: Some(pending.command.session_source_id.clone()),
        priority,
        stability: snapshot.memory_policy.postaction_memory_stability,
        tags: Vec::new(),
        evidence: vec![EvidenceRef {
            turn_id: pending.turn.id.clone(),
            excerpt: turn_excerpt(&pending.turn),
            occurred_at_ns: pending.turn.created_at_ns,
        }],
        evidence_source: (!candidate.evidence_source.trim().is_empty())
            .then(|| candidate.evidence_source.clone()),
        state: MemoryState::Active,
        importance,
        confidence: Some(0.9),
        // VMM stamps relational memory rows at analysis commit time while keeping
        // turn-derived expiry anchored to the original turn time. Core uses the
        // immutable analysis cutoff as the deterministic equivalent of that Host clock.
        // VMM 在分析提交时记录关系记忆时间戳，但轮次派生的过期时间仍锚定原始轮次时间。
        // Core 使用不可变分析截止点，作为该 Host 时钟的确定性等价物。
        created_at_ns: pending.command.analysis_cutoff_ns,
        updated_at_ns: pending.command.analysis_cutoff_ns,
        expires_at_ns: Some(expires_at_ns),
        last_recalled_at_ns: None,
        reinforcement_count: 0,
        memory_level: snapshot.memory_policy.postaction_memory_level,
        refresh_weight: u64::from(snapshot.memory_policy.postaction_memory_refresh_weight),
        dedupe_hash: None,
        recalled_count: 0,
        adopted_count: 0,
        cross_session_adopted_count: 0,
        decay_disabled: false,
        status_reason: None,
        last_adopted_at_ns: None,
        last_reinforced_at_ns: None,
        source_turn_id: Some(pending.turn.id.clone()),
        version: 0,
        vector_space_id: embedding
            .as_ref()
            .map(|_| pending.command.vector_space_id.clone()),
        embedding,
    })
}

/// Resolves the exact target identity, scope and version from the same candidate's reviewed facts.
/// 从同一候选的已审核事实中解析精确目标身份、作用域及版本。
fn reviewed_memory_for<'a>(
    pending: &'a PendingAnalyze,
    candidate_index: u32,
    memory_id: &str,
) -> Result<&'a MemoryRecord, CoreError> {
    pending
        .memory_reviews
        .get(candidate_index as usize)
        .ok_or_else(|| effect_type_error("reviewed candidate index is unavailable"))?
        .similar_memories
        .iter()
        .find(|hit| hit.memory.id.as_str() == memory_id)
        .map(|hit| &hit.memory)
        .ok_or_else(|| effect_type_error("reviewed supersede target version is unavailable"))
}

/// Computes the VMM-authoritative level lifetime from millisecond policy durations and returns a nanosecond instant.
/// 根据毫秒策略时长计算VMM权威等级生命周期，并返回纳秒瞬间。
fn profile_expiry_ns(
    base_ns: i64,
    level: ProfileLevel,
    refresh_weight: u32,
) -> Result<Option<i64>, CoreError> {
    const HOUR_MS: u64 = 3_600_000;
    const DAY_MS: u64 = 24 * HOUR_MS;
    let (base, step, maximum) = match level {
        ProfileLevel::Transient => (72 * HOUR_MS, 48 * HOUR_MS, 14 * DAY_MS),
        ProfileLevel::Situational => (14 * DAY_MS, 7 * DAY_MS, 90 * DAY_MS),
        ProfileLevel::Stable => (90 * DAY_MS, 30 * DAY_MS, 365 * DAY_MS),
        ProfileLevel::Persistent => return Ok(None),
    };
    let extension = step.saturating_mul(u64::from(refresh_weight));
    let lifetime = base.saturating_add(extension).min(maximum);
    checked_expiry_from_millis(base_ns, lifetime).map(Some)
}

/// Extracts dropped candidates from one review output.
/// 从一个审核输出提取被丢弃候选。
fn dropped_candidates(review: &LlmOutput) -> Vec<DroppedCandidate> {
    match review {
        LlmOutput::PostActionReview {
            dropped_candidates, ..
        } => dropped_candidates.clone(),
        _ => Vec::new(),
    }
}

/// Extracts superseded memory identities from one review output.
/// 从一个审核输出提取被替代记忆标识。
fn superseded_memory_ids(review: &LlmOutput) -> Vec<MemoryId> {
    match review {
        LlmOutput::PostActionReview {
            accepted_candidates,
            ..
        } => accepted_candidates
            .iter()
            .flat_map(|accepted| accepted.supersede_memory_ids.iter().cloned())
            .filter_map(|id| MemoryId::new(id).ok())
            .collect(),
        _ => Vec::new(),
    }
}

/// Completes one analysis by resolving every derived identity from the commit receipt.
/// 通过从提交收据解析每个派生身份来完成一次分析。
#[allow(clippy::too_many_lines)]
fn advance_analyze_derived(
    record: &mut CommandRecord,
    pending: PendingAnalyze,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let turn_id = pending.command.turn_id.clone();
    let accepted_indexes = accepted_candidate_indexes(&pending.review)?;
    let dropped = dropped_candidates(&pending.review);
    let superseded = superseded_memory_ids(&pending.review);
    let LlmOutput::PostActionReview {
        accepted_profile_candidates,
        retired_profile_nodes,
        ..
    } = &pending.review
    else {
        return Err(effect_type_error("PostAction review output"));
    };
    let accepted_profile_candidates = accepted_profile_candidates.clone();
    let retire_only_profile_nodes = retired_profile_nodes.clone();
    let mut expected_bindings = BTreeMap::new();
    for index in &accepted_indexes {
        let slot = EntitySlotId::new(format!("memory-{index}"))
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
        expected_bindings.insert(slot, EntityKind::Memory);
    }
    for accepted in &accepted_profile_candidates {
        let slot = EntitySlotId::new(format!("profile-{}", accepted.candidate_index))
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
        expected_bindings.insert(slot, EntityKind::ProfileNode);
    }
    advance_reconcilable_with_receipt(record, result, move |receipt| {
        validate_expected_bindings(&receipt, &expected_bindings)?;
        let bound = |slot: &str, expected_kind: EntityKind| -> Result<String, CoreError> {
            receipt
                .entity_bindings
                .iter()
                .find(|binding| {
                    binding.entity_slot_id.as_str() == slot && binding.entity_kind == expected_kind
                })
                .map(|binding| binding.domain_id.clone())
                .ok_or_else(|| {
                    command_error(
                        ErrorCode::Internal,
                        format!("entity slot binding is missing from the commit receipt: {slot}"),
                    )
                })
        };
        let accepted_memory_ids = accepted_indexes
            .iter()
            .map(|index| {
                MemoryId::new(bound(&format!("memory-{index}"), EntityKind::Memory)?)
                    .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))
            })
            .collect::<Result<Vec<_>, CoreError>>()?;
        // Bind each admitted candidate index to its committed domain identity so sparse L2
        // acceptance never shifts evidence rows into the wrong memory. The accepted list is
        // ordered by acceptance while the map stays keyed by the original candidate index.
        // 将每个已准入候选索引绑定到其已提交领域身份，使 L2 稀疏接受永远不会把证据行
        // 错位到错误的记忆。接受列表按接受顺序排列，而映射保持以原始候选索引为键。
        let accepted_memory_by_candidate = accepted_indexes
            .iter()
            .copied()
            .zip(accepted_memory_ids.iter().cloned())
            .collect::<BTreeMap<_, _>>();
        let active_profile_by_id = pending
            .current_profile_nodes
            .iter()
            .map(|node| (node.id.clone(), node))
            .collect::<BTreeMap<_, _>>();
        let accepted_profile_nodes = accepted_profile_candidates
            .iter()
            .map(|accepted| {
                let index = accepted.candidate_index as usize;
                let domain_id = bound(&format!("profile-{index}"), EntityKind::ProfileNode)?;
                let version = receipt
                    .committed_versions
                    .iter()
                    .find(|versioned| {
                        versioned.entity_kind == EntityKind::ProfileNode
                            && versioned.domain_id == domain_id
                    })
                    .map(|versioned| versioned.version)
                    .ok_or_else(|| {
                        command_error(
                            ErrorCode::StorageConflict,
                            "profile slot is missing its final committed version",
                        )
                    })?;
                build_analyze_profile_node(
                    &pending,
                    accepted,
                    &active_profile_by_id,
                    ProfileNodeId::new(domain_id)
                        .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?,
                    version,
                )
            })
            .collect::<Result<Vec<_>, CoreError>>()?;
        // Report aggregated evidence rows aligned with their bound memory identities.
        // 报告与其绑定记忆身份对齐的聚合证据行。
        let mut context_evidence: Vec<MemoryContextEvidence> = Vec::new();
        for row in &pending.context_evidence {
            let Some(memory_id) = accepted_memory_by_candidate.get(&row.candidate_index) else {
                continue;
            };
            let (support, rebuttal, last_supported, last_rebutted) = match row.relation {
                ContextEvidenceRelation::Support => {
                    (1_u32, 0_u32, Some(pending.turn.created_at_ns), None)
                }
                ContextEvidenceRelation::Rebuttal => {
                    (0_u32, 1_u32, None, Some(pending.turn.created_at_ns))
                }
            };
            if let Some(existing) = context_evidence.iter_mut().find(|existing_row| {
                existing_row.memory_id == *memory_id
                    && existing_row.context_key
                        == retrieval_kernel::normalize_context_key(&row.context_key)
                    && existing_row.context_value
                        == retrieval_kernel::normalize_context_value(&row.context_value)
            }) {
                existing.support_count += support;
                existing.rebuttal_count += rebuttal;
                if last_supported.is_some() {
                    existing.last_supported_at_ns = last_supported;
                }
                if last_rebutted.is_some() {
                    existing.last_rebutted_at_ns = last_rebutted;
                }
            } else {
                let context_key = retrieval_kernel::normalize_context_key(&row.context_key);
                let context_value = retrieval_kernel::normalize_context_value(&row.context_value);
                context_evidence.push(MemoryContextEvidence {
                    memory_id: memory_id.clone(),
                    context_key,
                    context_value,
                    support_count: support,
                    rebuttal_count: rebuttal,
                    last_supported_at_ns: last_supported,
                    last_rebutted_at_ns: last_rebutted,
                    version: 1,
                });
            }
        }
        for evidence in &mut context_evidence {
            let domain_id = vulcan_memorymesh_core_contracts::context_evidence_domain_id(
                &evidence.memory_id,
                &evidence.context_key,
                &evidence.context_value,
            );
            evidence.version = receipt
                .committed_versions
                .iter()
                .find(|versioned| {
                    versioned.entity_kind == EntityKind::ContextEvidence
                        && versioned.domain_id == domain_id
                })
                .map(|versioned| versioned.version)
                .ok_or_else(|| {
                    command_error(
                        ErrorCode::StorageConflict,
                        "context evidence result is missing its final committed version",
                    )
                })?;
        }
        Ok(CommandResult::PostActionAnalysis(
            PostActionAnalysisResult {
                turn_id,
                accepted_memory_ids,
                dropped_candidates: dropped,
                embedding_dropped_candidates: pending.embedding_dropped.clone(),
                superseded_memory_ids: superseded,
                accepted_profile_nodes,
                retired_profile_node_ids: accepted_profile_candidates
                    .iter()
                    .flat_map(|accepted| {
                        accepted
                            .supersede_nodes
                            .iter()
                            .map(|target| target.node_id.clone())
                    })
                    .chain(
                        retire_only_profile_nodes
                            .iter()
                            .map(|target| target.node_id.clone()),
                    )
                    .collect(),
                context_evidence,
                receipt: Some(receipt),
            },
        ))
    })
}

/// Extracts accepted candidate indexes from one review output.
/// 从一个审核输出提取已接受候选索引。
fn accepted_candidate_indexes(review: &LlmOutput) -> Result<Vec<u32>, CoreError> {
    let LlmOutput::PostActionReview {
        accepted_candidates,
        ..
    } = review
    else {
        return Err(effect_type_error("review output"));
    };
    Ok(accepted_candidates
        .iter()
        .map(|accepted| accepted.candidate_index)
        .collect())
}

/// Advances direct memory writing from embedding to atomic mutation.
/// 将直接记忆写入从嵌入推进到原子变更。
fn advance_memory_write_dedupe(
    record: &mut CommandRecord,
    pending: PendingMemoryWriteDedupe,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::RecentDirectWriteDedupe(mut matches) = expect_storage_query(result)?
    else {
        return Err(effect_type_error("recent direct-write dedupe result"));
    };
    if matches.len() != 1 {
        return Err(effect_type_error(
            "single direct-write dedupe result must contain exactly one aligned entry",
        ));
    }
    if let Some(existing) = matches.pop().flatten() {
        validate_recent_direct_write_hit(
            &record.snapshot,
            &pending.command.subject_id,
            &pending.command.session_source_id,
            pending.command.occurred_at_ns,
            &pending.command.item,
            &pending.dedupe_hash,
            &existing,
        )?;
        return complete(
            record,
            CommandResult::MemoryWrite(MemoryWriteResult {
                memory: existing,
                deduplicated: true,
                receipt: None,
            }),
        );
    }
    let effect = HostEffect::Embedding(EmbeddingEffect {
        allow_partial_invalid_inputs: false,
        vector_space_id: pending.command.vector_space_id.clone(),
        expected_dimension: record.snapshot.embedding_dimension,
        inputs: vec![memory_review_query(
            &pending.command.item.summary,
            &pending.command.item.details,
        )],
    });
    record.state = FlowState::MemoryWriteEmbedding(pending);
    emit_effect(record, effect)
}

/// Advances one atomic batch from recent durable soft deduplication to completion or embedding.
/// 将一个原子批次从近期持久软去重推进到完成或嵌入。
fn advance_memory_write_batch_dedupe(
    record: &mut CommandRecord,
    pending: PendingMemoryWriteBatchDedupe,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::RecentDirectWriteDedupe(existing) = expect_storage_query(result)?
    else {
        return Err(effect_type_error("recent direct-write batch dedupe result"));
    };
    if existing.len() != pending.command.items.len()
        || existing.len() != pending.dedupe_hashes.len()
    {
        return Err(effect_type_error(
            "direct-write batch dedupe result must align with every caller item",
        ));
    }
    for ((item, dedupe_hash), matched) in pending
        .command
        .items
        .iter()
        .zip(&pending.dedupe_hashes)
        .zip(&existing)
    {
        if let Some(memory) = matched {
            validate_recent_direct_write_hit(
                &record.snapshot,
                &pending.command.subject_id,
                &pending.command.session_source_id,
                pending.command.occurred_at_ns,
                item,
                dedupe_hash,
                memory,
            )?;
        }
    }
    if existing.iter().all(Option::is_some) {
        let memories = existing.into_iter().flatten().collect();
        return complete(
            record,
            CommandResult::MemoryWriteBatch(MemoryWriteBatchResult {
                memories,
                deduplicated: vec![true; pending.command.items.len()],
                receipt: None,
            }),
        );
    }
    let queries = plan_direct_queries(
        &record.snapshot,
        &pending.command.items,
        &existing,
        pending.command.occurred_at_ns,
    )?;
    let effect = HostEffect::Embedding(EmbeddingEffect {
        allow_partial_invalid_inputs: false,
        vector_space_id: pending.command.vector_space_id.clone(),
        expected_dimension: record.snapshot.embedding_dimension,
        inputs: queries.query_plan.unique_queries.clone(),
    });
    record.state = FlowState::MemoryWriteBatchEmbedding(PendingMemoryWriteBatchEmbedding {
        command: pending.command,
        dedupe_hashes: pending.dedupe_hashes,
        existing,
        queries,
    });
    emit_effect(record, effect)
}

/// Advances direct memory writing from embedding to atomic mutation.
/// 将直接记忆写入从嵌入推进到原子变更。
fn advance_memory_write_embedding(
    record: &mut CommandRecord,
    pending: PendingMemoryWriteDedupe,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let command = pending.command;
    // Extract the only requested memory embedding.
    // 提取唯一请求的记忆嵌入。
    let embedding = expect_embeddings(result, 1, record.snapshot.embedding_dimension)?
        .into_iter()
        .next()
        .ok_or_else(|| effect_type_error("memory embedding result is empty"))?;
    let memory = build_direct_memory_record(
        &record.snapshot,
        &DirectRecordSource {
            subject_id: &command.subject_id,
            session_source_id: &command.session_source_id,
            vector_space_id: &command.vector_space_id,
            occurred_at_ns: command.occurred_at_ns,
        },
        &command.item,
        &pending.dedupe_hash,
        0,
        embedding.clone(),
    )?;
    begin_direct_write_review(
        record,
        PendingDirectWriteReview {
            evidence_cache: ContextEvidenceCache::new(),
            queries: DirectQueryPlan {
                query_plan: QueryBatchPlan::new(&[direct_review_query(&memory)]),
                review_to_item: vec![0],
                earlier: vec![None],
            },
            query_facts: Vec::new(),
            occurred_at_ns: command.occurred_at_ns,
            candidate_scope: command.replacement_scope.candidates,
            mode: DirectWriteResultMode::Single,
            proposed: vec![memory],
            embeddings: vec![embedding],
            entries: vec![None],
            reviewer_to_item: Vec::new(),
            memory_reviews: Vec::new(),
            authorized_supersede_scopes: command.replacement_scope.authorized_supersede_scopes,
            scope_access: command.scope_access,
        },
    )
}

/// Builds all immutable proposed-record fields from the normalized command, for both execution and checkpoint verification.
/// 从规范化命令构造全部不可变拟写入字段，供执行及恢复校验共用；输入快照、来源、项目、哈希、索引和向量，返回记录或错误。
fn build_direct_memory_record(
    snapshot: &ConfigSnapshot,
    source: &DirectRecordSource<'_>,
    item: &MemoryWriteBatchItem,
    dedupe_hash: &str,
    index: usize,
    embedding: Vec<f32>,
) -> Result<MemoryRecord, CoreError> {
    let priority = item.priority.unwrap_or(20);
    Ok(MemoryRecord {
        id: MemoryId::new(format!("direct-memory-{index}"))
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?,
        access_class: MemoryAccessClass::SubjectPrivate,
        source_subject_id: Some(source.subject_id.clone()),
        scope_id: item.target_scope_id.clone(),
        scope_class: item.scope_class,
        category: item.category,
        summary: item.summary.clone(),
        content: item.details.clone(),
        source_kind: MemorySourceKind::Direct,
        origin_session_source_id: Some(source.session_source_id.clone()),
        priority,
        stability: direct_memory_stability(item.scope_class),
        expires_at_ns: direct_memory_expiry(
            snapshot,
            item.scope_class,
            item.expires_at_ns,
            source.occurred_at_ns,
        )?,
        last_recalled_at_ns: None,
        reinforcement_count: 0,
        memory_level: direct_memory_level(item.scope_class, item.memory_level),
        refresh_weight: 1,
        dedupe_hash: Some(dedupe_hash.to_owned()),
        recalled_count: 0,
        adopted_count: 0,
        cross_session_adopted_count: 0,
        decay_disabled: false,
        status_reason: None,
        last_adopted_at_ns: None,
        last_reinforced_at_ns: None,
        source_turn_id: None,
        tags: normalize_tags(&item.tags),
        evidence: Vec::new(),
        evidence_source: None,
        state: MemoryState::Active,
        importance: priority_to_importance(priority),
        confidence: Some(1.0),
        created_at_ns: source.occurred_at_ns,
        updated_at_ns: source.occurred_at_ns,
        version: 0,
        vector_space_id: Some(source.vector_space_id.clone()),
        embedding: Some(embedding),
    })
}

/// Plans exact-item collapse before query aliases from normalized items and validated soft hits.
/// 从规范化项目及已校验软命中规划精确项目折叠，再规划查询别名；返回映射或来源查询预算错误。
fn plan_direct_queries(
    snapshot: &ConfigSnapshot,
    items: &[MemoryWriteBatchItem],
    existing: &[Option<MemoryRecord>],
    occurred_at_ns: i64,
) -> Result<DirectQueryPlan, CoreError> {
    if items.len() != existing.len() {
        return Err(effect_type_error("direct query plan input alignment"));
    }
    let mut review_to_item = Vec::new();
    let mut earlier = vec![None; items.len()];
    for (index, item) in items.iter().enumerate() {
        if existing[index].is_some() {
            continue;
        }
        // Only earlier soft misses enter the exact-collapse map in the source Write flow.
        // 来源Write仅把先前软未命中项目加入精确折叠表，不能复用已软命中的拟写入内容。
        for previous in &review_to_item {
            if same_direct_item(snapshot, &items[*previous], item, occurred_at_ns)? {
                earlier[index] = Some(*previous);
                break;
            }
        }
        if earlier[index].is_none() {
            review_to_item.push(index);
        }
    }
    if review_to_item.len() > snapshot.memory_policy.retrieval_policy.max_query_items as usize {
        return Err(validation_error(
            "direct review query count exceeds the source Search budget",
        ));
    }
    let queries = review_to_item
        .iter()
        .map(|index| memory_review_query(&items[*index].summary, &items[*index].details))
        .collect::<Vec<_>>();
    Ok(DirectQueryPlan {
        query_plan: QueryBatchPlan::new(&queries),
        review_to_item,
        earlier,
    })
}

/// Compares fully interpreted source collapse fields, excluding tags and unrelated caller metadata.
/// 比较完整解释后的来源折叠字段，不含标签及无关调用元数据；输入快照、两项及共同写入时间，返回是否相同或有效期错误。
fn same_direct_item(
    snapshot: &ConfigSnapshot,
    left: &MemoryWriteBatchItem,
    right: &MemoryWriteBatchItem,
    occurred_at_ns: i64,
) -> Result<bool, CoreError> {
    Ok(left.target_scope_id == right.target_scope_id
        && left.scope_class == right.scope_class
        && left.category == right.category
        && normalize_hash_text(&left.summary) == normalize_hash_text(&right.summary)
        && normalize_hash_text(&left.details) == normalize_hash_text(&right.details)
        && left.priority.unwrap_or(20) == right.priority.unwrap_or(20)
        && direct_memory_level(left.scope_class, left.memory_level)
            == direct_memory_level(right.scope_class, right.memory_level)
        && direct_memory_expiry(
            snapshot,
            left.scope_class,
            left.expires_at_ns,
            occurred_at_ns,
        )? == direct_memory_expiry(
            snapshot,
            right.scope_class,
            right.expires_at_ns,
            occurred_at_ns,
        )?)
}

/// Advances one atomic direct-memory batch write from embeddings to hard dedupe.
/// 将一次原子主动记忆批量写入从嵌入推进到硬去重。
fn advance_memory_write_batch_embedding(
    record: &mut CommandRecord,
    pending: PendingMemoryWriteBatchEmbedding,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let command = pending.command;
    let unique_vectors = expect_embeddings(
        result,
        pending.queries.query_plan.unique_queries.len(),
        record.snapshot.embedding_dimension,
    )?;
    let mut embeddings = vec![Vec::new(); command.items.len()];
    for (review_index, item_index) in pending.queries.review_to_item.iter().copied().enumerate() {
        embeddings[item_index]
            .clone_from(&unique_vectors[pending.queries.query_plan.caller_to_unique[review_index]]);
    }
    let source = DirectRecordSource {
        subject_id: &command.subject_id,
        session_source_id: &command.session_source_id,
        vector_space_id: &command.vector_space_id,
        occurred_at_ns: command.occurred_at_ns,
    };
    let memories = command
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let hash = pending
                .dedupe_hashes
                .get(index)
                .ok_or_else(|| effect_type_error("direct-write semantic hash index"))?;
            build_direct_memory_record(
                &record.snapshot,
                &source,
                item,
                hash,
                index,
                embeddings[index].clone(),
            )
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    let mut entries = pending
        .existing
        .into_iter()
        .map(|existing| existing.map(PendingBatchMemory::Existing))
        .collect::<Vec<_>>();
    for (entry, earlier) in entries.iter_mut().zip(&pending.queries.earlier) {
        if let Some(source_index) = earlier {
            *entry = Some(PendingBatchMemory::Earlier(*source_index));
        }
    }
    begin_direct_write_review(
        record,
        PendingDirectWriteReview {
            evidence_cache: ContextEvidenceCache::new(),
            queries: pending.queries,
            query_facts: Vec::new(),
            occurred_at_ns: command.occurred_at_ns,
            candidate_scope: command.replacement_scope.candidates,
            mode: DirectWriteResultMode::Batch,
            proposed: memories,
            embeddings,
            entries,
            reviewer_to_item: Vec::new(),
            memory_reviews: Vec::new(),
            authorized_supersede_scopes: command.replacement_scope.authorized_supersede_scopes,
            scope_access: command.scope_access,
        },
    )
}

/// Resolves hard deduplication and candidate-local reviewer evidence for one direct item.
/// 为一条主动写入项目解析硬去重与候选局部审核证据。
fn finish_direct_write_candidate(
    record: &mut CommandRecord,
    mut pending: PendingDirectWriteReview,
    item_index: usize,
    hits: Vec<SearchHit>,
    hard_dedupe_pool: Vec<MemoryRecord>,
    hard_dedupe_threshold: f64,
    hard_dedupe_contract: vulcan_memorymesh_core_contracts::ScoreContractRef,
) -> Result<CommandStep, CoreError> {
    if direct_query_representative(&pending)? != item_index {
        return Err(effect_type_error("direct unique query result cursor"));
    }
    pending.query_facts.push(ReviewQueryFacts {
        hits,
        hard_dedupe_pool,
        hard_dedupe_threshold,
        hard_dedupe_contract,
    });
    begin_direct_write_review(record, pending)
}

/// Selects the category-matching strongest cosine duplicate with a stable identity tie break.
/// 选择类别匹配且余弦最强的重复项，以稳定身份打破平局。
fn select_hard_duplicate(
    pool: &[MemoryRecord],
    category: vulcan_memorymesh_core_contracts::MemoryCategory,
    embedding: &[f32],
    threshold: f64,
) -> Option<MemoryRecord> {
    let mut best: Option<(f64, &MemoryRecord)> = None;
    for memory in pool {
        if memory.category != category || memory.state != MemoryState::Active {
            continue;
        }
        let Some(score) = memory
            .embedding
            .as_ref()
            .and_then(|stored| retrieval_kernel::cosine(embedding, stored))
        else {
            continue;
        };
        if score < threshold {
            continue;
        }
        #[allow(clippy::float_cmp)]
        if best.is_none_or(|(previous, row)| {
            score > previous || (score == previous && memory.id < row.id)
        }) {
            best = Some((score, memory));
        }
    }
    best.map(|(_, memory)| memory.clone())
}

/// Applies VMM's reviewer score projection and floor after the complete retrieval pipeline.
/// 在完整检索流水线之后应用 VMM 的审核评分投影与下限。
fn review_similar_memories(
    mut hits: Vec<SearchHit>,
    policy: &vulcan_memorymesh_core_contracts::RetrievalPolicy,
) -> Vec<SearchHit> {
    let floor = retrieval_kernel::resolve_threshold(
        &policy.replace_min_similarity,
        &policy.vector_contract.contract,
    )
    .unwrap_or(0.0);
    let total = hits.len();
    for (index, hit) in hits.iter_mut().enumerate() {
        let score = hit.final_score.clamp(0.0, 1.0);
        hit.final_score = if hit.rerank_score.is_some() {
            score.max(retrieval_kernel::normalized_rank_score(index + 1, total))
        } else {
            score
        };
        if let Some(explanation) = &mut hit.explanation {
            explanation.pre_review_score = Some(score);
            explanation.final_score = hit.final_score;
        }
    }
    // Source review removes empty text and duplicates only after rank projection, preserving the original denominator.
    // 来源审核在排名投影之后才排除空文本与重复条目，保留原始排名分母；持久记录本身不被改写。
    hits.retain(|hit| {
        hit.final_score >= floor
            && (!hit.memory.summary.trim().is_empty() || !hit.memory.content.trim().is_empty())
    });
    hits.sort_by(|left, right| {
        right
            .final_score
            .total_cmp(&left.final_score)
            .then_with(|| left.memory.id.cmp(&right.memory.id))
    });
    let mut seen = BTreeSet::new();
    hits.retain(|hit| seen.insert(hit.memory.id.clone()));
    hits
}

/// Starts or resumes the next unresolved candidate in the shared direct-write review flow.
/// 启动或恢复共享主动写入审核流程中的下一条未解析候选。
fn begin_direct_write_review(
    record: &mut CommandRecord,
    mut pending: PendingDirectWriteReview,
) -> Result<CommandStep, CoreError> {
    if pending.proposed.len() != pending.embeddings.len()
        || pending.proposed.len() != pending.entries.len()
        || pending.query_facts.len() > pending.queries.query_plan.unique_queries.len()
    {
        return Err(effect_type_error(
            "direct-write review state is not caller-order aligned",
        ));
    }
    if pending.query_facts.len() < pending.queries.query_plan.unique_queries.len() {
        let item_index = direct_query_representative(&pending)?;
        return start_direct_write_candidate_search(record, pending, item_index);
    }
    let policy = &record.snapshot.memory_policy.retrieval_policy;
    // Aliases share observed hits, not the category-specific hard-deduplication decision or L2 index.
    // 别名共享已观察命中，不共享类别专属硬去重决定或L2索引。
    for (review_index, item_index) in pending.queries.review_to_item.iter().copied().enumerate() {
        let candidate = &pending.proposed[item_index];
        let facts = &pending.query_facts[pending.queries.query_plan.caller_to_unique[review_index]];
        if let Some(existing) = select_hard_duplicate(
            &facts.hard_dedupe_pool,
            candidate.category,
            &pending.embeddings[item_index],
            facts.hard_dedupe_threshold,
        ) {
            pending.entries[item_index] = Some(PendingBatchMemory::Existing(existing));
            continue;
        }
        pending.reviewer_to_item.push(item_index);
        pending.memory_reviews.push(MemoryReviewCandidate {
            candidate: MemoryCandidate {
                category: candidate.category,
                summary: candidate.summary.clone(),
                content: candidate.content.clone(),
                evidence_source: "assistant_tool_discovered".to_owned(),
                admission: Admission::Accept,
                admission_reason: None,
            },
            similar_memories: review_similar_memories(facts.hits.clone(), policy),
        });
    }
    pending.query_facts.clear();
    emit_direct_write_review(record, pending)
}

/// Finds the original caller item that first supplied the next unique query, not the next unresolved decision.
/// 查找首次提供下一唯一查询的原调用项，而不是下一个未决项目；输入查询续体，返回代表索引或错误。
fn direct_query_representative(pending: &PendingDirectWriteReview) -> Result<usize, CoreError> {
    let group = pending.query_facts.len();
    pending
        .queries
        .query_plan
        .caller_to_unique
        .iter()
        .position(|index| *index == group)
        .and_then(|index| pending.queries.review_to_item.get(index).copied())
        .ok_or_else(|| effect_type_error("direct unique query representative is unavailable"))
}

/// Runs the complete Core-owned retrieval pipeline for exactly one direct-write candidate.
/// 为恰好一条主动写入候选运行完整的 Core 自有检索流水线。
fn start_direct_write_candidate_search(
    record: &mut CommandRecord,
    pending: PendingDirectWriteReview,
    item_index: usize,
) -> Result<CommandStep, CoreError> {
    let work = direct_query_work(&pending, item_index, &record.snapshot)?;
    record.state = FlowState::SearchPipeline(Box::new(SearchPipeline {
        stage: PipelineStage::Vector,
        work,
        follow_up: PipelineFollowUp::DirectWriteReview {
            pending: Box::new(pending),
            item_index,
        },
        fused: Vec::new(),
        rerank_indices: Vec::new(),
    }));
    start_pipeline(record)
}

/// Builds one source-scoped direct query for both execution and restored-wait validation.
/// 为执行及恢复等待校验构建一条来源作用域的主动查询；输入续体、代表调用索引和快照，返回精确工作或错误。
fn direct_query_work(
    pending: &PendingDirectWriteReview,
    item_index: usize,
    snapshot: &ConfigSnapshot,
) -> Result<SearchWork, CoreError> {
    let candidate = pending
        .proposed
        .get(item_index)
        .ok_or_else(|| effect_type_error("direct-write candidate index is unavailable"))?;
    let embedding = pending
        .embeddings
        .get(item_index)
        .cloned()
        .ok_or_else(|| effect_type_error("direct-write candidate embedding is unavailable"))?;
    let query = pending
        .queries
        .query_plan
        .unique_queries
        .get(pending.query_facts.len())
        .cloned()
        .ok_or_else(|| effect_type_error("direct unique query text is unavailable"))?;
    Ok(SearchWork {
        subject_id: private_memory_subject(candidate)?,
        scope_access: pending.scope_access.clone(),
        queries: vec![query],
        embeddings: vec![embedding],
        vector_space_id: candidate.vector_space_id.clone().ok_or_else(|| {
            effect_type_error("direct-write candidate vector space is unavailable")
        })?,
        boundary: None,
        candidate_scope: Some(pending.candidate_scope.clone()),
        limit: snapshot.memory_policy.retrieval_policy.replacement_top_k,
        enable_hard_dedupe_pool: true,
        now_ns: pending.occurred_at_ns,
        separate_fallback_active: false,
        vector: Vec::new(),
        lexical: Vec::new(),
        records: BTreeMap::new(),
        evidence: Vec::new(),
    })
}

/// Builds the VMM-compatible semantic surface from a direct memory summary and details.
/// 从主动记忆摘要与详情构建与 VMM 兼容的语义表面。
fn direct_review_query(candidate: &MemoryRecord) -> String {
    memory_review_query(&candidate.summary, &candidate.content)
}

/// Emits the shared `PostAction` L2 reviewer or finalizes an all-hard-deduplicated write.
/// 发出共享 `PostAction` L2 审核，或完成全部已硬去重的写入。
fn emit_direct_write_review(
    record: &mut CommandRecord,
    mut pending: PendingDirectWriteReview,
) -> Result<CommandStep, CoreError> {
    // The source Search call has finished; model review and commit recovery no longer need its observation cache.
    // 来源Search调用已结束，模型审核与提交恢复不再需要其观察缓存。
    pending.evidence_cache.clear();
    if pending.memory_reviews.is_empty() {
        return finish_direct_write(record, pending, &BTreeMap::new());
    }
    let prompt = record.snapshot.prompt("postaction_l2_main.md")?.to_owned();
    record.state = FlowState::MemoryWriteReview(Box::new(pending.clone()));
    emit_llm_effect(
        record,
        LlmPurpose::PostActionReview,
        prompt,
        LlmInput::PostActionReview {
            context: vulcan_memorymesh_core_contracts::PostActionReviewContext::DirectWrite {
                occurred_at_ns: pending.occurred_at_ns,
            },
            memory_candidates: pending.memory_reviews,
            profile_candidates: Vec::new(),
            current_profile_nodes: Vec::new(),
        },
    )
}

/// Applies one closed reviewer partition to direct-write entries and supersede targets.
/// 将一个封闭审核划分应用到主动写入结果项与替代目标。
fn advance_direct_write_review(
    record: &mut CommandRecord,
    mut pending: PendingDirectWriteReview,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let review = expect_llm(result)?;
    vulcan_memorymesh_core_contracts::validate_llm_output(LlmPurpose::PostActionReview, &review)?;
    let LlmOutput::PostActionReview {
        accepted_candidates,
        dropped_candidates,
        accepted_profile_candidates,
        invalid_profile_candidate_indexes,
        retired_profile_nodes,
        ..
    } = review
    else {
        return Err(effect_type_error("direct-write review LLM output"));
    };
    if !accepted_profile_candidates.is_empty()
        || !invalid_profile_candidate_indexes.is_empty()
        || !retired_profile_nodes.is_empty()
    {
        return Err(effect_type_error(
            "direct-write review must not produce profile decisions",
        ));
    }
    let mut partition = BTreeSet::new();
    let mut supersede_by_item = BTreeMap::<usize, Vec<MemoryRecord>>::new();
    let mut used_supersede_targets = BTreeSet::new();
    for accepted in accepted_candidates {
        let reviewer_index = accepted.candidate_index as usize;
        if !partition.insert(reviewer_index) {
            return Err(effect_type_error(
                "direct-write review memory partition contains duplicates",
            ));
        }
        let item_index = *pending
            .reviewer_to_item
            .get(reviewer_index)
            .ok_or_else(|| effect_type_error("direct-write reviewer index is unavailable"))?;
        let review_candidate = pending
            .memory_reviews
            .get(reviewer_index)
            .ok_or_else(|| effect_type_error("direct-write reviewer facts are unavailable"))?;
        let mut targets = Vec::new();
        for identity in accepted.supersede_memory_ids {
            let target = review_candidate
                .similar_memories
                .iter()
                .find(|hit| hit.memory.id.as_str() == identity)
                .map(|hit| hit.memory.clone())
                .ok_or_else(|| {
                    effect_type_error("direct-write supersede target belongs to another candidate")
                })?;
            if target.state != MemoryState::Active
                || !pending
                    .authorized_supersede_scopes
                    .contains(&target.scope_id)
                || !used_supersede_targets.insert(target.id.clone())
            {
                return Err(effect_type_error(
                    "direct-write supersede target is inactive, read-only, or duplicated",
                ));
            }
            targets.push(target);
        }
        supersede_by_item.insert(item_index, targets);
    }
    for dropped in dropped_candidates {
        let reviewer_index = dropped.candidate_index as usize;
        if !partition.insert(reviewer_index) {
            return Err(effect_type_error(
                "direct-write review memory partition contains duplicates",
            ));
        }
        let item_index = *pending
            .reviewer_to_item
            .get(reviewer_index)
            .ok_or_else(|| effect_type_error("direct-write reviewer index is unavailable"))?;
        // A reviewer drop without a reliable existing target must still create the explicit caller write, matching VMM's direct-write semantics.
        // 审核丢弃但没有可信既有目标时，仍须新建调用方的显式写入，保持VMM主动写入语义。
        let Some(dedupe_memory_id) = dropped.dedupe_memory_id else {
            continue;
        };
        let existing = pending
            .memory_reviews
            .get(reviewer_index)
            .and_then(|candidate| {
                candidate
                    .similar_memories
                    .iter()
                    .find(|hit| hit.memory.id.as_str() == dedupe_memory_id)
            })
            .map(|hit| hit.memory.clone())
            .ok_or_else(|| {
                effect_type_error("direct-write dedupe target belongs to another candidate")
            })?;
        pending.entries[item_index] = Some(PendingBatchMemory::Existing(existing));
    }
    if partition.len() != pending.memory_reviews.len() {
        return Err(effect_type_error(
            "direct-write review must classify every candidate exactly once",
        ));
    }
    begin_direct_persistence_embedding(record, pending, supersede_by_item)
}

/// Requests strict summary vectors for new records while preserving all reuse decisions.
/// 在保留全部复用决策的同时为新记录请求严格摘要向量。
fn begin_direct_persistence_embedding(
    record: &mut CommandRecord,
    pending: PendingDirectWriteReview,
    supersede_by_item: BTreeMap<usize, Vec<MemoryRecord>>,
) -> Result<CommandStep, CoreError> {
    let inputs = pending
        .proposed
        .iter()
        .zip(&pending.entries)
        .filter(|(_, entry)| entry.is_none())
        .map(|(memory, _)| memory.summary.trim().to_owned())
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        return finish_direct_write(record, pending, &supersede_by_item);
    }
    let effect = EmbeddingEffect {
        vector_space_id: pending
            .proposed
            .first()
            .and_then(|memory| memory.vector_space_id.clone())
            .ok_or_else(|| effect_type_error("direct persistence vector space"))?,
        expected_dimension: record.snapshot.embedding_dimension,
        inputs,
        allow_partial_invalid_inputs: false,
    };
    record.state = FlowState::MemoryWritePersistenceEmbedding {
        pending: Box::new(pending),
        supersede_by_item,
    };
    emit_effect(record, HostEffect::Embedding(effect))
}

/// Binds strict persistence vectors to the still-new caller items before the atomic write.
/// 在原子写入前将严格持久化向量绑定到仍需新建的调用方项目。
fn advance_direct_persistence_embedding(
    record: &mut CommandRecord,
    mut pending: PendingDirectWriteReview,
    supersede_by_item: &BTreeMap<usize, Vec<MemoryRecord>>,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let indexes = pending
        .entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| entry.is_none().then_some(index))
        .collect::<Vec<_>>();
    let vectors = expect_embeddings(result, indexes.len(), record.snapshot.embedding_dimension)?;
    for (index, vector) in indexes.into_iter().zip(vectors) {
        pending.proposed[index].embedding = Some(vector);
    }
    finish_direct_write(record, pending, supersede_by_item)
}

/// Returns the required source subject from one validated private-memory proposal.
/// 从一条已校验私有记忆提案返回必需来源主体。
fn private_memory_subject(memory: &MemoryRecord) -> Result<SubjectId, CoreError> {
    if memory.access_class != MemoryAccessClass::SubjectPrivate {
        return Err(effect_type_error(
            "direct-write proposal is not subject-private",
        ));
    }
    memory
        .source_subject_id
        .clone()
        .ok_or_else(|| effect_type_error("direct-write proposal is missing its source subject"))
}

/// Replaces deduplicated result snapshots with their same-batch superseded lifecycle facts.
/// 使用同批次退役后的生命周期事实替换去重结果快照。
fn update_superseded_direct_entries(
    entries: &mut [Option<PendingBatchMemory>],
    superseded_records: &BTreeMap<MemoryId, MemoryRecord>,
) {
    for entry in entries {
        if let Some(PendingBatchMemory::Existing(memory)) = entry
            && let Some(superseded) = superseded_records.get(&memory.id)
        {
            *memory = superseded.clone();
        }
    }
}

/// Emits the final atomic mutations and selects the aligned single or batch result state.
/// 发出最终原子变更并选择对齐的单条或批量结果状态。
fn finish_direct_write(
    record: &mut CommandRecord,
    mut pending: PendingDirectWriteReview,
    supersede_by_item: &BTreeMap<usize, Vec<MemoryRecord>>,
) -> Result<CommandStep, CoreError> {
    let mut mutations = Vec::new();
    let mut superseded_records = BTreeMap::new();
    for index in 0..pending.proposed.len() {
        if pending.entries[index].is_some() {
            continue;
        }
        let memory = pending.proposed[index].clone();
        let slot = EntitySlotId::new(format!("direct-memory-{index}"))
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
        mutations.push(Mutation::PutMemory {
            entity: EntityRef::New(slot.clone()),
            record: memory.clone(),
        });
        for target in supersede_by_item.get(&index).into_iter().flatten() {
            let status_reason = Some("superseded-by-direct-review".to_owned());
            mutations.push(Mutation::SupersedeMemory {
                actor_subject_id: private_memory_subject(&memory)?,
                scope_id: target.scope_id.clone(),
                expected_access: MemoryAccessIdentity::from(target),
                memory_id: target.id.clone(),
                expected_version: target.version,
                superseded_by: EntityRef::New(slot.clone()),
                status_reason: status_reason.clone(),
            });
            let mut superseded = target.clone();
            superseded.state = MemoryState::Superseded;
            superseded.status_reason = status_reason;
            superseded_records.insert(target.id.clone(), superseded);
        }
        pending.entries[index] = Some(PendingBatchMemory::New { slot, memory });
    }
    update_superseded_direct_entries(&mut pending.entries, &superseded_records);
    let entries = pending
        .entries
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| effect_type_error("direct-write result entry is unresolved"))?;
    let deduplicated = entries
        .iter()
        .map(|entry| !matches!(entry, PendingBatchMemory::New { .. }))
        .collect::<Vec<_>>();
    let batch_pending = PendingMemoryWriteBatch {
        entries,
        deduplicated,
    };
    if mutations.is_empty() {
        let memories = resolve_batch_memories(&batch_pending.entries, None)?;
        return match pending.mode {
            DirectWriteResultMode::Single => complete(
                record,
                CommandResult::MemoryWrite(MemoryWriteResult {
                    memory: memories
                        .into_iter()
                        .next()
                        .ok_or_else(|| effect_type_error("direct-write result is empty"))?,
                    deduplicated: true,
                    receipt: None,
                }),
            ),
            DirectWriteResultMode::Batch => complete(
                record,
                CommandResult::MemoryWriteBatch(MemoryWriteBatchResult {
                    memories,
                    deduplicated: batch_pending.deduplicated,
                    receipt: None,
                }),
            ),
        };
    }
    let batch = record.mutation_batch(
        match pending.mode {
            DirectWriteResultMode::Single => "write",
            DirectWriteResultMode::Batch => "write-batch",
        },
        mutations,
    )?;
    match pending.mode {
        DirectWriteResultMode::Single => {
            let PendingBatchMemory::New { slot, memory } =
                batch_pending
                    .entries
                    .first()
                    .cloned()
                    .ok_or_else(|| effect_type_error("direct-write result is empty"))?
            else {
                return Err(effect_type_error(
                    "single direct-write mutation did not create a record",
                ));
            };
            record.state = FlowState::MemoryWriteMutation(PendingMemoryWrite { memory, slot });
        }
        DirectWriteResultMode::Batch => {
            record.state = FlowState::MemoryWriteBatchMutation(batch_pending);
        }
    }
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Reports the hard-dedupe cosine threshold from the frozen policy.
/// 从冻结策略报告硬去重余弦阈值。
#[allow(clippy::cast_precision_loss, clippy::map_unwrap_or)]
fn hard_dedupe_threshold(policy: &vulcan_memorymesh_core_contracts::RetrievalPolicy) -> f64 {
    retrieval_kernel::hard_duplicate_similarity(
        &policy.hard_dedupe,
        &policy.vector_contract.contract,
    )
    .unwrap_or(0.99)
}

/// Validates that one details result belongs to the exact requested identity and visibility boundary.
/// 校验单条详情结果属于精确请求的身份与可见性边界。
#[allow(clippy::suspicious_operation_groupings)]
fn validate_memory_details_result(
    command: &MemoryDetailsCommand,
    memory: Option<&MemoryRecord>,
) -> Result<(), CoreError> {
    let Some(memory) = memory else {
        return Ok(());
    };
    validate_host_memory_record(memory, None)?;
    if memory.id != command.memory_id
        || !memory.is_visible_to_subject(&command.subject_id)
        || !command
            .scope_access
            .visible_scopes
            .contains(&memory.scope_id)
    {
        return Err(effect_type_error(
            "MemoryDetails result identity or visibility boundary",
        ));
    }
    Ok(())
}

/// Validates caller-aligned batch details without trusting host-selected identities.
/// 校验调用方对齐的批量详情，并且不信任宿主自行选择的实体身份。
fn validate_memory_details_batch_result(
    command: &MemoryDetailsBatchCommand,
    memories: &[Option<MemoryRecord>],
    turns: &[Option<Turn>],
) -> Result<(), CoreError> {
    if memories.len() != command.memory_ids.len() || turns.len() != command.turn_ids.len() {
        return Err(effect_type_error("MemoryDetailsBatch result shape"));
    }
    for (requested_id, memory) in command.memory_ids.iter().zip(memories) {
        if let Some(memory) = memory {
            validate_host_memory_record(memory, None)?;
            if memory.id != *requested_id
                || !memory.is_visible_to_subject(&command.subject_id)
                || !command
                    .scope_access
                    .visible_scopes
                    .contains(&memory.scope_id)
            {
                return Err(effect_type_error(
                    "MemoryDetailsBatch memory identity or visibility boundary",
                ));
            }
        }
    }
    for (requested_id, turn) in command.turn_ids.iter().zip(turns) {
        if let Some(turn) = turn {
            validate_host_turn(turn)?;
            if turn.id != *requested_id
                || turn.subject_id != command.subject_id
                || !command.scope_access.visible_scopes.contains(&turn.scope_id)
            {
                return Err(effect_type_error(
                    "MemoryDetailsBatch turn identity or visibility boundary",
                ));
            }
        }
    }
    Ok(())
}

/// Advances one direct delete from an authorized exact-record preflight into its atomic mutation.
/// 将一次直接删除从已授权精确记录预检推进到原子变更。
fn advance_memory_delete_preflight(
    record: &mut CommandRecord,
    command: MemoryDeleteCommand,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::MemoryDetails(memory) = expect_storage_query(result)? else {
        return Err(effect_type_error("MemoryDelete preflight storage result"));
    };
    let memory = memory.ok_or_else(|| {
        command_error(
            ErrorCode::StorageConflict,
            "memory was not found during delete preflight",
        )
    })?;
    validate_host_memory_record(&memory, None)?;
    if memory.id != command.memory_id
        || memory.scope_id != command.scope_access.write_scope
        || !memory.is_visible_to_subject(&command.subject_id)
    {
        return Err(effect_type_error(
            "MemoryDelete preflight identity or access boundary",
        ));
    }
    if command
        .expected_version
        .is_some_and(|expected| expected != memory.version)
    {
        return Err(command_error(
            ErrorCode::StorageConflict,
            "memory version conflict during delete preflight",
        ));
    }
    let mutation = Mutation::DeleteMemory {
        actor_subject_id: command.subject_id,
        scope_id: command.scope_access.write_scope,
        expected_access: MemoryAccessIdentity::from(&memory),
        memory_id: command.memory_id,
        expected_version: command.expected_version,
    };
    let batch = record.mutation_batch("delete", vec![mutation])?;
    record.state = FlowState::MemoryDeleteMutation;
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Advances batch deletion from existence preflight to one atomic mutation.
/// 将批量删除从存在性预检推进到一次原子变更。
#[allow(clippy::suspicious_operation_groupings)]
fn advance_memory_delete_batch_preflight(
    record: &mut CommandRecord,
    command: MemoryDeleteBatchCommand,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::DetailsBatch { memories, turns } = expect_storage_query(result)? else {
        return Err(effect_type_error(
            "MemoryDeleteBatch preflight storage result",
        ));
    };
    if !turns.is_empty() || memories.len() != command.items.len() {
        return Err(effect_type_error(
            "MemoryDeleteBatch preflight result shape",
        ));
    }
    let mut deleted_memory_ids = Vec::new();
    let mut not_found_memory_ids = Vec::new();
    let mut mutations = Vec::new();
    for (item, memory) in command.items.into_iter().zip(memories) {
        let Some(memory) = memory else {
            not_found_memory_ids.push(item.memory_id);
            continue;
        };
        validate_host_memory_record(&memory, None)?;
        if memory.id != item.memory_id
            || !memory.is_visible_to_subject(&command.subject_id)
            || memory.scope_id != command.scope_access.write_scope
        {
            return Err(effect_type_error(
                "MemoryDeleteBatch preflight identity or ownership boundary",
            ));
        }
        if memory.state != MemoryState::Active {
            not_found_memory_ids.push(item.memory_id);
            continue;
        }
        if item
            .expected_version
            .is_some_and(|expected| expected != memory.version)
        {
            return Err(command_error(
                ErrorCode::StorageConflict,
                format!(
                    "memory version conflict during delete preflight: {}",
                    memory.id
                ),
            ));
        }
        deleted_memory_ids.push(item.memory_id.clone());
        mutations.push(Mutation::DeleteMemory {
            actor_subject_id: command.subject_id.clone(),
            scope_id: command.scope_access.write_scope.clone(),
            expected_access: MemoryAccessIdentity::from(&memory),
            memory_id: item.memory_id,
            expected_version: item.expected_version,
        });
    }
    let outcome = MemoryDeleteBatchResult {
        deleted_memory_ids,
        not_found_memory_ids,
        receipt: None,
    };
    if mutations.is_empty() {
        return complete(record, CommandResult::MemoryDeleteBatch(outcome));
    }
    let batch = record.mutation_batch("delete-batch", mutations)?;
    record.state = FlowState::MemoryDeleteBatchMutation(outcome);
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Completes one profile query with view-driven stable rendering.
/// 使用视图驱动的稳定渲染完成一次画像查询。
fn advance_profile_query(
    record: &mut CommandRecord,
    command: ProfileQueryCommand,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::ProfileNodes(nodes) = expect_storage_query(result)? else {
        return Err(effect_type_error("ProfileQuery storage result"));
    };
    if nodes.len() > MAX_DOMAIN_COLLECTION_ITEMS {
        return Err(effect_type_error(
            "ProfileQuery profile-node facts exceed the public collection bound",
        ));
    }
    let nodes = validate_and_filter_profile_nodes(
        nodes,
        &command.subject_id,
        &command.view.sections,
        command.evaluated_at_ns,
    )?;
    // Rebuild every section from active facts using VMM's authoritative timeline
    // ordering and P/L/W body format. Host order is never trusted as semantic order.
    // 使用 VMM 权威的时间线顺序与 P/L/W 正文格式，从活跃事实重建每个分区。
    // 宿主返回顺序绝不被当作语义顺序信任。
    let timezone = parse_profile_timezone(&command.view.display_timezone)?;
    let mut sections = Vec::with_capacity(command.view.sections.len());
    let mut ordered_nodes = Vec::with_capacity(nodes.len());
    for section in &command.view.sections {
        let section_nodes = nodes
            .iter()
            .filter(|node| node.scope_id == section.scope_id)
            .cloned()
            .collect::<Vec<_>>();
        let (section_nodes, rendered) = render_profile_timeline(section_nodes, timezone)?;
        ordered_nodes.extend(section_nodes.iter().cloned());
        sections.push(ProfileSectionBundle {
            scope_id: section.scope_id.clone(),
            role: section.role,
            display_label: normalize_text(&section.display_label),
            nodes: section_nodes,
            rendered,
        });
    }
    let rendered = match command.view.mode {
        vulcan_memorymesh_core_contracts::ProfileViewMode::Full => {
            render_full_profile_bundle(&sections, command.view.include_explanations)
        }
        vulcan_memorymesh_core_contracts::ProfileViewMode::Split => String::new(),
    };
    complete(
        record,
        CommandResult::ProfileQuery(ProfileBundle {
            subject_id: command.subject_id,
            visible_scopes: command.scope_access.visible_scopes,
            nodes: ordered_nodes,
            rendered,
            sections,
        }),
    )
}

/// Sorts owned nodes and returns their body-only timeline using VMM's P/L/W notation.
/// 排序传入的节点并返回使用 VMM P/L/W 记法的纯正文时间线。
fn render_profile_timeline(
    nodes: Vec<ProfileNode>,
    timezone: chrono_tz::Tz,
) -> Result<(Vec<ProfileNode>, String), CoreError> {
    let mut dated_nodes = nodes
        .into_iter()
        .map(|node| Ok((profile_timeline_day(&node, timezone)?, node)))
        .collect::<Result<Vec<_>, CoreError>>()?;
    dated_nodes.sort_by(|(left_day, left), (right_day, right)| {
        left_day
            .cmp(right_day)
            .then_with(|| {
                profile_priority_label(left.priority).cmp(profile_priority_label(right.priority))
            })
            .then_with(|| right.refresh_weight.cmp(&left.refresh_weight))
            .then_with(|| left.created_at_ns.cmp(&right.created_at_ns))
            .then_with(|| left.content.cmp(&right.content))
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut output = String::new();
    let mut current_day = None;
    for (day, node) in &dated_nodes {
        if current_day != Some(*day) {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&day.format("%Y-%m-%d").to_string());
            output.push_str(":\n");
            current_day = Some(*day);
        }
        output.push('[');
        output.push_str(profile_priority_label(node.priority));
        output.push_str("][");
        output.push_str(profile_level_label(node.profile_level));
        output.push_str("][W");
        output.push_str(&node.refresh_weight.to_string());
        output.push_str("] ");
        output.push_str(&node.content);
        output.push('\n');
    }
    Ok((
        dated_nodes.into_iter().map(|(_, node)| node).collect(),
        output.trim().to_owned(),
    ))
}

/// Resolves one historical local date and explicitly rejects timestamps outside the date library range.
/// 解析一个历史本地日期，并显式拒绝超出日期库范围的时间戳。
fn profile_timeline_day(
    node: &ProfileNode,
    timezone: chrono_tz::Tz,
) -> Result<chrono::NaiveDate, CoreError> {
    use chrono::Offset;
    let anchor = node.effective_date_ns.unwrap_or(node.created_at_ns);
    let utc = chrono::DateTime::from_timestamp_millis(anchor).ok_or_else(|| {
        effect_type_error("profile timeline timestamp is outside the supported date range")
    })?;
    let local_offset = utc
        .with_timezone(&timezone)
        .offset()
        .fix()
        .local_minus_utc();
    // Checked arithmetic prevents local-date overflow at the library's UTC boundaries.
    // 检查式运算避免在日期库的 UTC 边界产生本地日期溢出。
    utc.naive_utc()
        .checked_add_signed(chrono::Duration::seconds(i64::from(local_offset)))
        .map(|local| local.date())
        .ok_or_else(|| effect_type_error("profile local date overflows the supported date range"))
}

/// Parses the explicit IANA timezone without consulting process-local settings or guessing UTC.
/// 解析显式 IANA 时区，不读取进程本地设置，也不猜测回退为 UTC。
fn parse_profile_timezone(value: &str) -> Result<chrono_tz::Tz, CoreError> {
    if value.is_empty() || value.len() > 128 {
        return Err(validation_error(
            "profile display timezone must name a supported IANA timezone",
        ));
    }
    value.parse().map_err(|_| {
        validation_error("profile display timezone must name a supported IANA timezone")
    })
}

/// Maps Core's frozen normalized priority bands back to VMM's P0/P1/P2 labels.
/// 将 Core 冻结的规范化优先级档位映射回 VMM 的 P0/P1/P2 标签。
const fn profile_priority_label(priority: u32) -> &'static str {
    if priority >= 80 {
        "P0"
    } else if priority >= 50 {
        "P1"
    } else {
        "P2"
    }
}

/// Maps one typed profile lifetime to VMM's L0/L1/L2/L3 label.
/// 将一个强类型画像生命周期映射为 VMM 的 L0/L1/L2/L3 标签。
const fn profile_level_label(level: ProfileLevel) -> &'static str {
    match level {
        ProfileLevel::Transient => "L0",
        ProfileLevel::Situational => "L1",
        ProfileLevel::Stable => "L2",
        ProfileLevel::Persistent => "L3",
    }
}

/// Renders the generic full bundle while preserving VMM's environment-before-subject semantics.
/// 在保留 VMM“环境优先于主体”语义的同时渲染通用完整画像包。
fn render_full_profile_bundle(
    sections: &[ProfileSectionBundle],
    include_explanations: bool,
) -> String {
    const INTRO: &str = "以下内容是结合用户历史习惯、偏好、设定总结的画像。";
    const ENVIRONMENT_HEADER: &str =
        "以下是你当前所处的项目环境约束（后出现的环境分区优先于更早分区）：";
    const SUBJECT_HEADER: &str =
        "以下是你当前正在服务的目标主体偏好（请在不违反环境约束的前提下，尽量满足）：";
    const LEGEND: &str = "以下是等级与偏好权重说明：\n- P/L/W 说明：\n- P = Priority（优先级）\n  - P0：硬约束 / 不可违背的规则\n  - P1：重要偏好 / 重要工作约定\n  - P2：普通参考 / 低优先级偏好\n- L = Lifetime Level（生命周期级别）\n  - L0：短暂上下文\n  - L1：阶段性偏好或语境\n  - L2：稳定习惯或长期偏好\n  - L3：持久规则 / 强约束 / 身份特征\n- W = Refresh Weight（刷新权重）\n  - 更高的 W 代表这条记忆被再次确认或续期的次数更多。";
    let nonempty = sections
        .iter()
        .filter(|section| !section.rendered.is_empty())
        .collect::<Vec<_>>();
    if nonempty.is_empty() {
        return String::new();
    }
    let mut blocks = vec![INTRO.to_owned()];
    if include_explanations {
        let structure = nonempty
            .iter()
            .map(|section| {
                let role = match section.role {
                    SectionRole::Environment => "环境约束",
                    SectionRole::Subject => "主体偏好",
                };
                format!("- [{}]：{role}分区。", section.display_label)
            })
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!("{LEGEND}\n\n结构说明：\n{structure}"));
    }
    for (role, header) in [
        (SectionRole::Environment, ENVIRONMENT_HEADER),
        (SectionRole::Subject, SUBJECT_HEADER),
    ] {
        let body = nonempty
            .iter()
            .filter(|section| section.role == role)
            .map(|section| format!("[{}]\n{}", section.display_label, section.rendered))
            .collect::<Vec<_>>()
            .join("\n\n");
        if !body.is_empty() {
            blocks.push(format!("{header}\n{body}"));
        }
    }
    blocks.join("\n\n")
}

/// Advances profile instruction from current nodes to authoritative LLM review.
/// 将画像指令从当前节点推进到权威 LLM 审核。
fn advance_profile_instruction_query(
    record: &mut CommandRecord,
    command: ProfileInstructionCommand,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::ProfileNodes(current_nodes) = expect_storage_query(result)? else {
        return Err(effect_type_error("ProfileInstruction profile query result"));
    };
    if current_nodes.len() > MAX_DOMAIN_COLLECTION_ITEMS {
        return Err(effect_type_error(
            "ProfileInstruction profile-node facts exceed the public collection bound",
        ));
    }
    let current_nodes = validate_and_filter_profile_nodes(
        current_nodes,
        &command.subject_id,
        std::slice::from_ref(&command.target_section),
        command.occurred_at_ns,
    )?;
    // Load the authoritative profile instruction prompt.
    // 加载权威画像指令 Prompt。
    let prompt = record
        .snapshot
        .prompt("profile_instruction_main.md")?
        .to_owned();
    record.state = FlowState::ProfileInstructionLlm {
        command: command.clone(),
        current_nodes: current_nodes.clone(),
    };
    emit_llm_effect(
        record,
        LlmPurpose::ProfileInstruction,
        prompt,
        LlmInput::ProfileInstruction {
            target_section: command.target_section.clone(),
            instruction: command.instruction,
            current_nodes,
        },
    )
}

/// Advances profile instruction from reviewed candidates to atomic mutation.
/// 将画像指令从已审核候选推进到原子变更。
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_lines)]
fn advance_profile_instruction_llm(
    record: &mut CommandRecord,
    command: ProfileInstructionCommand,
    current_nodes: Vec<ProfileNode>,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let output = expect_llm(result)?;
    vulcan_memorymesh_core_contracts::validate_llm_output(LlmPurpose::ProfileInstruction, &output)?;
    let LlmOutput::ProfileInstruction {
        accepted_nodes,
        retired_nodes,
        review_reason,
    } = output
    else {
        return Err(effect_type_error("ProfileInstruction LLM output"));
    };
    // VMM preserves empty optional review/retirement reasons; required identity and ownership checks remain independent.
    // VMM保留可选总体/退役原因的空值；必填身份与归属检查仍独立执行，不能为通过校验虚构原因。
    let review_reason = review_reason.trim().to_owned();
    let mut current_by_id = BTreeMap::new();
    for node in &current_nodes {
        validate_host_profile_node(node)?;
        if !profile_node_is_visible(
            node,
            &command.subject_id,
            std::slice::from_ref(&command.target_section),
        ) || node.state != ProfileNodeState::Active
            || current_by_id.insert(node.id.clone(), node).is_some()
        {
            return Err(effect_type_error(
                "ProfileInstruction profile-node facts violate target ownership, scope, state, or identity bounds",
            ));
        }
    }
    let mut mutations = Vec::new();
    let mut retired_seen = BTreeSet::new();
    let mut explicit_retired = Vec::new();
    // Retire every explicitly named visible active node under exact version control.
    // 在精确版本控制下退役每个显式命名的可见活跃节点。
    for retired in &retired_nodes {
        let current = current_by_id
            .get(&retired.node_id)
            .ok_or_else(|| effect_type_error("ProfileInstruction visible retired node identity"))?;
        if current.scope_id != command.target_section.scope_id
            || current.state != ProfileNodeState::Active
        {
            return Err(effect_type_error(
                "ProfileInstruction writable active retired node",
            ));
        }
        if !retired_seen.insert(retired.node_id.clone()) {
            return Err(effect_type_error(
                "ProfileInstruction retires one profile node more than once",
            ));
        }
        let reason = retired.reason.trim().to_owned();
        mutations.push(Mutation::RetireProfileNode {
            subject_id: current.subject_id.clone(),
            scope_id: command.target_section.scope_id.clone(),
            profile_node_id: retired.node_id.clone(),
            expected_version: current.version,
            superseded_by: None,
            status_reason: (!reason.is_empty()).then(|| reason.clone()),
        });
        explicit_retired.push(RetiredProfileNode {
            node_id: retired.node_id.clone(),
            reason,
        });
    }
    // Create one new profile node per accepted instruction node.
    // 为每个已接受指令节点创建一个新画像节点。
    let mut accepted_built = Vec::new();
    let mut accepted_slots = Vec::new();
    let mut all_retired = Vec::new();
    for (index, accepted) in accepted_nodes.iter().enumerate() {
        let slot = EntitySlotId::new(format!("instruction-{index}"))
            .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?;
        let mut superseded_nodes = Vec::new();
        for superseded in &accepted.supersede_nodes {
            let current = current_by_id
                .get(&superseded.node_id)
                .ok_or_else(|| effect_type_error("ProfileInstruction superseded node identity"))?;
            if current.scope_id != command.target_section.scope_id {
                return Err(effect_type_error(
                    "ProfileInstruction cannot supersede a read-only visible profile node",
                ));
            }
            if !retired_seen.insert(superseded.node_id.clone()) {
                return Err(effect_type_error(
                    "ProfileInstruction retires one profile node more than once",
                ));
            }
            let reason = superseded.reason.trim().to_owned();
            superseded_nodes.push((*current, reason));
        }
        let refresh_weight = superseded_nodes
            .iter()
            .map(|(node, _)| node.refresh_weight)
            .max()
            .map_or(0, |weight| weight.saturating_add(1));
        // VMM records the first nonempty replacement rationale on this new node, independently of other accepted nodes.
        // VMM将第一个非空替代理由记录在本新节点上，不能丢失或从其他接纳节点借用原因。
        let status_reason = superseded_nodes
            .iter()
            .find(|(_, reason)| !reason.is_empty())
            .map(|(_, reason)| reason.clone());
        let (priority, profile_level) =
            manual_profile_authority_floor(accepted.priority, accepted.profile_level);
        let node = ProfileNode {
            id: ProfileNodeId::new(slot.as_str().to_owned())
                .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?,
            subject_id: Some(command.subject_id.clone()),
            scope_id: command.target_section.scope_id.clone(),
            content: accepted.content.trim().to_owned(),
            confidence: Some(0.9),
            state: ProfileNodeState::Active,
            source_turn_ids: Vec::new(),
            source_kind: ProfileSourceKind::ManualInstruction,
            source_id: Some(
                ProfileSourceId::new(command.mutation_identity.operation_id.as_str().to_owned())
                    .map_err(|error| command_error(ErrorCode::Internal, error.to_string()))?,
            ),
            evidence_source: None,
            priority,
            profile_level,
            level_reason: accepted.level_reason.clone(),
            refresh_weight,
            effective_date_ns: Some(command.occurred_at_ns),
            expires_at_ns: profile_expiry_ns(
                command.occurred_at_ns,
                profile_level,
                refresh_weight,
            )?,
            status_reason,
            superseded_by: None,
            version: 0,
            created_at_ns: command.occurred_at_ns,
            updated_at_ns: command.occurred_at_ns,
        };
        // Bind the new identity before any retirement references its same-batch slot.
        // 在任何退役项引用同批次槽位前先绑定新身份。
        mutations.push(Mutation::PutProfileNode {
            entity: EntityRef::New(slot.clone()),
            node: node.clone(),
        });
        // Retire the nodes this accepted node supersedes under exact versions.
        // 在精确版本下退役该已接受节点所替代的节点。
        for (current, reason) in superseded_nodes {
            mutations.push(Mutation::RetireProfileNode {
                subject_id: current.subject_id.clone(),
                scope_id: command.target_section.scope_id.clone(),
                profile_node_id: current.id.clone(),
                expected_version: current.version,
                superseded_by: Some(EntityRef::New(slot.clone())),
                status_reason: (!reason.is_empty()).then(|| reason.clone()),
            });
            all_retired.push(RetiredProfileNode {
                node_id: current.id.clone(),
                reason,
            });
        }
        accepted_slots.push(slot);
        accepted_built.push(node);
    }
    // Preserve VMM's response order: accepted-node supersessions precede independent retirements, without reordering the atomic batch.
    // 保留VMM响应顺序：接纳节点的替代项在独立退役项之前，同时不重排原子变更批次。
    all_retired.extend(explicit_retired);
    let outcome = ProfileInstructionResult {
        accepted_nodes: accepted_built,
        retired_nodes: all_retired,
        review_reason,
        receipt: None,
    };
    if mutations.is_empty() {
        return complete(record, CommandResult::ProfileInstruction(outcome));
    }
    let batch = record.mutation_batch("instruction", mutations)?;
    record.state = FlowState::ProfileInstructionMutation(Box::new(PendingProfileInstruction {
        outcome,
        accepted_slots,
    }));
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Applies the minimum authority assigned to an explicit user or project profile instruction.
/// 对显式用户或项目画像指令应用最低权限等级。
#[must_use]
const fn manual_profile_authority_floor(
    priority: u32,
    profile_level: ProfileLevel,
) -> (u32, ProfileLevel) {
    let priority = if priority < 60 { 60 } else { priority };
    let profile_level = match profile_level {
        ProfileLevel::Transient | ProfileLevel::Situational => ProfileLevel::Stable,
        ProfileLevel::Stable | ProfileLevel::Persistent => profile_level,
    };
    (priority, profile_level)
}

/// Resolves one analysis profile role to the exact host-authorized writable scope.
/// 将一个分析画像角色解析为宿主明确授权的精确可写作用域。
fn profile_scope_for_role(
    command: &PostActionAnalyzePendingTurnCommand,
    role: vulcan_memorymesh_core_contracts::SectionRole,
) -> Result<&vulcan_memorymesh_core_contracts::ScopeId, CoreError> {
    command
        .profile_sections
        .iter()
        .find(|section| section.role == role)
        .map(|section| &section.scope_id)
        .ok_or_else(|| {
            effect_type_error("PostAction profile candidate references an unbound section role")
        })
}

/// Reports whether one active profile fact belongs to an authorized section for this subject.
/// 判断一条活跃画像事实是否属于该主体获授权的分区。
fn profile_node_is_visible(
    node: &ProfileNode,
    subject_id: &SubjectId,
    sections: &[ProfileViewSection],
) -> bool {
    sections
        .iter()
        .find(|section| section.scope_id == node.scope_id)
        .is_some_and(|section| match section.role {
            SectionRole::Environment => true,
            SectionRole::Subject => node.subject_id.as_ref() == Some(subject_id),
        })
}

/// Validates Host profile facts and retains only nodes active at the caller-supplied business time.
/// 校验宿主画像事实，并只保留在调用方提供的业务时间点处于活跃状态的节点。
fn validate_and_filter_profile_nodes(
    nodes: Vec<ProfileNode>,
    subject_id: &SubjectId,
    sections: &[ProfileViewSection],
    evaluated_at_ns: i64,
) -> Result<Vec<ProfileNode>, CoreError> {
    let mut seen = BTreeSet::new();
    let mut active = Vec::with_capacity(nodes.len());
    for node in nodes {
        validate_host_profile_node(&node)?;
        if !profile_node_is_visible(&node, subject_id, sections)
            || node.state != ProfileNodeState::Active
            || node.created_at_ns > evaluated_at_ns
            || node.updated_at_ns > evaluated_at_ns
            || !seen.insert(node.id.clone())
        {
            return Err(effect_type_error(
                "Profile-node facts violate ownership, state, snapshot time, or identity bounds",
            ));
        }
        // Core owns lifecycle evaluation; Host may pre-filter but cannot decide the semantics.
        // Core 拥有生命周期评估权威；宿主可以预过滤，但不能决定其语义。
        if profile_node_is_active_at(&node, evaluated_at_ns) {
            active.push(node);
        }
    }
    Ok(active)
}

/// Reports whether one active-state profile node is effective and unexpired at a business time.
/// 判断一条 active 状态画像节点在指定业务时间是否已经生效且尚未过期。
fn profile_node_is_active_at(node: &ProfileNode, evaluated_at_ns: i64) -> bool {
    node.effective_date_ns
        .is_none_or(|effective_at_ns| effective_at_ns <= evaluated_at_ns)
        && node
            .expires_at_ns
            .is_none_or(|expires_at_ns| expires_at_ns > evaluated_at_ns)
}

/// Validates ordered profile section bindings against an authorized scope boundary.
/// 根据授权作用域边界校验有序画像分区绑定。
fn validate_profile_sections(
    scope_access: &ScopeAccess,
    sections: &[ProfileViewSection],
    require_unique_roles: bool,
) -> Result<(), CoreError> {
    if sections.is_empty() {
        return Err(validation_error(
            "profile sections require at least one ordered binding",
        ));
    }
    let visible_scopes = scope_access.visible_scopes.iter().collect::<BTreeSet<_>>();
    let mut scopes = BTreeSet::new();
    let mut roles = BTreeSet::new();
    for section in sections {
        validate_nonempty_text(&section.display_label, "profile section label")?;
        if section.display_label.trim() != section.display_label
            || section.display_label.len() > MAX_HOST_IDENTIFIER_BYTES
            || section.display_label.contains(['\r', '\n', '[', ']'])
        {
            return Err(validation_error(
                "profile section label must be trimmed, single-line, bracket-free, and identifier-sized",
            ));
        }
        if !visible_scopes.contains(&section.scope_id) || !scopes.insert(&section.scope_id) {
            return Err(validation_error(
                "profile sections must use unique visible scopes",
            ));
        }
        if require_unique_roles && !roles.insert(section.role as u32) {
            return Err(validation_error(
                "PostAction profile sections must use unique roles",
            ));
        }
    }
    Ok(())
}

/// Advances VMM-aligned retention facts into six independently executable plans.
/// 将与 VMM 对齐的保留事实推进为六类可独立执行的计划。
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_lines)]
fn advance_retention_query(
    record: &mut CommandRecord,
    command: RetentionCommand,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::RetentionScan(facts) = expect_storage_query(result)? else {
        return Err(effect_type_error("Retention storage result"));
    };
    let policy = &command.policy;
    if facts.subject_id != command.subject_id || facts.scope_id != command.scope_access.write_scope
    {
        return Err(effect_type_error(
            "Retention storage result identity does not match the requested subject and write scope",
        ));
    }
    validate_retention_fact_bounds(&facts, policy)?;
    let mut plans = RetentionPlanSet {
        identity: command.mutation_identity.clone(),
        subject_id: command.subject_id.clone(),
        scope_id: command.scope_access.write_scope.clone(),
        terminal_memory_recycle: Vec::new(),
        cold_turn_enqueue: Vec::new(),
        cold_turn_execute: Vec::new(),
        idle_session_recycle: Vec::new(),
        vector_gc_retry: Vec::new(),
        trash_purge: Vec::new(),
    };

    // Phase one only recycles terminal memories and applies VMM's narrowly scoped shared-memory
    // protection rule. Active memory age and importance are intentionally irrelevant here.
    // 第一阶段仅回收终态记忆，并应用 VMM 的窄范围共享记忆保护规则；活跃记忆年龄与重要度在此无关。
    let mut terminal_memories = facts.terminal_memories;
    terminal_memories.sort_by(|left, right| {
        left.memory
            .updated_at_ns
            .cmp(&right.memory.updated_at_ns)
            .then_with(|| left.memory.id.cmp(&right.memory.id))
    });
    let mut memory_ids = BTreeSet::new();
    for fact in terminal_memories {
        let memory = &fact.memory;
        validate_host_memory_record(memory, None)?;
        if !memory.is_visible_to_subject(&command.subject_id)
            || memory.scope_id != command.scope_access.write_scope
            || memory.updated_at_ns > command.now_ns
            || !matches!(
                memory.state,
                MemoryState::Superseded | MemoryState::Deleted | MemoryState::Expired
            )
            || !memory_ids.insert(memory.id.clone())
        {
            return Err(effect_type_error(
                "Retention terminal-memory facts violate ownership, state, or identity bounds",
            ));
        }
        let protected_shared = policy.skip_protected_shared_memories
            && fact.protected_shared
            && (memory.priority >= policy.protected_priority_floor
                || memory.memory_level as u32 >= policy.protected_memory_level_floor);
        if !protected_shared {
            plans.terminal_memory_recycle.push(PlannedMemoryRef {
                memory_id: memory.id.as_str().to_owned(),
                expected_version: memory.version,
                expected_access: MemoryAccessIdentity::from(memory),
                reason: RetentionReasonCode::TerminalMemoryRecycle,
            });
        }
    }

    // Phase two creates durable jobs from Host-resolved sessions with at least one unreferenced
    // turn outside the count-based hot window.
    // 第二阶段从 Host 已解析且至少有一个热窗口外无引用 Turn 的会话创建持久任务。
    let mut cold_candidates = facts.cold_turn_candidates;
    cold_candidates.sort_by(|left, right| left.session_source_id.cmp(&right.session_source_id));
    let mut cold_sessions = BTreeSet::new();
    for fact in cold_candidates {
        if fact.recyclable_turn_count == 0 || !cold_sessions.insert(fact.session_source_id.clone())
        {
            return Err(effect_type_error(
                "Retention cold-turn candidates violate count or identity bounds",
            ));
        }
        plans.cold_turn_enqueue.push(PlannedSessionRef {
            session_source_id: fact.session_source_id.as_str().to_owned(),
            reason: RetentionReasonCode::ColdTurnRecycle,
        });
    }

    // Phase three selects only due durable cold-turn jobs; enqueue and execution remain separate
    // so a crash cannot lose the maintenance authority.
    // 第三阶段仅选择已到期的持久冷 Turn 任务；入队与执行保持分离，避免崩溃丢失维护权威。
    let mut cold_jobs = facts.cold_turn_jobs;
    cold_jobs.sort_by(|left, right| {
        left.next_run_at_ns
            .cmp(&right.next_run_at_ns)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
    let mut cold_job_ids = BTreeSet::new();
    let mut cold_job_sessions = BTreeSet::new();
    for fact in cold_jobs {
        validate_host_identifier_text(&fact.job_id, "retention cold-turn job id")?;
        if !cold_job_ids.insert(fact.job_id.clone())
            || !cold_job_sessions.insert(fact.session_source_id.clone())
        {
            return Err(effect_type_error(
                "Retention cold-turn jobs violate identity bounds",
            ));
        }
        if fact.next_run_at_ns <= command.now_ns {
            plans.cold_turn_execute.push(PlannedColdTurnJobRef {
                job_id: fact.job_id,
                session_source_id: fact.session_source_id.as_str().to_owned(),
                reason: RetentionReasonCode::ColdTurnRecycle,
            });
        }
    }

    // Phase four selects sessions whose latest durable activity is older than the idle window and
    // that have no Pending Turn, preserving Pending Turn as the recovery authority.
    // 第四阶段选择最近持久活动超过闲置窗口且不存在 Pending Turn 的会话，继续以 Pending Turn 作为恢复权威。
    let mut idle_sessions = facts.idle_sessions;
    idle_sessions.sort_by(|left, right| {
        left.last_activity_ns
            .cmp(&right.last_activity_ns)
            .then_with(|| left.session_source_id.cmp(&right.session_source_id))
    });
    let mut idle_ids = BTreeSet::new();
    for fact in idle_sessions {
        if fact.last_activity_ns > command.now_ns
            || !idle_ids.insert(fact.session_source_id.clone())
        {
            return Err(effect_type_error(
                "Retention idle-session facts violate identity bounds",
            ));
        }
        let idle_ns = checked_duration_ns(policy.session_idle_ms, "session idle")?;
        if !fact.has_pending_turn && command.now_ns.saturating_sub(fact.last_activity_ns) >= idle_ns
        {
            plans.idle_session_recycle.push(PlannedSessionRef {
                session_source_id: fact.session_source_id.as_str().to_owned(),
                reason: RetentionReasonCode::IdleSessionRecycle,
            });
        }
    }

    // Phase five retries due sidecar vector deletions. This is GC retry, never an instruction to
    // recompute database-specific embeddings or similarity formulas inside Core.
    // 第五阶段重试到期旁路向量删除；这是 GC 重试，绝不是要求 Core 重算数据库专用嵌入或相似度公式。
    let mut vector_jobs = facts.vector_gc_jobs;
    vector_jobs.sort_by(|left, right| {
        left.next_run_at_ns
            .cmp(&right.next_run_at_ns)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
    let mut vector_job_ids = BTreeSet::new();
    let mut vector_ids = BTreeSet::new();
    for fact in vector_jobs {
        validate_host_identifier_text(&fact.job_id, "retention vector-GC job id")?;
        validate_host_identifier_text(&fact.vector_id, "retention vector id")?;
        if !vector_job_ids.insert(fact.job_id.clone()) || !vector_ids.insert(fact.vector_id.clone())
        {
            return Err(effect_type_error(
                "Retention vector-GC jobs violate identity bounds",
            ));
        }
        if fact.next_run_at_ns <= command.now_ns {
            plans.vector_gc_retry.push(PlannedVectorGcRef {
                job_id: fact.job_id,
                vector_id: fact.vector_id,
                reason: RetentionReasonCode::VectorGcRetry,
            });
        }
    }

    // Phase six purges whole expired recycle batches so relational trash coordinates remain
    // atomic instead of being weakened into unrelated per-memory deletes.
    // 第六阶段按完整过期回收批次清除，使关系回收站坐标保持原子性，而不退化成无关的逐记忆删除。
    let mut trash_batches = facts.trash_batches;
    trash_batches.sort_by(|left, right| {
        left.recycled_at_ns
            .cmp(&right.recycled_at_ns)
            .then_with(|| left.batch_id.cmp(&right.batch_id))
    });
    let mut trash_ids = BTreeSet::new();
    for fact in trash_batches {
        validate_host_identifier_text(&fact.batch_id, "retention trash batch id")?;
        if fact.recycled_at_ns > command.now_ns || !trash_ids.insert(fact.batch_id.clone()) {
            return Err(effect_type_error(
                "Retention trash facts violate identity bounds",
            ));
        }
        let retention_ns = checked_duration_ns(policy.trash_retention_ms, "trash retention")?;
        if command.now_ns.saturating_sub(fact.recycled_at_ns) >= retention_ns {
            plans.trash_purge.push(PlannedTrashBatchRef {
                batch_id: fact.batch_id,
                reason: RetentionReasonCode::TrashExpiryPurge,
            });
        }
    }
    complete(record, CommandResult::Retention(plans))
}

/// Rejects Host retention result families that exceed the exact requested physical bounds.
/// 拒绝超过精确请求物理边界的 Host 保留结果族。
fn validate_retention_fact_bounds(
    facts: &vulcan_memorymesh_core_contracts::RetentionScanFacts,
    policy: &vulcan_memorymesh_core_contracts::RetentionPolicy,
) -> Result<(), CoreError> {
    let within = |actual: usize, limit: u32| actual <= limit as usize;
    if !within(facts.terminal_memories.len(), policy.memory_recycle_limit)
        || !within(
            facts.cold_turn_candidates.len(),
            policy.cold_turn_scan_limit,
        )
        || !within(facts.cold_turn_jobs.len(), policy.cold_turn_job_limit)
        || !within(facts.idle_sessions.len(), policy.idle_session_limit)
        || !within(facts.vector_gc_jobs.len(), policy.vector_gc_limit)
        || !within(facts.trash_batches.len(), policy.trash_purge_limit)
    {
        return Err(effect_type_error(
            "Retention storage result exceeds requested family bounds",
        ));
    }
    Ok(())
}

/// Advances lifecycle maintenance from caller-ordered exact records into one access-bound atomic mutation.
/// 将生命周期维护从调用方有序精确记录推进为一个绑定访问身份的原子变更。
fn advance_maintenance_lifecycle_preflight(
    record: &mut CommandRecord,
    command: &MaintenanceCommand,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::DetailsBatch {
        memories: slots,
        turns,
    } = expect_storage_query(result)?
    else {
        return Err(effect_type_error(
            "maintenance lifecycle preflight details result",
        ));
    };
    let (items, restore) = match &command.action {
        MaintenanceAction::RestoreRecycled(items) => (items, true),
        MaintenanceAction::PurgeRecycled(items) => (items, false),
        MaintenanceAction::RebuildVectors { .. } => {
            return Err(command_error(
                ErrorCode::Internal,
                "lifecycle maintenance preflight retained a rebuild action",
            ));
        }
    };
    if !turns.is_empty() || slots.len() != items.len() {
        return Err(effect_type_error(
            "maintenance lifecycle caller-ordered detail slots",
        ));
    }
    let mut mutations = Vec::with_capacity(items.len());
    let mut affected_memory_ids = Vec::with_capacity(items.len());
    for (item, slot) in items.iter().zip(slots) {
        let memory = slot
            .ok_or_else(|| effect_type_error("maintenance lifecycle target memory must exist"))?;
        validate_host_memory_record(&memory, None)?;
        if memory.id != item.memory_id
            || memory.scope_id != command.scope_access.write_scope
            || !memory.is_visible_to_subject(&command.subject_id)
            || memory.state != MemoryState::Recycled
            || memory.version != item.expected_version
        {
            return Err(effect_type_error(
                "maintenance lifecycle memory access, state, or version boundary",
            ));
        }
        let expected_access = MemoryAccessIdentity::from(&memory);
        let mutation = if restore {
            Mutation::RestoreMemory {
                actor_subject_id: command.subject_id.clone(),
                scope_id: command.scope_access.write_scope.clone(),
                expected_access,
                memory_id: item.memory_id.clone(),
                expected_version: item.expected_version,
            }
        } else {
            Mutation::PurgeMemory {
                actor_subject_id: command.subject_id.clone(),
                scope_id: command.scope_access.write_scope.clone(),
                expected_access,
                memory_id: item.memory_id.clone(),
                expected_version: item.expected_version,
            }
        };
        affected_memory_ids.push(item.memory_id.clone());
        mutations.push(mutation);
    }
    let batch = record.mutation_batch("maintenance", mutations)?;
    record.state = FlowState::MaintenanceMutation(PendingMaintenanceResult {
        affected_memory_ids,
        rebuilt_vector_count: 0,
    });
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Advances vector maintenance from exact details to one embedding batch.
/// 将向量维护从精确详情推进到一次嵌入批次。
#[allow(clippy::needless_pass_by_value)]
fn advance_maintenance_rebuild_query(
    record: &mut CommandRecord,
    command: MaintenanceCommand,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let StorageQueryResult::DetailsBatch {
        memories: slots,
        turns,
    } = expect_storage_query(result)?
    else {
        return Err(effect_type_error("maintenance rebuild details result"));
    };
    let MaintenanceAction::RebuildVectors {
        memory_ids,
        vector_space_id,
    } = &command.action
    else {
        return Err(command_error(
            ErrorCode::Internal,
            "maintenance rebuild state lost its rebuild action",
        ));
    };
    if !turns.is_empty() || slots.len() != memory_ids.len() {
        return Err(effect_type_error(
            "maintenance rebuild caller-ordered detail slots",
        ));
    }
    let memories = slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            let memory = slot
                .ok_or_else(|| effect_type_error("maintenance rebuild existing memory detail"))?;
            validate_host_memory_record(&memory, None)?;
            if memory.id != memory_ids[index]
                || !memory.is_visible_to_subject(&command.subject_id)
                || memory.scope_id != command.scope_access.write_scope
                || memory.state != MemoryState::Active
                || memory.updated_at_ns > command.occurred_at_ns
            {
                return Err(effect_type_error(
                    "maintenance rebuild active owned memory detail within its cutoff",
                ));
            }
            Ok(memory)
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    let inputs = memories
        .iter()
        .map(|memory| memory.summary.trim().to_owned())
        .collect();
    record.state = FlowState::MaintenanceRebuildEmbedding {
        command: command.clone(),
        memories,
    };
    emit_effect(
        record,
        HostEffect::Embedding(EmbeddingEffect {
            allow_partial_invalid_inputs: false,
            vector_space_id: vector_space_id.clone(),
            expected_dimension: record.snapshot.embedding_dimension,
            inputs,
        }),
    )
}

/// Advances vector maintenance from embeddings to one atomic versioned replacement batch.
/// 将向量维护从嵌入推进到一次原子带版本替换批次。
#[allow(clippy::needless_pass_by_value)]
fn advance_maintenance_rebuild_embedding(
    record: &mut CommandRecord,
    command: MaintenanceCommand,
    mut memories: Vec<MemoryRecord>,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    let embeddings =
        expect_embeddings(result, memories.len(), record.snapshot.embedding_dimension)?;
    let MaintenanceAction::RebuildVectors {
        memory_ids,
        vector_space_id,
    } = &command.action
    else {
        return Err(command_error(
            ErrorCode::Internal,
            "maintenance rebuild state lost its rebuild action",
        ));
    };
    let mutations = memories
        .iter_mut()
        .zip(embeddings)
        .map(|(memory, embedding)| {
            let expected_version = memory.version;
            memory.vector_space_id = Some(vector_space_id.clone());
            memory.embedding = Some(embedding);
            memory.updated_at_ns = command.occurred_at_ns;
            Mutation::PutMemory {
                entity: EntityRef::Existing {
                    domain_id: memory.id.as_str().to_owned(),
                    expected_version,
                },
                record: memory.clone(),
            }
        })
        .collect();
    let rebuilt_vector_count = u32::try_from(memory_ids.len()).map_err(|_| {
        command_error(
            ErrorCode::InvalidArgument,
            "maintenance rebuild count exceeds u32",
        )
    })?;
    let batch = record.mutation_batch("rebuild", mutations)?;
    record.state = FlowState::MaintenanceMutation(PendingMaintenanceResult {
        affected_memory_ids: memory_ids.clone(),
        rebuilt_vector_count,
    });
    emit_effect(record, HostEffect::StorageMutation(batch))
}

/// Completes `ChatCompact` only from the storage-owned atomic boundary outcome.
/// 仅根据存储所有的原子边界结果完成 `ChatCompact`。
fn advance_chat_compact(
    record: &mut CommandRecord,
    result: HostEffectResult,
) -> Result<CommandStep, CoreError> {
    match expect_mutation(result)? {
        MutationOutcome::Committed {
            chat_compact: Some(outcome),
            receipt,
            ..
        } => {
            validate_commit_receipt_identity(record, &receipt)?;
            if !outcome.accepted || (outcome.updated && outcome.compacted_turn_id.is_none()) {
                return Err(effect_type_error(
                    "ChatCompact committed outcome invariants",
                ));
            }
            complete(
                record,
                CommandResult::ChatCompact(ChatCompactResult {
                    accepted: outcome.accepted,
                    updated: outcome.updated,
                    compacted_turn_id: outcome.compacted_turn_id,
                    receipt,
                }),
            )
        }
        MutationOutcome::Committed {
            chat_compact: None, ..
        } => Err(effect_type_error("ChatCompact committed outcome metadata")),
        MutationOutcome::Rejected { reason } => Err(CoreError::new(
            ErrorCode::StorageConflict,
            ErrorStage::Storage,
            reason,
        )),
        MutationOutcome::Uncertain => {
            let idempotency_key =
                record
                    .pending_mutation_idempotency_key
                    .clone()
                    .ok_or_else(|| {
                        command_error(
                            ErrorCode::Internal,
                            "pending mutation idempotency key is missing",
                        )
                    })?;
            mark_uncertain(record, idempotency_key)
        }
    }
}

/// Emits one host effect and records its only acceptable result identity.
/// 发出一个宿主 Effect 并记录其唯一可接受结果标识。
fn emit_effect(record: &mut CommandRecord, effect: HostEffect) -> Result<CommandStep, CoreError> {
    record.next_effect_sequence = record.next_effect_sequence.checked_add(1).ok_or_else(|| {
        command_error(
            ErrorCode::Internal,
            "effect identifier sequence is exhausted",
        )
    })?;
    // Derive one stable effect identity from the command and monotonic effect sequence.
    // 从命令与单调 Effect 序列派生一个稳定 Effect 标识。
    let effect_id = EffectId::new(format!(
        "{}:effect:{:08x}",
        record.handle.command_id, record.next_effect_sequence
    ))
    .map_err(|error| {
        command_error(
            ErrorCode::Internal,
            format!("effect identifier generation failed: {error}"),
        )
    })?;
    record.pending_effect_id = Some(effect_id.clone());
    record.pending_mutation_idempotency_key = match &effect {
        HostEffect::StorageMutation(batch) => Some(batch.identity.idempotency_key.clone()),
        _ => None,
    };
    record.status = CommandStatus::Waiting;
    // Mutation retries require durable reconciliation and therefore are never host-retryable.
    // 变更重试需要持久对账，因此宿主绝不能直接重试。
    let side_effect_possible = matches!(&effect, HostEffect::StorageMutation(_));
    let request = EffectRequest {
        effect_id,
        command_id: record.handle.command_id.clone(),
        config_snapshot_id: record.handle.config_snapshot_id.clone(),
        operation_id: record
            .mutation_identity
            .as_ref()
            .map(|identity| identity.operation_id.clone()),
        deadline_class: effect_deadline_class(&record.state),
        retryable: !side_effect_possible,
        side_effect_possible,
        effect,
    };
    record.pending_effect = Some(request.clone());
    Ok(CommandStep::Effects(vec![request]))
}

/// Emits one structured LLM effect with its closed output contract bound.
/// 发出一个绑定封闭输出契约的结构化 LLM Effect。
fn emit_llm_effect(
    record: &mut CommandRecord,
    purpose: LlmPurpose,
    system_prompt: String,
    input: LlmInput,
) -> Result<CommandStep, CoreError> {
    let output_contract = StructuredOutputContractId::for_purpose(purpose);
    emit_effect(
        record,
        HostEffect::Llm(LlmEffect {
            purpose,
            output_contract,
            output_contract_version: output_contract.version(),
            output_contract_digest: output_contract.digest(),
            system_prompt,
            input,
        }),
    )
}

/// Maps private flow state to the public host deadline class.
/// 将私有流程状态映射为公共宿主截止时间类别。
const fn effect_deadline_class(state: &FlowState) -> EffectDeadlineClass {
    match state {
        FlowState::RetentionQuery(_)
        | FlowState::MaintenanceRebuildQuery(_)
        | FlowState::MaintenanceRebuildEmbedding { .. }
        | FlowState::MaintenanceMutation(_) => EffectDeadlineClass::Maintenance,
        FlowState::IntakeNoiseQuery(_)
        | FlowState::IntakeNoiseEmbed(_)
        | FlowState::IntakeNoiseStore { .. }
        | FlowState::IntakeNoiseScore { .. }
        | FlowState::IntakeTurn { .. }
        | FlowState::AnalyzeAttempt { .. }
        | FlowState::AnalyzeLoadTurn { .. }
        | FlowState::AnalyzeAnalysis { .. }
        | FlowState::AnalyzeEmbedding(_)
        | FlowState::AnalyzeProfileQuery(_)
        | FlowState::AnalyzeReview(_)
        | FlowState::AnalyzeDerived(_)
        | FlowState::AnalyzePersistenceEmbedding(_)
        | FlowState::MemoryWriteReview(_)
        | FlowState::MemoryWritePersistenceEmbedding { .. }
        | FlowState::MemoryWriteDedupe(_)
        | FlowState::MemoryWriteEmbedding(_)
        | FlowState::MemoryWriteMutation(_)
        | FlowState::MemoryWriteBatchDedupe(_)
        | FlowState::MemoryWriteBatchEmbedding(_)
        | FlowState::MemoryWriteBatchMutation(_)
        | FlowState::MemoryDeletePreflight(_)
        | FlowState::MemoryDeleteMutation
        | FlowState::MemoryDeleteBatchPreflight(_)
        | FlowState::MemoryDeleteBatchMutation(_)
        | FlowState::ProfileInstructionQuery(_)
        | FlowState::ProfileInstructionLlm { .. }
        | FlowState::ProfileInstructionMutation(_)
        | FlowState::MaintenanceLifecyclePreflight(_) => EffectDeadlineClass::Background,
        _ => EffectDeadlineClass::Interactive,
    }
}

/// Marks one command completed and returns its final typed result.
/// 将一个命令标记为完成并返回最终强类型结果。
#[allow(clippy::unnecessary_wraps)]
fn complete(record: &mut CommandRecord, result: CommandResult) -> Result<CommandStep, CoreError> {
    record.status = CommandStatus::Completed;
    record.state = FlowState::Terminal;
    record.pending_effect_id = None;
    record.pending_effect = None;
    record.pending_mutation_idempotency_key = None;
    Ok(CommandStep::Complete(Box::new(result)))
}

/// Validates bounded diagnostics carried by every Host result before state advancement.
/// 在推进状态前校验每个 Host 结果携带的有界诊断文本。
fn validate_host_result_envelope(result: &HostEffectResult) -> Result<(), CoreError> {
    let diagnostic = match result {
        HostEffectResult::Failure(failure) => Some(failure.diagnostic.as_str()),
        HostEffectResult::StorageMutation(MutationOutcome::Rejected { reason }) => {
            Some(reason.as_str())
        }
        HostEffectResult::Llm(_)
        | HostEffectResult::Embedding(_)
        | HostEffectResult::Rerank(_)
        | HostEffectResult::StorageQuery(_)
        | HostEffectResult::StorageMutation(
            MutationOutcome::Committed { .. } | MutationOutcome::Uncertain,
        ) => None,
    };
    if diagnostic.is_some_and(|value| {
        normalize_text(value).is_empty() || value.len() > 8_192 || value.contains('\0')
    }) {
        return Err(effect_type_error(
            "Host result diagnostic violates its bounded text contract",
        ));
    }
    Ok(())
}

/// Extracts a typed LLM result.
/// 提取强类型 LLM 结果。
fn expect_llm(result: HostEffectResult) -> Result<LlmOutput, CoreError> {
    match result {
        HostEffectResult::Llm(output) => Ok(output),
        _ => Err(effect_type_error("LLM result")),
    }
}

/// Extracts and validates one batch embedding result.
/// 提取并校验一次批量嵌入结果。
fn expect_embeddings(
    result: HostEffectResult,
    expected_count: usize,
    expected_dimension: u32,
) -> Result<Vec<Vec<f32>>, CoreError> {
    let expected_dimension = usize::try_from(expected_dimension)
        .map_err(|_| effect_type_error("embedding dimension"))?;
    match result {
        HostEffectResult::Embedding(batch)
            if batch.dropped.is_empty()
                && (batch.result_indices.is_empty()
                    || batch
                        .result_indices
                        .iter()
                        .copied()
                        .eq(0..u32::try_from(expected_count).unwrap_or(u32::MAX)))
                && batch.vectors.len() == expected_count
                && batch.vectors.iter().all(|vector| {
                    vector.len() == expected_dimension
                        && vector.iter().all(|value| value.is_finite())
                }) =>
        {
            Ok(batch.vectors)
        }
        HostEffectResult::Embedding(_) => Err(effect_type_error(
            "embedding result count, dimension, or vector values",
        )),
        _ => Err(effect_type_error("embedding result")),
    }
}

/// Extracts and validates one batch rerank result.
/// 提取并校验一次批量重排结果。
fn expect_rerank(
    result: HostEffectResult,
    expected_count: usize,
) -> Result<Vec<vulcan_memorymesh_core_contracts::RerankEntry>, CoreError> {
    match result {
        HostEffectResult::Rerank(entries)
            if entries.len() <= MAX_DOMAIN_COLLECTION_ITEMS
                && entries.iter().all(|entry| {
                    (entry.input_index as usize) < expected_count && entry.score.is_finite()
                }) =>
        {
            Ok(entries)
        }
        HostEffectResult::Rerank(_) => Err(effect_type_error(
            "rerank occurrence capacity, input index, or score values",
        )),
        _ => Err(effect_type_error("rerank result")),
    }
}

/// Extracts one typed storage query result.
/// 提取一个强类型存储查询结果。
fn expect_storage_query(result: HostEffectResult) -> Result<StorageQueryResult, CoreError> {
    match result {
        HostEffectResult::StorageQuery(output) => Ok(output),
        _ => Err(effect_type_error("storage query result")),
    }
}

/// Extracts one typed storage mutation result.
/// 提取一个强类型存储变更结果。
fn expect_mutation(result: HostEffectResult) -> Result<MutationOutcome, CoreError> {
    match result {
        HostEffectResult::StorageMutation(output) => Ok(output),
        _ => Err(effect_type_error("storage mutation result")),
    }
}

/// Returns the non-empty provider-facing document indexes inside the rerank prefix.
/// 返回重排前缀内非空的面向 Provider 文档索引。
fn rerank_candidate_indices(hits: &[SearchHit], top_n: usize) -> Vec<usize> {
    hits.iter()
        .take(top_n)
        .enumerate()
        .filter_map(|(index, hit)| (!rerank_document_text(&hit.memory).is_empty()).then_some(index))
        .collect()
}

/// Builds the frozen VMM rerank document from distinct non-empty summary and details text.
/// 从互异的非空摘要与详情文本构建冻结 VMM 重排文档。
fn rerank_document_text(memory: &MemoryRecord) -> String {
    let summary = memory.summary.trim();
    let details = memory.content.trim();
    match (summary.is_empty(), details.is_empty(), summary == details) {
        (true, true, _) => String::new(),
        (false, true, _) | (false, false, true) => summary.to_owned(),
        (true, false, _) => details.to_owned(),
        (false, false, false) => format!("{summary}\n{details}"),
    }
}

/// Applies provider scores as replacements to the exact reranked subset and preserves the tail.
/// 将 Provider 分值作为替换值应用到精确重排子集，并保留尾部候选。
fn apply_rerank_scores(
    hits: Vec<SearchHit>,
    rerank_indices: &[usize],
    entries: Vec<vulcan_memorymesh_core_contracts::RerankEntry>,
) -> Result<Vec<SearchHit>, CoreError> {
    if rerank_indices.windows(2).any(|pair| pair[0] >= pair[1])
        || rerank_indices
            .last()
            .is_some_and(|index| *index >= hits.len())
    {
        return Err(effect_type_error(
            "rerank indexes and scores are not exactly aligned",
        ));
    }
    let mut reranked_set = BTreeSet::new();
    let mut reranked = entries
        .into_iter()
        .map(|entry| {
            let index = *rerank_indices
                .get(entry.input_index as usize)
                .ok_or_else(|| effect_type_error("rerank input index is unavailable"))?;
            reranked_set.insert(index);
            let mut hit = hits
                .get(index)
                .cloned()
                .ok_or_else(|| effect_type_error("rerank candidate index is unavailable"))?;
            let score = entry.score.clamp(0.0, 1.0);
            hit.rerank_score = Some(score);
            hit.final_score = score;
            if let Some(explanation) = hit.explanation.as_mut() {
                explanation.rerank_score = Some(score);
                explanation.final_score = score;
            }
            Ok(hit)
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    // The provider's sequence is authoritative even for ties, nonmonotonic scores, or repeated documents.
    // 提供方序列具有权威，包括同分、非单调分值和重复文档；不能在这里按分值或业务身份重排。
    reranked.extend(
        hits.into_iter()
            .enumerate()
            .filter_map(|(index, hit)| (!reranked_set.contains(&index)).then_some(hit)),
    );
    Ok(reranked)
}

/// Applies the frozen post-rerank lifecycle, context, MMR, and final truncation stages.
/// 应用冻结的重排后生命周期、情境、MMR 与最终截断阶段。
fn finish_scored_pipeline(
    record: &mut CommandRecord,
    pipeline: SearchPipeline,
    mut hits: Vec<SearchHit>,
) -> Result<CommandStep, CoreError> {
    let policy = record.snapshot.memory_policy.retrieval_policy.clone();
    apply_context_scores(&pipeline.work, &policy, &mut hits);
    let hard_dedupe_pool = hits
        .iter()
        .take(
            policy
                .hard_dedupe_pool_top_k
                .max(pipeline.work.limit)
                .min(policy.candidate_pool_max) as usize,
        )
        .map(|hit| hit.memory.clone())
        .collect();
    let selected = retrieval_kernel::mmr_select(hits, pipeline.work.limit as usize, &policy.mmr);
    finish_pipeline(record, pipeline, selected, hard_dedupe_pool)
}

/// Applies the frozen VMM decay rules before issuing the live evidence read.
/// 在发出实时证据读取之前按冻结VMM规则衰减；参数是检索事实、策略及原地更新的命中，不返回值。
#[allow(clippy::float_cmp)]
fn apply_lifecycle_scores(
    work: &SearchWork,
    policy: &vulcan_memorymesh_core_contracts::RetrievalPolicy,
    hits: &mut [SearchHit],
) {
    // VMM skips the decay stage for a single candidate but still exposes a neutral multiplier.
    // VMM 对单候选跳过衰减阶段，但仍暴露中性倍率。
    let apply_decay = policy.decay.enabled && hits.len() > 1;
    let mut decay_changes_order = false;
    for hit in hits.iter_mut() {
        let decay = if apply_decay {
            retrieval_kernel::weibull_multiplier(&hit.memory, &policy.decay, work.now_ns)
        } else {
            1.0
        };
        let next_score = (hit.final_score * decay).clamp(0.0, 1.0);
        // This source-owned trigger is a business ordering rule, not a cross-language acceptance tolerance.
        // 该来源触发值是业务排序规则，不是跨语言验收误差；微小衰减仍写回分值但不能触发全局重排。
        decay_changes_order |= apply_decay && (next_score - hit.final_score).abs() > 1e-9;
        hit.final_score = next_score;
        if let Some(explanation) = hit.explanation.as_mut() {
            explanation.decay_multiplier = Some(decay);
            explanation.final_score = hit.final_score;
        }
    }
    if decay_changes_order {
        hits.sort_by(|left, right| {
            if left.final_score == right.final_score {
                left.memory.id.cmp(&right.memory.id)
            } else {
                right.final_score.total_cmp(&left.final_score)
            }
        });
    }
}

/// Applies confirmed context facts after decay without replaying preceding score stages.
/// 衰减后应用已确认情境事实，不重放之前评分阶段；参数是检索事实、策略及原地更新的命中，不返回值。
#[allow(clippy::float_cmp)]
fn apply_context_scores(
    work: &SearchWork,
    policy: &vulcan_memorymesh_core_contracts::RetrievalPolicy,
    hits: &mut [SearchHit],
) {
    let query = work.queries.join(" ");
    let mut context_has_match = false;
    for hit in hits.iter_mut() {
        let evidence = work
            .evidence
            .iter()
            .find(|group| group.memory_id == hit.memory.id)
            .map_or(
                &[] as &[vulcan_memorymesh_core_contracts::MemoryContextEvidence],
                |group| group.evidence.as_slice(),
            );
        let context = retrieval_kernel::context_score(evidence, &query, &policy.context);
        context_has_match |= !context.matched_values.is_empty();
        // Source ordering consumes the unsaturated score; clamping before sorting invents ties at both boundaries.
        // 来源排序消费尚未饱和的分值；先钳制会在上下边界制造平局，错误触发证据次级排序。
        hit.final_score += context.delta;
        if let Some(explanation) = hit.explanation.as_mut() {
            explanation.context_delta = Some(context.delta);
            explanation.context_matched_values = context.matched_values;
            explanation.context_matched_support_count = context.matched_support_count;
            explanation.context_matched_rebuttal_count = context.matched_rebuttal_count;
            explanation.context_total_support_count = context.total_support_count;
            explanation.context_total_rebuttal_count = context.total_rebuttal_count;
            explanation.final_score = hit.final_score;
        }
    }
    // A matching context row triggers the stable ordering even when its bounded net delta is zero.
    // 匹配情境行即触发稳定排序，即使其有界净增量为零；完全未匹配时保持上一阶段顺序。
    if context_has_match {
        hits.sort_by(|left, right| {
            let score_order = if left.final_score == right.final_score {
                std::cmp::Ordering::Equal
            } else {
                right.final_score.total_cmp(&left.final_score)
            };
            score_order
                .then_with(|| {
                    let left = left.explanation.as_ref();
                    let right = right.explanation.as_ref();
                    let left_net = left.map_or(0_i64, |value| {
                        i64::from(value.context_matched_support_count)
                            - i64::from(value.context_matched_rebuttal_count)
                    });
                    let right_net = right.map_or(0_i64, |value| {
                        i64::from(value.context_matched_support_count)
                            - i64::from(value.context_matched_rebuttal_count)
                    });
                    right_net.cmp(&left_net)
                })
                .then_with(|| {
                    let left = left
                        .explanation
                        .as_ref()
                        .map_or(0, |value| value.context_matched_support_count);
                    let right = right
                        .explanation
                        .as_ref()
                        .map_or(0, |value| value.context_matched_support_count);
                    right.cmp(&left)
                })
                .then_with(|| {
                    let left = left
                        .explanation
                        .as_ref()
                        .map_or(0, |value| value.context_matched_rebuttal_count);
                    let right = right
                        .explanation
                        .as_ref()
                        .map_or(0, |value| value.context_matched_rebuttal_count);
                    left.cmp(&right)
                })
        });
    }
    // Public scores and explanation values are bounded only after the source-authoritative ordering has completed.
    // 来源权威排序完成后才钳制公开评分及解释，保留上一步已经决定的顺序。
    for hit in hits {
        hit.final_score = hit.final_score.clamp(0.0, 1.0);
        if let Some(explanation) = hit.explanation.as_mut() {
            explanation.final_score = hit.final_score;
        }
    }
}

/// Verifies occurrence bounds independently of any SDK serialization or Provider-specific policy.
/// 独立于 SDK 序列化或提供方特定策略验证条目边界。
#[cfg(test)]
mod rerank_entry_tests {
    use super::expect_rerank;
    use vulcan_memorymesh_core_contracts::{
        HostEffectResult, MAX_DOMAIN_COLLECTION_ITEMS, RerankEntry,
    };

    /// Accepts ordered partial/empty/repeated facts but rejects invalid indexes, nonfinite scores and unbounded responses.
    /// 接受有序部分/空/重复事实，拒绝非法索引、非有限评分和无界响应。
    #[test]
    fn ordered_occurrence_validation_preserves_source_shape() {
        for entries in [
            Vec::new(),
            vec![RerankEntry {
                input_index: 1,
                score: -2.0,
            }],
            vec![
                RerankEntry {
                    input_index: 1,
                    score: 3.0,
                },
                RerankEntry {
                    input_index: 0,
                    score: 0.5,
                },
                RerankEntry {
                    input_index: 1,
                    score: 1.0,
                },
            ],
        ] {
            assert_eq!(
                expect_rerank(HostEffectResult::Rerank(entries.clone()), 2).ok(),
                Some(entries)
            );
        }
        for entry in [
            RerankEntry {
                input_index: 2,
                score: 0.5,
            },
            RerankEntry {
                input_index: u32::MAX,
                score: 0.5,
            },
            RerankEntry {
                input_index: 0,
                score: f64::NAN,
            },
            RerankEntry {
                input_index: 0,
                score: f64::INFINITY,
            },
        ] {
            assert!(expect_rerank(HostEffectResult::Rerank(vec![entry]), 2).is_err());
        }
        assert!(
            expect_rerank(
                HostEffectResult::Rerank(vec![
                    RerankEntry {
                        input_index: 0,
                        score: 1.0
                    };
                    MAX_DOMAIN_COLLECTION_ITEMS + 1
                ]),
                2
            )
            .is_err()
        );
    }
}

/// Normalizes and deduplicates searchable tags.
/// 规范化并去重可搜索标签。
fn normalize_tags(tags: &[String]) -> Vec<String> {
    // Track lowercase identities while preserving first normalized spelling.
    // 跟踪小写身份，同时保留首次规范化拼写。
    let mut seen = BTreeSet::new();
    tags.iter()
        .map(|tag| normalize_text(tag))
        .filter(|tag| !tag.is_empty())
        .filter(|tag| seen.insert(tag.to_lowercase()))
        .collect()
}

/// L2-normalizes one host embedding vector.
/// L2 规范化一个宿主嵌入向量。
fn normalize_vector(mut vector: Vec<f32>) -> Vec<f32> {
    let norm = vector
        .iter()
        .map(|value| f64::from(*value * *value))
        .sum::<f64>();
    if norm == 0.0 {
        return vector;
    }
    // Match the source's rounded square root and float32 division, including overflow/underflow.
    // 对齐来源的舍入平方根与单精度除法，包括上溢和下溢行为。
    #[allow(clippy::cast_possible_truncation)]
    let scale = norm.sqrt() as f32;
    for value in &mut vector {
        *value /= scale;
    }
    vector
}

/// Normalizes and scrubs every timeline item under one frozen snapshot.
/// 在一个冻结快照下规范化并脱敏每个时间线项目。
fn normalize_timeline(
    snapshot: &ConfigSnapshot,
    timeline: &[TimelineItem],
) -> Result<Vec<TimelineItem>, CoreError> {
    timeline
        .iter()
        .map(|item| {
            Ok(TimelineItem {
                role: item.role,
                content: snapshot.pii_scrubber.scrub(item.content.trim())?.text,
                occurred_at_ns: item.occurred_at_ns,
            })
        })
        .collect()
}

/// Builds a bounded evidence excerpt from one canonical turn.
/// 从一个规范化轮次构建有界证据摘录。
fn turn_excerpt(turn: &Turn) -> String {
    // Join role-labeled timeline content for audit-friendly evidence.
    // 拼接带角色标签的时间线内容以形成便于审计的证据。
    let joined = turn
        .timeline
        .iter()
        .map(|item| {
            // Use stable English role labels independent of locale.
            // 使用与区域无关的稳定英文角色标签。
            let role = match item.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
                Role::Tool => "tool",
            };
            format!("{role}: {}", item.content)
        })
        .collect::<Vec<_>>()
        .join(" | ");
    joined.chars().take(512).collect()
}

/// Maps one generic direct-memory ownership class to its durable stability semantics.
/// 将一个通用主动记忆归属类别映射到其持久稳定性语义。
const fn direct_memory_stability(scope_class: MemoryScopeClass) -> MemoryStability {
    match scope_class {
        MemoryScopeClass::Session => MemoryStability::Working,
        MemoryScopeClass::Environment | MemoryScopeClass::Subject => MemoryStability::Stable,
    }
}

/// Resolves the explicit or frozen default lifecycle level for one direct memory.
/// 解析一条主动记忆的显式生命周期等级或冻结默认等级。
const fn direct_memory_level(
    scope_class: MemoryScopeClass,
    requested: Option<MemoryLevel>,
) -> MemoryLevel {
    match scope_class {
        MemoryScopeClass::Session => MemoryLevel::Session,
        MemoryScopeClass::Environment | MemoryScopeClass::Subject => match requested {
            Some(level) => level,
            None => MemoryLevel::Stable,
        },
    }
}

/// Computes the versioned Core-owned soft-idempotency hash for one normalized direct write.
/// 为一条已规范化主动写入计算版本化 Core 自有软幂等哈希。
fn direct_write_dedupe_hash(
    snapshot: &ConfigSnapshot,
    item: &MemoryWriteBatchItem,
    session_source_id: &vulcan_memorymesh_core_contracts::SessionSourceId,
) -> String {
    let priority = item.priority.unwrap_or(20);
    let level = direct_memory_level(item.scope_class, item.memory_level);
    let expiry_signature = item.expires_at_ns.map_or_else(
        || {
            let lifetime_ms = match item.scope_class {
                MemoryScopeClass::Session => snapshot.memory_policy.direct_memory_session_expiry_ms,
                MemoryScopeClass::Environment => {
                    snapshot.memory_policy.direct_memory_environment_expiry_ms
                }
                MemoryScopeClass::Subject => snapshot.memory_policy.direct_memory_subject_expiry_ms,
            };
            format!("ttl-ms:{lifetime_ms}")
        },
        |expires_at_ns| format!("at-ns:{expires_at_ns}"),
    );
    let material = [
        "vmm-core-direct-dedupe-v1".to_owned(),
        (item.scope_class as u32).to_string(),
        session_source_id.as_str().to_owned(),
        normalize_hash_text(&item.summary),
        normalize_hash_text(&item.details),
        (item.category as u32).to_string(),
        priority.to_string(),
        (level as u32).to_string(),
        expiry_signature,
    ]
    .join("\n");
    format!(
        "v1:{:x}",
        <sha2::Sha256 as sha2::Digest>::digest(material.as_bytes())
    )
}

/// Validates one Host-supplied recent direct-write match against the frozen query facts.
/// 根据冻结查询事实校验一条 Host 提供的近期主动写入命中。
#[allow(clippy::too_many_arguments)]
fn validate_recent_direct_write_hit(
    snapshot: &ConfigSnapshot,
    subject_id: &SubjectId,
    session_source_id: &vulcan_memorymesh_core_contracts::SessionSourceId,
    occurred_at_ns: i64,
    item: &MemoryWriteBatchItem,
    dedupe_hash: &str,
    existing: &MemoryRecord,
) -> Result<(), CoreError> {
    validate_host_memory_record(existing, None)?;
    let expected_priority = item.priority.unwrap_or(20);
    let expected_level = direct_memory_level(item.scope_class, item.memory_level);
    let lower_bound = occurred_at_ns.saturating_sub(DIRECT_WRITE_DEDUPE_WINDOW_NS);
    let unexpired = existing
        .expires_at_ns
        .is_none_or(|expires_at_ns| expires_at_ns > occurred_at_ns);
    if existing.access_class != MemoryAccessClass::SubjectPrivate
        || existing.source_subject_id.as_ref() != Some(subject_id)
        || existing.scope_id != item.target_scope_id
        || existing.source_kind != MemorySourceKind::Direct
        || existing.origin_session_source_id.as_ref() != Some(session_source_id)
        || existing.category != item.category
        || normalize_hash_text(&existing.summary) != normalize_hash_text(&item.summary)
        || normalize_hash_text(&existing.content) != normalize_hash_text(&item.details)
        || existing.priority != expected_priority
        || existing.memory_level != expected_level
        || existing.stability != direct_memory_stability(item.scope_class)
        || existing.dedupe_hash.as_deref() != Some(dedupe_hash)
        || existing.state != MemoryState::Active
        || existing.created_at_ns < lower_bound
        || existing.created_at_ns > occurred_at_ns
        || !unexpired
    {
        return Err(effect_type_error(
            "recent direct-write dedupe hit violates subject, scope, session, content, lifecycle, hash, state, or time bounds",
        ));
    }
    let expected_expiry = direct_memory_expiry(
        snapshot,
        item.scope_class,
        item.expires_at_ns,
        occurred_at_ns,
    )?;
    if item.expires_at_ns.is_some() && existing.expires_at_ns != expected_expiry {
        return Err(effect_type_error(
            "recent direct-write dedupe hit violates explicit expiration identity",
        ));
    }
    Ok(())
}

/// Resolves one direct-memory expiration from an explicit value or the frozen scope policy.
/// 从显式值或冻结的作用域策略解析一条主动记忆的过期时间。
fn direct_memory_expiry(
    snapshot: &ConfigSnapshot,
    scope_class: MemoryScopeClass,
    requested: Option<i64>,
    occurred_at_ns: i64,
) -> Result<Option<i64>, CoreError> {
    if requested.is_some() {
        return Ok(requested);
    }
    let lifetime_ms = match scope_class {
        MemoryScopeClass::Session => snapshot.memory_policy.direct_memory_session_expiry_ms,
        MemoryScopeClass::Environment => snapshot.memory_policy.direct_memory_environment_expiry_ms,
        MemoryScopeClass::Subject => snapshot.memory_policy.direct_memory_subject_expiry_ms,
    };
    checked_expiry_from_millis(occurred_at_ns, lifetime_ms).map(Some)
}

/// Adds a finite millisecond policy lifetime to a nanosecond source instant.
/// 把有限毫秒策略时长加到纳秒来源瞬间。
fn checked_expiry_from_millis(base_ns: i64, lifetime_millis: u64) -> Result<i64, CoreError> {
    let added_nanos = checked_duration_ns(lifetime_millis, "finite lifetime")?;
    base_ns
        .checked_add(added_nanos)
        .ok_or_else(|| validation_error("finite expiration overflows timestamp range"))
}

/// Maps public priority to a deterministic legacy-compatible importance band.
/// 将公共优先级映射为确定性的旧版兼容重要度档位。
const fn priority_to_importance(priority: u32) -> f32 {
    if priority >= 80 {
        0.8
    } else if priority >= 50 {
        0.5
    } else {
        0.2
    }
}

/// Validates all command fields and scope contracts before publishing a handle.
/// 在发布句柄前校验全部命令字段与作用域契约。
#[allow(clippy::too_many_lines)]
fn validate_command(command: &CoreCommand, snapshot: &ConfigSnapshot) -> Result<(), CoreError> {
    let policy = &snapshot.memory_policy.retrieval_policy;
    match command {
        CoreCommand::PreCheck(command) => {
            validate_scope(&command.scope_access)?;
            command
                .candidate_scope
                .validate(&command.scope_access)
                .map_err(validation_error)?;
            validate_nonempty_text(&command.user_content, "PreCheck user content")?;
            validate_timeline(&command.recent_timeline, true)?;
            validate_observed_time(
                command.now_ns,
                command.mutation_identity.observed_at_ns,
                "PreCheck evaluation time",
            )?;
            if !(1..=policy.max_query_items).contains(&command.max_search_queries)
                || !(1..=policy.top_k_max).contains(&command.max_memories)
            {
                return Err(validation_error(
                    "PreCheck limits are outside supported ranges",
                ));
            }
        }
        CoreCommand::PostActionIntake(command) => {
            validate_scope(&command.scope_access)?;
            validate_timeline(&command.timeline, false)?;
            if !command.timeline.iter().any(|item| item.role == Role::User) {
                return Err(validation_error("intake timeline requires one user item"));
            }
            if command.input_kind
                == vulcan_memorymesh_core_contracts::PostActionInputKind::SingleTurn
                && !matches!(
                    command.timeline.as_slice(),
                    [TimelineItem {
                        role: Role::User,
                        ..
                    }] | [
                        TimelineItem {
                            role: Role::User,
                            ..
                        },
                        TimelineItem {
                            role: Role::Assistant,
                            ..
                        }
                    ]
                )
            {
                return Err(validation_error(
                    "single-turn intake requires user and optional final assistant only",
                ));
            }
            if command
                .timeline
                .last()
                .is_some_and(|item| item.occurred_at_ns > command.completed_at_ns)
                || command.completed_at_ns > command.mutation_identity.observed_at_ns
            {
                return Err(validation_error(
                    "PostAction intake timestamps exceed their completion or observation boundary",
                ));
            }
        }
        CoreCommand::PostActionAnalyzePendingTurn(command) => {
            validate_scope(&command.scope_access)?;
            command
                .replacement_scope
                .validate(&command.scope_access)
                .map_err(validation_error)?;
            if command.analysis_generation == 0 {
                return Err(validation_error(
                    "analysis generation must be a positive durable counter",
                ));
            }
            validate_observed_time(
                command.analysis_cutoff_ns,
                command.mutation_identity.observed_at_ns,
                "analysis cutoff",
            )?;
            if command.direct_write_observed_after_ns > command.analysis_cutoff_ns {
                return Err(validation_error(
                    "direct-write watermark must not exceed the analysis cutoff",
                ));
            }
            validate_profile_sections(&command.scope_access, &command.profile_sections, true)?;
        }
        CoreCommand::ChatCompact(command) => {
            validate_scope(&command.scope_access)?;
            validate_observed_time(
                command.compacted_at_ns,
                command.mutation_identity.observed_at_ns,
                "chat compact event time",
            )?;
        }
        CoreCommand::MemorySearch(command) => {
            validate_scope(&command.scope_access)?;
            if let Some(candidate_scope) = &command.boundary.candidate_scope {
                candidate_scope
                    .validate(&command.scope_access)
                    .map_err(validation_error)?;
            }
            validate_nonempty_text(&command.query, "memory search query")?;
            if command.limit > policy.top_k_max {
                return Err(validation_error("memory search limit is invalid"));
            }
        }
        CoreCommand::MemoryDetails(command) => validate_scope(&command.scope_access)?,
        CoreCommand::MemoryWrite(command) => {
            validate_scope(&command.scope_access)?;
            command
                .replacement_scope
                .validate(&command.scope_access)
                .map_err(validation_error)?;
            validate_observed_time(
                command.occurred_at_ns,
                command.mutation_identity.observed_at_ns,
                "memory write event time",
            )?;
            validate_direct_memory_item(&command.item, command.occurred_at_ns)?;
            if command.item.target_scope_id != command.scope_access.write_scope {
                return Err(validation_error(
                    "single memory write target must equal the command write scope",
                ));
            }
        }
        CoreCommand::MemoryDelete(command) => validate_scope(&command.scope_access)?,
        CoreCommand::ProfileQuery(command) => {
            validate_scope(&command.scope_access)?;
            validate_profile_sections(&command.scope_access, &command.view.sections, false)?;
            parse_profile_timezone(&command.view.display_timezone)?;
        }
        CoreCommand::ProfileInstruction(command) => {
            validate_scope(&command.scope_access)?;
            validate_nonempty_text(&command.instruction, "profile instruction")?;
            validate_observed_time(
                command.occurred_at_ns,
                command.mutation_identity.observed_at_ns,
                "profile instruction event time",
            )?;
            validate_profile_sections(
                &command.scope_access,
                std::slice::from_ref(&command.target_section),
                true,
            )?;
            if command.target_section.scope_id != command.scope_access.write_scope {
                return Err(validation_error(
                    "profile instruction target section must equal the write scope",
                ));
            }
        }
        CoreCommand::Retention(command) => {
            validate_scope(&command.scope_access)?;
            validate_observed_time(
                command.now_ns,
                command.mutation_identity.observed_at_ns,
                "retention evaluation time",
            )?;
            let limits = [
                command.policy.memory_recycle_limit,
                command.policy.cold_turn_scan_limit,
                command.policy.cold_turn_job_limit,
                command.policy.idle_session_limit,
                command.policy.vector_gc_limit,
                command.policy.trash_purge_limit,
            ];
            if command.policy.schema_version != 1
                || command.policy.turn_hot_window_size == 0
                || command.policy.session_idle_ms == 0
                || command.policy.trash_retention_ms == 0
                || command.policy.protected_priority_floor > 100
                || !(1..=MemoryLevel::Persistent as u32)
                    .contains(&command.policy.protected_memory_level_floor)
                || limits.iter().all(|limit| *limit == 0)
                || limits.iter().any(|limit| *limit > 10_000)
            {
                return Err(validation_error("retention policy values are invalid"));
            }
        }
        CoreCommand::MemorySearchBatch(command) => {
            validate_scope(&command.scope_access)?;
            if let Some(candidate_scope) = &command.boundary.candidate_scope {
                candidate_scope
                    .validate(&command.scope_access)
                    .map_err(validation_error)?;
            }
            if command.queries.is_empty()
                || command.queries.len() > policy.max_query_items as usize
                || command.top_k > policy.top_k_max
            {
                return Err(validation_error("memory search batch limits are invalid"));
            }
            for query in &command.queries {
                validate_nonempty_text(query, "memory search batch query")?;
            }
        }
        CoreCommand::MemoryDetailsBatch(command) => {
            validate_scope(&command.scope_access)?;
            let count = command
                .memory_ids
                .len()
                .checked_add(command.turn_ids.len())
                .ok_or_else(|| validation_error("memory details batch size overflow"))?;
            if count == 0 || count > 1024 {
                return Err(validation_error("memory details batch size is invalid"));
            }
            validate_unique_ids(
                command.memory_ids.iter().map(ToString::to_string),
                "memory details batch memory identities",
            )?;
            validate_unique_ids(
                command.turn_ids.iter().map(ToString::to_string),
                "memory details batch turn identities",
            )?;
        }
        CoreCommand::MemoryWriteBatch(command) => {
            validate_scope(&command.scope_access)?;
            command
                .replacement_scope
                .validate(&command.scope_access)
                .map_err(validation_error)?;
            validate_observed_time(
                command.occurred_at_ns,
                command.mutation_identity.observed_at_ns,
                "memory write batch event time",
            )?;
            if command.items.is_empty() || command.items.len() > 256 {
                return Err(validation_error("memory write batch size is invalid"));
            }
            if command.authorized_write_scopes.is_empty() {
                return Err(validation_error(
                    "memory write batch requires explicit authorized write scopes",
                ));
            }
            let authorized = command
                .authorized_write_scopes
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            if authorized.len() != command.authorized_write_scopes.len()
                || !authorized.contains(&command.scope_access.write_scope)
                || authorized
                    .iter()
                    .any(|scope| !command.scope_access.visible_scopes.contains(scope))
            {
                return Err(validation_error(
                    "memory write batch authorized scopes are duplicated or outside visible bounds",
                ));
            }
            for item in &command.items {
                validate_direct_memory_item(item, command.occurred_at_ns)?;
                if !authorized.contains(&item.target_scope_id) {
                    return Err(validation_error(
                        "memory write batch item targets an unauthorized scope",
                    ));
                }
            }
        }
        CoreCommand::MemoryDeleteBatch(command) => {
            validate_scope(&command.scope_access)?;
            validate_nonempty_text(&command.reason, "memory delete batch reason")?;
            if command.items.is_empty() || command.items.len() > 256 {
                return Err(validation_error("memory delete batch size is invalid"));
            }
            validate_unique_ids(
                command.items.iter().map(|item| item.memory_id.to_string()),
                "memory delete batch identities",
            )?;
        }
        CoreCommand::Maintenance(command) => {
            validate_scope(&command.scope_access)?;
            validate_observed_time(
                command.occurred_at_ns,
                command.mutation_identity.observed_at_ns,
                "maintenance event time",
            )?;
            let (identities, name) = match &command.action {
                MaintenanceAction::RestoreRecycled(items)
                | MaintenanceAction::PurgeRecycled(items) => {
                    if items.is_empty() || items.len() > 256 {
                        return Err(validation_error(
                            "maintenance lifecycle batch size is invalid",
                        ));
                    }
                    (
                        items
                            .iter()
                            .map(|item| item.memory_id.to_string())
                            .collect::<Vec<_>>(),
                        "maintenance lifecycle identities",
                    )
                }
                MaintenanceAction::RebuildVectors { memory_ids, .. } => {
                    if memory_ids.is_empty() || memory_ids.len() > 256 {
                        return Err(validation_error(
                            "maintenance rebuild batch size is invalid",
                        ));
                    }
                    (
                        memory_ids.iter().map(ToString::to_string).collect(),
                        "maintenance rebuild identities",
                    )
                }
            };
            validate_unique_ids(identities, name)?;
        }
    }
    Ok(())
}

/// Validates that one business event or evaluation time was already observable by the host.
/// 校验一个业务事件或评估时间在宿主观测时已经发生。
fn validate_observed_time(
    business_time_ns: i64,
    observed_at_ns: i64,
    field_name: &str,
) -> Result<(), CoreError> {
    if business_time_ns > observed_at_ns {
        return Err(validation_error(format!(
            "{field_name} exceeds the mutation observation boundary"
        )));
    }
    Ok(())
}

/// Validates one direct-memory item before embedding or persistence.
/// 在嵌入或持久化前校验一条主动记忆项。
fn validate_direct_memory_item(
    item: &vulcan_memorymesh_core_contracts::MemoryWriteBatchItem,
    occurred_at_ns: i64,
) -> Result<(), CoreError> {
    validate_nonempty_text(&item.summary, "direct memory summary")?;
    validate_nonempty_text(&item.details, "direct memory details")?;
    validate_input_tags(&item.tags)?;
    if item.priority.is_some_and(|priority| priority > 100) {
        return Err(validation_error("direct memory priority is invalid"));
    }
    if item
        .expires_at_ns
        .is_some_and(|expires_at_ns| expires_at_ns <= occurred_at_ns)
    {
        return Err(validation_error(
            "direct memory expiration must follow event time",
        ));
    }
    Ok(())
}

/// Validates the resource bounds of caller-supplied searchable tags before normalization.
/// 在规范化前校验调用方提供的可搜索标签资源边界。
fn validate_input_tags(tags: &[String]) -> Result<(), CoreError> {
    if tags.len() > MAX_MEMORY_TAGS
        || tags
            .iter()
            .any(|tag| tag.len() > MAX_MEMORY_TAG_BYTES || tag.contains('\0'))
    {
        return Err(validation_error(
            "memory tags exceed the supported count or UTF-8 byte length",
        ));
    }
    Ok(())
}

/// Verifies one mutating command's host identity and canonical request fingerprint.
/// 校验一个变更命令的宿主身份与规范请求指纹。
fn verify_identity(
    command: &CoreCommand,
    snapshot: &ConfigSnapshot,
) -> Result<Option<MutationIdentity>, CoreError> {
    let Some(identity) = command.mutation_identity() else {
        return Ok(None);
    };
    identity.validate()?;
    vulcan_memorymesh_core_contracts::verify_fingerprint(
        command,
        snapshot.id.as_str(),
        &identity.request_fingerprint,
    )?;
    Ok(Some(identity.clone()))
}

/// Validates one stream of public identities for deterministic batch semantics.
/// 校验一组公共标识流以保证确定性批处理语义。
fn validate_unique_ids(
    identities: impl IntoIterator<Item = String>,
    name: &str,
) -> Result<(), CoreError> {
    let mut seen = BTreeSet::new();
    if identities
        .into_iter()
        .any(|identity| !seen.insert(identity))
    {
        return Err(validation_error(format!("{name} must be unique")));
    }
    Ok(())
}

/// Validates one explicit scope access contract.
/// 校验一个显式作用域访问契约。
fn validate_scope(scope_access: &ScopeAccess) -> Result<(), CoreError> {
    scope_access.validate().map_err(|diagnostic| {
        CoreError::new(
            ErrorCode::ScopeViolation,
            ErrorStage::Validation,
            diagnostic,
        )
    })
}

/// Validates one required text value without rewriting it.
/// 校验一个必需文本值且不改写它。
fn validate_nonempty_text(value: &str, name: &str) -> Result<(), CoreError> {
    if normalize_text(value).is_empty() {
        return Err(validation_error(format!("{name} must not be empty")));
    }
    if value.len() > MAX_DOMAIN_TEXT_BYTES {
        return Err(validation_error(format!("{name} exceeds 4 MiB")));
    }
    if value.contains('\0') {
        return Err(validation_error(format!("{name} contains NUL")));
    }
    Ok(())
}

/// Validates one Host-owned identifier-shaped string that has no dedicated public newtype.
/// 校验一个尚无专用公共新类型的宿主自有标识字符串。
fn validate_host_identifier_text(value: &str, name: &str) -> Result<(), CoreError> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.len() > MAX_HOST_IDENTIFIER_BYTES
        || value.contains('\0')
    {
        return Err(effect_type_error(format!(
            "{name} must be non-empty, trimmed, NUL-free, and at most {MAX_HOST_IDENTIFIER_BYTES} UTF-8 bytes"
        )));
    }
    Ok(())
}

/// Validates one Host-supplied durable memory record before Core uses or returns it.
/// 在 Core 使用或返回一条宿主持久记忆记录前校验其领域不变量。
fn validate_host_memory_record(
    memory: &MemoryRecord,
    expected_vector: Option<(&VectorSpaceId, u32)>,
) -> Result<(), CoreError> {
    // Existing VMM tables permit empty text; read validity is distinct from admission of a new write.
    // 既有 VMM 表允许空文本；读取合法性与新建写入准入分离，不能为重排补造正文或拒绝整批历史读取。
    if !memory.has_valid_access_identity()
        || memory.summary.len() > MAX_DOMAIN_TEXT_BYTES
        || memory.content.len() > MAX_DOMAIN_TEXT_BYTES
        || memory.summary.contains('\0')
        || memory.content.contains('\0')
        || memory.priority > 100
        || !(0.0..=1.0).contains(&memory.importance)
        || memory
            .confidence
            .is_some_and(|confidence| !(0.0..=1.0).contains(&confidence))
        || memory.version == 0
        || memory.updated_at_ns < memory.created_at_ns
        || memory
            .expires_at_ns
            .is_some_and(|expires_at_ns| expires_at_ns <= 0)
        || memory.evidence.len() > MAX_DOMAIN_COLLECTION_ITEMS
        || memory.evidence.iter().any(|evidence| {
            normalize_text(&evidence.excerpt).is_empty()
                || evidence.excerpt.len() > MAX_DOMAIN_TEXT_BYTES
                || evidence.excerpt.contains('\0')
                || evidence.occurred_at_ns > memory.updated_at_ns
        })
        || memory.tags.len() > MAX_MEMORY_TAGS
        || memory.tags.iter().any(|tag| {
            normalize_text(tag).is_empty() || tag.len() > MAX_MEMORY_TAG_BYTES || tag.contains('\0')
        })
        || invalid_optional_host_text(memory.evidence_source.as_deref(), MAX_DOMAIN_TEXT_BYTES)
        || invalid_optional_host_text(memory.dedupe_hash.as_deref(), MAX_MEMORY_TAG_BYTES)
        || invalid_optional_host_text(memory.status_reason.as_deref(), MAX_DOMAIN_TEXT_BYTES)
        || [
            memory.last_recalled_at_ns,
            memory.last_adopted_at_ns,
            memory.last_reinforced_at_ns,
        ]
        .into_iter()
        .flatten()
        .any(|timestamp| timestamp < memory.created_at_ns || timestamp > memory.updated_at_ns)
    {
        return Err(effect_type_error(
            "Host memory record violates text, score, version, time, or evidence bounds",
        ));
    }
    let unique_tags = memory
        .tags
        .iter()
        .map(|tag| normalize_text(tag).to_lowercase())
        .collect::<BTreeSet<_>>();
    if unique_tags.len() != memory.tags.len() {
        return Err(effect_type_error(
            "Host memory record contains duplicate normalized tags",
        ));
    }
    let unique_evidence = memory
        .evidence
        .iter()
        .map(|evidence| {
            (
                &evidence.turn_id,
                evidence.excerpt.as_str(),
                evidence.occurred_at_ns,
            )
        })
        .collect::<BTreeSet<_>>();
    if unique_evidence.len() != memory.evidence.len() {
        return Err(effect_type_error(
            "Host memory record contains duplicate evidence facts",
        ));
    }
    match (&memory.vector_space_id, &memory.embedding) {
        (None, None) if expected_vector.is_none() => Ok(()),
        (Some(vector_space_id), Some(embedding))
            if !embedding.is_empty()
                && embedding.len()
                    <= usize::try_from(vulcan_memorymesh_core_config::MAX_EMBEDDING_DIMENSION)
                        .unwrap_or(usize::MAX)
                && embedding.iter().all(|value| value.is_finite())
                && expected_vector.is_none_or(|(expected_space, expected_dimension)| {
                    vector_space_id == expected_space
                        && embedding.len()
                            == usize::try_from(expected_dimension).unwrap_or(usize::MAX)
                }) =>
        {
            Ok(())
        }
        _ => Err(effect_type_error(
            "Host memory record violates vector-space, dimension, or finite-value bounds",
        )),
    }
}

/// Validates one Host-supplied canonical turn before Core exposes it to callers.
/// 在 Core 向调用方暴露一条宿主规范轮次前校验其领域不变量。
fn validate_host_turn(turn: &Turn) -> Result<(), CoreError> {
    if turn.timeline.is_empty()
        || turn.timeline.len() > MAX_DOMAIN_COLLECTION_ITEMS
        || turn.timeline.iter().any(|item| {
            normalize_text(&item.content).is_empty()
                || item.content.len() > MAX_DOMAIN_TEXT_BYTES
                || item.content.contains('\0')
                || item.occurred_at_ns > turn.created_at_ns
        })
        || turn
            .timeline
            .windows(2)
            .any(|window| window[0].occurred_at_ns > window[1].occurred_at_ns)
        || turn.previous_turn_ids.len() > MAX_DOMAIN_COLLECTION_ITEMS
        || turn.next_turn_ids.len() > MAX_DOMAIN_COLLECTION_ITEMS
        || turn.analysis_details.as_ref().is_some_and(|details| {
            normalize_text(details).is_empty()
                || details.len() > MAX_DOMAIN_TEXT_BYTES
                || details.contains('\0')
        })
        || match turn.status {
            TurnStatus::Pending | TurnStatus::AnalysisBypassed | TurnStatus::Corrupted => {
                turn.user_input_kind.is_some() || turn.analysis_details.is_some()
            }
            TurnStatus::Completed => {
                turn.user_input_kind.is_none() || turn.analysis_details.is_none()
            }
            TurnStatus::ProcessedUnknown => turn.user_input_kind.is_some(),
        }
    {
        return Err(effect_type_error(
            "Host turn violates timeline, adjacency, time, or analysis-detail bounds",
        ));
    }
    let mut adjacent = BTreeSet::new();
    if turn
        .previous_turn_ids
        .iter()
        .chain(&turn.next_turn_ids)
        .any(|turn_id| turn_id == &turn.id || !adjacent.insert(turn_id))
    {
        return Err(effect_type_error(
            "Host turn adjacency contains a self-reference or duplicate identity",
        ));
    }
    Ok(())
}

/// Validates one Host-supplied profile node before review, rendering, or mutation.
/// 在审核、渲染或变更一条宿主画像节点前校验其领域不变量。
fn validate_host_profile_node(node: &ProfileNode) -> Result<(), CoreError> {
    if normalize_text(&node.content).is_empty()
        || node.content.len() > MAX_DOMAIN_TEXT_BYTES
        || node.content.contains('\0')
        || node
            .confidence
            .is_some_and(|confidence| !(0.0..=1.0).contains(&confidence))
        || node.priority > 100
        || node.version == 0
        || node.created_at_ns <= 0
        || node.updated_at_ns < node.created_at_ns
        || node.source_turn_ids.len() > MAX_DOMAIN_COLLECTION_ITEMS
        || node
            .evidence_source
            .as_ref()
            .is_some_and(|value| invalid_required_host_text(value, MAX_DOMAIN_TEXT_BYTES))
        || node
            .level_reason
            .as_ref()
            .is_some_and(|value| invalid_required_host_text(value, MAX_DOMAIN_TEXT_BYTES))
        || node
            .status_reason
            .as_ref()
            .is_some_and(|value| invalid_required_host_text(value, MAX_DOMAIN_TEXT_BYTES))
        || node.state != ProfileNodeState::Inactive && node.superseded_by.is_some()
        || node
            .effective_date_ns
            .zip(node.expires_at_ns)
            .is_some_and(|(effective, expires)| expires <= effective)
        || node.effective_date_ns.is_some_and(|value| value <= 0)
        || node.expires_at_ns.is_some_and(|value| value <= 0)
    {
        return Err(effect_type_error(
            "Host profile node violates text, score, priority, version, or source bounds",
        ));
    }
    let unique_sources = node.source_turn_ids.iter().collect::<BTreeSet<_>>();
    if unique_sources.len() != node.source_turn_ids.len()
        || node.superseded_by.as_ref() == Some(&node.id)
    {
        return Err(effect_type_error(
            "Host profile node contains duplicate sources or a self-supersede reference",
        ));
    }
    let source_is_valid = match node.source_kind {
        ProfileSourceKind::TurnExtract => {
            !node.source_turn_ids.is_empty()
                && node.source_id.as_ref().map_or_else(
                    || node.state == ProfileNodeState::Pending,
                    |source_id| {
                        node.source_turn_ids
                            .iter()
                            .any(|turn_id| turn_id.as_str() == source_id.as_str())
                    },
                )
        }
        ProfileSourceKind::ManualInstruction => {
            node.source_id.is_some() && node.source_turn_ids.is_empty()
        }
        ProfileSourceKind::SystemSeed => node.source_turn_ids.is_empty(),
        ProfileSourceKind::RetainedAfterSubjectDelete => {
            node.source_id.is_some()
                && node.source_turn_ids.is_empty()
                && node.status_reason.is_some()
        }
    };
    if !source_is_valid {
        return Err(effect_type_error(
            "Host profile node source kind, source identity, and source turns are inconsistent",
        ));
    }
    Ok(())
}

/// Reports whether one required Host text violates normalization, byte, or NUL bounds.
/// 报告一段必需 Host 文本是否违反规范化、字节或 NUL 边界。
fn invalid_required_host_text(value: &str, maximum_bytes: usize) -> bool {
    normalize_text(value).is_empty() || value.len() > maximum_bytes || value.contains('\0')
}

/// Reports whether one optional Host text is present but violates required-text bounds.
/// 报告一段可选 Host 文本是否存在但违反必需文本边界。
fn invalid_optional_host_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_some_and(|value| invalid_required_host_text(value, maximum_bytes))
}

/// Validates timeline size, ordering, and required content.
/// 校验时间线大小、顺序与必需内容。
fn validate_timeline(timeline: &[TimelineItem], allow_empty: bool) -> Result<(), CoreError> {
    if (!allow_empty && timeline.is_empty()) || timeline.len() > MAX_DOMAIN_COLLECTION_ITEMS {
        return Err(validation_error("timeline size is invalid"));
    }
    // Track the preceding timestamp to reject ambiguous ordering.
    // 跟踪前一个时间戳以拒绝不明确顺序。
    let mut previous = None;
    for item in timeline {
        validate_nonempty_text(&item.content, "timeline content")?;
        if previous.is_some_and(|timestamp| item.occurred_at_ns < timestamp) {
            return Err(validation_error("timeline timestamps are not ordered"));
        }
        previous = Some(item.occurred_at_ns);
    }
    Ok(())
}

/// Builds the bound structured-output and score contract set for one checkpoint.
/// 为一个检查点构建绑定的结构化输出与评分契约集合。
fn checkpoint_contracts(
    command: &CoreCommand,
    snapshot: &ConfigSnapshot,
) -> vulcan_memorymesh_core_contracts::CheckpointContractSet {
    let structured_output_contracts = match command {
        CoreCommand::PreCheck(_) => vec![
            StructuredOutputContractId::PreCheckIntent.digest(),
            StructuredOutputContractId::PreCheckReview.digest(),
        ],
        CoreCommand::PostActionAnalyzePendingTurn(_) => vec![
            StructuredOutputContractId::PostActionAnalysis.digest(),
            StructuredOutputContractId::PostActionReview.digest(),
        ],
        CoreCommand::MemoryWrite(_) | CoreCommand::MemoryWriteBatch(_) => {
            vec![StructuredOutputContractId::PostActionReview.digest()]
        }
        CoreCommand::ProfileInstruction(_) => {
            vec![StructuredOutputContractId::ProfileInstruction.digest()]
        }
        _ => Vec::new(),
    };
    let retrieval = &snapshot.memory_policy.retrieval_policy;
    let mut score_contracts = vec![
        retrieval.vector_contract.contract.clone(),
        retrieval.lexical_contract.contract.clone(),
    ];
    if let Some(fallback) = &retrieval.separate_channel_fallback {
        for contract in [
            &fallback.vector_contract.contract,
            &fallback.lexical_contract.contract,
        ] {
            if !score_contracts.contains(contract) {
                score_contracts.push(contract.clone());
            }
        }
    }
    vulcan_memorymesh_core_contracts::CheckpointContractSet {
        structured_output_contracts,
        score_contracts,
    }
}

/// Reports one stable flow discriminator for checkpoint summaries.
/// 为检查点摘要报告一个稳定的流程判别名。
const fn flow_state_name(state: &FlowState) -> &'static str {
    match state {
        FlowState::PreCheckIntent { .. } => "PreCheckIntent",
        FlowState::PreCheckEmbedding { .. } => "PreCheckEmbedding",
        FlowState::SearchPipeline(_) => "SearchPipeline",
        FlowState::PreCheckReview { .. } => "PreCheckReview",
        FlowState::PreCheckReinforce(_) => "PreCheckReinforce",
        FlowState::IntakeNoiseQuery(_) => "IntakeNoiseQuery",
        FlowState::IntakeNoiseEmbed(_) => "IntakeNoiseEmbed",
        FlowState::IntakeNoiseStore { .. } => "IntakeNoiseStore",
        FlowState::IntakeNoiseScore { .. } => "IntakeNoiseScore",
        FlowState::IntakeTurn { .. } => "IntakeTurn",
        FlowState::AnalyzeAttempt { .. } => "AnalyzeAttempt",
        FlowState::AnalyzeLoadTurn { .. } => "AnalyzeLoadTurn",
        FlowState::AnalyzeAnalysis { .. } => "AnalyzeAnalysis",
        FlowState::AnalyzeEmbedding(_) => "AnalyzeEmbedding",
        FlowState::AnalyzeProfileQuery(_) => "AnalyzeProfileQuery",
        FlowState::AnalyzeReview(_) => "AnalyzeReview",
        FlowState::AnalyzeDerived(_) => "AnalyzeDerived",
        FlowState::AnalyzePersistenceEmbedding(_) => "AnalyzePersistenceEmbedding",
        FlowState::MemoryWritePersistenceEmbedding { .. } => "MemoryWritePersistenceEmbedding",
        FlowState::ChatCompactMutation => "ChatCompactMutation",
        FlowState::MemoryDetails(_) => "MemoryDetails",
        FlowState::MemoryDetailsBatch(_) => "MemoryDetailsBatch",
        FlowState::MemoryWriteDedupe(_) => "MemoryWriteDedupe",
        FlowState::MemoryWriteEmbedding(_) => "MemoryWriteEmbedding",
        FlowState::MemoryWriteMutation(_) => "MemoryWriteMutation",
        FlowState::MemoryWriteBatchDedupe(_) => "MemoryWriteBatchDedupe",
        FlowState::MemoryWriteBatchEmbedding(_) => "MemoryWriteBatchEmbedding",
        FlowState::MemoryWriteBatchMutation(_) => "MemoryWriteBatchMutation",
        FlowState::MemoryWriteReview(_) => "MemoryWriteReview",
        FlowState::MemoryDeletePreflight(_) => "MemoryDeletePreflight",
        FlowState::MemoryDeleteMutation => "MemoryDeleteMutation",
        FlowState::MemoryDeleteBatchPreflight(_) => "MemoryDeleteBatchPreflight",
        FlowState::MemoryDeleteBatchMutation(_) => "MemoryDeleteBatchMutation",
        FlowState::ProfileQuery(_) => "ProfileQuery",
        FlowState::ProfileInstructionQuery(_) => "ProfileInstructionQuery",
        FlowState::ProfileInstructionLlm { .. } => "ProfileInstructionLlm",
        FlowState::ProfileInstructionMutation(_) => "ProfileInstructionMutation",
        FlowState::RetentionQuery(_) => "RetentionQuery",
        FlowState::MaintenanceRebuildQuery(_) => "MaintenanceRebuildQuery",
        FlowState::MaintenanceLifecyclePreflight(_) => "MaintenanceLifecyclePreflight",
        FlowState::MaintenanceRebuildEmbedding { .. } => "MaintenanceRebuildEmbedding",
        FlowState::MaintenanceMutation(_) => "MaintenanceMutation",
        FlowState::Terminal => "Terminal",
    }
}

/// Describes one checkpointed lifecycle position without leaking engine internals.
/// 描述一个已检查点化的生命周期位置且不泄露引擎内部实现。
fn checkpoint_state(record: &CommandRecord) -> vulcan_memorymesh_core_contracts::CheckpointState {
    if let Some(effect_id) = &record.pending_effect_id {
        return vulcan_memorymesh_core_contracts::CheckpointState::WaitingForEffect {
            effect_id: effect_id.clone(),
            effect_kind: flow_state_name(&record.state).to_owned(),
        };
    }
    if record.status == CommandStatus::ResultUncertain
        && let Some(key) = &record.pending_mutation_idempotency_key
    {
        return vulcan_memorymesh_core_contracts::CheckpointState::AwaitingReconciliation {
            idempotency_key: key.clone(),
        };
    }
    vulcan_memorymesh_core_contracts::CheckpointState::Ready
}

/// Verifies the canonical integrity digest of one checkpoint before trusting it.
/// 在信任一个检查点前校验其规范完整性摘要。
fn verify_checkpoint_digest(checkpoint: &CommandCheckpoint) -> Result<(), CoreError> {
    let expected = vulcan_memorymesh_core_contracts::checkpoint_payload_digest(checkpoint)?;
    if expected == checkpoint.payload_digest {
        Ok(())
    } else {
        Err(CoreError::new(
            ErrorCode::CheckpointIncompatible,
            ErrorStage::Validation,
            "checkpoint payload digest does not match its canonical content",
        ))
    }
}

/// Validates that one public checkpoint position exactly describes its private continuation.
/// 校验一个公共检查点位置是否精确描述其私有续体。
fn validate_checkpoint_continuation(
    checkpoint: &CommandCheckpoint,
    state: &FlowState,
    pending_effect: Option<&EffectRequest>,
    command: &CoreCommand,
) -> Result<(), CoreError> {
    let continuation = checkpoint.continuation.as_ref().ok_or_else(|| {
        command_error(
            ErrorCode::CheckpointIncompatible,
            "checkpoint continuation is missing",
        )
    })?;
    validate_checkpoint_summary(checkpoint, continuation)?;
    let request = pending_effect.ok_or_else(|| {
        command_error(
            ErrorCode::CheckpointIncompatible,
            "restorable checkpoint has no retained effect",
        )
    })?;
    match &checkpoint.state {
        vulcan_memorymesh_core_contracts::CheckpointState::WaitingForEffect {
            effect_id,
            effect_kind,
        } => {
            let expected_effect_id = format!(
                "{}:effect:{:08x}",
                checkpoint.handle.command_id, continuation.next_effect_sequence
            );
            let expected_operation_id = checkpoint
                .mutation_identity
                .as_ref()
                .map(|identity| &identity.operation_id);
            let side_effect_possible = matches!(request.effect, HostEffect::StorageMutation(_));
            if request.effect_id != *effect_id
                || request.effect_id.as_str() != expected_effect_id
                || request.command_id != checkpoint.handle.command_id
                || request.config_snapshot_id != checkpoint.handle.config_snapshot_id
                || request.operation_id.as_ref() != expected_operation_id
                || request.deadline_class != effect_deadline_class(state)
                || request.side_effect_possible != side_effect_possible
                || request.retryable == side_effect_possible
                || effect_kind != flow_state_name(state)
                || !pending_effect_matches_state(state, &request.effect)
                || checkpoint.received_summary.len().checked_add(1)
                    != usize::try_from(continuation.next_effect_sequence).ok()
            {
                return Err(command_error(
                    ErrorCode::CheckpointIncompatible,
                    "waiting checkpoint effect envelope, sequence, or flow kind is inconsistent",
                ));
            }
        }
        vulcan_memorymesh_core_contracts::CheckpointState::AwaitingReconciliation {
            idempotency_key,
        } => {
            let expected_effect_id = format!(
                "{}:effect:{:08x}",
                checkpoint.handle.command_id, continuation.next_effect_sequence
            );
            let expected_operation_id = checkpoint
                .mutation_identity
                .as_ref()
                .map(|identity| &identity.operation_id);
            if continuation.pending_mutation_idempotency_key.as_ref() != Some(idempotency_key)
                || checkpoint.received_summary.len()
                    != usize::try_from(continuation.next_effect_sequence).unwrap_or(usize::MAX)
                || request.effect_id.as_str() != expected_effect_id
                || request.command_id != checkpoint.handle.command_id
                || request.config_snapshot_id != checkpoint.handle.config_snapshot_id
                || request.operation_id.as_ref() != expected_operation_id
                || request.deadline_class != effect_deadline_class(state)
                || request.retryable
                || !request.side_effect_possible
                || !pending_effect_matches_state(state, &request.effect)
            {
                return Err(command_error(
                    ErrorCode::CheckpointIncompatible,
                    "uncertain checkpoint mutation envelope or reconciliation identity is inconsistent",
                ));
            }
        }
        vulcan_memorymesh_core_contracts::CheckpointState::Ready
        | vulcan_memorymesh_core_contracts::CheckpointState::Terminal => {
            return Err(command_error(
                ErrorCode::CheckpointIncompatible,
                "only waiting or result-uncertain checkpoints are resumable",
            ));
        }
    }
    validate_checkpoint_pending_mutation(
        checkpoint.mutation_identity.as_ref(),
        continuation.pending_mutation_idempotency_key.as_ref(),
        &request.effect,
        state,
        command,
    )
}

/// Validates command-specific and physical-observation invariants before installing a restored pipeline.
/// 安装恢复流水线前校验命令专属及物理观察不变量；参数为完整恢复上下文，返回错误或成功。
fn validate_pipeline_checkpoint(
    state: &FlowState,
    request: Option<&EffectRequest>,
    command: &CoreCommand,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    validate_precheck_checkpoint(state, request, command, snapshot)?;
    validate_postaction_review_checkpoint(state, request, command)?;
    validate_analyze_fallback_checkpoint(state, command)?;
    review_query_checkpoint::validate(state, request, command, snapshot).map_err(|_| {
        command_error(
            ErrorCode::CheckpointIncompatible,
            "review query plan, source or completed facts are inconsistent",
        )
    })?;
    validate_memory_search_checkpoint(state, command, snapshot)?;
    validate_search_batch_checkpoint(state, request, command, snapshot)?;
    validate_evidence_cache_checkpoint(state, request, snapshot)?;
    validate_observation_checkpoint(state, request, snapshot)
}

/// Binds a single public-search continuation to its normalized command and exact candidate boundaries.
/// 将单条公共搜索续体绑定到其规范命令及精确候选边界。
fn validate_memory_search_checkpoint(
    state: &FlowState,
    original: &CoreCommand,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    let FlowState::SearchPipeline(pipeline) = state else {
        return Ok(());
    };
    if !matches!(pipeline.follow_up, PipelineFollowUp::MemorySearch) {
        return Ok(());
    }
    let CoreCommand::MemorySearch(command) = original else {
        return Err(command_error(
            ErrorCode::CheckpointIncompatible,
            "memory search continuation is bound to another command kind",
        ));
    };
    let expected_limit = if command.limit == 0 {
        snapshot.memory_policy.retrieval_policy.top_k_default
    } else {
        command.limit
    };
    let work = &pipeline.work;
    if work.subject_id != command.subject_id
        || work.scope_access != command.scope_access
        || work.queries.as_slice() != [normalize_text(&command.query)]
        || work.vector_space_id != command.vector_space_id
        || work.boundary != command.boundary.session_recall_boundary
        || work.candidate_scope != command.boundary.candidate_scope
        || work.limit != expected_limit
        || work.enable_hard_dedupe_pool
        || work.now_ns != command.now_ns
    {
        return Err(command_error(
            ErrorCode::CheckpointIncompatible,
            "memory search continuation is inconsistent with its frozen command",
        ));
    }
    Ok(())
}

/// Rejects recovery that turns an L1 fallback into a partial review or fabricates profile acceptance fields.
/// 拒绝把L1回退变成局部评审或伪造画像接纳字段的恢复状态；参数为续体，返回成功或检查点错误。
fn validate_analyze_fallback_checkpoint(
    state: &FlowState,
    command: &CoreCommand,
) -> Result<(), CoreError> {
    let (pending, persistence) = match state {
        FlowState::AnalyzeProfileQuery(pending) | FlowState::AnalyzeReview(pending) => {
            (pending.as_ref(), false)
        }
        FlowState::AnalyzePersistenceEmbedding(pending) | FlowState::AnalyzeDerived(pending) => {
            (pending.as_ref(), true)
        }
        _ => return Ok(()),
    };
    if !pending.review_fallback_to_l1 {
        return Ok(());
    }
    let invalid = || {
        command_error(
            ErrorCode::CheckpointIncompatible,
            "L1 fallback retained incompatible review decisions",
        )
    };
    if !matches!(command, CoreCommand::PostActionAnalyzePendingTurn(original)
        if original == &pending.command && original.turn_id == pending.turn.id
            && original.subject_id == pending.turn.subject_id && original.scope_access.write_scope == pending.turn.scope_id
            && original.session_source_id == pending.turn.session_source_id)
        || matches!(state, FlowState::AnalyzePersistenceEmbedding(_))
            && !pending.embedding_dropped.is_empty()
    {
        return Err(invalid());
    }
    if !persistence
        || !pending.hard_dropped.is_empty()
        || !pending.memory_reviews.is_empty()
        || !pending.reviewer_to_candidate.is_empty()
        || !pending.current_profile_nodes.is_empty()
    {
        return Err(invalid());
    }
    let LlmOutput::PostActionReview {
        accepted_candidates,
        dropped_candidates,
        accepted_profile_candidates,
        invalid_profile_candidate_indexes,
        retired_profile_nodes,
        ..
    } = &pending.review
    else {
        return Err(invalid());
    };
    let dropped = pending
        .embedding_dropped
        .iter()
        .map(|drop| drop.input_index as usize)
        .collect::<BTreeSet<_>>();
    if dropped.len() != pending.embedding_dropped.len()
        || dropped.iter().any(|index| *index >= pending.memories.len())
        || !accepted_candidates
            .iter()
            .map(|candidate| candidate.candidate_index as usize)
            .eq((0..pending.memories.len()).filter(|index| !dropped.contains(index)))
        || accepted_candidates
            .iter()
            .any(|candidate| !candidate.supersede_memory_ids.is_empty())
        || !dropped_candidates.is_empty()
        || !invalid_profile_candidate_indexes.is_empty()
        || !retired_profile_nodes.is_empty()
        || accepted_profile_candidates.len() != pending.profiles.len()
    {
        return Err(invalid());
    }
    for (index, (accepted, original)) in accepted_profile_candidates
        .iter()
        .zip(&pending.profiles)
        .enumerate()
    {
        if accepted.candidate_index as usize != index
            || accepted.normalized_content != original.content
            || accepted.priority != 100
            || accepted.profile_level != ProfileLevel::Transient
            || accepted.level_reason.is_some()
            || !accepted.supersede_nodes.is_empty()
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Binds unique-query continuations to caller input, prepared vectors and the exact pending effect.
/// 将唯一查询续体绑定至调用输入、准备向量及精确待处理效果；参数为恢复上下文，不一致返回检查点错误。
#[allow(clippy::too_many_lines)]
fn validate_search_batch_checkpoint(
    state: &FlowState,
    request: Option<&EffectRequest>,
    original: &CoreCommand,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    let FlowState::SearchPipeline(pipeline) = state else {
        return Ok(());
    };
    let invalid = || {
        command_error(
            ErrorCode::CheckpointIncompatible,
            "search batch query plan, cursor or prepared facts are inconsistent",
        )
    };
    let PipelineFollowUp::MemorySearchBatchGroup {
        command,
        query_plan,
        embeddings,
        group,
        completed,
        evidence_cache,
    } = &pipeline.follow_up
    else {
        return if matches!(original, CoreCommand::MemorySearchBatch(_)) {
            Err(invalid())
        } else {
            Ok(())
        };
    };
    let CoreCommand::MemorySearchBatch(original) = original else {
        return Err(invalid());
    };
    let mut normalized = original.clone();
    normalized.queries = normalized
        .queries
        .iter()
        .map(|query| normalize_text(query))
        .collect();
    if normalized.top_k == 0 {
        normalized.top_k = snapshot.memory_policy.retrieval_policy.top_k_default;
    }
    if command != &normalized
        || query_plan != &QueryBatchPlan::new(&normalized.queries)
        || *group >= query_plan.unique_queries.len()
        || completed.len() != *group
    {
        return Err(invalid());
    }

    // Completed groups are unique-query results; caller aliases are expanded only after all groups finish.
    // 已完成组是唯一查询结果；全部分组完成之后才展开调用别名。
    if completed.iter().enumerate().any(|(index, result)| {
        result.query_index as usize != index
            || result.query != query_plan.unique_queries[index]
            || result.hits.len() > command.top_k as usize
    }) {
        return Err(invalid());
    }
    let work = &pipeline.work;
    // The pipeline calls the caller's top_k a limit; it must match despite the distinct field names.
    // 流水线将调用方top_k称为limit，字段命名不同但值必须一致。
    if work.limit != command.top_k {
        return Err(invalid());
    }
    if work.subject_id != command.subject_id
        || work.scope_access != command.scope_access
        || work.vector_space_id != command.vector_space_id
        || work.now_ns != command.now_ns
        || work.boundary != command.boundary.session_recall_boundary
        || work.candidate_scope != command.boundary.candidate_scope
        || work.enable_hard_dedupe_pool
    {
        return Err(invalid());
    }

    if pipeline.stage == PipelineStage::Embedding {
        if *group != 0
            || !embeddings.is_empty()
            || !work.embeddings.is_empty()
            || !evidence_cache.is_empty()
            || work.queries != query_plan.unique_queries
            || !work.vector.is_empty()
            || !work.lexical.is_empty()
            || !work.records.is_empty()
            || !work.evidence.is_empty()
            || !pipeline.fused.is_empty()
            || !pipeline.rerank_indices.is_empty()
        {
            return Err(invalid());
        }
    } else if embeddings.len() != query_plan.unique_queries.len()
        || embeddings.iter().any(|vector| {
            vector.len() != snapshot.embedding_dimension as usize
                || vector.iter().any(|value| !value.is_finite())
        })
        || work.queries != [query_plan.unique_queries[*group].clone()]
        || work.embeddings != [embeddings[*group].clone()]
    {
        return Err(invalid());
    }

    let expected = pipeline_effect(pipeline, snapshot).map_err(|_| invalid())?;
    if request.is_none_or(|request| request.effect != expected) {
        return Err(invalid());
    }
    Ok(())
}

/// Revalidates cached rows and the exact incremental wait against the retained invocation cursor.
/// 按保留的调用游标重新校验证据缓存行和精确增量等待；参数为恢复状态、当前效果和快照，错误拒绝恢复。
fn validate_evidence_cache_checkpoint(
    state: &FlowState,
    request: Option<&EffectRequest>,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    let FlowState::SearchPipeline(pipeline) = state else {
        return Ok(());
    };
    let invalid = || {
        command_error(
            ErrorCode::CheckpointIncompatible,
            "context evidence cache or incremental wait is inconsistent",
        )
    };
    if let Some(cache) = pipeline_evidence_cache(pipeline) {
        // A waiting group's cache contains only earlier completed observations, never a prefetched current query.
        // 等待分组的缓存只能包含之前已完成的观察，不能提前塞入当前查询的观察。
        let previous_groups = match &pipeline.follow_up {
            PipelineFollowUp::MemorySearchBatchGroup {
                group, query_plan, ..
            } => {
                if *group >= query_plan.unique_queries.len() {
                    return Err(invalid());
                }
                *group
            }
            PipelineFollowUp::PreCheckGroup(pending) => pending.group,
            PipelineFollowUp::AnalyzeReview(search) => {
                if search.completed.len() >= search.query_plan.unique_queries.len() {
                    return Err(invalid());
                }
                search.completed.len()
            }
            PipelineFollowUp::DirectWriteReview {
                item_index,
                pending,
            } => {
                if *item_index >= pending.proposed.len() {
                    return Err(invalid());
                }
                *item_index
            }
            PipelineFollowUp::MemorySearch => return Err(invalid()),
        };
        let max_ids = previous_groups
            .checked_mul(snapshot.memory_policy.retrieval_policy.candidate_pool_max as usize)
            .ok_or_else(invalid)?;
        if cache.len() > max_ids {
            return Err(invalid());
        }
        for (id, rows) in cache {
            MemoryId::new(id.as_str()).map_err(|_| invalid())?;
            let group = ContextEvidenceGroup {
                memory_id: id.clone(),
                evidence: rows.clone(),
            };
            validate_context_evidence_groups(std::slice::from_ref(id), &[group])
                .map_err(|_| invalid())?;
        }
    }
    if pipeline.stage == PipelineStage::Evidence {
        let missing = pipeline_missing_evidence_ids(pipeline);
        if missing.is_empty()
            || !matches!(request.map(|request| &request.effect),
            Some(HostEffect::StorageQuery(StorageQuery::ContextEvidenceBatch { subject_id, scope_access, memory_ids }))
                if subject_id == &pipeline.work.subject_id && scope_access == &pipeline.work.scope_access && memory_ids == &missing)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Verifies retained L2 provenance against both the original command and its private continuation.
/// 对照原命令与私有续体验证保留的L2来源；参数为恢复状态、效果和命令，不一致返回检查点错误。
fn validate_postaction_review_checkpoint(
    state: &FlowState,
    request: Option<&EffectRequest>,
    command: &CoreCommand,
) -> Result<(), CoreError> {
    let direct = match state {
        FlowState::MemoryWriteReview(pending)
        | FlowState::MemoryWritePersistenceEmbedding { pending, .. } => Some(pending.as_ref()),
        FlowState::SearchPipeline(pipeline) => match &pipeline.follow_up {
            PipelineFollowUp::DirectWriteReview { pending, .. } => Some(pending.as_ref()),
            _ => None,
        },
        _ => None,
    };
    let direct_time_valid = direct.is_none_or(|pending| match (&pending.mode, command) {
        (DirectWriteResultMode::Single, CoreCommand::MemoryWrite(original)) => {
            pending.occurred_at_ns == original.occurred_at_ns
        }
        (DirectWriteResultMode::Batch, CoreCommand::MemoryWriteBatch(original)) => {
            pending.occurred_at_ns == original.occurred_at_ns
        }
        _ => false,
    });
    let input = match request.map(|request| &request.effect) {
        Some(HostEffect::Llm(effect)) => Some(&effect.input),
        _ => None,
    };
    let input_valid = match state {
        FlowState::AnalyzeReview(pending) => {
            matches!(command, CoreCommand::PostActionAnalyzePendingTurn(original) if original == &pending.command)
                && matches!(input, Some(LlmInput::PostActionReview {
                    context: vulcan_memorymesh_core_contracts::PostActionReviewContext::Turn {
                        target_turn, user_input_kind, profile_sections,
                    }, memory_candidates, profile_candidates, current_profile_nodes,
                }) if target_turn.as_ref() == &pending.turn
                    && pending.turn.id == pending.command.turn_id
                    && *user_input_kind == pending.user_input_kind
                    && profile_sections == &pending.command.profile_sections
                    && profile_candidates == &pending.profiles
                    && current_profile_nodes == &pending.current_profile_nodes
                    && memory_candidates.len() == pending.reviewer_to_candidate.len()
                    && memory_candidates.iter().zip(&pending.reviewer_to_candidate).all(|(actual, index)|
                        pending.memory_reviews.get(*index as usize).is_some_and(|expected| expected == actual)))
        }
        FlowState::MemoryWriteReview(pending) => matches!(input, Some(LlmInput::PostActionReview {
            context: vulcan_memorymesh_core_contracts::PostActionReviewContext::DirectWrite { occurred_at_ns },
            memory_candidates, profile_candidates, current_profile_nodes,
        }) if *occurred_at_ns == pending.occurred_at_ns
            && memory_candidates == &pending.memory_reviews
            && profile_candidates.is_empty() && current_profile_nodes.is_empty()),
        _ => true,
    };
    if direct_time_valid && input_valid {
        Ok(())
    } else {
        Err(command_error(
            ErrorCode::CheckpointIncompatible,
            "PostAction review source, time, roles, or candidates disagree with retained command state",
        ))
    }
}

/// Validates the grouped `PreCheck` continuation against the command, exact snapshot and retained reviewer input.
/// 将分组 `PreCheck` 续体与命令、精确快照及保留审核输入对照，拒绝检查点内部的查询归属漂移。
fn validate_precheck_checkpoint(
    state: &FlowState,
    request: Option<&EffectRequest>,
    command: &CoreCommand,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    let policy = &snapshot.memory_policy.precheck_policy;
    let valid = match state {
        FlowState::SearchPipeline(pipeline) => match &pipeline.follow_up {
            PipelineFollowUp::PreCheckGroup(pending) => {
                let work = &pipeline.work;
                let expected_boundary = SessionRecallBoundary {
                    session_source_id: pending.command.session_source_id.clone(),
                    mode: pending.command.session_recall_mode,
                };
                matches!(command, CoreCommand::PreCheck(original) if original == &pending.command)
                    && pending.group < pending.queries.len()
                    && pending.queries.len() == pending.embeddings.len()
                    && pending.queries.len() <= pending.command.max_search_queries as usize
                    && work.queries.as_slice() == &pending.queries[pending.group..=pending.group]
                    && work.embeddings.as_slice()
                        == &pending.embeddings[pending.group..=pending.group]
                    && work.subject_id == pending.command.subject_id
                    && work.scope_access == pending.command.scope_access
                    && work.vector_space_id == pending.command.vector_space_id
                    && work.boundary.as_ref() == Some(&expected_boundary)
                    && work.candidate_scope.as_ref() == Some(&pending.command.candidate_scope)
                    && !work.enable_hard_dedupe_pool
                    && work.limit == policy.recall_top_k
                    && work.now_ns == pending.command.now_ns
                    && crate::precheck::validate_review_accumulator(
                        &pending.completed,
                        pending.group,
                        policy,
                    )
            }
            _ => true,
        },
        FlowState::PreCheckReview {
            command: retained,
            queries,
            candidates,
        } => {
            matches!(command, CoreCommand::PreCheck(original) if original == retained)
                && candidates.len() <= policy.review_candidate_limit as usize
                && crate::precheck::validate_review_states(candidates, policy)
                && matches!(request.map(|request| &request.effect), Some(HostEffect::Llm(effect))
                    if matches!(&effect.input, LlmInput::PreCheckReview { candidates: views, search_queries, .. }
                        if views == &review_candidate_views(candidates) && search_queries == queries))
        }
        FlowState::PreCheckReinforce(pending) => {
            pending.candidates.len() <= policy.review_candidate_limit as usize
                && crate::precheck::validate_review_states(&pending.candidates, policy)
                && crate::precheck::validate_review_states(&pending.accepted, policy)
                && pending
                    .accepted
                    .iter()
                    .all(|accepted| pending.candidates.contains(accepted))
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(command_error(
            ErrorCode::CheckpointIncompatible,
            "PreCheck grouped continuation is inconsistent",
        ))
    }
}

/// Validates observation-specific state without asking the host to reconstruct an earlier database view.
/// 校验同次观察专属状态，不让宿主重建先前数据库视图；参数为续体、待处理效果和精确快照。
fn validate_observation_checkpoint(
    state: &FlowState,
    request: Option<&EffectRequest>,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    let FlowState::SearchPipeline(pipeline) = state else {
        return Ok(());
    };
    let single = snapshot.memory_policy.retrieval_policy.candidate_read_mode
        == vulcan_memorymesh_core_contracts::CandidateReadMode::SingleObservation;
    let invalid = || {
        command_error(
            ErrorCode::CheckpointIncompatible,
            "candidate observation continuation is inconsistent with its frozen query or policy",
        )
    };
    if !single {
        if pipeline.stage == PipelineStage::Observation || pipeline.work.separate_fallback_active {
            return Err(invalid());
        }
        if pipeline.stage == PipelineStage::Embedding {
            let work = &pipeline.work;
            if !work.vector.is_empty()
                || !work.lexical.is_empty()
                || !work.records.is_empty()
                || !work.evidence.is_empty()
                || !pipeline.fused.is_empty()
                || !pipeline.rerank_indices.is_empty()
            {
                return Err(invalid());
            }
            if let Some(request) = request
                && request.effect != pipeline_effect(pipeline, snapshot).map_err(|_| invalid())?
            {
                return Err(invalid());
            }
            return Ok(());
        }
        return validate_separate_channel_checkpoint(pipeline, request, snapshot)
            .map_err(|_| invalid());
    }
    if pipeline.work.separate_fallback_active {
        if snapshot
            .memory_policy
            .retrieval_policy
            .separate_channel_fallback
            .is_none()
        {
            return Err(invalid());
        }
        return validate_separate_channel_checkpoint(pipeline, request, snapshot)
            .map_err(|_| invalid());
    }
    match pipeline.stage {
        PipelineStage::Vector | PipelineStage::Lexical | PipelineStage::Records => Err(invalid()),
        PipelineStage::Embedding | PipelineStage::Observation => {
            // No candidate may precede the indivisible confirmation, including inside a restored continuation.
            // 不可分割确认之前不得已有候选事实，恢复的续体也不能例外。
            let work = &pipeline.work;
            if !work.vector.is_empty()
                || !work.lexical.is_empty()
                || !work.records.is_empty()
                || !work.evidence.is_empty()
                || !pipeline.fused.is_empty()
                || !pipeline.rerank_indices.is_empty()
            {
                return Err(invalid());
            }
            if pipeline.stage == PipelineStage::Observation {
                let expected =
                    candidate_observation_query(work, snapshot).map_err(|_| invalid())?;
                if !matches!(request.map(|request| &request.effect),
                    Some(HostEffect::StorageQuery(StorageQuery::CandidateObservation(actual))) if actual == &expected)
                {
                    return Err(invalid());
                }
            }
            Ok(())
        }
        PipelineStage::Evidence | PipelineStage::Rerank => {
            // Revalidate the retained complete union locally; a missing record must never trigger a fresh read.
            // 仅在本地重新校验保留的完整并集；缺失记录绝不能触发重新读取。
            let work = &pipeline.work;
            let hybrid = snapshot.memory_policy.retrieval_policy.hybrid_enabled;
            if !hybrid && !work.lexical.is_empty() {
                return Err(invalid());
            }
            let observation = vulcan_memorymesh_core_contracts::CandidateObservation {
                vector_channel: vulcan_memorymesh_core_contracts::PhysicalCandidateChannel {
                    candidates: work.vector.clone(),
                },
                lexical_channel: hybrid.then(|| {
                    vulcan_memorymesh_core_contracts::PhysicalCandidateChannel {
                        candidates: work.lexical.clone(),
                    }
                }),
                memories: work.records.values().cloned().collect(),
            };
            accept_candidate_observation(
                &mut work.clone(),
                observation,
                &snapshot.memory_policy,
                snapshot.embedding_dimension,
            )
            .map_err(|_| invalid())
        }
    }
}

/// Revalidates a split-read continuation without replaying any physical storage call.
/// 不重放任何物理存储调用，重新校验分离读取续体、已接收事实及待处理效果。
fn validate_separate_channel_checkpoint(
    pipeline: &SearchPipeline,
    request: Option<&EffectRequest>,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    if matches!(
        pipeline.stage,
        PipelineStage::Embedding | PipelineStage::Observation
    ) {
        return Err(effect_type_error(
            "active separate-channel fallback has an impossible pipeline stage",
        ));
    }
    let work = &pipeline.work;
    let policy = effective_retrieval_policy(work, &snapshot.memory_policy.retrieval_policy)?;
    let limit = candidate_pool_limit(work, &snapshot.memory_policy);
    let validate_channel =
        |candidates: &[PhysicalCandidate],
         descriptor: &vulcan_memorymesh_core_contracts::ScoreContractDescriptor,
         name: &str| {
            validate_candidate_result_bound(candidates, limit, name)?;
            vulcan_memorymesh_core_contracts::validate_candidate_channel(candidates)
                .map_err(|error| effect_type_error(error.diagnostic))?;
            vulcan_memorymesh_core_contracts::validate_candidate_channel_contract(
                candidates, descriptor,
            )
            .map_err(|error| effect_type_error(error.diagnostic))
        };

    // Each waiting stage may retain only facts accepted by earlier split reads.
    // 每个等待阶段只能保留先前分离读取已接收的事实，不能预填当前或后续阶段结果。
    match pipeline.stage {
        PipelineStage::Vector => {
            if !work.vector.is_empty()
                || !work.lexical.is_empty()
                || !work.records.is_empty()
                || !work.evidence.is_empty()
                || !pipeline.fused.is_empty()
                || !pipeline.rerank_indices.is_empty()
            {
                return Err(effect_type_error(
                    "fallback vector wait contains premature facts",
                ));
            }
        }
        PipelineStage::Lexical => {
            validate_channel(&work.vector, &policy.vector_contract, "fallback vector")?;
            let vector_ids = work
                .vector
                .iter()
                .map(|candidate| MemoryId::new(candidate.memory_id.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| effect_type_error(error.to_string()))?;
            validate_retained_candidate_records(work, &vector_ids, snapshot.embedding_dimension)?;
            if !work.lexical.is_empty()
                || !work.evidence.is_empty()
                || !pipeline.fused.is_empty()
                || !pipeline.rerank_indices.is_empty()
            {
                return Err(effect_type_error(
                    "fallback lexical wait contains premature facts",
                ));
            }
        }
        PipelineStage::Records => {
            validate_channel(&work.vector, &policy.vector_contract, "fallback vector")?;
            validate_channel(&work.lexical, &policy.lexical_contract, "fallback lexical")?;
            let requested_ids = candidate_memory_ids(work)?;
            validate_retained_candidate_records(
                work,
                &requested_ids,
                snapshot.embedding_dimension,
            )?;
            if candidate_missing_record_ids(work)?.is_empty()
                || !work.evidence.is_empty()
                || !pipeline.fused.is_empty()
                || !pipeline.rerank_indices.is_empty()
            {
                return Err(effect_type_error(
                    "fallback record wait contains premature facts",
                ));
            }
        }
        PipelineStage::Evidence | PipelineStage::Rerank => {
            validate_channel(&work.vector, &policy.vector_contract, "fallback vector")?;
            validate_channel(&work.lexical, &policy.lexical_contract, "fallback lexical")?;
            let requested_ids = candidate_memory_ids(work)?;
            validate_retained_candidate_records(
                work,
                &requested_ids,
                snapshot.embedding_dimension,
            )?;
            let records = requested_ids
                .iter()
                .map(|id| work.records.get(id).cloned())
                .collect::<Vec<_>>();
            validate_candidate_records(
                work,
                &requested_ids,
                &records,
                snapshot.embedding_dimension,
            )?;
        }
        PipelineStage::Embedding | PipelineStage::Observation => {
            return Err(effect_type_error(
                "active fallback reached an impossible pipeline stage",
            ));
        }
    }
    validate_retained_pipeline_effect(pipeline, request, snapshot)
}

/// Confirms that a restored pending effect is derived exactly from retained pipeline state.
/// 确认恢复后的待处理效果由已保留流水线状态精确派生。
fn validate_retained_pipeline_effect(
    pipeline: &SearchPipeline,
    request: Option<&EffectRequest>,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    let Some(request) = request else {
        return Ok(());
    };
    if request.effect != pipeline_effect(pipeline, snapshot)? {
        return Err(effect_type_error(
            "split-read pending effect differs from the retained pipeline stage",
        ));
    }
    Ok(())
}

/// Validates the bounded effect sequence and received-result digest ledger of one checkpoint.
/// 校验一个检查点的有界 Effect 序列与已接收结果摘要账本。
fn validate_checkpoint_summary(
    checkpoint: &CommandCheckpoint,
    continuation: &vulcan_memorymesh_core_contracts::CheckpointContinuation,
) -> Result<(), CoreError> {
    if continuation.next_effect_sequence == 0
        || checkpoint
            .received_summary
            .iter()
            .any(|summary| !valid_effect_result_summary(summary))
    {
        return Err(command_error(
            ErrorCode::CheckpointIncompatible,
            "checkpoint effect sequence or received-result summary is invalid",
        ));
    }
    Ok(())
}

/// Checks one bounded received-result summary without decoding or exposing the original payload.
/// 检查一个有界的已接收结果摘要，且不解码或暴露原始载荷。
fn valid_effect_result_summary(summary: &str) -> bool {
    let mut parts = summary.split(':');
    let kind = parts.next();
    let algorithm = parts.next();
    let digest = parts.next();
    let known_kind = matches!(
        kind,
        Some("llm" | "embedding" | "rerank" | "storage-query" | "storage-mutation" | "failure")
    );
    known_kind
        && algorithm == Some("sha256")
        && digest.is_some_and(|value| {
            value.len() == 64
                && value
                    .as_bytes()
                    .iter()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        })
        && parts.next().is_none()
}

/// Validates a restored pending mutation identity against its root command identity.
/// 根据根命令身份校验恢复的待处理变更身份。
fn validate_checkpoint_pending_mutation(
    root: Option<&MutationIdentity>,
    pending_key: Option<&IdempotencyKey>,
    effect: &HostEffect,
    state: &FlowState,
    command: &CoreCommand,
) -> Result<(), CoreError> {
    match effect {
        HostEffect::StorageMutation(batch) => {
            if pending_key != Some(&batch.identity.idempotency_key) {
                return Err(command_error(
                    ErrorCode::CheckpointIncompatible,
                    "checkpoint mutation key does not match its pending batch",
                ));
            }
            let root = root.ok_or_else(|| {
                command_error(
                    ErrorCode::CheckpointIncompatible,
                    "checkpoint mutation batch has no root command identity",
                )
            })?;
            validate_derived_mutation_identity(
                root,
                &batch.identity,
                checkpoint_mutation_role(state, command)?,
            )
        }
        _ if pending_key.is_none() => Ok(()),
        _ => Err(command_error(
            ErrorCode::CheckpointIncompatible,
            "read-only checkpoint effect carries a mutation reconciliation key",
        )),
    }
}

/// Checks that a restored effect discriminant is the only one valid for its private flow state.
/// 检查恢复的 Effect 判别值是其私有流程状态唯一允许的类型。
#[allow(clippy::too_many_lines)]
fn pending_effect_matches_state(state: &FlowState, effect: &HostEffect) -> bool {
    match state {
        FlowState::Terminal => false,
        FlowState::PreCheckIntent { .. } => {
            matches!(effect, HostEffect::Llm(value) if value.purpose == LlmPurpose::PreCheckIntent)
        }
        FlowState::PreCheckEmbedding { .. }
        | FlowState::IntakeNoiseEmbed(_)
        | FlowState::IntakeNoiseScore { .. }
        | FlowState::AnalyzeEmbedding(_)
        | FlowState::MemoryWriteEmbedding(_)
        | FlowState::MemoryWriteBatchEmbedding(_)
        | FlowState::MaintenanceRebuildEmbedding { .. }
        | FlowState::AnalyzePersistenceEmbedding(_)
        | FlowState::MemoryWritePersistenceEmbedding { .. } => {
            matches!(effect, HostEffect::Embedding(_))
        }
        FlowState::SearchPipeline(pipeline) => match pipeline.stage {
            PipelineStage::Observation => matches!(
                effect,
                HostEffect::StorageQuery(StorageQuery::CandidateObservation(_))
            ),
            PipelineStage::Embedding => matches!(effect, HostEffect::Embedding(_)),
            PipelineStage::Vector => matches!(
                effect,
                HostEffect::StorageQuery(StorageQuery::VectorCandidates { .. })
            ),
            PipelineStage::Lexical => matches!(
                effect,
                HostEffect::StorageQuery(StorageQuery::LexicalCandidates { .. })
            ),
            PipelineStage::Records => matches!(
                effect,
                HostEffect::StorageQuery(StorageQuery::MemoryRecordBatch { .. })
            ),
            PipelineStage::Evidence => matches!(
                effect,
                HostEffect::StorageQuery(StorageQuery::ContextEvidenceBatch { .. })
            ),
            PipelineStage::Rerank => matches!(effect, HostEffect::Rerank(_)),
        },
        FlowState::PreCheckReview { .. } => {
            matches!(effect, HostEffect::Llm(value) if value.purpose == LlmPurpose::PreCheckReview)
        }
        FlowState::AnalyzeAnalysis { .. } => matches!(
            effect,
            HostEffect::Llm(value) if value.purpose == LlmPurpose::PostActionAnalysis
        ),
        FlowState::AnalyzeReview(_) | FlowState::MemoryWriteReview(_) => matches!(
            effect,
            HostEffect::Llm(value) if value.purpose == LlmPurpose::PostActionReview
        ),
        FlowState::ProfileInstructionLlm { .. } => matches!(
            effect,
            HostEffect::Llm(value) if value.purpose == LlmPurpose::ProfileInstruction
        ),
        FlowState::IntakeNoiseQuery(_) => matches!(
            effect,
            HostEffect::StorageQuery(StorageQuery::NoisePrototypeBundle { .. })
        ),
        FlowState::AnalyzeLoadTurn { .. } => matches!(
            effect,
            HostEffect::StorageQuery(StorageQuery::PostActionContext { .. })
        ),
        FlowState::AnalyzeProfileQuery(_)
        | FlowState::ProfileQuery(_)
        | FlowState::ProfileInstructionQuery(_) => matches!(
            effect,
            HostEffect::StorageQuery(StorageQuery::ProfileNodes { .. })
        ),
        FlowState::MemoryDetails(_) | FlowState::MemoryDeletePreflight(_) => matches!(
            effect,
            HostEffect::StorageQuery(StorageQuery::MemoryDetails { .. })
        ),
        FlowState::MemoryDetailsBatch(_)
        | FlowState::MemoryDeleteBatchPreflight(_)
        | FlowState::MaintenanceRebuildQuery(_)
        | FlowState::MaintenanceLifecyclePreflight(_) => matches!(
            effect,
            HostEffect::StorageQuery(StorageQuery::DetailsBatch { .. })
        ),
        FlowState::MemoryWriteDedupe(_) | FlowState::MemoryWriteBatchDedupe(_) => matches!(
            effect,
            HostEffect::StorageQuery(StorageQuery::RecentDirectWriteDedupe { .. })
        ),
        FlowState::RetentionQuery(_) => matches!(
            effect,
            HostEffect::StorageQuery(StorageQuery::RetentionScan { .. })
        ),
        FlowState::PreCheckReinforce(_)
        | FlowState::IntakeNoiseStore { .. }
        | FlowState::IntakeTurn { .. }
        | FlowState::AnalyzeAttempt { .. }
        | FlowState::AnalyzeDerived(_)
        | FlowState::ChatCompactMutation
        | FlowState::MemoryWriteMutation(_)
        | FlowState::MemoryWriteBatchMutation(_)
        | FlowState::MemoryDeleteMutation
        | FlowState::MemoryDeleteBatchMutation(_)
        | FlowState::ProfileInstructionMutation(_)
        | FlowState::MaintenanceMutation(_) => {
            matches!(effect, HostEffect::StorageMutation(_))
        }
    }
}

/// Reads the only mutation role allowed by the restored private state and original command.
/// 从已恢复私有状态和原命令读取唯一允许的变更角色，不从不透明身份猜测。
fn checkpoint_mutation_role(
    state: &FlowState,
    command: &CoreCommand,
) -> Result<&'static str, CoreError> {
    match state {
        FlowState::PreCheckReinforce(_) => Ok("reinforce"),
        FlowState::IntakeNoiseStore { .. } => Ok("noise-cache"),
        FlowState::IntakeTurn { .. } => Ok("intake"),
        FlowState::AnalyzeAttempt { .. } => Ok("attempt"),
        FlowState::AnalyzeDerived(_) => Ok("derive"),
        FlowState::ChatCompactMutation => Ok("compact"),
        FlowState::MemoryWriteMutation(_) => Ok("write"),
        FlowState::MemoryWriteBatchMutation(_) => Ok("write-batch"),
        FlowState::MemoryDeleteMutation => Ok("delete"),
        FlowState::MemoryDeleteBatchMutation(_) => Ok("delete-batch"),
        FlowState::ProfileInstructionMutation(_) => Ok("instruction"),
        FlowState::MaintenanceMutation(_) => match command {
            CoreCommand::Maintenance(MaintenanceCommand {
                action: MaintenanceAction::RebuildVectors { .. },
                ..
            }) => Ok("rebuild"),
            CoreCommand::Maintenance(_) => Ok("maintenance"),
            _ => Err(command_error(
                ErrorCode::CheckpointIncompatible,
                "maintenance checkpoint has a different original command",
            )),
        },
        _ => Err(command_error(
            ErrorCode::CheckpointIncompatible,
            "checkpoint flow has no mutation role",
        )),
    }
}

/// Validates the complete restored sub-operation against its exact state-derived role.
/// 根据精确状态角色校验完整恢复的子操作身份。
fn validate_derived_mutation_identity(
    root: &MutationIdentity,
    derived: &MutationIdentity,
    role: &str,
) -> Result<(), CoreError> {
    if *derived != derive_batch_identity(root, role)? {
        return Err(command_error(
            ErrorCode::CheckpointIncompatible,
            "checkpoint mutation identity fields do not match their root identity",
        ));
    }
    Ok(())
}

/// Produces one bounded deterministic digest summary without retaining sensitive effect payloads.
/// 生成一个有界确定性摘要，且不保留敏感 Effect 载荷。
fn effect_result_summary(result: &HostEffectResult) -> Result<String, CoreError> {
    let kind = match result {
        HostEffectResult::Llm(_) => "llm",
        HostEffectResult::Embedding(_) => "embedding",
        HostEffectResult::Rerank(_) => "rerank",
        HostEffectResult::StorageQuery(_) => "storage-query",
        HostEffectResult::StorageMutation(_) => "storage-mutation",
        HostEffectResult::Failure(_) => "failure",
    };
    // Encode with the same recoverability bound as checkpoints, but classify overflow or
    // malformed host data as an Effect contract failure rather than a checkpoint failure.
    // 使用与检查点相同的可恢复性上限编码，但将超限或畸形宿主数据归类为 Effect 契约失败，
    // 而不是检查点失败。
    let configuration = bincode::config::standard()
        .with_little_endian()
        .with_fixed_int_encoding()
        .with_limit::<MAX_PRIVATE_CONTINUATION_BYTES>();
    let encoded = bincode::serde::encode_to_vec(result, configuration).map_err(|error| {
        effect_type_error(format!(
            "received effect result exceeds the recoverable binary bound: {error}"
        ))
    })?;
    if encoded.len() > MAX_PRIVATE_CONTINUATION_BYTES {
        return Err(effect_type_error(
            "received effect result exceeds the recoverable binary bound",
        ));
    }
    let digest = <sha2::Sha256 as sha2::Digest>::digest(encoded);
    Ok(format!("{kind}:sha256:{digest:x}"))
}

/// Encodes one private continuation value with the bounded deterministic checkpoint codec.
/// 使用有界确定性检查点编解码器编码一个私有续体值。
fn encode_checkpoint_continuation<T: Serialize>(
    value: &T,
    label: &str,
) -> Result<Vec<u8>, CoreError> {
    let configuration = bincode::config::standard()
        .with_little_endian()
        .with_fixed_int_encoding()
        .with_limit::<MAX_PRIVATE_CONTINUATION_BYTES>();
    let encoded = bincode::serde::encode_to_vec(value, configuration).map_err(|error| {
        command_error(
            ErrorCode::CheckpointIncompatible,
            format!("checkpoint {label} binary serialization failed: {error}"),
        )
    })?;
    if encoded.len() > MAX_PRIVATE_CONTINUATION_BYTES {
        return Err(command_error(
            ErrorCode::CheckpointIncompatible,
            format!("checkpoint {label} binary payload exceeds the recoverable bound"),
        ));
    }
    Ok(encoded)
}

/// Decodes one private continuation value and rejects trailing or oversized binary payloads.
/// 解码一个私有续体值，并拒绝尾随字节或超限二进制载荷。
fn decode_checkpoint_continuation<T: DeserializeOwned>(
    bytes: &[u8],
    label: &str,
) -> Result<T, CoreError> {
    let configuration = bincode::config::standard()
        .with_little_endian()
        .with_fixed_int_encoding()
        .with_limit::<MAX_PRIVATE_CONTINUATION_BYTES>();
    let (value, consumed) =
        bincode::serde::decode_from_slice(bytes, configuration).map_err(|error| {
            command_error(
                ErrorCode::CheckpointIncompatible,
                format!("checkpoint {label} binary payload is invalid: {error}"),
            )
        })?;
    if consumed != bytes.len() {
        return Err(command_error(
            ErrorCode::CheckpointIncompatible,
            format!("checkpoint {label} binary payload contains trailing bytes"),
        ));
    }
    Ok(value)
}

/// Creates one validation-stage error.
/// 创建一个校验阶段错误。
fn validation_error(diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidArgument,
        ErrorStage::Validation,
        diagnostic,
    )
}

/// Creates one command-stage error.
/// 创建一个命令阶段错误。
fn command_error(code: ErrorCode, diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(code, ErrorStage::Command, diagnostic)
}

/// Creates one effect result shape error.
/// 创建一个 Effect 结果形状错误。
fn effect_type_error(expected: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::EffectMismatch,
        ErrorStage::Effect,
        format!("unexpected or invalid {}", expected.into()),
    )
}

/// Verifies that explicit profile instructions cannot be downgraded by model output.
/// 验证显式画像指令不会被模型输出降级。
#[cfg(test)]
mod manual_profile_authority_tests {
    use super::*;

    /// Clamps weak output while preserving stronger priority and lifecycle declarations.
    /// 钳制过弱输出，同时保留更强的优先级与生命周期声明。
    #[test]
    fn explicit_instruction_enforces_p1_l2_floor() {
        assert_eq!(
            manual_profile_authority_floor(20, ProfileLevel::Transient),
            (60, ProfileLevel::Stable)
        );
        assert_eq!(
            manual_profile_authority_floor(100, ProfileLevel::Persistent),
            (100, ProfileLevel::Persistent)
        );
    }
}

/// Verifies finite expiry arithmetic without manufacturing permanent or saturated records.
/// 验证有限到期时间算术，禁止意外产生永久或截断时间的记录。
#[cfg(test)]
mod lifecycle_time_tests {
    use super::*;

    /// Accepts the exact representable boundary and rejects overflow before any write.
    /// 接受可表示的精确边界，并在任何写入之前拒绝溢出。
    #[test]
    fn finite_expiry_never_saturates_or_becomes_permanent() -> Result<(), CoreError> {
        const MILLIS_IN_NANOS: i64 = 1_000_000;
        const STABLE_NS: i64 = 90 * 24 * 3_600_000 * MILLIS_IN_NANOS;
        assert_eq!(
            checked_expiry_from_millis(i64::MAX - MILLIS_IN_NANOS, 1)?,
            i64::MAX
        );
        assert!(checked_expiry_from_millis(i64::MAX, 1).is_err());
        assert!(checked_expiry_from_millis(0, u64::MAX).is_err());
        assert_eq!(
            profile_expiry_ns(i64::MAX - STABLE_NS, ProfileLevel::Stable, 0)?,
            Some(i64::MAX)
        );
        assert!(profile_expiry_ns(i64::MAX - STABLE_NS + 1, ProfileLevel::Stable, 0).is_err());
        assert_eq!(
            profile_expiry_ns(i64::MAX, ProfileLevel::Persistent, u32::MAX)?,
            None
        );
        assert_eq!(
            profile_expiry_ns(0, ProfileLevel::Stable, u32::MAX)?,
            Some(365 * 24 * 3_600_000 * MILLIS_IN_NANOS)
        );
        Ok(())
    }
}

/// Verifies sub-operation identity derivation across the complete public input domain.
/// 验证子操作身份派生覆盖公共身份输入的完整合法范围。
#[cfg(test)]
mod mutation_identity_tests {
    use super::*;
    use vulcan_memorymesh_core_contracts::OperationId;

    /// Keeps maximum-length and Unicode caller identities stable and stage-isolated.
    /// 保持最大长度及 Unicode 调用方身份可派生、稳定且按阶段隔离。
    #[test]
    fn derived_identity_accepts_the_full_public_id_bound() -> Result<(), Box<dyn std::error::Error>>
    {
        for root in ["x".repeat(512), "界".repeat(170), "caller#stage".to_owned()] {
            let identity = MutationIdentity {
                operation_id: OperationId::new(&root)?,
                idempotency_key: IdempotencyKey::new(&root)?,
                request_fingerprint: format!("fp256:{}", "0".repeat(64)),
                source_kind: vulcan_memorymesh_core_contracts::MutationSourceKind::HostRpc,
                request_source_id: "identity-regression".to_owned(),
                observed_at_ns: 1,
            };
            let batch = derive_batch_identity(&identity, "write-batch")?;
            assert_eq!(batch, derive_batch_identity(&identity, "write-batch")?);
            let other = derive_batch_identity(&identity, "derive")?;
            assert_ne!(batch.operation_id, other.operation_id);
            assert_ne!(batch.idempotency_key, other.idempotency_key);
            assert_eq!(batch.request_fingerprint, identity.request_fingerprint);
            assert_ne!(batch.operation_id.as_str(), batch.idempotency_key.as_str());
            assert!(validate_derived_mutation_identity(&identity, &batch, "write-batch").is_ok());
            assert!(validate_derived_mutation_identity(&identity, &batch, "derive").is_err());
        }
        Ok(())
    }
}

/// Normalizes L1 evidence and removes duplicate observations within one candidate only.
/// 规范化 L1 证据，仅移除同一候选内部的重复观测；空规范标签显式失败。
fn normalize_analysis_evidence(
    rows: Vec<ContextEvidenceCandidate>,
) -> Result<Vec<ContextEvidenceCandidate>, CoreError> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::with_capacity(rows.len());
    for mut row in rows {
        row.context_key = retrieval_kernel::normalize_context_key(&row.context_key);
        row.context_value = retrieval_kernel::normalize_context_value(&row.context_value);
        if row.context_key.is_empty() || row.context_value.is_empty() {
            return Err(CoreError::new(
                ErrorCode::StructuredOutputContractViolation,
                ErrorStage::Effect,
                "PostAction context evidence has an empty normalized key or value",
            ));
        }
        // A field tuple avoids delimiter collisions while retaining separate relations and candidates.
        // 字段元组避免分隔符碰撞，同时保留不同关系与候选的独立观测。
        if seen.insert((
            row.candidate_index,
            row.context_key.clone(),
            row.context_value.clone(),
            row.relation as u32,
        )) {
            normalized.push(row);
        }
    }
    Ok(normalized)
}

/// Checks candidate-local normalization before any admission-index compression.
/// 验证准入索引压缩前的候选局部规范化。
#[cfg(test)]
mod evidence_normalization_tests {
    use super::*;

    /// Retains observations from different candidates and rejects separator-only labels.
    /// 保留来自不同候选的观测，并拒绝仅含分隔符的标签。
    #[test]
    fn evidence_is_unique_per_candidate_and_relation() -> Result<(), CoreError> {
        let row = ContextEvidenceCandidate {
            candidate_index: 0,
            context_key: "BUILD.Mode".to_owned(),
            context_value: " Rust/Lang ".to_owned(),
            relation: ContextEvidenceRelation::Support,
        };
        let mut other = row.clone();
        other.candidate_index = 1;
        let mut rebuttal = row.clone();
        rebuttal.relation = ContextEvidenceRelation::Rebuttal;
        let rows = normalize_analysis_evidence(vec![row.clone(), row.clone(), other, rebuttal])?;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].context_key, "build_mode");
        assert_eq!(rows[0].context_value, "rust lang");
        let mut invalid = row;
        invalid.context_key = "_-/.\\".to_owned();
        assert_eq!(
            normalize_analysis_evidence(vec![invalid])
                .err()
                .map(|error| error.code),
            Some(ErrorCode::StructuredOutputContractViolation)
        );
        Ok(())
    }
}

/// Verifies the permanent-invalid-input partition independently of Provider integrations.
/// 独立于 Provider 集成验证永久非法输入划分。
#[cfg(test)]
mod embedding_partition_tests {
    use super::*;
    use vulcan_memorymesh_core_contracts::{
        EmbeddingBatchResult, EmbeddingDropReason, EmbeddingDroppedInput,
    };

    /// Builds a typed input-too-large rejection for the requested position.
    /// 为请求位置构建强类型输入超长拒绝。
    fn rejected(input_index: u32) -> EmbeddingDroppedInput {
        EmbeddingDroppedInput {
            input_index,
            reason: EmbeddingDropReason::InputTooLarge,
        }
    }

    /// Rejects missing, duplicate, out-of-range, non-finite, and strict-mode partial vectors.
    /// 拒绝缺失、重复、越界、非有限向量以及严格模式部分向量。
    #[test]
    fn partial_embedding_requires_an_exact_partition() {
        let valid = EmbeddingBatchResult {
            vectors: vec![vec![1.0], vec![2.0]],
            result_indices: vec![2, 0],
            dropped: vec![rejected(1)],
        };
        assert!(
            expect_partial_embeddings(HostEffectResult::Embedding(valid.clone()), 3, 1).is_ok()
        );
        assert!(expect_embeddings(HostEffectResult::Embedding(valid.clone()), 3, 1).is_err());
        let mut duplicate = valid.clone();
        duplicate.result_indices[1] = 1;
        assert!(expect_partial_embeddings(HostEffectResult::Embedding(duplicate), 3, 1).is_err());
        let mut missing = valid.clone();
        missing.dropped.clear();
        assert!(expect_partial_embeddings(HostEffectResult::Embedding(missing), 3, 1).is_err());
        let mut outside = valid.clone();
        outside.result_indices[0] = 3;
        assert!(expect_partial_embeddings(HostEffectResult::Embedding(outside), 3, 1).is_err());
        let mut nonfinite = valid;
        nonfinite.vectors[0][0] = f32::NAN;
        assert!(expect_partial_embeddings(HostEffectResult::Embedding(nonfinite), 3, 1).is_err());
        let all_dropped = EmbeddingBatchResult {
            vectors: Vec::new(),
            result_indices: Vec::new(),
            dropped: vec![rejected(0), rejected(1)],
        };
        assert!(expect_partial_embeddings(HostEffectResult::Embedding(all_dropped), 2, 1).is_ok());
        let complete = HostEffectResult::Embedding(vec![vec![1.0], vec![2.0]].into());
        assert!(expect_partial_embeddings(complete, 2, 1).is_ok());
    }

    /// Preserves non-size validation rejection only in the explicitly partial persistence stage.
    /// 仅在显式允许部分成功的持久阶段保留非长度校验拒绝。
    #[test]
    fn provider_confirmed_rejection_is_partial_only() -> Result<(), CoreError> {
        let rejected = EmbeddingDroppedInput {
            input_index: 1,
            reason: EmbeddingDropReason::ProviderConfirmedInvalidInput,
        };
        let mixed = EmbeddingBatchResult {
            vectors: vec![vec![1.0]],
            result_indices: vec![0],
            dropped: vec![rejected],
        };
        let restored = expect_partial_embeddings(HostEffectResult::Embedding(mixed.clone()), 2, 1)?;
        assert_eq!(restored.dropped, vec![rejected]);
        assert!(expect_embeddings(HostEffectResult::Embedding(mixed), 2, 1).is_err());
        let all = EmbeddingBatchResult {
            vectors: Vec::new(),
            result_indices: Vec::new(),
            dropped: vec![
                EmbeddingDroppedInput {
                    input_index: 0,
                    ..rejected
                },
                rejected,
            ],
        };
        assert!(expect_partial_embeddings(HostEffectResult::Embedding(all.clone()), 2, 1).is_ok());
        assert!(expect_embeddings(HostEffectResult::Embedding(all), 2, 1).is_err());
        Ok(())
    }

    /// Keeps query-text construction identical for direct writes and analyzed candidates.
    /// 保持主动写入与分析候选的查询文本构建一致。
    #[test]
    fn review_query_preserves_summary_and_details_roles() {
        assert_eq!(
            memory_review_query(" summary ", " details "),
            "summary details"
        );
        assert_eq!(memory_review_query("Rust", "RUST"), "Rust");
        assert_eq!(
            memory_review_query("Rust", "Uses Rust daily"),
            "Uses Rust daily"
        );
        assert_eq!(memory_review_query("", "details"), "details");
        assert_eq!(memory_review_query("summary", ""), "summary");
    }
}
