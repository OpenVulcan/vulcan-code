//! Semantic storage port and deterministic transactional reference backend.
//! 语义存储端口与确定性事务参考后端。

use serde::{Deserialize, Serialize};
mod manual_recycle;
pub use manual_recycle::StorageManualRecycleSelection;
mod session;
pub use session::StorageSessionStatus;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use vulcan_memorymesh_core_contracts::{
    AnalysisAttemptIdentity, ChatCompactBoundary, ChatCompactMutationResult, CommitReceipt,
    ContextEvidenceGroup, ContextEvidenceRelation, CoreError, EntityBinding, EntityKind, EntityRef,
    ErrorCode, ErrorStage, ExportDataset, IdempotencyKey, MemoryAccessIdentity,
    MemoryContextEvidence, MemoryId, MemoryRecord, MemorySourceKind, MemoryState, Mutation,
    MutationBatch, MutationOutcome, MutationSourceKind, NoisePrototype,
    NoisePrototypeCacheIdentity, PhysicalCandidate, PhysicalCandidateScore, PostActionDirectWrite,
    PostActionReferenceTurn, ProfileNode, ProfileNodeId, ProfileNodeState, ProfileSourceKind,
    ProfileViewSection, RawScoreKind, RetentionColdTurnCandidateFact, RetentionColdTurnJobFact,
    RetentionIdleSessionFact, RetentionMemoryFact, RetentionReasonCode, RetentionScanFacts,
    RetentionTrashBatchFact, ScopeAccess, ScoreContractRef, ScoreDirection, SectionRole,
    SessionRecallBoundary, SessionRecallMode, SessionSourceId, StorageQuery, StorageQueryResult,
    SubjectId, Turn, TurnId, TurnStatus, VectorSpaceId, VersionedEntity,
    context_evidence_domain_id,
};

/// Declares the reference backend's vector score contract identity.
/// 声明参考后端向量评分契约身份。
pub const REFERENCE_VECTOR_CONTRACT_ID: &str = "reference-cosine";
/// Declares the reference backend's lexical score contract identity.
/// 声明参考后端词法评分契约身份。
pub const REFERENCE_LEXICAL_CONTRACT_ID: &str = "reference-lexical";
/// Reports the reference contract version.
/// 报告参考契约版本。
pub const REFERENCE_CONTRACT_VERSION: u32 = 1;

/// Uses separate scope, language, model, dimension and rule fields rather than delimiter text.
/// 使用独立的作用域、语言、模型、维度与规则字段，而非分隔符文本。
type NoiseCacheKey = (
    vulcan_memorymesh_core_contracts::ScopeId,
    String,
    String,
    u32,
    String,
);

/// Describes a durable idempotency ledger entry.
/// 描述一条持久幂等账本条目。
#[derive(Clone, Debug)]
struct LedgerEntry {
    /// Carries the stable operation identity recorded at commit time.
    /// 携带提交时记录的稳定操作身份。
    operation_id: vulcan_memorymesh_core_contracts::OperationId,
    /// Carries the canonical request fingerprint recorded at commit time.
    /// 携带提交时记录的规范请求指纹。
    request_fingerprint: String,
    /// Carries the committed outcome returned on retry.
    /// 携带重试时返回的已提交结果。
    outcome: MutationOutcome,
}

/// Carries one serializable durable idempotency ledger entry for a host snapshot.
/// 携带宿主快照使用的一条可序列化持久幂等账本条目。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageLedgerEntry {
    /// Carries the stable operation identity recorded at commit time.
    /// 携带提交时记录的稳定操作身份。
    pub operation_id: vulcan_memorymesh_core_contracts::OperationId,
    /// Carries the canonical request fingerprint recorded at commit time.
    /// 携带提交时记录的规范请求指纹。
    pub request_fingerprint: String,
    /// Carries the committed outcome returned on retry.
    /// 携带重试时返回的已提交结果。
    pub outcome: MutationOutcome,
    /// Carries the durable idempotency key indexing this entry.
    /// 携带索引本条目的持久幂等键。
    pub idempotency_key: IdempotencyKey,
}

/// Carries one serializable semantic-noise prototype cache entry for a host snapshot.
/// 携带宿主快照使用的一条可序列化语义 Noise 原型缓存条目。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageNoisePrototypeEntry {
    /// Carries the complete typed cache identity.
    /// 携带完整的强类型缓存身份。
    pub identity: NoisePrototypeCacheIdentity,
    /// Carries every normalized prototype vector in the cache entry.
    /// 携带缓存条目中的每一个规范化原型向量。
    pub prototypes: Vec<NoisePrototype>,
}

/// Carries one durable recycle-bin batch owned by the host storage snapshot.
/// 携带一个由宿主存储快照持有的耐久回收站批次。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageRetentionTrashBatch {
    /// Identifies this exact atomic recycle operation.
    /// 标识本次精确原子回收操作。
    pub batch_id: String,
    /// Identifies the subject that owned every recycled record.
    /// 标识拥有全部已回收记录的主体。
    pub subject_id: SubjectId,
    /// Identifies the one owner scope represented by this batch.
    /// 标识该批次代表的唯一归属作用域。
    pub scope_id: vulcan_memorymesh_core_contracts::ScopeId,
    /// Carries the time when the records entered the recycle bin.
    /// 携带记录进入回收站的时间。
    pub recycled_at_ns: i64,
    /// Freezes the expiry deadline selected when this batch was created.
    /// 冻结创建该批次时选定的到期截止时间。
    pub expires_at_ns: i64,
    /// Carries the stable retention reason for audit and restore presentation.
    /// 携带用于审计与恢复展示的稳定保留原因。
    pub reason: RetentionReasonCode,
    /// Stores the exact manual selection and original session metadata; none means policy cleanup.
    /// 保存准确人工选择及原会话元数据；空值表示策略清洗。
    pub manual_selection: Option<StorageManualRecycleSelection>,
    /// Carries every domain record removed from active storage by this batch.
    /// 携带本批次从活动存储移除的全部领域记录。
    pub dataset: ExportDataset,
    /// Carries analysis-attempt identities attached to recycled turns.
    /// 携带关联到已回收轮次的分析尝试身份。
    pub analysis_attempts: Vec<AnalysisAttemptIdentity>,
}

/// Carries one durable cold-turn maintenance job owned by the native host.
/// 携带一个由原生宿主持有的耐久冷回合维护任务。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageRetentionColdTurnJob {
    /// Identifies this exact durable job.
    /// 标识该精确耐久任务。
    pub job_id: String,
    /// Identifies the subject authorized to maintain the session.
    /// 标识获授权维护该会话的主体。
    pub subject_id: SubjectId,
    /// Identifies the exact owner scope.
    /// 标识精确归属作用域。
    pub scope_id: vulcan_memorymesh_core_contracts::ScopeId,
    /// Identifies the host session containing old unreferenced turns.
    /// 标识包含旧无引用回合的宿主会话。
    pub session_source_id: SessionSourceId,
    /// Carries the first time at which the job may execute.
    /// 携带任务首次允许执行的时间。
    pub next_run_at_ns: i64,
}

/// Stores one authoritative memory session independently of retained turn bodies.
/// 独立于所保留回合正文存储一条权威记忆会话记录。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageMemorySession {
    /// Stores management lifecycle independently of retained turns and extraction progress.
    /// 独立于所保留回合及提炼进度保存管理生命周期。
    pub status: StorageSessionStatus,
    /// Identifies the exact subject; native mode uses the fixed local subject.
    /// 标识精确主体；原生模式使用固定本地主体。
    pub subject_id: SubjectId,
    /// Identifies the unique writing project, not linked recall projects.
    /// 标识唯一写入项目，而不是连带召回项目。
    pub scope_id: vulcan_memorymesh_core_contracts::ScopeId,
    /// Identifies the host session independently of its display title.
    /// 独立于显示标题标识宿主会话。
    pub session_source_id: SessionSourceId,
    /// Stores the inclusive observation cutoff of the last successful extraction; new sessions use zero.
    /// 保存上次成功提取的包含式观察截止时间；新会话使用零。
    pub last_extraction_observed_at_ns: i64,
    /// Stores the actual host commit time, absent until the first successful extraction.
    /// 保存实际宿主提交时间，在首次成功提取之前为空。
    pub last_extraction_completed_at_ns: Option<i64>,
}

/// Carries all reference-storage state required for durable host restart recovery.
/// 携带宿主重启恢复所需的全部参考存储状态。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageSnapshot {
    /// Carries the portable domain dataset, including optional in-memory embeddings.
    /// 携带可移植领域数据集，其中包含可选的内存嵌入。
    pub dataset: ExportDataset,
    /// Carries the monotonic storage version.
    /// 携带单调递增的存储版本。
    pub storage_version: u64,
    /// Carries the next host-local entity sequence.
    /// 携带下一个宿主本地实体序列号。
    pub next_entity_sequence: u64,
    /// Carries committed idempotency outcomes.
    /// 携带已提交的幂等结果。
    pub outcomes: Vec<StorageLedgerEntry>,
    /// Carries persisted analysis attempt identities.
    /// 携带已持久化的分析尝试身份。
    pub analysis_attempts: Vec<AnalysisAttemptIdentity>,
    /// Carries authoritative session records; attempts and compact boundaries are not substitutes.
    /// 携带权威会话记录；分析尝试及压缩边界不能替代该记录。
    pub memory_sessions: Vec<StorageMemorySession>,
    /// Carries persisted semantic-noise prototype bundles.
    /// 携带已持久化的语义 Noise 原型束。
    pub noise_prototypes: Vec<StorageNoisePrototypeEntry>,
    /// Carries durable recycle-bin batches retained for restore or expiry purge.
    /// 携带为恢复或到期清除而保留的耐久回收站批次。
    #[serde(default)]
    pub retention_trash_batches: Vec<StorageRetentionTrashBatch>,
    /// Carries durable cold-turn jobs awaiting or undergoing a retention pass.
    /// 携带等待或正在执行保留轮次的耐久冷回合任务。
    #[serde(default)]
    pub retention_cold_turn_jobs: Vec<StorageRetentionColdTurnJob>,
}

impl StorageSnapshot {
    /// Removes embeddings from a cloned snapshot for relational-only persistence.
    /// 从克隆快照中移除嵌入，以便仅持久化关系数据库部分。
    #[must_use]
    pub fn without_embeddings(&self) -> Self {
        let mut snapshot = self.clone();
        for memory in &mut snapshot.dataset.memories {
            memory.embedding = None;
        }
        for batch in &mut snapshot.retention_trash_batches {
            for memory in &mut batch.dataset.memories {
                memory.embedding = None;
            }
        }
        snapshot
    }

    /// Returns the memory embeddings that must be persisted in the vector data plane.
    /// 返回必须持久化到向量数据平面的记忆嵌入。
    #[must_use]
    pub fn memory_embeddings(&self) -> Vec<(MemoryId, VectorSpaceId, Vec<f32>)> {
        self.dataset
            .memories
            .iter()
            .chain(
                self.retention_trash_batches
                    .iter()
                    .flat_map(|batch| batch.dataset.memories.iter()),
            )
            .filter_map(|memory| {
                memory.embedding.clone().and_then(|embedding| {
                    memory
                        .vector_space_id
                        .clone()
                        .map(|vector_space_id| (memory.id.clone(), vector_space_id, embedding))
                })
            })
            .collect()
    }
}

/// Validates one durable recycle-bin batch without interpreting host policy.
/// 在不解释宿主策略的前提下校验一个耐久回收站批次。
fn validate_retention_trash_batch(batch: &StorageRetentionTrashBatch) -> Result<(), String> {
    if batch.batch_id.trim().is_empty() || batch.batch_id.len() > 512 {
        return Err(
            "storage snapshot contains an invalid retention trash batch identity".to_owned(),
        );
    }
    if batch.recycled_at_ns < 0 {
        return Err("storage snapshot contains a negative retention recycle timestamp".to_owned());
    }
    if batch.expires_at_ns <= batch.recycled_at_ns {
        return Err("storage retention trash expiry must follow its recycle timestamp".to_owned());
    }
    let mut turn_ids = BTreeSet::new();
    for turn in &batch.dataset.turns {
        if turn.subject_id != batch.subject_id || turn.scope_id != batch.scope_id {
            return Err("storage retention trash turn violates batch ownership".to_owned());
        }
        if !turn_ids.insert(turn.id.clone()) {
            return Err(
                "storage retention trash batch contains duplicate turn identity".to_owned(),
            );
        }
    }
    let mut memory_ids = BTreeSet::new();
    for memory in &batch.dataset.memories {
        if memory.scope_id != batch.scope_id || !memory.is_visible_to_subject(&batch.subject_id) {
            return Err("storage retention trash memory violates batch ownership".to_owned());
        }
        if !memory_ids.insert(memory.id.clone()) {
            return Err(
                "storage retention trash batch contains duplicate memory identity".to_owned(),
            );
        }
    }
    let mut profile_ids = BTreeSet::new();
    for profile_node in &batch.dataset.profile_nodes {
        if profile_node.scope_id != batch.scope_id {
            return Err("storage retention trash profile violates batch ownership".to_owned());
        }
        if !profile_ids.insert(profile_node.id.clone()) {
            return Err(
                "storage retention trash batch contains duplicate profile identity".to_owned(),
            );
        }
    }
    if batch
        .dataset
        .context_evidence
        .iter()
        .any(|evidence| !memory_ids.contains(&evidence.memory_id))
    {
        return Err("storage retention trash evidence has no memory in its batch".to_owned());
    }
    if batch
        .dataset
        .chat_compact_boundaries
        .iter()
        .any(|boundary| {
            boundary.subject_id != batch.subject_id || boundary.scope_id != batch.scope_id
        })
    {
        return Err("storage retention trash compact boundary violates batch ownership".to_owned());
    }
    if batch
        .analysis_attempts
        .iter()
        .any(|attempt| !turn_ids.contains(&attempt.turn_id))
    {
        return Err("storage retention trash analysis attempt has no turn in its batch".to_owned());
    }
    Ok(())
}

/// Validates one durable cold-turn job without applying scheduling policy.
/// 在不应用调度策略的前提下校验一个耐久冷回合任务。
fn validate_retention_cold_turn_job(job: &StorageRetentionColdTurnJob) -> Result<(), String> {
    if job.job_id.trim().is_empty() || job.job_id.len() > 512 {
        return Err(
            "storage snapshot contains an invalid retention cold-turn job identity".to_owned(),
        );
    }
    if job.next_run_at_ns < 0 {
        return Err("storage snapshot contains a negative cold-turn job timestamp".to_owned());
    }
    Ok(())
}

/// Defines the host-facing semantic storage contract.
/// 定义面向宿主的语义存储契约。
pub trait StoragePort {
    /// Executes one side-effect-free semantic query.
    /// 执行一次无副作用的语义查询。
    ///
    /// # Errors
    /// Returns an error when scope isolation, vector compatibility, or query shape is invalid.
    /// 当作用域隔离、向量兼容性或查询形状无效时返回错误。
    fn query(&self, query: &StorageQuery) -> Result<StorageQueryResult, CoreError>;

    /// Applies one mutation batch atomically and idempotently.
    /// 原子且幂等地应用一个变更批次。
    ///
    /// # Errors
    /// Returns an error only for malformed batch contracts; business conflicts return `Rejected`.
    /// 仅在批次契约格式错误时返回错误；业务冲突返回 `Rejected`。
    fn mutate(&mut self, batch: &MutationBatch) -> Result<MutationOutcome, CoreError>;

    /// Reconciles a previous mutation by durable idempotency key.
    /// 通过持久幂等键对账先前变更。
    #[must_use]
    fn reconcile(&self, idempotency_key: &IdempotencyKey) -> Option<MutationOutcome>;
}

