//! The direct VCE functional entry for refresh, retrieval, and management inspection.
//! VCE 刷新、检索和管理检查使用的直接功能入口。
//!
//! This module composes the existing pure OCE-inspired domain stages. It never creates an AI
//! client, vector database, SQLite connection, watcher, or MCP server; every external operation
//! is routed through the ports in `ports`.
//! 本模块组合现有的 OCE 思路纯领域阶段，但不会创建 AI 客户端、向量数据库、SQLite 连接、监听器或 MCP 服务；
//! 所有外部操作都通过 `ports` 中的端口完成。

use crate::chunking::ChunkerError;
use crate::embedding::{
    EmbeddingError, EmbeddingInput, EmbeddingPlan, EmbeddingSpace, EmbeddingVector, SegmentVector,
};
use crate::indexing::{IndexingConfig, IndexingEngine, IndexingError, IngestResult};
use crate::management::{
    VceManagementQueryRequest, VceManagementQueryResponse, VceOperationPhase,
    validate_management_query, validate_management_response,
};
use crate::model::{FileRecord, FileStatus, SearchHit, SearchSource};
use crate::ports::{
    AuxiliaryIndexStatus, DenseSearchRequest, ExactSearchRequest, FindMissingRequest,
    IndexAbortRequest, IndexCommit, IndexStageBatch, IndexStageReceipt, IndexedBlob,
    IndexedBlobStatus, PathSearchRequest, RefreshMode, RefreshReason, RefreshRequest,
    RetrievalScope, RetrieveRequest, VceAiCapabilitySnapshot, VceAiPort, VceAiPurpose,
    VceAiRequestContext, VceEmbeddingRequest, VceEmbeddingResponse, VceIndexBinding,
    VceIndexManagementSnapshot, VceIndexPort, VceIndexStoreStatus, VceManagementRequest,
    VcePathSearchResponse, VcePortError, VcePortOperation, VceQueryRewriteRequest,
    VceQueryRewriteResponse, VceRerankRequest, VceSearchResponse as IndexSearchResponse,
    VceSourceExpectation, VceSourcePort, VceSourceReadRequest, VceSourceReadResponse,
};
use crate::query::{QueryError, QueryIntent, QueryPlan, QueryPlanner, QueryPlannerConfig};
use crate::retrieval::{
    RankedHits, RerankCallback, RetrievalPipeline, merge_semantic_and_exact, reciprocal_rank_fusion,
};
use crate::state::{VceStateIdentity, VceStateSnapshot};
use crate::structure::{
    FallbackStructureAnalyzer, RecursiveStructureAnalyzer, StructureError, VceStructureAnalyzer,
    validate_structure_analysis,
};
use crate::workspace::{
    Checkpoint, ScopeResolution, SyncPlan, WorkspaceError, WorkspaceSnapshot, WorkspaceState,
};
use futures::{StreamExt, stream};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Configuration for one provider- and storage-neutral VCE engine.
/// 一个不绑定 Provider 和存储实现的 VCE 引擎配置。
#[derive(Debug, Clone, PartialEq)]
pub struct VceConfig {
    /// Stable host-owned namespace dedicated to code-semantic data.
    /// 专用于代码语义数据的稳定宿主持有命名空间。
    pub index_namespace_id: String,
    /// Storage Schema version required from the bound index port.
    /// 绑定索引端口必须提供的存储 Schema 版本。
    pub index_schema_version: u32,
    /// Stable checkpoint chain identity for this code-semantic namespace.
    /// 此代码语义命名空间使用的稳定检查点链身份。
    pub checkpoint_chain_id: String,
    /// Existing deterministic indexing configuration.
    /// 现有的确定性索引配置。
    pub indexing: IndexingConfig,
    /// Existing deterministic query planner configuration.
    /// 现有的确定性查询规划器配置。
    pub query: QueryPlannerConfig,
    /// Existing provider-neutral retrieval fusion configuration.
    /// 现有的不绑定 Provider 的检索融合配置。
    pub retrieval: RetrievalPipeline,
    /// Logical and physical compatibility contract for all VCE vectors.
    /// VCE 所有向量使用的逻辑和物理兼容性契约。
    pub embedding_space: EmbeddingSpace,
    /// Maximum content candidates requested per query variant.
    /// 每个查询变体请求的最大内容候选数量。
    pub dense_candidate_limit: usize,
    /// Maximum path candidates requested per query variant.
    /// 每个查询变体请求的最大路径候选数量。
    pub path_candidate_limit: usize,
    /// Maximum exact symbol candidates requested per retrieval.
    /// 每次检索请求的最大精确符号候选数量。
    pub exact_candidate_limit: usize,
    /// Maximum candidates sent to the optional host Rerank capability.
    /// 发送给可选宿主重排能力的最大候选数量。
    pub rerank_candidate_limit: usize,
    /// Maximum fused candidates retained before optional Rerank and selection.
    /// 可选重排和选择前保留的最大融合候选数量。
    pub fusion_candidate_limit: usize,
    /// Maximum Blob scope for one exact-symbol search.
    /// 一次精确符号搜索允许的最大 Blob 范围。
    pub exact_scope_limit: usize,
    /// Maximum prepared Blobs retained before one staging flush.
    /// 一次暂存刷新前最多保留的已准备 Blob 数量。
    pub index_stage_blob_limit: usize,
    /// Maximum aggregate source bytes prepared before one staging flush.
    /// 一次暂存刷新前最多准备的源码总字节数。
    pub index_stage_max_bytes: u64,
    /// Maximum concurrent host Embedding batches within one logical plan.
    /// 一个逻辑计划内并发调用宿主向量批次的最大数量。
    pub embedding_max_concurrency: usize,
    /// Maximum query variants concurrently executing Embedding and dense retrieval.
    /// 并发执行向量化和内容召回的查询变体最大数量。
    pub query_max_concurrency: usize,
}

impl VceConfig {
    /// Build a validated configuration using the lightweight OCE-inspired defaults.
    /// 使用轻量 OCE 思路默认值构造经过校验的配置。
    pub fn new(
        checkpoint_chain_id: impl Into<String>,
        embedding_space: EmbeddingSpace,
    ) -> Result<Self, VceConfigError> {
        let checkpoint_chain_id = checkpoint_chain_id.into();
        let config = Self {
            index_namespace_id: format!("vce:{checkpoint_chain_id}"),
            index_schema_version: 1,
            checkpoint_chain_id,
            indexing: IndexingConfig::default(),
            query: QueryPlannerConfig::default(),
            retrieval: RetrievalPipeline::default(),
            embedding_space,
            dense_candidate_limit: 64,
            path_candidate_limit: 32,
            exact_candidate_limit: 32,
            rerank_candidate_limit: 32,
            fusion_candidate_limit: 64,
            exact_scope_limit: 2_000,
            index_stage_blob_limit: 8,
            index_stage_max_bytes: 4 * 1_048_576,
            embedding_max_concurrency: 4,
            query_max_concurrency: 3,
        };
        config.validate()?;
        Ok(config)
    }

    /// Validate all identifiers, vector compatibility fields, and lookup budgets.
    /// 校验所有身份标识、向量兼容性字段和检索预算。
    pub fn validate(&self) -> Result<(), VceConfigError> {
        if self.checkpoint_chain_id.trim().is_empty() || self.index_namespace_id.trim().is_empty() {
            return Err(VceConfigError::EmptyCheckpointChainId);
        }
        if !self.embedding_space.is_well_formed() {
            return Err(VceConfigError::InvalidEmbeddingSpace);
        }
        if self.dense_candidate_limit == 0
            || self.path_candidate_limit == 0
            || self.exact_candidate_limit == 0
            || self.rerank_candidate_limit == 0
            || self.fusion_candidate_limit == 0
            || self.exact_scope_limit == 0
            || self.index_stage_blob_limit == 0
            || self.index_stage_max_bytes == 0
            || self.embedding_max_concurrency == 0
            || self.query_max_concurrency == 0
            || self.index_schema_version == 0
        {
            return Err(VceConfigError::InvalidLookupBudget);
        }
        if self.index_stage_max_bytes < self.indexing.max_file_bytes {
            return Err(VceConfigError::InvalidStagingBudget);
        }
        if self.indexing.max_file_bytes == 0
            || self.indexing.embedding_max_inputs == 0
            || self.indexing.embedding_max_chars == 0
            || self.indexing.embedding_max_segment_chars == 0
            || self.indexing.embedding_overlap_chars >= self.indexing.embedding_max_segment_chars
        {
            return Err(VceConfigError::InvalidIndexingConfig);
        }
        let retrieval = self.retrieval;
        if !retrieval.rrf.rrf_k.is_finite()
            || retrieval.rrf.rrf_k < 0.0
            || !retrieval.rrf.original_query_weight.is_finite()
            || retrieval.rrf.original_query_weight <= 0.0
            || !retrieval.rrf.facet_query_weight.is_finite()
            || retrieval.rrf.facet_query_weight <= 0.0
            || retrieval.coverage.max_per_path == 0
            || retrieval.coverage.max_chars == 0
            || retrieval.coverage.max_results == 0
            || !retrieval.coverage.overlap_threshold.is_finite()
            || !(0.0..=1.0).contains(&retrieval.coverage.overlap_threshold)
            || retrieval
                .confidence_floor
                .is_some_and(|floor| !floor.is_finite())
            || !retrieval.path_boost_weight.is_finite()
            || retrieval.path_boost_weight < 0.0
        {
            return Err(VceConfigError::InvalidRetrievalConfig);
        }
        Ok(())
    }
}

/// Configuration errors detected before the functional engine is created.
/// 创建功能引擎前检测到的配置错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VceConfigError {
    /// The checkpoint chain identity is empty.
    /// 检查点链身份为空。
    EmptyCheckpointChainId,
    /// The vector-space compatibility contract is incomplete or dimensionless.
    /// 向量空间兼容性契约不完整或维度为零。
    InvalidEmbeddingSpace,
    /// A host lookup budget is zero.
    /// 宿主检索预算为零。
    InvalidLookupBudget,
    /// The staging byte budget cannot contain one accepted source file.
    /// 暂存字节预算无法容纳一个已接受的源码文件。
    InvalidStagingBudget,
    /// Source admission or Embedding planning budgets are internally inconsistent.
    /// 源码准入或向量规划预算内部不一致。
    InvalidIndexingConfig,
    /// Retrieval scoring or coverage configuration is invalid.
    /// 检索评分或覆盖配置无效。
    InvalidRetrievalConfig,
    /// Existing deterministic Chunker construction failed.
    /// 现有确定性切块器构造失败。
    Chunker(ChunkerError),
    /// Existing deterministic query planner construction failed.
    /// 现有确定性查询规划器构造失败。
    QueryPlanner(QueryError),
    /// A host-restored workspace snapshot violated the VCE state contract.
    /// 宿主恢复的工作区快照违反了 VCE 状态契约。
    WorkspaceSnapshot(WorkspaceError),
    /// Structural analyzer construction or compatibility validation failed.
    /// 结构分析器构造或兼容性校验失败。
    Structure(StructureError),
    /// A versioned persisted engine snapshot was invalid or incompatible.
    /// 版本化持久化引擎快照无效或不兼容。
    StateSnapshot(String),
}

impl Display for VceConfigError {
    /// Format a deterministic configuration diagnostic.
    /// 将配置错误格式化为确定性的诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCheckpointChainId => {
                formatter.write_str("VCE checkpoint chain identity must not be empty")
            }
            Self::InvalidEmbeddingSpace => {
                formatter.write_str("VCE Embedding space is incomplete or has zero dimension")
            }
            Self::InvalidLookupBudget => {
                formatter.write_str("VCE lookup budgets must be greater than zero")
            }
            Self::InvalidStagingBudget => formatter
                .write_str("VCE staging byte budget must cover the maximum accepted source file"),
            Self::InvalidIndexingConfig => formatter
                .write_str("VCE indexing and Embedding budgets are internally inconsistent"),
            Self::InvalidRetrievalConfig => {
                formatter.write_str("VCE retrieval scoring or coverage configuration is invalid")
            }
            Self::Chunker(error) => write!(formatter, "VCE Chunker configuration failed: {error}"),
            Self::QueryPlanner(error) => {
                write!(formatter, "VCE query planner configuration failed: {error}")
            }
            Self::WorkspaceSnapshot(error) => {
                write!(formatter, "VCE workspace snapshot restore failed: {error}")
            }
            Self::Structure(error) => {
                write!(
                    formatter,
                    "VCE structure analyzer configuration failed: {error}"
                )
            }
            Self::StateSnapshot(reason) => {
                write!(formatter, "VCE persisted state restore failed: {reason}")
            }
        }
    }
}

impl Error for VceConfigError {}

/// Lifecycle state exposed to VulcanCode management pages.
/// 暴露给 VulcanCode 管理页面的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VceEngineState {
    /// The engine has no committed checkpoint yet.
    /// 引擎尚未拥有已提交检查点。
    Cold,
    /// A committed workspace snapshot is available.
    /// 已有可用的已提交工作区快照。
    Ready,
    /// The workspace has an explicit change that has not been published to the index.
    /// 工作区存在尚未发布到索引的显式变更。
    PendingRefresh,
    /// A refresh is currently being processed by the caller.
    /// 调用方当前正在处理刷新。
    Refreshing,
    /// A retrieval is currently being processed by the caller.
    /// 调用方当前正在处理检索。
    Retrieving,
    /// The last functional operation failed and the error remains visible.
    /// 最近一次功能操作失败，错误仍然可见。
    Failed,
}

/// Optional retrieval features that were unavailable but safely bypassed.
/// 安全绕过但当前不可用的可选检索能力。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum VceDegradedFeature {
    /// The host LLM query rewrite was not used.
    /// 未使用宿主 LLM 查询改写。
    QueryRewrite,
    /// The host candidate Rerank was not used.
    /// 未使用宿主候选重排。
    Rerank,
    /// The optional path vector channel was not used.
    /// 未使用可选路径向量通道。
    PathSearch,
    /// The optional exact symbol channel was not used.
    /// 未使用可选精确符号通道。
    ExactSearch,
}

/// One observable optional-stage degradation retained for diagnostics and management history.
/// 为诊断和管理历史保留的一项可观测可选阶段降级。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VceDegradation {
    /// Optional capability or search channel that was bypassed.
    /// 被绕过的可选能力或搜索通道。
    pub feature: VceDegradedFeature,
    /// Stable operation identity associated with the degradation.
    /// 与此次降级关联的稳定操作标识。
    pub operation_id: String,
    /// Explicit diagnostic explaining why the optional stage was bypassed.
    /// 解释可选阶段为何被绕过的明确诊断。
    pub reason: String,
}

/// Summary of one successful or attempted refresh operation.
/// 一次刷新操作的成功或尝试摘要。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VceRefreshSummary {
    /// Stable refresh request identity.
    /// 稳定的刷新请求标识。
    pub request_id: String,
    /// Refresh mode used by the host.
    /// 宿主使用的刷新模式。
    pub mode: RefreshMode,
    /// Refresh reason recorded for diagnostics.
    /// 用于诊断记录的刷新原因。
    pub reason: RefreshReason,
    /// Workspace generation after reconciliation.
    /// 协调完成后的工作区代数。
    pub generation: u64,
    /// Number of newly visible Blob identities.
    /// 新可见 Blob 身份数量。
    pub added_blob_count: usize,
    /// Number of removed Blob identities.
    /// 被移除 Blob 身份数量。
    pub deleted_blob_count: usize,
    /// Number of Blob records prepared and committed by VCE.
    /// 由 VCE 准备并提交的 Blob 记录数量。
    pub indexed_blob_count: usize,
    /// Number of skipped workspace paths in the reconciled snapshot.
    /// 协调后快照中被跳过的工作区路径数量。
    pub skipped_path_count: usize,
    /// Checkpoint version after a successful commit.
    /// 成功提交后的检查点版本。
    pub checkpoint_version: Option<u64>,
    /// Optional host-provided source revision.
    /// 宿主提供的可选源版本。
    pub source_revision: Option<String>,
}

/// Summary of one retrieval operation used by statistics and page projections.
/// 用于统计和页面投影的一次检索操作摘要。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VceRetrievalSummary {
    /// Stable retrieval request identity.
    /// 稳定的检索请求标识。
    pub request_id: String,
    /// Classified query intent.
    /// 分类得到的查询意图。
    pub intent: QueryIntent,
    /// Number of query variants actually searched.
    /// 实际执行搜索的查询变体数量。
    pub query_count: usize,
    /// Raw dense result count before fusion.
    /// 融合前的原始内容向量结果数量。
    pub dense_hit_count: usize,
    /// Raw path result count before fusion.
    /// 融合前的原始路径结果数量。
    pub path_hit_count: usize,
    /// Raw exact result count before fusion.
    /// 融合前的原始精确结果数量。
    pub exact_hit_count: usize,
    /// Candidate count before final coverage selection.
    /// 最终覆盖选择前的候选数量。
    pub candidate_count: usize,
    /// Selected context hit count.
    /// 最终选中的上下文命中数量。
    pub selected_hit_count: usize,
    /// Optional stages safely bypassed during this operation.
    /// 此次操作中安全绕过的可选阶段。
    pub degraded_features: Vec<VceDegradedFeature>,
    /// Detailed optional-stage diagnostics for management history.
    /// 管理历史使用的可选阶段详细诊断。
    pub degradations: Vec<VceDegradation>,
}

/// Engine-local state snapshot that does not require host storage I/O.
/// 不需要宿主存储 I/O 的引擎本地状态快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceEngineSnapshot {
    /// Current engine lifecycle state.
    /// 当前引擎生命周期状态。
    pub state: VceEngineState,
    /// Current workspace generation.
    /// 当前工作区代数。
    pub workspace_generation: u64,
    /// Last committed checkpoint version, if available.
    /// 最近一次已提交检查点版本（如果可用）。
    pub checkpoint_version: Option<u64>,
    /// Last refresh summary, if available.
    /// 最近一次刷新摘要（如果可用）。
    pub last_refresh: Option<VceRefreshSummary>,
    /// Last retrieval summary, if available.
    /// 最近一次检索摘要（如果可用）。
    pub last_retrieval: Option<VceRetrievalSummary>,
    /// Last functional error, if any.
    /// 最近一次功能错误（如果有）。
    pub last_error: Option<String>,
}

/// Workspace counts derived from the VCE-owned in-memory snapshot.
/// 从 VCE 持有的内存快照推导出的工作区统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VceWorkspaceStatistics {
    /// Total retained file records.
    /// 保留的文件记录总数。
    pub files: u64,
    /// Present files eligible for indexing or retrieval.
    /// 可参与索引或检索的存在文件数量。
    pub present_files: u64,
    /// Skipped files retained for diagnostics.
    /// 为诊断保留的跳过文件数量。
    pub skipped_files: u64,
    /// Deleted file records retained for reconciliation.
    /// 为协调保留的已删除文件记录数量。
    pub deleted_files: u64,
    /// Present records carrying a Blob identity.
    /// 携带 Blob 身份的存在记录数量。
    pub present_blobs: u64,
    /// Number of members in the current checkpoint.
    /// 当前检查点成员数量。
    pub checkpoint_blobs: u64,
}

/// Counters for refresh, retrieval, and result-channel usage.
/// 刷新、检索和结果通道使用统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VceQueryStatistics {
    /// Number of refresh attempts.
    /// 刷新尝试次数。
    pub refresh_attempts: u64,
    /// Number of successful refreshes.
    /// 成功刷新次数。
    pub refresh_successes: u64,
    /// Number of retrieval attempts.
    /// 检索尝试次数。
    pub retrieval_attempts: u64,
    /// Number of successful retrievals.
    /// 成功检索次数。
    pub retrieval_successes: u64,
    /// Total dense hits returned by the host.
    /// 宿主返回的内容向量命中总数。
    pub dense_hits: u64,
    /// Total path hits returned by the host.
    /// 宿主返回的路径命中总数。
    pub path_hits: u64,
    /// Total exact hits returned by the host.
    /// 宿主返回的精确命中总数。
    pub exact_hits: u64,
    /// Total final context hits selected by the engine.
    /// 引擎最终选中的上下文命中总数。
    pub selected_hits: u64,
}

/// Combined local and host data consumed by a VulcanCode page projection.
/// 供 VulcanCode 页面投影使用的本地与宿主组合管理数据。
#[derive(Debug, Clone, PartialEq)]
pub struct VceManagementSnapshot {
    /// Engine-local lifecycle and last-operation state.
    /// 引擎本地生命周期和最近操作状态。
    pub engine: VceEngineSnapshot,
    /// Workspace counts derived without storage access.
    /// 不访问存储即可得到的工作区统计。
    pub workspace: VceWorkspaceStatistics,
    /// Retrieval and refresh counters.
    /// 检索和刷新计数器。
    pub query_statistics: VceQueryStatistics,
    /// Host AI registration status.
    /// 宿主 AI 注册状态。
    pub ai: VceAiCapabilitySnapshot,
    /// Host storage status, counts, and integrity data when queried.
    /// 查询后获得的宿主存储状态、统计和完整性数据。
    pub index: Option<VceIndexManagementSnapshot>,
}

