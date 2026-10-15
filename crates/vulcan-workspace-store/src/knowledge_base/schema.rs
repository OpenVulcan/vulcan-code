//! Authoritative content, immutable history, and derived index schema.
//! 权威内容、不可变历史与派生索引结构。

/// Exact supported format; other versions are rejected without mutation.
/// 唯一支持格式；其他版本直接拒绝且不修改。
pub const KNOWLEDGE_SCHEMA_VERSION: i64 = 2;
/// Fixed derived Jieba index identity.
/// 固定派生结巴索引身份。
pub const KNOWLEDGE_FTS_INDEX: &str = "knowledge_chunks_fts";

/// Builds a fresh library schema for a validated UUID; returns SQL without upgrades.
/// 为已验证 UUID 构建全新知识库结构；返回不含升级路径的 SQL。
pub fn library_schema_script(library_id: &str) -> String {
    format!(
        r#"
PRAGMA foreign_keys = ON;
CREATE TABLE knowledge_metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO knowledge_metadata VALUES
 ('schema_version', '{KNOWLEDGE_SCHEMA_VERSION}'), ('library_id', '{library_id}'),
 ('head_change_seq', '0'), ('dictionary_revision', '0'), ('indexed_dictionary_revision', '0'),
 ('indexed_change_seq', '0'), ('index_status', 'empty');
CREATE TABLE kb_blobs(sha256 TEXT PRIMARY KEY, content TEXT NOT NULL, byte_size INTEGER NOT NULL CHECK(byte_size >= 0));
CREATE TABLE kb_folders(path_key TEXT PRIMARY KEY, path TEXT NOT NULL, description TEXT NOT NULL,
 created_change_seq INTEGER NOT NULL, updated_change_seq INTEGER NOT NULL);
CREATE TABLE kb_documents(id TEXT PRIMARY KEY, path TEXT NOT NULL, path_key TEXT NOT NULL UNIQUE,
 folder_path TEXT NOT NULL, title TEXT NOT NULL, summary TEXT NOT NULL,
 summary_state TEXT NOT NULL CHECK(summary_state IN ('provided', 'pending', 'stale')),
 keywords_json TEXT NOT NULL, body_sha256 TEXT NOT NULL REFERENCES kb_blobs(sha256),
 line_count INTEGER NOT NULL CHECK(line_count >= 0),
 embed_policy TEXT NOT NULL CHECK(embed_policy IN ('summary', 'full', 'none')),
 provenance_json TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0),
 created_change_seq INTEGER NOT NULL, updated_change_seq INTEGER NOT NULL);
CREATE INDEX idx_kb_documents_folder ON kb_documents(folder_path, path_key);
CREATE TABLE kb_changes(seq INTEGER PRIMARY KEY AUTOINCREMENT, at_ms INTEGER NOT NULL,
 actor_kind TEXT NOT NULL CHECK(actor_kind IN ('ai', 'user', 'system')), session_id TEXT, turn_id TEXT,
 tool_call_id TEXT, operation TEXT NOT NULL, note TEXT NOT NULL, restore_target_seq INTEGER);
CREATE TABLE kb_change_items(change_seq INTEGER NOT NULL REFERENCES kb_changes(seq), ordinal INTEGER NOT NULL,
 entity TEXT NOT NULL CHECK(entity IN ('document', 'folder')), entity_key TEXT NOT NULL,
 before_json TEXT, after_json TEXT, PRIMARY KEY(change_seq, ordinal));
CREATE TABLE kb_chunks(id TEXT PRIMARY KEY, document_id TEXT NOT NULL REFERENCES kb_documents(id) ON DELETE CASCADE,
 ordinal INTEGER NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('summary', 'body')), heading_path TEXT NOT NULL,
 start_line INTEGER NOT NULL, end_line INTEGER NOT NULL, text TEXT NOT NULL, content_sha256 TEXT NOT NULL,
 embed INTEGER NOT NULL CHECK(embed IN (0, 1)), UNIQUE(document_id, ordinal));
CREATE INDEX idx_kb_chunks_hash ON kb_chunks(content_sha256);
CREATE TABLE kb_vector_state(content_sha256 TEXT NOT NULL, vector_space_id TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('pending', 'ready', 'failed')), attempts INTEGER NOT NULL CHECK(attempts >= 0),
 last_error TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(content_sha256, vector_space_id));
"#
    ) + vulcan_sqlite_engine::tokenizer::dictionary_table_schema_script()
}