/// Implements the full semantic storage contract in memory for tests and demos.
/// 为测试与 Demo 在内存中实现完整语义存储契约。
#[derive(Clone, Debug, Default)]
pub struct InMemoryStorage {
    /// Stores canonical turns by stable identity.
    /// 按稳定标识存储规范化对话轮次。
    turns: BTreeMap<TurnId, Turn>,
    /// Stores memories by stable identity.
    /// 按稳定标识存储记忆。
    memories: BTreeMap<MemoryId, MemoryRecord>,
    /// Stores profile nodes by stable identity.
    /// 按稳定标识存储画像节点。
    profile_nodes: BTreeMap<ProfileNodeId, ProfileNode>,
    /// Stores context evidence by ownership tuple.
    /// 按所有权元组存储情境证据。
    context_evidence: BTreeMap<(MemoryId, String, String), MemoryContextEvidence>,
    /// Stores session compact boundaries by exact ownership tuple.
    /// 按精确所有权元组存储会话压缩边界。
    chat_compact_boundaries: BTreeMap<
        (
            SubjectId,
            vulcan_memorymesh_core_contracts::ScopeId,
            SessionSourceId,
        ),
        ChatCompactBoundary,
    >,
    /// Stores committed idempotency ledger entries keyed by durable idempotency key.
    /// 按持久幂等键存储已提交幂等账本条目。
    outcomes: BTreeMap<IdempotencyKey, LedgerEntry>,
    /// Stores persisted analysis attempts keyed by exact turn and generation.
    /// 按精确轮次与代次存储已持久化分析尝试。
    analysis_attempts: HashMap<(TurnId, u64), AnalysisAttemptIdentity>,
    /// Stores authoritative sessions by the exact subject/project/session tuple.
    /// 按精确主体/项目/会话元组保存权威会话。
    memory_sessions: BTreeMap<
        (
            SubjectId,
            vulcan_memorymesh_core_contracts::ScopeId,
            SessionSourceId,
        ),
        StorageMemorySession,
    >,
    /// Stores persisted semantic noise prototype bundles by an unambiguous typed tuple.
    /// 按无歧义强类型元组存储已持久化语义 Noise 原型束。
    noise_prototypes: BTreeMap<NoiseCacheKey, Vec<NoisePrototype>>,
    /// Stores complete recycle-bin batches by stable batch identity.
    /// 按稳定批次身份存储完整回收站批次。
    retention_trash_batches: BTreeMap<String, StorageRetentionTrashBatch>,
    /// Stores durable cold-turn jobs by stable job identity.
    /// 按稳定任务身份存储耐久冷回合任务。
    retention_cold_turn_jobs: BTreeMap<String, StorageRetentionColdTurnJob>,
    /// Carries the monotonic storage version.
    /// 携带单调递增存储版本。
    storage_version: u64,
    /// Monotonically allocates durable entity sequence numbers inside this reference host.
    /// 在该参考宿主内单调分配持久实体序列号。
    next_entity_sequence: u64,
}

/// Binds the invariant ownership and session boundary of one search operation.
/// 绑定一次搜索操作中不变的所有权与会话边界。
#[derive(Clone, Copy)]
struct MemorySearchContext<'a> {
    /// Applies the explicit business clock when the complete-observation query supplies one.
    /// 完整观察查询提供业务时钟时使用它；旧独立查询没有该字段。
    evaluated_at_ns: Option<i64>,
    /// Applies independent candidate scopes and exact origin-session filtering before scoring.
    /// 在评分前应用独立候选作用域及精确来源会话过滤。
    candidate_scope: Option<&'a vulcan_memorymesh_core_contracts::MemoryCandidateScope>,
    /// Identifies the exact subject whose memories may be returned.
    /// 标识允许返回其记忆的精确主体。
    subject_id: &'a SubjectId,
    /// Carries the caller-resolved write and visible scope contract.
    /// 携带调用方已解析的写入与可见作用域契约。
    scope_access: &'a ScopeAccess,
    /// Restricts both physical channels to the query's exact embedding space before truncation.
    /// 在截断前将两个物理通道限定为查询的精确嵌入空间。
    vector_space_id: &'a VectorSpaceId,
    /// Carries the optional exact-session compact visibility boundary.
    /// 携带可选的精确会话压缩可见边界。
    session_recall_boundary: Option<&'a SessionRecallBoundary>,
}

/// Groups the actor, exact scope, and immutable access identity guarding one memory mutation.
/// 汇集保护一次记忆变更的操作者、精确作用域与不可变访问身份。
#[derive(Clone, Copy)]
struct MemoryMutationGuard<'a> {
    /// Identifies the subject performing the authorized mutation.
    /// 标识执行授权变更的主体。
    actor_subject_id: &'a SubjectId,
    /// Identifies the exact authorized owning scope.
    /// 标识精确获授权所属作用域。
    scope_id: &'a vulcan_memorymesh_core_contracts::ScopeId,
    /// Carries the access identity observed during the preceding authorized read.
    /// 携带前序授权读取时观测到的访问身份。
    expected_access: &'a MemoryAccessIdentity,
}

impl InMemoryStorage {
    /// Creates an empty reference backend.
    /// 创建空参考后端。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a deterministic complete dataset snapshot.
    /// 返回确定性的完整数据集快照。
    #[must_use]
    pub fn dataset(&self) -> ExportDataset {
        ExportDataset {
            turns: self.turns.values().cloned().collect(),
            memories: self.memories.values().cloned().collect(),
            profile_nodes: self.profile_nodes.values().cloned().collect(),
            context_evidence: self.context_evidence.values().cloned().collect(),
            chat_compact_boundaries: self.chat_compact_boundaries.values().cloned().collect(),
        }
    }

    /// Captures every durable reference-backend state needed for restart recovery.
    /// 捕获重启恢复所需的参考后端全部持久状态。
    #[must_use]
    pub fn snapshot(&self) -> StorageSnapshot {
        // Hash-map traversal changes after reconstruction; snapshots must preserve a canonical attempt order.
        // 哈希表遍历顺序在重建后会变化；快照必须保持规范的提炼尝试顺序。
        let mut analysis_attempts = self.analysis_attempts.values().cloned().collect::<Vec<_>>();
        analysis_attempts.sort_by(|left, right| {
            (&left.turn_id, left.analysis_generation)
                .cmp(&(&right.turn_id, right.analysis_generation))
        });
        StorageSnapshot {
            dataset: self.dataset(),
            storage_version: self.storage_version,
            next_entity_sequence: self.next_entity_sequence,
            outcomes: self
                .outcomes
                .iter()
                .map(|(idempotency_key, entry)| StorageLedgerEntry {
                    operation_id: entry.operation_id.clone(),
                    request_fingerprint: entry.request_fingerprint.clone(),
                    outcome: entry.outcome.clone(),
                    idempotency_key: idempotency_key.clone(),
                })
                .collect(),
            analysis_attempts,
            memory_sessions: self.memory_sessions.values().cloned().collect(),
            noise_prototypes: self
                .noise_prototypes
                .iter()
                .map(|(key, prototypes)| StorageNoisePrototypeEntry {
                    identity: NoisePrototypeCacheIdentity {
                        scope_id: key.0.clone(),
                        language: key.1.clone(),
                        model: key.2.clone(),
                        dimension: key.3,
                        rules_hash: key.4.clone(),
                    },
                    prototypes: prototypes.clone(),
                })
                .collect(),
            retention_trash_batches: self.retention_trash_batches.values().cloned().collect(),
            retention_cold_turn_jobs: self.retention_cold_turn_jobs.values().cloned().collect(),
        }
    }

    /// Applies Core's relational visibility rules to one externally scored vector channel.
    /// 对一个由外部存储完成评分的向量通道应用 Core 的关系可见性规则。
    ///
    /// `query` must be a vector-bearing storage query and `candidates` must preserve the
    /// physical producer order. The returned records exactly cover the retained candidates.
    /// `query` 必须是携带向量的存储查询，`candidates` 必须保留物理生产者顺序。
    /// 返回记录会精确覆盖保留的候选。
    ///
    /// # Errors
    /// Returns a stable Core error for an incompatible query, score contract, or scope boundary.
    /// 当查询、评分契约或作用域边界不兼容时返回稳定 Core 错误。
    pub fn materialize_external_vector_candidates(
        &self,
        query: &StorageQuery,
        candidates: Vec<PhysicalCandidate>,
    ) -> Result<vulcan_memorymesh_core_contracts::CandidateChannelResult, CoreError> {
        self.materialize_external_candidates(query, candidates, false)
    }

    /// Applies authoritative visibility to an externally scored lexical page without recomputing scores.
    /// 对外部评分的词法页面应用权威可见性，不重新计算评分。
    /// `query` selects the lexical contract; `candidates` preserves physical BM25 order and identity.
    /// `query` 选择词法契约；`candidates` 保留物理 BM25 顺序和身份。
    /// Returns retained records and original score facts, or an explicit query/contract/scope error.
    /// 返回保留的记录及原始评分事实，或明确的查询、契约、作用域错误。
    pub fn materialize_external_lexical_candidates(
        &self,
        query: &StorageQuery,
        candidates: Vec<PhysicalCandidate>,
    ) -> Result<vulcan_memorymesh_core_contracts::CandidateChannelResult, CoreError> {
        self.materialize_external_candidates(query, candidates, true)
    }

    /// Shares relational eligibility for the two explicitly defined physical producer channels.
    /// 为两种明确定义的物理生产者通道共享关系资格判断。
    /// `lexical_channel` selects the declared query branch and contract, never a score-based guess.
    /// `lexical_channel` 选择声明的查询分支及契约，绝不依据分数猜测。
    /// Returns at most the query limit while preserving every retained producer score unchanged.
    /// 最多返回查询上限条记录，并原样保留每个被保留的生产者评分。
    fn materialize_external_candidates(
        &self,
        query: &StorageQuery,
        candidates: Vec<PhysicalCandidate>,
        lexical_channel: bool,
    ) -> Result<vulcan_memorymesh_core_contracts::CandidateChannelResult, CoreError> {
        // context and expected_contract come from the typed query's selected channel.
        // context 和 expected_contract 来自强类型查询选定的通道。
        let (context, limit, expected_contract) = match query {
            StorageQuery::CandidateObservation(request) => (
                MemorySearchContext {
                    evaluated_at_ns: Some(request.now_ns),
                    candidate_scope: request.boundary.candidate_scope.as_ref(),
                    subject_id: &request.subject_id,
                    scope_access: &request.scope_access,
                    vector_space_id: &request.vector_space_id,
                    session_recall_boundary: request.boundary.session_recall_boundary.as_ref(),
                },
                request.limit,
                if lexical_channel {
                    request.lexical_contract.as_ref().ok_or_else(|| {
                        storage_error(
                            ErrorCode::InvalidArgument,
                            "external lexical materialization requires an enabled lexical channel",
                        )
                    })?
                } else {
                    &request.vector_contract
                },
            ),
            StorageQuery::VectorCandidates {
                subject_id,
                scope_access,
                vector_space_id,
                boundary,
                limit,
                expected_contract,
                now_ns,
                ..
            } if !lexical_channel => (
                MemorySearchContext {
                    evaluated_at_ns: Some(*now_ns),
                    candidate_scope: boundary.candidate_scope.as_ref(),
                    subject_id,
                    scope_access,
                    vector_space_id,
                    session_recall_boundary: boundary.session_recall_boundary.as_ref(),
                },
                *limit,
                expected_contract,
            ),
            StorageQuery::LexicalCandidates {
                subject_id,
                scope_access,
                vector_space_id,
                boundary,
                limit,
                expected_contract,
                now_ns,
                ..
            } if lexical_channel => (
                MemorySearchContext {
                    evaluated_at_ns: Some(*now_ns),
                    candidate_scope: boundary.candidate_scope.as_ref(),
                    subject_id,
                    scope_access,
                    vector_space_id,
                    session_recall_boundary: boundary.session_recall_boundary.as_ref(),
                },
                *limit,
                expected_contract,
            ),
            _ => {
                return Err(storage_error(
                    ErrorCode::InvalidArgument,
                    "external materialization requires a storage query for the selected channel",
                ));
            }
        };

        validate_scope(context.scope_access)?;
        if let Some(scope) = context.candidate_scope {
            scope
                .validate(context.scope_access)
                .map_err(|message| storage_error(ErrorCode::ScopeViolation, message))?;
        }
        vulcan_memorymesh_core_contracts::validate_candidate_channel(&candidates)?;
        if candidates
            .iter()
            .any(|candidate| candidate.score.contract != *expected_contract)
        {
            return Err(storage_error(
                ErrorCode::ScoreContractViolation,
                "external candidates do not match the requested score contract",
            ));
        }

        let mut eligible = self
            .eligible_memories(context)
            .into_iter()
            .map(|memory| (memory.id.as_str().to_owned(), memory))
            .collect::<BTreeMap<_, _>>();
        let mut retained_candidates = Vec::new();
        let mut memories = Vec::new();
        for candidate in candidates {
            if retained_candidates.len() >= limit as usize {
                break;
            }
            let Some(memory) = eligible.remove(&candidate.memory_id) else {
                continue;
            };
            retained_candidates.push(candidate);
            memories.push(memory);
        }

        Ok(vulcan_memorymesh_core_contracts::CandidateChannelResult {
            candidates: retained_candidates,
            memories,
        })
    }

    /// Reconstructs the reference backend from one validated durable snapshot.
    /// 从一份已校验的持久快照重建参考后端。
    ///
    /// # Errors
    /// Returns an error when the snapshot contains duplicate identities or malformed cache rows.
    /// 当快照包含重复身份或格式错误的缓存行时返回错误。
    pub fn from_snapshot(snapshot: StorageSnapshot) -> Result<Self, String> {
        let mut storage = Self {
            storage_version: snapshot.storage_version,
            next_entity_sequence: snapshot.next_entity_sequence,
            ..Self::default()
        };

        for turn in snapshot.dataset.turns {
            if storage.turns.insert(turn.id.clone(), turn).is_some() {
                return Err("storage snapshot contains duplicate turn identity".to_owned());
            }
        }
        for memory in snapshot.dataset.memories {
            if storage.memories.insert(memory.id.clone(), memory).is_some() {
                return Err("storage snapshot contains duplicate memory identity".to_owned());
            }
        }
        for profile_node in snapshot.dataset.profile_nodes {
            if storage
                .profile_nodes
                .insert(profile_node.id.clone(), profile_node)
                .is_some()
            {
                return Err("storage snapshot contains duplicate profile identity".to_owned());
            }
        }
        for evidence in snapshot.dataset.context_evidence {
            let key = (
                evidence.memory_id.clone(),
                evidence.context_key.clone(),
                evidence.context_value.clone(),
            );
            if storage.context_evidence.insert(key, evidence).is_some() {
                return Err("storage snapshot contains duplicate context evidence".to_owned());
            }
        }
        for boundary in snapshot.dataset.chat_compact_boundaries {
            let key = (
                boundary.subject_id.clone(),
                boundary.scope_id.clone(),
                boundary.session_source_id.clone(),
            );
            if storage
                .chat_compact_boundaries
                .insert(key, boundary)
                .is_some()
            {
                return Err("storage snapshot contains duplicate compact boundary".to_owned());
            }
        }
        for entry in snapshot.outcomes {
            if storage
                .outcomes
                .insert(
                    entry.idempotency_key,
                    LedgerEntry {
                        operation_id: entry.operation_id,
                        request_fingerprint: entry.request_fingerprint,
                        outcome: entry.outcome,
                    },
                )
                .is_some()
            {
                return Err("storage snapshot contains duplicate idempotency key".to_owned());
            }
        }
        for attempt in snapshot.analysis_attempts {
            let key = (attempt.turn_id.clone(), attempt.analysis_generation);
            if storage.analysis_attempts.insert(key, attempt).is_some() {
                return Err("storage snapshot contains duplicate analysis attempt".to_owned());
            }
        }
        for session in snapshot.memory_sessions {
            if session.last_extraction_observed_at_ns < 0
                || match session.last_extraction_completed_at_ns {
                    None => session.last_extraction_observed_at_ns != 0,
                    Some(completed) => completed < session.last_extraction_observed_at_ns,
                }
            {
                return Err(
                    "storage snapshot contains an invalid session extraction window".to_owned(),
                );
            }
            // key preserves exact ownership even when projects use the same host-session string.
            // key 即使面对不同项目下同名宿主会话，也保留精确归属。
            let key = (
                session.subject_id.clone(),
                session.scope_id.clone(),
                session.session_source_id.clone(),
            );
            if storage.memory_sessions.insert(key, session).is_some() {
                return Err(
                    "storage snapshot contains duplicate memory session ownership".to_owned(),
                );
            }
        }
        for entry in snapshot.noise_prototypes {
            if entry
                .prototypes
                .iter()
                .any(|prototype| prototype.vector.is_empty())
            {
                return Err("storage snapshot contains an empty noise prototype vector".to_owned());
            }
            if storage
                .noise_prototypes
                .insert(noise_cache_key(&entry.identity), entry.prototypes)
                .is_some()
            {
                return Err("storage snapshot contains duplicate noise cache identity".to_owned());
            }
        }
        // Recycle-bin records remain live durable identities until purge. Reserve active and
        // previously loaded trash identities together so restore can never overwrite another row.
        // 回收站记录在永久清除前仍是有效耐久身份；统一保留活动区与已加载回收数据身份，避免恢复覆盖其他记录。
        let mut reserved_turn_ids = storage.turns.keys().cloned().collect::<BTreeSet<_>>();
        let mut reserved_memory_ids = storage.memories.keys().cloned().collect::<BTreeSet<_>>();
        let mut reserved_profile_ids = storage
            .profile_nodes
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut reserved_evidence_ids = storage
            .context_evidence
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut reserved_boundary_ids = storage
            .chat_compact_boundaries
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut reserved_attempt_ids = storage
            .analysis_attempts
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        // A recycled session may have only one batch owning its original lifecycle and watermarks.
        // 一个已回收会话只能由一个批次持有其原生命周期和水位。
        let mut reserved_manual_sessions = BTreeSet::new();
        for batch in snapshot.retention_trash_batches {
            validate_retention_trash_batch(&batch)?;
            manual_recycle::validate_selection(&batch, &storage, &mut reserved_manual_sessions)?;
            if batch
                .dataset
                .turns
                .iter()
                .any(|turn| !reserved_turn_ids.insert(turn.id.clone()))
            {
                return Err(
                    "storage snapshot contains an active-or-trash turn identity collision"
                        .to_owned(),
                );
            }
            if batch
                .dataset
                .memories
                .iter()
                .any(|memory| !reserved_memory_ids.insert(memory.id.clone()))
            {
                return Err(
                    "storage snapshot contains an active-or-trash memory identity collision"
                        .to_owned(),
                );
            }
            if batch
                .dataset
                .profile_nodes
                .iter()
                .any(|node| !reserved_profile_ids.insert(node.id.clone()))
            {
                return Err(
                    "storage snapshot contains an active-or-trash profile identity collision"
                        .to_owned(),
                );
            }
            if batch.dataset.context_evidence.iter().any(|evidence| {
                !reserved_evidence_ids.insert((
                    evidence.memory_id.clone(),
                    evidence.context_key.clone(),
                    evidence.context_value.clone(),
                ))
            }) {
                return Err(
                    "storage snapshot contains an active-or-trash context evidence collision"
                        .to_owned(),
                );
            }
            if batch
                .dataset
                .chat_compact_boundaries
                .iter()
                .any(|boundary| {
                    !reserved_boundary_ids.insert((
                        boundary.subject_id.clone(),
                        boundary.scope_id.clone(),
                        boundary.session_source_id.clone(),
                    ))
                })
            {
                return Err(
                    "storage snapshot contains an active-or-trash compact boundary collision"
                        .to_owned(),
                );
            }
            if batch.analysis_attempts.iter().any(|attempt| {
                !reserved_attempt_ids.insert((attempt.turn_id.clone(), attempt.analysis_generation))
            }) {
                return Err(
                    "storage snapshot contains an active-or-trash analysis attempt collision"
                        .to_owned(),
                );
            }
            if storage
                .retention_trash_batches
                .insert(batch.batch_id.clone(), batch)
                .is_some()
            {
                return Err(
                    "storage snapshot contains duplicate retention trash batch identity".to_owned(),
                );
            }
        }
        let mut reserved_cold_sessions = BTreeSet::new();
        for job in snapshot.retention_cold_turn_jobs {
            validate_retention_cold_turn_job(&job)?;
            if !reserved_cold_sessions.insert((
                job.subject_id.clone(),
                job.scope_id.clone(),
                job.session_source_id.clone(),
            )) {
                return Err(
                    "storage snapshot contains duplicate retention cold-turn session identity"
                        .to_owned(),
                );
            }
            if storage
                .retention_cold_turn_jobs
                .insert(job.job_id.clone(), job)
                .is_some()
            {
                return Err(
                    "storage snapshot contains duplicate retention cold-turn job identity"
                        .to_owned(),
                );
            }
        }
        Ok(storage)
    }

