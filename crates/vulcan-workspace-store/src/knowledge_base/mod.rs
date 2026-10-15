//! AI-maintained content with explicit human lifecycle control.
//! AI 维护内容与显式人工生命周期控制。

mod markdown;
mod models;
mod runtime;
mod schema;

pub use markdown::{
    CHUNK_OVERLAP_TOKENS, HARD_CHUNK_TOKENS, KnowledgeBodyLines, TARGET_CHUNK_TOKENS,
    chunk_markdown, extract_document_title, knowledge_body_lines, normalize_markdown_newlines,
    sha256_hex,
};
pub use models::*;
pub use runtime::KnowledgeBaseRuntime;
pub(crate) use runtime::KnowledgeBaseRuntimeSharedState;
pub use schema::KNOWLEDGE_SCHEMA_VERSION;
