//! Host-native Markdown knowledge libraries backed by isolated SQLite and vldb Jieba FTS5.
//! 由隔离 SQLite 与 vldb Jieba FTS5 承载的宿主原生 Markdown 资料库。

mod markdown;
mod models;
mod runtime;
mod schema;

pub use markdown::{
    CHUNK_OVERLAP_TOKENS, HARD_CHUNK_TOKENS, TARGET_CHUNK_TOKENS, extract_document_title,
};
pub use models::{
    KNOWLEDGE_PACKAGE_FORMAT, KNOWLEDGE_PACKAGE_VERSION, KnowledgeChunk, KnowledgeDocumentContent,
    KnowledgeDocumentSummary, KnowledgeImportJobSummary, KnowledgeImportReport,
    KnowledgeIndexIntegritySummary, KnowledgeLibrarySummary, KnowledgeOperationKind,
    KnowledgePackageExportResult, KnowledgePackageImportResult, KnowledgePackageInspection,
    KnowledgePackageManifest, KnowledgeProjectBinding, KnowledgeSearchFailure, KnowledgeSearchHit,
    KnowledgeSearchResult, KnowledgeSourceSummary, MAX_KNOWLEDGE_PACKAGE_BYTES,
};
pub use runtime::KnowledgeBaseRuntime;
pub(crate) use runtime::KnowledgeBaseRuntimeSharedState;