    /// Reports the current monotonic storage version.
    /// 报告当前单调递增存储版本。
    #[must_use]
    pub const fn storage_version(&self) -> u64 {
        self.storage_version
    }

    /// Borrows one active-store memory by exact identity without copying its embedding.
    /// 按准确身份借用一条活动存储记忆，不复制嵌入。
    /// Takes a canonical memory ID; returns no record for recycled or permanently removed content.
    /// 接收规范记忆 ID；内容已回收或永久移除时不返回记录。
    pub fn memory_record(&self, memory_id: &MemoryId) -> Option<&MemoryRecord> {
        self.memories.get(memory_id)
    }

    /// Borrows one stored profile node by exact identity without scanning or copying the dataset.
    /// 按准确身份借用一个已存储画像节点，不扫描或复制数据集。
    /// Takes a canonical node ID; returns the stored node, whose state and visibility still require validation.
    /// 接收规范节点 ID；返回存储节点，其状态及可见性仍需校验。
    pub fn profile_node_record(&self, node_id: &ProfileNodeId) -> Option<&ProfileNode> {
        self.profile_nodes.get(node_id)
    }

    /// Borrows authoritative session metadata by its complete ownership tuple.
    /// 按完整归属元组借用权威会话元数据。
    /// Takes subject, write scope and source session; absence means no registered metadata.
    /// 接收主体、写入作用域及来源会话；缺失表示没有登记元数据。
    pub fn memory_session(
        &self,
        subject_id: &SubjectId,
        scope_id: &vulcan_memorymesh_core_contracts::ScopeId,
        session_source_id: &SessionSourceId,
    ) -> Option<&StorageMemorySession> {
        self.memory_sessions.get(&(
            subject_id.clone(),
            scope_id.clone(),
            session_source_id.clone(),
        ))
    }

    /// Allocates a fresh identity without overwriting caller-assigned or imported opaque identities.
    /// 分配新身份，不覆盖调用方分配或导入的不透明身份。
    fn allocate_domain_id(&mut self, kind: EntityKind) -> Result<String, String> {
        let prefix = match kind {
            EntityKind::Turn => "turn",
            EntityKind::Memory => "memory",
            EntityKind::ProfileNode => "profile",
            EntityKind::ContextEvidence => {
                return Err("context evidence uses composite identities".to_owned());
            }
        };
        loop {
            self.next_entity_sequence = self
                .next_entity_sequence
                .checked_add(1)
                .ok_or_else(|| "domain identity sequence is exhausted".to_owned())?;
            let domain_id = format!("vmm-{prefix}-{}", self.next_entity_sequence);
            // Imports do not synchronize this local counter; collision checks stay inside the transaction copy.
            // 导入不会同步此局部计数器；碰撞检查始终位于事务副本内部。
            let occupied = match kind {
                EntityKind::Turn => {
                    let identity =
                        TurnId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                    self.turns.contains_key(&identity)
                        || self
                            .retention_trash_batches
                            .values()
                            .any(|batch| batch.dataset.turns.iter().any(|turn| turn.id == identity))
                }
                EntityKind::Memory => {
                    let identity =
                        MemoryId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                    self.memories.contains_key(&identity)
                        || self.retention_trash_batches.values().any(|batch| {
                            batch
                                .dataset
                                .memories
                                .iter()
                                .any(|memory| memory.id == identity)
                        })
                }
                EntityKind::ProfileNode => {
                    let identity =
                        ProfileNodeId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                    self.profile_nodes.contains_key(&identity)
                        || self.retention_trash_batches.values().any(|batch| {
                            batch
                                .dataset
                                .profile_nodes
                                .iter()
                                .any(|node| node.id == identity)
                        })
                }
                EntityKind::ContextEvidence => {
                    return Err("context evidence uses composite identities".to_owned());
                }
            };
            if !occupied {
                return Ok(domain_id);
            }
        }
    }

    /// Builds one closed physical candidate score under the requested contract.
    /// 在请求契约下构建一个封闭物理候选评分。
    fn physical_score(
        raw_value: f32,
        kind: RawScoreKind,
        direction: ScoreDirection,
        contract_id: &str,
    ) -> PhysicalCandidateScore {
        PhysicalCandidateScore {
            raw_value: f64::from(raw_value),
            raw_score_kind: kind,
            direction,
            decision_score: Some(f64::from(raw_value.clamp(0.0, 1.0))),
            contract: ScoreContractRef {
                id: contract_id.to_owned(),
                version: REFERENCE_CONTRACT_VERSION,
            },
        }
    }

    /// Selects active visible memories eligible for a candidate channel.
    /// 选择具备候选通道资格的活跃可见记忆。
    fn eligible_memories(&self, context: MemorySearchContext<'_>) -> Vec<MemoryRecord> {
        self.memories
            .values()
            .filter(|memory| {
                memory.is_visible_to_subject(context.subject_id)
                    && context
                        .evaluated_at_ns
                        .is_none_or(|now| memory.expires_at_ns.is_none_or(|expiry| expiry > now))
                    && context
                        .scope_access
                        .visible_scopes
                        .contains(&memory.scope_id)
                    && memory.state == MemoryState::Active
                    && memory.vector_space_id.as_ref() == Some(context.vector_space_id)
                    && context
                        .candidate_scope
                        .is_none_or(|scope| scope.matches(memory))
                    && self.memory_visible_for_session_recall(memory, context)
            })
            .cloned()
            .collect()
    }

    /// Scores eligible memories with cosine similarity for a vector channel.
    /// 使用余弦相似度为向量通道给可见记忆评分。
    fn vector_candidates(
        &self,
        context: MemorySearchContext<'_>,
        embeddings: &[Vec<f32>],
        limit: u32,
        contract: &ScoreContractRef,
    ) -> Result<Vec<PhysicalCandidate>, CoreError> {
        if contract.id != REFERENCE_VECTOR_CONTRACT_ID
            || contract.version != REFERENCE_CONTRACT_VERSION
        {
            return Err(storage_error(
                ErrorCode::ScoreContractViolation,
                "reference backend only serves the frozen reference cosine contract",
            ));
        }
        let eligible = self.eligible_memories(context);
        let mut scored = eligible
            .iter()
            .filter_map(|memory| {
                let vector = memory.embedding.as_ref()?;
                let best = embeddings
                    .iter()
                    .filter_map(|query| cosine_similarity(query, vector))
                    .fold(0.0_f32, f32::max);
                if best <= 0.0 {
                    return None;
                }
                Some(PhysicalCandidate {
                    memory_id: memory.id.as_str().to_owned(),
                    score: Self::physical_score(
                        best,
                        RawScoreKind::CosineSimilarity,
                        ScoreDirection::HigherIsBetter,
                        REFERENCE_VECTOR_CONTRACT_ID,
                    ),
                    physical_rank: 0,
                })
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| {
            right
                .score
                .raw_value
                .clamp(0.0, 1.0)
                .total_cmp(&left.score.raw_value.clamp(0.0, 1.0))
                .then_with(|| left.memory_id.cmp(&right.memory_id))
        });
        scored.truncate(limit as usize);
        assign_ranks(&mut scored);
        Ok(scored)
    }

    /// Scores eligible memories with lexical overlap for a lexical channel.
    /// 使用词法重合度为词法通道给可见记忆评分。
    fn lexical_candidates(
        &self,
        context: MemorySearchContext<'_>,
        queries: &[String],
        limit: u32,
        contract: &ScoreContractRef,
    ) -> Result<Vec<PhysicalCandidate>, CoreError> {
        if contract.id != REFERENCE_LEXICAL_CONTRACT_ID
            || contract.version != REFERENCE_CONTRACT_VERSION
        {
            return Err(storage_error(
                ErrorCode::ScoreContractViolation,
                "reference backend only serves the frozen reference lexical contract",
            ));
        }
        let eligible = self.eligible_memories(context);
        let mut scored = eligible
            .iter()
            .map(|memory| {
                let best = queries
                    .iter()
                    .map(|query| lexical_score(&memory.content, query))
                    .fold(0.0_f32, f32::max);
                PhysicalCandidate {
                    memory_id: memory.id.as_str().to_owned(),
                    score: Self::physical_score(
                        best,
                        RawScoreKind::TokenOverlap,
                        ScoreDirection::HigherIsBetter,
                        REFERENCE_LEXICAL_CONTRACT_ID,
                    ),
                    physical_rank: 0,
                }
            })
            .filter(|candidate| candidate.score.raw_value > 0.0)
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| {
            right
                .score
                .raw_value
                .clamp(0.0, 1.0)
                .total_cmp(&left.score.raw_value.clamp(0.0, 1.0))
                .then_with(|| left.memory_id.cmp(&right.memory_id))
        });
        scored.truncate(limit as usize);
        assign_ranks(&mut scored);
        Ok(scored)
    }

    /// Materializes records aligned with one candidate channel under the same immutable storage borrow.
    /// 在同一次不可变存储借用下物化与候选通道对齐的记录。
    fn candidate_channel_memories(
        &self,
        candidates: &[PhysicalCandidate],
    ) -> Result<Vec<MemoryRecord>, CoreError> {
        candidates
            .iter()
            .map(|candidate| {
                let id = MemoryId::new(candidate.memory_id.clone()).map_err(|error| {
                    storage_error(ErrorCode::InvalidIdentifier, error.to_string())
                })?;
                self.memories.get(&id).cloned().ok_or_else(|| {
                    storage_error(
                        ErrorCode::StorageConflict,
                        "candidate record disappeared during source observation",
                    )
                })
            })
            .collect()
    }

    /// Applies compact-aware visibility only to evidence from the exact current session.
    /// 仅对来自精确当前会话的证据应用压缩感知可见性。
    fn memory_visible_for_session_recall(
        &self,
        memory: &MemoryRecord,
        context: MemorySearchContext<'_>,
    ) -> bool {
        let Some(boundary_request) = context.session_recall_boundary else {
            return true;
        };
        if boundary_request.mode == SessionRecallMode::Full {
            return true;
        }
        let current_session_turns = memory
            .source_turn_id
            .iter()
            .filter_map(|turn_id| self.turns.get(turn_id))
            .filter(|turn| {
                turn.subject_id == *context.subject_id
                    && turn.scope_id == context.scope_access.write_scope
                    && turn.session_source_id == boundary_request.session_source_id
            })
            .collect::<Vec<_>>();
        if current_session_turns.is_empty() {
            return true;
        }
        let key = (
            context.subject_id.clone(),
            context.scope_access.write_scope.clone(),
            boundary_request.session_source_id.clone(),
        );
        let Some(boundary) = self.chat_compact_boundaries.get(&key) else {
            // Before the first compact, no turn-derived memory from this session is visible.
            // 首次压缩前，当前会话中源自轮次的记忆均不可见。
            return false;
        };
        let Some(boundary_turn) = self.turns.get(&boundary.compacted_turn_id) else {
            // Imported and mutated state validates this reference; fail closed if corrupted.
            // 导入与变更状态会校验该引用；若状态损坏则采用封闭失败。
            return false;
        };
        // Opaque turn IDs are never compared as counters; canonical time plus ID is the total order.
        // 不将不透明轮次标识作为计数器比较；规范时间加标识构成全序。
        current_session_turns.iter().any(|turn| {
            (turn.created_at_ns, &turn.id) <= (boundary_turn.created_at_ns, &boundary_turn.id)
        })
    }

    /// Reads every active atomic profile statement in deterministic scope, content, and identity order.
    /// 以确定性的作用域、内容与身份顺序读取全部活跃原子画像陈述。
    fn profile_nodes(
        &self,
        subject_id: &SubjectId,
        scope_access: &ScopeAccess,
        sections: &[ProfileViewSection],
        evaluated_at_ns: i64,
    ) -> Result<Vec<ProfileNode>, CoreError> {
        validate_scope(scope_access)?;
        if sections.is_empty() {
            return Err(storage_error(
                ErrorCode::InvalidArgument,
                "profile sections must not be empty",
            ));
        }
        let visible_scopes = scope_access.visible_scopes.iter().collect::<BTreeSet<_>>();
        let mut section_scopes = BTreeSet::new();
        let mut selected = Vec::new();
        for section in sections {
            if !visible_scopes.contains(&section.scope_id)
                || !section_scopes.insert(&section.scope_id)
            {
                return Err(storage_error(
                    ErrorCode::InvalidArgument,
                    "profile sections must use unique visible scopes",
                ));
            }
            let mut nodes = self
                .profile_nodes
                .values()
                .filter(|node| {
                    node.scope_id == section.scope_id
                        && match section.role {
                            SectionRole::Environment => true,
                            SectionRole::Subject => node.subject_id.as_ref() == Some(subject_id),
                        }
                        && node.state == ProfileNodeState::Active
                        && node
                            .effective_date_ns
                            .is_none_or(|effective_at_ns| effective_at_ns <= evaluated_at_ns)
                        && node
                            .expires_at_ns
                            .is_none_or(|expires_at_ns| expires_at_ns > evaluated_at_ns)
                })
                .cloned()
                .collect::<Vec<_>>();
            nodes.sort_by(|left, right| {
                left.content
                    .cmp(&right.content)
                    .then_with(|| left.id.cmp(&right.id))
            });
            selected.extend(nodes);
        }
        Ok(selected)
    }