/// Result of one functional workspace refresh.
/// 一次功能性工作区刷新的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct RefreshResult {
    /// Stable refresh request identity.
    /// 稳定的刷新请求标识。
    pub request_id: String,
    /// Resulting workspace generation.
    /// 生成的工作区代数。
    pub generation: u64,
    /// Deterministic OCE-inspired synchronization plan.
    /// OCE 思路确定性同步计划。
    pub sync_plan: SyncPlan,
    /// Blob records actually prepared and committed by VCE.
    /// 由 VCE 实际准备并提交的 Blob 记录。
    pub indexed_blobs: Vec<String>,
    /// Blob identities removed from host storage.
    /// 从宿主存储中移除的 Blob 身份。
    pub deleted_blobs: BTreeSet<String>,
    /// Paths intentionally skipped by admission rules.
    /// 按准入规则主动跳过的路径。
    pub skipped_paths: BTreeSet<String>,
    /// Published checkpoint version, if the refresh produced one.
    /// 此次刷新产生的已发布检查点版本。
    pub checkpoint_version: Option<u64>,
    /// Host commit receipt when a storage commit was required.
    /// 需要存储提交时返回的宿主提交回执。
    pub commit_receipt: Option<crate::ports::CommitReceipt>,
}

/// Result of one functional semantic retrieval.
/// 一次功能性语义检索的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct RetrieveResult {
    /// Stable retrieval request identity.
    /// 稳定的检索请求标识。
    pub request_id: String,
    /// Final deterministic query plan including accepted rewrites.
    /// 包含已接受改写的最终确定性查询计划。
    pub query_plan: QueryPlan,
    /// Exact Blob scope used for every host search.
    /// 所有宿主搜索实际使用的精确 Blob 范围。
    pub scope: BTreeSet<String>,
    /// Final coverage-selected structured hits.
    /// 最终经过覆盖选择的结构化命中。
    pub hits: Vec<SearchHit>,
    /// Line-numbered context rendered after selection.
    /// 选择完成后渲染的带行号上下文。
    pub formatted_context: String,
    /// Optional stages safely bypassed for this retrieval.
    /// 此次检索安全绕过的可选阶段。
    pub degraded_features: Vec<VceDegradedFeature>,
    /// Detailed reasons and operation identities for every bypassed optional stage.
    /// 每个被绕过可选阶段的详细原因与操作标识。
    pub degradations: Vec<VceDegradation>,
    /// Retrieval counters for this operation.
    /// 此次操作的检索计数摘要。
    pub summary: VceRetrievalSummary,
}

/// Errors raised by the direct VCE functional entry.
/// VCE 直接功能入口产生的错误。
#[derive(Debug)]
pub enum VceError {
    /// A refresh or retrieval request is malformed.
    /// 刷新或检索请求格式无效。
    InvalidRequest(String),
    /// Workspace reconciliation failed.
    /// 工作区协调失败。
    Workspace(WorkspaceError),
    /// Deterministic indexing failed.
    /// 确定性索引失败。
    Indexing(crate::indexing::IndexingError),
    /// Embedding planning or aggregation failed before the host commit.
    /// 在宿主提交前向量计划或聚合失败。
    Embedding(EmbeddingError),
    /// Query planning failed.
    /// 查询规划失败。
    Query(QueryError),
    /// Host-independent structural analysis failed before provider execution.
    /// 不依赖宿主的结构分析在供应商执行前失败。
    Structure(StructureError),
    /// An external AI or storage port failed.
    /// 外部 AI 或存储端口失败。
    Port(VcePortError),
    /// The current-checkpoint scope cannot be resolved safely.
    /// 无法安全解析当前检查点范围。
    MissingCheckpoint,
    /// The caller explicitly supplied an empty search scope.
    /// 调用方显式提供了空搜索范围。
    EmptyScope,
    /// A host result escaped the exact authorized scope.
    /// 宿主结果越过了精确授权范围。
    ScopeViolation {
        /// Blob identity returned outside the authorized retrieval scope.
        /// 在授权检索范围之外返回的 Blob 身份。
        blob_name: String,
    },
    /// A host response identity did not match the request.
    /// 宿主响应身份与请求不匹配。
    ResponseIdentity {
        /// Operation that returned the mismatched response.
        /// 返回身份不匹配响应的操作。
        operation: VcePortOperation,
        /// Expected operation identity.
        /// 预期的操作标识。
        expected: String,
        /// Actual operation identity.
        /// 实际返回的操作标识。
        actual: String,
    },
    /// A host AI binding field did not match the fixed VCE registration contract.
    /// 宿主 AI 绑定字段与 VCE 固定注册契约不匹配。
    AiBindingMismatch {
        /// Binding field whose value did not match.
        /// 值不匹配的绑定字段。
        field: String,
        /// Expected value from the configured registration.
        /// 来自配置注册的预期值。
        expected: String,
        /// Actual value reported by the host capability.
        /// 宿主能力报告的实际值。
        actual: String,
    },
    /// A host index binding field did not match the configured VCE namespace.
    /// 宿主索引绑定字段与配置的 VCE 命名空间不匹配。
    IndexBindingMismatch {
        /// Binding field whose value did not match.
        /// 值不匹配的绑定字段。
        field: String,
        /// Expected value from the VCE configuration.
        /// 来自 VCE 配置的预期值。
        expected: String,
        /// Actual value reported by the host index port.
        /// 宿主索引端口报告的实际值。
        actual: String,
    },
    /// A required host AI capability is not registered or available.
    /// 所需宿主 AI 能力未注册或不可用。
    AiCapabilityUnavailable {
        /// Semantic AI purpose required by the current operation.
        /// 当前操作所需的语义 AI 用途。
        purpose: VceAiPurpose,
    },
    /// A host returned a vector space incompatible with the index namespace.
    /// 宿主返回了与索引命名空间不兼容的向量空间。
    EmbeddingSpaceMismatch {
        /// Vector-space identity required by the VCE namespace.
        /// VCE 命名空间要求的向量空间身份。
        expected: String,
        /// Vector-space identity reported by the host.
        /// 宿主报告的向量空间身份。
        actual: String,
    },
    /// The host returned a validly typed response with invalid VCE data.
    /// 宿主返回了类型正确但违反 VCE 数据契约的响应。
    InvalidHostResponse {
        /// Host port operation that returned invalid data.
        /// 返回无效数据的宿主端口操作。
        operation: VcePortOperation,
        /// Explicit VCE contract violation detail.
        /// 明确的 VCE 契约违反详情。
        reason: String,
    },
    /// Cleanup of unpublished staging failed after the primary refresh error.
    /// 主刷新错误发生后，未发布暂存数据的清理也失败。
    StageAbortFailed {
        /// Original refresh error.
        /// 原始刷新错误。
        primary: String,
        /// Staging abort error.
        /// 暂存中止错误。
        abort: String,
    },
}

impl Display for VceError {
    /// Format a functional-entry error for host diagnostics.
    /// 将功能入口错误格式化为宿主诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(formatter, "invalid VCE request: {message}"),
            Self::Workspace(error) => {
                write!(formatter, "VCE workspace reconciliation failed: {error}")
            }
            Self::Indexing(error) => write!(formatter, "VCE indexing failed: {error}"),
            Self::Embedding(error) => write!(formatter, "VCE Embedding processing failed: {error}"),
            Self::Query(error) => write!(formatter, "VCE query planning failed: {error}"),
            Self::Structure(error) => write!(formatter, "VCE structure analysis failed: {error}"),
            Self::Port(error) => write!(formatter, "VCE host port failed: {error}"),
            Self::MissingCheckpoint => {
                formatter.write_str("VCE retrieval requires an explicit committed checkpoint")
            }
            Self::EmptyScope => formatter.write_str("VCE retrieval scope must not be empty"),
            Self::ScopeViolation { blob_name } => {
                write!(
                    formatter,
                    "VCE host result escaped authorized Blob scope: {blob_name}"
                )
            }
            Self::ResponseIdentity {
                operation,
                expected,
                actual,
            } => write!(
                formatter,
                "VCE {} response identity mismatch: expected `{expected}`, received `{actual}`",
                operation.as_str()
            ),
            Self::AiBindingMismatch {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "VCE AI binding `{field}` mismatch: expected `{expected}`, received `{actual}`"
            ),
            Self::IndexBindingMismatch {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "VCE index binding `{field}` mismatch: expected `{expected}`, received `{actual}`"
            ),
            Self::AiCapabilityUnavailable { purpose } => {
                write!(
                    formatter,
                    "VCE AI capability `{}` is unavailable",
                    purpose.as_str()
                )
            }
            Self::EmbeddingSpaceMismatch { expected, actual } => write!(
                formatter,
                "VCE Embedding space mismatch: expected `{expected}`, received `{actual}`"
            ),
            Self::InvalidHostResponse { operation, reason } => write!(
                formatter,
                "VCE {} host response is invalid: {reason}",
                operation.as_str()
            ),
            Self::StageAbortFailed { primary, abort } => write!(
                formatter,
                "VCE refresh failed with `{primary}` and staging cleanup failed with `{abort}`"
            ),
        }
    }
}

impl Error for VceError {}

impl From<WorkspaceError> for VceError {
    /// Convert a workspace state error into a functional-entry error.
    /// 将工作区状态错误转换为功能入口错误。
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}

impl From<crate::indexing::IndexingError> for VceError {
    /// Convert an indexing state error into a functional-entry error.
    /// 将索引状态错误转换为功能入口错误。
    fn from(error: crate::indexing::IndexingError) -> Self {
        Self::Indexing(error)
    }
}

impl From<EmbeddingError> for VceError {
    /// Convert Embedding planning and aggregation errors into a functional-entry error.
    /// 将向量计划和聚合错误转换为功能入口错误。
    fn from(error: EmbeddingError) -> Self {
        Self::Embedding(error)
    }
}

impl From<QueryError> for VceError {
    /// Convert a query planning error into a functional-entry error.
    /// 将查询规划错误转换为功能入口错误。
    fn from(error: QueryError) -> Self {
        Self::Query(error)
    }
}

impl From<StructureError> for VceError {
    /// Convert structural-analysis errors into a functional-entry error.
    /// 将结构分析错误转换为功能入口错误。
    fn from(error: StructureError) -> Self {
        Self::Structure(error)
    }
}

impl From<VcePortError> for VceError {
    /// Convert a host port error into a functional-entry error.
    /// 将宿主端口错误转换为功能入口错误。
    fn from(error: VcePortError) -> Self {
        Self::Port(error)
    }
}

/// Compose workspace state, indexing, query planning, retrieval, and management logic.
/// 组合工作区状态、索引、查询规划、检索和管理逻辑。
#[derive(Debug)]
pub struct VceEngine {
    /// Immutable engine configuration.
    /// 不可变引擎配置。
    config: VceConfig,
    /// Provider-independent workspace and checkpoint state.
    /// 不绑定 Provider 的工作区和检查点状态。
    workspace: WorkspaceState,
    /// Existing deterministic indexing stage.
    /// 现有确定性索引阶段。
    indexing: IndexingEngine,
    /// Host-independent semantic analyzer with a guaranteed recursive fallback implementation.
    /// 具备递归回退保证且不依赖宿主的语义分析器。
    structure_analyzer: Box<dyn VceStructureAnalyzer>,
    /// Existing deterministic query planner.
    /// 现有确定性查询规划器。
    query_planner: QueryPlanner,
    /// Existing deterministic retrieval fusion and coverage stage.
    /// 现有确定性检索融合和覆盖选择阶段。
    retrieval: RetrievalPipeline,
    /// Current lifecycle state shown to management callers.
    /// 提供给管理调用方的当前生命周期状态。
    state: VceEngineState,
    /// Last refresh operation summary.
    /// 最近一次刷新操作摘要。
    last_refresh: Option<VceRefreshSummary>,
    /// Last retrieval operation summary.
    /// 最近一次检索操作摘要。
    last_retrieval: Option<VceRetrievalSummary>,
    /// Last operation error retained for page diagnostics.
    /// 为页面诊断保留的最近一次操作错误。
    last_error: Option<String>,
    /// Counters retained independently from host storage counters.
    /// 独立于宿主存储统计保留的计数器。
    statistics: VceQueryStatistics,
}

/// Deterministic result of one independently executable query variant.
/// 一个可独立执行查询变体的确定性结果。
#[derive(Debug)]
struct QueryVariantResult {
    /// Original query-plan position used to restore deterministic ordering.
    /// 用于恢复确定性顺序的原始查询计划位置。
    query_index: usize,
    /// Required dense content hits.
    /// 必需的内容向量命中。
    dense_hits: Vec<SearchHit>,
    /// Optional path-document hits.
    /// 可选的路径文档命中。
    path_hits: Vec<SearchHit>,
    /// Optional first-content fallbacks associated with path hits.
    /// 与路径命中关联的可选首内容回填。
    path_fallback_hits: Vec<SearchHit>,
    /// Optional-stage diagnostics produced by this query variant.
    /// 此查询变体产生的可选阶段诊断。
    degradations: Vec<VceDegradation>,
}

/// Owned input for one bounded concurrent query-variant operation.
/// 一个有界并发查询变体操作的自有输入。
#[derive(Debug)]
struct QueryVariantRequest {
    /// Stable parent retrieval identity.
    /// 稳定的父检索身份。
    request_id: String,
    /// Original query-plan position.
    /// 原始查询计划位置。
    query_index: usize,
    /// Exact query text to embed and search.
    /// 待向量化和搜索的精确查询文本。
    query: String,
    /// Whether the optional path channel should run.
    /// 是否应执行可选路径通道。
    enable_path_index: bool,
    /// Exact authorized Blob scope.
    /// 精确授权的 Blob 范围。
    scope: BTreeSet<String>,
}

/// Borrowed dependencies and stable identities shared by one refresh staging operation.
/// 一次刷新暂存操作共享的借用依赖与稳定身份。
struct RefreshStageContext<'a> {
    /// Stable parent refresh identity used to derive child operation IDs.
    /// 用于派生子操作 ID 的稳定父刷新身份。
    refresh_id: &'a str,
    /// Workspace generation published by this refresh.
    /// 本次刷新发布的工作区代次。
    generation: u64,
    /// Host-owned code index port.
    /// 宿主持有的代码索引端口。
    index: &'a dyn VceIndexPort,
    /// Host-owned shared AI port.
    /// 宿主持有的共享 AI 端口。
    ai: &'a dyn VceAiPort,
    /// Frozen and validated AI capability snapshot for this refresh.
    /// 本次刷新使用的已冻结且经过校验的 AI 能力快照。
    capability: &'a VceAiCapabilitySnapshot,
    /// Host-owned immutable source snapshot port.
    /// 宿主持有的不可变源码快照端口。
    source: &'a dyn VceSourcePort,
}

impl VceEngine {
    /// Construct an empty functional engine from validated configuration.
    /// 根据经过校验的配置构造空的功能引擎。
    pub fn new(config: VceConfig) -> Result<Self, VceConfigError> {
        config.validate()?;
        let indexing = IndexingEngine::new(config.indexing).map_err(|error| match error {
            IndexingError::Chunking(error) => VceConfigError::Chunker(error),
            _ => VceConfigError::InvalidIndexingConfig,
        })?;
        let structure_analyzer = RecursiveStructureAnalyzer::new(
            config.indexing.chunker,
            config.embedding_space.chunker_version.clone(),
        )
        .map_err(VceConfigError::Structure)?;
        let query_planner =
            QueryPlanner::new(config.query).map_err(VceConfigError::QueryPlanner)?;
        Ok(Self {
            retrieval: config.retrieval,
            config,
            workspace: WorkspaceState::default(),
            indexing,
            structure_analyzer: Box::new(structure_analyzer),
            query_planner,
            state: VceEngineState::Cold,
            last_refresh: None,
            last_retrieval: None,
            last_error: None,
            statistics: VceQueryStatistics::default(),
        })
    }

    /// Replace the built-in fallback with a compatible host-independent structural analyzer.
    /// 使用兼容且不依赖宿主的结构分析器替换内置回退实现。
    pub fn with_structure_analyzer(
        mut self,
        analyzer: Box<dyn VceStructureAnalyzer>,
    ) -> Result<Self, VceConfigError> {
        if analyzer.version() != self.config.embedding_space.chunker_version {
            return Err(VceConfigError::Structure(StructureError::VersionMismatch {
                expected: self.config.embedding_space.chunker_version.clone(),
                actual: analyzer.version().to_owned(),
            }));
        }
        let fallback = RecursiveStructureAnalyzer::new(
            self.config.indexing.chunker,
            self.config.embedding_space.chunker_version.clone(),
        )
        .map_err(VceConfigError::Structure)?;
        self.structure_analyzer = Box::new(
            FallbackStructureAnalyzer::new(analyzer, fallback)
                .map_err(VceConfigError::Structure)?,
        );
        Ok(self)
    }

    /// Construct an engine whose pure workspace state was restored by the host.
    /// 构造一个使用宿主恢复的纯工作区状态的引擎。
    pub fn from_workspace_snapshot(
        config: VceConfig,
        snapshot: WorkspaceSnapshot,
    ) -> Result<Self, VceConfigError> {
        let mut engine = Self::new(config)?;
        engine.workspace =
            WorkspaceState::from_snapshot(snapshot).map_err(VceConfigError::WorkspaceSnapshot)?;
        engine.state = workspace_state(&engine.workspace);
        Ok(engine)
    }

    /// Construct an engine from one validated, namespace-bound persisted snapshot.
    /// 根据一份经过校验且绑定命名空间的持久化快照构造引擎。
    pub fn from_state_snapshot(
        config: VceConfig,
        snapshot: VceStateSnapshot,
    ) -> Result<Self, VceConfigError> {
        snapshot
            .validate()
            .map_err(|error| VceConfigError::StateSnapshot(error.to_string()))?;
        validate_state_identity(&config, &snapshot.identity)?;
        let mut engine = Self::new(config)?;
        engine.workspace = WorkspaceState::from_snapshot(snapshot.workspace)
            .map_err(VceConfigError::WorkspaceSnapshot)?;
        engine.statistics = snapshot.statistics;
        engine.last_refresh = snapshot.last_refresh;
        engine.last_retrieval = snapshot.last_retrieval;
        engine.last_error = snapshot.last_error;
        engine.state = if engine.last_error.is_some() {
            VceEngineState::Failed
        } else {
            workspace_state(&engine.workspace)
        };
        Ok(engine)
    }

    /// Return the immutable VCE configuration.
    /// 返回不可变 VCE 配置。
    pub fn config(&self) -> &VceConfig {
        &self.config
    }

    /// Return the current pure workspace snapshot.
    /// 返回当前纯工作区快照。
    pub fn workspace_snapshot(&self) -> WorkspaceSnapshot {
        self.workspace.snapshot()
    }

    /// Return the immutable identity that binds persisted state and host index storage.
    /// 返回绑定持久化状态与宿主索引存储的不可变身份。
    pub fn state_identity(&self) -> VceStateIdentity {
        VceStateIdentity::new(
            self.config.index_namespace_id.clone(),
            self.config.checkpoint_chain_id.clone(),
            self.config.index_schema_version,
            self.config.embedding_space.clone(),
        )
    }

    /// Return all VCE-owned state that an upper host may persist atomically.
    /// 返回上层宿主可原子持久化的全部 VCE 自有状态。
    pub fn state_snapshot(&self) -> VceStateSnapshot {
        VceStateSnapshot::new(
            self.state_identity(),
            self.workspace.snapshot(),
            self.statistics,
            self.last_refresh.clone(),
            self.last_retrieval.clone(),
            self.last_error.clone(),
        )
    }

    /// Return the deterministic sync plan without invoking storage or AI.
    /// 在不调用存储和 AI 的情况下返回确定性同步计划。
    pub fn sync_plan(&self) -> SyncPlan {
        self.workspace.sync_plan()
    }

