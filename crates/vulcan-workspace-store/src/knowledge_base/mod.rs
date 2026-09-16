//! Host-native Markdown knowledge libraries backed by isolated SQLite and vldb Jieba FTS5.
//! 由隔离 SQLite 与 vldb Jieba FTS5 承载的宿主原生 Markdown 知识库。

mod markdown;
mod models;
mod runtime;
mod schema;
mod selection;
pub use selection::*;

pub use markdown::{
    CHUNK_OVERLAP_TOKENS, HARD_CHUNK_TOKENS, TARGET_CHUNK_TOKENS, chunk_markdown,
    extract_document_title, normalize_markdown_newlines, sha256_hex,
};
pub use models::{
    KNOWLEDGE_DEFAULT_EMBEDDING_BATCH_SIZE, KNOWLEDGE_PACKAGE_FORMAT, KNOWLEDGE_PACKAGE_VERSION,
    KnowledgeBatchSizeSource, KnowledgeChunk, KnowledgeCurationSettings, KnowledgeDisplayChunk,
    KnowledgeDisplayDocument, KnowledgeDisplayIndexState, KnowledgeDisplayIndexStatus,
    KnowledgeDisplaySkippedDocument, KnowledgeDocumentContent, KnowledgeDocumentSummary,
    KnowledgeEmbeddingSettings, KnowledgeFallbackPolicy, KnowledgeFtsQuery,
    KnowledgeImportJobSummary, KnowledgeImportReport, KnowledgeIndexIntegritySummary,
    KnowledgeLibrarySettings, KnowledgeLibrarySummary, KnowledgeModelSnapshot,
    KnowledgeOperationCurrentRequest, KnowledgeOperationKind, KnowledgeOperationObservability,
    KnowledgeOperationRequestSummary, KnowledgePackageArtifact, KnowledgePackageExportResult,
    KnowledgePackageImportResult, KnowledgePackageInspection, KnowledgePackageManifest,
    KnowledgeProjectBinding, KnowledgeRetrievalMode, KnowledgeRouteBindingKind,
    KnowledgeRouteReference, KnowledgeSearchFailure, KnowledgeSearchHit, KnowledgeSearchResult,
    KnowledgeSourceSummary, KnowledgeVectorIndexState, KnowledgeVectorIndexStatus,
    MAX_KNOWLEDGE_PACKAGE_BYTES, resolve_embedding_batch_size,
};
pub use runtime::KnowledgeBaseRuntime;
pub(crate) use runtime::KnowledgeBaseRuntimeSharedState;
