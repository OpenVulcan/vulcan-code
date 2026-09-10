//! Provider- and storage-agnostic code semantic retrieval engine.
//! 不依赖模型服务和具体存储实现的代码语义检索引擎。

pub mod chunking;
pub mod embedding;
pub mod embedding_execution;
pub mod engine;
pub mod identity;
pub mod indexing;
pub mod management;
pub mod manifest;
pub mod model;
pub mod path_index;
pub mod ports;
pub mod query;
pub mod refresh_control;
pub mod retrieval;
pub mod state;
pub mod structure;
pub mod symbol;
pub mod tooling;
pub mod workspace;

#[cfg(feature = "host-ai-contract")]
pub mod host_adapter;

#[cfg(feature = "host-ai-contract")]
pub use host_adapter::{
    HostEmbeddingBatchResult, HostEmbeddingRequestConfig, build_embedding_request,
    cancel_execution, create_embedding_batch, create_embedding_batch_with_response,
    execution_status, rerank_documents, start_response_execution, stream_response, wait_response,
};

pub use chunking::{ChunkerError, RecursiveChunker, RecursiveChunkerConfig};
pub use embedding::{
    EmbeddingBatch, EmbeddingError, EmbeddingInput, EmbeddingPlan, EmbeddingSpace, EmbeddingVector,
    SegmentVector, TextSegment, normalize_l2, split_text,
};
pub use embedding_execution::VceEmbeddingExecutionContract;
pub use engine::{
    RefreshResult, RetrieveResult, VceConfig, VceConfigError, VceDegradation, VceDegradedFeature,
    VceEngine, VceEngineSnapshot, VceEngineState, VceError, VceManagementSnapshot,
    VceQueryStatistics, VceRefreshSummary, VceRetrievalLimits, VceRetrievalSummary,
    VceWorkspaceStatistics,
};
pub use identity::{
    PathError, blob_name, chunk_id, chunk_id_with_offset, content_hash, is_sha256_identity,
    is_tagged_sha256_identity, normalize_relative_path,
};
pub use indexing::{
    IndexStatus, IndexingConfig, IndexingEngine, IngestResult, PendingIndex, ReadyIndex,
};
pub use management::{
    VceBlobDetail, VceChannelState, VceChunkSummary, VceCompactionOptions, VceFileQuery,
    VceFileSummary, VceIntegrityIssue, VceIntegrityIssueKind, VceMaintenanceAction,
    VceMaintenanceMode, VceMaintenancePlan, VceManagementContractError, VceManagementQuery,
    VceManagementQueryRequest, VceManagementQueryResponse, VceManagementQueryResult,
    VceOperationKind, VceOperationPhase, VceOperationSummary, VcePage, VcePageRequest,
    VcePhysicalFragmentSummary, VcePhysicalTablePlan, VcePhysicalTableRole,
    VcePhysicalTableStatistics, validate_management_query, validate_management_response,
};
pub use manifest::{
    VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION, VceCodeNodeSnapshot, VceEmbeddingBatchRecord,
    VceEmbeddingBatchResult, VceEmbeddingBatchState, VceEmbeddingRunContract,
    VceEmbeddingRunManifest, VceEmbeddingRunState, VceEmbeddingVectorResult,
};
pub use model::{
    Chunk, ChunkError, ChunkKind, FileRecord, FileSource, FileStatus, HitKind, SearchHit,
    SearchSource, SourceRange,
};
pub use path_index::{PathDocument, PathDocumentError};
pub use ports::{
    AuxiliaryIndexStatus, CommitReceipt, DenseSearchRequest, ExactSearchRequest,
    FindMissingRequest, FindMissingResponse, IndexAbortReceipt, IndexAbortRequest, IndexCommit,
    IndexStageBatch, IndexStageReceipt, IndexedBlob, IndexedBlobStatus, PathSearchRequest,
    RefreshMode, RefreshReason, RefreshRequest, RetrievalScope, RetrieveRequest, VceAiBinding,
    VceAiCapabilitySnapshot, VceAiPort, VceAiPurpose, VceAiRequestContext, VceEmbeddingRequest,
    VceEmbeddingResponse, VceFuture, VceIndexBinding, VceIndexCounts, VceIndexIntegrity,
    VceIndexJobSummary, VceIndexManagementSnapshot, VceIndexPort, VceIndexStoreStatus,
    VceManagementRequest, VcePathSearchResponse, VcePortError, VcePortOperation,
    VceQueryRewriteRequest, VceQueryRewriteResponse, VceRerankRequest, VceRerankResponse,
    VceSearchResponse, VceSourceDocument, VceSourceExpectation, VceSourcePort,
    VceSourceReadRequest, VceSourceReadResponse,
};
pub use query::{QueryIntent, QueryPlan, QueryPlanner, QueryPlannerConfig};
pub use refresh_control::{
    VceCancelOutcome, VceRefreshControl, VceRefreshFailure, VceRefreshFailureObserver,
    VceRefreshObserver, VceRefreshPhase, VceRefreshProgress, VceRefreshUnit,
};
pub use retrieval::{
    CoverageConfig, RankedHits, RerankCallback, RetrievalPipeline, RrfConfig,
    append_path_fallbacks, apply_path_boost, apply_source_priority, coverage_select,
    filter_confidence, merge_semantic_and_exact, reciprocal_rank_fusion, render_context,
    reorder_by_rerank, source_priority_factor,
};
pub use state::{VCE_STATE_SCHEMA_VERSION, VceStateError, VceStateIdentity, VceStateSnapshot};
pub use structure::{
    FallbackStructureAnalyzer, RecursiveStructureAnalyzer, StructureAnalysis, StructureError,
    StructureMode, VceStructureAnalyzer, validate_structure_analysis,
};
pub use symbol::{SymbolKind, SymbolOccurrence, extract_symbols_from_chunks, search_exact_symbols};
pub use tooling::{
    CODEBASE_RETRIEVAL_TOOL_DESCRIPTION, CODEBASE_RETRIEVAL_TOOL_ID, CODEBASE_RETRIEVAL_TOOL_NAME,
    CodebaseRetrievalRequest, HostOperationRequest, ToolMappingError, VceToolCall, VceToolCatalog,
    VceToolDefinition, VceToolDescriptor, VceToolOperation, VceToolScope, canonical_public_tool_id,
    parse_codebase_retrieval_arguments,
};
pub use workspace::{
    Checkpoint, FileObservation, ScopeResolution, SyncPlan, WorkspaceSnapshot, WorkspaceState,
};