    /// Add an explicit unsaved overlay through the functional engine entry.
    /// 通过功能引擎入口添加显式的未保存内容覆盖层。
    ///
    /// The host should call `refresh` afterwards when it wants the overlay to be
    /// prepared and published to the registered index port.
    /// 宿主如果希望将覆盖层准备并发布到已注册的索引端口，应随后调用 `refresh`。
    pub fn observe(
        &mut self,
        path: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<WorkspaceSnapshot, VceError> {
        self.workspace.observe(path, content)?;
        self.state = workspace_state(&self.workspace);
        Ok(self.workspace.snapshot())
    }

    /// Mark one workspace-relative path as removed through the functional engine entry.
    /// 通过功能引擎入口将一个工作区相对路径标记为已移除。
    ///
    /// The host still decides when to publish the deletion by calling `refresh`.
    /// 宿主仍然负责决定何时通过 `refresh` 发布删除变更。
    pub fn remove(&mut self, path: impl Into<String>) -> Result<WorkspaceSnapshot, VceError> {
        self.workspace.remove(path)?;
        self.state = workspace_state(&self.workspace);
        Ok(self.workspace.snapshot())
    }

    /// Plan one query without invoking AI or host storage.
    /// 在不调用 AI 和宿主存储的情况下规划一次查询。
    pub fn plan_query(&self, information_request: &str) -> Result<QueryPlan, VceError> {
        Ok(self.query_planner.plan(information_request)?)
    }

    /// Resolve the current committed checkpoint without widening scope.
    /// 解析当前已提交检查点，且不会扩大检索范围。
    pub fn current_scope(&self) -> ScopeResolution {
        self.workspace.resolve_scope(Vec::new(), Vec::new())
    }

    /// Return local lifecycle, workspace, and query statistics.
    /// 返回本地生命周期、工作区和查询统计。
    pub fn status(&self) -> VceEngineSnapshot {
        self.engine_snapshot()
    }

    /// Return engine-local operation counters.
    /// 返回引擎本地操作计数器。
    pub fn statistics(&self) -> VceQueryStatistics {
        self.statistics
    }

    /// Return workspace counts without calling the host index store.
    /// 在不调用宿主索引存储的情况下返回工作区统计。
    pub fn workspace_statistics(&self) -> VceWorkspaceStatistics {
        let mut statistics = VceWorkspaceStatistics {
            files: self.workspace.records.len() as u64,
            checkpoint_blobs: self
                .workspace
                .checkpoint
                .as_ref()
                .map_or(0, |checkpoint| checkpoint.members.len() as u64),
            ..VceWorkspaceStatistics::default()
        };
        for record in self.workspace.records.values() {
            match record.status {
                FileStatus::Present => {
                    statistics.present_files += 1;
                    if record.blob_name.is_some() {
                        statistics.present_blobs += 1;
                    }
                }
                FileStatus::Skipped => statistics.skipped_files += 1,
                FileStatus::Deleted => statistics.deleted_files += 1,
            }
        }
        statistics
    }

    /// Build a local management snapshot for a VulcanCode page without storage I/O.
    /// 构造供 VulcanCode 页面使用的本地管理快照，不执行存储 I/O。
    pub fn local_management_snapshot(
        &self,
        ai: &dyn VceAiPort,
    ) -> Result<VceManagementSnapshot, VceError> {
        let capability = self.validate_ai_binding(ai, None)?;
        Ok(VceManagementSnapshot {
            engine: self.engine_snapshot(),
            workspace: self.workspace_statistics(),
            query_statistics: self.statistics,
            ai: capability,
            index: None,
        })
    }

    /// Query host storage for index counts and integrity, then combine it with local state.
    /// 查询宿主存储的索引统计和完整性，并与本地状态组合。
    pub async fn management_snapshot(
        &self,
        request: VceManagementRequest,
        index: &dyn VceIndexPort,
        ai: &dyn VceAiPort,
    ) -> Result<VceManagementSnapshot, VceError> {
        validate_request_id(&request.request_id)?;
        self.validate_index_binding(index.binding())?;
        let capability = self.validate_ai_binding(ai, None)?;
        let operation_id = request.request_id.clone();
        let index_snapshot = index.management_snapshot(request).await?;
        if index_snapshot.operation_id != operation_id {
            return Err(VceError::ResponseIdentity {
                operation: VcePortOperation::ManagementSnapshot,
                expected: operation_id,
                actual: index_snapshot.operation_id.clone(),
            });
        }
        validate_index_management_snapshot(&index_snapshot)?;
        Ok(VceManagementSnapshot {
            engine: self.engine_snapshot(),
            workspace: self.workspace_statistics(),
            query_statistics: self.statistics,
            ai: capability,
            index: Some(index_snapshot),
        })
    }

    /// Execute one typed read-only management query through the bound index namespace.
    /// 通过绑定索引命名空间执行一项类型化只读管理查询。
    pub async fn management_query(
        &self,
        request: VceManagementQueryRequest,
        index: &dyn VceIndexPort,
    ) -> Result<VceManagementQueryResponse, VceError> {
        validate_request_id(&request.operation_id)?;
        validate_management_query(&request.query).map_err(|error| {
            VceError::InvalidRequest(format!("management query failed validation: {error}"))
        })?;
        self.validate_index_binding(index.binding())?;
        let operation_id = request.operation_id.clone();
        let query = request.query.clone();
        let response = index.management_query(request).await?;
        if response.operation_id != operation_id {
            return Err(VceError::ResponseIdentity {
                operation: VcePortOperation::ManagementQuery,
                expected: operation_id,
                actual: response.operation_id,
            });
        }
        validate_management_response(&query, &response.result).map_err(|error| {
            VceError::InvalidHostResponse {
                operation: VcePortOperation::ManagementQuery,
                reason: error.to_string(),
            }
        })?;
        Ok(response)
    }

    /// Reconcile host observations, prepare changed Blobs, call shared Embeddings, and commit atomically.
    /// 协调宿主观察结果、准备变化 Blob、调用共享向量服务并原子提交。
    pub async fn refresh(
        &mut self,
        request: RefreshRequest,
        index: &dyn VceIndexPort,
        ai: &dyn VceAiPort,
        source: &dyn VceSourcePort,
    ) -> Result<RefreshResult, VceError> {
        self.statistics.refresh_attempts = self.statistics.refresh_attempts.saturating_add(1);
        self.state = VceEngineState::Refreshing;
        let result = self.refresh_inner(request, index, ai, source).await;
        match result {
            Ok(result) => {
                self.statistics.refresh_successes =
                    self.statistics.refresh_successes.saturating_add(1);
                self.state = VceEngineState::Ready;
                self.last_error = None;
                Ok(result)
            }
            Err(error) => {
                self.state = VceEngineState::Failed;
                self.last_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    /// Execute the refresh pipeline after the lifecycle wrapper has recorded its state.
    /// 在生命周期包装器记录状态后执行刷新流水线。
    async fn refresh_inner(
        &mut self,
        request: RefreshRequest,
        index: &dyn VceIndexPort,
        ai: &dyn VceAiPort,
        source: &dyn VceSourcePort,
    ) -> Result<RefreshResult, VceError> {
        validate_refresh_request(&request)?;
        self.validate_index_binding(index.binding())?;
        let capability = self.validate_ai_binding(ai, None)?;
        let request_id = request.request_id;
        let mode = request.mode;
        let reason = request.reason;
        let force_reindex = request.force_reindex;
        let source_revision = request.source_revision;
        let snapshot = match mode {
            RefreshMode::Full => self.workspace.reconcile(request.observations)?,
            RefreshMode::Paths => self
                .workspace
                .reconcile_paths(request.observations, request.removed_paths)?,
        };
        let sync_plan = self.workspace.sync_plan();
        // candidate_blobs widens only an explicit full rebuild to every present content identity.
        // candidate_blobs 仅在显式完整重建时扩展为全部现存内容身份。
        let candidate_blobs = if force_reindex {
            self.workspace
                .snapshot()
                .records
                .into_iter()
                .filter(|record| record.status == crate::model::FileStatus::Present)
                .filter_map(|record| record.blob_name)
                .collect::<BTreeSet<_>>()
        } else {
            sync_plan.added_blobs.clone()
        };
        let missing = if candidate_blobs.is_empty() || force_reindex {
            crate::ports::FindMissingResponse::default()
        } else {
            let operation_id = format!("{request_id}:find-missing");
            let response = index
                .find_missing(FindMissingRequest {
                    request_id: operation_id.clone(),
                    blob_names: candidate_blobs.clone(),
                })
                .await?;
            validate_missing_response(&operation_id, &candidate_blobs, &response)?;
            response
        };

        let blobs_to_prepare = if force_reindex {
            candidate_blobs
        } else {
            let mut missing_blobs = missing.unknown;
            missing_blobs.extend(missing.nonindexed);
            missing_blobs
        };
        let stage_context = RefreshStageContext {
            refresh_id: &request_id,
            generation: snapshot.generation,
            index,
            ai,
            capability: &capability,
            source,
        };
        let staged_blob_names = match self.stage_blobs(&blobs_to_prepare, stage_context).await {
            Ok(staged) => staged,
            Err(primary) => {
                if !blobs_to_prepare.is_empty()
                    && let Err(abort) = abort_staging(index, &request_id, snapshot.generation).await
                {
                    return Err(VceError::StageAbortFailed {
                        primary: primary.to_string(),
                        abort: abort.to_string(),
                    });
                }
                return Err(primary);
            }
        };

        let checkpoint_needed = force_reindex
            || self.workspace.checkpoint.is_none()
            || !sync_plan.added_blobs.is_empty()
            || !sync_plan.deleted_blobs.is_empty();
        let (checkpoint_version, commit_receipt, summary) = if checkpoint_needed {
            let expected_checkpoint_version = self
                .workspace
                .checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.version);
            let checkpoint = self.next_checkpoint()?;
            let checkpoint_version = checkpoint.version;
            let mut committed_workspace = self.workspace.clone();
            committed_workspace.commit_checkpoint(checkpoint.clone())?;
            committed_workspace.release_committed_filesystem_content();
            let summary = VceRefreshSummary {
                request_id: request_id.clone(),
                mode,
                reason,
                generation: snapshot.generation,
                added_blob_count: sync_plan.added_blobs.len(),
                deleted_blob_count: sync_plan.deleted_blobs.len(),
                indexed_blob_count: staged_blob_names.len(),
                skipped_path_count: sync_plan.skipped_paths.len(),
                checkpoint_version: Some(checkpoint_version),
                source_revision: source_revision.clone(),
            };
            let mut committed_statistics = self.statistics;
            committed_statistics.refresh_successes =
                committed_statistics.refresh_successes.saturating_add(1);
            // The host publishes this prospective successful state with the checkpoint. The live
            // engine adopts the same workspace only after the exact commit receipt is validated.
            // 宿主将这份预期成功状态与检查点一同发布；只有精确校验提交回执后，活动引擎才采用同一工作区。
            let state_snapshot = VceStateSnapshot::new(
                self.state_identity(),
                committed_workspace.snapshot(),
                committed_statistics,
                Some(summary.clone()),
                self.last_retrieval.clone(),
                None,
            );
            let commit = IndexCommit {
                request_id: format!("{request_id}:commit"),
                generation: snapshot.generation,
                refresh_id: request_id.clone(),
                source_revision: source_revision.clone(),
                deleted_blobs: sync_plan.deleted_blobs.clone(),
                staged_blob_names,
                expected_checkpoint_version,
                checkpoint: checkpoint.clone(),
                state_snapshot,
            };
            let publication = match index.commit(commit.clone()).await {
                Ok(receipt) => validate_commit_receipt(&commit, &receipt).map(|()| receipt),
                Err(error) => Err(VceError::Port(error)),
            };
            let receipt = match publication {
                Ok(receipt) => receipt,
                Err(primary) => {
                    if !commit.staged_blob_names.is_empty()
                        && let Err(abort) =
                            abort_staging(index, &request_id, snapshot.generation).await
                    {
                        return Err(VceError::StageAbortFailed {
                            primary: primary.to_string(),
                            abort: abort.to_string(),
                        });
                    }
                    return Err(primary);
                }
            };
            self.workspace = committed_workspace;
            (Some(checkpoint_version), Some(receipt), summary)
        } else {
            self.workspace.release_committed_filesystem_content();
            let checkpoint_version = self
                .workspace
                .checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.version);
            let summary = VceRefreshSummary {
                request_id: request_id.clone(),
                mode,
                reason,
                generation: snapshot.generation,
                added_blob_count: sync_plan.added_blobs.len(),
                deleted_blob_count: sync_plan.deleted_blobs.len(),
                indexed_blob_count: 0,
                skipped_path_count: sync_plan.skipped_paths.len(),
                checkpoint_version,
                source_revision: source_revision.clone(),
            };
            (checkpoint_version, None, summary)
        };
        self.last_refresh = Some(summary);
        Ok(RefreshResult {
            request_id,
            generation: snapshot.generation,
            sync_plan: sync_plan.clone(),
            indexed_blobs: commit_receipt
                .as_ref()
                .map_or_else(Vec::new, |receipt| receipt.indexed_blob_names.clone()),
            deleted_blobs: sync_plan.deleted_blobs.clone(),
            skipped_paths: sync_plan.skipped_paths.clone(),
            checkpoint_version,
            commit_receipt,
        })
    }

    /// Retrieve semantic code context through dense, path, exact, fusion, Rerank, and coverage stages.
    /// 通过内容向量、路径、精确、融合、重排和覆盖选择阶段检索代码上下文。
    pub async fn retrieve(
        &mut self,
        request: RetrieveRequest,
        index: &dyn VceIndexPort,
        ai: &dyn VceAiPort,
    ) -> Result<RetrieveResult, VceError> {
        self.statistics.retrieval_attempts = self.statistics.retrieval_attempts.saturating_add(1);
        self.state = VceEngineState::Retrieving;
        let result = self.retrieve_inner(request, index, ai).await;
        match result {
            Ok(result) => {
                self.statistics.retrieval_successes =
                    self.statistics.retrieval_successes.saturating_add(1);
                self.statistics.dense_hits = self
                    .statistics
                    .dense_hits
                    .saturating_add(result.summary.dense_hit_count as u64);
                self.statistics.path_hits = self
                    .statistics
                    .path_hits
                    .saturating_add(result.summary.path_hit_count as u64);
                self.statistics.exact_hits = self
                    .statistics
                    .exact_hits
                    .saturating_add(result.summary.exact_hit_count as u64);
                self.statistics.selected_hits = self
                    .statistics
                    .selected_hits
                    .saturating_add(result.summary.selected_hit_count as u64);
                self.state = VceEngineState::Ready;
                self.last_error = None;
                self.last_retrieval = Some(result.summary.clone());
                Ok(result)
            }
            Err(error) => {
                self.state = VceEngineState::Failed;
                self.last_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    /// Execute the retrieval pipeline after the lifecycle wrapper has recorded its state.
    /// 在生命周期包装器记录状态后执行检索流水线。
    async fn retrieve_inner(
        &self,
        request: RetrieveRequest,
        index: &dyn VceIndexPort,
        ai: &dyn VceAiPort,
    ) -> Result<RetrieveResult, VceError> {
        validate_retrieve_request(&request)?;
        self.validate_index_binding(index.binding())?;
        let scope = self.resolve_scope(&request.scope)?;
        let capability = self.validate_ai_binding(ai, Some(VceAiPurpose::CodeQueryEmbedding))?;
        let mut query_plan = self.query_planner.plan(&request.information_request)?;
        let mut degraded_features = Vec::new();
        let mut degradations = Vec::new();

        if query_plan.enable_query_rewrite
            && request.allow_query_rewrite
            && self.query_planner.config.max_queries > 1
        {
            if capability.query_rewrite_available {
                let operation_id = format!("{}:query-rewrite", request.request_id);
                let max_rewrites = self.query_planner.config.max_queries.saturating_sub(1);
                let response = ai
                    .rewrite_query(VceQueryRewriteRequest {
                        context: VceAiRequestContext::new(
                            operation_id.clone(),
                            VceAiPurpose::CodeQueryRewrite,
                            ai.binding(),
                        ),
                        query: query_plan.original_query.clone(),
                        max_rewrites,
                    })
                    .await;
                match response {
                    Ok(response) => {
                        match validate_rewrite_response(&operation_id, max_rewrites, &response) {
                            Ok(()) => {
                                query_plan.queries = self.query_planner.merge_rewrites(
                                    &query_plan.original_query,
                                    response.rewrites,
                                )?;
                            }
                            Err(error) => record_degradation(
                                &mut degraded_features,
                                &mut degradations,
                                VceDegradedFeature::QueryRewrite,
                                operation_id,
                                error.to_string(),
                            ),
                        }
                    }
                    Err(error) => record_degradation(
                        &mut degraded_features,
                        &mut degradations,
                        VceDegradedFeature::QueryRewrite,
                        operation_id,
                        error.to_string(),
                    ),
                }
            } else {
                record_degradation(
                    &mut degraded_features,
                    &mut degradations,
                    VceDegradedFeature::QueryRewrite,
                    format!("{}:query-rewrite", request.request_id),
                    "registered query rewrite capability is unavailable",
                );
            }
        }

        let mut ranked_lists = Vec::new();
        let mut path_hits = Vec::new();
        let mut path_fallback_hits = Vec::new();
        let mut dense_hit_count = 0usize;
        let mut path_hit_count = 0usize;
        let query_requests =
            query_plan
                .queries
                .iter()
                .cloned()
                .enumerate()
                .map(|(query_index, query)| {
                    self.execute_query_variant(
                        QueryVariantRequest {
                            request_id: request.request_id.clone(),
                            query_index,
                            query,
                            enable_path_index: query_plan.enable_path_index,
                            scope: scope.clone(),
                        },
                        index,
                        ai,
                        &capability,
                    )
                });
        let mut query_results = stream::iter(query_requests)
            .buffer_unordered(self.config.query_max_concurrency)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        query_results.sort_by_key(|result| result.query_index);
        for result in query_results {
            dense_hit_count = dense_hit_count.saturating_add(result.dense_hits.len());
            path_hit_count = path_hit_count.saturating_add(result.path_hits.len());
            ranked_lists.push(RankedHits {
                weight: if result.query_index == 0 {
                    self.retrieval.rrf.original_query_weight
                } else {
                    self.retrieval.rrf.facet_query_weight
                },
                hits: result.dense_hits,
            });
            path_hits.extend(result.path_hits);
            path_fallback_hits.extend(result.path_fallback_hits);
            for degradation in result.degradations {
                push_degraded(&mut degraded_features, degradation.feature);
                degradations.push(degradation);
            }
        }

        let mut exact_hits = Vec::new();
        if !query_plan.identifiers.is_empty() && scope.len() <= self.config.exact_scope_limit {
            let operation_id = format!("{}:exact-search", request.request_id);
            match index
                .search_exact(ExactSearchRequest {
                    request_id: operation_id.clone(),
                    identifiers: query_plan.identifiers.clone(),
                    allowed_blobs: scope.clone(),
                    limit: self.config.exact_candidate_limit,
                })
                .await
            {
                Ok(exact_response) => match validate_search_response(
                    VcePortOperation::ExactSearch,
                    &operation_id,
                    exact_response,
                    SearchSource::Exact,
                    &scope,
                    self.config.exact_candidate_limit,
                    Some(&query_plan.identifiers),
                ) {
                    Ok(hits) => exact_hits = hits,
                    Err(error) => record_degradation(
                        &mut degraded_features,
                        &mut degradations,
                        VceDegradedFeature::ExactSearch,
                        operation_id,
                        error.to_string(),
                    ),
                },
                Err(error) => record_degradation(
                    &mut degraded_features,
                    &mut degradations,
                    VceDegradedFeature::ExactSearch,
                    operation_id,
                    error.to_string(),
                ),
            }
        } else if !query_plan.identifiers.is_empty() {
            record_degradation(
                &mut degraded_features,
                &mut degradations,
                VceDegradedFeature::ExactSearch,
                format!("{}:exact-search", request.request_id),
                format!(
                    "exact search scope {} exceeds configured limit {}",
                    scope.len(),
                    self.config.exact_scope_limit
                ),
            );
        }

        let exact_hit_count = exact_hits.len();
        let semantic_hits = reciprocal_rank_fusion(&ranked_lists, self.retrieval.rrf.rrf_k);
        let mut fused_candidates = merge_semantic_and_exact(
            semantic_hits,
            exact_hits,
            query_plan.intent,
            self.config.fusion_candidate_limit,
        );
        fused_candidates.truncate(self.config.fusion_candidate_limit);
        let mut prepared_candidates = self.retrieval.prepare_candidates(
            fused_candidates,
            &path_hits,
            &path_fallback_hits,
            query_plan.intent,
        );
        prepared_candidates.truncate(self.config.fusion_candidate_limit);
        let candidate_count = prepared_candidates.len();
        let rerank_input = prepared_candidates
            .iter()
            .take(self.config.rerank_candidate_limit)
            .cloned()
            .collect::<Vec<_>>();
        let mut rerank_order = None;
        if query_plan.enable_llm_rerank && request.allow_rerank && !rerank_input.is_empty() {
            if capability.rerank_available {
                let operation_id = format!("{}:rerank", request.request_id);
                let rerank_input_count = rerank_input.len();
                let response = ai
                    .rerank(VceRerankRequest {
                        context: VceAiRequestContext::new(
                            operation_id.clone(),
                            VceAiPurpose::CodeCandidateRerank,
                            ai.binding(),
                        ),
                        query: query_plan.original_query.clone(),
                        candidates: rerank_input,
                        top_n: rerank_input_count,
                    })
                    .await;
                match response {
                    Ok(response) => {
                        match response.validate_for(&operation_id, rerank_input_count) {
                            Ok(()) => rerank_order = Some(response.ordered_candidate_indexes),
                            Err(error) => record_degradation(
                                &mut degraded_features,
                                &mut degradations,
                                VceDegradedFeature::Rerank,
                                operation_id,
                                error.to_string(),
                            ),
                        }
                    }
                    Err(error) => record_degradation(
                        &mut degraded_features,
                        &mut degradations,
                        VceDegradedFeature::Rerank,
                        operation_id,
                        error.to_string(),
                    ),
                }
            } else {
                record_degradation(
                    &mut degraded_features,
                    &mut degradations,
                    VceDegradedFeature::Rerank,
                    format!("{}:rerank", request.request_id),
                    "registered Rerank capability is unavailable",
                );
            }
        }

        let reranker = rerank_order
            .map(|order| Box::new(move |_: &[SearchHit]| order.clone()) as Box<RerankCallback>);
        let reranker_ref: Option<&RerankCallback> =
            reranker.as_ref().map(|callback| callback.as_ref());
        let hits = self.retrieval.finalize_prepared_candidates(
            prepared_candidates,
            query_plan.intent,
            &query_plan.identifiers,
            query_plan.promote_endpoint_definitions,
            reranker_ref,
        );
        let summary = VceRetrievalSummary {
            request_id: request.request_id.clone(),
            intent: query_plan.intent,
            query_count: query_plan.queries.len(),
            dense_hit_count,
            path_hit_count,
            exact_hit_count,
            candidate_count,
            selected_hit_count: hits.len(),
            degraded_features: degraded_features.clone(),
            degradations: degradations.clone(),
        };
        Ok(RetrieveResult {
            request_id: request.request_id,
            query_plan,
            scope,
            formatted_context: crate::retrieval::render_context(&hits),
            hits,
            degraded_features,
            degradations,
            summary,
        })
    }

    /// Execute one query variant independently while treating the path channel as optional.
    /// 独立执行一个查询变体，同时将路径通道视为可选能力。
    async fn execute_query_variant(
        &self,
        request: QueryVariantRequest,
        index: &dyn VceIndexPort,
        ai: &dyn VceAiPort,
        capability: &VceAiCapabilitySnapshot,
    ) -> Result<QueryVariantResult, VceError> {
        let request_id = request.request_id;
        let query_index = request.query_index;
        let query = request.query;
        let scope = request.scope;
        let content_vector = self
            .query_vector(
                &query,
                &request_id,
                query_index,
                VceAiPurpose::CodeQueryEmbedding,
                ai,
                capability,
            )
            .await?;
        let dense_operation_id = format!("{request_id}:dense-search:{query_index}");
        let dense_response = index
            .search_dense(DenseSearchRequest {
                request_id: dense_operation_id.clone(),
                query_index,
                query_vector: content_vector,
                allowed_blobs: scope.clone(),
                limit: self.config.dense_candidate_limit,
            })
            .await?;
        let dense_hits = validate_search_response(
            VcePortOperation::DenseSearch,
            &dense_operation_id,
            dense_response,
            SearchSource::Dense,
            &scope,
            self.config.dense_candidate_limit,
            None,
        )?;
        let mut result = QueryVariantResult {
            query_index,
            dense_hits,
            path_hits: Vec::new(),
            path_fallback_hits: Vec::new(),
            degradations: Vec::new(),
        };
        if !request.enable_path_index {
            return Ok(result);
        }

        let path_vector = match self
            .query_vector(
                &query,
                &request_id,
                query_index,
                VceAiPurpose::CodePathQueryEmbedding,
                ai,
                capability,
            )
            .await
        {
            Ok(vector) => vector,
            Err(error) => {
                result.degradations.push(VceDegradation {
                    feature: VceDegradedFeature::PathSearch,
                    operation_id: format!(
                        "{request_id}:query:{query_index}:{}",
                        VceAiPurpose::CodePathQueryEmbedding.as_str()
                    ),
                    reason: error.to_string(),
                });
                return Ok(result);
            }
        };

        let path_operation_id = format!("{request_id}:path-search:{query_index}");
        let response = index
            .search_path(PathSearchRequest {
                request_id: path_operation_id.clone(),
                query_index,
                query_vector: path_vector,
                allowed_blobs: scope.clone(),
                limit: self.config.path_candidate_limit,
            })
            .await;
        match response {
            Ok(response) => match validate_path_response(
                &path_operation_id,
                response,
                &scope,
                self.config.path_candidate_limit,
            ) {
                Ok((path_hits, path_fallback_hits)) => {
                    result.path_hits = path_hits;
                    result.path_fallback_hits = path_fallback_hits;
                }
                Err(error) => result.degradations.push(VceDegradation {
                    feature: VceDegradedFeature::PathSearch,
                    operation_id: path_operation_id,
                    reason: error.to_string(),
                }),
            },
            Err(error) => result.degradations.push(VceDegradation {
                feature: VceDegradedFeature::PathSearch,
                operation_id: path_operation_id,
                reason: error.to_string(),
            }),
        }
        Ok(result)
    }

    /// Prepare and flush bounded Blob batches so initial indexing does not retain all vectors.
    /// 准备并刷新有界 Blob 批次，避免初次索引长期保留全部向量。
    async fn stage_blobs(
        &self,
        blob_names: &BTreeSet<String>,
        context: RefreshStageContext<'_>,
    ) -> Result<BTreeSet<String>, VceError> {
        let mut record_batches = Vec::<Vec<FileRecord>>::new();
        let mut current_batch = Vec::new();
        let mut current_bytes = 0_u64;
        for blob_name in blob_names {
            let record = self.record_for_blob(blob_name)?.clone();
            // Over-limit records publish metadata only and therefore consume no source-read or
            // Embedding staging budget. They still consume one Blob slot in the bounded batch.
            // 超限记录只发布元数据，因此不占用源码读取或向量暂存字节预算，但仍占用有界批次中的一个 Blob 槽位。
            let staged_source_bytes = if record.size > self.config.indexing.max_file_bytes {
                0
            } else {
                record.size
            };
            let next_bytes = current_bytes
                .checked_add(staged_source_bytes)
                .ok_or_else(|| {
                    VceError::InvalidRequest("staging source byte count overflowed".to_owned())
                })?;
            if !current_batch.is_empty()
                && (current_batch.len() >= self.config.index_stage_blob_limit
                    || next_bytes > self.config.index_stage_max_bytes)
            {
                record_batches.push(std::mem::take(&mut current_batch));
                current_bytes = 0;
            }
            current_bytes = current_bytes
                .checked_add(staged_source_bytes)
                .ok_or_else(|| {
                    VceError::InvalidRequest("staging source byte count overflowed".to_owned())
                })?;
            current_batch.push(record);
        }
        if !current_batch.is_empty() {
            record_batches.push(current_batch);
        }
        let mut staged = BTreeSet::new();
        for (batch_index, records) in record_batches.into_iter().enumerate() {
            let loaded = self
                .read_sources_for_records(context.refresh_id, batch_index, &records, context.source)
                .await?;
            let mut blobs = Vec::with_capacity(records.len());
            for record in &records {
                if record.size > self.config.indexing.max_file_bytes {
                    blobs.push(self.prepare_over_limit_blob(record, context.generation)?);
                    continue;
                }
                let content = record
                    .content
                    .as_deref()
                    .or_else(|| loaded.get(&record.path).map(String::as_str))
                    .ok_or_else(|| VceError::InvalidHostResponse {
                        operation: VcePortOperation::SourceRead,
                        reason: format!("source `{}` was not returned", record.path),
                    })?;
                let blob_name = record.blob_name.as_deref().ok_or_else(|| {
                    VceError::InvalidRequest(format!(
                        "present source `{}` has no Blob identity",
                        record.path
                    ))
                })?;
                let blob_request_id = format!("{}:blob:{blob_name}", context.refresh_id);
                blobs.push(
                    self.prepare_blob(
                        record,
                        content,
                        &blob_request_id,
                        context.generation,
                        context.ai,
                        context.capability,
                    )
                    .await?,
                );
            }
            let operation_id = format!("{}:stage:{batch_index}", context.refresh_id);
            let request = IndexStageBatch {
                operation_id: operation_id.clone(),
                refresh_id: context.refresh_id.to_owned(),
                generation: context.generation,
                blobs,
            };
            let receipt = context.index.stage(request.clone()).await?;
            validate_stage_receipt(&request, &receipt)?;
            staged.extend(receipt.staged_blob_names);
        }
        Ok(staged)
    }

    /// Load only missing source text and verify it still matches the planned Blob identities.
    /// 仅加载缺失的源码文本，并校验其仍与计划中的 Blob 身份一致。
    async fn read_sources_for_records(
        &self,
        refresh_id: &str,
        batch_index: usize,
        records: &[FileRecord],
        source: &dyn VceSourcePort,
    ) -> Result<BTreeMap<String, String>, VceError> {
        let expectations = records
            .iter()
            .filter(|record| {
                record.content.is_none() && record.size <= self.config.indexing.max_file_bytes
            })
            .map(|record| {
                let blob_name = record.blob_name.clone().ok_or_else(|| {
                    VceError::InvalidRequest(format!(
                        "present source `{}` has no Blob identity",
                        record.path
                    ))
                })?;
                Ok(VceSourceExpectation {
                    path: record.path.clone(),
                    blob_name,
                    size: record.size,
                    mtime_ns: record.mtime_ns,
                })
            })
            .collect::<Result<Vec<_>, VceError>>()?;
        if expectations.is_empty() {
            return Ok(BTreeMap::new());
        }
        let operation_id = format!("{refresh_id}:source-read:{batch_index}");
        let response = source
            .read_sources(VceSourceReadRequest {
                operation_id: operation_id.clone(),
                expectations: expectations.clone(),
            })
            .await?;
        validate_source_response(&operation_id, &expectations, response)
    }

    /// Build a metadata-only terminal record for a source above semantic admission size.
    /// 为超过语义准入大小的源码构建仅含元数据的终态记录。
    fn prepare_over_limit_blob(
        &self,
        record: &FileRecord,
        generation: u64,
    ) -> Result<IndexedBlob, VceError> {
        let blob_name = record.blob_name.clone().ok_or_else(|| {
            VceError::InvalidRequest(format!(
                "present source `{}` has no Blob identity",
                record.path
            ))
        })?;
        if record.size <= self.config.indexing.max_file_bytes {
            return Err(VceError::InvalidRequest(format!(
                "source `{}` does not exceed the semantic admission size",
                record.path
            )));
        }
        Ok(IndexedBlob {
            blob_name,
            path: record.path.clone(),
            generation,
            structure_analyzer_id: None,
            structure_mode: None,
            structure_fallback_reason: None,
            chunks: Vec::new(),
            embedding_vectors: Vec::new(),
            path_document: None,
            path_embedding: None,
            symbols: Vec::new(),
            path_status: AuxiliaryIndexStatus::NotApplicable,
            symbol_status: AuxiliaryIndexStatus::NotApplicable,
            status: IndexedBlobStatus::ReadyWithoutEmbedding {
                reason: "file exceeds configured admission size".to_owned(),
            },
        })
    }

    async fn prepare_blob(
        &self,
        record: &FileRecord,
        content: &str,
        request_id: &str,
        generation: u64,
        ai: &dyn VceAiPort,
        capability: &VceAiCapabilitySnapshot,
    ) -> Result<IndexedBlob, VceError> {
        let expected_blob = record.blob_name.as_deref().ok_or_else(|| {
            VceError::InvalidRequest(format!(
                "present source `{}` has no Blob identity",
                record.path
            ))
        })?;
        let ingest = self.indexing.ingest(&record.path, content, generation)?;
        match ingest {
            IngestResult::ReadyWithoutEmbedding {
                blob_name,
                path,
                reason,
                generation,
            } => {
                validate_prepared_blob_identity(expected_blob, &blob_name, &record.path)?;
                Ok(IndexedBlob {
                    blob_name,
                    path,
                    generation,
                    structure_analyzer_id: None,
                    structure_mode: None,
                    structure_fallback_reason: None,
                    chunks: Vec::new(),
                    embedding_vectors: Vec::new(),
                    path_document: None,
                    path_embedding: None,
                    symbols: Vec::new(),
                    path_status: AuxiliaryIndexStatus::NotApplicable,
                    symbol_status: AuxiliaryIndexStatus::NotApplicable,
                    status: IndexedBlobStatus::ReadyWithoutEmbedding { reason },
                })
            }
            IngestResult::Pending(pending) => {
                validate_prepared_blob_identity(expected_blob, &pending.blob_name, &record.path)?;
                let analysis = self.structure_analyzer.analyze(
                    &pending.blob_name,
                    &pending.path,
                    &pending.content,
                )?;
                validate_structure_analysis(
                    &analysis,
                    &self.config.embedding_space.chunker_version,
                )?;
                let structure_analyzer_id = analysis.analyzer_id;
                let structure_mode = analysis.mode;
                let structure_fallback_reason = analysis.fallback_reason;
                let symbols = analysis.symbols;
                let prepared = self
                    .indexing
                    .prepare_with_chunks(pending, analysis.chunks)?;
                let path_document = prepared.path_document.clone();
                let content_segment_vectors = match prepared.embedding_plan.as_ref() {
                    Some(plan) => {
                        self.execute_embedding_plan(
                            plan,
                            request_id,
                            VceAiPurpose::CodeDocumentEmbedding,
                            ai,
                            capability,
                        )
                        .await?
                    }
                    None => Vec::new(),
                };
                let ready = self.indexing.complete(
                    prepared,
                    content_segment_vectors,
                    self.config.embedding_space.dimension,
                )?;
                let path_result: Result<Option<EmbeddingVector>, VceError> = match path_document {
                    Some(document) => {
                        async {
                            let input = EmbeddingInput::new(
                                document.path_id.clone(),
                                document.document.clone(),
                            )?;
                            let plan = EmbeddingPlan::new(
                                vec![input],
                                self.config.indexing.embedding_max_inputs,
                                self.config.indexing.embedding_max_chars,
                                self.config.indexing.embedding_max_segment_chars,
                                self.config.indexing.embedding_overlap_chars,
                            )?;
                            let segments = self
                                .execute_embedding_plan(
                                    &plan,
                                    request_id,
                                    VceAiPurpose::CodePathEmbedding,
                                    ai,
                                    capability,
                                )
                                .await?;
                            let mut vectors = plan.aggregate(segments)?;
                            vectors
                                .pop()
                                .ok_or_else(|| VceError::InvalidHostResponse {
                                    operation: VcePortOperation::Embedding,
                                    reason: "path Embedding returned no logical vector".to_owned(),
                                })
                                .map(Some)
                        }
                        .await
                    }
                    None => Ok(None),
                };
                let (path_embedding, path_status) = match path_result {
                    Ok(Some(vector)) => (Some(vector), AuxiliaryIndexStatus::Ready),
                    Ok(None) => (None, AuxiliaryIndexStatus::NotApplicable),
                    Err(error) => (
                        None,
                        AuxiliaryIndexStatus::Failed {
                            reason: error.to_string(),
                        },
                    ),
                };
                let symbol_status = if symbols.is_empty() {
                    AuxiliaryIndexStatus::NotApplicable
                } else {
                    AuxiliaryIndexStatus::Ready
                };
                Ok(IndexedBlob {
                    blob_name: ready.blob_name,
                    path: ready.path,
                    generation: ready.generation,
                    structure_analyzer_id: Some(structure_analyzer_id),
                    structure_mode: Some(structure_mode),
                    structure_fallback_reason,
                    chunks: ready.chunks,
                    embedding_vectors: ready.embedding_vectors,
                    path_document: ready.path_document,
                    path_embedding,
                    symbols,
                    path_status,
                    symbol_status,
                    status: IndexedBlobStatus::Ready,
                })
            }
        }
    }

    /// Execute all batches of one Embedding plan and preserve segment identities.
    /// 执行一个向量计划的全部批次并保持分段身份对齐。
    async fn execute_embedding_plan(
        &self,
        plan: &EmbeddingPlan,
        request_id: &str,
        purpose: VceAiPurpose,
        ai: &dyn VceAiPort,
        capability: &VceAiCapabilitySnapshot,
    ) -> Result<Vec<SegmentVector>, VceError> {
        if !capability.registered || !capability.embedding_available {
            return Err(VceError::AiCapabilityUnavailable { purpose });
        }
        let requests = plan
            .batches
            .iter()
            .cloned()
            .enumerate()
            .map(|(batch_index, batch)| {
                let operation_id = format!("{request_id}:{}:{batch_index}", purpose.as_str());
                async move {
                    let response = ai
                        .embed(VceEmbeddingRequest {
                            context: VceAiRequestContext::new(
                                operation_id.clone(),
                                purpose,
                                ai.binding(),
                            ),
                            batch: batch.clone(),
                        })
                        .await;
                    (batch_index, operation_id, batch, response)
                }
            });
        let mut responses = stream::iter(requests)
            .buffer_unordered(self.config.embedding_max_concurrency)
            .collect::<Vec<_>>()
            .await;
        responses.sort_by_key(|(batch_index, _, _, _)| *batch_index);

        let mut vectors = Vec::new();
        for (_, operation_id, batch, response) in responses {
            let response = response?;
            let response_vectors =
                validate_embedding_response(self, &operation_id, response, &batch)?;
            vectors.extend(response_vectors);
        }
        Ok(vectors)
    }

    /// Create and execute one logical query Embedding plan.
    /// 创建并执行一个逻辑查询向量计划。
    async fn query_vector(
        &self,
        query: &str,
        request_id: &str,
        query_index: usize,
        purpose: VceAiPurpose,
        ai: &dyn VceAiPort,
        capability: &VceAiCapabilitySnapshot,
    ) -> Result<EmbeddingVector, VceError> {
        let input_id = format!(
            "{request_id}:query-vector:{query_index}:{}",
            purpose.as_str()
        );
        let input = EmbeddingInput::new(input_id, query.to_owned())?;
        let plan = EmbeddingPlan::new(
            vec![input],
            self.config.indexing.embedding_max_inputs,
            self.config.indexing.embedding_max_chars,
            self.config.indexing.embedding_max_segment_chars,
            self.config.indexing.embedding_overlap_chars,
        )?;
        let embedding_request_id = format!("{request_id}:query:{query_index}");
        let segments = self
            .execute_embedding_plan(&plan, &embedding_request_id, purpose, ai, capability)
            .await?;
        let mut vectors = plan.aggregate(segments)?;
        vectors.pop().ok_or_else(|| VceError::InvalidHostResponse {
            operation: VcePortOperation::Embedding,
            reason: "query Embedding returned no logical vector".to_owned(),
        })
    }

    /// Return the exact workspace record owning one Blob identity.
    /// 返回拥有指定 Blob 身份的精确工作区记录。
    fn record_for_blob(&self, blob_name: &str) -> Result<&FileRecord, VceError> {
        self.workspace
            .records
            .values()
            .find(|record| record.blob_name.as_deref() == Some(blob_name))
            .ok_or_else(|| {
                VceError::InvalidRequest(format!(
                    "workspace has no present record for added Blob `{blob_name}`"
                ))
            })
    }

    /// Build the next checkpoint from the current present workspace records.
    /// 根据当前存在的工作区记录构造下一个检查点。
    fn next_checkpoint(&self) -> Result<Checkpoint, VceError> {
        let version = self
            .workspace
            .checkpoint
            .as_ref()
            .map_or(Some(1), |checkpoint| checkpoint.version.checked_add(1))
            .ok_or_else(|| {
                VceError::InvalidRequest("checkpoint version is exhausted".to_owned())
            })?;
        let members = self
            .workspace
            .records
            .values()
            .filter(|record| record.is_present())
            .filter_map(|record| record.blob_name.clone())
            .collect::<BTreeSet<_>>();
        Ok(Checkpoint::new(
            self.config.checkpoint_chain_id.clone(),
            version,
            members,
        ))
    }

    /// Resolve and validate an explicit retrieval scope.
    /// 解析并校验明确的检索范围。
    fn resolve_scope(&self, requested: &RetrievalScope) -> Result<BTreeSet<String>, VceError> {
        let scope = match requested {
            RetrievalScope::CurrentCheckpoint => match self.current_scope() {
                ScopeResolution::Members(members) => members,
                ScopeResolution::MissingCheckpoint => return Err(VceError::MissingCheckpoint),
                ScopeResolution::Empty => return Err(VceError::EmptyScope),
            },
            RetrievalScope::Blobs(members) => members.clone(),
        };
        if scope.is_empty() {
            return Err(VceError::EmptyScope);
        }
        if scope
            .iter()
            .any(|blob_name| !crate::identity::is_sha256_identity(blob_name))
        {
            return Err(VceError::InvalidRequest(
                "retrieval scope contains a noncanonical Blob identity".to_owned(),
            ));
        }
        Ok(scope)
    }

    /// Validate that the host AI port uses the configured shared vector space.
    /// 校验宿主 AI 端口使用配置的共享向量空间。
    fn validate_ai_binding(
        &self,
        ai: &dyn VceAiPort,
        required_embedding_purpose: Option<VceAiPurpose>,
    ) -> Result<VceAiCapabilitySnapshot, VceError> {
        let binding = ai.binding();
        for (field, actual) in [
            ("registration_id", binding.registration_id.as_str()),
            ("consumer_profile_id", binding.consumer_profile_id.as_str()),
            ("vector_space_id", binding.vector_space_id.as_str()),
        ] {
            if actual.trim().is_empty() {
                return Err(VceError::AiBindingMismatch {
                    field: field.to_owned(),
                    expected: "non-empty identifier".to_owned(),
                    actual: actual.to_owned(),
                });
            }
        }
        if binding.vector_space_id != self.config.embedding_space.vector_space_id {
            return Err(VceError::AiBindingMismatch {
                field: "vector_space_id".to_owned(),
                expected: self.config.embedding_space.vector_space_id.clone(),
                actual: binding.vector_space_id.clone(),
            });
        }
        let capability = ai.capability_snapshot();
        if capability.registration_id != binding.registration_id {
            return Err(VceError::AiBindingMismatch {
                field: "registration_id".to_owned(),
                expected: binding.registration_id.clone(),
                actual: capability.registration_id,
            });
        }
        if capability.consumer_profile_id != binding.consumer_profile_id {
            return Err(VceError::AiBindingMismatch {
                field: "consumer_profile_id".to_owned(),
                expected: binding.consumer_profile_id.clone(),
                actual: capability.consumer_profile_id,
            });
        }
        if capability.vector_space_id != binding.vector_space_id {
            return Err(VceError::AiBindingMismatch {
                field: "vector_space_id".to_owned(),
                expected: binding.vector_space_id.clone(),
                actual: capability.vector_space_id,
            });
        }
        if !capability.registered
            && (capability.embedding_available
                || capability.query_rewrite_available
                || capability.rerank_available)
        {
            return Err(VceError::AiBindingMismatch {
                field: "registered".to_owned(),
                expected: "available capabilities require an active registration".to_owned(),
                actual: "inactive registration exposes available capabilities".to_owned(),
            });
        }
        if let Some(purpose) = required_embedding_purpose
            && (!capability.registered || !capability.embedding_available)
        {
            return Err(VceError::AiCapabilityUnavailable { purpose });
        }
        Ok(capability)
    }

    /// Validate that storage is permanently bound to this code-semantic namespace.
    /// 校验存储已永久绑定到此代码语义命名空间。
    fn validate_index_binding(&self, binding: &VceIndexBinding) -> Result<(), VceError> {
        validate_index_binding_field(
            "namespace_id",
            &self.config.index_namespace_id,
            &binding.namespace_id,
        )?;
        validate_index_binding_field(
            "checkpoint_chain_id",
            &self.config.checkpoint_chain_id,
            &binding.checkpoint_chain_id,
        )?;
        if self.config.index_schema_version != binding.schema_version {
            return Err(VceError::IndexBindingMismatch {
                field: "schema_version".to_owned(),
                expected: self.config.index_schema_version.to_string(),
                actual: binding.schema_version.to_string(),
            });
        }
        if self.config.embedding_space != binding.embedding_space {
            return Err(VceError::IndexBindingMismatch {
                field: "embedding_space".to_owned(),
                expected: format!("{:?}", self.config.embedding_space),
                actual: format!("{:?}", binding.embedding_space),
            });
        }
        Ok(())
    }

    /// Build a local lifecycle snapshot for management callers.
    /// 为管理调用方构造本地生命周期快照。
    fn engine_snapshot(&self) -> VceEngineSnapshot {
        VceEngineSnapshot {
            state: self.state,
            workspace_generation: self.workspace.generation,
            checkpoint_version: self
                .workspace
                .checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.version),
            last_refresh: self.last_refresh.clone(),
            last_retrieval: self.last_retrieval.clone(),
            last_error: self.last_error.clone(),
        }
    }
}

/// Validate persisted namespace identity against the configuration selected by the host.
/// 根据宿主选择的配置校验持久化命名空间身份。
fn validate_state_identity(
    config: &VceConfig,
    identity: &VceStateIdentity,
) -> Result<(), VceConfigError> {
    let mismatch = |field: &str, expected: String, actual: String| {
        VceConfigError::StateSnapshot(format!(
            "identity field `{field}` mismatch: expected `{expected}`, got `{actual}`"
        ))
    };
    if identity.index_namespace_id != config.index_namespace_id {
        return Err(mismatch(
            "index_namespace_id",
            config.index_namespace_id.clone(),
            identity.index_namespace_id.clone(),
        ));
    }
    if identity.checkpoint_chain_id != config.checkpoint_chain_id {
        return Err(mismatch(
            "checkpoint_chain_id",
            config.checkpoint_chain_id.clone(),
            identity.checkpoint_chain_id.clone(),
        ));
    }
    if identity.index_schema_version != config.index_schema_version {
        return Err(mismatch(
            "index_schema_version",
            config.index_schema_version.to_string(),
            identity.index_schema_version.to_string(),
        ));
    }
    let expected = &config.embedding_space;
    let actual = &identity.embedding_space;
    if actual.vector_space_id != expected.vector_space_id {
        return Err(mismatch(
            "embedding_space.vector_space_id",
            expected.vector_space_id.clone(),
            actual.vector_space_id.clone(),
        ));
    }
    if actual.physical_fingerprint != expected.physical_fingerprint {
        return Err(mismatch(
            "embedding_space.physical_fingerprint",
            format!("{:?}", expected.physical_fingerprint),
            format!("{:?}", actual.physical_fingerprint),
        ));
    }
    if actual.dimension != expected.dimension {
        return Err(mismatch(
            "embedding_space.dimension",
            expected.dimension.to_string(),
            actual.dimension.to_string(),
        ));
    }
    if actual.preprocessing_version != expected.preprocessing_version {
        return Err(mismatch(
            "embedding_space.preprocessing_version",
            expected.preprocessing_version.clone(),
            actual.preprocessing_version.clone(),
        ));
    }
    if actual.chunker_version != expected.chunker_version {
        return Err(mismatch(
            "embedding_space.chunker_version",
            expected.chunker_version.clone(),
            actual.chunker_version.clone(),
        ));
    }
    Ok(())
}

/// Validate the stable identity of a host-facing functional request.
/// 校验宿主功能请求的稳定身份标识。
fn validate_request_id(request_id: &str) -> Result<(), VceError> {
    if request_id.trim().is_empty() {
        Err(VceError::InvalidRequest(
            "request_id must not be empty".to_owned(),
        ))
    } else {
        Ok(())
    }
}

/// Ensure freshly prepared source content still represents its reconciled Blob identity.
/// 确保刚准备的源码内容仍代表其协调阶段确定的 Blob 身份。
fn validate_prepared_blob_identity(
    expected: &str,
    actual: &str,
    path: &str,
) -> Result<(), VceError> {
    if expected == actual {
        return Ok(());
    }
    Err(VceError::InvalidHostResponse {
        operation: VcePortOperation::SourceRead,
        reason: format!("source content changed after reconciliation for `{path}`"),
    })
}

/// Validate aggregate host index data before projecting it into a management page.
/// 在将宿主索引聚合数据投影到管理页面前校验它。
fn validate_index_management_snapshot(
    snapshot: &VceIndexManagementSnapshot,
) -> Result<(), VceError> {
    let invalid = |reason: String| VceError::InvalidHostResponse {
        operation: VcePortOperation::ManagementSnapshot,
        reason,
    };
    let counts = snapshot.counts;
    let classified_files = counts
        .searchable_files
        .checked_add(counts.skipped_files)
        .and_then(|value| value.checked_add(counts.deleted_files))
        .ok_or_else(|| invalid("index file counters overflow".to_owned()))?;
    if classified_files > counts.files || counts.blobs > counts.files {
        return Err(invalid(
            "index file and Blob counters violate aggregate bounds".to_owned(),
        ));
    }
    let integrity = &snapshot.integrity;
    if integrity.missing_blob_count > integrity.checkpoint_blob_count
        || integrity.orphan_blob_count > integrity.indexed_blob_count
        || integrity.checkpoint_blob_count - integrity.missing_blob_count
            != integrity.indexed_blob_count - integrity.orphan_blob_count
        || (!integrity.index_exists && integrity.indexed_blob_count > 0)
        || (integrity.searchable && (!integrity.index_exists || integrity.missing_blob_count != 0))
        || snapshot.checkpoint_version.is_some_and(|version| {
            version == 0 || snapshot.generation == 0 || version > snapshot.generation
        })
        || (snapshot.status == VceIndexStoreStatus::Error
            && snapshot
                .last_error
                .as_ref()
                .is_none_or(|error| error.trim().is_empty()))
        || integrity
            .detail
            .as_ref()
            .is_some_and(|detail| detail.trim().is_empty())
        || snapshot
            .last_error
            .as_ref()
            .is_some_and(|error| error.trim().is_empty())
    {
        return Err(invalid(
            "index integrity or diagnostic fields are inconsistent".to_owned(),
        ));
    }
    if let Some(job) = &snapshot.active_job {
        let is_refresh = matches!(
            job.kind,
            crate::management::VceOperationKind::InitialBuild
                | crate::management::VceOperationKind::IncrementalRefresh
        );
        if job.job_id.trim().is_empty()
            || job.attempt == 0
            || job.completed_units > job.total_units
            || (job.phase == VceOperationPhase::Completed
                && (job.completed_units != job.total_units || job.error.is_some()))
            || is_refresh != job.mode.is_some()
            || is_refresh != job.reason.is_some()
            || job.unit.as_ref().is_some_and(|unit| unit.trim().is_empty())
            || (job.phase == VceOperationPhase::Failed
                && job
                    .error
                    .as_ref()
                    .is_none_or(|error| error.trim().is_empty()))
            || job
                .error
                .as_ref()
                .is_some_and(|error| error.trim().is_empty())
            || matches!(
                (job.started_at_ns, job.finished_at_ns),
                (Some(started), Some(finished)) if finished < started
            )
        {
            return Err(invalid(format!(
                "index job `{}` violates the management contract",
                job.job_id
            )));
        }
    }
    Ok(())
}

/// Compare one immutable index binding field with a deterministic error.
/// 比较一个不可变索引绑定字段，并在不匹配时返回确定性错误。
fn validate_index_binding_field(field: &str, expected: &str, actual: &str) -> Result<(), VceError> {
    if expected == actual {
        return Ok(());
    }
    Err(VceError::IndexBindingMismatch {
        field: field.to_owned(),
        expected: expected.to_owned(),
        actual: actual.to_owned(),
    })
}

/// Record one optional retrieval stage only once in a result projection.
/// 在结果投影中只记录一次某个可选检索阶段。
fn push_degraded(features: &mut Vec<VceDegradedFeature>, feature: VceDegradedFeature) {
    if !features.contains(&feature) {
        features.push(feature);
    }
}

/// Retain one optional-stage failure with both a compact feature set and detailed evidence.
/// 同时以紧凑能力集合和详细证据保留一次可选阶段失败。
fn record_degradation(
    features: &mut Vec<VceDegradedFeature>,
    degradations: &mut Vec<VceDegradation>,
    feature: VceDegradedFeature,
    operation_id: impl Into<String>,
    reason: impl Into<String>,
) {
    push_degraded(features, feature);
    degradations.push(VceDegradation {
        feature,
        operation_id: operation_id.into(),
        reason: reason.into(),
    });
}

/// Derive the management lifecycle state from checkpoint and pending Blob changes.
/// 根据检查点和待发布 Blob 变化推导管理生命周期状态。
fn workspace_state(workspace: &WorkspaceState) -> VceEngineState {
    if workspace.checkpoint.is_none() {
        VceEngineState::Cold
    } else {
        let sync_plan = workspace.sync_plan();
        if sync_plan.added_blobs.is_empty() && sync_plan.deleted_blobs.is_empty() {
            VceEngineState::Ready
        } else {
            VceEngineState::PendingRefresh
        }
    }
}

/// Validate refresh path ownership before mutating workspace state.
/// 在修改工作区状态前校验刷新路径的唯一归属。
fn validate_refresh_request(request: &RefreshRequest) -> Result<(), VceError> {
    validate_request_id(&request.request_id)?;
    if request
        .source_revision
        .as_ref()
        .is_some_and(|revision| revision.trim().is_empty())
    {
        return Err(VceError::InvalidRequest(
            "source_revision must not be empty when provided".to_owned(),
        ));
    }
    if request.mode == RefreshMode::Full && !request.removed_paths.is_empty() {
        return Err(VceError::InvalidRequest(
            "full refresh must represent deletions by omitting paths, not removed_paths".to_owned(),
        ));
    }
    if request.force_reindex && request.mode != RefreshMode::Full {
        return Err(VceError::InvalidRequest(
            "force_reindex requires a full refresh".to_owned(),
        ));
    }
    let mut paths = BTreeSet::new();
    for observation in &request.observations {
        let path = crate::identity::normalize_relative_path(&observation.path)
            .map_err(|error| VceError::InvalidRequest(error.to_string()))?;
        if !paths.insert(path.clone()) {
            return Err(VceError::InvalidRequest(format!(
                "refresh mentions path more than once: {path}"
            )));
        }
    }
    for removed_path in &request.removed_paths {
        let path = crate::identity::normalize_relative_path(removed_path)
            .map_err(|error| VceError::InvalidRequest(error.to_string()))?;
        if !paths.insert(path.clone()) {
            return Err(VceError::InvalidRequest(format!(
                "refresh mentions path more than once: {path}"
            )));
        }
    }
    Ok(())
}

/// Validate retrieval request identity and information text.
/// 校验检索请求身份和信息文本。
fn validate_retrieve_request(request: &RetrieveRequest) -> Result<(), VceError> {
    validate_request_id(&request.request_id)?;
    if request.information_request.trim().is_empty() {
        return Err(VceError::InvalidRequest(
            "information_request must not be empty".to_owned(),
        ));
    }
    Ok(())
}

/// Validate missing-state classification against the exact requested Blob set.
/// 针对精确请求的 Blob 集合校验缺失状态分类。
fn validate_missing_response(
    operation_id: &str,
    requested: &BTreeSet<String>,
    response: &crate::ports::FindMissingResponse,
) -> Result<(), VceError> {
    if response.operation_id != operation_id {
        return Err(VceError::ResponseIdentity {
            operation: VcePortOperation::FindMissing,
            expected: operation_id.to_owned(),
            actual: response.operation_id.clone(),
        });
    }
    if !response.unknown.is_subset(requested) || !response.nonindexed.is_subset(requested) {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::FindMissing,
            reason: "find-missing response contains a Blob outside the requested set".to_owned(),
        });
    }
    if response
        .unknown
        .intersection(&response.nonindexed)
        .next()
        .is_some()
    {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::FindMissing,
            reason: "find-missing response classifies one Blob as both unknown and nonindexed"
                .to_owned(),
        });
    }
    Ok(())
}

