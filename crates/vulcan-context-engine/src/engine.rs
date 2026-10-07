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
use crate::indexing::{IndexingConfig, IndexingEngine, IndexingError, IngestResult, PreparedIndex};
use crate::management::{
    VceManagementQueryRequest, VceManagementQueryResponse, VceOperationPhase,
    validate_management_query, validate_management_response,
};
use crate::manifest::{
    VceCodeNodeSnapshot, VceEmbeddingBatchRecord, VceEmbeddingBatchResult, VceEmbeddingBatchState,
    VceEmbeddingRemoteBatchRecord, VceEmbeddingRemoteBatchState, VceEmbeddingRunContract,
    VceEmbeddingRunManifest, VceEmbeddingRunState,
};
use crate::model::{FileRecord, FileSource, FileStatus, SearchHit, SearchSource};
use crate::ports::{
    AuxiliaryIndexStatus, DenseSearchRequest, ExactSearchRequest, FindMissingRequest,
    IndexAbortRequest, IndexCommit, IndexStageBatch, IndexStageReceipt, IndexedBlob,
    IndexedBlobStatus, PathSearchRequest, RefreshMode, RefreshReason, RefreshRequest,
    RetrievalScope, RetrieveRequest, VceAiCapabilitySnapshot, VceAiPort, VceAiPurpose,
    VceAiRequestContext, VceEmbeddingBatchProviderState, VceEmbeddingBatchRequest,
    VceEmbeddingBatchTransport, VceEmbeddingRequest, VceEmbeddingResponse, VceIndexBinding,
    VceIndexManagementSnapshot, VceIndexPort, VceIndexStoreStatus, VceManagementRequest,
    VcePathSearchResponse, VcePortError, VcePortOperation, VceQueryRewriteRequest,
    VceQueryRewriteResponse, VceRerankRequest, VceSearchResponse as IndexSearchResponse,
    VceSourceExpectation, VceSourcePort, VceSourceReadRequest, VceSourceReadResponse,
};
use crate::query::{QueryError, QueryIntent, QueryPlan, QueryPlanner, QueryPlannerConfig};
use crate::refresh_control::{
    VceEmbeddingActivity, VceEmbeddingActivityStage, VceRefreshControl, VceRefreshFailure,
    VceRefreshPhase, VceRefreshProgress, VceRefreshProviderBatchState, VceRefreshUnit,
};
use crate::retrieval::{
    RankedHits, RerankCallback, RetrievalPipeline, merge_semantic_and_exact, reciprocal_rank_fusion,
};
use crate::state::{VceStateIdentity, VceStateSnapshot};
use crate::structure::{
    FallbackStructureAnalyzer, RecursiveStructureAnalyzer, StructureError, StructureMode,
    VceStructureAnalyzer, validate_structure_analysis,
};
use crate::symbol::SymbolOccurrence;
use crate::workspace::{
    Checkpoint, ScopeResolution, SyncPlan, WorkspaceError, WorkspaceSnapshot, WorkspaceState,
};
use futures::{StreamExt, stream};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{Duration, sleep};

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
    /// Optional exact host grant for a different producer; never part of persisted origin identity.
    /// 不同生产来源的可选精确宿主授权；绝不成为持久化原始身份的一部分。
    pub embedding_execution: Option<crate::VceEmbeddingExecutionContract>,
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
            embedding_execution: None,
            dense_candidate_limit: 64,
            path_candidate_limit: 32,
            exact_candidate_limit: 32,
            rerank_candidate_limit: 32,
            fusion_candidate_limit: 64,
            exact_scope_limit: 2_000,
            // Bound local work by file count; storage no longer uses the retired RPC message ceiling.
            // 按文件数量约束本地工作；存储不再使用已退役的 RPC 消息上限。
            index_stage_blob_limit: 64,
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
        // An execution grant is valid for exactly this unchanged index, not another project or chain.
        // 执行授权仅对当前不变索引有效，不适用于另一个项目或检查点链。
        if let Some(grant) = &self.embedding_execution {
            let original = grant.original();
            if original.namespace_id != self.index_namespace_id
                || original.checkpoint_chain_id != self.checkpoint_chain_id
                || original.schema_version != self.index_schema_version
                || original.embedding_space != self.embedding_space
            {
                return Err(VceConfigError::InvalidEmbeddingExecution);
            }
        }
        if self.dense_candidate_limit == 0
            || self.path_candidate_limit == 0
            || self.exact_candidate_limit == 0
            || self.rerank_candidate_limit == 0
            || self.fusion_candidate_limit == 0
            || self.exact_scope_limit == 0
            || self.index_stage_blob_limit == 0
            || self.embedding_max_concurrency == 0
            || self.query_max_concurrency == 0
            || self.index_schema_version == 0
        {
            return Err(VceConfigError::InvalidLookupBudget);
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

/// Per-query execution limits independent of immutable index and checkpoint identity.
/// 独立于不可变索引及检查点身份的单次查询执行限制。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VceRetrievalLimits {
    /// Maximum concurrent query variants for this request.
    /// 此请求的最大并发查询变体数量。
    pub query_max_concurrency: usize,
    /// Maximum selected code chunks for this request.
    /// 此请求最多选择的代码块数量。
    pub max_results: usize,
    /// Aggregate selected-context character limit for this request.
    /// 此请求所选上下文的总字符限制。
    pub max_context_chars: usize,
}

impl VceRetrievalLimits {
    /// Read the engine defaults without modifying index configuration or choosing host policy.
    /// 读取引擎默认值，不修改索引配置或选择宿主策略。
    /// `config` is validated engine configuration; returns the three independent query limits.
    /// `config` 为已验证引擎配置；返回三个独立查询限制。
    pub fn from_config(config: &VceConfig) -> Self {
        Self {
            query_max_concurrency: config.query_max_concurrency,
            max_results: config.retrieval.coverage.max_results,
            max_context_chars: config.retrieval.coverage.max_chars,
        }
    }

    /// Reject zero budgets before any AI or storage call; returns no value on success.
    /// 在任何模型或存储调用前拒绝零预算；成功时不返回值。
    /// No additional inputs; host-specific upper bounds remain the host's explicit admission policy.
    /// 无额外输入；宿主专属上限仍由宿主明确准入策略负责。
    pub fn validate(&self) -> Result<(), VceError> {
        if self.query_max_concurrency == 0 || self.max_results == 0 || self.max_context_chars == 0 {
            return Err(VceError::InvalidRequest(String::from(
                "retrieval execution limits must be positive",
            )));
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
    /// The host execution authorization is malformed or attempts to override a hard storage guard.
    /// 宿主执行授权格式错误，或试图覆盖硬性存储保护。
    InvalidEmbeddingExecution,
    /// A host lookup budget is zero.
    /// 宿主检索预算为零。
    InvalidLookupBudget,
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
            Self::InvalidEmbeddingExecution => formatter.write_str(
                "VCE embedding execution authorization does not match the original index",
            ),
            Self::InvalidLookupBudget => {
                formatter.write_str("VCE lookup budgets must be greater than zero")
            }
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
    /// The host cancelled this exact refresh before its publication boundary.
    /// 宿主在发布边界前取消了此精确刷新。
    RefreshCancelled,
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
            Self::RefreshCancelled => formatter.write_str("VCE refresh was cancelled by the host"),
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
    /// Shared host job cancellation and observation, including the earlier source scan.
    /// 共享宿主作业取消与观察，包括此前源码扫描。
    control: &'a VceRefreshControl,
    /// Stable parent refresh identity used to derive child operation IDs.
    /// 用于派生子操作 ID 的稳定父刷新身份。
    refresh_id: &'a str,
    /// Optional source revision frozen with the refresh request.
    /// 随刷新请求冻结的可选源码修订号。
    source_revision: Option<&'a str>,
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
    /// Whether this initial or explicit rebuild may use remote asynchronous Batch execution.
    /// 本次初始化或显式重建是否可以使用远端异步 Batch 执行。
    use_remote_embedding_batch: bool,
    /// Host-owned immutable source snapshot port.
    /// 宿主持有的不可变源码快照端口。
    source: &'a dyn VceSourcePort,
}

/// Group the stable identities and host capabilities shared by one Embedding plan execution.
/// 聚合一次向量计划执行共享的稳定身份与宿主能力。
struct EmbeddingExecutionContext<'a> {
    /// Stable request prefix used to derive provider operation identities.
    /// 用于派生供应商操作身份的稳定请求前缀。
    request_id: &'a str,
    /// Exact semantic purpose bound to every request in the plan.
    /// 绑定到计划内每个请求的精确语义用途。
    purpose: VceAiPurpose,
    /// Host-owned AI execution port.
    /// 宿主持有的 AI 执行端口。
    ai: &'a dyn VceAiPort,
    /// Frozen capability snapshot validated for this execution.
    /// 为本次执行冻结并校验的能力快照。
    capability: &'a VceAiCapabilitySnapshot,
    /// Whether this plan must use the declared remote asynchronous Batch path.
    /// 当前计划是否必须使用已声明的远端异步 Batch 路径。
    use_remote_batch: bool,
    /// Optional whole-refresh progress base and total.
    /// 可选的完整刷新进度基数与总量。
    overall: Option<(u64, u64)>,
}

/// Keep one prepared Blob's local metadata until shared content batching completes.
/// 在共享内容批次完成前保留一个已准备 Blob 的局部元数据。
struct PreparedBlobForEmbedding {
    /// Deterministic chunking result whose vectors must be supplied to `complete`.
    /// 必须接收向量后交给 `complete` 的确定性切块结果。
    prepared: PreparedIndex,
    /// Structure analyzer identity captured with the source snapshot.
    /// 随源码快照捕获的结构分析器身份。
    structure_analyzer_id: String,
    /// Structure analysis mode used for this Blob.
    /// 此 Blob 使用的结构分析模式。
    structure_mode: StructureMode,
    /// Optional deterministic fallback explanation.
    /// 可选的确定性回退说明。
    structure_fallback_reason: Option<String>,
    /// Symbols produced by the same structure analysis.
    /// 同一次结构分析产生的符号集合。
    symbols: Vec<SymbolOccurrence>,
}

/// Separate metadata-only results from Blobs waiting for the shared Embedding plan.
/// 将仅元数据结果与等待共享向量计划的 Blob 分开。
enum PreparedBlobResult {
    /// A binary or over-limit Blob that needs no Embedding call.
    /// 无需调用向量模型的二进制或超限 Blob。
    Ready(IndexedBlob),
    /// A text Blob waiting for one plan shared by the current staging batch.
    /// 等待当前暂存批次共享计划的文本 Blob。
    Pending(PreparedBlobForEmbedding),
}

/// Preserve source order while deferred Embedding results are completed later.
/// 在延迟完成向量后仍保持源码记录顺序。
enum PreparedBlobSlot {
    /// A Blob that is already ready for staging.
    /// 已经可以暂存的 Blob。
    Ready(Box<IndexedBlob>),
    /// Index into the pending preparation vector.
    /// 指向待处理准备向量的索引。
    Pending(usize),
}

/// Keep all source batches prepared while one global provider plan is executed.
/// 在执行一个全局供应商计划期间保留全部源码批次的准备结果。
struct PreparedStageBatch {
    /// Source-order batch number used in durable operation identities.
    /// 用于持久化操作身份的源码批次编号。
    batch_index: usize,
    /// Ready and pending slots in the original source order.
    /// 按原始源码顺序排列的就绪和待处理槽位。
    blob_slots: Vec<PreparedBlobSlot>,
    /// Pending preparation artifacts addressed by `PreparedBlobSlot::Pending`.
    /// 由 `PreparedBlobSlot::Pending` 索引的待处理准备产物。
    pending_blobs: Vec<PreparedBlobForEmbedding>,
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
                expected: self.config.embedding_space.chunker_version,
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
                actual: index_snapshot.operation_id,
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
        self.refresh_observed(request, index, ai, source, &VceRefreshControl::default())
            .await
    }