    /// Applies one mutation to a detached transactional copy with slot binding tracking.
    /// 在带槽位绑定跟踪的独立事务副本上应用一次变更。
    #[allow(clippy::too_many_lines)]
    fn apply_mutation(
        &mut self,
        mutation: &Mutation,
        slots: &mut BTreeMap<String, EntityBinding>,
        advanced: &mut Vec<VersionedEntity>,
        committed_at_ns: Option<i64>,
    ) -> Result<(), String> {
        match mutation {
            Mutation::PutTurn { entity, turn } => {
                self.put_turn(entity, turn.clone(), slots, advanced)
            }
            Mutation::TransitionTurnStatus {
                turn_id,
                from,
                to,
                user_input_kind,
                analysis_details,
            } => self.transition_turn_status(
                turn_id,
                *from,
                *to,
                *user_input_kind,
                analysis_details.clone(),
            ),
            Mutation::AdvanceChatCompactBoundary { .. } => {
                Err("advance compact boundary must be the only mutation in its batch".to_owned())
            }
            Mutation::PutChatCompactBoundary(boundary) => {
                self.put_chat_compact_boundary(boundary.clone())
            }
            Mutation::RestoreImportedMemory(record) => {
                self.restore_imported_memory(record.clone(), advanced)
            }
            Mutation::RestoreImportedProfileNode(node) => {
                self.restore_imported_profile_node(node.clone(), advanced)
            }
            Mutation::RestoreImportedContextEvidence(evidence) => {
                self.restore_imported_context_evidence(evidence.clone(), advanced)
            }
            Mutation::PutMemory { entity, record } => {
                self.put_memory(entity, record.clone(), slots, advanced)
            }
            Mutation::ReinforceMemory {
                actor_subject_id,
                scope_id,
                expected_access,
                memory_id,
                expected_version,
                recalled_at_ns,
                source_session_id,
            } => self.reinforce_memory(
                MemoryMutationGuard {
                    actor_subject_id,
                    scope_id,
                    expected_access,
                },
                memory_id,
                *expected_version,
                *recalled_at_ns,
                source_session_id.as_ref(),
                advanced,
            ),
            Mutation::SupersedeMemory {
                actor_subject_id,
                scope_id,
                expected_access,
                memory_id,
                expected_version,
                superseded_by,
                status_reason,
            } => self.supersede_memory(
                MemoryMutationGuard {
                    actor_subject_id,
                    scope_id,
                    expected_access,
                },
                memory_id,
                *expected_version,
                superseded_by,
                status_reason.clone(),
                slots,
                advanced,
            ),
            Mutation::AggregateContextEvidence {
                memory,
                context_key,
                context_value,
                relation,
                occurred_at_ns,
            } => self.aggregate_context_evidence(
                memory,
                context_key,
                context_value,
                *relation,
                *occurred_at_ns,
                slots,
                advanced,
            ),
            Mutation::DeleteMemory {
                actor_subject_id,
                scope_id,
                expected_access,
                memory_id,
                expected_version,
            } => self.delete_memory(
                MemoryMutationGuard {
                    actor_subject_id,
                    scope_id,
                    expected_access,
                },
                memory_id,
                *expected_version,
            ),
            Mutation::PutProfileNode { entity, node } => {
                self.put_profile_node(entity, node.clone(), slots, advanced)
            }
            Mutation::RetireProfileNode {
                subject_id,
                scope_id,
                profile_node_id,
                expected_version,
                superseded_by,
                status_reason,
            } => self.retire_profile_node(
                subject_id.as_ref(),
                scope_id,
                profile_node_id,
                *expected_version,
                superseded_by.clone(),
                status_reason.clone(),
                slots,
                advanced,
            ),
            Mutation::PutAnalysisAttempt(attempt) => self.put_analysis_attempt(attempt.clone()),
            Mutation::AdvanceSessionExtraction {
                turn_id,
                observed_after_ns,
                observed_through_ns,
            } => self.advance_session_extraction(
                turn_id,
                *observed_after_ns,
                *observed_through_ns,
                committed_at_ns,
            ),
            Mutation::PutNoisePrototypeBundle {
                identity,
                prototypes,
            } => self.put_noise_prototype_bundle(identity, prototypes),
            Mutation::RecycleMemory {
                actor_subject_id,
                scope_id,
                expected_access,
                memory_id,
                expected_version,
                status_reason,
            } => self.recycle_memory(
                MemoryMutationGuard {
                    actor_subject_id,
                    scope_id,
                    expected_access,
                },
                memory_id,
                *expected_version,
                status_reason.clone(),
                advanced,
            ),
            Mutation::RestoreMemory {
                actor_subject_id,
                scope_id,
                expected_access,
                memory_id,
                expected_version,
            } => self.restore_memory(
                MemoryMutationGuard {
                    actor_subject_id,
                    scope_id,
                    expected_access,
                },
                memory_id,
                *expected_version,
                advanced,
            ),
            Mutation::PurgeMemory {
                actor_subject_id,
                scope_id,
                expected_access,
                memory_id,
                expected_version,
            } => self.purge_memory(
                MemoryMutationGuard {
                    actor_subject_id,
                    scope_id,
                    expected_access,
                },
                memory_id,
                *expected_version,
            ),
        }
    }

    /// Stores one turn through a slot binding or an exact existing identity.
    /// 通过槽位绑定或精确既有身份存储一个轮次。
    fn put_turn(
        &mut self,
        entity: &EntityRef,
        mut turn: Turn,
        slots: &mut BTreeMap<String, EntityBinding>,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        validate_turn_analysis_state(&turn)?;
        // Registration creates the session without claiming that extraction has succeeded.
        // 登记仅创建会话，不宣称提取已成功。
        let session_key = (
            turn.subject_id.clone(),
            turn.scope_id.clone(),
            turn.session_source_id.clone(),
        );
        let stored_session =
            self.memory_sessions
                .entry(session_key)
                .or_insert_with(|| StorageMemorySession {
                    status: StorageSessionStatus::Active,
                    subject_id: turn.subject_id.clone(),
                    scope_id: turn.scope_id.clone(),
                    session_source_id: turn.session_source_id.clone(),
                    last_extraction_observed_at_ns: 0,
                    last_extraction_completed_at_ns: None,
                });
        // Registration must not reactivate a recycled or forgotten session through ordinary intake.
        // 普通入库登记不得重新激活已回收或已遗忘会话。
        if !stored_session.status.allows_runtime_memory() {
            return Err("memory session does not allow turn intake".to_owned());
        }
        match entity {
            EntityRef::New(slot) => {
                if slots.contains_key(slot.as_str()) {
                    return Err(format!("duplicate entity slot: {slot}"));
                }
                let domain_id = self.allocate_domain_id(EntityKind::Turn)?;
                let turn_id = TurnId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                turn.id = turn_id.clone();
                self.turns.insert(turn_id.clone(), turn);
                slots.insert(
                    slot.as_str().to_owned(),
                    EntityBinding {
                        entity_slot_id: slot.clone(),
                        entity_kind: EntityKind::Turn,
                        domain_id,
                    },
                );
                advanced.push(VersionedEntity {
                    entity_kind: EntityKind::Turn,
                    domain_id: turn_id.as_str().to_owned(),
                    version: 1,
                });
                Ok(())
            }
            EntityRef::Existing {
                domain_id,
                expected_version,
            } => {
                let turn_id = TurnId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                if turn.id != turn_id {
                    return Err("turn identity conflicts with entity ref".to_owned());
                }
                if let Some(existing) = self.turns.get(&turn_id) {
                    if *expected_version == 1 && existing == &turn {
                        return Ok(());
                    }
                    return Err(format!("turn identity or version conflict: {turn_id}"));
                }
                if *expected_version != 0 {
                    return Err(format!("turn version conflict: {turn_id}"));
                }
                self.turns.insert(turn_id.clone(), turn);
                advanced.push(VersionedEntity {
                    entity_kind: EntityKind::Turn,
                    domain_id: turn_id.as_str().to_owned(),
                    version: 1,
                });
                Ok(())
            }
        }
    }

    /// Transitions one turn between explicit durable states with version guard.
    /// 在版本守护下在显式持久状态之间迁移一个轮次。
    fn transition_turn_status(
        &mut self,
        turn_id: &TurnId,
        from: TurnStatus,
        to: TurnStatus,
        user_input_kind: Option<vulcan_memorymesh_core_contracts::TurnInputKind>,
        analysis_details: Option<String>,
    ) -> Result<(), String> {
        let turn = self
            .turns
            .get_mut(turn_id)
            .ok_or_else(|| format!("turn not found: {turn_id}"))?;
        if turn.status != from {
            return Err(format!("turn status conflict: {turn_id}"));
        }
        if from != TurnStatus::Pending || to != TurnStatus::Completed {
            return Err("turn transition must advance Pending to Completed".to_owned());
        }
        turn.status = to;
        turn.user_input_kind = user_input_kind;
        turn.analysis_details = analysis_details;
        validate_turn_analysis_state(turn)?;
        Ok(())
    }

    /// Restores one exact compact boundary after validating its referenced turn ownership.
    /// 在校验引用轮次所有权后恢复一个精确压缩边界。
    fn put_chat_compact_boundary(&mut self, boundary: ChatCompactBoundary) -> Result<(), String> {
        let turn = self.turns.get(&boundary.compacted_turn_id).ok_or_else(|| {
            format!(
                "chat compact boundary turn not found: {}",
                boundary.compacted_turn_id
            )
        })?;
        if turn.subject_id != boundary.subject_id
            || turn.scope_id != boundary.scope_id
            || turn.session_source_id != boundary.session_source_id
        {
            return Err(format!(
                "chat compact boundary ownership conflict: {}",
                boundary.compacted_turn_id
            ));
        }
        self.chat_compact_boundaries.insert(
            (
                boundary.subject_id.clone(),
                boundary.scope_id.clone(),
                boundary.session_source_id.clone(),
            ),
            boundary,
        );
        Ok(())
    }

    /// Restores one absent memory without rewriting its exported optimistic version.
    /// 在不改写导出乐观版本的前提下恢复一条不存在的记忆。
    fn restore_imported_memory(
        &mut self,
        record: MemoryRecord,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        validate_memory_scores(&record)?;
        if record.version == 0 {
            return Err(format!("imported memory version is zero: {}", record.id));
        }
        if self.memories.contains_key(&record.id) {
            return Err(format!("imported memory identity conflict: {}", record.id));
        }
        for evidence in &record.evidence {
            let turn = self.turns.get(&evidence.turn_id).ok_or_else(|| {
                format!(
                    "imported memory evidence turn not found: {}",
                    evidence.turn_id
                )
            })?;
            if turn.scope_id != record.scope_id
                || record
                    .source_subject_id
                    .as_ref()
                    .is_some_and(|source| turn.subject_id != *source)
            {
                return Err(format!(
                    "imported memory evidence ownership conflict: {}",
                    evidence.turn_id
                ));
            }
        }
        if let Some(source_turn_id) = &record.source_turn_id {
            let turn = self.turns.get(source_turn_id).ok_or_else(|| {
                format!("imported memory source turn not found: {source_turn_id}")
            })?;
            if turn.scope_id != record.scope_id
                || record
                    .source_subject_id
                    .as_ref()
                    .is_some_and(|source| turn.subject_id != *source)
            {
                return Err(format!(
                    "imported memory source ownership conflict: {source_turn_id}"
                ));
            }
        }
        let memory_id = record.id.clone();
        let version = record.version;
        self.memories.insert(memory_id.clone(), record);
        advanced.push(VersionedEntity {
            entity_kind: EntityKind::Memory,
            domain_id: memory_id.as_str().to_owned(),
            version,
        });
        Ok(())
    }

    /// Restores one absent profile node without rewriting its exported optimistic version.
    /// 在不改写导出乐观版本的前提下恢复一个不存在的画像节点。
    fn restore_imported_profile_node(
        &mut self,
        node: ProfileNode,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        if validate_profile_node_snapshot(&node).is_err() || node.version == 0 {
            return Err(format!("imported profile node is invalid: {}", node.id));
        }
        if self.profile_nodes.contains_key(&node.id) {
            return Err(format!(
                "imported profile node identity conflict: {}",
                node.id
            ));
        }
        for turn_id in &node.source_turn_ids {
            let turn = self
                .turns
                .get(turn_id)
                .ok_or_else(|| format!("imported profile source turn not found: {turn_id}"))?;
            if node.subject_id.as_ref() != Some(&turn.subject_id) {
                return Err(format!(
                    "imported profile source ownership conflict: {turn_id}"
                ));
            }
        }
        let node_id = node.id.clone();
        let version = node.version;
        self.profile_nodes.insert(node_id.clone(), node);
        advanced.push(VersionedEntity {
            entity_kind: EntityKind::ProfileNode,
            domain_id: node_id.as_str().to_owned(),
            version,
        });
        Ok(())
    }

    /// Restores one absent context-evidence aggregate with its exact counters and timestamps.
    /// 使用精确计数与时间戳恢复一条不存在的情境证据聚合。
    fn restore_imported_context_evidence(
        &mut self,
        evidence: MemoryContextEvidence,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        validate_context_evidence_snapshot(&evidence)?;
        if !self.memories.contains_key(&evidence.memory_id) {
            return Err(format!(
                "imported context evidence target memory missing: {}",
                evidence.memory_id
            ));
        }
        let key = (
            evidence.memory_id.clone(),
            evidence.context_key.clone(),
            evidence.context_value.clone(),
        );
        if self.context_evidence.contains_key(&key) {
            return Err(format!(
                "imported context evidence identity conflict: {}|{}|{}",
                evidence.memory_id, evidence.context_key, evidence.context_value
            ));
        }
        let evidence_domain_id = context_evidence_domain_id(
            &evidence.memory_id,
            &evidence.context_key,
            &evidence.context_value,
        );
        let version = evidence.version;
        self.context_evidence.insert(key, evidence);
        advanced.push(VersionedEntity {
            entity_kind: EntityKind::ContextEvidence,
            domain_id: evidence_domain_id,
            version,
        });
        Ok(())
    }

    /// Atomically selects the latest exact-session turn and advances its durable boundary.
    /// 原子选择精确会话的最新轮次并推进其持久边界。
    fn advance_chat_compact_boundary(
        &mut self,
        subject_id: &SubjectId,
        scope_id: &vulcan_memorymesh_core_contracts::ScopeId,
        session_source_id: &SessionSourceId,
        compacted_at_ns: i64,
    ) -> ChatCompactMutationResult {
        // Use canonical time and identity ordering because opaque Core turn identities are not numeric.
        // 使用规范时间与身份排序，因为 Core 的不透明轮次身份不是数值。
        let latest = self
            .turns
            .values()
            .filter(|turn| {
                turn.subject_id == *subject_id
                    && turn.scope_id == *scope_id
                    && turn.session_source_id == *session_source_id
            })
            .max_by(|left, right| {
                left.created_at_ns
                    .cmp(&right.created_at_ns)
                    .then_with(|| left.id.cmp(&right.id))
            });
        let Some(latest) = latest else {
            return ChatCompactMutationResult {
                accepted: true,
                updated: false,
                compacted_turn_id: None,
            };
        };
        let key = (
            subject_id.clone(),
            scope_id.clone(),
            session_source_id.clone(),
        );
        if self
            .chat_compact_boundaries
            .get(&key)
            .is_some_and(|boundary| boundary.compacted_turn_id == latest.id)
        {
            return ChatCompactMutationResult {
                accepted: true,
                updated: false,
                compacted_turn_id: Some(latest.id.clone()),
            };
        }
        let compacted_turn_id = latest.id.clone();
        self.chat_compact_boundaries.insert(
            key,
            ChatCompactBoundary {
                subject_id: subject_id.clone(),
                scope_id: scope_id.clone(),
                session_source_id: session_source_id.clone(),
                compacted_turn_id: compacted_turn_id.clone(),
                compacted_at_ns,
            },
        );
        ChatCompactMutationResult {
            accepted: true,
            updated: true,
            compacted_turn_id: Some(compacted_turn_id),
        }
    }

    /// Creates a new memory through a slot or version-replaces one existing record.
    /// 通过槽位创建新记忆，或按版本替换一条既有记录。
    fn put_memory(
        &mut self,
        entity: &EntityRef,
        mut record: MemoryRecord,
        slots: &mut BTreeMap<String, EntityBinding>,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        validate_memory_scores(&record)?;
        match entity {
            EntityRef::New(slot) => {
                if slots.contains_key(slot.as_str()) {
                    return Err(format!("duplicate entity slot: {slot}"));
                }
                let domain_id = self.allocate_domain_id(EntityKind::Memory)?;
                let memory_id =
                    MemoryId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                record.id = memory_id.clone();
                record.version = 1;
                let new_version = record.version;
                self.memories.insert(memory_id.clone(), record);
                slots.insert(
                    slot.as_str().to_owned(),
                    EntityBinding {
                        entity_slot_id: slot.clone(),
                        entity_kind: EntityKind::Memory,
                        domain_id,
                    },
                );
                advanced.push(VersionedEntity {
                    entity_kind: EntityKind::Memory,
                    domain_id: memory_id.as_str().to_owned(),
                    version: new_version,
                });
                Ok(())
            }
            EntityRef::Existing {
                domain_id,
                expected_version,
            } => {
                let memory_id =
                    MemoryId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                if record.id != memory_id {
                    return Err("memory identity conflicts with entity ref".to_owned());
                }
                let Some(existing) = self.memories.get(&memory_id) else {
                    // A direct caller-assigned write creates the record when no version is expected.
                    // 当没有期望版本时，调用方分配身份的直接写入会创建记录。
                    if *expected_version == 0 {
                        record.version = 1;
                        let new_version = record.version;
                        self.memories.insert(memory_id, record);
                        advanced.push(VersionedEntity {
                            entity_kind: EntityKind::Memory,
                            domain_id: domain_id.clone(),
                            version: new_version,
                        });
                        return Ok(());
                    }
                    return Err(format!("memory not found: {memory_id}"));
                };
                if !existing.has_same_access_identity(&record) {
                    return Err(format!("memory ownership conflict: {memory_id}"));
                }
                if *expected_version != existing.version {
                    return Err(format!("memory version conflict: {memory_id}"));
                }
                record.version = existing
                    .version
                    .checked_add(1)
                    .ok_or_else(|| format!("memory version is exhausted: {memory_id}"))?;
                let new_version = record.version;
                self.memories.insert(memory_id.clone(), record);
                advanced.push(VersionedEntity {
                    entity_kind: EntityKind::Memory,
                    domain_id: domain_id.clone(),
                    version: new_version,
                });
                Ok(())
            }
        }
    }