/// Validate one bounded immutable source response and return content by canonical path.
/// 校验一份有界不可变源码响应，并按规范路径返回源码内容。
fn validate_source_response(
    operation_id: &str,
    expectations: &[VceSourceExpectation],
    response: VceSourceReadResponse,
) -> Result<BTreeMap<String, String>, VceError> {
    if response.operation_id != operation_id {
        return Err(VceError::ResponseIdentity {
            operation: VcePortOperation::SourceRead,
            expected: operation_id.to_owned(),
            actual: response.operation_id,
        });
    }
    let expected = expectations
        .iter()
        .map(|expectation| (expectation.path.clone(), expectation))
        .collect::<BTreeMap<_, _>>();
    if expected.len() != expectations.len() || response.documents.len() != expectations.len() {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::SourceRead,
            reason: "source response does not exactly cover the requested paths".to_owned(),
        });
    }
    let mut documents = BTreeMap::new();
    for document in response.documents {
        let path = crate::identity::normalize_relative_path(&document.path).map_err(|error| {
            VceError::InvalidHostResponse {
                operation: VcePortOperation::SourceRead,
                reason: format!("source path `{}` is invalid: {error}", document.path),
            }
        })?;
        if path != document.path || documents.contains_key(&path) {
            return Err(VceError::InvalidHostResponse {
                operation: VcePortOperation::SourceRead,
                reason: format!(
                    "source path `{}` is noncanonical or duplicated",
                    document.path
                ),
            });
        }
        let expectation = expected
            .get(&path)
            .ok_or_else(|| VceError::InvalidHostResponse {
                operation: VcePortOperation::SourceRead,
                reason: format!("source response returned unrequested path `{path}`"),
            })?;
        let actual_size = document.content.len() as u64;
        if document.size != actual_size
            || document.size != expectation.size
            || document.mtime_ns != expectation.mtime_ns
        {
            return Err(VceError::InvalidHostResponse {
                operation: VcePortOperation::SourceRead,
                reason: format!("source metadata changed while reading `{path}`"),
            });
        }
        let actual_blob = crate::identity::blob_name(&path, &document.content)
            .map_err(WorkspaceError::InvalidPath)?;
        if actual_blob != expectation.blob_name {
            return Err(VceError::InvalidHostResponse {
                operation: VcePortOperation::SourceRead,
                reason: format!("source content changed while reading `{path}`"),
            });
        }
        documents.insert(path, document.content);
    }
    Ok(documents)
}

