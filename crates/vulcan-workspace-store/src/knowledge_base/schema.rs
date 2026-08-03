//! SQLite schema definitions for one physically isolated knowledge library.
//! 单个物理隔离知识库使用的 SQLite 结构定义。

/// Current per-library authoritative schema version.
/// 当前单知识库权威结构版本。
pub const KNOWLEDGE_SCHEMA_VERSION: i64 = 1;
/// Fixed vldb-managed FTS5 index name.
/// 由 vldb 管理的固定 FTS5 索引名称。
pub const KNOWLEDGE_FTS_INDEX: &str = "knowledge_chunks_fts";
/// Prefix used for generation-scoped display FTS indexes.
/// 显示库代次 FTS 索引使用的固定前缀。
pub const KNOWLEDGE_DISPLAY_FTS_INDEX_PREFIX: &str = "knowledge_display_fts";

/// Builds the idempotent schema for one derived display database.
/// 构建一个派生显示库数据库的幂等结构。
pub fn display_schema_script(library_id: &str) -> String {
    format!(
        r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS display_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO display_metadata(key, value) VALUES
    ('schema_version', '{schema_version}'),
    ('library_id', '{library_id}'),
    ('active_generation', ''),
    ('active_fts_index', ''),
    ('source_content_revision', '0')
ON CONFLICT(key) DO NOTHING;

CREATE TABLE IF NOT EXISTS display_documents (
    id TEXT PRIMARY KEY,
    source_document_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    folder_path TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    keywords_json TEXT NOT NULL,
    symbols_json TEXT NOT NULL,
    embedding_text TEXT NOT NULL,
    markdown TEXT NOT NULL,
    generation TEXT NOT NULL,
    generated_at_ms INTEGER NOT NULL,
    UNIQUE(source_document_id, generation)
);

CREATE INDEX IF NOT EXISTS idx_display_documents_generation
ON display_documents(generation, relative_path);

CREATE TABLE IF NOT EXISTS display_chunks (
    id TEXT PRIMARY KEY,
    display_document_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    heading_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    content TEXT NOT NULL,
    embedding_text TEXT NOT NULL,
    generation TEXT NOT NULL,
    UNIQUE(display_document_id, ordinal, generation),
    FOREIGN KEY(display_document_id) REFERENCES display_documents(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_display_chunks_generation
ON display_chunks(generation, display_document_id, ordinal);
"#,
        schema_version = KNOWLEDGE_SCHEMA_VERSION,
        library_id = library_id,
    )
}

/// Builds the complete idempotent schema for one library database.
/// 构建单个知识库数据库的完整幂等结构。
pub fn library_schema_script(library_id: &str) -> String {
    format!(
        r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS knowledge_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO knowledge_metadata(key, value) VALUES
    ('schema_version', '{schema_version}'),
    ('library_id', '{library_id}'),
    ('content_revision', '0'),
    ('indexed_content_revision', '0'),
    ('dictionary_revision', '0'),
    ('indexed_dictionary_revision', '0'),
    ('tokenizer', 'jieba'),
    ('index_status', 'empty'),
    ('last_index_error', '')
ON CONFLICT(key) DO NOTHING;

CREATE TABLE IF NOT EXISTS knowledge_sources (
    id TEXT PRIMARY KEY,
    canonical_root TEXT NOT NULL,
    root_key TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    last_scanned_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS knowledge_documents (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    relative_path_key TEXT NOT NULL,
    folder_path TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    source_sha256 TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    line_count INTEGER NOT NULL,
    source_modified_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE(source_id, relative_path_key),
    FOREIGN KEY(source_id) REFERENCES knowledge_sources(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_knowledge_documents_folder
ON knowledge_documents(folder_path, relative_path);

CREATE TABLE IF NOT EXISTS knowledge_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    heading_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    content TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    estimated_tokens INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE(document_id, ordinal),
    FOREIGN KEY(document_id) REFERENCES knowledge_documents(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_document
ON knowledge_chunks(document_id, ordinal);

CREATE TABLE IF NOT EXISTS knowledge_import_staging_context (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    job_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    committed_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS knowledge_staged_paths (
    relative_path_key TEXT PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS knowledge_staged_documents (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    relative_path_key TEXT NOT NULL,
    folder_path TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    source_sha256 TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    line_count INTEGER NOT NULL,
    source_modified_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS knowledge_staged_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    heading_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    content TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    estimated_tokens INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
"#,
        schema_version = KNOWLEDGE_SCHEMA_VERSION,
        library_id = library_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the authoritative schema remains separate from the derived FTS table.
    /// 验证权威结构与派生 FTS 表保持分离。
    #[test]
    fn schema_keeps_fts_derived() {
        let schema = library_schema_script("00000000-0000-0000-0000-000000000000");
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS knowledge_documents"));
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS knowledge_chunks"));
        assert!(!schema.contains("CREATE VIRTUAL TABLE"));
        assert!(!schema.contains(KNOWLEDGE_FTS_INDEX));
    }

    /// Verifies display storage is separate from authoritative content storage.
    /// 验证显示库存储与权威原始内容存储相互独立。
    #[test]
    fn display_schema_has_generation_and_source_mapping() {
        let schema = display_schema_script("00000000-0000-0000-0000-000000000000");
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS display_documents"));
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS display_chunks"));
        assert!(schema.contains("source_document_id TEXT NOT NULL"));
        assert!(schema.contains("generation TEXT NOT NULL"));
    }
}