    /// Applies one adoption reinforcement fact under version guard.
    /// 在版本守护下应用一条采纳强化事实。
    fn reinforce_memory(
        &mut self,
        guard: MemoryMutationGuard<'_>,
        memory_id: &MemoryId,
        expected_version: u64,
        recalled_at_ns: i64,
        source_session_id: Option<&SessionSourceId>,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        let record = self
            .memories
            .get_mut(memory_id)
            .ok_or_else(|| format!("memory not found: {memory_id}"))?;
        if !memory_matches_access_identity(record, guard) {
            return Err(format!("memory access identity conflict: {memory_id}"));
        }
        if record.version != expected_version {
            return Err(format!("memory version conflict: {memory_id}"));
        }
        // The record's origin session also covers direct writes; a missing origin is not evidence of a different session.
        // 记录来源会话同时覆盖主动写入；缺失来源不能作为跨会话证据，不能依赖可能不存在的来源 Turn。
        let cross_session = record
            .origin_session_source_id
            .as_ref()
            .zip(source_session_id)
            .is_some_and(|(origin, current)| origin != current);
        record.reinforcement_count = record.reinforcement_count.saturating_add(1);
        record.adopted_count = record.adopted_count.saturating_add(1);
        record.refresh_weight = record.refresh_weight.max(1).saturating_add(1);
        if cross_session {
            record.cross_session_adopted_count =
                record.cross_session_adopted_count.saturating_add(1);
        }
        record.recalled_count = record.recalled_count.saturating_add(1);
        record.last_recalled_at_ns = Some(recalled_at_ns);
        record.last_adopted_at_ns = Some(recalled_at_ns);
        record.last_reinforced_at_ns = Some(recalled_at_ns);
        // Mirror the authoritative VMM adoption mutation: lifecycle timestamps and the generic
        // update timestamp advance together inside the same atomic write.
        // 镜像 VMM 权威采纳变更：生命周期时间戳与通用更新时间在同一原子写入中共同推进。
        record.updated_at_ns = recalled_at_ns;
        record.version = record
            .version
            .checked_add(1)
            .ok_or_else(|| format!("memory version is exhausted: {memory_id}"))?;
        advanced.push(VersionedEntity {
            entity_kind: EntityKind::Memory,
            domain_id: memory_id.as_str().to_owned(),
            version: record.version,
        });
        Ok(())
    }

    /// Retires one existing memory in favor of an accepted replacement reference.
    /// 退役一条既有记忆并启用已接受的替代引用。
    #[allow(clippy::too_many_arguments)]
    fn supersede_memory(
        &mut self,
        guard: MemoryMutationGuard<'_>,
        memory_id: &MemoryId,
        expected_version: u64,
        superseded_by: &EntityRef,
        status_reason: Option<String>,
        slots: &BTreeMap<String, EntityBinding>,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        let replacement_id = match superseded_by {
            EntityRef::New(slot) => {
                let binding = slots
                    .get(slot.as_str())
                    .ok_or_else(|| "supersede target slot is not bound in this batch".to_owned())?;
                if binding.entity_kind != EntityKind::Memory {
                    return Err("supersede target slot has the wrong entity kind".to_owned());
                }
                MemoryId::new(binding.domain_id.clone()).map_err(|error| error.to_string())?
            }
            EntityRef::Existing {
                domain_id,
                expected_version,
            } => {
                let replacement_id =
                    MemoryId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                let replacement = self
                    .memories
                    .get(&replacement_id)
                    .ok_or_else(|| format!("replacement memory not found: {replacement_id}"))?;
                if replacement.version != *expected_version
                    || replacement.state != MemoryState::Active
                {
                    return Err(format!(
                        "replacement memory state or version conflict: {replacement_id}"
                    ));
                }
                replacement_id
            }
        };
        if replacement_id == *memory_id {
            return Err("memory cannot supersede itself".to_owned());
        }
        let existing = self
            .memories
            .get(memory_id)
            .ok_or_else(|| format!("memory not found: {memory_id}"))?;
        let replacement = self
            .memories
            .get(&replacement_id)
            .ok_or_else(|| format!("replacement memory not found: {replacement_id}"))?;
        if !memory_matches_access_identity(existing, guard)
            || !replacement.is_visible_to_subject(guard.actor_subject_id)
        {
            return Err(format!(
                "replacement memory ownership conflict: {replacement_id}"
            ));
        }
        let record = self
            .memories
            .get_mut(memory_id)
            .ok_or_else(|| format!("memory not found after validation: {memory_id}"))?;
        if record.version != expected_version || record.state != MemoryState::Active {
            return Err(format!(
                "memory supersede state or version conflict: {memory_id}"
            ));
        }
        record.state = MemoryState::Superseded;
        record.status_reason = status_reason;
        record.version = record
            .version
            .checked_add(1)
            .ok_or_else(|| format!("memory version is exhausted: {memory_id}"))?;
        advanced.push(VersionedEntity {
            entity_kind: EntityKind::Memory,
            domain_id: memory_id.as_str().to_owned(),
            version: record.version,
        });
        Ok(())
    }

    /// Aggregates one context evidence observation onto a bound memory identity.
    /// 将一条情境证据观测聚合到一个已绑定记忆身份上。
    #[allow(clippy::too_many_arguments)]
    fn aggregate_context_evidence(
        &mut self,
        memory: &EntityRef,
        context_key: &str,
        context_value: &str,
        relation: ContextEvidenceRelation,
        occurred_at_ns: i64,
        slots: &BTreeMap<String, EntityBinding>,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        let domain_id = match memory {
            EntityRef::New(slot) => {
                let binding = slots
                    .get(slot.as_str())
                    .ok_or_else(|| "evidence target slot is not bound in this batch".to_owned())?;
                if binding.entity_kind != EntityKind::Memory {
                    return Err("evidence target slot has the wrong entity kind".to_owned());
                }
                binding.domain_id.clone()
            }
            EntityRef::Existing {
                domain_id,
                expected_version,
            } => {
                let memory_id =
                    MemoryId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                let record = self.memories.get(&memory_id).ok_or_else(|| {
                    format!("context evidence target memory missing: {memory_id}")
                })?;
                if record.version != *expected_version {
                    return Err(format!(
                        "context evidence target memory version conflict: {memory_id}"
                    ));
                }
                domain_id.clone()
            }
        };
        let memory_id = MemoryId::new(domain_id).map_err(|error| error.to_string())?;
        if !self.memories.contains_key(&memory_id) {
            return Err(format!(
                "context evidence target memory missing: {memory_id}"
            ));
        }
        let key = (
            memory_id.clone(),
            context_key.to_owned(),
            context_value.to_owned(),
        );
        let row = self
            .context_evidence
            .entry(key)
            .or_insert_with(|| MemoryContextEvidence {
                memory_id: memory_id.clone(),
                context_key: context_key.to_owned(),
                context_value: context_value.to_owned(),
                support_count: 0,
                rebuttal_count: 0,
                last_supported_at_ns: None,
                last_rebutted_at_ns: None,
                version: 0,
            });
        match relation {
            ContextEvidenceRelation::Support => {
                row.support_count = row.support_count.saturating_add(1);
                row.last_supported_at_ns = Some(occurred_at_ns);
            }
            ContextEvidenceRelation::Rebuttal => {
                row.rebuttal_count = row.rebuttal_count.saturating_add(1);
                row.last_rebutted_at_ns = Some(occurred_at_ns);
            }
        }
        row.version = row
            .version
            .checked_add(1)
            .ok_or_else(|| "context evidence version is exhausted".to_owned())?;
        advanced.push(VersionedEntity {
            entity_kind: EntityKind::ContextEvidence,
            domain_id: context_evidence_domain_id(&memory_id, context_key, context_value),
            version: row.version,
        });
        Ok(())
    }

    /// Deletes one exactly owned and version-matched memory.
    /// 删除一条所有权与版本精确匹配的记忆。
    fn delete_memory(
        &mut self,
        guard: MemoryMutationGuard<'_>,
        memory_id: &MemoryId,
        expected_version: Option<u64>,
    ) -> Result<(), String> {
        let existing = self
            .memories
            .get(memory_id)
            .ok_or_else(|| format!("memory not found: {memory_id}"))?;
        if !memory_matches_access_identity(existing, guard) {
            return Err(format!("memory access identity conflict: {memory_id}"));
        }
        if expected_version.is_some_and(|version| version != existing.version) {
            return Err(format!("memory version conflict: {memory_id}"));
        }
        self.memories.remove(memory_id);
        self.context_evidence.retain(|key, _| key.0 != *memory_id);
        Ok(())
    }

    /// Creates a new profile node through a slot or version-replaces one existing node.
    /// 通过槽位创建新画像节点，或按版本替换一个既有节点。
    fn put_profile_node(
        &mut self,
        entity: &EntityRef,
        mut node: ProfileNode,
        slots: &mut BTreeMap<String, EntityBinding>,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        validate_profile_node_snapshot(&node)?;
        match entity {
            EntityRef::New(slot) => {
                if slots.contains_key(slot.as_str()) {
                    return Err(format!("duplicate entity slot: {slot}"));
                }
                let domain_id = self.allocate_domain_id(EntityKind::ProfileNode)?;
                let node_id =
                    ProfileNodeId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                node.id = node_id.clone();
                node.version = 1;
                let new_version = node.version;
                self.profile_nodes.insert(node_id.clone(), node);
                slots.insert(
                    slot.as_str().to_owned(),
                    EntityBinding {
                        entity_slot_id: slot.clone(),
                        entity_kind: EntityKind::ProfileNode,
                        domain_id,
                    },
                );
                advanced.push(VersionedEntity {
                    entity_kind: EntityKind::ProfileNode,
                    domain_id: node_id.as_str().to_owned(),
                    version: new_version,
                });
                Ok(())
            }
            EntityRef::Existing {
                domain_id,
                expected_version,
            } => {
                let node_id =
                    ProfileNodeId::new(domain_id.clone()).map_err(|error| error.to_string())?;
                if node.id != node_id {
                    return Err("profile identity conflicts with entity ref".to_owned());
                }
                let existing = self
                    .profile_nodes
                    .get(&node_id)
                    .ok_or_else(|| format!("profile node not found: {node_id}"))?;
                if existing.subject_id != node.subject_id || existing.scope_id != node.scope_id {
                    return Err(format!("profile ownership conflict: {node_id}"));
                }
                if *expected_version != existing.version {
                    return Err(format!("profile version conflict: {node_id}"));
                }
                node.version = existing
                    .version
                    .checked_add(1)
                    .ok_or_else(|| format!("profile version is exhausted: {node_id}"))?;
                let new_version = node.version;
                self.profile_nodes.insert(node_id.clone(), node);
                advanced.push(VersionedEntity {
                    entity_kind: EntityKind::ProfileNode,
                    domain_id: domain_id.clone(),
                    version: new_version,
                });
                Ok(())
            }
        }
    }

    /// Retires one exactly owned profile node after optimistic-version validation.
    /// 在乐观版本校验后退役一个精确所属画像节点。
    #[allow(clippy::too_many_arguments)]
    fn retire_profile_node(
        &mut self,
        subject_id: Option<&SubjectId>,
        scope_id: &vulcan_memorymesh_core_contracts::ScopeId,
        profile_node_id: &ProfileNodeId,
        expected_version: u64,
        superseded_by: Option<EntityRef>,
        status_reason: Option<String>,
        slots: &BTreeMap<String, EntityBinding>,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        let current = self
            .profile_nodes
            .get(profile_node_id)
            .ok_or_else(|| format!("profile node not found: {profile_node_id}"))?;
        if current.subject_id.as_ref() != subject_id || &current.scope_id != scope_id {
            return Err(format!("profile ownership conflict: {profile_node_id}"));
        }
        if current.version != expected_version {
            return Err(format!("profile version conflict: {profile_node_id}"));
        }
        let replacement = superseded_by
            .map(|reference| {
                let (replacement_id, replacement_version) = match reference {
                    EntityRef::New(slot) => {
                        let binding = slots.get(slot.as_str()).ok_or_else(|| {
                            "profile supersede target slot is not bound in this batch".to_owned()
                        })?;
                        if binding.entity_kind != EntityKind::ProfileNode {
                            return Err(
                                "profile supersede target slot has the wrong entity kind".to_owned()
                            );
                        }
                        (
                            ProfileNodeId::new(binding.domain_id.clone())
                                .map_err(|error| error.to_string())?,
                            None,
                        )
                    }
                    EntityRef::Existing {
                        domain_id,
                        expected_version,
                    } => (
                        ProfileNodeId::new(domain_id).map_err(|error| error.to_string())?,
                        Some(expected_version),
                    ),
                };
                if replacement_id == *profile_node_id {
                    return Err("profile node cannot supersede itself".to_owned());
                }
                let replacement_node = self
                    .profile_nodes
                    .get(&replacement_id)
                    .ok_or_else(|| format!("profile replacement not found: {replacement_id}"))?;
                // Shared environment successors may have another creator. Core validates section
                // authorization; storage retains exact old-owner/version and same-scope checks.
                // 共享环境的后继节点可以由其他主体创建。Core 校验分区授权；存储仍校验旧节点
                // 精确归属与版本，并要求后继处于同一作用域。
                if replacement_node.scope_id != *scope_id
                    || replacement_node.state != ProfileNodeState::Active
                    || replacement_version
                        .is_some_and(|version| replacement_node.version != version)
                {
                    return Err(format!(
                        "profile replacement ownership, state, or version conflict: {replacement_id}"
                    ));
                }
                Ok(replacement_id)
            })
            .transpose()?;
        let node = self
            .profile_nodes
            .get_mut(profile_node_id)
            .ok_or_else(|| format!("profile node not found after validation: {profile_node_id}"))?;
        node.state = ProfileNodeState::Inactive;
        node.superseded_by = replacement;
        node.status_reason = status_reason;
        node.version = node
            .version
            .checked_add(1)
            .ok_or_else(|| format!("profile version is exhausted: {profile_node_id}"))?;
        advanced.push(VersionedEntity {
            entity_kind: EntityKind::ProfileNode,
            domain_id: profile_node_id.as_str().to_owned(),
            version: node.version,
        });
        Ok(())
    }

    /// Persists one analysis attempt before its first external effect, rejecting a reused conflicting identity.
    /// 在首个外部 Effect 前持久化一次分析尝试，并拒绝复用的冲突身份。
    fn put_analysis_attempt(&mut self, attempt: AnalysisAttemptIdentity) -> Result<(), String> {
        let key = (attempt.turn_id.clone(), attempt.analysis_generation);
        if let Some(existing) = self.analysis_attempts.get(&key) {
            if existing == &attempt {
                return Ok(());
            }
            return Err(format!(
                "analysis attempt identity conflict for generation {}",
                attempt.analysis_generation
            ));
        }
        self.analysis_attempts.insert(key, attempt);
        Ok(())
    }

    /// Atomically replaces one complete semantic noise prototype bundle.
    /// 原子替换一个完整的语义 Noise 原型束。
    fn put_noise_prototype_bundle(
        &mut self,
        identity: &NoisePrototypeCacheIdentity,
        prototypes: &[NoisePrototype],
    ) -> Result<(), String> {
        if prototypes.iter().any(|row| row.vector.is_empty()) {
            return Err("noise prototype vector is empty".to_owned());
        }
        self.noise_prototypes
            .insert(noise_cache_key(identity), prototypes.to_vec());
        Ok(())
    }

    /// Marks one exactly owned memory recycled after version validation.
    /// 在版本校验后将一条精确归属的记忆标记为已回收。
    fn recycle_memory(
        &mut self,
        guard: MemoryMutationGuard<'_>,
        memory_id: &MemoryId,
        expected_version: u64,
        status_reason: Option<String>,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        let record = self
            .memories
            .get_mut(memory_id)
            .ok_or_else(|| format!("memory not found: {memory_id}"))?;
        if !memory_matches_access_identity(record, guard) {
            return Err(format!("memory access identity conflict: {memory_id}"));
        }
        if record.version != expected_version {
            return Err(format!("memory version conflict: {memory_id}"));
        }
        record.state = MemoryState::Recycled;
        record.status_reason = status_reason;
        record.version = record
            .version
            .checked_add(1)
            .ok_or_else(|| format!("memory version is exhausted: {memory_id}"))?;
        advanced.push(VersionedEntity {
            entity_kind: EntityKind::Memory,
            domain_id: memory_id.as_str().to_owned(),
            version: record.version,
        });
        Ok(())
    }