/// Validate an idempotent staging acknowledgement before releasing prepared vectors.
/// 在释放已准备向量前校验幂等暂存确认。
fn validate_stage_receipt(
    request: &IndexStageBatch,
    receipt: &IndexStageReceipt,
) -> Result<(), VceError> {
    if receipt.operation_id != request.operation_id {
        return Err(VceError::ResponseIdentity {
            operation: VcePortOperation::Stage,
            expected: request.operation_id.clone(),
            actual: receipt.operation_id.clone(),
        });
    }
    if receipt.refresh_id != request.refresh_id || receipt.generation != request.generation {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::Stage,
            reason: "staging receipt refresh identity or generation does not match".to_owned(),
        });
    }
    let expected = request
        .blobs
        .iter()
        .map(|blob| blob.blob_name.clone())
        .collect::<BTreeSet<_>>();
    let actual = receipt
        .staged_blob_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if expected.len() != request.blobs.len()
        || actual.len() != receipt.staged_blob_names.len()
        || actual != expected
    {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::Stage,
            reason: "staging receipt Blob identities do not exactly match the batch".to_owned(),
        });
    }
    Ok(())
}

/// Validate an atomic publication receipt before advancing the local checkpoint.
/// 在推进本地检查点前校验原子发布回执。
fn validate_commit_receipt(
    request: &IndexCommit,
    receipt: &crate::ports::CommitReceipt,
) -> Result<(), VceError> {
    if receipt.operation_id != request.request_id {
        return Err(VceError::ResponseIdentity {
            operation: VcePortOperation::Commit,
            expected: request.request_id.clone(),
            actual: receipt.operation_id.clone(),
        });
    }
    let indexed = receipt
        .indexed_blob_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let deleted = receipt
        .deleted_blob_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if receipt.generation != request.generation
        || receipt.refresh_id != request.refresh_id
        || receipt.checkpoint_chain_id != request.checkpoint.chain_id
        || receipt.checkpoint_version != request.checkpoint.version
        || receipt.indexed_count != request.staged_blob_names.len()
        || receipt.deleted_count != request.deleted_blobs.len()
        || indexed.len() != receipt.indexed_blob_names.len()
        || deleted.len() != receipt.deleted_blob_names.len()
        || indexed != request.staged_blob_names
        || deleted != request.deleted_blobs
    {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::Commit,
            reason: "publication receipt does not exactly match the staged commit".to_owned(),
        });
    }
    Ok(())
}

/// Abort one unpublished refresh staging transaction and validate its acknowledgement.
/// 中止一次尚未发布的刷新暂存事务并校验其确认回执。
async fn abort_staging(
    index: &dyn VceIndexPort,
    refresh_id: &str,
    generation: u64,
) -> Result<(), VceError> {
    let operation_id = format!("{refresh_id}:abort-stage");
    let result = index
        .abort_stage(IndexAbortRequest {
            operation_id: operation_id.clone(),
            refresh_id: refresh_id.to_owned(),
            generation,
        })
        .await;
    validate_abort_result(&operation_id, refresh_id, generation, result)
}

/// Validate cleanup acknowledgement while preserving the primary refresh failure.
/// 校验清理确认，同时保留主刷新失败。
fn validate_abort_result(
    operation_id: &str,
    refresh_id: &str,
    generation: u64,
    result: Result<crate::ports::IndexAbortReceipt, VcePortError>,
) -> Result<(), VceError> {
    let receipt = result?;
    if receipt.operation_id != operation_id {
        return Err(VceError::ResponseIdentity {
            operation: VcePortOperation::AbortStage,
            expected: operation_id.to_owned(),
            actual: receipt.operation_id,
        });
    }
    if receipt.refresh_id != refresh_id || receipt.generation != generation {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::AbortStage,
            reason: "staging abort receipt refresh or generation identity does not match"
                .to_owned(),
        });
    }
    Ok(())
}

/// Validate a rewrite response identity before accepting its candidate text.
/// 在接受候选改写文本前校验改写响应身份。
fn validate_rewrite_response(
    operation_id: &str,
    max_rewrites: usize,
    response: &VceQueryRewriteResponse,
) -> Result<(), VceError> {
    if response.operation_id != operation_id {
        return Err(VceError::ResponseIdentity {
            operation: VcePortOperation::QueryRewrite,
            expected: operation_id.to_owned(),
            actual: response.operation_id.clone(),
        });
    }
    if response.rewrites.len() > max_rewrites {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::QueryRewrite,
            reason: format!(
                "host returned {} rewrites above limit {max_rewrites}",
                response.rewrites.len()
            ),
        });
    }
    Ok(())
}

/// Validate Embedding response identity, space compatibility, and complete segment coverage.
/// 校验向量响应身份、空间兼容性和完整分段覆盖。
fn validate_embedding_response(
    engine: &VceEngine,
    operation_id: &str,
    response: VceEmbeddingResponse,
    batch: &crate::embedding::EmbeddingBatch,
) -> Result<Vec<SegmentVector>, VceError> {
    if response.operation_id != operation_id {
        return Err(VceError::ResponseIdentity {
            operation: VcePortOperation::Embedding,
            expected: operation_id.to_owned(),
            actual: response.operation_id,
        });
    }
    validate_embedding_space(engine, &response.space)?;
    let expected_ids = batch.request_input_ids();
    let actual_ids = response
        .vectors
        .iter()
        .map(|vector| vector.input_id.clone())
        .collect::<Vec<_>>();
    let expected_id_set = expected_ids.iter().collect::<BTreeSet<_>>();
    let actual_id_set = actual_ids.iter().collect::<BTreeSet<_>>();
    if actual_ids.len() != expected_ids.len() || actual_id_set != expected_id_set {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::Embedding,
            reason: format!(
                "Embedding segment coverage mismatch: expected {:?}, received {:?}",
                expected_ids, actual_ids
            ),
        });
    }
    if let Some(vector) = response.vectors.iter().find(|vector| {
        vector.values.len() != engine.config.embedding_space.dimension
            || vector.values.iter().any(|value| !value.is_finite())
    }) {
        return Err(VceError::InvalidHostResponse {
            operation: VcePortOperation::Embedding,
            reason: format!(
                "Embedding vector `{}` has an invalid dimension or non-finite value",
                vector.input_id
            ),
        });
    }
    Ok(response.vectors)
}