    /// Refresh with host-provided observation and cooperative cancellation, without host dependencies.
    /// 使用宿主提供的观察与协作取消进行刷新，不引入宿主依赖。
    ///
    /// `request` owns the source observations; `index`, `ai`, and `source` are registered ports.
    /// `control` belongs to this exact job and may already have been used during source scanning.
    /// Returns the validated committed result or an explicit failure/cancellation after cleanup.
    /// `request` 持有源码观察；`index`、`ai` 与 `source` 为已注册端口。
    /// `control` 属于此精确作业，可能已用于源码扫描。
    /// 返回已校验提交结果，或清理后的明确失败／取消。
    pub async fn refresh_observed(
        &mut self,
        request: RefreshRequest,
        index: &dyn VceIndexPort,
        ai: &dyn VceAiPort,
        source: &dyn VceSourcePort,
        control: &VceRefreshControl,
    ) -> Result<RefreshResult, VceError> {
        self.statistics.refresh_attempts = self.statistics.refresh_attempts.saturating_add(1);
        self.state = VceEngineState::Refreshing;
        let result = self
            .refresh_inner(request, index, ai, source, control)
            .await;
        control.finish();
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
        // control spans the same host job rather than a separate core-only cancellation token.
        // control 覆盖同一个宿主作业，而非单独的核心取消令牌。
        control: &VceRefreshControl,
    ) -> Result<RefreshResult, VceError> {
        control.check()?;
        validate_refresh_request(&request)?;
        self.validate_index_binding(index.binding())?;
        let capability = self.validate_ai_binding(ai, None)?;
        let request_id = request.request_id;
        let mode = request.mode;
        let reason = request.reason;
        let force_reindex = request.force_reindex;
        let source_revision = request.source_revision;
        let durable_summary = index
            .load_embedding_run_summary(request_id.clone())
            .await
            .map_err(VceError::Port)?;
        let retry_generation = durable_summary
            .filter(|(_, state)| *state != VceEmbeddingRunState::Succeeded)
            .map(|(generation, _)| generation);
        // Only candidate reconciliation needs the full plan; ordinary retries load it once during staging.
        // 仅候选对账需要完整计划；普通重试在暂存阶段加载一次。
        let durable_manifest = if durable_summary
            .is_some_and(|(_, state)| state == VceEmbeddingRunState::CandidateReady)
        {
            index
                .load_embedding_manifest(request_id.clone())
                .await
                .map_err(VceError::Port)?
        } else {
            None
        };
        control.observe(VceRefreshProgress {
            activity: None,
            phase: VceRefreshPhase::Reconciling,
            operation_id: request_id.clone(),
            unit: VceRefreshUnit::Files,
            completed: 0,
            total: Some(request.observations.len() as u64),
            overall_completed: 0,
            overall_total: Some(request.observations.len() as u64),
            provider_batch_state: None,
            current_path: None,
        });
        control.check()?;
        let snapshot = match mode {
            RefreshMode::Full => match retry_generation {
                Some(generation) => self.workspace.reconcile_for_existing_run(
                    true,
                    generation,
                    request.observations,
                    Vec::new(),
                )?,
                None => self.workspace.reconcile(request.observations)?,
            },
            RefreshMode::Paths => match retry_generation {
                Some(generation) => self.workspace.reconcile_for_existing_run(
                    false,
                    generation,
                    request.observations,
                    request.removed_paths,
                )?,
                None => self
                    .workspace
                    .reconcile_paths(request.observations, request.removed_paths)?,
            },
        };
        let sync_plan = self.workspace.sync_plan();
        let recovered_candidate = if let Some(manifest) = durable_manifest {
            self.reconcile_candidate_ready_manifest(
                &request_id,
                manifest,
                &sync_plan,
                force_reindex,
                source_revision.as_deref(),
                index,
            )
            .await?
        } else {
            false
        };
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
            control,
            refresh_id: &request_id,
            source_revision: source_revision.as_deref(),
            generation: snapshot.generation,
            index,
            ai,
            capability: &capability,
            use_remote_embedding_batch: reason == RefreshReason::Initial || force_reindex,
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

        // Finalize a candidate-ready manifest inside the same commit transaction as its checkpoint.
        // 将候选就绪清单与检查点放入同一个提交事务中完成终态收口。
        let finalize_embedding_manifest = index
            .load_embedding_run_summary(request_id.clone())
            .await
            .map_err(VceError::Port)?
            .map(|(generation, state)| {
                if generation != snapshot.generation {
                    return Err(VceError::InvalidRequest(
                        "durable Embedding manifest does not match the refresh commit".to_owned(),
                    ));
                }
                Ok(state == VceEmbeddingRunState::CandidateReady)
            })
            .transpose()?
            .unwrap_or(false);

        let checkpoint_needed = !recovered_candidate
            && (force_reindex
                || self.workspace.checkpoint.is_none()
                || !sync_plan.added_blobs.is_empty()
                || !sync_plan.deleted_blobs.is_empty());
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
                finalize_embedding_manifest,
            };
            let publication = match control.begin_commit() {
                Ok(()) => {
                    control.observe(VceRefreshProgress {
                        activity: None,
                        phase: VceRefreshPhase::Committing,
                        operation_id: request_id.clone(),
                        unit: VceRefreshUnit::None,
                        completed: 0,
                        total: None,
                        overall_completed: 0,
                        overall_total: None,
                        provider_batch_state: None,
                        current_path: None,
                    });
                    match index.commit(commit.clone()).await {
                        Ok(receipt) => {
                            let validated =
                                validate_commit_receipt(&commit, &receipt).map(|()| receipt);
                            if validated.is_ok() {
                                // Commit is one atomic host operation; report its completed boundary
                                // before the manifest transitions to published.
                                // 提交是一次原子宿主操作；在清单转为已发布前报告其完成边界。
                                control.observe(VceRefreshProgress {
                                    activity: None,
                                    phase: VceRefreshPhase::Committing,
                                    operation_id: request_id.clone(),
                                    unit: VceRefreshUnit::None,
                                    completed: 1,
                                    total: Some(1),
                                    overall_completed: 1,
                                    overall_total: Some(1),
                                    provider_batch_state: None,
                                    current_path: None,
                                });
                            }
                            validated
                        }
                        Err(error) => Err(VceError::Port(error)),
                    }
                }
                Err(error) => Err(error),
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
            // No-change success still races cancellation; atomically settle before changing local state.
            // 无变化成功仍与取消竞争；改变本地状态之前必须原子结算。
            control.finish_without_commit()?;
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
        control.finish();
        control.observe(VceRefreshProgress {
            activity: None,
            phase: VceRefreshPhase::Complete,
            operation_id: request_id.clone(),
            unit: VceRefreshUnit::None,
            completed: 0,
            total: None,
            overall_completed: 0,
            overall_total: None,
            provider_batch_state: None,
            current_path: None,
        });
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

    /// Reconcile a candidate-ready manifest after a process stopped at the publication boundary.
    /// 在进程停在发布边界后收敛一份候选就绪清单。
    ///
    /// A candidate is considered published only when both the local workspace and host checkpoint
    /// identify the target version. The base-version branch intentionally returns `false`, leaving
    /// the normal atomic commit path to publish the still-unpublished candidate.
    /// 只有本地工作区和宿主检查点同时指向目标版本时，候选才被视为已发布；基准版本分支返回 `false`，
    /// 由正常原子提交路径发布仍未发布的候选。
    async fn reconcile_candidate_ready_manifest(
        &self,
        request_id: &str,
        manifest: VceEmbeddingRunManifest,
        sync_plan: &SyncPlan,
        force_reindex: bool,
        source_revision: Option<&str>,
        index: &dyn VceIndexPort,
    ) -> Result<bool, VceError> {
        if manifest.state != VceEmbeddingRunState::CandidateReady {
            return Ok(false);
        }
        if manifest.source_revision.as_deref() != source_revision {
            return Err(VceError::InvalidRequest(
                "durable Embedding candidate source revision does not match the retry".to_owned(),
            ));
        }
        let target_checkpoint_version = manifest
            .base_checkpoint_version
            .map(|version| {
                version.checked_add(1).ok_or_else(|| {
                    VceError::InvalidRequest(
                        "durable Embedding target checkpoint version is exhausted".to_owned(),
                    )
                })
            })
            .transpose()?
            .unwrap_or(1);
        let operation_id = format!("{request_id}:reconcile-candidate");
        let host_snapshot = index
            .management_snapshot(VceManagementRequest {
                request_id: operation_id.clone(),
            })
            .await
            .map_err(VceError::Port)?;
        if host_snapshot.operation_id != operation_id {
            return Err(VceError::ResponseIdentity {
                operation: VcePortOperation::ManagementSnapshot,
                expected: operation_id,
                actual: host_snapshot.operation_id,
            });
        }
        validate_index_management_snapshot(&host_snapshot)?;
        let local_checkpoint_version = self
            .workspace
            .checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.version);
        if local_checkpoint_version == Some(target_checkpoint_version)
            && host_snapshot.checkpoint_version == Some(target_checkpoint_version)
        {
            if force_reindex
                || !sync_plan.added_blobs.is_empty()
                || !sync_plan.deleted_blobs.is_empty()
            {
                return Err(VceError::InvalidRequest(
                    "durable Embedding candidate is already published; start a new refresh"
                        .to_owned(),
                ));
            }
            if host_snapshot.generation != manifest.generation
                || self.workspace.generation() != manifest.generation
            {
                return Err(VceError::InvalidRequest(
                    "durable Embedding candidate publication generation is inconsistent".to_owned(),
                ));
            }
            let mut published = manifest;
            published
                .mark_succeeded(current_unix_epoch_millis())
                .map_err(VceError::InvalidRequest)?;
            persist_manifest_run_state(index, &published).await?;
            return Ok(true);
        }

        let base_checkpoint_version = manifest.base_checkpoint_version;
        if local_checkpoint_version == base_checkpoint_version
            && host_snapshot.checkpoint_version == base_checkpoint_version
        {
            return Ok(false);
        }
        Err(VceError::InvalidRequest(
            "durable Embedding candidate and published checkpoint disagree".to_owned(),
        ))
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
        let result = self.retrieve_read_view(request, index, ai).await;
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

    /// Execute retrieval from an immutable published engine without mutating shared lifecycle state.
    /// 从不可变已发布引擎执行检索，不修改共享生命周期状态。
    ///
    /// `request` selects the query and scope. The host must pin `index` to this engine's checkpoint
    /// and retain its storage lease throughout the call; `ai` is the admitted execution registration.
    /// `request` 选择查询与范围。宿主必须将 `index` 固定到此引擎检查点，
    /// 并在整个调用期间保留存储租约；`ai` 为已准入的执行注册。
    /// Returns query results and their own summary, or an explicit failure. Concurrent callers
    /// do not change another query's counters, error or state; the host owns aggregate observation.
    /// 返回查询结果及其独立摘要，或明确失败。并发调用者不改变其他查询的计数、错误
    /// 或状态；聚合观察由宿主持有。本入口本身不创建数据库快照或保证底层读取隔离。
    pub async fn retrieve_read_view(
        &self,
        request: RetrieveRequest,
        index: &dyn VceIndexPort,
        ai: &dyn VceAiPort,
    ) -> Result<RetrieveResult, VceError> {
        self.retrieve_read_view_with_limits(
            request,
            index,
            ai,
            VceRetrievalLimits::from_config(&self.config),
        )
        .await
    }

    /// Query one immutable checkpoint with a host-frozen request-local budget.
    /// 使用宿主固定的请求本地预算查询一个不可变检查点。
    /// `request` owns query intent, `index` owns the read lease, `ai` is the registered host, and `limits` owns budgets.
    /// `request` 持有查询意图，`index` 持有读取租约，`ai` 为已注册宿主，`limits` 持有预算。
    /// Returns independent results without changing this engine, another reader, or an active index writer.
    /// 返回独立结果，不改变此引擎、其他读取方或活动索引写入者。
    pub async fn retrieve_read_view_with_limits(
        &self,
        request: RetrieveRequest,
        index: &dyn VceIndexPort,
        ai: &dyn VceAiPort,
        limits: VceRetrievalLimits,
    ) -> Result<RetrieveResult, VceError> {
        limits.validate()?;
        // retrieval is a small value copy; no workspace, checkpoint or provider registration is cloned.
        // retrieval 是小型值拷贝；不克隆工作区、检查点或供应商注册。
        let mut retrieval = self.retrieval;
        retrieval.coverage.max_results = limits.max_results;
        retrieval.coverage.max_chars = limits.max_context_chars;
        validate_retrieve_request(&request)?;
        self.validate_index_binding(index.binding())?;
        let scope = self.resolve_scope(&request.scope)?;
        let capability = self.validate_ai_binding(ai, Some(VceAiPurpose::CodeQueryEmbedding))?;
        let mut query_plan = self.query_planner.plan(&request.information_request)?;
        // A committed empty index is a successful empty search, not missing initialization.
        // 已提交空索引代表成功的空检索，不代表缺少初始化。
        // Skip all model and storage calls when the exact committed membership has no candidates.
        // 精确已提交成员集合没有候选时，跳过全部模型和存储调用。
        if scope.is_empty() {
            let summary = VceRetrievalSummary {
                request_id: request.request_id.clone(),
                intent: query_plan.intent,
                query_count: query_plan.queries.len(),
                dense_hit_count: 0,
                path_hit_count: 0,
                exact_hit_count: 0,
                candidate_count: 0,
                selected_hit_count: 0,
                degraded_features: Vec::new(),
                degradations: Vec::new(),
            };
            return Ok(RetrieveResult {
                request_id: request.request_id,
                query_plan,
                scope,
                hits: Vec::new(),
                formatted_context: crate::retrieval::render_selected_context(
                    &[],
                    limits.max_context_chars,
                ),
                degraded_features: Vec::new(),
                degradations: Vec::new(),
                summary,
            });
        }
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
            .buffer_unordered(limits.query_max_concurrency)
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
        let hits = retrieval.finalize_prepared_candidates(
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
            formatted_context: crate::retrieval::render_selected_context(
                &hits,
                limits.max_context_chars,
            ),
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
        self.stage_blobs_global(blob_names, context).await
    }

    /// Prepare one complete source snapshot, execute global provider-sized batches, and stage results.
    /// 准备完整源码快照，执行全局供应商有界批次，并暂存结果。
    async fn stage_blobs_global(
        &self,
        blob_names: &BTreeSet<String>,
        context: RefreshStageContext<'_>,
    ) -> Result<BTreeSet<String>, VceError> {
        if blob_names.is_empty() {
            return Ok(BTreeSet::new());
        }
        let persisted_manifest = context
            .index
            .load_embedding_manifest(context.refresh_id.to_owned())
            .await
            .map_err(VceError::Port)?;
        // Borrow the frozen nodes; a lookup map must not clone the complete source snapshot.
        // 借用冻结节点；定位映射不应克隆整份源码快照。
        let persisted_nodes = persisted_manifest.as_ref().map(|manifest| {
            manifest
                .nodes
                .iter()
                .map(|node| (node.blob_name.clone(), node))
                .collect::<BTreeMap<_, _>>()
        });
        let selected_records = if let Some(existing) = persisted_nodes.as_ref() {
            let persisted_blob_names = existing.keys().cloned().collect::<BTreeSet<_>>();
            if persisted_blob_names != *blob_names {
                return Err(VceError::InvalidRequest(
                    "durable Embedding source snapshot does not match the requested Blob set"
                        .to_owned(),
                ));
            }
            blob_names
                .iter()
                .map(|blob_name| {
                    let node = existing
                        .get(blob_name)
                        .ok_or_else(|| {
                            VceError::InvalidRequest(format!(
                                "durable Embedding source snapshot is missing Blob `{blob_name}`"
                            ))
                        })?;
                    if node.generation != context.generation {
                        return Err(VceError::InvalidRequest(format!(
                            "durable Embedding source snapshot generation changed for Blob `{blob_name}`"
                        )));
                    }
                    let has_source_body = node.source_content.is_some()
                        || node
                            .prepared_index
                            .as_ref()
                            .is_some_and(|_| true);
                    if node.source_size <= self.config.indexing.max_file_bytes && !has_source_body {
                        return Err(VceError::InvalidRequest(format!(
                            "durable Embedding source snapshot has no source body for Blob `{blob_name}`"
                        )));
                    }
                    Ok(FileRecord {
                        path: node.path.clone(),
                        blob_name: Some(node.blob_name.clone()),
                        committed_blob_name: None,
                        status: FileStatus::Present,
                        size: node.source_size,
                        mtime_ns: node.mtime_ns,
                        source: FileSource::Filesystem,
                        content: node.source_content.clone().or_else(|| {
                            node.prepared_index
                                .as_ref()
                                .map(|prepared| prepared.pending.content.clone())
                        }),
                        generation: node.generation,
                        skip_reason: None,
                    })
                })
                .collect::<Result<Vec<_>, VceError>>()?
        } else {
            staging_records(self.workspace.records.values(), blob_names, context.control)?
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        };
        let mut source_metadata = BTreeMap::new();
        let mut source_contents = BTreeMap::<String, String>::new();
        let mut record_batches = Vec::<Vec<FileRecord>>::new();
        let mut current_batch = Vec::new();
        for record in &selected_records {
            context.control.check()?;
            let blob_name = record.blob_name.clone().ok_or_else(|| {
                VceError::InvalidRequest(format!(
                    "present source `{}` has no Blob identity",
                    record.path
                ))
            })?;
            if source_metadata
                .insert(
                    blob_name,
                    (record.path.clone(), record.size, record.mtime_ns),
                )
                .is_some()
            {
                return Err(VceError::InvalidRequest(
                    "workspace returned duplicate Blob metadata".to_owned(),
                ));
            }
            let record = record.clone();
            // The local source and storage ports share a document count bound, not an RPC payload bound.
            // 本地源码与存储端口共用文档数量边界，不使用 RPC 载荷边界。
            if current_batch.len() >= self.config.index_stage_blob_limit {
                record_batches.push(std::mem::take(&mut current_batch));
            }
            current_batch.push(record);
        }
        if !current_batch.is_empty() {
            record_batches.push(current_batch);
        }

        // File preparation counts span every source batch, including filtered and restored records.
        // 文件准备计数覆盖全部源码批次，包括已过滤及恢复的记录。
        let mut prepared_files = 0_u64;
        // The source set is frozen before any file preparation starts.
        // 开始准备文件前已冻结源码集合。
        let preparation_total = selected_records.len() as u64;
        // Freeze reusable preparation limits before reading any cache entry.
        // 在读取任何缓存条目前冻结可复用准备限制。
        let configured_max_inputs = self.config.indexing.embedding_max_inputs;
        let provider_max_inputs = context.ai.embedding_max_inputs();
        let effective_max_inputs = provider_max_inputs.map_or(configured_max_inputs, |limit| {
            configured_max_inputs.min(limit)
        });
        let mut contract = VceEmbeddingRunContract {
            // An approved producer switch preserves storage identity but changes result compatibility.
            // 已批准生产方切换保留存储身份，但改变结果兼容性。
            embedding_space: match &self.config.embedding_execution {
                Some(grant) => grant.execution().clone(),
                None => self.config.embedding_space.clone(),
            },
            configured_max_inputs,
            provider_max_inputs,
            effective_max_inputs,
            max_chars: self.config.indexing.embedding_max_chars,
            max_segment_chars: self.config.indexing.embedding_max_segment_chars,
            overlap_chars: self.config.indexing.embedding_overlap_chars,
            max_file_bytes: self.config.indexing.max_file_bytes,
            purposes: Vec::new(),
        };
        contract.validate().map_err(VceError::InvalidRequest)?;
        let mut prepared_batches = Vec::with_capacity(record_batches.len());
        let mut content_inputs = Vec::new();
        let mut content_segment_owners = BTreeMap::<String, (usize, usize)>::new();
        for (batch_index, records) in record_batches.into_iter().enumerate() {
            context.control.check()?;
            // Exact same-run recovery is authoritative; only new runs consult reusable preparation.
            // 同运行精确恢复具有权威性；仅新运行查询可复用准备。
            let cached_nodes = if persisted_nodes.is_none() {
                context
                    .index
                    .load_prepared_artifacts(
                        contract.clone(),
                        records
                            .iter()
                            .filter_map(|record| record.blob_name.clone())
                            .collect(),
                        self.structure_analyzer.analyzer_id().to_owned(),
                        context.generation,
                    )
                    .await
                    .map_err(VceError::Port)?
                    .into_iter()
                    .map(|node| (node.blob_name.clone(), node))
                    .collect::<BTreeMap<_, _>>()
            } else {
                BTreeMap::new()
            };
            let loaded = self
                .read_sources_for_records(context.refresh_id, batch_index, &records, context.source)
                .await?;
            let mut blob_slots = Vec::with_capacity(records.len());
            let mut pending_blobs = Vec::new();
            for record in &records {
                context.control.check()?;
                observe_prepared_files(
                    context.control,
                    context.refresh_id,
                    prepared_files,
                    preparation_total,
                    0,
                    &record.path,
                );
                if record.size > self.config.indexing.max_file_bytes {
                    let blob_name = record.blob_name.as_deref().ok_or_else(|| {
                        VceError::InvalidRequest(format!(
                            "present source `{}` has no Blob identity",
                            record.path
                        ))
                    })?;
                    let restored = persisted_nodes
                        .as_ref()
                        .and_then(|nodes| nodes.get(blob_name))
                        .map(|node| self.restore_prepared_blob(record, None, node, &context))
                        .transpose()?
                        .flatten();
                    blob_slots.push(PreparedBlobSlot::Ready(Box::new(match restored {
                        Some(PreparedBlobResult::Ready(blob)) => blob,
                        Some(PreparedBlobResult::Pending(_)) => {
                            return Err(VceError::InvalidRequest(format!(
                                "durable Embedding snapshot unexpectedly prepares over-limit Blob `{blob_name}`"
                            )));
                        }
                        None => self.prepare_over_limit_blob(record, context.generation)?,
                    })));
                    prepared_files += 1;
                    observe_prepared_files(
                        context.control,
                        context.refresh_id,
                        prepared_files,
                        preparation_total,
                        0,
                        &record.path,
                    );
                    context.control.check()?;
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
                source_contents.insert(blob_name.to_owned(), content.to_owned());
                // Modification time is observation metadata; exact path/content Blob identity guards reuse.
                // 修改时间是观察元数据；精确路径和正文的 Blob 身份保护复用。
                let restored = if let Some(nodes) = &persisted_nodes {
                    nodes
                        .get(blob_name)
                        .map(|node| {
                            self.restore_prepared_blob(record, Some(content), node, &context)
                        })
                        .transpose()?
                        .flatten()
                } else if let Some(cached) = cached_nodes.get(blob_name) {
                    let mut node = cached.clone();
                    node.mtime_ns = record.mtime_ns;
                    self.restore_prepared_blob(record, Some(content), &node, &context)?
                } else {
                    None
                };
                let prepared_result = match restored {
                    Some(result) => Ok(result),
                    None => self.prepare_blob(record, content, &context),
                };
                match prepared_result {
                    Ok(PreparedBlobResult::Ready(blob)) => {
                        blob_slots.push(PreparedBlobSlot::Ready(Box::new(blob)));
                    }
                    Ok(PreparedBlobResult::Pending(prepared)) => {
                        let owner = pending_blobs.len();
                        if let Some(plan) = prepared.prepared.embedding_plan.as_ref() {
                            content_inputs.extend(plan.inputs.iter().cloned());
                            for batch in &plan.batches {
                                for segment in &batch.segments {
                                    if content_segment_owners
                                        .insert(
                                            segment.request_input_id(),
                                            (prepared_batches.len(), owner),
                                        )
                                        .is_some()
                                    {
                                        return Err(VceError::InvalidRequest(
                                            "content Embedding segment identity is shared by multiple Blobs"
                                                .to_owned(),
                                        ));
                                    }
                                }
                            }
                        }
                        pending_blobs.push(prepared);
                        blob_slots.push(PreparedBlobSlot::Pending(owner));
                    }
                    Err(VceError::Structure(StructureError::AnalysisFailed { message })) => {
                        let error = StructureError::AnalysisFailed {
                            message: message.clone(),
                        };
                        context.control.observe_failure(VceRefreshFailure {
                            phase: VceRefreshPhase::Chunking,
                            operation_id: format!(
                                "{}:structure:{}",
                                context.refresh_id, record.path
                            ),
                            path: Some(record.path.clone()),
                            message: error.to_string(),
                            api_request_reached: false,
                        });
                        blob_slots.push(PreparedBlobSlot::Ready(Box::new(
                            self.prepare_structure_failure_blob(record, context.generation, error)?,
                        )));
                    }
                    Err(error) => return Err(error),
                }
                prepared_files += 1;
                observe_prepared_files(
                    context.control,
                    context.refresh_id,
                    prepared_files,
                    preparation_total,
                    0,
                    &record.path,
                );
                context.control.check()?;
            }
            prepared_batches.push(PreparedStageBatch {
                batch_index,
                blob_slots,
                pending_blobs,
            });
        }

        // Leave the last filename before potentially expensive global planning and manifest persistence.
        // 在可能耗时的全局规划与清单持久化之前离开最后一个文件名状态。
        observe_embedding_activity(
            context.control,
            context.refresh_id,
            VceEmbeddingActivityStage::Preparing,
            Some((0, blob_names.len() as u64)),
            0,
            None,
            None,
        );
        let build_plan = |inputs: Vec<EmbeddingInput>| {
            EmbeddingPlan::new(
                inputs,
                effective_max_inputs,
                self.config.indexing.embedding_max_chars,
                self.config.indexing.embedding_max_segment_chars,
                self.config.indexing.embedding_overlap_chars,
            )
            .map_err(VceError::Embedding)
        };

        let mut path_inputs = Vec::new();
        for prepared_batch in &prepared_batches {
            for slot in &prepared_batch.blob_slots {
                let document = match slot {
                    PreparedBlobSlot::Ready(blob) => blob.path_document.as_ref(),
                    PreparedBlobSlot::Pending(owner) => prepared_batch
                        .pending_blobs
                        .get(*owner)
                        .and_then(|prepared| prepared.prepared.path_document.as_ref()),
                };
                if let Some(document) = document {
                    path_inputs.push(EmbeddingInput::new(
                        document.path_id.clone(),
                        document.document.clone(),
                    )?);
                }
            }
        }
        observe_embedding_activity(
            context.control,
            context.refresh_id,
            VceEmbeddingActivityStage::Planning,
            Some((0, blob_names.len() as u64)),
            0,
            None,
            None,
        );
        context.control.check()?;
        let content_plan = (!content_inputs.is_empty())
            .then(|| build_plan(content_inputs))
            .transpose()?;
        let path_plan = (!path_inputs.is_empty())
            .then(|| build_plan(path_inputs))
            .transpose()?;

        let mut purposes = Vec::new();
        if content_plan.is_some() {
            purposes.push(VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned());
        }
        if path_plan.is_some() {
            purposes.push(VceAiPurpose::CodePathEmbedding.as_str().to_owned());
        }
        contract.purposes = purposes;
        contract.validate().map_err(VceError::InvalidRequest)?;

        let current_blob_names = self
            .workspace
            .records
            .values()
            .filter(|record| record.status == FileStatus::Present)
            .filter_map(|record| record.blob_name.clone())
            .collect::<BTreeSet<_>>();
        let base_checkpoint_version = self
            .workspace
            .checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.version);
        let reused_blob_names = self
            .workspace
            .checkpoint
            .as_ref()
            .map(|checkpoint| {
                checkpoint
                    .members
                    .intersection(&current_blob_names)
                    .filter(|blob_name| !blob_names.contains(*blob_name))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut nodes = Vec::new();
        for prepared_batch in &prepared_batches {
            for slot in &prepared_batch.blob_slots {
                let (
                    blob_name,
                    path,
                    chunks,
                    content_input_ids,
                    path_input_id,
                    prepared_index,
                    structure_analyzer_id,
                    structure_mode,
                    structure_fallback_reason,
                    symbols,
                    ready_without_embedding_reason,
                ) = match slot {
                    PreparedBlobSlot::Ready(blob) => {
                        let ready_without_embedding_reason = match &blob.status {
                            IndexedBlobStatus::ReadyWithoutEmbedding { reason } => {
                                Some(reason.clone())
                            }
                            IndexedBlobStatus::Ready => None,
                        };
                        (
                            blob.blob_name.clone(),
                            blob.path.clone(),
                            blob.chunks
                                .iter()
                                .map(|chunk| chunk.chunk_id.clone())
                                .collect(),
                            Vec::new(),
                            blob.path_document
                                .as_ref()
                                .map(|document| document.path_id.clone()),
                            None,
                            blob.structure_analyzer_id.clone(),
                            blob.structure_mode,
                            blob.structure_fallback_reason.clone(),
                            blob.symbols.clone(),
                            ready_without_embedding_reason,
                        )
                    }
                    PreparedBlobSlot::Pending(owner) => {
                        let prepared =
                            prepared_batch.pending_blobs.get(*owner).ok_or_else(|| {
                                VceError::InvalidRequest(
                                    "prepared Blob completion owner is missing".to_owned(),
                                )
                            })?;
                        (
                            prepared.prepared.pending.blob_name.clone(),
                            prepared.prepared.pending.path.clone(),
                            prepared
                                .prepared
                                .chunks
                                .iter()
                                .map(|chunk| chunk.chunk_id.clone())
                                .collect(),
                            prepared.prepared.embedding_plan.as_ref().map_or_else(
                                Vec::new,
                                |plan| {
                                    plan.inputs
                                        .iter()
                                        .map(|input| input.input_id.clone())
                                        .collect()
                                },
                            ),
                            prepared
                                .prepared
                                .path_document
                                .as_ref()
                                .map(|document| document.path_id.clone()),
                            Some(prepared.prepared.clone()),
                            Some(prepared.structure_analyzer_id.clone()),
                            Some(prepared.structure_mode),
                            prepared.structure_fallback_reason.clone(),
                            prepared.symbols.clone(),
                            None,
                        )
                    }
                };
                let (metadata_path, source_size, mtime_ns) =
                    source_metadata.get(&blob_name).cloned().ok_or_else(|| {
                        VceError::InvalidRequest(format!(
                            "source metadata is missing for Blob `{blob_name}`"
                        ))
                    })?;
                if metadata_path != path {
                    return Err(VceError::InvalidRequest(format!(
                        "source metadata path changed for Blob `{blob_name}`"
                    )));
                }
                let source_content = prepared_index
                    .is_none()
                    .then(|| source_contents.get(&blob_name).cloned())
                    .flatten();
                nodes.push(VceCodeNodeSnapshot {
                    blob_name,
                    path,
                    generation: context.generation,
                    source_size,
                    mtime_ns,
                    source_content,
                    prepared_index,
                    structure_analyzer_id,
                    structure_mode,
                    structure_fallback_reason,
                    symbols,
                    ready_without_embedding_reason,
                    chunk_ids: chunks,
                    content_input_ids,
                    path_input_id,
                });
            }
        }

        let mut manifest_batches = Vec::new();
        if let Some(plan) = &content_plan {
            for (batch_index, batch) in plan.batches.iter().enumerate() {
                manifest_batches.push(VceEmbeddingBatchRecord::pending(
                    format!("{}:content:{batch_index}", context.refresh_id),
                    VceAiPurpose::CodeDocumentEmbedding.as_str(),
                    batch,
                ));
            }
        }
        if let Some(plan) = &path_plan {
            for (batch_index, batch) in plan.batches.iter().enumerate() {
                manifest_batches.push(VceEmbeddingBatchRecord::pending(
                    format!("{}:path:{batch_index}", context.refresh_id),
                    VceAiPurpose::CodePathEmbedding.as_str(),
                    batch,
                ));
            }
        }
        let now = current_unix_epoch_millis();
        let planned_manifest = VceEmbeddingRunManifest {
            schema_version: crate::manifest::VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION,
            run_id: context.refresh_id.to_owned(),
            namespace_id: context.index.binding().namespace_id.clone(),
            generation: context.generation,
            source_revision: context.source_revision.map(str::to_owned),
            state: VceEmbeddingRunState::Planned,
            contract,
            base_checkpoint_version,
            reused_blob_names,
            nodes,
            batches: manifest_batches,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        };
        observe_embedding_activity(
            context.control,
            context.refresh_id,
            VceEmbeddingActivityStage::Validating,
            Some((0, blob_names.len() as u64)),
            0,
            None,
            None,
        );
        context.control.check()?;
        planned_manifest
            .validate()
            .map_err(VceError::InvalidRequest)?;
        observe_embedding_activity(
            context.control,
            context.refresh_id,
            VceEmbeddingActivityStage::Persisting,
            Some((0, blob_names.len() as u64)),
            0,
            None,
            None,
        );
        context.control.check()?;
        let mut manifest = match persisted_manifest {
            Some(existing) => {
                validate_embedding_manifest_plan(&planned_manifest, &existing)?;
                if existing.state == VceEmbeddingRunState::Succeeded {
                    return Err(VceError::InvalidRequest(
                        "durable Embedding run was already published".to_owned(),
                    ));
                }
                existing
            }
            None => {
                context
                    .index
                    .persist_embedding_manifest_observed(
                        planned_manifest.clone(),
                        context.control.clone(),
                    )
                    .await
                    .map_err(VceError::Port)?;
                planned_manifest
            }
        };
        manifest
            .mark_running(current_unix_epoch_millis())
            .map_err(VceError::InvalidRequest)?;
        context
            .index
            .persist_embedding_run_state(
                manifest.run_id.clone(),
                manifest.state,
                manifest.updated_at_unix_ms,
            )
            .await
            .map_err(VceError::Port)?;

        // Adopt compatible results before planning remote tasks, preserving Running -> Succeeded admission.
        // 在规划远端任务前接纳兼容结果，保留 Running 到 Succeeded 的准入顺序。
        for position in 0..manifest.batches.len() {
            context.control.check()?;
            if manifest.batches[position].state == VceEmbeddingBatchState::Succeeded {
                continue;
            }
            if context
                .index
                .reuse_embedding_batch_result(
                    manifest.run_id.clone(),
                    manifest.batches[position].batch_id.clone(),
                )
                .await
                .map_err(VceError::Port)?
                .is_some()
            {
                if matches!(
                    manifest.batches[position].state,
                    VceEmbeddingBatchState::Pending | VceEmbeddingBatchState::Failed
                ) {
                    manifest.batches[position]
                        .mark_running()
                        .map_err(VceError::InvalidRequest)?;
                    manifest.updated_at_unix_ms = current_unix_epoch_millis();
                    persist_manifest_batch_record(context.index, &manifest, position).await?;
                }
                manifest.batches[position].mark_succeeded();
                manifest.updated_at_unix_ms = current_unix_epoch_millis();
                persist_manifest_batch_record(context.index, &manifest, position).await?;
            }
        }

        let use_remote_embedding_batch = context.use_remote_embedding_batch
            && context.ai.embedding_batch_capabilities().is_some();
        if use_remote_embedding_batch {
            observe_embedding_activity(
                context.control,
                context.refresh_id,
                VceEmbeddingActivityStage::PlanningTasks,
                Some((0, blob_names.len() as u64)),
                0,
                None,
                None,
            );
            if let Some(plan) = &content_plan {
                self.ensure_embedding_remote_batch_records(
                    plan,
                    &format!("{}:content", context.refresh_id),
                    VceAiPurpose::CodeDocumentEmbedding,
                    context.ai,
                    context.index,
                    &manifest,
                    context.control,
                    Some((0, blob_names.len() as u64)),
                )
                .await?;
            }
            if let Some(plan) = &path_plan {
                self.ensure_embedding_remote_batch_records(
                    plan,
                    &format!("{}:path", context.refresh_id),
                    VceAiPurpose::CodePathEmbedding,
                    context.ai,
                    context.index,
                    &manifest,
                    context.control,
                    Some((0, blob_names.len() as u64)),
                )
                .await?;
            }
        }

        let mut content_vectors_by_blob = prepared_batches
            .iter()
            .map(|batch| {
                (0..batch.pending_blobs.len())
                    .map(|_| Vec::<SegmentVector>::new())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if let Some(plan) = &content_plan {
            let content_segments = match self
                .execute_embedding_plan_durable(
                    plan,
                    EmbeddingExecutionContext {
                        request_id: &format!("{}:content", context.refresh_id),
                        purpose: VceAiPurpose::CodeDocumentEmbedding,
                        ai: context.ai,
                        capability: context.capability,
                        use_remote_batch: use_remote_embedding_batch,
                        overall: Some((0, blob_names.len() as u64)),
                    },
                    context.control,
                    context.index,
                    &mut manifest,
                )
                .await
            {
                Ok(segments) => segments,
                Err(error) => {
                    manifest.state = if matches!(error, VceError::RefreshCancelled) {
                        VceEmbeddingRunState::Cancelled
                    } else {
                        VceEmbeddingRunState::Failed
                    };
                    manifest.updated_at_unix_ms = current_unix_epoch_millis();
                    let _ = persist_manifest_run_state(context.index, &manifest).await;
                    return Err(error);
                }
            };
            for segment in content_segments {
                let (batch_index, owner) = content_segment_owners
                    .get(&segment.input_id)
                    .copied()
                    .ok_or_else(|| VceError::InvalidHostResponse {
                    operation: VcePortOperation::Embedding,
                    reason: format!(
                        "content Embedding returned an unknown segment `{}`",
                        segment.input_id
                    ),
                })?;
                content_vectors_by_blob
                    .get_mut(batch_index)
                    .and_then(|batches| batches.get_mut(owner))
                    .ok_or_else(|| VceError::InvalidHostResponse {
                        operation: VcePortOperation::Embedding,
                        reason: "content Embedding owner is outside the prepared plan".to_owned(),
                    })?
                    .push(segment);
            }
        }

        let mut path_vectors = BTreeMap::new();
        let mut path_error = None;
        if let Some(plan) = &path_plan {
            match self
                .execute_embedding_plan_durable(
                    plan,
                    EmbeddingExecutionContext {
                        request_id: &format!("{}:path", context.refresh_id),
                        purpose: VceAiPurpose::CodePathEmbedding,
                        ai: context.ai,
                        capability: context.capability,
                        use_remote_batch: use_remote_embedding_batch,
                        overall: Some((0, blob_names.len() as u64)),
                    },
                    context.control,
                    context.index,
                    &mut manifest,
                )
                .await
            {
                Ok(segments) => {
                    for vector in plan.aggregate(segments)? {
                        path_vectors.insert(vector.input_id.clone(), vector);
                    }
                }
                Err(VceError::RefreshCancelled) => {
                    manifest.state = VceEmbeddingRunState::Cancelled;
                    manifest.updated_at_unix_ms = current_unix_epoch_millis();
                    let _ = persist_manifest_run_state(context.index, &manifest).await;
                    return Err(VceError::RefreshCancelled);
                }
                Err(error) => {
                    let path_batch_positions = manifest
                        .batches
                        .iter()
                        .enumerate()
                        .filter_map(|(position, batch)| {
                            (batch.purpose == VceAiPurpose::CodePathEmbedding.as_str())
                                .then_some(position)
                        })
                        .collect::<Vec<_>>();
                    for position in path_batch_positions {
                        manifest
                            .batches
                            .get_mut(position)
                            .ok_or_else(|| {
                                VceError::InvalidRequest(
                                    "durable path Embedding batch position is invalid".to_owned(),
                                )
                            })?
                            .mark_failed(error.to_string());
                        manifest.updated_at_unix_ms = current_unix_epoch_millis();
                        persist_manifest_batch_record(context.index, &manifest, position).await?;
                    }
                    path_error = Some(error);
                }
            }
        }

        let mut staged = BTreeSet::new();
        for prepared_batch in &mut prepared_batches {
            if let Err(error) = context.control.check() {
                if matches!(&error, VceError::RefreshCancelled) {
                    // Persist cancellation after the provider phase so a resumed run can distinguish
                    // an intentional stop from an unobserved crash during source materialization.
                    // 在供应商阶段之后持久化取消，使恢复运行能够区分主动停止与源码物化期间未观察到的崩溃。
                    manifest.state = VceEmbeddingRunState::Cancelled;
                    manifest.updated_at_unix_ms = current_unix_epoch_millis();
                    let _ = persist_manifest_run_state(context.index, &manifest).await;
                }
                return Err(error);
            }
            let mut completed_pending = Vec::with_capacity(prepared_batch.pending_blobs.len());
            for (owner, prepared) in prepared_batch.pending_blobs.drain(..).enumerate() {
                let symbol_status = if prepared.symbols.is_empty() {
                    AuxiliaryIndexStatus::NotApplicable
                } else {
                    AuxiliaryIndexStatus::Ready
                };
                let ready = self.indexing.complete(
                    prepared.prepared,
                    content_vectors_by_blob[prepared_batch.batch_index][owner]
                        .drain(..)
                        .collect(),
                    self.config.embedding_space.dimension,
                )?;
                completed_pending.push(Some(IndexedBlob {
                    blob_name: ready.blob_name,
                    path: ready.path,
                    generation: ready.generation,
                    structure_analyzer_id: Some(prepared.structure_analyzer_id),
                    structure_mode: Some(prepared.structure_mode),
                    structure_fallback_reason: prepared.structure_fallback_reason,
                    chunks: ready.chunks,
                    embedding_vectors: ready.embedding_vectors,
                    path_document: ready.path_document,
                    path_embedding: None,
                    symbols: prepared.symbols,
                    path_status: AuxiliaryIndexStatus::NotApplicable,
                    symbol_status,
                    status: IndexedBlobStatus::Ready,
                }));
            }
            let mut blobs = Vec::with_capacity(prepared_batch.blob_slots.len());
            for slot in std::mem::take(&mut prepared_batch.blob_slots) {
                match slot {
                    PreparedBlobSlot::Ready(blob) => blobs.push(*blob),
                    PreparedBlobSlot::Pending(owner) => blobs.push(
                        completed_pending
                            .get_mut(owner)
                            .and_then(Option::take)
                            .ok_or_else(|| {
                                VceError::InvalidRequest(
                                    "prepared Blob completion owner is missing".to_owned(),
                                )
                            })?,
                    ),
                }
            }
            for blob in &mut blobs {
                if let Some(document) = blob.path_document.as_ref() {
                    if let Some(vector) = path_vectors.get(&document.path_id) {
                        blob.path_embedding = Some(vector.clone());
                        blob.path_status = AuxiliaryIndexStatus::Ready;
                    } else if let Some(error) = path_error.as_ref() {
                        blob.path_status = AuxiliaryIndexStatus::Failed {
                            reason: error.to_string(),
                        };
                    } else {
                        return Err(VceError::InvalidHostResponse {
                            operation: VcePortOperation::Embedding,
                            reason: format!(
                                "path Embedding returned no logical vector for `{}`",
                                document.path_id
                            ),
                        });
                    }
                }
            }
            let operation_id = format!(
                "{}:stage:{}",
                context.refresh_id, prepared_batch.batch_index
            );
            let request = IndexStageBatch {
                operation_id,
                refresh_id: context.refresh_id.to_owned(),
                generation: context.generation,
                blobs,
            };
            let receipt = context.index.stage(request.clone()).await?;
            validate_stage_receipt(&request, &receipt)?;
            staged.extend(receipt.staged_blob_names);
            context.control.observe(VceRefreshProgress {
                activity: None,
                phase: VceRefreshPhase::Writing,
                operation_id: context.refresh_id.to_owned(),
                unit: VceRefreshUnit::Blobs,
                completed: staged.len() as u64,
                total: Some(blob_names.len() as u64),
                overall_completed: staged.len() as u64,
                overall_total: Some(blob_names.len() as u64),
                provider_batch_state: None,
                current_path: None,
            });
        }
        manifest
            .mark_candidate_ready(current_unix_epoch_millis())
            .map_err(VceError::InvalidRequest)?;
        persist_manifest_run_state(context.index, &manifest).await?;
        Ok(staged)
    }

    /// Restore one immutable prepared artifact and avoid re-reading or re-analyzing its source.
    /// 恢复一个不可变准备产物，避免重新读取源码或重新执行结构分析。
    fn restore_prepared_blob(
        &self,
        record: &FileRecord,
        source_content: Option<&str>,
        node: &VceCodeNodeSnapshot,
        context: &RefreshStageContext<'_>,
    ) -> Result<Option<PreparedBlobResult>, VceError> {
        let expected_blob = record.blob_name.as_deref().ok_or_else(|| {
            VceError::InvalidRequest(format!(
                "present source `{}` has no Blob identity",
                record.path
            ))
        })?;
        if node.blob_name != expected_blob
            || node.path != record.path
            || node.generation != context.generation
            || node.source_size != record.size
            || node.mtime_ns != record.mtime_ns
        {
            return Err(VceError::InvalidRequest(format!(
                "durable Embedding snapshot changed for `{}`",
                record.path
            )));
        }
        if let Some(content) = source_content {
            let actual_blob = crate::identity::blob_name(&record.path, content)
                .map_err(WorkspaceError::InvalidPath)?;
            validate_prepared_blob_identity(expected_blob, &actual_blob, &record.path)?;
        }

        if let Some(reason) = &node.ready_without_embedding_reason {
            if node.prepared_index.is_some() {
                return Err(VceError::InvalidRequest(format!(
                    "durable Embedding snapshot mixes terminal and prepared states for `{}`",
                    record.path
                )));
            }
            return Ok(Some(PreparedBlobResult::Ready(IndexedBlob {
                blob_name: expected_blob.to_owned(),
                path: record.path.clone(),
                generation: context.generation,
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
                    reason: reason.clone(),
                },
            })));
        }

        let Some(prepared) = node.prepared_index.clone() else {
            // Persisted manifests without a frozen contract are rejected before this helper;
            // this branch remains only for explicit legacy in-memory fixtures.
            // 持久清单若缺少冻结契约会在此辅助函数前被拒绝；此分支仅保留给显式旧版内存夹具。
            return Ok(None);
        };
        if prepared.pending.generation != context.generation
            || prepared.pending.blob_name != expected_blob
            || prepared.pending.path != record.path
            || source_content.is_some_and(|content| content != prepared.pending.content)
            || prepared
                .chunks
                .iter()
                .any(|chunk| chunk.blob_name != expected_blob || chunk.path != record.path)
        {
            return Err(VceError::InvalidRequest(format!(
                "durable prepared Embedding artifact changed for `{}`",
                record.path
            )));
        }
        let actual_blob =
            crate::identity::blob_name(&prepared.pending.path, &prepared.pending.content)
                .map_err(WorkspaceError::InvalidPath)?;
        validate_prepared_blob_identity(expected_blob, &actual_blob, &record.path)?;
        if let Some(plan) = &prepared.embedding_plan {
            plan.validate().map_err(VceError::Embedding)?;
        }
        let structure_analyzer_id = node.structure_analyzer_id.clone().ok_or_else(|| {
            VceError::InvalidRequest(format!(
                "durable prepared Embedding artifact has no analyzer identity for `{}`",
                record.path
            ))
        })?;
        let structure_mode = node.structure_mode.ok_or_else(|| {
            VceError::InvalidRequest(format!(
                "durable prepared Embedding artifact has no structure mode for `{}`",
                record.path
            ))
        })?;
        Ok(Some(PreparedBlobResult::Pending(
            PreparedBlobForEmbedding {
                prepared,
                structure_analyzer_id,
                structure_mode,
                structure_fallback_reason: node.structure_fallback_reason.clone(),
                symbols: node.symbols.clone(),
            },
        )))
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

    /// Preserve one source Blob as metadata when its structural symbols are invalid.
    /// 当一个源码 Blob 的结构符号无效时，将其保留为元数据。
    ///
    /// Structural analysis is a per-Blob admission step. A malformed symbol occurrence must not
    /// discard already staged sibling Blobs, while its exact diagnostic remains durable for repair.
    /// 结构分析属于单 Blob 准入步骤。单个损坏的符号出现记录不能丢弃已暂存的其他 Blob，同时保留精确诊断供修复。
    fn prepare_structure_failure_blob(
        &self,
        record: &FileRecord,
        generation: u64,
        error: StructureError,
    ) -> Result<IndexedBlob, VceError> {
        let blob_name = record.blob_name.clone().ok_or_else(|| {
            VceError::InvalidRequest(format!(
                "present source `{}` has no Blob identity",
                record.path
            ))
        })?;
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
            symbol_status: AuxiliaryIndexStatus::Failed {
                reason: error.to_string(),
            },
            status: IndexedBlobStatus::ReadyWithoutEmbedding {
                reason: format!("structural analysis skipped: {error}"),
            },
        })
    }

    /// Prepare one exact source and its content/path vectors under a shared refresh control.
    /// 在共享刷新控制下准备一个精确源码及其内容／路径向量。
    ///
    /// `record` and `content` are verified source facts; `request_id` identifies this blob's operations.
    /// `context` retains the admitted generation, frozen host inference, cancellation and progress.
    /// Returns a validated indexed blob or an explicit error without publishing a checkpoint.
    /// `record` 与 `content` 是已验证源码事实；`request_id` 标识此块的操作。
    /// `context` 保留已准入代次、冻结宿主推理、取消及进度。
    /// 返回已校验索引块对象或明确错误，不发布检查点。
    fn prepare_blob(
        &self,
        record: &FileRecord,
        content: &str,
        context: &RefreshStageContext<'_>,
    ) -> Result<PreparedBlobResult, VceError> {
        // Preserve one admitted staging context across content and path embedding without copying dependencies.
        // 在内容及路径向量之间保留同一准入暂存上下文，不复制依赖。
        let generation = context.generation;
        // control retains the parent job's cancellation and observations.
        // control 保留父作业的取消与观察。
        let control = context.control;
        control.check()?;
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
                Ok(PreparedBlobResult::Ready(IndexedBlob {
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
                }))
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
                control.check()?;
                Ok(PreparedBlobResult::Pending(PreparedBlobForEmbedding {
                    prepared,
                    structure_analyzer_id,
                    structure_mode,
                    structure_fallback_reason,
                    symbols,
                }))
            }
        }
    }

    /// Execute all batches of one Embedding plan and preserve segment identities.
    /// 执行一个向量计划的全部批次并保持分段身份对齐。
    async fn execute_embedding_plan(
        &self,
        plan: &EmbeddingPlan,
        context: EmbeddingExecutionContext<'_>,
        // Query calls have no indexing control; refresh calls share their exact job control.
        // 查询调用没有索引控制；刷新调用共享其精确作业控制。
        control: Option<&VceRefreshControl>,
    ) -> Result<Vec<SegmentVector>, VceError> {
        let EmbeddingExecutionContext {
            request_id,
            purpose,
            ai,
            capability,
            use_remote_batch: _,
            overall,
        } = context;
        if !capability.registered || !capability.embedding_available {
            return Err(VceError::AiCapabilityUnavailable { purpose });
        }
        // Never let a consumer budget exceed the exact model contract exposed by the host.
        // 绝不允许调用方预算超过宿主暴露的精确模型契约。
        let configured_max_inputs = self.config.indexing.embedding_max_inputs;
        let effective_max_inputs = ai
            .embedding_max_inputs()
            .map_or(configured_max_inputs, |model_max_inputs| {
                configured_max_inputs.min(model_max_inputs)
            });
        let effective_plan = if effective_max_inputs == configured_max_inputs {
            plan.clone()
        } else {
            EmbeddingPlan::new(
                plan.inputs.clone(),
                effective_max_inputs,
                self.config.indexing.embedding_max_chars,
                self.config.indexing.embedding_max_segment_chars,
                self.config.indexing.embedding_overlap_chars,
            )?
        };
        let requests =
            effective_plan
                .batches
                .iter()
                .cloned()
                .enumerate()
                .map(|(batch_index, batch)| {
                    let operation_id = format!("{request_id}:{}:{batch_index}", purpose.as_str());
                    async move {
                        if let Some(control) = control {
                            control.check()?;
                        }
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
                        Ok::<_, VceError>((batch_index, operation_id, batch, response))
                    }
                });
        // responses yields actual completions; progress cannot count merely scheduled requests.
        // responses 产生实际完成结果；进度不能统计仅排队的请求。
        let mut responses =
            stream::iter(requests).buffer_unordered(self.config.embedding_max_concurrency);
        // completed_batches preserves original order after validating out-of-order responses.
        // completed_batches 在校验乱序响应后保留原始顺序。
        let mut completed_batches = Vec::new();
        if let Some(control) = control {
            control.observe(VceRefreshProgress {
                activity: None,
                phase: VceRefreshPhase::Embedding,
                operation_id: format!("{request_id}:{}", purpose.as_str()),
                unit: VceRefreshUnit::Batches,
                completed: 0,
                total: Some(effective_plan.batches.len() as u64),
                overall_completed: overall.map_or(0, |(completed, _)| completed),
                overall_total: overall.map(|(_, total)| total),
                provider_batch_state: None,
                current_path: None,
            });
        }
        while let Some(response) = responses.next().await {
            let (batch_index, operation_id, batch, response) = response?;
            let response = response?;
            let response_vectors =
                validate_embedding_response(self, &operation_id, response, &batch)?;
            completed_batches.push((batch_index, response_vectors));
            if let Some(control) = control {
                control.check()?;
                control.observe(VceRefreshProgress {
                    activity: None,
                    phase: VceRefreshPhase::Embedding,
                    operation_id: format!("{request_id}:{}", purpose.as_str()),
                    unit: VceRefreshUnit::Batches,
                    completed: completed_batches.len() as u64,
                    total: Some(effective_plan.batches.len() as u64),
                    overall_completed: overall.map_or(0, |(completed, _)| completed),
                    overall_total: overall.map(|(_, total)| total),
                    provider_batch_state: None,
                    current_path: None,
                });
            }
        }
        completed_batches.sort_by_key(|(batch_index, _)| *batch_index);
        Ok(completed_batches
            .into_iter()
            .flat_map(|(_, vectors)| vectors)
            .collect())
    }

    /// Ensure every remote task for one purpose is durably planned before provider I/O.
    /// 在供应商 I/O 前确保一个用途的全部远端任务均已持久规划。
    #[allow(clippy::too_many_arguments)]
    async fn ensure_embedding_remote_batch_records(
        &self,
        plan: &EmbeddingPlan,
        request_id: &str,
        purpose: VceAiPurpose,
        ai: &dyn VceAiPort,
        index: &dyn VceIndexPort,
        manifest: &VceEmbeddingRunManifest,
        control: &VceRefreshControl,
        overall: Option<(u64, u64)>,
    ) -> Result<(), VceError> {
        control.check()?;
        observe_embedding_activity(
            control,
            request_id,
            VceEmbeddingActivityStage::PlanningTasks,
            overall,
            0,
            Some(plan.batches.len() as u64),
            None,
        );
        plan.validate()?;
        let mut records = index
            .load_embedding_remote_batch_records(manifest.run_id.clone(), purpose)
            .await
            .map_err(VceError::Port)?;
        let mut first_logical_batch = 0_usize;
        // Successful logical batches already own durable vectors and need no remote task.
        // 成功逻辑批次已持有持久向量，不需要远端任务。
        let logical_batches = manifest
            .batches
            .iter()
            .filter(|batch| batch.purpose == purpose.as_str())
            .collect::<Vec<_>>();
        if logical_batches.len() != plan.batches.len() {
            return Err(VceError::InvalidRequest(String::from(
                "Remote plan logical batch count changed",
            )));
        }
        let mut frozen_capabilities = records
            .first()
            .map(|record| record.plan.capabilities.clone());
        let active_capabilities = ai.embedding_batch_capabilities().ok_or_else(|| {
            VceError::InvalidRequest(String::from(
                "remote embedding Batch capability disappeared before planning completed",
            ))
        })?;
        active_capabilities
            .validate()
            .map_err(VceError::InvalidRequest)?;
        if frozen_capabilities
            .as_ref()
            .is_some_and(|capabilities| *capabilities != active_capabilities)
        {
            return Err(VceError::InvalidRequest(String::from(
                "current embedding Batch capabilities differ from the durable VCE plan",
            )));
        }
        frozen_capabilities.get_or_insert(active_capabilities);
        for (remote_index, record) in records.iter().enumerate() {
            control.check()?;
            record.validate().map_err(VceError::InvalidRequest)?;
            // Gaps are valid only where a frozen logical result was already adopted before planning.
            // 仅在规划前已接纳冻结逻辑结果的位置允许间隙。
            while first_logical_batch < record.first_logical_batch_index as usize
                && logical_batches
                    .get(first_logical_batch)
                    .is_some_and(|batch| batch.state == VceEmbeddingBatchState::Succeeded)
            {
                first_logical_batch += 1;
            }
            if record.purpose != purpose.as_str()
                || usize::try_from(record.first_logical_batch_index).ok()
                    != Some(first_logical_batch)
                || record.plan.tasks.len() != 1
                || record.plan.request_id != format!("{request_id}:remote:{remote_index}")
                || frozen_capabilities
                    .as_ref()
                    .is_some_and(|capabilities| record.plan.capabilities != *capabilities)
            {
                return Err(VceError::InvalidRequest(String::from(
                    "durable remote embedding Batch records do not match the frozen VCE plan",
                )));
            }
            let logical_count = usize::try_from(record.logical_batch_count).map_err(|_| {
                VceError::InvalidRequest(String::from(
                    "remote embedding Batch logical count exceeds usize",
                ))
            })?;
            let end = first_logical_batch
                .checked_add(logical_count)
                .ok_or_else(|| {
                    VceError::InvalidRequest(String::from(
                        "remote embedding Batch logical range overflowed",
                    ))
                })?;
            if end > plan.batches.len() {
                return Err(VceError::InvalidRequest(String::from(
                    "remote embedding Batch logical range exceeds the frozen plan",
                )));
            }
            let grouped = merge_embedding_batches(&plan.batches[first_logical_batch..end])?;
            record
                .plan
                .validate(grouped.segments.len())
                .map_err(VceError::InvalidRequest)?;
            first_logical_batch = end;
            observe_embedding_activity(
                control,
                request_id,
                VceEmbeddingActivityStage::PlanningTasks,
                overall,
                first_logical_batch as u64,
                Some(plan.batches.len() as u64),
                None,
            );
        }

        while first_logical_batch < plan.batches.len() {
            control.check()?;
            if logical_batches[first_logical_batch].state == VceEmbeddingBatchState::Succeeded {
                first_logical_batch += 1;
                continue;
            }
            let remote_index = records.len();
            let operation_id = format!("{request_id}:remote:{remote_index}");
            let maximum_requests = usize::try_from(
                frozen_capabilities
                    .as_ref()
                    .ok_or_else(|| {
                        VceError::InvalidRequest(String::from(
                            "remote embedding Batch capabilities are unavailable",
                        ))
                    })?
                    .maximum_requests,
            )
            .map_err(|_| {
                VceError::InvalidRequest(String::from(
                    "remote embedding Batch request limit exceeds usize",
                ))
            })?;
            let mut probe_logical_count = 0_usize;
            let mut probe_segment_count = 0_usize;
            for (offset, batch) in plan.batches[first_logical_batch..].iter().enumerate() {
                if logical_batches[first_logical_batch + offset].state
                    == VceEmbeddingBatchState::Succeeded
                {
                    break;
                }
                let next = probe_segment_count
                    .checked_add(batch.segments.len())
                    .ok_or(EmbeddingError::InvalidBudget)?;
                if probe_logical_count != 0 && next > maximum_requests {
                    break;
                }
                probe_segment_count = next;
                probe_logical_count += 1;
                if probe_segment_count >= maximum_requests {
                    break;
                }
            }
            let probe_end = first_logical_batch
                .checked_add(probe_logical_count)
                .ok_or(EmbeddingError::InvalidBudget)?;
            let remaining = merge_embedding_batches(&plan.batches[first_logical_batch..probe_end])?;
            let probe = ai
                .plan_embedding_batch(VceEmbeddingBatchRequest {
                    context: VceAiRequestContext::new(operation_id.clone(), purpose, ai.binding()),
                    batch: remaining,
                })
                .await
                .map_err(VceError::Port)?;
            if frozen_capabilities
                .as_ref()
                .is_some_and(|capabilities| probe.capabilities != *capabilities)
            {
                return Err(VceError::InvalidRequest(String::from(
                    "current embedding Batch capabilities differ from the durable VCE plan",
                )));
            }
            if frozen_capabilities.is_none() {
                frozen_capabilities = Some(probe.capabilities.clone());
            }
            let first_task_input_count = probe
                .tasks
                .first()
                .ok_or_else(|| {
                    VceError::InvalidRequest(String::from(
                        "host returned an empty remote embedding Batch plan",
                    ))
                })?
                .input_count;
            let first_task_input_count = usize::try_from(first_task_input_count).map_err(|_| {
                VceError::InvalidRequest(String::from(
                    "remote embedding Batch input count exceeds usize",
                ))
            })?;
            let mut logical_count = 0_usize;
            let mut segment_count = 0_usize;
            for batch in &plan.batches[first_logical_batch..probe_end] {
                let next = segment_count
                    .checked_add(batch.segments.len())
                    .ok_or(EmbeddingError::InvalidBudget)?;
                if next > first_task_input_count {
                    break;
                }
                segment_count = next;
                logical_count += 1;
            }
            if logical_count == 0 {
                logical_count = 1;
            }
            let end = first_logical_batch
                .checked_add(logical_count)
                .ok_or(EmbeddingError::InvalidBudget)?;
            let grouped = merge_embedding_batches(&plan.batches[first_logical_batch..end])?;
            let provider_plan = ai
                .plan_embedding_batch(VceEmbeddingBatchRequest {
                    context: VceAiRequestContext::new(operation_id, purpose, ai.binding()),
                    batch: grouped.clone(),
                })
                .await
                .map_err(VceError::Port)?;
            if frozen_capabilities
                .as_ref()
                .is_some_and(|capabilities| provider_plan.capabilities != *capabilities)
            {
                return Err(VceError::InvalidRequest(String::from(
                    "current embedding Batch capabilities differ from the durable VCE plan",
                )));
            }
            provider_plan
                .validate(grouped.segments.len())
                .map_err(VceError::InvalidRequest)?;
            if provider_plan.tasks.len() != 1 {
                return Err(VceError::InvalidRequest(format!(
                    "one VCE logical embedding batch exceeds the provider Batch limits for {}",
                    purpose.as_str()
                )));
            }
            let provider_task = provider_plan.tasks[0].clone();
            let now = current_unix_epoch_millis();
            let record = VceEmbeddingRemoteBatchRecord {
                remote_task_id: provider_task.task_id.clone(),
                purpose: purpose.as_str().to_owned(),
                first_logical_batch_index: u32::try_from(first_logical_batch).map_err(|_| {
                    VceError::InvalidRequest(String::from(
                        "remote embedding Batch first logical index exceeds u32",
                    ))
                })?,
                logical_batch_count: u32::try_from(logical_count).map_err(|_| {
                    VceError::InvalidRequest(String::from(
                        "remote embedding Batch logical count exceeds u32",
                    ))
                })?,
                plan: provider_plan,
                state: VceEmbeddingRemoteBatchState::Planned,
                uploaded_input: None,
                submission: None,
                snapshot: None,
                poll_attempt: 0,
                next_poll_not_before_unix_ms: None,
                error: None,
                cleanup_warning: None,
                updated_at_unix_ms: now,
            };
            index
                .persist_embedding_remote_batch_record(manifest.run_id.clone(), record.clone())
                .await
                .map_err(VceError::Port)?;
            records.push(record);
            first_logical_batch = end;
            observe_embedding_activity(
                control,
                request_id,
                VceEmbeddingActivityStage::PlanningTasks,
                overall,
                first_logical_batch as u64,
                Some(plan.batches.len() as u64),
                None,
            );
        }
        if first_logical_batch != plan.batches.len() {
            return Err(VceError::InvalidRequest(String::from(
                "remote embedding Batch records do not cover the frozen logical plan",
            )));
        }
        Ok(())
    }

    /// Execute one durable plan through asynchronous provider Batch tasks and retain logical results.
    /// 通过异步供应商 Batch 任务执行一份持久计划并保留逻辑结果。
    async fn execute_embedding_plan_remote_durable(
        &self,
        plan: &EmbeddingPlan,
        context: &EmbeddingExecutionContext<'_>,
        control: &VceRefreshControl,
        index: &dyn VceIndexPort,
        manifest: &mut VceEmbeddingRunManifest,
    ) -> Result<Vec<SegmentVector>, VceError> {
        let request_id = context.request_id;
        let purpose = context.purpose;
        let ai = context.ai;
        let overall = context.overall;
        self.ensure_embedding_remote_batch_records(
            plan, request_id, purpose, ai, index, manifest, control, overall,
        )
        .await?;
        let mut records = index
            .load_embedding_remote_batch_records(manifest.run_id.clone(), purpose)
            .await
            .map_err(VceError::Port)?;
        let manifest_positions = manifest
            .batches
            .iter()
            .enumerate()
            .filter_map(|(position, batch)| (batch.purpose == purpose.as_str()).then_some(position))
            .collect::<Vec<_>>();
        if manifest_positions.len() != plan.batches.len() {
            return Err(VceError::InvalidRequest(format!(
                "durable Embedding manifest batch count mismatch for {}",
                purpose.as_str()
            )));
        }
        // Cache-only gaps have no remote owner; keep their validated vectors alongside task results.
        // 仅缓存命中的间隙没有远端归属；将其已校验向量与任务结果一同保留。
        let mut cached_vectors = Vec::new();
        for (logical_index, position) in manifest_positions.iter().enumerate() {
            let covered = records.iter().any(|record| {
                logical_index >= record.first_logical_batch_index as usize
                    && logical_index
                        < (record.first_logical_batch_index + record.logical_batch_count) as usize
            });
            if !covered {
                if manifest.batches[*position].state != VceEmbeddingBatchState::Succeeded {
                    return Err(VceError::InvalidRequest(String::from(
                        "Unowned remote logical batch has no successful result",
                    )));
                }
                let result = index
                    .load_embedding_batch_result(
                        manifest.run_id.clone(),
                        manifest.batches[*position].batch_id.clone(),
                    )
                    .await
                    .map_err(VceError::Port)?
                    .ok_or_else(|| {
                        VceError::InvalidRequest(String::from(
                            "Cached logical batch result is missing",
                        ))
                    })?;
                cached_vectors.extend(result.into_vectors().map_err(VceError::InvalidRequest)?);
            }
        }
        if records.is_empty() {
            return Ok(cached_vectors);
        }
        // Admit every unfinished logical batch before remote I/O so success can never skip Running.
        // 在远端调用前准入每个未完成逻辑批次，确保成功状态绝不会跳过 Running。
        observe_embedding_activity(
            control,
            request_id,
            VceEmbeddingActivityStage::Admitting,
            overall,
            0,
            Some(manifest_positions.len() as u64),
            None,
        );
        for (admitted, position) in manifest_positions.iter().enumerate() {
            control.check()?;
            if matches!(
                manifest.batches[*position].state,
                VceEmbeddingBatchState::Pending | VceEmbeddingBatchState::Failed
            ) {
                manifest.batches[*position]
                    .mark_running()
                    .map_err(VceError::InvalidRequest)?;
                manifest.updated_at_unix_ms = current_unix_epoch_millis();
                persist_manifest_batch_record(index, manifest, *position).await?;
            }
            // Advance only after the durable state write and its storage read-back have succeeded.
            // 仅在持久状态写入及存储回读成功之后推进计数。
            observe_embedding_activity(
                control,
                request_id,
                VceEmbeddingActivityStage::Admitting,
                overall,
                (admitted + 1) as u64,
                Some(manifest_positions.len() as u64),
                None,
            );
        }
        let first_capabilities = &records
            .first()
            .ok_or_else(|| {
                VceError::InvalidRequest(String::from(
                    "VCE remote embedding Batch plan contains no task records",
                ))
            })?
            .plan
            .capabilities;
        let maximum_tasks =
            usize::try_from(first_capabilities.maximum_tasks_per_run).map_err(|_| {
                VceError::InvalidRequest(String::from(
                    "VCE remote embedding Batch task limit exceeds usize",
                ))
            })?;
        if records.len() > maximum_tasks {
            return Err(VceError::InvalidRequest(String::from(
                "VCE remote embedding Batch task count exceeds the effective run limit",
            )));
        }
        if let Some(limit) = first_capabilities.provider_maximum_files {
            let limit = usize::try_from(limit).map_err(|_| {
                VceError::InvalidRequest(String::from(
                    "VCE remote embedding Batch provider file limit exceeds usize",
                ))
            })?;
            if records.len() > limit {
                return Err(VceError::InvalidRequest(String::from(
                    "VCE remote embedding Batch task count exceeds the provider file quota",
                )));
            }
        }
        let total_payload_bytes = records.iter().try_fold(0_u64, |total, record| {
            record.plan.tasks.iter().try_fold(total, |subtotal, task| {
                subtotal.checked_add(task.encoded_payload_bytes)
            })
        });
        let total_payload_bytes = total_payload_bytes.ok_or_else(|| {
            VceError::InvalidRequest(String::from(
                "VCE remote embedding Batch aggregate payload size overflowed",
            ))
        })?;
        if let Some(limit) = first_capabilities.provider_maximum_total_file_bytes
            && total_payload_bytes > limit
        {
            return Err(VceError::InvalidRequest(String::from(
                "VCE remote embedding Batch payloads exceed the provider aggregate file quota",
            )));
        }

        // Upload and submit every planned remote task before polling so provider-side work can overlap.
        // 在轮询前上传并提交全部已规划远端任务，使供应商侧工作能够并行。
        let mut last_file_operation_unix_ms =
            latest_remote_file_operation_unix_ms(records.as_slice());
        let submission_phase = async {
            let mut last_submission_unix_ms =
                latest_remote_submission_unix_ms(records.as_slice());
            // Transport progress counts remote task files, independently of their logical batch contents.
            // 传输进度统计远端任务文件，与其中包含的逻辑批次数分开。
            let remote_task_count = records.len() as u64;
            for (remote_task_index, record) in records.iter_mut().enumerate() {
                control.check()?;
                let first = usize::try_from(record.first_logical_batch_index).map_err(|_| {
                    VceError::InvalidRequest(String::from(
                        "remote embedding Batch first logical index exceeds usize",
                    ))
                })?;
                let count = usize::try_from(record.logical_batch_count).map_err(|_| {
                    VceError::InvalidRequest(String::from(
                        "remote embedding Batch logical count exceeds usize",
                    ))
                })?;
                let end = first.checked_add(count).ok_or(EmbeddingError::InvalidBudget)?;
                let grouped = merge_embedding_batches(&plan.batches[first..end])?;
                let batch_request = VceEmbeddingBatchRequest {
                    context: VceAiRequestContext::new(
                        record.plan.request_id.clone(),
                        purpose,
                        ai.binding(),
                    ),
                    batch: grouped,
                };
                let task = record.plan.tasks[0].clone();

                if record.state == VceEmbeddingRemoteBatchState::Uploading {
                    record.state = VceEmbeddingRemoteBatchState::UploadUnknown;
                    record.error = Some(String::from(
                        "process restarted while provider Batch input upload outcome was unknown",
                    ));
                    record.updated_at_unix_ms = current_unix_epoch_millis();
                    index
                        .persist_embedding_remote_batch_record(
                            manifest.run_id.clone(),
                            record.clone(),
                        )
                        .await
                        .map_err(VceError::Port)?;
                }
                if record.state == VceEmbeddingRemoteBatchState::UploadUnknown {
                    return Err(VceError::Port(VcePortError::SubmissionUnknown {
                        operation: VcePortOperation::Embedding,
                        message: record.error.clone().unwrap_or_else(|| {
                            String::from("provider Batch input upload outcome is unknown")
                        }),
                    }));
                }

                if record.state == VceEmbeddingRemoteBatchState::Submitting {
                    record.state = VceEmbeddingRemoteBatchState::SubmissionUnknown;
                    record.error = Some(String::from(
                        "process restarted while provider Batch creation outcome was unknown",
                    ));
                    record.updated_at_unix_ms = current_unix_epoch_millis();
                    index
                        .persist_embedding_remote_batch_record(
                            manifest.run_id.clone(),
                            record.clone(),
                        )
                        .await
                        .map_err(VceError::Port)?;
                }
                if record.state == VceEmbeddingRemoteBatchState::SubmissionUnknown {
                    let Some(uploaded_input) = record.uploaded_input.clone() else {
                        return Err(VceError::Port(VcePortError::SubmissionUnknown {
                            operation: VcePortOperation::Embedding,
                            message: record.error.clone().unwrap_or_else(|| {
                                String::from("provider Batch submission outcome is unknown")
                            }),
                        }));
                    };
                    wait_for_remote_rate_limit(
                        &mut last_file_operation_unix_ms,
                        remote_reconciliation_interval_ms(&record.plan.capabilities),
                        control,
                    )
                    .await?;
                    match ai
                        .reconcile_embedding_batch_submission(
                            batch_request.clone(),
                            record.plan.clone(),
                            task.clone(),
                            uploaded_input,
                        )
                        .await
                        .map_err(VceError::Port)?
                    {
                        Some(submission) => {
                            record.submission = Some(submission);
                            record.state = VceEmbeddingRemoteBatchState::Submitted;
                            record.error = None;
                            record.next_poll_not_before_unix_ms = Some(
                                current_unix_epoch_millis().saturating_add(
                                    record.plan.capabilities.minimum_poll_interval_ms,
                                ),
                            );
                            record.updated_at_unix_ms = current_unix_epoch_millis();
                            index
                                .persist_embedding_remote_batch_record(
                                    manifest.run_id.clone(),
                                    record.clone(),
                                )
                                .await
                                .map_err(VceError::Port)?;
                        }
                        None => {
                            return Err(VceError::Port(VcePortError::SubmissionUnknown {
                                operation: VcePortOperation::Embedding,
                                message: record.error.clone().unwrap_or_else(|| {
                                    String::from(
                                        "provider Batch creation remains absent during exact file reconciliation",
                                    )
                                }),
                            }));
                        }
                    }
                }

                if record.plan.capabilities.transport == VceEmbeddingBatchTransport::JsonlFile
                    && matches!(
                        record.state,
                        VceEmbeddingRemoteBatchState::Planned
                            | VceEmbeddingRemoteBatchState::UploadFailed
                    )
                {
                    wait_for_remote_rate_limit(
                        &mut last_file_operation_unix_ms,
                        remote_file_upload_interval_ms(&record.plan.capabilities),
                        control,
                    )
                    .await?;
                    observe_embedding_activity(control, request_id, VceEmbeddingActivityStage::Uploading,
                        overall, remote_task_index as u64, Some(remote_task_count), Some(record));
                    record.state = VceEmbeddingRemoteBatchState::Uploading;
                    record.error = None;
                    record.updated_at_unix_ms = current_unix_epoch_millis();
                    index
                        .persist_embedding_remote_batch_record(
                            manifest.run_id.clone(),
                            record.clone(),
                        )
                        .await
                        .map_err(VceError::Port)?;
                    match ai
                        .upload_embedding_batch_input(
                            batch_request.clone(),
                            record.plan.clone(),
                            task.clone(),
                        )
                        .await
                    {
                        Ok(uploaded_input) => {
                            record.uploaded_input = Some(uploaded_input);
                            record.state = VceEmbeddingRemoteBatchState::Uploaded;
                            record.updated_at_unix_ms = current_unix_epoch_millis();
                            index
                                .persist_embedding_remote_batch_record(
                                    manifest.run_id.clone(),
                                    record.clone(),
                                )
                                .await
                                .map_err(VceError::Port)?;
                        }
                        Err(error) => {
                            record.state = if matches!(
                                error,
                                VcePortError::SubmissionUnknown { .. }
                            ) {
                                VceEmbeddingRemoteBatchState::UploadUnknown
                            } else {
                                VceEmbeddingRemoteBatchState::UploadFailed
                            };
                            record.error = Some(bounded_batch_diagnostic(&error.to_string()));
                            record.updated_at_unix_ms = current_unix_epoch_millis();
                            let _ = index
                                .persist_embedding_remote_batch_record(
                                    manifest.run_id.clone(),
                                    record.clone(),
                                )
                                .await;
                            return Err(VceError::Port(error));
                        }
                    }
                }

                if !matches!(
                    record.state,
                    VceEmbeddingRemoteBatchState::Planned
                        | VceEmbeddingRemoteBatchState::Uploaded
                        | VceEmbeddingRemoteBatchState::SubmissionFailed
                ) {
                    continue;
                }
                if let Some(last_submission) = last_submission_unix_ms {
                    let deadline = last_submission.saturating_add(remote_submission_interval_ms(
                        &record.plan.capabilities,
                    ));
                    wait_until_remote_deadline(deadline, control).await?;
                }
                observe_embedding_activity(control, request_id, VceEmbeddingActivityStage::Submitting,
                    overall, remote_task_index as u64, Some(remote_task_count), Some(record));
                record.state = VceEmbeddingRemoteBatchState::Submitting;
                record.error = None;
                record.snapshot = None;
                record.next_poll_not_before_unix_ms = None;
                record.updated_at_unix_ms = current_unix_epoch_millis();
                index
                    .persist_embedding_remote_batch_record(
                        manifest.run_id.clone(),
                        record.clone(),
                    )
                    .await
                    .map_err(VceError::Port)?;
                match ai
                    .submit_embedding_batch(
                        batch_request,
                        record.plan.clone(),
                        task,
                        record.uploaded_input.clone(),
                    )
                    .await
                {
                    Ok(submission) => {
                        record.submission = Some(submission);
                        record.state = VceEmbeddingRemoteBatchState::Submitted;
                        record.next_poll_not_before_unix_ms = Some(
                            current_unix_epoch_millis().saturating_add(
                                record.plan.capabilities.minimum_poll_interval_ms,
                            ),
                        );
                        record.updated_at_unix_ms = current_unix_epoch_millis();
                        index
                            .persist_embedding_remote_batch_record(
                                manifest.run_id.clone(),
                                record.clone(),
                            )
                            .await
                            .map_err(VceError::Port)?;
                        last_submission_unix_ms = Some(record.updated_at_unix_ms);
                        observe_embedding_activity(control, request_id, VceEmbeddingActivityStage::Waiting,
                            overall, 0, Some(plan.batches.len() as u64), Some(record));
                    }
                    Err(error) => {
                        record.state = if matches!(error, VcePortError::SubmissionUnknown { .. }) {
                            VceEmbeddingRemoteBatchState::SubmissionUnknown
                        } else {
                            VceEmbeddingRemoteBatchState::SubmissionFailed
                        };
                        record.error = Some(bounded_batch_diagnostic(&error.to_string()));
                        record.updated_at_unix_ms = current_unix_epoch_millis();
                        let _ = index
                            .persist_embedding_remote_batch_record(
                                manifest.run_id.clone(),
                                record.clone(),
                            )
                            .await;
                        return Err(VceError::Port(error));
                    }
                }
            }
            Ok(())
        }
        .await;
        if matches!(submission_phase, Err(VceError::RefreshCancelled)) {
            self.cancel_remote_embedding_batches(
                plan,
                context,
                index,
                &manifest.run_id,
                &mut records,
            )
            .await;
        }
        submission_phase?;
        let mut completed_logical_batches = 0_u64;
        let total_provider_requests = records.iter().fold(0_u64, |total, record| {
            total.saturating_add(u64::from(record.plan.tasks[0].input_count))
        });
        // Restore every task's latest settled count so the first post-restart observation cannot
        // regress aggregate provider progress while another active task is being polled.
        // 恢复每个任务最近的已结算计数，避免重启后首次轮询其他活动任务时让供应商总进度回退。
        let mut settled_provider_requests = records
            .iter()
            .map(remote_embedding_batch_record_settled_requests)
            .collect::<Vec<_>>();
        let mut last_status_poll_unix_ms: Option<u64> = None;
        // Per-task vectors preserve frozen task order even when later provider tasks finish first.
        // 即使后续供应商任务先完成，按任务保存的向量仍保持冻结任务顺序。
        let mut task_vectors = vec![None; records.len()];
        let polling_phase = async {
        while task_vectors.iter().any(Option::is_none) {
        for (record_index, record) in records.iter_mut().enumerate() {
            if task_vectors[record_index].is_some() {
                continue;
            }
            let first = usize::try_from(record.first_logical_batch_index).map_err(|_| {
                VceError::InvalidRequest(String::from(
                    "remote embedding Batch first logical index exceeds usize",
                ))
            })?;
            let count = usize::try_from(record.logical_batch_count).map_err(|_| {
                VceError::InvalidRequest(String::from(
                    "remote embedding Batch logical count exceeds usize",
                ))
            })?;
            let end = first.checked_add(count).ok_or(EmbeddingError::InvalidBudget)?;
            if end > plan.batches.len() {
                return Err(VceError::InvalidRequest(String::from(
                    "remote embedding Batch range exceeds the logical plan",
                )));
            }
            let grouped = merge_embedding_batches(&plan.batches[first..end])?;
            let batch_request = VceEmbeddingBatchRequest {
                context: VceAiRequestContext::new(record.plan.request_id.clone(), purpose, ai.binding()),
                batch: grouped,
            };
            let task = record
                .plan
                .tasks
                .iter()
                .find(|task| task.task_id == record.remote_task_id)
                .cloned()
                .ok_or_else(|| {
                    VceError::InvalidRequest(String::from(
                        "remote embedding Batch task is absent from its frozen plan",
                    ))
                })?;

            let mut durable_logical_results = BTreeMap::new();
            for (logical_index, manifest_position) in manifest_positions
                .iter()
                .copied()
                .enumerate()
                .take(end)
                .skip(first)
            {
                let batch_id = manifest
                    .batches
                    .get(manifest_position)
                    .ok_or_else(|| {
                        VceError::InvalidRequest(String::from(
                            "durable Embedding manifest batch position is invalid",
                        ))
                    })?
                    .batch_id
                    .clone();
                if let Some(result) = index
                    .load_embedding_batch_result(manifest.run_id.clone(), batch_id)
                    .await
                    .map_err(VceError::Port)?
                {
                    let expected_batch_id = manifest
                        .batches
                        .get(manifest_position)
                        .ok_or_else(|| {
                            VceError::InvalidRequest(String::from(
                                "durable Embedding manifest batch position is invalid",
                            ))
                        })?
                        .batch_id
                        .clone();
                    if result.batch_id != expected_batch_id {
                        return Err(VceError::InvalidHostResponse {
                            operation: VcePortOperation::EmbeddingManifest,
                            reason: format!(
                                "durable Embedding result identity mismatch for {expected_batch_id}"
                            ),
                        });
                    }
                    let logical_batch = plan.batches.get(logical_index).ok_or_else(|| {
                        VceError::InvalidRequest(String::from(
                            "remote embedding Batch logical result index is invalid",
                        ))
                    })?;
                    let validation_operation_id =
                        format!("{request_id}:{logical_index}:durable-remote");
                    let vectors = validate_embedding_response(
                        self,
                        validation_operation_id.as_str(),
                        VceEmbeddingResponse {
                            operation_id: validation_operation_id.clone(),
                            vectors: result
                                .into_vectors()
                                .map_err(VceError::InvalidRequest)?,
                            space: manifest.contract.embedding_space.clone(),
                        },
                        logical_batch,
                    )?;
                    durable_logical_results.insert(logical_index, vectors);
                }
            }
            let all_results_durable = durable_logical_results.len() == count;
            if all_results_durable {
                if record.state == VceEmbeddingRemoteBatchState::Ready {
                    if record
                        .plan
                        .capabilities
                        .deletes_input_file_after_ingestion
                        && let Some(submission) = record.submission.clone()
                    {
                        wait_for_remote_rate_limit(
                            &mut last_file_operation_unix_ms,
                            remote_file_operation_interval_ms(&record.plan.capabilities),
                            control,
                        )
                        .await?;
                        if let Err(error) = ai
                            .cleanup_embedding_batch_input(
                                batch_request.clone(),
                                record.plan.clone(),
                                submission,
                            )
                            .await
                        {
                            record.cleanup_warning =
                                Some(bounded_batch_diagnostic(&error.to_string()));
                        }
                    }
                    record.state = VceEmbeddingRemoteBatchState::Succeeded;
                    record.updated_at_unix_ms = current_unix_epoch_millis();
                    index
                        .persist_embedding_remote_batch_record(
                            manifest.run_id.clone(),
                            record.clone(),
                        )
                        .await
                        .map_err(VceError::Port)?;
                } else if record.state != VceEmbeddingRemoteBatchState::Succeeded {
                    return Err(VceError::InvalidRequest(String::from(
                        "durable logical results exist before their remote Batch is ready",
                    )));
                }
                for position in manifest_positions.iter().copied().take(end).skip(first) {
                    if manifest.batches[position].state != VceEmbeddingBatchState::Succeeded {
                        manifest.batches[position].mark_succeeded();
                        manifest.updated_at_unix_ms = current_unix_epoch_millis();
                        persist_manifest_batch_record(index, manifest, position).await?;
                    }
                }
                let mut recovered_task_vectors = Vec::new();
                for logical_index in first..end {
                    recovered_task_vectors.extend(durable_logical_results.remove(&logical_index).ok_or_else(
                        || {
                            VceError::InvalidRequest(String::from(
                                "durable remote embedding Batch result map is incomplete",
                            ))
                        },
                    )?);
                }
                task_vectors[record_index] = Some(recovered_task_vectors);
                completed_logical_batches = completed_logical_batches.saturating_add(count as u64);
                settled_provider_requests[record_index] = u64::from(task.input_count);
                observe_remote_batch_progress(
                    control,
                    request_id,
                    purpose,
                    completed_logical_batches,
                    plan.batches.len() as u64,
                    overall,
                );
                continue;
            }
            if record.state == VceEmbeddingRemoteBatchState::Succeeded {
                return Err(
                    persist_remote_batch_contract_failure(
                        index,
                        manifest.run_id.as_str(),
                        record,
                        "remote embedding Batch succeeded without all durable logical results",
                    )
                    .await,
                );
            }
            loop {
                control.check()?;
                match record.state {
                    VceEmbeddingRemoteBatchState::Submitted => {
                        if let Some(snapshot) = record.snapshot.as_ref() {
                            // A persisted nonterminal response remains the authoritative waiting state on the next round.
                            // 下一轮等待时，已持久化的非终态响应仍是权威等待状态。
                            let settled = settled_provider_requests
                                .iter()
                                .copied()
                                .fold(0_u64, u64::saturating_add)
                                .min(total_provider_requests);
                            observe_remote_request_progress(
                                control,
                                request_id,
                                purpose,
                                settled,
                                total_provider_requests,
                                snapshot,
                                overall,
                                record.next_poll_not_before_unix_ms,
                            );
                        } else {
                            observe_embedding_activity(control, request_id, VceEmbeddingActivityStage::Waiting,
                                overall, completed_logical_batches, Some(plan.batches.len() as u64), Some(record));
                        }
                        wait_for_remote_poll(record, control).await?;
                        if let Some(last_poll) = last_status_poll_unix_ms {
                            wait_until_remote_deadline(
                                last_poll.saturating_add(remote_status_interval_ms(
                                    &record.plan.capabilities,
                                )),
                                control,
                            )
                            .await?;
                        }
                        let submission = record.submission.clone().ok_or_else(|| {
                            VceError::InvalidRequest(String::from(
                                "submitted remote embedding Batch has no provider identity",
                            ))
                        })?;
                        observe_embedding_activity(control, request_id, VceEmbeddingActivityStage::Polling,
                            overall, completed_logical_batches, Some(plan.batches.len() as u64), Some(record));
                        let snapshot = ai
                            .embedding_batch_status(
                                batch_request.clone(),
                                record.plan.clone(),
                                submission,
                            )
                            .await
                            .map_err(VceError::Port)?;
                        last_status_poll_unix_ms = Some(current_unix_epoch_millis());
                        record.poll_attempt = record.poll_attempt.saturating_add(1);
                        record.snapshot = Some(snapshot.clone());
                        record.updated_at_unix_ms = current_unix_epoch_millis();
                        match snapshot.state {
                            VceEmbeddingBatchProviderState::Completed => {
                                record.state = VceEmbeddingRemoteBatchState::Ready;
                                record.next_poll_not_before_unix_ms = None;
                            }
                            VceEmbeddingBatchProviderState::Failed
                            | VceEmbeddingBatchProviderState::Expired => {
                                record.state = VceEmbeddingRemoteBatchState::Failed;
                                record.next_poll_not_before_unix_ms = None;
                                record.error = Some(bounded_batch_diagnostic(
                                    snapshot.error.as_deref().unwrap_or(
                                        "provider Batch reached a failed or expired terminal state",
                                    ),
                                ));
                            }
                            VceEmbeddingBatchProviderState::Cancelled => {
                                record.state = VceEmbeddingRemoteBatchState::Cancelled;
                                record.next_poll_not_before_unix_ms = None;
                                record.error = Some(bounded_batch_diagnostic(
                                    snapshot
                                        .error
                                        .as_deref()
                                        .unwrap_or("provider Batch was cancelled"),
                                ));
                            }
                            VceEmbeddingBatchProviderState::Validating
                            | VceEmbeddingBatchProviderState::InProgress
                            | VceEmbeddingBatchProviderState::Finalizing
                            | VceEmbeddingBatchProviderState::Cancelling => {
                                // A successful nonterminal status query uses the frozen fixed interval; exponential
                                // backoff would hide real provider progress for minutes without a transport failure.
                                // 成功取得非终态后使用冻结的固定间隔；指数退避会无故掩盖数分钟的真实进度。
                                let delay = record.plan.capabilities.minimum_poll_interval_ms;
                                record.next_poll_not_before_unix_ms = Some(
                                    current_unix_epoch_millis().saturating_add(delay),
                                );
                            }
                        }
                        index
                            .persist_embedding_remote_batch_record(
                                manifest.run_id.clone(),
                                record.clone(),
                            )
                            .await
                            .map_err(VceError::Port)?;
                        settled_provider_requests[record_index] =
                            remote_embedding_batch_record_settled_requests(record);
                        let aggregate_settled_provider_requests = settled_provider_requests
                            .iter()
                            .copied()
                            .fold(0_u64, u64::saturating_add)
                            .min(total_provider_requests);
                        observe_remote_request_progress(
                            control,
                            request_id,
                            purpose,
                            aggregate_settled_provider_requests,
                            total_provider_requests,
                            &snapshot,
                            overall,
                            record.next_poll_not_before_unix_ms,
                        );
                        if record.state == VceEmbeddingRemoteBatchState::Submitted {
                            // One active status observation per task and round prevents the first slow task
                            // from hiding completion or failure of every later submitted task.
                            // 每轮每个活动任务只观测一次，避免首个慢任务遮蔽后续任务的完成或失败。
                            break;
                        }
                    }
                    VceEmbeddingRemoteBatchState::Ready => {
                        observe_embedding_activity(control, request_id, VceEmbeddingActivityStage::Collecting,
                            overall, completed_logical_batches, Some(plan.batches.len() as u64), Some(record));
                        let snapshot = record.snapshot.clone().ok_or_else(|| {
                            VceError::InvalidRequest(String::from(
                                "ready remote embedding Batch has no completed snapshot",
                            ))
                        })?;
                        if record.plan.capabilities.transport
                            == VceEmbeddingBatchTransport::JsonlFile
                        {
                            wait_for_remote_rate_limit(
                                &mut last_file_operation_unix_ms,
                                remote_file_operation_interval_ms(&record.plan.capabilities),
                                control,
                            )
                            .await?;
                        }
                        let collected = match ai
                            .collect_embedding_batch(
                                batch_request.clone(),
                                record.plan.clone(),
                                task.clone(),
                                snapshot,
                            )
                            .await
                        {
                            Ok(collected) => collected,
                            Err(error) => {
                                if !matches!(error, VcePortError::Retryable { .. }) {
                                    record.state = VceEmbeddingRemoteBatchState::Failed;
                                    record.error = Some(bounded_batch_diagnostic(&error.to_string()));
                                    record.next_poll_not_before_unix_ms = None;
                                    record.updated_at_unix_ms = current_unix_epoch_millis();
                                    let _ = index
                                        .persist_embedding_remote_batch_record(
                                            manifest.run_id.clone(),
                                            record.clone(),
                                        )
                                        .await;
                                }
                                return Err(VceError::Port(error));
                            }
                        };
                        let mut cleanup_warning = collected.cleanup_warning;
                        let collected_vector_count = collected.vectors.len();
                        let mut by_input_id = collected
                            .vectors
                            .into_iter()
                            .map(|vector| (vector.input_id.clone(), vector))
                            .collect::<BTreeMap<_, _>>();
                        let expected_input_ids = plan.batches[first..end]
                            .iter()
                            .flat_map(|batch| batch.request_input_ids())
                            .collect::<BTreeSet<_>>();
                        // Validate the complete identity set before the first logical result write.
                        // 在写入第一项逻辑结果前校验完整身份集合。
                        if by_input_id.len() != collected_vector_count
                            || expected_input_ids.len() != collected_vector_count
                            || !by_input_id.keys().eq(expected_input_ids.iter())
                        {
                            return Err(
                                persist_remote_batch_contract_failure(
                                    index,
                                    manifest.run_id.as_str(),
                                    record,
                                    "remote embedding Batch returned duplicate, missing, or unknown input identities",
                                )
                                .await,
                            );
                        }
                        let mut collected_task_vectors = Vec::new();
                        for (logical_index, position) in manifest_positions
                            .iter()
                            .copied()
                            .enumerate()
                            .take(end)
                            .skip(first)
                        {
                            let logical_batch = &plan.batches[logical_index];
                            let mut logical_vectors = Vec::with_capacity(logical_batch.segments.len());
                            for input_id in logical_batch.request_input_ids() {
                                logical_vectors.push(by_input_id.remove(&input_id).ok_or_else(|| {
                                    VceError::InvalidHostResponse {
                                        operation: VcePortOperation::Embedding,
                                        reason: format!(
                                            "remote embedding Batch omitted input `{input_id}`"
                                        ),
                                    }
                                })?);
                            }
                            let batch_id = manifest.batches[position].batch_id.clone();
                            let result =
                                VceEmbeddingBatchResult::from_vectors(batch_id, &logical_vectors);
                            if let Some(durable_vectors) =
                                durable_logical_results.remove(&logical_index)
                            {
                                if durable_vectors != logical_vectors {
                                    return Err(
                                        persist_remote_batch_contract_failure(
                                            index,
                                            manifest.run_id.as_str(),
                                            record,
                                            "remote embedding Batch output changed after a partial durable write",
                                        )
                                        .await,
                                    );
                                }
                            } else {
                                index
                                    .persist_embedding_batch_result(manifest.run_id.clone(), result)
                                    .await
                                    .map_err(VceError::Port)?;
                            }
                            manifest.batches[position].mark_succeeded();
                            manifest.updated_at_unix_ms = current_unix_epoch_millis();
                            persist_manifest_batch_record(index, manifest, position).await?;
                            collected_task_vectors.extend(logical_vectors);
                        }
                        if !by_input_id.is_empty() {
                            return Err(
                                persist_remote_batch_contract_failure(
                                    index,
                                    manifest.run_id.as_str(),
                                    record,
                                    "remote embedding Batch returned inputs outside its logical range",
                                )
                                .await,
                            );
                        }
                        if record
                            .plan
                            .capabilities
                            .deletes_input_file_after_ingestion
                            && let Some(submission) = record.submission.clone()
                        {
                            wait_for_remote_rate_limit(
                                &mut last_file_operation_unix_ms,
                                remote_file_operation_interval_ms(&record.plan.capabilities),
                                control,
                            )
                            .await?;
                            if let Err(error) = ai
                                .cleanup_embedding_batch_input(
                                    batch_request.clone(),
                                    record.plan.clone(),
                                    submission,
                                )
                                .await
                            {
                                cleanup_warning =
                                    Some(bounded_batch_diagnostic(&error.to_string()));
                            }
                        }
                        record.cleanup_warning = cleanup_warning;
                        record.state = VceEmbeddingRemoteBatchState::Succeeded;
                        record.updated_at_unix_ms = current_unix_epoch_millis();
                        index
                            .persist_embedding_remote_batch_record(
                                manifest.run_id.clone(),
                                record.clone(),
                            )
                            .await
                            .map_err(VceError::Port)?;
                        task_vectors[record_index] = Some(collected_task_vectors);
                        break;
                    }
                    VceEmbeddingRemoteBatchState::Failed
                    | VceEmbeddingRemoteBatchState::Cancelled => {
                        return Err(VceError::InvalidRequest(record.error.clone().unwrap_or_else(
                            || String::from("remote embedding Batch reached a terminal state"),
                        )));
                    }
                    VceEmbeddingRemoteBatchState::Succeeded => break,
                    VceEmbeddingRemoteBatchState::Planned
                    | VceEmbeddingRemoteBatchState::Uploading
                    | VceEmbeddingRemoteBatchState::UploadUnknown
                    | VceEmbeddingRemoteBatchState::UploadFailed
                    | VceEmbeddingRemoteBatchState::Uploaded
                    | VceEmbeddingRemoteBatchState::Submitting
                    | VceEmbeddingRemoteBatchState::SubmissionUnknown
                    | VceEmbeddingRemoteBatchState::SubmissionFailed => {
                        return Err(VceError::InvalidRequest(String::from(
                            "remote embedding Batch entered an invalid execution state",
                        )));
                    }
                }
            }
            // Leaving the inner loop after a nonterminal poll does not complete any logical batch.
            // 非终态轮询退出内层循环时，没有任何逻辑批次完成。
            if task_vectors[record_index].is_some() {
                settled_provider_requests[record_index] = u64::from(task.input_count);
                completed_logical_batches = completed_logical_batches.saturating_add(count as u64);
                observe_remote_batch_progress(
                    control,
                    request_id,
                    purpose,
                    completed_logical_batches,
                    plan.batches.len() as u64,
                    overall,
                );
            }
        }
        }
        let mut vectors = cached_vectors;
        for task in task_vectors {
            let task = task.ok_or_else(|| {
                VceError::InvalidRequest(String::from(
                    "remote embedding Batch result order is incomplete",
                ))
            })?;
            vectors.extend(task);
        }
        // Reconstruct input order across remote ranges and cache gaps, rejecting missing or duplicate IDs.
        // 跨远端范围与缓存间隙重建输入顺序，拒绝缺失或重复 ID。
        let vector_count = vectors.len();
        let mut by_id = vectors.into_iter().map(|vector|(vector.input_id.clone(),vector)).collect::<BTreeMap<_,_>>();
        if by_id.len() != vector_count { return Err(VceError::InvalidRequest(String::from("Duplicate remote embedding result identity"))); }
        let mut ordered = Vec::with_capacity(vector_count);
        for batch in &plan.batches {
            for id in batch.request_input_ids() {
                ordered.push(by_id.remove(&id).ok_or_else(||VceError::InvalidRequest(format!("Missing embedding result {id}")))?);
            }
        }
        if !by_id.is_empty() { return Err(VceError::InvalidRequest(String::from("Unexpected remote embedding result identity"))); }
        Ok(ordered)
        }
        .await;
        if matches!(polling_phase, Err(VceError::RefreshCancelled))
            || remote_embedding_batch_run_has_terminal_failure(&records)
        {
            self.cancel_remote_embedding_batches(
                plan,
                context,
                index,
                &manifest.run_id,
                &mut records,
            )
            .await;
        }
        polling_phase
    }

    /// Best-effort cancel every submitted remote task owned by one frozen logical plan.
    /// 尽力取消一份冻结逻辑计划持有的每个已提交远端任务。
    async fn cancel_remote_embedding_batches(
        &self,
        plan: &EmbeddingPlan,
        context: &EmbeddingExecutionContext<'_>,
        index: &dyn VceIndexPort,
        run_id: &str,
        records: &mut [VceEmbeddingRemoteBatchRecord],
    ) {
        let purpose = context.purpose;
        let ai = context.ai;
        let mut last_cancel_unix_ms: Option<u64> = None;
        for record in records {
            // Settled or non-cancellable tasks must not consume the provider cancel-rate budget.
            // 已结算或不可取消的任务不得消耗供应商取消限速预算。
            if !remote_embedding_batch_can_cancel(record) {
                continue;
            }
            let Ok(first) = usize::try_from(record.first_logical_batch_index) else {
                continue;
            };
            let Ok(count) = usize::try_from(record.logical_batch_count) else {
                continue;
            };
            let Some(end) = first.checked_add(count) else {
                continue;
            };
            let Some(logical_batches) = plan.batches.get(first..end) else {
                continue;
            };
            let Ok(grouped) = merge_embedding_batches(logical_batches) else {
                continue;
            };
            let request = VceEmbeddingBatchRequest {
                context: VceAiRequestContext::new(
                    record.plan.request_id.clone(),
                    purpose,
                    ai.binding(),
                ),
                batch: grouped,
            };
            if let Some(last_cancel) = last_cancel_unix_ms {
                let deadline = last_cancel
                    .saturating_add(remote_cancel_interval_ms(&record.plan.capabilities));
                let _ = wait_until_remote_deadline(deadline, &VceRefreshControl::default()).await;
            }
            self.cancel_remote_embedding_batch(request, ai, index, run_id, record)
                .await;
            last_cancel_unix_ms = Some(current_unix_epoch_millis());
        }
    }

    /// Best-effort remote cancellation before returning the host cancellation signal.
    /// 在返回宿主取消信号前尽力取消远端任务。
    async fn cancel_remote_embedding_batch(
        &self,
        request: VceEmbeddingBatchRequest,
        ai: &dyn VceAiPort,
        index: &dyn VceIndexPort,
        run_id: &str,
        record: &mut VceEmbeddingRemoteBatchRecord,
    ) {
        // Only a provider task that is still active can be cancelled. Ready or succeeded tasks
        // already completed upstream and must never be downgraded by a late host cancellation.
        // 仅仍处于供应商活动态的任务可以取消；就绪或成功任务已在上游完成，绝不能被迟到的宿主取消降级。
        if !remote_embedding_batch_can_cancel(record) {
            return;
        }
        let Some(submission) = record.submission.clone() else {
            return;
        };
        if let Ok(snapshot) = ai
            .cancel_embedding_batch(request, record.plan.clone(), submission)
            .await
        {
            record.poll_attempt = record.poll_attempt.saturating_add(1);
            record.snapshot = Some(snapshot.clone());
            record.updated_at_unix_ms = current_unix_epoch_millis();
            match snapshot.state {
                VceEmbeddingBatchProviderState::Completed => {
                    record.state = VceEmbeddingRemoteBatchState::Ready;
                    record.error = None;
                    record.next_poll_not_before_unix_ms = None;
                }
                VceEmbeddingBatchProviderState::Failed
                | VceEmbeddingBatchProviderState::Expired => {
                    record.state = VceEmbeddingRemoteBatchState::Failed;
                    record.error = Some(bounded_batch_diagnostic(
                        snapshot
                            .error
                            .as_deref()
                            .unwrap_or("provider Batch failed while cancellation was requested"),
                    ));
                    record.next_poll_not_before_unix_ms = None;
                }
                VceEmbeddingBatchProviderState::Cancelled => {
                    record.state = VceEmbeddingRemoteBatchState::Cancelled;
                    record.error = Some(String::from("remote embedding Batch cancelled by host"));
                    record.next_poll_not_before_unix_ms = None;
                }
                VceEmbeddingBatchProviderState::Validating
                | VceEmbeddingBatchProviderState::InProgress
                | VceEmbeddingBatchProviderState::Finalizing
                | VceEmbeddingBatchProviderState::Cancelling => {
                    record.state = VceEmbeddingRemoteBatchState::Submitted;
                    record.error = None;
                    record.next_poll_not_before_unix_ms = Some(
                        record
                            .updated_at_unix_ms
                            .saturating_add(record.plan.capabilities.minimum_poll_interval_ms),
                    );
                }
            }
            let _ = index
                .persist_embedding_remote_batch_record(run_id.to_owned(), record.clone())
                .await;
        }
    }

    /// Execute one durable synchronous or asynchronous plan and retain every validated result.
    /// 执行一份可恢复的同步或异步计划，并保留每个已校验结果。
    async fn execute_embedding_plan_durable(
        &self,
        plan: &EmbeddingPlan,
        context: EmbeddingExecutionContext<'_>,
        control: &VceRefreshControl,
        index: &dyn VceIndexPort,
        manifest: &mut VceEmbeddingRunManifest,
    ) -> Result<Vec<SegmentVector>, VceError> {
        if !context.capability.registered || !context.capability.embedding_available {
            return Err(VceError::AiCapabilityUnavailable {
                purpose: context.purpose,
            });
        }
        plan.validate()?;
        if !context.use_remote_batch
            && !index
                .load_embedding_remote_batch_records(manifest.run_id.clone(), context.purpose)
                .await
                .map_err(VceError::Port)?
                .is_empty()
        {
            return Err(VceError::InvalidRequest(String::from(
                "a durable remote embedding Batch plan cannot be downgraded to synchronous execution",
            )));
        }
        if context.use_remote_batch {
            return self
                .execute_embedding_plan_remote_durable(plan, &context, control, index, manifest)
                .await;
        }
        let EmbeddingExecutionContext {
            request_id,
            purpose,
            ai,
            capability: _,
            use_remote_batch: _,
            overall,
        } = context;
        let manifest_positions = manifest
            .batches
            .iter()
            .enumerate()
            .filter_map(|(position, batch)| (batch.purpose == purpose.as_str()).then_some(position))
            .collect::<Vec<_>>();
        if manifest_positions.len() != plan.batches.len() {
            return Err(VceError::InvalidRequest(format!(
                "durable Embedding manifest batch count mismatch for {}",
                purpose.as_str()
            )));
        }
        let mut completed_batches = 0_u64;
        let mut vectors = Vec::new();
        for (batch_index, batch) in plan.batches.iter().enumerate() {
            control.check()?;
            let manifest_position = manifest_positions[batch_index];
            let expected_input_ids = batch.request_input_ids();
            let (batch_id, batch_state, actual_input_ids, actual_total_chars) = {
                let manifest_batch = manifest.batches.get(manifest_position).ok_or_else(|| {
                    VceError::InvalidRequest(
                        "durable Embedding manifest batch position is invalid".to_owned(),
                    )
                })?;
                (
                    manifest_batch.batch_id.clone(),
                    manifest_batch.state,
                    manifest_batch.input_ids.clone(),
                    manifest_batch.total_chars,
                )
            };
            if actual_input_ids != expected_input_ids || actual_total_chars != batch.total_chars {
                return Err(VceError::InvalidRequest(format!(
                    "durable Embedding manifest batch {} no longer matches the frozen plan",
                    batch_id
                )));
            }
            let operation_id = format!("{request_id}:{batch_index}");
            let recovered = match index
                .load_embedding_batch_result(manifest.run_id.clone(), batch_id.clone())
                .await
                .map_err(VceError::Port)?
            {
                Some(result) => {
                    if result.batch_id != batch_id {
                        return Err(VceError::InvalidHostResponse {
                            operation: VcePortOperation::EmbeddingManifest,
                            reason: format!(
                                "durable Embedding result identity mismatch for {}",
                                batch_id
                            ),
                        });
                    }
                    let stored_vectors = result.into_vectors().map_err(VceError::InvalidRequest)?;
                    let recovered = validate_embedding_response(
                        self,
                        &operation_id,
                        VceEmbeddingResponse {
                            operation_id: operation_id.clone(),
                            vectors: stored_vectors,
                            space: manifest.contract.embedding_space.clone(),
                        },
                        batch,
                    )?;
                    if batch_state != VceEmbeddingBatchState::Succeeded {
                        // A durable result is authoritative even when the following manifest write was interrupted.
                        // 即使后续清单写入被中断，已持久化的结果仍是权威事实。
                        manifest
                            .batches
                            .get_mut(manifest_position)
                            .ok_or_else(|| {
                                VceError::InvalidRequest(
                                    "durable Embedding manifest batch position is invalid"
                                        .to_owned(),
                                )
                            })?
                            .mark_succeeded();
                        manifest.updated_at_unix_ms = current_unix_epoch_millis();
                        persist_manifest_batch_record(index, manifest, manifest_position).await?;
                    }
                    Some(recovered)
                }
                None if batch_state == VceEmbeddingBatchState::Succeeded => {
                    if let Some(manifest_batch) = manifest.batches.get_mut(manifest_position) {
                        manifest_batch.state = VceEmbeddingBatchState::Pending;
                        manifest_batch.error = None;
                    } else {
                        return Err(VceError::InvalidRequest(
                            "durable Embedding manifest batch position is invalid".to_owned(),
                        ));
                    }
                    manifest.updated_at_unix_ms = current_unix_epoch_millis();
                    persist_manifest_batch_record(index, manifest, manifest_position).await?;
                    None
                }
                None => None,
            };
            let batch_vectors = if let Some(recovered) = recovered {
                recovered
            } else {
                manifest
                    .batches
                    .get_mut(manifest_position)
                    .ok_or_else(|| {
                        VceError::InvalidRequest(
                            "durable Embedding manifest batch position is invalid".to_owned(),
                        )
                    })?
                    .mark_running()
                    .map_err(VceError::InvalidRequest)?;
                manifest.updated_at_unix_ms = current_unix_epoch_millis();
                persist_manifest_batch_record(index, manifest, manifest_position).await?;
                observe_embedding_activity(
                    control,
                    request_id,
                    VceEmbeddingActivityStage::Requesting,
                    overall,
                    completed_batches,
                    Some(plan.batches.len() as u64),
                    None,
                );
                let response = match ai
                    .embed(VceEmbeddingRequest {
                        context: VceAiRequestContext::new(
                            operation_id.clone(),
                            purpose,
                            ai.binding(),
                        ),
                        batch: batch.clone(),
                    })
                    .await
                {
                    Ok(response) => response,
                    Err(error) => {
                        if let Some(manifest_batch) = manifest.batches.get_mut(manifest_position) {
                            manifest_batch.mark_failed(error.to_string());
                        } else {
                            return Err(VceError::InvalidRequest(
                                "durable Embedding manifest batch position is invalid".to_owned(),
                            ));
                        }
                        manifest.updated_at_unix_ms = current_unix_epoch_millis();
                        let _ =
                            persist_manifest_batch_record(index, manifest, manifest_position).await;
                        return Err(VceError::Port(error));
                    }
                };
                let response_vectors =
                    match validate_embedding_response(self, &operation_id, response, batch) {
                        Ok(vectors) => vectors,
                        Err(error) => {
                            if let Some(manifest_batch) =
                                manifest.batches.get_mut(manifest_position)
                            {
                                manifest_batch.mark_failed(error.to_string());
                            } else {
                                return Err(VceError::InvalidRequest(
                                    "durable Embedding manifest batch position is invalid"
                                        .to_owned(),
                                ));
                            }
                            manifest.updated_at_unix_ms = current_unix_epoch_millis();
                            let _ =
                                persist_manifest_batch_record(index, manifest, manifest_position)
                                    .await;
                            return Err(error);
                        }
                    };
                let result = VceEmbeddingBatchResult::from_vectors(&batch_id, &response_vectors);
                result.validate().map_err(VceError::InvalidRequest)?;
                index
                    .persist_embedding_batch_result(manifest.run_id.clone(), result)
                    .await
                    .map_err(VceError::Port)?;
                manifest
                    .batches
                    .get_mut(manifest_position)
                    .ok_or_else(|| {
                        VceError::InvalidRequest(
                            "durable Embedding manifest batch position is invalid".to_owned(),
                        )
                    })?
                    .mark_succeeded();
                manifest.updated_at_unix_ms = current_unix_epoch_millis();
                persist_manifest_batch_record(index, manifest, manifest_position).await?;
                response_vectors
            };
            vectors.extend(batch_vectors);
            completed_batches = completed_batches.saturating_add(1);
            control.observe(VceRefreshProgress {
                activity: None,
                phase: VceRefreshPhase::Embedding,
                operation_id: format!("{request_id}:{}", purpose.as_str()),
                unit: VceRefreshUnit::Batches,
                completed: completed_batches,
                total: Some(plan.batches.len() as u64),
                // Child batch completions cannot advance the Blob-based whole-refresh counter.
                // 子批次完成量不能推进以 Blob 为单位的完整刷新计数。
                overall_completed: overall.map_or(completed_batches, |(base, _)| base),
                overall_total: overall.map(|(_, total)| total),
                provider_batch_state: None,
                current_path: None,
            });
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
            .execute_embedding_plan(
                &plan,
                EmbeddingExecutionContext {
                    request_id: &embedding_request_id,
                    purpose,
                    ai,
                    capability,
                    use_remote_batch: false,
                    overall: None,
                },
                None,
            )
            .await?;
        let mut vectors = plan.aggregate(segments)?;
        vectors.pop().ok_or_else(|| VceError::InvalidHostResponse {
            operation: VcePortOperation::Embedding,
            reason: "query Embedding returned no logical vector".to_owned(),
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
                // This branch already proved a committed checkpoint; explicit empty scopes still fail below.
                // 此分支已证明存在已提交检查点；明确传入的空范围仍在下方失败。
                ScopeResolution::Empty => return Ok(BTreeSet::new()),
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

/// Resolve exact staging records without taking ownership of their source content.
/// 解析精确暂存记录，不取得其源码正文所有权。
/// `records` comes from the ownership-validated workspace; `blob_names` is the exact requested set.
/// `records` 来自归属已校验的工作区；`blob_names` 是精确请求集合。
/// `control` retains this refresh's cancellation; returns borrowed records in Blob order or an explicit error.
/// `control` 保留本次刷新取消；按块顺序返回借用记录，或返回明确错误。
fn staging_records<'a>(
    records: impl Iterator<Item = &'a FileRecord>,
    blob_names: &BTreeSet<String>,
    control: &VceRefreshControl,
) -> Result<Vec<&'a FileRecord>, VceError> {
    control.check()?;
    if blob_names.is_empty() {
        return Ok(Vec::new());
    }
    // Workspace reconciliation already proved unique ownership; retain only requested record references.
    // 工作区协调已证明唯一归属；仅保留请求记录的引用。
    let mut selected = BTreeMap::new();
    for record in records {
        control.check()?;
        if let Some(blob_name) = record.blob_name.as_deref()
            && blob_names.contains(blob_name)
        {
            selected.insert(blob_name, record);
            if selected.len() == blob_names.len() {
                break;
            }
        }
    }
    // Requested Blob order preserves batch and child-operation identities without another workspace scan.
    // 请求块顺序保留批次和子操作身份，不再次扫描工作区。
    blob_names
        .iter()
        .map(|blob_name| {
            control.check()?;
            selected.get(blob_name.as_str()).copied().ok_or_else(|| {
                VceError::InvalidRequest(format!(
                    "workspace has no present record for added Blob `{blob_name}`"
                ))
            })
        })
        .collect()
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

/// Merge complete logical batches into one provider request without changing segment order.
/// 在不改变分段顺序的前提下把完整逻辑批次合并为一个供应商请求。
fn merge_embedding_batches(
    batches: &[crate::embedding::EmbeddingBatch],
) -> Result<crate::embedding::EmbeddingBatch, VceError> {
    if batches.is_empty() {
        return Err(VceError::Embedding(EmbeddingError::InvalidPlan {
            reason: String::from("cannot merge an empty logical embedding batch range"),
        }));
    }
    let mut segments = Vec::new();
    let mut total_chars = 0_usize;
    for batch in batches {
        batch.validate()?;
        total_chars = total_chars
            .checked_add(batch.total_chars)
            .ok_or(EmbeddingError::InvalidBudget)?;
        segments.extend(batch.segments.iter().cloned());
    }
    let merged = crate::embedding::EmbeddingBatch {
        segments,
        total_chars,
    };
    merged.validate()?;
    Ok(merged)
}

/// Wait until a durable provider poll deadline while checking cancellation at least once per second.
/// 等待持久供应商轮询截止时间，并至少每秒检查一次取消信号。
async fn wait_for_remote_poll(
    record: &VceEmbeddingRemoteBatchRecord,
    control: &VceRefreshControl,
) -> Result<(), VceError> {
    let Some(deadline) = record.next_poll_not_before_unix_ms else {
        return Ok(());
    };
    wait_until_remote_deadline(deadline, control).await
}

/// Wait for one Unix-millisecond deadline while retaining responsive cancellation.
/// 等待一个 Unix 毫秒截止时间，同时保持取消操作响应性。
async fn wait_until_remote_deadline(
    deadline: u64,
    control: &VceRefreshControl,
) -> Result<(), VceError> {
    loop {
        control.check()?;
        let now = current_unix_epoch_millis();
        if now >= deadline {
            return Ok(());
        }
        sleep(Duration::from_millis(
            deadline.saturating_sub(now).min(1_000),
        ))
        .await;
    }
}

/// Apply one shared per-run provider rate interval before dispatching an external operation.
/// 在分派外部操作前应用一次运行内共享供应商速率间隔。
///
/// # Parameters
/// 参数
/// `last_dispatch_unix_ms` records the preceding operation, `interval_ms` is the declared spacing,
/// and `control` retains cooperative cancellation while waiting.
/// `last_dispatch_unix_ms` 记录上一次操作，`interval_ms` 是声明间隔，`control` 在等待时保持协作取消。
///
/// # Returns
/// 返回值
/// Unit after the next dispatch slot is reserved, or the current cancellation error.
/// 预留下一次分派时隙后返回空值，否则返回当前取消错误。
async fn wait_for_remote_rate_limit(
    last_dispatch_unix_ms: &mut Option<u64>,
    interval_ms: u64,
    control: &VceRefreshControl,
) -> Result<(), VceError> {
    if let Some(last_dispatch) = *last_dispatch_unix_ms {
        wait_until_remote_deadline(last_dispatch.saturating_add(interval_ms), control).await?;
    }
    *last_dispatch_unix_ms = Some(current_unix_epoch_millis());
    Ok(())
}

/// Derive the aggregate provider file-operation interval used by upload, download, and cleanup.
/// 推导上传、下载与清理共同使用的供应商文件接口总间隔。
fn remote_file_operation_interval_ms(
    capabilities: &crate::ports::VceEmbeddingBatchCapabilities,
) -> u64 {
    capabilities
        .provider_file_operation_qps
        .map(|qps| 1_000_u64.div_ceil(u64::from(qps)))
        .unwrap_or(0)
}

/// Derive the stricter interval for provider input-file uploads.
/// 推导供应商输入文件上传使用的更严格间隔。
fn remote_file_upload_interval_ms(
    capabilities: &crate::ports::VceEmbeddingBatchCapabilities,
) -> u64 {
    remote_file_operation_interval_ms(capabilities).max(
        capabilities
            .provider_file_upload_qps
            .map(|qps| 1_000_u64.div_ceil(u64::from(qps)))
            .unwrap_or(0),
    )
}

/// Derive the stricter interval for exact input-file reconciliation list requests.
/// 推导精确输入文件对账列表请求使用的更严格间隔。
fn remote_reconciliation_interval_ms(
    capabilities: &crate::ports::VceEmbeddingBatchCapabilities,
) -> u64 {
    remote_file_operation_interval_ms(capabilities).max(
        capabilities
            .provider_task_list_rpm
            .map(|rpm| 60_000_u64.div_ceil(u64::from(rpm)))
            .unwrap_or(0),
    )
}

/// Derive a conservative per-run submission interval from declared provider file QPS limits.
/// 根据供应商声明的文件 QPS 限制推导保守的单运行提交间隔。
fn remote_submission_interval_ms(
    capabilities: &crate::ports::VceEmbeddingBatchCapabilities,
) -> u64 {
    [
        capabilities.provider_file_upload_qps,
        capabilities.provider_file_operation_qps,
    ]
    .into_iter()
    .flatten()
    .map(|qps| 1_000_u64.div_ceil(u64::from(qps)))
    .max()
    .unwrap_or(0)
    .max(
        capabilities
            .provider_task_create_rpm
            .map(|rpm| 60_000_u64.div_ceil(u64::from(rpm)))
            .unwrap_or(0),
    )
}

/// Derive a per-run interval from the provider Batch-status requests-per-minute limit.
/// 根据供应商 Batch 状态接口每分钟请求限制推导单运行间隔。
fn remote_status_interval_ms(capabilities: &crate::ports::VceEmbeddingBatchCapabilities) -> u64 {
    capabilities
        .provider_task_status_rpm
        .map(|rpm| 60_000_u64.div_ceil(u64::from(rpm)))
        .unwrap_or(0)
}

/// Derive a per-run interval from the provider Batch-cancel requests-per-minute limit.
/// 根据供应商 Batch 取消接口每分钟请求限制推导单运行间隔。
fn remote_cancel_interval_ms(capabilities: &crate::ports::VceEmbeddingBatchCapabilities) -> u64 {
    capabilities
        .provider_task_cancel_rpm
        .map(|rpm| 60_000_u64.div_ceil(u64::from(rpm)))
        .unwrap_or(0)
}

/// Returns whether one durable remote task can consume a provider cancellation call.
/// 返回一条持久远端任务是否可以消耗一次供应商取消调用。
///
/// # Parameters
/// 参数
/// `record` is the exact durable task considered by host cancellation.
/// `record` 是宿主取消流程正在考虑的精确持久任务。
///
/// # Returns
/// 返回值
/// `true` only for an active submitted task with declared cancellation support and identity.
/// 仅对已声明取消能力且持有提交身份的活动已提交任务返回 `true`。
fn remote_embedding_batch_can_cancel(record: &VceEmbeddingRemoteBatchRecord) -> bool {
    record.plan.capabilities.supports_cancel
        && record.state == VceEmbeddingRemoteBatchState::Submitted
        && record.submission.is_some()
}

/// Returns whether one remote run has reached a durable terminal failure that should cancel peers.
/// 返回一个远端运行是否已达到应取消兄弟任务的持久失败终态。
///
/// # Parameters
/// 参数
/// `records` contains the latest durable local state for every remote task in the run.
/// `records` 包含本次运行每个远端任务最新的持久本地状态。
///
/// # Returns
/// 返回值
/// `true` only after a provider or contract failure is durably terminal; transient port failures
/// leave every record active and therefore remain resumable.
/// 仅在供应商或契约失败已持久化为终态后返回 `true`；瞬时端口失败保持全部记录活动，因而仍可恢复。
fn remote_embedding_batch_run_has_terminal_failure(
    records: &[VceEmbeddingRemoteBatchRecord],
) -> bool {
    records.iter().any(|record| {
        matches!(
            record.state,
            VceEmbeddingRemoteBatchState::Failed | VceEmbeddingRemoteBatchState::Cancelled
        )
    })
}

/// Returns the latest bounded settled-request count represented by one durable remote task.
/// 返回一项持久远端任务表示的最近有界已结算请求数量。
///
/// # Parameters
/// 参数
/// `record` contains the frozen task size, local lifecycle, and latest provider snapshot.
/// `record` 包含冻结任务大小、本地生命周期及最近供应商快照。
///
/// # Returns
/// 返回值
/// A count capped by the frozen task size; ready and succeeded tasks are fully settled.
/// 返回不超过冻结任务大小的计数；就绪及成功任务视为全部结算。
fn remote_embedding_batch_record_settled_requests(record: &VceEmbeddingRemoteBatchRecord) -> u64 {
    let planned = record
        .plan
        .tasks
        .first()
        .map_or(0, |task| u64::from(task.input_count));
    if matches!(
        record.state,
        VceEmbeddingRemoteBatchState::Ready | VceEmbeddingRemoteBatchState::Succeeded
    ) {
        return planned;
    }
    record
        .snapshot
        .as_ref()
        .map(|snapshot| {
            u64::from(snapshot.completed_requests.unwrap_or(0))
                .saturating_add(u64::from(snapshot.failed_requests.unwrap_or(0)))
        })
        .unwrap_or(0)
        .min(planned)
}

/// Restores the latest durable file-operation clock after process restart.
/// 在进程重启后恢复最近一次持久文件操作时钟。
///
/// # Parameters
/// 参数
/// `records` contains the complete ordered remote-task plan for one VCE purpose.
/// `records` 包含一个 VCE 用途的完整有序远端任务计划。
///
/// # Returns
/// 返回值
/// The latest conservative file-operation timestamp, or `None` before any file I/O began.
/// 返回最近的保守文件操作时间戳；尚未开始文件 I/O 时返回 `None`。
fn latest_remote_file_operation_unix_ms(records: &[VceEmbeddingRemoteBatchRecord]) -> Option<u64> {
    records
        .iter()
        .filter(|record| {
            record.plan.capabilities.transport == VceEmbeddingBatchTransport::JsonlFile
                && record.state != VceEmbeddingRemoteBatchState::Planned
        })
        .map(|record| {
            record
                .uploaded_input
                .as_ref()
                .map_or(record.updated_at_unix_ms, |uploaded| {
                    uploaded.uploaded_at_unix_ms.max(record.updated_at_unix_ms)
                })
        })
        .max()
}

/// Restores the latest durable Batch-creation clock after process restart.
/// 在进程重启后恢复最近一次持久 Batch 创建时钟。
///
/// # Parameters
/// 参数
/// `records` contains the complete ordered remote-task plan for one VCE purpose.
/// `records` 包含一个 VCE 用途的完整有序远端任务计划。
///
/// # Returns
/// 返回值
/// The latest conservative creation timestamp, or `None` before any creation attempt began.
/// 返回最近的保守创建时间戳；尚未开始创建尝试时返回 `None`。
fn latest_remote_submission_unix_ms(records: &[VceEmbeddingRemoteBatchRecord]) -> Option<u64> {
    records
        .iter()
        .filter_map(|record| {
            record
                .submission
                .as_ref()
                .map(|submission| {
                    submission
                        .submitted_at_unix_ms
                        .max(record.updated_at_unix_ms)
                })
                .or_else(|| {
                    matches!(
                        record.state,
                        VceEmbeddingRemoteBatchState::Submitting
                            | VceEmbeddingRemoteBatchState::SubmissionUnknown
                            | VceEmbeddingRemoteBatchState::SubmissionFailed
                    )
                    .then_some(record.updated_at_unix_ms)
                })
        })
        .max()
}

/// Observe completed source preparation with a stable operation and a frozen file denominator.
/// 使用稳定操作及冻结文件分母报告已完成源码准备。
/// The control receives confirmed completed/total files and staged writes for refresh_id; returns nothing.
/// control 接收 refresh_id 下已确认完成／总文件数及已暂存写入数；无返回值。
fn observe_prepared_files(
    control: &VceRefreshControl,
    refresh_id: &str,
    completed: u64,
    total: u64,
    staged: u64,
    current_path: &str,
) {
    control.observe(VceRefreshProgress {
        activity: None,
        phase: VceRefreshPhase::Chunking,
        operation_id: format!("{refresh_id}:prepare"),
        unit: VceRefreshUnit::Files,
        completed,
        total: Some(total),
        overall_completed: staged,
        overall_total: Some(total),
        provider_batch_state: None,
        current_path: Some(current_path.to_owned()),
    });
}

/// Publish the exact awaited embedding action before entering network or durable planning work.
/// 在进入网络或持久规划工作之前发布精确等待的向量动作。
/// Counters describe confirmed work in this stage; record supplies only acknowledged provider metadata.
/// 计数描述此阶段已确认的工作；record 仅提供已确认的供应商元数据。
fn observe_embedding_activity(
    control: &VceRefreshControl,
    request_id: &str,
    stage: VceEmbeddingActivityStage,
    overall: Option<(u64, u64)>,
    completed: u64,
    total: Option<u64>,
    record: Option<&VceEmbeddingRemoteBatchRecord>,
) {
    control.observe(VceRefreshProgress {
        activity: Some(VceEmbeddingActivity {
            stage,
            provider_batch_id: record
                .and_then(|record| record.submission.as_ref())
                .map(|submission| submission.provider_batch_id.clone()),
            next_poll_at_unix_ms: record.and_then(|record| record.next_poll_not_before_unix_ms),
        }),
        phase: VceRefreshPhase::Embedding,
        operation_id: request_id.to_owned(),
        unit: if total.is_none() {
            VceRefreshUnit::None
        } else if stage == VceEmbeddingActivityStage::Uploading {
            VceRefreshUnit::Files
        } else {
            VceRefreshUnit::Batches
        },
        completed,
        total,
        overall_completed: overall.map_or(0, |(completed, _)| completed),
        overall_total: overall.map(|(_, total)| total),
        provider_batch_state: None,
        current_path: None,
    });
}

/// Publish remote Batch progress in the existing VCE refresh progress contract.
/// 通过现有 VCE 刷新进度契约发布远端 Batch 进度。
fn observe_remote_batch_progress(
    control: &VceRefreshControl,
    request_id: &str,
    purpose: VceAiPurpose,
    completed: u64,
    total: u64,
    overall: Option<(u64, u64)>,
) {
    control.observe(VceRefreshProgress {
        activity: None,
        phase: VceRefreshPhase::Embedding,
        operation_id: format!("{request_id}:{}", purpose.as_str()),
        unit: VceRefreshUnit::Batches,
        completed,
        total: Some(total),
        overall_completed: overall.map_or(completed, |(base, _)| base),
        overall_total: overall.map(|(_, all)| all),
        provider_batch_state: None,
        current_path: None,
    });
}

/// Publish provider-reported request counts while one durable remote task is running.
/// 在一个持久远端任务运行期间发布供应商报告的请求计数。
#[allow(clippy::too_many_arguments)]
fn observe_remote_request_progress(
    control: &VceRefreshControl,
    request_id: &str,
    purpose: VceAiPurpose,
    completed: u64,
    total: u64,
    snapshot: &crate::ports::VceEmbeddingBatchSnapshot,
    overall: Option<(u64, u64)>,
    next_poll_at_unix_ms: Option<u64>,
) {
    control.observe(VceRefreshProgress {
        activity: Some(VceEmbeddingActivity {
            stage: VceEmbeddingActivityStage::Waiting,
            provider_batch_id: Some(snapshot.submission.provider_batch_id.clone()),
            next_poll_at_unix_ms,
        }),
        phase: VceRefreshPhase::Embedding,
        operation_id: format!("{request_id}:{}:provider", purpose.as_str()),
        unit: VceRefreshUnit::Requests,
        completed,
        total: Some(total),
        overall_completed: overall.map_or(completed, |(base, _)| base),
        overall_total: overall.map(|(_, all)| all),
        provider_batch_state: Some(match snapshot.state {
            VceEmbeddingBatchProviderState::Validating => VceRefreshProviderBatchState::Validating,
            VceEmbeddingBatchProviderState::InProgress => VceRefreshProviderBatchState::InProgress,
            VceEmbeddingBatchProviderState::Finalizing => VceRefreshProviderBatchState::Finalizing,
            VceEmbeddingBatchProviderState::Completed => VceRefreshProviderBatchState::Completed,
            VceEmbeddingBatchProviderState::Failed => VceRefreshProviderBatchState::Failed,
            VceEmbeddingBatchProviderState::Expired => VceRefreshProviderBatchState::Expired,
            VceEmbeddingBatchProviderState::Cancelling => VceRefreshProviderBatchState::Cancelling,
            VceEmbeddingBatchProviderState::Cancelled => VceRefreshProviderBatchState::Cancelled,
        }),
        current_path: None,
    });
}

/// Persists one non-retryable local contract failure before returning it to the refresh owner.
/// 在把一项不可重试的本地契约失败返回刷新所有者前先持久化该失败。
///
/// # Parameters
/// 参数
/// `index` owns durable task storage, `run_id` scopes the task, `record` is updated in place,
/// and `reason` is a bounded content-free contract diagnostic.
/// `index` 持有持久任务存储，`run_id` 限定任务范围，`record` 会被原地更新，
/// `reason` 是有界且不含内容的契约诊断。
///
/// # Returns
/// 返回值
/// The original host-response failure after persistence, or the persistence failure if durability failed.
/// 持久化成功后返回原始宿主响应失败；若持久化失败则返回持久化失败。
async fn persist_remote_batch_contract_failure(
    index: &dyn VceIndexPort,
    run_id: &str,
    record: &mut VceEmbeddingRemoteBatchRecord,
    reason: &str,
) -> VceError {
    record.state = VceEmbeddingRemoteBatchState::Failed;
    record.error = Some(bounded_batch_diagnostic(reason));
    record.next_poll_not_before_unix_ms = None;
    record.updated_at_unix_ms = current_unix_epoch_millis();
    if let Err(error) = index
        .persist_embedding_remote_batch_record(run_id.to_owned(), record.clone())
        .await
    {
        return VceError::Port(error);
    }
    VceError::InvalidHostResponse {
        operation: VcePortOperation::Embedding,
        reason: reason.to_owned(),
    }
}

/// Bound one persisted provider diagnostic without splitting a UTF-8 scalar.
/// 在不切断 UTF-8 字符的前提下限制一条持久供应商诊断。
fn bounded_batch_diagnostic(value: &str) -> String {
    let mut end = value.len().min(8_192);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

/// Return the current Unix timestamp in milliseconds for bounded manifest metadata.
/// 返回当前 Unix 毫秒时间戳，用于有界清单元数据。
fn current_unix_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().min(u64::MAX as u128) as u64
        })
}

/// Persist only the current run lifecycle projection from one complete in-memory manifest.
/// 仅从一份完整内存清单持久化当前运行生命周期投影。
///
/// `index` owns `manifest`; returns after saving its run identity, state, and timestamp or propagates the port error.
/// `index` 拥有 `manifest`；保存运行身份、状态及时间戳后返回，失败时透传端口错误。
async fn persist_manifest_run_state(
    index: &dyn VceIndexPort,
    manifest: &VceEmbeddingRunManifest,
) -> Result<(), VceError> {
    index
        .persist_embedding_run_state(
            manifest.run_id.clone(),
            manifest.state,
            manifest.updated_at_unix_ms,
        )
        .await
        .map_err(VceError::Port)
}

/// Persist one mutable batch projection without rewriting the manifest's frozen source nodes.
/// 持久化一条可变批次投影，不重写清单中的冻结源码节点。
///
/// `batch_position` selects a batch in `manifest` owned by `index`; returns persistence success or an explicit lookup/port error.
/// `batch_position` 选择 `index` 所属 `manifest` 中的批次；返回持久化成功或明确的位置／端口错误。
async fn persist_manifest_batch_record(
    index: &dyn VceIndexPort,
    manifest: &VceEmbeddingRunManifest,
    batch_position: usize,
) -> Result<(), VceError> {
    // Copy only the selected mutable record; the complete frozen source plan stays in memory.
    // 仅复制选中的可变记录；完整冻结源码计划保留在内存中。
    let batch = manifest
        .batches
        .get(batch_position)
        .cloned()
        .ok_or_else(|| {
            VceError::InvalidRequest(
                "durable Embedding manifest batch position is invalid".to_owned(),
            )
        })?;
    index
        .persist_embedding_batch_record(manifest.run_id.clone(), batch, manifest.updated_at_unix_ms)
        .await
        .map_err(VceError::Port)
}

/// Verify that a recovered manifest describes exactly the newly computed source plan.
/// 校验恢复清单是否精确描述了本次重新计算得到的源码计划。
fn validate_embedding_manifest_plan(
    planned: &VceEmbeddingRunManifest,
    existing: &VceEmbeddingRunManifest,
) -> Result<(), VceError> {
    existing.validate().map_err(VceError::InvalidRequest)?;
    if existing.run_id != planned.run_id
        || existing.namespace_id != planned.namespace_id
        || existing.generation != planned.generation
        || existing.source_revision != planned.source_revision
        || existing.contract != planned.contract
        || existing.base_checkpoint_version != planned.base_checkpoint_version
        || existing.reused_blob_names != planned.reused_blob_names
        || existing.nodes != planned.nodes
        || existing.batches.len() != planned.batches.len()
    {
        return Err(VceError::InvalidRequest(
            "durable Embedding manifest does not match the frozen source plan".to_owned(),
        ));
    }
    for (planned_batch, existing_batch) in planned.batches.iter().zip(&existing.batches) {
        if planned_batch.batch_id != existing_batch.batch_id
            || planned_batch.purpose != existing_batch.purpose
            || planned_batch.input_ids != existing_batch.input_ids
            || planned_batch.total_chars != existing_batch.total_chars
        {
            return Err(VceError::InvalidRequest(
                "durable Embedding manifest batch does not match the frozen source plan".to_owned(),
            ));
        }
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
    // A validated grant changes only the expected live producer, never the stored-state identity.
    // 已验证授权仅改变预期实时生产来源，绝不改变已存状态身份。
    let expected = match &engine.config.embedding_execution {
        Some(grant) => grant.execution(),
        None => &engine.config.embedding_space,
    };
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
mod tests;