    /// Restores one exactly owned recycled memory after version validation.
    /// 在版本校验后恢复一条精确所属的已回收记忆。
    fn restore_memory(
        &mut self,
        guard: MemoryMutationGuard<'_>,
        memory_id: &MemoryId,
        expected_version: u64,
        advanced: &mut Vec<VersionedEntity>,
    ) -> Result<(), String> {
        let record = self
            .memories
            .get_mut(memory_id)
            .ok_or_else(|| format!("memory not found: {memory_id}"))?;
        if !memory_matches_access_identity(record, guard) {
            return Err(format!("memory access identity conflict: {memory_id}"));
        }
        if record.version != expected_version || record.state != MemoryState::Recycled {
            return Err(format!(
                "memory restore state or version conflict: {memory_id}"
            ));
        }
        record.state = MemoryState::Active;
        record.status_reason = None;
        record.version = record
            .version
            .checked_add(1)
            .ok_or_else(|| format!("memory version is exhausted: {memory_id}"))?;
        advanced.push(VersionedEntity {
            entity_kind: EntityKind::Memory,
            domain_id: memory_id.as_str().to_owned(),
            version: record.version,
        });
        Ok(())
    }

    /// Physically purges one exactly owned recycled memory and dependent evidence.
    /// 物理清除一条精确所属的已回收记忆及其依赖证据。
    fn purge_memory(
        &mut self,
        guard: MemoryMutationGuard<'_>,
        memory_id: &MemoryId,
        expected_version: u64,
    ) -> Result<(), String> {
        let record = self
            .memories
            .get(memory_id)
            .ok_or_else(|| format!("memory not found: {memory_id}"))?;
        if !memory_matches_access_identity(record, guard) {
            return Err(format!("memory access identity conflict: {memory_id}"));
        }
        if record.version != expected_version || record.state != MemoryState::Recycled {
            return Err(format!(
                "memory purge state or version conflict: {memory_id}"
            ));
        }
        self.memories.remove(memory_id);
        self.context_evidence.retain(|key, _| key.0 != *memory_id);
        Ok(())
    }
}

impl InMemoryStorage {
    /// Applies a batch with the actual host commit time, returning its atomic outcome.
    /// 使用实际宿主提交时间应用批次，返回原子结果。
    /// `committed_at_ns` is Unix nanoseconds and is mandatory for extraction-window advancement.
    /// `committed_at_ns` 是 Unix 纳秒时间，提取窗口推进必须提供该值。
    pub fn mutate_at(
        &mut self,
        batch: &MutationBatch,
        committed_at_ns: i64,
    ) -> Result<MutationOutcome, CoreError> {
        self.mutate_with_commit_time(batch, Some(committed_at_ns))
    }

    /// Advances a completed turn's owner session using compare-and-set on the observed start.
    /// 对观察起点执行比较并交换，推进已完成回合的归属会话。
    /// The turn, prior boundary, inclusive cutoff and host clock must all agree; otherwise reject.
    /// 回合、前次边界、包含式截止及宿主时钟必须全部吻合，否则拒绝变更。
    fn advance_session_extraction(
        &mut self,
        turn_id: &TurnId,
        observed_after_ns: i64,
        observed_through_ns: i64,
        committed_at_ns: Option<i64>,
    ) -> Result<(), String> {
        // committed_at_ns is supplied by the host, not inferred from an old turn or analysis cutoff.
        // committed_at_ns 由宿主提供，不能从旧回合或分析截止时间推断。
        let committed_at_ns = committed_at_ns
            .ok_or_else(|| "session extraction requires a host commit timestamp".to_owned())?;
        // turn is the sole authority for subject/project/session ownership.
        // turn 是主体/项目/会话归属的唯一权威来源。
        let turn = self
            .turns
            .get(turn_id)
            .ok_or_else(|| "session extraction turn is missing".to_owned())?;
        if turn.status != TurnStatus::Completed
            || observed_after_ns < 0
            || observed_through_ns < observed_after_ns
            || observed_through_ns < turn.created_at_ns
            || committed_at_ns < observed_through_ns
        {
            return Err(
                "session extraction requires a completed turn and a valid observation window"
                    .to_owned(),
            );
        }
        // key isolates identically named sessions across writing projects and subjects.
        // key 隔离不同写入项目和主体下的同名会话。
        let key = (
            turn.subject_id.clone(),
            turn.scope_id.clone(),
            turn.session_source_id.clone(),
        );
        // session must have been registered with the turn; absent metadata is not a zero watermark.
        // session 必须与回合一同登记；元数据缺失不能视为零水位。
        let session = self
            .memory_sessions
            .get_mut(&key)
            .ok_or_else(|| "session extraction owner record is missing".to_owned())?;
        if session.last_extraction_observed_at_ns != observed_after_ns {
            return Err("session extraction observation boundary changed".to_owned());
        }
        session.last_extraction_observed_at_ns = observed_through_ns;
        session.last_extraction_completed_at_ns = Some(committed_at_ns);
        Ok(())
    }

    /// Applies existing transactional semantics with an explicit optional host clock.
    /// 使用明确的可选宿主时钟应用既有事务语义。
    /// Clock-free callers retain deterministic behavior; extraction mutations reject missing time.
    /// 无时钟调用方保持确定性行为；提取变更会拒绝缺失时间。
    fn mutate_with_commit_time(
        &mut self,
        batch: &MutationBatch,
        committed_at_ns: Option<i64>,
    ) -> Result<MutationOutcome, CoreError> {
        if batch.mutations.is_empty() {
            return Err(storage_error(
                ErrorCode::InvalidArgument,
                "mutation batch must not be empty",
            ));
        }
        batch.identity.validate()?;
        let contains_import_restore = batch.mutations.iter().any(|mutation| {
            matches!(
                mutation,
                Mutation::RestoreImportedMemory(_)
                    | Mutation::RestoreImportedProfileNode(_)
                    | Mutation::RestoreImportedContextEvidence(_)
            )
        });
        if contains_import_restore && batch.identity.source_kind != MutationSourceKind::Import {
            return Err(storage_error(
                ErrorCode::InvalidArgument,
                "exact import restore mutations require an import mutation identity",
            ));
        }
        if let Some(entry) = self.outcomes.get(&batch.identity.idempotency_key) {
            if entry.operation_id != batch.identity.operation_id
                || entry.request_fingerprint != batch.identity.request_fingerprint
            {
                return Err(storage_error(
                    ErrorCode::IdempotencyConflict,
                    "durable idempotency key was reused with a different operation or request fingerprint",
                ));
            }
            return Ok(entry.outcome.clone());
        }
        // Window advancement is inseparable from this batch's successful pending-turn transition.
        // 窗口推进必须与本批次待处理回合的成功状态迁移不可分离。
        for mutation in &batch.mutations {
            if let Mutation::AdvanceSessionExtraction { turn_id, .. } = mutation
                && !batch.mutations.iter().any(|mutation| matches!(mutation,
                    Mutation::TransitionTurnStatus { turn_id: completed_id, from: TurnStatus::Pending, to: TurnStatus::Completed, .. }
                    if completed_id == turn_id))
            {
                return Ok(MutationOutcome::Rejected { reason: "session extraction must share its completed-turn batch".to_owned() });
            }
        }
        let is_advance_boundary = matches!(
            &batch.mutations[..],
            [Mutation::AdvanceChatCompactBoundary { .. }]
        );
        let mut transaction = self.clone();
        let chat_compact = if is_advance_boundary {
            if let [
                Mutation::AdvanceChatCompactBoundary {
                    subject_id,
                    scope_id,
                    session_source_id,
                    compacted_at_ns,
                },
            ] = batch.mutations.as_slice()
            {
                let result = transaction.advance_chat_compact_boundary(
                    subject_id,
                    scope_id,
                    session_source_id,
                    *compacted_at_ns,
                );
                if result.updated {
                    transaction.storage_version =
                        transaction.storage_version.checked_add(1).ok_or_else(|| {
                            storage_error(
                                ErrorCode::StorageConflict,
                                "storage version is exhausted",
                            )
                        })?;
                }
                Some(result)
            } else {
                return Err(storage_error(
                    ErrorCode::InvalidArgument,
                    "advance compact boundary batch shape is invalid",
                ));
            }
        } else {
            if batch
                .mutations
                .iter()
                .any(|mutation| matches!(mutation, Mutation::AdvanceChatCompactBoundary { .. }))
            {
                return Err(storage_error(
                    ErrorCode::InvalidArgument,
                    "advance compact boundary must be the only mutation in its batch",
                ));
            }
            let mut slots = BTreeMap::new();
            let mut advanced = Vec::new();
            for mutation in &batch.mutations {
                if let Err(reason) =
                    transaction.apply_mutation(mutation, &mut slots, &mut advanced, committed_at_ns)
                {
                    return Ok(MutationOutcome::Rejected { reason });
                }
            }
            transaction.storage_version =
                transaction.storage_version.checked_add(1).ok_or_else(|| {
                    storage_error(ErrorCode::StorageConflict, "storage version is exhausted")
                })?;
            let outcome = MutationOutcome::Committed {
                storage_version: transaction.storage_version,
                chat_compact: None,
                receipt: CommitReceipt {
                    operation_id: batch.identity.operation_id.clone(),
                    idempotency_key: batch.identity.idempotency_key.clone(),
                    request_fingerprint: batch.identity.request_fingerprint.clone(),
                    entity_bindings: slots.into_values().collect(),
                    committed_versions: final_committed_versions(advanced),
                    side_effect_summary: summarize_side_effects(&batch.mutations),
                },
            };
            transaction.outcomes.insert(
                batch.identity.idempotency_key.clone(),
                LedgerEntry {
                    operation_id: batch.identity.operation_id.clone(),
                    request_fingerprint: batch.identity.request_fingerprint.clone(),
                    outcome: outcome.clone(),
                },
            );
            *self = transaction;
            return Ok(outcome);
        };
        let outcome = MutationOutcome::Committed {
            storage_version: transaction.storage_version,
            chat_compact,
            receipt: CommitReceipt {
                operation_id: batch.identity.operation_id.clone(),
                idempotency_key: batch.identity.idempotency_key.clone(),
                request_fingerprint: batch.identity.request_fingerprint.clone(),
                entity_bindings: Vec::new(),
                committed_versions: Vec::new(),
                side_effect_summary: summarize_side_effects(&batch.mutations),
            },
        };
        transaction.outcomes.insert(
            batch.identity.idempotency_key.clone(),
            LedgerEntry {
                operation_id: batch.identity.operation_id.clone(),
                request_fingerprint: batch.identity.request_fingerprint.clone(),
                outcome: outcome.clone(),
            },
        );
        *self = transaction;
        Ok(outcome)
    }
}