/// Validate returned vector-space compatibility against the engine namespace.
/// 校验返回向量空间与引擎命名空间的兼容性。
fn validate_embedding_space(engine: &VceEngine, space: &EmbeddingSpace) -> Result<(), VceError> {
    let expected = &engine.config.embedding_space;
    if !space.is_well_formed()
        || space.vector_space_id != expected.vector_space_id
        || space.dimension != expected.dimension
        || space.preprocessing_version != expected.preprocessing_version
        || space.chunker_version != expected.chunker_version
        || space.physical_fingerprint != expected.physical_fingerprint
    {
        return Err(VceError::EmbeddingSpaceMismatch {
            expected: expected.vector_space_id.clone(),
            actual: space.vector_space_id.clone(),
        });
    }
    Ok(())
}

/// Validate a host search result identity and enforce exact Blob scope.
/// 校验宿主搜索结果身份，并强制执行精确 Blob 范围。
fn validate_search_response(
    operation: VcePortOperation,
    operation_id: &str,
    response: IndexSearchResponse,
    expected_source: SearchSource,
    scope: &BTreeSet<String>,
    limit: usize,
    exact_identifiers: Option<&[String]>,
) -> Result<Vec<SearchHit>, VceError> {
    if response.operation_id != operation_id {
        return Err(VceError::ResponseIdentity {
            operation,
            expected: operation_id.to_owned(),
            actual: response.operation_id,
        });
    }
    let mut hits = response.hits;
    validate_hits(operation, &hits, scope, limit, exact_identifiers)?;
    validate_source_hit_lines(operation, &hits)?;
    if operation == VcePortOperation::DenseSearch
        && hits
            .iter()
            .any(|hit| hit.kind != crate::model::HitKind::Content)
    {
        return Err(VceError::InvalidHostResponse {
            operation,
            reason: "dense content search returned a non-content semantic kind".to_owned(),
        });
    }
    for hit in &mut hits {
        hit.source = expected_source;
    }
    Ok(hits)
}

/// Validate path hits and content fallbacks from one host response.
/// 校验一次宿主路径响应中的路径命中和内容回退结果。
fn validate_path_response(
    operation_id: &str,
    response: VcePathSearchResponse,
    scope: &BTreeSet<String>,
    limit: usize,
) -> Result<(Vec<SearchHit>, Vec<SearchHit>), VceError> {
    if response.operation_id != operation_id {
        return Err(VceError::ResponseIdentity {
            operation: VcePortOperation::PathSearch,
            expected: operation_id.to_owned(),
            actual: response.operation_id,
        });
    }
    let mut path_hits = response.hits;
    validate_hits(VcePortOperation::PathSearch, &path_hits, scope, limit, None)?;
    let mut path_blobs = BTreeSet::new();
    for hit in &path_hits {
        let expected = crate::path_index::PathDocument::new(hit.blob_name.clone(), &hit.path)
            .and_then(|document| document.to_hit(hit.score))
            .map_err(|error| VceError::InvalidHostResponse {
                operation: VcePortOperation::PathSearch,
                reason: format!("host path hit cannot be reconstructed: {error}"),
            })?;
        if hit.id != expected.id
            || hit.content_hash != expected.content_hash
            || hit.content != expected.content
            || hit.range != expected.range
            || hit.kind != expected.kind
        {
            return Err(VceError::InvalidHostResponse {
                operation: VcePortOperation::PathSearch,
                reason: format!(
                    "host path hit `{}` is not a canonical path document",
                    hit.id
                ),
            });
        }
        if !path_blobs.insert(hit.blob_name.clone()) {
            return Err(VceError::InvalidHostResponse {
                operation: VcePortOperation::PathSearch,
                reason: format!(
                    "host returned more than one path document for Blob `{}`",
                    hit.blob_name
                ),
            });
        }
    }
    for hit in &mut path_hits {
        hit.source = SearchSource::Path;
    }
    let mut fallback_hits = response.fallback_hits;
    validate_hits(
        VcePortOperation::PathSearch,
        &fallback_hits,
        scope,
        path_blobs.len(),
        None,
    )?;
    validate_source_hit_lines(VcePortOperation::PathSearch, &fallback_hits)?;
    let mut fallback_blobs = BTreeSet::new();
    for hit in &mut fallback_hits {
        if hit.kind != crate::model::HitKind::Content
            || !path_blobs.contains(&hit.blob_name)
            || !fallback_blobs.insert(hit.blob_name.clone())
        {
            return Err(VceError::InvalidHostResponse {
                operation: VcePortOperation::PathSearch,
                reason: format!(
                    "fallback Blob `{}` is absent from the path channel or duplicated",
                    hit.blob_name
                ),
            });
        }
        hit.source = SearchSource::Dense;
    }
    Ok((path_hits, fallback_hits))
}

/// Validate structural and scope invariants shared by all host search responses.
/// 校验全部宿主搜索响应共享的结构和范围不变量。
fn validate_hits(
    operation: VcePortOperation,
    hits: &[SearchHit],
    scope: &BTreeSet<String>,
    limit: usize,
    exact_identifiers: Option<&[String]>,
) -> Result<(), VceError> {
    if hits.len() > limit {
        return Err(VceError::InvalidHostResponse {
            operation,
            reason: format!("host returned {} hits above limit {limit}", hits.len()),
        });
    }
    let mut occurrence_keys = BTreeSet::new();
    let mut ids = BTreeMap::<String, String>::new();
    let mut previous_score = None;
    for hit in hits {
        if !scope.contains(&hit.blob_name) {
            return Err(VceError::ScopeViolation {
                blob_name: hit.blob_name.clone(),
            });
        }
        let normalized = crate::identity::normalize_relative_path(&hit.path).map_err(|error| {
            VceError::InvalidHostResponse {
                operation,
                reason: format!("host hit path `{}` is invalid: {error}", hit.path),
            }
        })?;
        if normalized != hit.path
            || hit.id.trim().is_empty()
            || !crate::identity::is_sha256_identity(&hit.id)
            || hit.blob_name.trim().is_empty()
            || !crate::identity::is_sha256_identity(&hit.blob_name)
            || hit.content.is_empty()
            || !hit.content.chars().any(char::is_alphanumeric)
            || hit.range.start_line == 0
            || hit.range.end_line < hit.range.start_line
            || !hit.score.is_finite()
            || crate::identity::content_hash(&hit.content) != hit.content_hash
            || (operation != VcePortOperation::ExactSearch
                && crate::identity::chunk_id_with_offset(
                    &hit.blob_name,
                    &hit.content_hash,
                    hit.range.start_line,
                    hit.range.end_line,
                    hit.start_char_offset,
                ) != hit.id)
            || (operation == VcePortOperation::ExactSearch
                && exact_identifiers.is_none_or(|identifiers| {
                    !crate::symbol::matches_exact_hit_identity(hit, identifiers)
                }))
        {
            return Err(VceError::InvalidHostResponse {
                operation,
                reason: format!("host hit `{}` violates the SearchHit contract", hit.id),
            });
        }
        if previous_score.is_some_and(|score| hit.score > score) {
            return Err(VceError::InvalidHostResponse {
                operation,
                reason: "host search hits are not ordered by descending score".to_owned(),
            });
        }
        previous_score = Some(hit.score);
        let occurrence_key = hit.occurrence_key();
        if !occurrence_keys.insert(occurrence_key.clone()) {
            return Err(VceError::InvalidHostResponse {
                operation,
                reason: format!("host returned duplicate occurrence `{occurrence_key}`"),
            });
        }
        if ids.insert(hit.id.clone(), occurrence_key.clone()).is_some() {
            return Err(VceError::InvalidHostResponse {
                operation,
                reason: format!("host returned duplicate hit identity `{}`", hit.id),
            });
        }
    }
    Ok(())
}