impl StoragePort for InMemoryStorage {
    /// Executes one side-effect-free semantic query.
    /// 执行一次无副作用的语义查询。
    #[allow(clippy::too_many_lines)]
    fn query(&self, query: &StorageQuery) -> Result<StorageQueryResult, CoreError> {
        match query {
            StorageQuery::CandidateObservation(request) => {
                validate_scope(&request.scope_access)?;
                if let Some(scope) = &request.boundary.candidate_scope {
                    scope
                        .validate(&request.scope_access)
                        .map_err(|message| storage_error(ErrorCode::ScopeViolation, message))?;
                }
                // The immutable storage borrow covers all channels and records; no mutation can interleave.
                // 不可变存储借用覆盖全部通道和记录，期间不允许插入变更。
                let context = MemorySearchContext {
                    evaluated_at_ns: Some(request.now_ns),
                    subject_id: &request.subject_id,
                    scope_access: &request.scope_access,
                    vector_space_id: &request.vector_space_id,
                    candidate_scope: request.boundary.candidate_scope.as_ref(),
                    session_recall_boundary: request.boundary.session_recall_boundary.as_ref(),
                };
                let vector = self.vector_candidates(
                    context,
                    std::slice::from_ref(&request.query_embedding),
                    request.limit,
                    &request.vector_contract,
                )?;
                let lexical = request
                    .lexical_contract
                    .as_ref()
                    .map(|contract| {
                        self.lexical_candidates(
                            context,
                            std::slice::from_ref(&request.query),
                            request.limit,
                            contract,
                        )
                        .map(|candidates| {
                            vulcan_memorymesh_core_contracts::PhysicalCandidateChannel {
                                candidates,
                            }
                        })
                    })
                    .transpose()?;
                let identities = vector
                    .iter()
                    .chain(lexical.iter().flat_map(|channel| &channel.candidates))
                    .map(|row| {
                        MemoryId::new(row.memory_id.clone()).map_err(|error| {
                            storage_error(ErrorCode::InvalidIdentifier, error.to_string())
                        })
                    })
                    .collect::<Result<BTreeSet<_>, _>>()?;
                let memories = identities
                    .iter()
                    .map(|id| {
                        self.memories.get(id).cloned().ok_or_else(|| {
                            storage_error(
                                ErrorCode::StorageConflict,
                                "observation record disappeared",
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(StorageQueryResult::CandidateObservation(
                    vulcan_memorymesh_core_contracts::CandidateObservation {
                        vector_channel:
                            vulcan_memorymesh_core_contracts::PhysicalCandidateChannel {
                                candidates: vector,
                            },
                        lexical_channel: lexical,
                        memories,
                    },
                ))
            }
            StorageQuery::VectorCandidates {
                subject_id,
                scope_access,
                query_embeddings,
                vector_space_id,
                boundary,
                limit,
                expected_contract,
                now_ns,
                ..
            } => {
                validate_scope(scope_access)?;
                if let Some(scope) = &boundary.candidate_scope {
                    scope
                        .validate(scope_access)
                        .map_err(|message| storage_error(ErrorCode::ScopeViolation, message))?;
                }
                let candidates = self.vector_candidates(
                    MemorySearchContext {
                        evaluated_at_ns: Some(*now_ns),
                        candidate_scope: boundary.candidate_scope.as_ref(),
                        subject_id,
                        scope_access,
                        vector_space_id,
                        session_recall_boundary: boundary.session_recall_boundary.as_ref(),
                    },
                    query_embeddings,
                    *limit,
                    expected_contract,
                )?;
                let memories = self.candidate_channel_memories(&candidates)?;
                Ok(StorageQueryResult::VectorCandidates(
                    vulcan_memorymesh_core_contracts::CandidateChannelResult {
                        candidates,
                        memories,
                    },
                ))
            }
            StorageQuery::LexicalCandidates {
                subject_id,
                scope_access,
                queries,
                vector_space_id,
                boundary,
                limit,
                expected_contract,
                now_ns,
                ..
            } => {
                validate_scope(scope_access)?;
                if let Some(scope) = &boundary.candidate_scope {
                    scope
                        .validate(scope_access)
                        .map_err(|message| storage_error(ErrorCode::ScopeViolation, message))?;
                }
                let candidates = self.lexical_candidates(
                    MemorySearchContext {
                        evaluated_at_ns: Some(*now_ns),
                        candidate_scope: boundary.candidate_scope.as_ref(),
                        subject_id,
                        scope_access,
                        vector_space_id,
                        session_recall_boundary: boundary.session_recall_boundary.as_ref(),
                    },
                    queries,
                    *limit,
                    expected_contract,
                )?;
                let memories = self.candidate_channel_memories(&candidates)?;
                Ok(StorageQueryResult::LexicalCandidates(
                    vulcan_memorymesh_core_contracts::CandidateChannelResult {
                        candidates,
                        memories,
                    },
                ))
            }
            StorageQuery::MemoryRecordBatch {
                subject_id,
                scope_access,
                memory_ids,
            } => {
                validate_scope(scope_access)?;
                let records = memory_ids
                    .iter()
                    .map(|memory_id| {
                        self.memories
                            .get(memory_id)
                            .filter(|memory| {
                                memory.is_visible_to_subject(subject_id)
                                    && scope_access.visible_scopes.contains(&memory.scope_id)
                            })
                            .cloned()
                    })
                    .collect();
                Ok(StorageQueryResult::MemoryRecordBatch(records))
            }
            StorageQuery::RecentDirectWriteDedupe {
                subject_id,
                scope_access,
                session_source_id,
                lookups,
                created_not_before_ns,
                evaluated_at_ns,
            } => {
                validate_scope(scope_access)?;
                let records = lookups
                    .iter()
                    .map(|lookup| {
                        if !scope_access
                            .visible_scopes
                            .contains(&lookup.target_scope_id)
                        {
                            return None;
                        }
                        self.memories
                            .values()
                            .filter(|memory| {
                                memory.access_class
                                    == vulcan_memorymesh_core_contracts::MemoryAccessClass::SubjectPrivate
                                    && memory.source_subject_id.as_ref() == Some(subject_id)
                                    && memory.scope_id == lookup.target_scope_id
                                    && memory.source_kind == MemorySourceKind::Direct
                                    && memory.origin_session_source_id.as_ref()
                                        == Some(session_source_id)
                                    && memory.dedupe_hash.as_deref()
                                        == Some(lookup.dedupe_hash.as_str())
                                    && memory.state == MemoryState::Active
                                    && memory.created_at_ns >= *created_not_before_ns
                                    && memory.created_at_ns <= *evaluated_at_ns
                                    && memory.expires_at_ns.is_none_or(|expires_at_ns| {
                                        expires_at_ns > *evaluated_at_ns
                                    })
                            })
                            .max_by(|left, right| {
                                left.created_at_ns
                                    .cmp(&right.created_at_ns)
                                    .then_with(|| left.id.as_str().cmp(right.id.as_str()))
                            })
                            .cloned()
                    })
                    .collect();
                Ok(StorageQueryResult::RecentDirectWriteDedupe(records))
            }
            StorageQuery::ContextEvidenceBatch {
                subject_id,
                scope_access,
                memory_ids,
            } => {
                validate_scope(scope_access)?;
                let groups = memory_ids
                    .iter()
                    .map(|memory_id| ContextEvidenceGroup {
                        memory_id: memory_id.clone(),
                        evidence: self
                            .context_evidence
                            .values()
                            .filter(|row| {
                                row.memory_id == *memory_id
                                    && self.memories.get(memory_id).is_some_and(|memory| {
                                        memory.is_visible_to_subject(subject_id)
                                            && scope_access
                                                .visible_scopes
                                                .contains(&memory.scope_id)
                                    })
                            })
                            .cloned()
                            .collect(),
                    })
                    .collect();
                Ok(StorageQueryResult::ContextEvidenceBatch(groups))
            }
            StorageQuery::NoisePrototypeBundle { identity } => {
                Ok(StorageQueryResult::NoisePrototypes {
                    identity: identity.clone(),
                    prototypes: self
                        .noise_prototypes
                        .get(&noise_cache_key(identity))
                        .cloned()
                        .unwrap_or_default(),
                })
            }
            StorageQuery::RetentionScan {
                subject_id,
                scope_access,
                now_ns,
                policy,
            } => {
                validate_scope(scope_access)?;
                // Profile expiry spans the write project and the fixed subject-profile scope.
                // 画像过期扫描覆盖写入项目与固定主体画像作用域。
                let mut expired_profile_nodes = self
                    .profile_nodes
                    .values()
                    .filter(|node| {
                        node.state == ProfileNodeState::Active
                            && node
                                .expires_at_ns
                                .is_some_and(|expires_at_ns| expires_at_ns <= *now_ns)
                            && scope_access.visible_scopes.contains(&node.scope_id)
                            && (node.scope_id == scope_access.write_scope
                                || node.subject_id.as_ref() == Some(subject_id))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                expired_profile_nodes.sort_by(|left, right| {
                    left.expires_at_ns
                        .cmp(&right.expires_at_ns)
                        .then_with(|| left.id.cmp(&right.id))
                });
                expired_profile_nodes.truncate(policy.profile_expiry_limit as usize);
                // Terminal memory facts are the only durable-memory rows eligible for the
                // first VMM retention phase; active rows must never be recycled by age alone.
                // 终态记忆事实是 VMM 第一阶段唯一可回收的持久记忆行；活跃行绝不能仅因年龄被回收。
                let mut terminal_memories = self
                    .memories
                    .values()
                    .filter(|memory| {
                        memory.is_visible_to_subject(subject_id)
                            && memory.scope_id == scope_access.write_scope
                            && matches!(
                                memory.state,
                                MemoryState::Superseded
                                    | MemoryState::Deleted
                                    | MemoryState::Expired
                            )
                    })
                    .cloned()
                    .map(|memory| RetentionMemoryFact {
                        protected_shared: memory.scope_class
                            != vulcan_memorymesh_core_contracts::MemoryScopeClass::Session,
                        memory,
                    })
                    .collect::<Vec<_>>();
                terminal_memories.sort_by(|left, right| {
                    left.memory
                        .updated_at_ns
                        .cmp(&right.memory.updated_at_ns)
                        .then_with(|| left.memory.id.cmp(&right.memory.id))
                });
                terminal_memories.truncate(policy.memory_recycle_limit as usize);

                // Group the subject's write-scope timeline once so cold-turn and idle-session
                // facts share the same deterministic physical snapshot.
                // 对主体写入作用域时间线只分组一次，使冷 Turn 与闲置会话事实共享同一确定性物理快照。
                let mut session_turns = BTreeMap::<SessionSourceId, Vec<&Turn>>::new();
                // session_latest_activity_ns mirrors the external session updated timestamp by
                // combining turn creation, analysis observation, and compaction activity.
                // session_latest_activity_ns 通过组合回合创建、分析观察和压缩活动，
                // 对齐外置 VMM 的 session 更新时间语义。
                let mut session_latest_activity_ns = BTreeMap::<SessionSourceId, i64>::new();
                // Successful extraction activity survives recycling of its old turn body.
                // 成功提取活动在其旧回合正文被回收之后仍然保留。
                for session in self.memory_sessions.values().filter(|session| {
                    session.subject_id == *subject_id
                        && session.scope_id == scope_access.write_scope
                }) {
                    if let Some(completed_at_ns) = session.last_extraction_completed_at_ns
                        && completed_at_ns <= *now_ns
                    {
                        session_turns
                            .entry(session.session_source_id.clone())
                            .or_default();
                        session_latest_activity_ns
                            .insert(session.session_source_id.clone(), completed_at_ns);
                    }
                }
                // turn_sessions resolves durable analysis attempts back to their owning session.
                // turn_sessions 把耐久分析尝试解析回其所属会话。
                let mut turn_sessions = BTreeMap::<TurnId, SessionSourceId>::new();
                for turn in self.turns.values() {
                    if turn.subject_id == *subject_id
                        && turn.scope_id == scope_access.write_scope
                        && turn.created_at_ns <= *now_ns
                    {
                        session_turns
                            .entry(turn.session_source_id.clone())
                            .or_default()
                            .push(turn);
                        session_latest_activity_ns
                            .entry(turn.session_source_id.clone())
                            .and_modify(|latest| *latest = (*latest).max(turn.created_at_ns))
                            .or_insert(turn.created_at_ns);
                        turn_sessions.insert(turn.id.clone(), turn.session_source_id.clone());
                    }
                }
                // Direct writes can establish a durable session without creating a canonical Turn.
                // Seed those sessions from their visible write-scope memory so idle cleanup retains
                // the external VMM session-table behavior instead of silently skipping them forever.
                // 主动写入可以在不创建规范 Turn 的情况下建立耐久会话。这里从可见写入作用域
                // 记忆补齐这些会话，使闲置清洗保持外置 VMM 的会话表语义，而不会永久漏扫。
                for memory in self.memories.values().filter(|memory| {
                    memory.is_visible_to_subject(subject_id)
                        && memory.scope_id == scope_access.write_scope
                        && memory.created_at_ns <= *now_ns
                }) {
                    let Some(session_source_id) = memory.origin_session_source_id.as_ref() else {
                        continue;
                    };
                    session_turns.entry(session_source_id.clone()).or_default();
                    session_latest_activity_ns
                        .entry(session_source_id.clone())
                        .and_modify(|latest| *latest = (*latest).max(memory.created_at_ns))
                        .or_insert(memory.created_at_ns);
                }
                for boundary in self.chat_compact_boundaries.values().filter(|boundary| {
                    boundary.subject_id == *subject_id
                        && boundary.scope_id == scope_access.write_scope
                        && boundary.compacted_at_ns <= *now_ns
                }) {
                    // A retained compact boundary is also an authoritative session anchor even when
                    // every former hot Turn has already moved to the recycle bin.
                    // 即使此前所有热 Turn 已进入回收站，保留的压缩边界仍是权威会话锚点。
                    session_turns
                        .entry(boundary.session_source_id.clone())
                        .or_default();
                    session_latest_activity_ns
                        .entry(boundary.session_source_id.clone())
                        .and_modify(|latest| *latest = (*latest).max(boundary.compacted_at_ns))
                        .or_insert(boundary.compacted_at_ns);
                }
                for attempt in self
                    .analysis_attempts
                    .values()
                    .filter(|attempt| attempt.mutation_identity.observed_at_ns <= *now_ns)
                {
                    let Some(session_source_id) = turn_sessions.get(&attempt.turn_id) else {
                        continue;
                    };
                    if let Some(latest) = session_latest_activity_ns.get_mut(session_source_id) {
                        *latest = (*latest).max(attempt.mutation_identity.observed_at_ns);
                    }
                }
                let mut referenced_turns = self
                    .memories
                    .values()
                    .filter_map(|memory| memory.source_turn_id.clone())
                    .collect::<BTreeSet<_>>();
                for profile_node in self.profile_nodes.values() {
                    referenced_turns.extend(profile_node.source_turn_ids.iter().cloned());
                }
                let queued_cold_sessions = self
                    .retention_cold_turn_jobs
                    .values()
                    .filter(|job| {
                        job.subject_id == *subject_id && job.scope_id == scope_access.write_scope
                    })
                    .map(|job| job.session_source_id.clone())
                    .collect::<BTreeSet<_>>();
                let idle_duration_ns =
                    i64::try_from(u128::from(policy.session_idle_ms).saturating_mul(1_000_000))
                        .map_err(|error| {
                            storage_error(
                                ErrorCode::InvalidArgument,
                                format!("retention session idle duration overflow: {error}"),
                            )
                        })?;
                let idle_cutoff_ns = now_ns.saturating_sub(idle_duration_ns);
                // Carry the latest activity beside each cold candidate until physical pagination
                // is complete so the oldest sessions receive maintenance first, matching VMM.
                // 在物理分页完成前为每个冷候选携带最近活动时间，使最久未活动会话优先维护并与 VMM 对齐。
                let mut cold_turn_candidates = Vec::new();
                let mut idle_sessions = Vec::new();
                for (session_source_id, turns) in &mut session_turns {
                    turns.sort_by(|left, right| {
                        left.created_at_ns
                            .cmp(&right.created_at_ns)
                            .then_with(|| left.id.cmp(&right.id))
                    });
                    let hot_start = turns
                        .len()
                        .saturating_sub(policy.turn_hot_window_size as usize);
                    let recyclable_turn_count = u32::try_from(
                        turns[..hot_start]
                            .iter()
                            .filter(|turn| {
                                turn.status != TurnStatus::Pending
                                    && !referenced_turns.contains(&turn.id)
                            })
                            .count(),
                    )
                    .map_err(|error| {
                        storage_error(
                            ErrorCode::Internal,
                            format!("cold-turn candidate count overflow: {error}"),
                        )
                    })?;
                    if recyclable_turn_count > 0
                        && !queued_cold_sessions.contains(session_source_id)
                    {
                        let last_activity_ns = session_latest_activity_ns
                            .get(session_source_id)
                            .copied()
                            .unwrap_or_default();
                        cold_turn_candidates.push((
                            last_activity_ns,
                            RetentionColdTurnCandidateFact {
                                session_source_id: session_source_id.clone(),
                                recyclable_turn_count,
                            },
                        ));
                    }
                    let has_idle_memory = self.memories.values().any(|memory| {
                        memory.is_visible_to_subject(subject_id)
                            && memory.scope_id == scope_access.write_scope
                            && memory.scope_class
                                == vulcan_memorymesh_core_contracts::MemoryScopeClass::Session
                            && memory.state == MemoryState::Active
                            && memory.origin_session_source_id.as_ref() == Some(session_source_id)
                            && memory
                                .expires_at_ns
                                .is_some_and(|expires_at| expires_at <= idle_cutoff_ns)
                            && [
                                Some(memory.created_at_ns),
                                Some(memory.updated_at_ns),
                                memory.last_recalled_at_ns,
                                memory.last_adopted_at_ns,
                                memory.last_reinforced_at_ns,
                            ]
                            .into_iter()
                            .flatten()
                            .max()
                            .is_some_and(|latest| latest <= idle_cutoff_ns)
                    });
                    if recyclable_turn_count > 0 || has_idle_memory {
                        idle_sessions.push(RetentionIdleSessionFact {
                            session_source_id: session_source_id.clone(),
                            last_activity_ns: session_latest_activity_ns
                                .get(session_source_id)
                                .copied()
                                .unwrap_or_default(),
                            has_pending_turn: turns
                                .iter()
                                .any(|turn| turn.status == TurnStatus::Pending),
                        });
                    }
                }
                cold_turn_candidates.sort_by(|left, right| {
                    left.0
                        .cmp(&right.0)
                        .then_with(|| left.1.session_source_id.cmp(&right.1.session_source_id))
                });
                cold_turn_candidates.truncate(policy.cold_turn_scan_limit as usize);
                let cold_turn_candidates = cold_turn_candidates
                    .into_iter()
                    .map(|(_, candidate)| candidate)
                    .collect();
                idle_sessions.sort_by(|left, right| {
                    left.last_activity_ns
                        .cmp(&right.last_activity_ns)
                        .then_with(|| left.session_source_id.cmp(&right.session_source_id))
                });
                idle_sessions.truncate(policy.idle_session_limit as usize);

                let mut cold_turn_jobs = self
                    .retention_cold_turn_jobs
                    .values()
                    .filter(|job| {
                        job.subject_id == *subject_id && job.scope_id == scope_access.write_scope
                    })
                    .map(|job| RetentionColdTurnJobFact {
                        job_id: job.job_id.clone(),
                        session_source_id: job.session_source_id.clone(),
                        next_run_at_ns: job.next_run_at_ns,
                    })
                    .collect::<Vec<_>>();
                cold_turn_jobs.sort_by(|left, right| {
                    left.next_run_at_ns
                        .cmp(&right.next_run_at_ns)
                        .then_with(|| left.job_id.cmp(&right.job_id))
                });
                cold_turn_jobs.truncate(policy.cold_turn_job_limit as usize);

                let mut trash_batches = self
                    .retention_trash_batches
                    .values()
                    .filter(|batch| {
                        batch.subject_id == *subject_id
                            && batch.scope_id == scope_access.write_scope
                            && batch.expires_at_ns <= *now_ns
                    })
                    .map(|batch| RetentionTrashBatchFact {
                        batch_id: batch.batch_id.clone(),
                        recycled_at_ns: batch.recycled_at_ns,
                        expires_at_ns: batch.expires_at_ns,
                    })
                    .collect::<Vec<_>>();
                trash_batches.sort_by(|left, right| {
                    left.expires_at_ns
                        .cmp(&right.expires_at_ns)
                        .then_with(|| left.batch_id.cmp(&right.batch_id))
                });
                trash_batches.truncate(policy.trash_purge_limit as usize);

                Ok(StorageQueryResult::RetentionScan(RetentionScanFacts {
                    subject_id: subject_id.clone(),
                    scope_id: scope_access.write_scope.clone(),
                    expired_profile_nodes,
                    terminal_memories,
                    cold_turn_candidates,
                    cold_turn_jobs,
                    idle_sessions,
                    vector_gc_jobs: Vec::new(),
                    trash_batches,
                }))
            }
            StorageQuery::MemoryDetails {
                subject_id,
                scope_access,
                memory_id,
            } => {
                validate_scope(scope_access)?;
                let record = self
                    .memories
                    .get(memory_id)
                    .filter(|memory| {
                        memory.is_visible_to_subject(subject_id)
                            && scope_access.visible_scopes.contains(&memory.scope_id)
                    })
                    .cloned();
                Ok(StorageQueryResult::MemoryDetails(Box::new(record)))
            }
            StorageQuery::DetailsBatch {
                subject_id,
                scope_access,
                memory_ids,
                turn_ids,
            } => {
                validate_scope(scope_access)?;
                let memories = memory_ids
                    .iter()
                    .map(|memory_id| {
                        self.memories
                            .get(memory_id)
                            .filter(|memory| {
                                memory.is_visible_to_subject(subject_id)
                                    && scope_access.visible_scopes.contains(&memory.scope_id)
                            })
                            .cloned()
                    })
                    .collect();
                let turns = turn_ids
                    .iter()
                    .map(|turn_id| {
                        self.turns
                            .get(turn_id)
                            .filter(|turn| {
                                turn.subject_id == *subject_id
                                    && scope_access.visible_scopes.contains(&turn.scope_id)
                            })
                            .cloned()
                    })
                    .collect();
                Ok(StorageQueryResult::DetailsBatch { memories, turns })
            }
            StorageQuery::PostActionContext {
                subject_id,
                scope_access,
                session_source_id,
                target_turn_id,
                reference_turn_limit,
                direct_write_observed_after_ns,
                analysis_cutoff_ns,
            } => {
                validate_scope(scope_access)?;
                let target = self
                    .turns
                    .get(target_turn_id)
                    .filter(|turn| {
                        turn.subject_id == *subject_id
                            && turn.scope_id == scope_access.write_scope
                            && turn.session_source_id == *session_source_id
                            && turn.status == TurnStatus::Pending
                            && turn.created_at_ns <= *analysis_cutoff_ns
                    })
                    .cloned()
                    .ok_or_else(|| {
                        storage_error(
                            ErrorCode::InvalidArgument,
                            "PostAction pending target turn is missing or outside query bounds",
                        )
                    })?;
                let mut reference_turns = self
                    .turns
                    .values()
                    .filter(|turn| {
                        turn.id != target.id
                            && turn.subject_id == *subject_id
                            && turn.scope_id == scope_access.write_scope
                            && turn.session_source_id == *session_source_id
                            && turn_can_supply_postaction_reference(turn)
                            && (turn.created_at_ns, &turn.id) < (target.created_at_ns, &target.id)
                    })
                    .filter_map(|turn| {
                        turn.analysis_details
                            .as_ref()
                            .filter(|details| !details.trim().is_empty())
                            .map(|details| PostActionReferenceTurn {
                                turn_id: turn.id.clone(),
                                subject_id: turn.subject_id.clone(),
                                scope_id: turn.scope_id.clone(),
                                session_source_id: turn.session_source_id.clone(),
                                details: details.clone(),
                                created_at_ns: turn.created_at_ns,
                            })
                    })
                    .collect::<Vec<_>>();
                reference_turns.sort_by(|left, right| {
                    left.created_at_ns
                        .cmp(&right.created_at_ns)
                        .then_with(|| left.turn_id.cmp(&right.turn_id))
                });
                let keep = usize::try_from(*reference_turn_limit).unwrap_or(usize::MAX);
                if reference_turns.len() > keep {
                    reference_turns.drain(..reference_turns.len() - keep);
                }
                let mut recent_direct_writes = self
                    .memories
                    .values()
                    .filter(|memory| {
                        memory.access_class
                            == vulcan_memorymesh_core_contracts::MemoryAccessClass::SubjectPrivate
                            && memory.source_subject_id.as_ref() == Some(subject_id)
                            && scope_access.visible_scopes.contains(&memory.scope_id)
                            && memory.source_kind
                                == vulcan_memorymesh_core_contracts::MemorySourceKind::Direct
                            && memory.origin_session_source_id.as_ref() == Some(session_source_id)
                            && memory.state == MemoryState::Active
                            && memory.created_at_ns > *direct_write_observed_after_ns
                            && memory.created_at_ns <= *analysis_cutoff_ns
                            && memory
                                .expires_at_ns
                                .is_none_or(|expiry| expiry > *analysis_cutoff_ns)
                    })
                    .map(|memory| PostActionDirectWrite {
                        memory_id: memory.id.clone(),
                        scope_id: memory.scope_id.clone(),
                        scope_class: memory.scope_class,
                        session_source_id: session_source_id.clone(),
                        summary: memory.summary.clone(),
                        content: memory.content.clone(),
                        created_at_ns: memory.created_at_ns,
                    })
                    .collect::<Vec<_>>();
                recent_direct_writes.sort_by(|left, right| {
                    left.created_at_ns
                        .cmp(&right.created_at_ns)
                        .then_with(|| left.memory_id.cmp(&right.memory_id))
                });
                Ok(StorageQueryResult::PostActionContext {
                    target_turn: Box::new(target),
                    reference_turns,
                    recent_direct_writes,
                })
            }
            StorageQuery::ProfileNodes {
                subject_id,
                scope_access,
                sections,
                evaluated_at_ns,
            } => self
                .profile_nodes(subject_id, scope_access, sections, *evaluated_at_ns)
                .map(StorageQueryResult::ProfileNodes),
        }
    }

    /// Applies one mutation batch atomically and idempotently under its persistent identity.
    /// 在其持久身份下原子且幂等地应用一个变更批次。
    #[allow(clippy::too_many_lines)]
    fn mutate(&mut self, batch: &MutationBatch) -> Result<MutationOutcome, CoreError> {
        self.mutate_with_commit_time(batch, None)
    }

    /// Reconciles a previous mutation by durable idempotency key.
    /// 通过持久幂等键对账先前变更。
    fn reconcile(&self, idempotency_key: &IdempotencyKey) -> Option<MutationOutcome> {
        self.outcomes
            .get(idempotency_key)
            .map(|entry| entry.outcome.clone())
    }
}

/// Assigns stable zero-based physical ranks to an ordered candidate channel.
/// 为一个有序候选通道分配稳定的从零开始物理排名。
fn assign_ranks(candidates: &mut [PhysicalCandidate]) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        candidate.physical_rank = u32::try_from(index).unwrap_or(u32::MAX);
    }
}

/// Reports whether a turn may supply details-only context to `PostAction` analysis.
/// 判断一个`Turn`是否可以向`PostAction`分析提供仅详情型上下文。
///
/// `Completed` turns carry full successful-analysis metadata. Legacy processed-unknown turns keep
/// their historical effect only when they retain non-empty details; this does not grant them any
/// other successful-analysis capability.
/// `Completed Turn`携带完整成功分析元数据。历史`ProcessedUnknown`仅在保留非空详情时维持原有参考作用，
/// 且不会因此获得其他成功分析资格。
fn turn_can_supply_postaction_reference(turn: &Turn) -> bool {
    matches!(
        turn.status,
        TurnStatus::Completed | TurnStatus::ProcessedUnknown
    ) && turn
        .analysis_details
        .as_ref()
        .is_some_and(|details| !details.trim().is_empty())
}

/// Rejects metadata combinations that contradict the exact durable turn state before storage.
/// 在数据进入长期存储前拒绝与精确Turn状态矛盾的元数据组合。
fn validate_turn_analysis_state(turn: &Turn) -> Result<(), String> {
    let invalid_state = match turn.status {
        TurnStatus::Pending | TurnStatus::AnalysisBypassed | TurnStatus::Corrupted => {
            turn.user_input_kind.is_some() || turn.analysis_details.is_some()
        }
        TurnStatus::Completed => turn.user_input_kind.is_none() || turn.analysis_details.is_none(),
        TurnStatus::ProcessedUnknown => turn.user_input_kind.is_some(),
    };
    if invalid_state
        || turn.analysis_details.as_ref().is_some_and(|details| {
            details.trim().is_empty()
                || details.len() > vulcan_memorymesh_core_contracts::MAX_DOMAIN_TEXT_BYTES
                || details.contains('\0')
        })
    {
        return Err(format!(
            "turn analysis metadata conflicts with its durable state: {}",
            turn.id
        ));
    }
    Ok(())
}

/// Builds one minimal turn used to verify terminal-state metadata combinations.
/// 构建一条用于验证终态元数据组合的最小轮次。
#[cfg(test)]
fn turn_state_test_fixture(status: TurnStatus) -> Turn {
    Turn {
        id: TurnId::new("turn-state-test").expect("valid turn id"),
        subject_id: SubjectId::new("subject-state-test").expect("valid subject id"),
        scope_id: vulcan_memorymesh_core_contracts::ScopeId::new("scope-state-test")
            .expect("valid scope id"),
        session_source_id: SessionSourceId::new("session-state-test")
            .expect("valid session source id"),
        timeline: Vec::new(),
        previous_turn_ids: Vec::new(),
        next_turn_ids: Vec::new(),
        created_at_ns: 1,
        status,
        user_input_kind: None,
        analysis_details: None,
    }
}

/// Verifies every durable turn state accepts only its authoritative metadata presence combination.
/// 验证每个长期轮次状态只接受其权威元数据存在性组合。
#[cfg(test)]
#[test]
fn turn_state_metadata_rules_preserve_history_without_claiming_success() {
    let mut completed = turn_state_test_fixture(TurnStatus::Completed);
    completed.user_input_kind = Some(vulcan_memorymesh_core_contracts::TurnInputKind::Statement);
    completed.analysis_details = Some("completed details".to_owned());
    assert!(validate_turn_analysis_state(&completed).is_ok());

    let mut unknown = turn_state_test_fixture(TurnStatus::ProcessedUnknown);
    unknown.analysis_details = Some("historical details with unknown input kind".to_owned());
    assert!(validate_turn_analysis_state(&unknown).is_ok());
    unknown.user_input_kind = Some(vulcan_memorymesh_core_contracts::TurnInputKind::Statement);
    assert!(validate_turn_analysis_state(&unknown).is_err());

    for status in [
        TurnStatus::Pending,
        TurnStatus::AnalysisBypassed,
        TurnStatus::Corrupted,
    ] {
        let mut turn = turn_state_test_fixture(status);
        assert!(validate_turn_analysis_state(&turn).is_ok());
        turn.analysis_details = Some("not allowed".to_owned());
        assert!(validate_turn_analysis_state(&turn).is_err());
    }
}

/// Validates one durable memory record score and vector invariants.
/// 校验一条持久记忆记录的分值与向量不变量。
fn validate_memory_scores(record: &MemoryRecord) -> Result<(), String> {
    if !record.has_valid_access_identity()
        || !(0.0..=1.0).contains(&record.importance)
        || record
            .confidence
            .is_some_and(|confidence| !(0.0..=1.0).contains(&confidence))
    {
        return Err(format!("memory score out of range: {}", record.id));
    }
    if let Some(embedding) = &record.embedding {
        if embedding.is_empty() || embedding.iter().any(|value| !value.is_finite()) {
            return Err(format!("memory embedding is invalid: {}", record.id));
        }
        if record.vector_space_id.is_none() {
            return Err(format!("memory vector space is missing: {}", record.id));
        }
    }
    Ok(())
}

/// Verifies one current record against the actor, authorized scope and immutable access identity captured during preflight.
/// 针对操作者、授权作用域及预检时捕获的不可变访问身份校验一条当前记录。
fn memory_matches_access_identity(record: &MemoryRecord, guard: MemoryMutationGuard<'_>) -> bool {
    record.scope_id == *guard.scope_id
        && record.access_class == guard.expected_access.access_class
        && record.source_subject_id == guard.expected_access.source_subject_id
        && record.is_visible_to_subject(guard.actor_subject_id)
}

/// Validates one complete profile node before reference persistence or exact import restore.
/// 在参考持久化或精确导入恢复前校验一条完整画像节点。
fn validate_profile_node_snapshot(node: &ProfileNode) -> Result<(), String> {
    let source_is_valid = match node.source_kind {
        // Pending extraction preserves the real turn link even when source_id was never stamped by review.
        // 待评审提炼保留真实轮次关联，即使source_id尚未由评审盖章也不能伪造编号。
        ProfileSourceKind::TurnExtract => {
            !node.source_turn_ids.is_empty()
                && node.source_id.as_ref().map_or(
                    node.state == ProfileNodeState::Pending,
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
    if node.content.trim().is_empty()
        || node
            .confidence
            .is_some_and(|confidence| !(0.0..=1.0).contains(&confidence))
        || node.priority > 100
        || node.created_at_ns <= 0
        || node.updated_at_ns < node.created_at_ns
        || node.state != ProfileNodeState::Inactive && node.superseded_by.is_some()
        || node.superseded_by.as_ref() == Some(&node.id)
        || node
            .effective_date_ns
            .zip(node.expires_at_ns)
            .is_some_and(|(effective_at_ns, expires_at_ns)| expires_at_ns <= effective_at_ns)
        || node.effective_date_ns.is_some_and(|value| value <= 0)
        || node.expires_at_ns.is_some_and(|value| value <= 0)
        || node.source_turn_ids.iter().collect::<BTreeSet<_>>().len() != node.source_turn_ids.len()
        || !source_is_valid
    {
        return Err(format!("profile node is invalid: {}", node.id));
    }
    Ok(())
}

/// Validates one exact context-evidence snapshot before portable import.
/// 在可移植导入前校验一条精确情境证据快照。
fn validate_context_evidence_snapshot(evidence: &MemoryContextEvidence) -> Result<(), String> {
    let support_valid = (evidence.support_count == 0) == evidence.last_supported_at_ns.is_none();
    let rebuttal_valid = (evidence.rebuttal_count == 0) == evidence.last_rebutted_at_ns.is_none();
    if evidence.context_key.trim().is_empty()
        || evidence.context_value.trim().is_empty()
        || evidence.version == 0
        || evidence
            .support_count
            .saturating_add(evidence.rebuttal_count)
            == 0
        || !support_valid
        || !rebuttal_valid
    {
        return Err(format!(
            "imported context evidence is invalid: {}|{}|{}",
            evidence.memory_id, evidence.context_key, evidence.context_value
        ));
    }
    Ok(())
}

/// Collapses intermediate version advances into one deterministic final row per exact entity.
/// 将中间版本推进折叠为每个精确实体一条确定性的最终记录。
fn final_committed_versions(advanced: Vec<VersionedEntity>) -> Vec<VersionedEntity> {
    let mut final_versions = BTreeMap::new();
    for versioned in advanced {
        final_versions.insert(
            (versioned.entity_kind.as_u32(), versioned.domain_id.clone()),
            versioned,
        );
    }
    final_versions.into_values().collect()
}

/// Summarizes durable side effects for one committed receipt.
/// 为一条已提交收据总结持久副作用。
fn summarize_side_effects(mutations: &[Mutation]) -> String {
    let mut summary = String::new();
    for mutation in mutations {
        let label = match mutation {
            Mutation::PutTurn { .. } => "put-turn",
            Mutation::TransitionTurnStatus { .. } => "transition-turn",
            Mutation::AdvanceChatCompactBoundary { .. } => "advance-boundary",
            Mutation::PutChatCompactBoundary(_) => "put-boundary",
            Mutation::RestoreImportedMemory(_) => "restore-imported-memory",
            Mutation::RestoreImportedProfileNode(_) => "restore-imported-profile",
            Mutation::RestoreImportedContextEvidence(_) => "restore-imported-evidence",
            Mutation::PutMemory { .. } => "put-memory",
            Mutation::ReinforceMemory { .. } => "reinforce",
            Mutation::SupersedeMemory { .. } => "supersede",
            Mutation::AggregateContextEvidence { .. } => "evidence",
            Mutation::DeleteMemory { .. } => "delete",
            Mutation::PutProfileNode { .. } => "put-profile",
            Mutation::RetireProfileNode { .. } => "retire-profile",
            Mutation::PutAnalysisAttempt(_) => "analysis-attempt",
            Mutation::AdvanceSessionExtraction { .. } => "session-extraction",
            Mutation::PutNoisePrototypeBundle { .. } => "noise-bundle",
            Mutation::RecycleMemory { .. } => "recycle",
            Mutation::RestoreMemory { .. } => "restore",
            Mutation::PurgeMemory { .. } => "purge",
        };
        if !summary.is_empty() {
            summary.push('+');
        }
        summary.push_str(label);
    }
    summary
}

/// Calculates lexical token-overlap score for one memory content and query.
/// 计算一条记忆内容与查询的词元重合度分值。
fn lexical_score(content: &str, query: &str) -> f32 {
    let content = content.to_lowercase();
    let tokens = query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if tokens.is_empty() {
        return 0.0;
    }
    let matches = tokens
        .iter()
        .filter(|token| content.contains(token.as_str()))
        .count();
    #[allow(clippy::cast_precision_loss)]
    {
        (matches as f32) / (tokens.len() as f32)
    }
}

/// Calculates cosine similarity for equal finite non-zero vectors.
/// 为相同长度、有限且非零的向量计算余弦相似度。
fn cosine_similarity(left: &[f32], right: &[f32]) -> Option<f32> {
    if left.len() != right.len()
        || left.is_empty()
        || left.iter().chain(right).any(|value| !value.is_finite())
    {
        return None;
    }
    // Finite f32 products fit in f64, including subnormal inputs; epsilon would discard valid scales.
    // 有限 f32 乘积可放入 f64，包括次正规输入；epsilon 会丢弃合法尺度。
    let (dot, left_norm, right_norm) = left.iter().zip(right).fold(
        (0.0_f64, 0.0_f64, 0.0_f64),
        |(dot, left_norm, right_norm), (left, right)| {
            let left = f64::from(*left);
            let right = f64::from(*right);
            (
                left.mul_add(right, dot),
                left.mul_add(left, left_norm),
                right.mul_add(right, right_norm),
            )
        },
    );
    if left_norm == 0.0 || right_norm == 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some((dot / (left_norm.sqrt() * right_norm.sqrt())).clamp(-1.0, 1.0) as f32)
}

/// Builds a lossless component tuple for one noise prototype cache key.
/// 为一个 Noise 原型缓存键构建无损分量元组。
fn noise_cache_key(identity: &NoisePrototypeCacheIdentity) -> NoiseCacheKey {
    (
        identity.scope_id.clone(),
        identity.language.clone(),
        identity.model.clone(),
        identity.dimension,
        identity.rules_hash.clone(),
    )
}

/// Validates explicit scope isolation before any storage access.
/// 在任何存储访问前校验显式作用域隔离。
fn validate_scope(scope_access: &ScopeAccess) -> Result<(), CoreError> {
    scope_access
        .validate()
        .map_err(|diagnostic| storage_error(ErrorCode::ScopeViolation, diagnostic))
}

/// Creates one storage-stage error.
/// 创建一个存储阶段错误。
fn storage_error(code: ErrorCode, diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(code, ErrorStage::Storage, diagnostic)
}