/// Validate that source-backed hits contain exactly the lines declared by their range.
/// 校验源码命中包含的物理行与其声明范围完全一致。
fn validate_source_hit_lines(
    operation: VcePortOperation,
    hits: &[SearchHit],
) -> Result<(), VceError> {
    if let Some(hit) = hits
        .iter()
        .find(|hit| crate::model::source_line_count(&hit.content) != hit.range.line_count())
    {
        return Err(VceError::InvalidHostResponse {
            operation,
            reason: format!(
                "host source hit `{}` text does not match its declared line range",
                hit.id
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::embedding::{EmbeddingSpace, SegmentVector};
    use crate::management::{VceOperationKind, VceOperationPhase};
    use crate::model::{Chunk, ChunkKind, HitKind};
    use crate::ports::{
        CommitReceipt, FindMissingResponse, VceAiBinding, VceIndexCounts, VceIndexIntegrity,
        VceIndexJobSummary, VceIndexStoreStatus, VceRerankResponse, VceSearchResponse,
    };
    use futures::executor::block_on;
    use std::sync::{Arc, Mutex};

    /// Build a deterministic test embedding space shared by fake ports.
    /// 构造供伪端口共享的确定性测试向量空间。
    fn test_space() -> EmbeddingSpace {
        EmbeddingSpace {
            vector_space_id: "code-test".to_owned(),
            physical_fingerprint: Some("fake-model".to_owned()),
            dimension: 2,
            preprocessing_version: "v1".to_owned(),
            chunker_version: "recursive-v1".to_owned(),
        }
    }

    /// Build an engine with small deterministic chunking limits for unit tests.
    /// 使用小型确定性切块限制构造单元测试引擎。
    fn test_engine() -> VceEngine {
        let mut config = VceConfig::new("code-chain", test_space()).unwrap();
        config.indexing.chunker.max_chars = 100;
        config.indexing.chunker.min_chars = 1;
        VceEngine::new(config).unwrap()
    }

    /// Return a ready future for a fake asynchronous host operation.
    /// 为伪异步宿主操作返回立即就绪的 Future。
    fn ready<T: Send + 'static>(value: T) -> crate::ports::VceFuture<T> {
        Box::pin(async move { value })
    }

    /// Fake shared AI registration that returns deterministic two-dimensional vectors.
    /// 返回确定性二维向量的伪共享 AI 注册。
    struct FakeAi {
        /// Fixed registration metadata.
        /// 固定注册元数据。
        binding: VceAiBinding,
        /// Vector space returned by every Embedding request.
        /// 每次向量请求返回的向量空间。
        space: EmbeddingSpace,
        /// Whether optional rewrite calls should fail.
        /// 可选改写调用是否失败。
        fail_rewrite: bool,
        /// Whether optional Rerank calls should fail.
        /// 可选重排调用是否失败。
        fail_rerank: bool,
        /// Whether only path Embedding purposes should fail.
        /// 是否仅让路径向量用途失败。
        fail_path_embedding: bool,
        /// Whether the fake host reports Embedding as available.
        /// 伪宿主是否报告向量能力可用。
        embedding_available: bool,
        /// Whether the fake common registration is active.
        /// 伪共同注册是否处于活动状态。
        registered: bool,
        /// Whether the fake host reports query rewriting as available.
        /// 伪宿主是否报告查询改写能力可用。
        query_rewrite_available: bool,
        /// Whether the fake host reports Rerank as available.
        /// 伪宿主是否报告重排能力可用。
        rerank_available: bool,
        /// Embedding operation identities retained for uniqueness assertions.
        /// 为唯一性断言保留的向量操作标识。
        embedding_operation_ids: Arc<Mutex<Vec<String>>>,
        /// Number of optional query-rewrite calls retained for budget assertions.
        /// 为预算断言保留的可选查询改写调用次数。
        rewrite_call_count: Arc<Mutex<usize>>,
        /// Candidate counts received by the fake Rerank capability.
        /// 伪重排能力接收到的候选数量。
        rerank_candidate_counts: Arc<Mutex<Vec<usize>>>,
        /// Number of frozen capability snapshots read by logical engine entries.
        /// 逻辑引擎入口读取冻结能力快照的次数。
        capability_snapshot_call_count: Arc<Mutex<usize>>,
    }

    impl VceAiPort for FakeAi {
        /// Return the fake fixed registration.
        /// 返回伪固定注册。
        fn binding(&self) -> &VceAiBinding {
            &self.binding
        }

        /// Return all fake capabilities as available.
        /// 返回全部可用的伪能力状态。
        fn capability_snapshot(&self) -> VceAiCapabilitySnapshot {
            *self.capability_snapshot_call_count.lock().unwrap() += 1;
            VceAiCapabilitySnapshot {
                registration_id: self.binding.registration_id.clone(),
                consumer_profile_id: self.binding.consumer_profile_id.clone(),
                vector_space_id: self.binding.vector_space_id.clone(),
                registered: self.registered,
                embedding_available: self.embedding_available,
                query_rewrite_available: self.query_rewrite_available,
                rerank_available: self.rerank_available,
            }
        }

        /// Return one deterministic vector for every requested segment.
        /// 为每个请求分段返回一个确定性向量。
        fn embed(
            &self,
            request: VceEmbeddingRequest,
        ) -> crate::ports::VceFuture<Result<VceEmbeddingResponse, VcePortError>> {
            self.embedding_operation_ids
                .lock()
                .unwrap()
                .push(request.context.operation_id.clone());
            if self.fail_path_embedding
                && matches!(
                    request.context.purpose,
                    VceAiPurpose::CodePathEmbedding | VceAiPurpose::CodePathQueryEmbedding
                )
            {
                return ready(Err(VcePortError::Failed {
                    operation: VcePortOperation::Embedding,
                    message: "fake path Embedding failure".to_owned(),
                }));
            }
            let vectors = request
                .batch
                .segments
                .iter()
                .map(|segment| SegmentVector {
                    input_id: segment.request_input_id(),
                    values: vec![1.0, 1.0],
                })
                .collect();
            ready(Ok(VceEmbeddingResponse {
                operation_id: request.context.operation_id,
                vectors,
                space: self.space.clone(),
            }))
        }

        /// Return no rewrite or an explicit fake failure.
        /// 返回无改写结果或显式伪失败。
        fn rewrite_query(
            &self,
            request: VceQueryRewriteRequest,
        ) -> crate::ports::VceFuture<Result<VceQueryRewriteResponse, VcePortError>> {
            *self.rewrite_call_count.lock().unwrap() += 1;
            if self.fail_rewrite {
                return ready(Err(VcePortError::Failed {
                    operation: VcePortOperation::QueryRewrite,
                    message: "fake rewrite failure".to_owned(),
                }));
            }
            ready(Ok(VceQueryRewriteResponse {
                operation_id: request.context.operation_id,
                rewrites: vec![format!("{} implementation", request.query)],
            }))
        }

        /// Return the source order or an explicit fake failure.
        /// 返回原顺序或显式伪失败。
        fn rerank(
            &self,
            request: VceRerankRequest,
        ) -> crate::ports::VceFuture<Result<VceRerankResponse, VcePortError>> {
            self.rerank_candidate_counts
                .lock()
                .unwrap()
                .push(request.candidates.len());
            if self.fail_rerank {
                return ready(Err(VcePortError::Failed {
                    operation: VcePortOperation::Rerank,
                    message: "fake rerank failure".to_owned(),
                }));
            }
            let ordered_candidate_indexes = (0..request.candidates.len()).collect();
            ready(Ok(VceRerankResponse {
                operation_id: request.context.operation_id,
                ordered_candidate_indexes,
            }))
        }
    }

    /// Fake VLDB boundary that records commits and serves one deterministic hit.
    /// 记录提交并返回一个确定性命中的伪 VLDB 边界。
    struct FakeIndex {
        /// Fixed code-semantic namespace binding.
        /// 固定的代码语义命名空间绑定。
        binding: VceIndexBinding,
        /// Shared commit records for assertions.
        /// 供断言使用的共享提交记录。
        commits: Arc<Mutex<Vec<IndexCommit>>>,
        /// Prepared staged records retained for assertions.
        /// 为断言保留的已准备暂存记录。
        staged: Arc<Mutex<Vec<IndexedBlob>>>,
        /// Number of bounded staging calls retained for assertions.
        /// 为断言保留的有界暂存调用次数。
        stage_call_count: Arc<Mutex<usize>>,
        /// Number of staging abort calls retained for assertions.
        /// 为断言保留的暂存中止调用次数。
        abort_count: Arc<Mutex<usize>>,
        /// Whether commit should fail after staging.
        /// 提交是否应在暂存后失败。
        fail_commit: bool,
        /// Whether commit should return a mismatched receipt.
        /// 提交是否应返回不匹配的回执。
        invalid_commit_receipt: bool,
        /// Whether staging should return a mismatched receipt.
        /// 暂存是否应返回不匹配的回执。
        invalid_stage_receipt: bool,
        /// Optional content hit returned by dense search.
        /// 内容向量搜索返回的可选命中。
        hit: SearchHit,
        /// Whether each query variant should return a distinct valid occurrence.
        /// 每个查询变体是否应返回不同且有效的出现位置。
        vary_dense_hit_by_query: bool,
        /// Whether path retrieval should add one uncovered first-Chunk fallback.
        /// 路径检索是否应添加一个未覆盖文件的首代码块回填。
        return_path_fallback: bool,
    }

    impl VceIndexPort for FakeIndex {
        /// Return the fixed fake index binding.
        /// 返回固定的伪索引绑定。
        fn binding(&self) -> &VceIndexBinding {
            &self.binding
        }

        /// Mark every requested Blob as unknown so the engine prepares it.
        /// 将每个请求 Blob 标记为未知，使引擎准备它。
        fn find_missing(
            &self,
            request: FindMissingRequest,
        ) -> crate::ports::VceFuture<Result<FindMissingResponse, VcePortError>> {
            ready(Ok(FindMissingResponse {
                operation_id: request.request_id,
                unknown: request.blob_names,
                nonindexed: BTreeSet::new(),
            }))
        }

        /// Acknowledge exactly the bounded staged Blob set.
        /// 精确确认有界暂存 Blob 集合。
        fn stage(
            &self,
            request: IndexStageBatch,
        ) -> crate::ports::VceFuture<Result<IndexStageReceipt, VcePortError>> {
            *self.stage_call_count.lock().unwrap() += 1;
            self.staged
                .lock()
                .unwrap()
                .extend(request.blobs.iter().cloned());
            let operation_id = if self.invalid_stage_receipt {
                format!("{}:invalid", request.operation_id)
            } else {
                request.operation_id
            };
            ready(Ok(IndexStageReceipt {
                operation_id,
                refresh_id: request.refresh_id,
                generation: request.generation,
                staged_blob_names: request
                    .blobs
                    .into_iter()
                    .map(|blob| blob.blob_name)
                    .collect(),
            }))
        }

        /// Acknowledge cleanup for an unpublished fake staging transaction.
        /// 确认清理尚未发布的伪暂存事务。
        fn abort_stage(
            &self,
            request: IndexAbortRequest,
        ) -> crate::ports::VceFuture<Result<crate::ports::IndexAbortReceipt, VcePortError>>
        {
            *self.abort_count.lock().unwrap() += 1;
            ready(Ok(crate::ports::IndexAbortReceipt {
                operation_id: request.operation_id,
                refresh_id: request.refresh_id,
                generation: request.generation,
                removed_blob_count: 0,
            }))
        }

        /// Record the staged publication and return its exact identities.
        /// 记录暂存发布并返回其精确身份。
        fn commit(
            &self,
            request: IndexCommit,
        ) -> crate::ports::VceFuture<Result<CommitReceipt, VcePortError>> {
            if self.fail_commit {
                return ready(Err(VcePortError::Failed {
                    operation: VcePortOperation::Commit,
                    message: "fake commit failure".to_owned(),
                }));
            }
            let indexed_blob_names = request
                .staged_blob_names
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let deleted_blob_names = request.deleted_blobs.iter().cloned().collect::<Vec<_>>();
            let receipt = CommitReceipt {
                operation_id: if self.invalid_commit_receipt {
                    format!("{}:invalid", request.request_id)
                } else {
                    request.request_id.clone()
                },
                refresh_id: request.refresh_id.clone(),
                checkpoint_chain_id: request.checkpoint.chain_id.clone(),
                generation: request.generation,
                checkpoint_version: request.checkpoint.version,
                indexed_count: indexed_blob_names.len(),
                deleted_count: request.deleted_blobs.len(),
                indexed_blob_names,
                deleted_blob_names,
            };
            self.commits.lock().unwrap().push(request);
            ready(Ok(receipt))
        }

        /// Return one dense hit inside the exact requested scope.
        /// 在精确请求范围内返回一个内容向量命中。
        fn search_dense(
            &self,
            request: DenseSearchRequest,
        ) -> crate::ports::VceFuture<Result<VceSearchResponse, VcePortError>> {
            let mut hit = self.hit.clone();
            hit.blob_name = request.allowed_blobs.iter().next().cloned().unwrap();
            if self.vary_dense_hit_by_query {
                hit.content = format!("fn candidate_{}() {{}}", request.query_index);
                hit.content_hash = crate::identity::content_hash(&hit.content);
            }
            hit.id = crate::identity::chunk_id_with_offset(
                &hit.blob_name,
                &hit.content_hash,
                hit.range.start_line,
                hit.range.end_line,
                hit.start_char_offset,
            );
            ready(Ok(VceSearchResponse {
                operation_id: request.request_id,
                hits: vec![hit],
            }))
        }

        /// Return no path hits in the generic fake.
        /// 在通用伪实现中不返回路径命中。
        fn search_path(
            &self,
            request: PathSearchRequest,
        ) -> crate::ports::VceFuture<Result<VcePathSearchResponse, VcePortError>> {
            if self.return_path_fallback {
                let blob_name = request.allowed_blobs.iter().next_back().cloned().unwrap();
                let path = "src/path_only.rs";
                let path_hit = crate::path_index::PathDocument::new(blob_name.clone(), path)
                    .unwrap()
                    .to_hit(0.9)
                    .unwrap();
                let fallback = SearchHit::from_chunk(
                    &Chunk::new(
                        blob_name,
                        path,
                        "fn path_only() {}",
                        1,
                        1,
                        ChunkKind::Recursive,
                    )
                    .unwrap(),
                    0.9,
                    SearchSource::Dense,
                    HitKind::Content,
                );
                return ready(Ok(VcePathSearchResponse {
                    operation_id: request.request_id,
                    hits: vec![path_hit],
                    fallback_hits: vec![fallback],
                }));
            }
            ready(Ok(VcePathSearchResponse {
                operation_id: request.request_id,
                hits: Vec::new(),
                fallback_hits: Vec::new(),
            }))
        }

        /// Return no exact hits in the generic fake.
        /// 在通用伪实现中不返回精确命中。
        fn search_exact(
            &self,
            request: ExactSearchRequest,
        ) -> crate::ports::VceFuture<Result<VceSearchResponse, VcePortError>> {
            ready(Ok(VceSearchResponse {
                operation_id: request.request_id,
                hits: Vec::new(),
            }))
        }

        /// Return a ready empty management snapshot.
        /// 返回一个就绪的空管理快照。
        fn management_snapshot(
            &self,
            request: VceManagementRequest,
        ) -> crate::ports::VceFuture<Result<VceIndexManagementSnapshot, VcePortError>> {
            ready(Ok(VceIndexManagementSnapshot {
                operation_id: request.request_id.clone(),
                status: VceIndexStoreStatus::Ready,
                generation: 0,
                checkpoint_version: None,
                counts: VceIndexCounts::default(),
                integrity: VceIndexIntegrity::default(),
                active_job: Some(VceIndexJobSummary {
                    job_id: request.request_id,
                    kind: VceOperationKind::InitialBuild,
                    generation: 0,
                    mode: Some(RefreshMode::Full),
                    reason: Some(RefreshReason::Initial),
                    phase: VceOperationPhase::Completed,
                    attempt: 1,
                    completed_units: 0,
                    total_units: 0,
                    unit: None,
                    started_at_ns: None,
                    finished_at_ns: None,
                    coalesced: false,
                    error: None,
                }),
                last_error: None,
            }))
        }

        /// Return an empty result for every typed management query variant.
        /// 为每一种类型化管理查询变体返回空结果。
        fn management_query(
            &self,
            request: VceManagementQueryRequest,
        ) -> crate::ports::VceFuture<Result<VceManagementQueryResponse, VcePortError>> {
            let result = match request.query {
                crate::management::VceManagementQuery::Files(_) => {
                    crate::management::VceManagementQueryResult::Files(crate::management::VcePage {
                        items: Vec::new(),
                        next_cursor: None,
                        total: Some(0),
                    })
                }
                crate::management::VceManagementQuery::BlobDetail { .. } => {
                    crate::management::VceManagementQueryResult::BlobDetail(None)
                }
                crate::management::VceManagementQuery::Operations(_) => {
                    crate::management::VceManagementQueryResult::Operations(
                        crate::management::VcePage {
                            items: Vec::new(),
                            next_cursor: None,
                            total: Some(0),
                        },
                    )
                }
                crate::management::VceManagementQuery::IntegrityIssues(_) => {
                    crate::management::VceManagementQueryResult::IntegrityIssues(
                        crate::management::VcePage {
                            items: Vec::new(),
                            next_cursor: None,
                            total: Some(0),
                        },
                    )
                }
                crate::management::VceManagementQuery::MaintenancePlan { mode } => {
                    crate::management::VceManagementQueryResult::MaintenancePlan(
                        crate::management::VceMaintenancePlan {
                            mode,
                            actions: Vec::new(),
                            affected_blob_count: 0,
                        },
                    )
                }
            };
            ready(Ok(VceManagementQueryResponse {
                operation_id: request.operation_id,
                result,
            }))
        }
    }

    /// Fake source boundary that fails if a test unexpectedly drops retained source text.
    /// 当测试意外丢失已保留源码文本时失败的伪源码边界。
    struct FakeSource;

    impl VceSourcePort for FakeSource {
        /// Reject unexpected source reads in retained-content tests.
        /// 在保留内容测试中拒绝意外源码读取。
        fn read_sources(
            &self,
            _request: VceSourceReadRequest,
        ) -> crate::ports::VceFuture<Result<VceSourceReadResponse, VcePortError>> {
            ready(Err(VcePortError::Failed {
                operation: VcePortOperation::SourceRead,
                message: "unexpected fake source read".to_owned(),
            }))
        }
    }

    /// Fake host source boundary that returns immutable text by canonical path.
    /// 按规范路径返回不可变文本的伪宿主源码边界。
    struct SnapshotSource {
        /// Canonical path-to-content map owned by the fake host.
        /// 伪宿主持有的规范路径到内容映射。
        documents: BTreeMap<String, String>,
    }

    impl VceSourcePort for SnapshotSource {
        /// Return exactly the source documents requested by VCE.
        /// 精确返回 VCE 请求的源码文档。
        fn read_sources(
            &self,
            request: VceSourceReadRequest,
        ) -> crate::ports::VceFuture<Result<VceSourceReadResponse, VcePortError>> {
            let documents = request
                .expectations
                .iter()
                .map(|expectation| {
                    let content = self
                        .documents
                        .get(&expectation.path)
                        .expect("fake source must contain every requested canonical path")
                        .clone();
                    crate::ports::VceSourceDocument {
                        path: expectation.path.clone(),
                        size: content.len() as u64,
                        mtime_ns: expectation.mtime_ns,
                        content,
                    }
                })
                .collect();
            ready(Ok(VceSourceReadResponse {
                operation_id: request.operation_id,
                documents,
            }))
        }
    }

    /// Create a fake host pair for functional tests.
    /// 为功能测试创建伪宿主端口对。
    fn fake_host() -> (FakeAi, FakeIndex, FakeSource, Arc<Mutex<Vec<IndexCommit>>>) {
        let binding = VceAiBinding::new("registration", "code-profile", "code-test").unwrap();
        let chunk = Chunk::new(
            "placeholder",
            "src/lib.rs",
            "fn main() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let commits = Arc::new(Mutex::new(Vec::new()));
        let staged = Arc::new(Mutex::new(Vec::new()));
        let abort_count = Arc::new(Mutex::new(0));
        (
            FakeAi {
                binding,
                space: test_space(),
                fail_rewrite: false,
                fail_rerank: false,
                fail_path_embedding: false,
                embedding_available: true,
                registered: true,
                query_rewrite_available: true,
                rerank_available: true,
                embedding_operation_ids: Arc::new(Mutex::new(Vec::new())),
                rewrite_call_count: Arc::new(Mutex::new(0)),
                rerank_candidate_counts: Arc::new(Mutex::new(Vec::new())),
                capability_snapshot_call_count: Arc::new(Mutex::new(0)),
            },
            FakeIndex {
                binding: VceIndexBinding::new("vce:code-chain", "code-chain", 1, test_space())
                    .unwrap(),
                commits: commits.clone(),
                staged,
                stage_call_count: Arc::new(Mutex::new(0)),
                abort_count,
                fail_commit: false,
                invalid_commit_receipt: false,
                invalid_stage_receipt: false,
                hit: SearchHit::from_chunk(&chunk, 1.0, SearchSource::Dense, HitKind::Content),
                vary_dense_hit_by_query: false,
                return_path_fallback: false,
            },
            FakeSource,
            commits,
        )
    }

    /// Verify invalid Embedding planning budgets fail before the engine can call a host.
    /// 验证非法向量规划预算会在引擎调用宿主前失败。
    #[test]
    fn rejects_inconsistent_indexing_budgets() {
        let mut config = VceConfig::new("code-chain", test_space()).unwrap();
        config.indexing.embedding_max_segment_chars = 8;
        config.indexing.embedding_overlap_chars = 8;

        assert!(matches!(
            VceEngine::new(config),
            Err(VceConfigError::InvalidIndexingConfig)
        ));
    }

    /// Verify a full refresh uses one atomic commit and shared Embeddings.
    /// 验证完整刷新使用一次原子提交和共享向量调用。
    #[test]
    fn refresh_is_a_functional_entry() {
        let mut engine = test_engine();
        let (ai, index, source, commits) = fake_host();
        let result = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-1",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                Some("git-1".to_owned()),
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();
        assert_eq!(result.generation, 1);
        assert_eq!(result.indexed_blobs.len(), 1);
        let committed = commits.lock().unwrap();
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].state_snapshot, engine.state_snapshot());
        assert_eq!(engine.status().state, VceEngineState::Ready);
    }

    /// Verify one refresh freezes AI capabilities once across content and path Embeddings.
    /// 验证一次刷新会跨内容与路径向量调用仅冻结一次 AI 能力。
    #[test]
    fn refresh_reads_one_frozen_ai_capability_snapshot() {
        let mut engine = test_engine();
        let (ai, index, source, _) = fake_host();
        let capability_snapshot_call_count = ai.capability_snapshot_call_count.clone();

        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-frozen-capabilities",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        assert_eq!(*capability_snapshot_call_count.lock().unwrap(), 1);
    }

    /// Verify a no-op filesystem refresh does not retain committed source text in memory.
    /// 验证无变化的文件系统刷新不会在内存中保留已提交源码正文。
    #[test]
    fn no_op_refresh_releases_committed_filesystem_content() {
        let mut engine = test_engine();
        let (ai, index, source, _) = fake_host();
        let observation = crate::workspace::FileObservation::present(
            "src/lib.rs",
            "fn main() {}",
            Some(1),
            crate::model::FileSource::Filesystem,
        );
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-release-initial",
                RefreshReason::Initial,
                vec![observation.clone()],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        let second = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-release-no-op",
                RefreshReason::Manual,
                vec![observation],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        assert!(second.commit_receipt.is_none());
        assert!(
            engine
                .workspace_snapshot()
                .records
                .iter()
                .all(|record| record.content.is_none())
        );
    }

    /// Verify an explicit full rebuild reindexes unchanged Blobs and publishes a new checkpoint.
    /// 验证明示完整重建会重新索引未变化 Blob 并发布新检查点。
    #[test]
    fn forced_full_rebuild_reindexes_unchanged_blobs() {
        let mut engine = test_engine();
        let (ai, index, source, commits) = fake_host();
        let stage_call_count = Arc::clone(&index.stage_call_count);
        let observation = crate::workspace::FileObservation::present(
            "src/lib.rs",
            "fn main() {}",
            Some(1),
            crate::model::FileSource::Filesystem,
        );
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-before-rebuild",
                RefreshReason::Initial,
                vec![observation.clone()],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        let rebuilt = block_on(engine.refresh(
            RefreshRequest::full_rebuild(
                "refresh-rebuild",
                RefreshReason::Manual,
                vec![observation],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        assert_eq!(rebuilt.indexed_blobs.len(), 1);
        assert_eq!(rebuilt.checkpoint_version, Some(2));
        assert_eq!(*stage_call_count.lock().unwrap(), 2);
        assert_eq!(commits.lock().unwrap().len(), 2);
    }

    /// Verify editor overlay changes are visible as pending work before publication.
    /// 验证编辑器覆盖层变更在发布前会显示为待刷新状态。
    #[test]
    fn overlay_is_a_functional_entry() {
        let mut engine = test_engine();
        let (ai, index, source, _) = fake_host();
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-1",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        let snapshot = engine
            .observe("src/lib.rs", "fn main() { println!(\"overlay\"); }")
            .unwrap();
        assert_eq!(snapshot.generation, 2);
        assert_eq!(engine.status().state, VceEngineState::PendingRefresh);
        assert_eq!(engine.workspace_statistics().present_blobs, 1);
    }

    /// Verify retrieval invokes the functional pipeline and returns rendered context.
    /// 验证检索调用功能流水线并返回渲染后的上下文。
    #[test]
    fn retrieval_is_a_functional_entry() {
        let mut engine = test_engine();
        let (ai, index, source, _) = fake_host();
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-1",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();
        let result = block_on(engine.retrieve(
            RetrieveRequest::current_checkpoint("query-1", "find main function"),
            &index,
            &ai,
        ))
        .unwrap();
        assert!(!result.hits.is_empty());
        assert!(result.formatted_context.contains("Path:"));
        assert_eq!(engine.statistics().retrieval_successes, 1);
    }

    /// Verify explicit retrieval scopes reject invalid Blob identities before host search.
    /// 验证明确定义的检索范围会在宿主搜索前拒绝非法 Blob 身份。
    #[test]
    fn retrieval_rejects_noncanonical_explicit_blob_scope() {
        let mut engine = test_engine();
        let (ai, index, _, _) = fake_host();
        let error = block_on(engine.retrieve(
            RetrieveRequest::in_blobs(
                "query-invalid-scope",
                "find the implementation",
                BTreeSet::from(["not-a-blob".to_owned()]),
            ),
            &index,
            &ai,
        ))
        .unwrap_err();

        assert!(matches!(error, VceError::InvalidRequest(_)));
    }

    /// Verify the configured Rerank limit bounds the payload sent to the shared AI capability.
    /// 验证配置的重排限制会约束发送给共享 AI 能力的载荷。
    #[test]
    fn rerank_candidate_limit_bounds_host_payload() {
        let mut config = VceConfig::new("code-chain", test_space()).unwrap();
        config.indexing.chunker.max_chars = 100;
        config.indexing.chunker.min_chars = 1;
        config.rerank_candidate_limit = 1;
        let mut engine = VceEngine::new(config).unwrap();
        let (ai, mut index, source, _) = fake_host();
        index.vary_dense_hit_by_query = true;
        let rerank_candidate_counts = ai.rerank_candidate_counts.clone();
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-rerank-limit",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        // This test opts into rewrite so the fake host produces multiple fusion candidates.
        // 本测试显式启用改写，使伪宿主产生多个融合候选。
        let mut request =
            RetrieveRequest::current_checkpoint("query-rerank-limit", "find main function");
        request.allow_query_rewrite = true;
        let result = block_on(engine.retrieve(request, &index, &ai)).unwrap();

        assert_eq!(rerank_candidate_counts.lock().unwrap().as_slice(), &[1]);
        assert!(result.summary.candidate_count > 1);
    }

    /// Verify a planner with no rewrite capacity never spends an optional LLM call.
    /// 验证没有改写容量的规划器绝不会消耗可选 LLM 调用。
    #[test]
    fn single_query_budget_skips_query_rewrite_call() {
        let mut config = VceConfig::new("code-chain", test_space()).unwrap();
        config.query.max_queries = 1;
        let mut engine = VceEngine::new(config).unwrap();
        let (ai, index, source, _) = fake_host();
        let rewrite_call_count = ai.rewrite_call_count.clone();
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-single-query",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        let mut request = RetrieveRequest::current_checkpoint("query-single", "find main function");
        request.allow_query_rewrite = true;
        let result = block_on(engine.retrieve(request, &index, &ai)).unwrap();

        assert_eq!(*rewrite_call_count.lock().unwrap(), 0);
        assert_eq!(result.query_plan.queries.len(), 1);
        assert!(result.degradations.is_empty());
    }

    /// Verify path fallbacks cannot expand the post-fusion candidate budget.
    /// 验证路径回填不能扩大融合后的候选预算。
    #[test]
    fn path_fallbacks_obey_fusion_candidate_limit() {
        let mut config = VceConfig::new("code-chain", test_space()).unwrap();
        config.fusion_candidate_limit = 1;
        let mut engine = VceEngine::new(config).unwrap();
        let (ai, mut index, source, _) = fake_host();
        index.return_path_fallback = true;
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-fusion-limit",
                RefreshReason::Initial,
                vec![
                    crate::workspace::FileObservation::present(
                        "src/a.rs",
                        "fn a() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                    crate::workspace::FileObservation::present(
                        "src/z.rs",
                        "fn z() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                ],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        let result = block_on(engine.retrieve(
            RetrieveRequest::current_checkpoint("query-fusion-limit", "find config.json"),
            &index,
            &ai,
        ))
        .unwrap();

        assert_eq!(result.summary.candidate_count, 1);
        assert!(result.hits.len() <= 1);
    }

    /// Verify rewrite and Rerank failures remain visible as safe degradation.
    /// 验证改写和重排失败会以安全降级方式保持可见。
    #[test]
    fn optional_ai_stages_degrade_without_losing_retrieval() {
        let mut engine = test_engine();
        let (mut ai, index, source, _) = fake_host();
        ai.fail_rewrite = true;
        ai.fail_rerank = true;
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-1",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();
        // This test explicitly exercises both optional AI stages.
        // 本测试显式覆盖两个可选 AI 阶段。
        let mut request = RetrieveRequest::current_checkpoint("query-1", "find main function");
        request.allow_query_rewrite = true;
        let result = block_on(engine.retrieve(request, &index, &ai)).unwrap();
        assert!(
            result
                .degraded_features
                .contains(&VceDegradedFeature::QueryRewrite)
        );
        assert!(
            result
                .degraded_features
                .contains(&VceDegradedFeature::Rerank)
        );
        assert_eq!(result.degradations.len(), 2);
        assert!(
            result
                .degradations
                .iter()
                .all(|degradation| !degradation.reason.is_empty())
        );
        assert!(!result.hits.is_empty());
    }

    /// Verify metadata-only observations are read through the host and not retained after commit.
    /// 验证仅元数据观察通过宿主读取，且提交后不再保留源码正文。
    #[test]
    fn identified_observation_uses_bounded_source_port() {
        let mut engine = test_engine();
        let (ai, index, _, _) = fake_host();
        let staged = index.staged.clone();
        let content = "pub fn answer() -> u32 { 42 }";
        let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
        let source = SnapshotSource {
            documents: BTreeMap::from([("src/lib.rs".to_owned(), content.to_owned())]),
        };
        let result = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-identified",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::identified(
                    "src/lib.rs",
                    blob.clone(),
                    content.len() as u64,
                    Some(9),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();
        assert_eq!(result.indexed_blobs, vec![blob]);
        assert_eq!(staged.lock().unwrap().len(), 1);
        assert!(engine.workspace_snapshot().records[0].content.is_none());
    }

    /// Verify metadata-only admission does not require an unused Embedding capability.
    /// 验证仅元数据准入不会要求未被使用的向量能力。
    #[test]
    fn over_limit_blob_refresh_succeeds_without_embedding_capability() {
        let mut config = VceConfig::new("code-chain", test_space()).unwrap();
        config.indexing.max_file_bytes = 1;
        let mut engine = VceEngine::new(config).unwrap();
        let (mut ai, index, source, _) = fake_host();
        ai.embedding_available = false;

        let result = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-without-embedding",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "assets/large.txt",
                    "larger than one byte",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        assert_eq!(result.indexed_blobs.len(), 1);
        assert!(ai.embedding_operation_ids.lock().unwrap().is_empty());
    }

    /// Verify an over-limit metadata observation can exceed the source staging byte budget.
    /// 验证超限的仅元数据观察可以大于源码暂存字节预算。
    #[test]
    fn over_limit_metadata_blob_bypasses_source_staging_bytes() {
        let mut config = VceConfig::new("code-chain", test_space()).unwrap();
        config.indexing.max_file_bytes = 1;
        config.index_stage_max_bytes = 1;
        let mut engine = VceEngine::new(config).unwrap();
        let (mut ai, index, source, _) = fake_host();
        ai.embedding_available = false;
        let staged = index.staged.clone();
        let blob = crate::identity::blob_name("assets/large.txt", "ab").unwrap();

        let result = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-large-metadata",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::identified(
                    "assets/large.txt",
                    blob.clone(),
                    2,
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();

        assert_eq!(result.indexed_blobs, vec![blob]);
        assert!(ai.embedding_operation_ids.lock().unwrap().is_empty());
        let staged = staged.lock().unwrap();
        assert_eq!(staged.len(), 1);
        assert!(matches!(
            staged[0].status,
            IndexedBlobStatus::ReadyWithoutEmbedding { .. }
        ));
    }

    /// Verify path-vector failure does not invalidate primary content indexing.
    /// 验证路径向量失败不会使主要内容索引失效。
    #[test]
    fn path_embedding_failure_is_persisted_as_auxiliary_degradation() {
        let mut engine = test_engine();
        let (mut ai, index, source, _) = fake_host();
        ai.fail_path_embedding = true;
        let staged = index.staged.clone();
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-path-failure",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();
        let staged = staged.lock().unwrap();
        assert_eq!(staged.len(), 1);
        assert!(matches!(staged[0].status, IndexedBlobStatus::Ready));
        assert!(matches!(
            staged[0].path_status,
            AuxiliaryIndexStatus::Failed { .. }
        ));
    }

    /// Verify a mismatched staging receipt is rejected and its transaction is aborted.
    /// 验证不匹配的暂存回执会被拒绝，并中止对应事务。
    #[test]
    fn invalid_stage_receipt_aborts_without_checkpoint() {
        let mut engine = test_engine();
        let (ai, mut index, source, _) = fake_host();
        index.invalid_stage_receipt = true;
        let abort_count = index.abort_count.clone();
        let error = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-invalid-stage",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap_err();
        assert!(matches!(error, VceError::ResponseIdentity { .. }));
        assert_eq!(*abort_count.lock().unwrap(), 1);
        assert!(engine.workspace_snapshot().checkpoint.is_none());
    }

    /// Verify a commit failure cleans unpublished staging and never advances local state.
    /// 验证提交失败会清理未发布暂存，且绝不推进本地状态。
    #[test]
    fn commit_failure_aborts_without_checkpoint() {
        let mut engine = test_engine();
        let (ai, mut index, source, _) = fake_host();
        index.fail_commit = true;
        let abort_count = index.abort_count.clone();
        let error = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-commit-failure",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap_err();
        assert!(matches!(error, VceError::Port(_)));
        assert_eq!(*abort_count.lock().unwrap(), 1);
        assert!(engine.workspace_snapshot().checkpoint.is_none());
    }

    /// Verify an invalid commit receipt is rejected before local checkpoint publication.
    /// 验证无效提交回执会在本地检查点发布前被拒绝。
    #[test]
    fn invalid_commit_receipt_aborts_without_checkpoint() {
        let mut engine = test_engine();
        let (ai, mut index, source, _) = fake_host();
        index.invalid_commit_receipt = true;
        let abort_count = index.abort_count.clone();
        let error = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-invalid-commit",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap_err();
        assert!(matches!(error, VceError::ResponseIdentity { .. }));
        assert_eq!(*abort_count.lock().unwrap(), 1);
        assert!(engine.workspace_snapshot().checkpoint.is_none());
    }

    /// Verify an abort acknowledgement cannot refer to another workspace generation.
    /// 验证暂存中止确认不能指向另一工作区代数。
    #[test]
    fn rejects_abort_receipt_for_another_generation() {
        let error = validate_abort_result(
            "abort-op",
            "refresh-op",
            7,
            Ok(crate::ports::IndexAbortReceipt {
                operation_id: "abort-op".to_owned(),
                refresh_id: "refresh-op".to_owned(),
                generation: 6,
                removed_blob_count: 0,
            }),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            VceError::InvalidHostResponse {
                operation: VcePortOperation::AbortStage,
                ..
            }
        ));
    }

    /// Verify versioned state survives JSON and remains bound to its exact namespace.
    /// 验证版本化状态可通过 JSON 恢复，并始终绑定其精确命名空间。
    #[test]
    fn state_snapshot_round_trip_rejects_namespace_drift() {
        let mut engine = test_engine();
        let (ai, index, source, _) = fake_host();
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-state",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();
        let json = engine.state_snapshot().to_json().unwrap();
        let snapshot = VceStateSnapshot::from_json(&json).unwrap();
        let restored = VceEngine::from_state_snapshot(engine.config().clone(), snapshot).unwrap();
        assert_eq!(restored.status().state, VceEngineState::Ready);
        assert_eq!(restored.workspace_snapshot(), engine.workspace_snapshot());

        let mut invalid = engine.state_snapshot();
        invalid.identity.index_namespace_id = "vce:other".to_owned();
        assert!(matches!(
            VceEngine::from_state_snapshot(engine.config().clone(), invalid),
            Err(VceConfigError::StateSnapshot(_))
        ));
    }

    /// Verify restart recovery preserves an observable last-operation failure.
    /// 验证重启恢复会保留可观测的最近操作失败状态。
    #[test]
    fn state_snapshot_restores_failed_lifecycle_state() {
        let mut engine = test_engine();
        let (ai, index, source, _) = fake_host();
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-before-failure",
                RefreshReason::Initial,
                vec![crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();
        let error = block_on(engine.retrieve(
            RetrieveRequest::in_blobs("failed-retrieval", "find main", BTreeSet::new()),
            &index,
            &ai,
        ));
        assert!(error.is_err());
        assert_eq!(engine.status().state, VceEngineState::Failed);

        let restored =
            VceEngine::from_state_snapshot(engine.config().clone(), engine.state_snapshot())
                .unwrap();

        assert_eq!(restored.status().state, VceEngineState::Failed);
        assert!(restored.status().last_error.is_some());
    }

    /// Verify malformed host hits are rejected even when their Blob remains in scope.
    /// 验证即使命中 Blob 仍在范围内，畸形宿主命中也会被拒绝。
    #[test]
    fn search_hit_validation_rejects_nonfinite_scores() {
        let content = "fn main() {}";
        let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
        let chunk = Chunk::new(
            blob.clone(),
            "src/lib.rs",
            content,
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let mut hit =
            SearchHit::from_chunk(&chunk, f32::NAN, SearchSource::Dense, HitKind::Content);
        hit.blob_name = blob.clone();
        let error = validate_hits(
            VcePortOperation::DenseSearch,
            &[hit],
            &BTreeSet::from([blob]),
            1,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));
    }

    /// Verify every AI adapter must return finite vectors in the configured physical dimension.
    /// 验证每个 AI 适配器都必须按配置物理维度返回有限向量。
    #[test]
    fn embedding_response_rejects_invalid_vector_values() {
        let engine = test_engine();
        let input = EmbeddingInput::new("query", "find code").unwrap();
        let plan = EmbeddingPlan::new(vec![input], 1, 100, 100, 0).unwrap();
        let batch = &plan.batches[0];
        let input_id = batch.segments[0].request_input_id();
        for values in [vec![1.0], vec![f32::NAN, 1.0]] {
            let response = VceEmbeddingResponse {
                operation_id: "embedding-op".to_owned(),
                space: test_space(),
                vectors: vec![SegmentVector {
                    input_id: input_id.clone(),
                    values,
                }],
            };

            assert!(matches!(
                validate_embedding_response(&engine, "embedding-op", response, batch),
                Err(VceError::InvalidHostResponse {
                    operation: VcePortOperation::Embedding,
                    ..
                })
            ));
        }
    }

    /// Verify an absent configured fingerprint is not a wildcard for provider model identity.
    /// 验证配置中缺省的指纹不会成为供应商模型身份的通配符。
    #[test]
    fn embedding_response_requires_exact_optional_fingerprint() {
        let mut expected = test_space();
        expected.physical_fingerprint = None;
        let config = VceConfig::new("code-chain", expected).unwrap();
        let engine = VceEngine::new(config).unwrap();
        let mut actual = engine.config.embedding_space.clone();
        actual.physical_fingerprint = Some("unexpected-model".to_owned());

        assert!(matches!(
            validate_embedding_space(&engine, &actual),
            Err(VceError::EmbeddingSpaceMismatch { .. })
        ));
    }

    /// Verify the path channel can return only the canonical generated path document.
    /// 验证路径通道只能返回规范生成的路径文档。
    #[test]
    fn path_search_rejects_arbitrary_self_consistent_chunks() {
        let content = "fn main() {}";
        let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
        let chunk = Chunk::new(
            blob.clone(),
            "src/lib.rs",
            content,
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let hit = SearchHit::from_chunk(&chunk, 0.9, SearchSource::Path, HitKind::Content);

        let error = validate_path_response(
            "path-search",
            VcePathSearchResponse {
                operation_id: "path-search".to_owned(),
                hits: vec![hit],
                fallback_hits: Vec::new(),
            },
            &BTreeSet::from([blob]),
            1,
        )
        .unwrap_err();

        assert!(matches!(error, VceError::InvalidHostResponse { .. }));
    }

    /// Verify one Blob cannot claim multiple canonical path documents in one response.
    /// 验证一个 Blob 不能在同一响应中声明多个规范路径文档。
    #[test]
    fn path_search_rejects_multiple_paths_for_one_blob() {
        let blob = crate::identity::content_hash("shared blob");
        let first = crate::path_index::PathDocument::new(blob.clone(), "src/a.rs")
            .unwrap()
            .to_hit(1.0)
            .unwrap();
        let second = crate::path_index::PathDocument::new(blob.clone(), "src/b.rs")
            .unwrap()
            .to_hit(0.9)
            .unwrap();

        let error = validate_path_response(
            "path-search",
            VcePathSearchResponse {
                operation_id: "path-search".to_owned(),
                hits: vec![first, second],
                fallback_hits: Vec::new(),
            },
            &BTreeSet::from([blob]),
            2,
        )
        .unwrap_err();

        assert!(matches!(error, VceError::InvalidHostResponse { .. }));
    }

    /// Verify optional host responses and source revisions obey their declared bounds.
    /// 验证可选宿主响应与源码版本遵守其声明边界。
    #[test]
    fn optional_response_and_revision_bounds_are_enforced() {
        assert!(
            validate_rewrite_response(
                "rewrite",
                1,
                &VceQueryRewriteResponse {
                    operation_id: "rewrite".to_owned(),
                    rewrites: vec!["one".to_owned(), "two".to_owned()],
                },
            )
            .is_err()
        );
        let request = RefreshRequest::full(
            "refresh",
            RefreshReason::Manual,
            Vec::new(),
            Some("   ".to_owned()),
        );
        assert!(validate_refresh_request(&request).is_err());
    }

    /// Verify host search channels cannot return an ascending score sequence.
    /// 验证宿主搜索通道不能返回分数升序序列。
    #[test]
    fn search_hit_validation_requires_descending_scores() {
        let source = "fn first() {}\nfn second() {}";
        let blob = crate::identity::blob_name("src/lib.rs", source).unwrap();
        let first = Chunk::new(
            blob.clone(),
            "src/lib.rs",
            "fn first() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let second = Chunk::new(
            blob.clone(),
            "src/lib.rs",
            "fn second() {}",
            2,
            2,
            ChunkKind::Recursive,
        )
        .unwrap();
        let error = validate_hits(
            VcePortOperation::DenseSearch,
            &[
                SearchHit::from_chunk(&first, 0.25, SearchSource::Dense, HitKind::Content),
                SearchHit::from_chunk(&second, 0.75, SearchSource::Dense, HitKind::Content),
            ],
            &BTreeSet::from([blob]),
            2,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));
    }

    /// Verify exact search accepts stable symbol occurrence identities instead of Chunk IDs.
    /// 验证精确搜索接受稳定符号出现身份，而不是错误地要求 Chunk 身份。
    #[test]
    fn exact_search_accepts_symbol_occurrence_identity() {
        let content = "fn run() {}";
        let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
        let chunk = Chunk::new(
            blob.clone(),
            "src/lib.rs",
            content,
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let hit = crate::symbol::SymbolOccurrence::from_chunk(
            &chunk,
            "run",
            crate::symbol::SymbolKind::Definition,
        )
        .to_hit(0.9);

        assert!(
            validate_hits(
                VcePortOperation::ExactSearch,
                std::slice::from_ref(&hit),
                &BTreeSet::from([blob.clone()]),
                1,
                Some(&["run".to_owned()]),
            )
            .is_ok()
        );
        assert!(
            validate_hits(
                VcePortOperation::DenseSearch,
                &[hit],
                &BTreeSet::from([blob]),
                1,
                None,
            )
            .is_err()
        );
    }

    /// Verify exact search cannot substitute an unrelated SHA for a requested symbol occurrence.
    /// 验证精确搜索不能为请求的符号出现位置替换无关 SHA 身份。
    #[test]
    fn exact_search_rejects_forged_occurrence_identity() {
        let content = "fn run() {}";
        let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
        let chunk = Chunk::new(
            blob.clone(),
            "src/lib.rs",
            content,
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let mut hit = crate::symbol::SymbolOccurrence::from_chunk(
            &chunk,
            "run",
            crate::symbol::SymbolKind::Definition,
        )
        .to_hit(0.9);
        hit.id = crate::identity::content_hash("forged occurrence");

        assert!(
            validate_hits(
                VcePortOperation::ExactSearch,
                &[hit],
                &BTreeSet::from([blob]),
                1,
                Some(&["run".to_owned()]),
            )
            .is_err()
        );
    }

    /// Verify host search hits cannot substitute an unrelated SHA-256 value as their Chunk ID.
    /// 验证宿主搜索命中不能使用无关 SHA-256 值冒充 Chunk 身份。
    #[test]
    fn search_hit_validation_rejects_mismatched_chunk_identity() {
        let blob_name = crate::identity::blob_name("src/lib.rs", "fn main() {}").unwrap();
        let chunk = Chunk::new(
            blob_name.clone(),
            "src/lib.rs",
            "fn main() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let mut hit = SearchHit::from_chunk(&chunk, 1.0, SearchSource::Dense, HitKind::Content);
        hit.id = crate::identity::content_hash("unrelated identity");

        let error = validate_hits(
            VcePortOperation::DenseSearch,
            &[hit],
            &BTreeSet::from([blob_name]),
            1,
            None,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            VceError::InvalidHostResponse {
                operation: VcePortOperation::DenseSearch,
                ..
            }
        ));
    }

    /// Verify semantic kinds cannot cross from exact symbols into content-only channels.
    /// 验证精确符号的语义类型不能越界进入仅内容通道。
    #[test]
    fn content_channels_reject_symbol_semantic_kinds() {
        let blob_name = crate::identity::blob_name("src/lib.rs", "fn main() {}").unwrap();
        let chunk = Chunk::new(
            blob_name.clone(),
            "src/lib.rs",
            "fn main() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let scope = BTreeSet::from([blob_name.clone()]);
        let endpoint = SearchHit::from_chunk(&chunk, 1.0, SearchSource::Dense, HitKind::Endpoint);
        let error = validate_search_response(
            VcePortOperation::DenseSearch,
            "dense-1",
            VceSearchResponse {
                operation_id: "dense-1".to_owned(),
                hits: vec![endpoint.clone()],
            },
            SearchSource::Dense,
            &scope,
            1,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));

        let path_hit = crate::path_index::PathDocument::new(blob_name, "src/lib.rs")
            .unwrap()
            .to_hit(1.0)
            .unwrap();
        let error = validate_path_response(
            "path-1",
            VcePathSearchResponse {
                operation_id: "path-1".to_owned(),
                hits: vec![path_hit],
                fallback_hits: vec![endpoint],
            },
            &scope,
            1,
        )
        .unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));
    }

    /// Verify management snapshots reject impossible lifecycle and checkpoint states.
    /// 验证管理快照拒绝不可能的生命周期与检查点状态。
    #[test]
    fn management_snapshot_rejects_impossible_lifecycle_state() {
        let mut snapshot = VceIndexManagementSnapshot {
            operation_id: "management-test".to_owned(),
            status: VceIndexStoreStatus::Refreshing,
            generation: 1,
            checkpoint_version: None,
            counts: VceIndexCounts::default(),
            integrity: VceIndexIntegrity::default(),
            active_job: Some(VceIndexJobSummary {
                job_id: "job-test".to_owned(),
                kind: VceOperationKind::InitialBuild,
                generation: 1,
                mode: Some(RefreshMode::Full),
                reason: None,
                phase: VceOperationPhase::Embedding,
                attempt: 1,
                completed_units: 0,
                total_units: 1,
                unit: None,
                started_at_ns: None,
                finished_at_ns: None,
                coalesced: false,
                error: None,
            }),
            last_error: None,
        };
        let error = validate_index_management_snapshot(&snapshot).unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));

        snapshot.active_job.as_mut().unwrap().reason = Some(RefreshReason::Initial);
        assert!(validate_index_management_snapshot(&snapshot).is_ok());

        snapshot.active_job.as_mut().unwrap().phase = VceOperationPhase::Completed;
        let error = validate_index_management_snapshot(&snapshot).unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));

        snapshot.active_job.as_mut().unwrap().completed_units = 1;
        snapshot.active_job.as_mut().unwrap().error = Some("stale failure".to_owned());
        let error = validate_index_management_snapshot(&snapshot).unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));

        snapshot.active_job.as_mut().unwrap().phase = VceOperationPhase::Embedding;
        snapshot.active_job.as_mut().unwrap().completed_units = 0;
        snapshot.active_job.as_mut().unwrap().error = None;

        snapshot.status = VceIndexStoreStatus::Error;
        let error = validate_index_management_snapshot(&snapshot).unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));

        snapshot.status = VceIndexStoreStatus::Refreshing;
        snapshot.active_job.as_mut().unwrap().phase = VceOperationPhase::Failed;
        let error = validate_index_management_snapshot(&snapshot).unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));

        snapshot.active_job.as_mut().unwrap().phase = VceOperationPhase::Embedding;
        snapshot.checkpoint_version = Some(0);
        let error = validate_index_management_snapshot(&snapshot).unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));

        snapshot.checkpoint_version = Some(2);
        let error = validate_index_management_snapshot(&snapshot).unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));

        snapshot.checkpoint_version = None;
        snapshot.integrity = VceIndexIntegrity {
            index_exists: true,
            searchable: false,
            checkpoint_blob_count: 2,
            indexed_blob_count: 2,
            missing_blob_count: 1,
            orphan_blob_count: 0,
            detail: None,
        };
        let error = validate_index_management_snapshot(&snapshot).unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));

        snapshot.integrity = VceIndexIntegrity {
            index_exists: false,
            searchable: false,
            checkpoint_blob_count: 1,
            indexed_blob_count: 1,
            missing_blob_count: 0,
            orphan_blob_count: 0,
            detail: None,
        };
        let error = validate_index_management_snapshot(&snapshot).unwrap_err();
        assert!(matches!(error, VceError::InvalidHostResponse { .. }));
    }

    /// Verify local diagnostics reject capabilities exposed by an inactive registration.
    /// 验证本地诊断会拒绝由未激活注册暴露的能力。
    #[test]
    fn local_management_snapshot_validates_ai_registration_state() {
        let engine = test_engine();
        let (mut ai, _, _, _) = fake_host();
        ai.registered = false;

        assert!(matches!(
            engine.local_management_snapshot(&ai),
            Err(VceError::AiBindingMismatch { .. })
        ));

        ai.embedding_available = false;
        ai.query_rewrite_available = false;
        ai.rerank_available = false;
        let snapshot = engine.local_management_snapshot(&ai).unwrap();
        assert!(!snapshot.ai.registered);
        assert!(snapshot.index.is_none());
    }

    /// Verify initial indexing flushes prepared Blobs by both count and source bytes.
    /// 验证初次索引会同时按 Blob 数量和源码字节数刷新暂存批次。
    #[test]
    fn staging_batches_obey_source_byte_budget() {
        let mut config = VceConfig::new("code-chain", test_space()).unwrap();
        config.indexing.chunker.max_chars = 100;
        config.indexing.chunker.min_chars = 1;
        config.indexing.max_file_bytes = 16;
        config.index_stage_blob_limit = 8;
        config.index_stage_max_bytes = 16;
        let mut engine = VceEngine::new(config).unwrap();
        let (ai, index, source, _) = fake_host();
        let staged = index.staged.clone();
        let stage_call_count = index.stage_call_count.clone();
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-byte-batches",
                RefreshReason::Initial,
                vec![
                    crate::workspace::FileObservation::present(
                        "src/a.rs",
                        "fn first() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                    crate::workspace::FileObservation::present(
                        "src/b.rs",
                        "fn second() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                ],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();
        let staged = staged.lock().unwrap();
        assert_eq!(*stage_call_count.lock().unwrap(), 2);
        assert_eq!(staged.len(), 2);
    }

    /// Verify every Blob and query variant receives distinct host Embedding operation identities.
    /// 验证每个 Blob 与查询变体都会获得不同的宿主向量操作标识。
    #[test]
    fn embedding_operation_identities_are_unique_across_blobs() {
        let mut engine = test_engine();
        let (ai, index, source, _) = fake_host();
        let operation_ids = ai.embedding_operation_ids.clone();
        block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-identities",
                RefreshReason::Initial,
                vec![
                    crate::workspace::FileObservation::present(
                        "src/lib.rs",
                        "fn first() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                    crate::workspace::FileObservation::present(
                        "src/main.rs",
                        "fn second() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                ],
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap();
        let operation_ids = operation_ids.lock().unwrap();
        assert_eq!(
            operation_ids.iter().collect::<BTreeSet<_>>().len(),
            operation_ids.len()
        );
    }

    /// Verify public AI binding fields are revalidated before any host operation starts.
    /// 验证任何宿主操作开始前都会重新校验公开 AI 绑定字段。
    #[test]
    fn empty_ai_binding_identity_is_rejected_at_the_engine_boundary() {
        let mut engine = test_engine();
        let (mut ai, index, source, _) = fake_host();
        ai.binding.registration_id.clear();

        let error = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-empty-ai-binding",
                RefreshReason::Manual,
                Vec::new(),
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap_err();

        assert!(matches!(error, VceError::AiBindingMismatch { .. }));
    }

    /// Verify an index namespace mismatch is rejected before any staging mutation.
    /// 验证索引命名空间不匹配会在任何暂存修改前被拒绝。
    #[test]
    fn index_binding_mismatch_fails_before_staging() {
        let mut engine = test_engine();
        let (ai, mut index, source, _) = fake_host();
        index.binding.namespace_id = "vce:other".to_owned();
        let staged = index.staged.clone();
        let error = block_on(engine.refresh(
            RefreshRequest::full(
                "refresh-wrong-index",
                RefreshReason::Initial,
                Vec::new(),
                None,
            ),
            &index,
            &ai,
            &source,
        ))
        .unwrap_err();
        assert!(matches!(error, VceError::IndexBindingMismatch { .. }));
        assert!(staged.lock().unwrap().is_empty());
    }
}
