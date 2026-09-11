//! Native VMM-only Jieba FTS storage inside the VMM's own in-process SQLite database.
//! 原生 VMM 专用 Jieba 全文存储，位于 VMM 自身的进程内 SQLite 数据库中。

use serde::{Deserialize, Serialize};
use vulcan_sqlite_engine::sql_exec::SqliteValue;
use vulcan_sqlite_engine::tokenizer::TokenizerMode;

use super::{VmmDataPlaneHandle, sqlite_string_literal, sqlite_text};
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;

#[cfg(test)]
#[path = "lexical_tests.rs"]
mod tests;

/// Names the VMM-private FTS index; no knowledge-library identifiers or dictionary are reused.
/// 命名 VMM 专有全文索引；不复用知识库标识或词典。
const VMM_LEXICAL_INDEX: &str = "vmm_memory_fts";
/// Bounds one physical query page while allowing the caller to continue past filtered candidates.
/// 限制单页物理查询，同时允许调用方继续读取被过滤候选之后的页面。
const MAX_LEXICAL_PAGE_SIZE: u32 = 1_024;

/// Carries only searchable text and its stable Core memory ID; Core remains the visibility authority.
/// 仅携带可搜索文本及稳定 Core 记忆 ID；可见性权威仍由 Core 持有。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VmmLexicalDocument {
    /// Identifies the immutable Core memory identity, not a display name or vector-generation ID.
    /// 标识不可变 Core 记忆身份，而不是显示名称或向量代次 ID。
    pub memory_id: String,
    /// Supplies the searchable abstract, mapped to the upstream title column.
    /// 提供可搜索摘要，对应上游标题列。
    pub summary: String,
    /// Supplies the searchable details, mapped to the upstream content column.
    /// 提供可搜索详情，对应上游正文列。
    pub details: String,
}

/// Reports an unfiltered physical FTS hit with the original lower-is-better SQLite BM25 value.
/// 报告未经过滤的物理全文命中，携带 SQLite 越小越优的原始 BM25 值。
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct VmmLexicalHit {
    /// Resolves back to one authoritative Core memory record.
    /// 回指一条权威 Core 记忆记录。
    pub memory_id: String,
    /// Preserves raw BM25; this is neither cosine similarity nor a normalized decision score.
    /// 保留原始 BM25；它既不是余弦相似度，也不是归一化决策分数。
    pub raw_score: f64,
}

/// Reports actual derived document and FTS row counts without asserting index integrity.
/// 报告真实派生文档及全文行数，不据计数宣称索引完整。
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct VmmLexicalIndexCounts {
    /// Number of VMM-derived source documents.
    /// VMM 派生源文档数量。
    pub document_count: u64,
    /// Number of physical FTS document rows, not token counts.
    /// 全文物理文档行数，而非词元数。
    pub indexed_row_count: u64,
}

/// Defines derived documents and triggers that keep FTS text changes atomic with each statement.
/// 定义派生文档及触发器，使每条语句中的全文文本变化保持原子性。
/// All identifiers are fixed VMM-owned names; user text enters only later as bound JSON.
/// 所有标识均为固定 VMM 专有名称；用户文本仅在后续作为绑定 JSON 输入。
/// Stable memory IDs link both tables; physical row IDs may change during an upstream rebuild.
/// 两表通过稳定记忆 ID 关联；上游重建时物理行号可以变化。
const LEXICAL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS vmm_lexical_documents (
    memory_id TEXT NOT NULL PRIMARY KEY,
    summary TEXT NOT NULL,
    details TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS vmm_lexical_insert AFTER INSERT ON vmm_lexical_documents BEGIN
    INSERT INTO vmm_memory_fts(id, file_path, title, content)
    VALUES (new.memory_id, new.memory_id, new.summary, new.details);
END;
CREATE TRIGGER IF NOT EXISTS vmm_lexical_update AFTER UPDATE ON vmm_lexical_documents BEGIN
    DELETE FROM vmm_memory_fts WHERE id = old.memory_id;
    INSERT INTO vmm_memory_fts(id, file_path, title, content)
    VALUES (new.memory_id, new.memory_id, new.summary, new.details);
END;
CREATE TRIGGER IF NOT EXISTS vmm_lexical_delete AFTER DELETE ON vmm_lexical_documents BEGIN
    DELETE FROM vmm_memory_fts WHERE id = old.memory_id;
END;
"#;

impl VmmDataPlaneHandle {
    /// Reads exact document counts from this VMM binding without initializing or changing it.
    /// 从当前 VMM 绑定读取准确文档计数，不初始化或改变它。
    /// Returns one typed aggregate row or an explicit engine/shape error.
    /// 返回一条强类型聚合记录，或明确的引擎/形状错误。
    pub async fn lexical_index_counts(&self) -> Result<VmmLexicalIndexCounts, String> {
        // Both aggregates share the same SQLite statement snapshot.
        // 两个聚合共用同一条 SQLite 语句快照。
        let result = local_sql::query_json(
            self.local.as_ref(),
            LogicalDatabase::VmmMetadata,
            "SELECT (SELECT COUNT(*) FROM vmm_lexical_documents) AS document_count, (SELECT COUNT(*) FROM vmm_memory_fts) AS indexed_row_count;",
            &[],
        )
        .map_err(|error| format!("读取原生 VMM 全文索引计数失败：{error}"))?;
        let mut rows: Vec<VmmLexicalIndexCounts> = serde_json::from_str(&result.json_data)
            .map_err(|error| format!("解析原生 VMM 全文索引计数失败：{error}"))?;
        if rows.len() != 1 {
            return Err(String::from("原生 VMM 全文索引计数未返回唯一聚合记录。"));
        }
        rows.pop()
            .ok_or_else(|| String::from("原生 VMM 全文索引计数记录缺失。"))
    }

    /// Atomically replaces only derived text using the caller's complete authoritative active set.
    /// 使用调用方完整的权威活跃集合，仅原子替换派生文本。
    /// Caller must hold the VMM mutation barrier; duplicates/empty IDs are rejected before SQL.
    /// 调用方必须持有 VMM 变更屏障，执行 SQL 前拒绝重复或空标识。
    pub async fn replace_lexical_documents(
        &self,
        documents: &[VmmLexicalDocument],
    ) -> Result<(), String> {
        // Validate the complete set before deleting any derived row.
        // 删除任何派生行之前校验完整集合。
        let mut identities = std::collections::BTreeSet::new();
        for document in documents {
            if document.memory_id.is_empty() || !identities.insert(&document.memory_id) {
                return Err(String::from("原生 VMM 全文重建文档 ID 为空或重复。"));
            }
        }
        // Reuse the existing VMM transaction-script quoting contract, including quotes and newlines.
        // 复用现有 VMM 事务脚本的引用契约，包括引号与换行。
        let serialized = serde_json::to_string(documents)
            .map_err(|error| format!("序列化原生 VMM 全文重建文档失败：{error}"))?;
        let escaped = sqlite_string_literal(&serialized);
        // Clear orphan FTS rows too; inserting documents runs the existing stable-ID triggers.
        // 同时清除孤立全文行，插入文档时运行现有稳定 ID 触发器。
        let script = format!(
            "BEGIN IMMEDIATE;\nDELETE FROM vmm_lexical_documents;\nDELETE FROM vmm_memory_fts;\nINSERT OR ROLLBACK INTO vmm_lexical_documents(memory_id, summary, details) SELECT json_extract(value, '$.memory_id'), json_extract(value, '$.summary'), json_extract(value, '$.details') FROM json_each('{escaped}');\nCOMMIT;"
        );
        let result = local_sql::execute_script(
            self.local.as_ref(),
            LogicalDatabase::VmmMetadata,
            &script,
            &[],
        )
        .map_err(|error| format!("替换原生 VMM 全文派生文档失败：{error}"))?;
        if !result.success {
            return Err(format!("替换原生 VMM 全文派生文档失败：{}", result.message));
        }
        Ok(())
    }

    /// Creates the isolated VMM Jieba index and its transactional text-maintenance triggers.
    /// 创建隔离的 VMM Jieba 索引及其事务性文本维护触发器。
    /// Returns an explicit engine error; it never switches to substring or another tokenizer.
    /// 返回明确引擎错误；绝不切换到子串匹配或其他分词器。
    pub async fn ensure_lexical_index(&self) -> Result<(), String> {
        // ensured comes from the same upstream capability already used by knowledge libraries.
        // ensured 来自知识库已使用的同一上游能力。
        let ensured = local_sql::ensure_fts_index(
            self.local.as_ref(),
            LogicalDatabase::VmmMetadata,
            VMM_LEXICAL_INDEX,
            TokenizerMode::Jieba,
        )
        .map_err(|error| format!("初始化原生 VMM 全文索引失败：{error}"))?;
        if !ensured.success
            || ensured.index_name != VMM_LEXICAL_INDEX
            || ensured.tokenizer_mode != "jieba"
        {
            return Err("原生 VMM 全文索引未返回声明的 Jieba 索引".to_owned());
        }
        // schema contains no user values and is safe for the unparameterized script path.
        // schema 不含用户值，可安全使用无参数脚本路径。
        let schema = local_sql::execute_script(
            self.local.as_ref(),
            LogicalDatabase::VmmMetadata,
            LEXICAL_SCHEMA,
            &[],
        )
        .map_err(|error| format!("初始化原生 VMM 全文文档表失败：{error}"))?;
        if !schema.success {
            return Err("原生 VMM 全文文档表初始化未成功".to_owned());
        }
        Ok(())
    }

    /// Rebuilds this VMM index with the declared Jieba tokenizer using the upstream transactional API.
    /// 使用上游事务 API 及声明的 Jieba 分词器重建此 VMM 索引。
    /// Returns the exact reindexed document count; callers serialize this explicit maintenance with writes.
    /// 返回精确重建文档数；调用方须将这项显式维护与写入串行化。
    /// Neither memory identities, vectors nor authoritative text are changed or regenerated.
    /// 不改变或重新生成记忆身份、向量及权威文本。
    pub async fn rebuild_lexical_index(&self) -> Result<u64, String> {
        // rebuilt is the actual engine receipt, not a count inferred from a Core snapshot.
        // rebuilt 是实际引擎回执，而不是从 Core 快照推断的数量。
        let rebuilt = local_sql::rebuild_fts_index(
            self.local.as_ref(),
            LogicalDatabase::VmmMetadata,
            VMM_LEXICAL_INDEX,
            TokenizerMode::Jieba,
        )
        .map_err(|error| format!("重建原生 VMM 全文索引失败：{error}"))?;
        if !rebuilt.success
            || rebuilt.index_name != VMM_LEXICAL_INDEX
            || rebuilt.tokenizer_mode != "jieba"
        {
            return Err("原生 VMM 全文重建未返回声明的 Jieba 索引".to_owned());
        }
        Ok(rebuilt.reindexed_rows)
    }

    /// Upserts one caller-bounded document batch and its index in a single bound SQL statement.
    /// 使用一条绑定 SQL 语句更新调用方限定大小的一批文档及其索引。
    /// `documents` contains canonical IDs and sanitized memory text; empty batches do nothing.
    /// `documents` 包含规范 ID 和已清理记忆文本；空批次不执行操作。
    pub async fn upsert_lexical_documents(
        &self,
        documents: &[VmmLexicalDocument],
    ) -> Result<(), String> {
        if documents.is_empty() {
            return Ok(());
        }
        // identities rejects ambiguous duplicate rows instead of depending on input overwrite order.
        // identities 拒绝有歧义的重复行，而不依赖输入覆盖顺序。
        let mut identities = std::collections::BTreeSet::new();
        for document in documents {
            if document.memory_id.is_empty() || !identities.insert(&document.memory_id) {
                return Err("原生 VMM 全文文档 ID 为空或重复".to_owned());
            }
        }
        // serialized remains a bound value, including quotes, JSON punctuation and multiline text.
        // serialized 始终作为绑定值，包括引号、JSON 标点及多行文本。
        let serialized = serde_json::to_string(documents)
            .map_err(|error| format!("序列化原生 VMM 全文文档失败：{error}"))?;
        // result updates only text changes, avoiding needless reindexing on recall-counter updates.
        // result 仅更新文本变化，避免召回计数更新引发无谓重建索引。
        let result = local_sql::execute_script(
            self.local.as_ref(),
            LogicalDatabase::VmmMetadata,
            r#"INSERT INTO vmm_lexical_documents(memory_id, summary, details)
               SELECT json_extract(value, '$.memory_id'), json_extract(value, '$.summary'), json_extract(value, '$.details')
               FROM json_each(?) WHERE 1
               ON CONFLICT(memory_id) DO UPDATE SET summary = excluded.summary, details = excluded.details
               WHERE summary != excluded.summary OR details != excluded.details;"#,
            &[sqlite_text(serialized)],
        )
        .map_err(|error| format!("更新原生 VMM 全文文档失败：{error}"))?;
        if !result.success {
            return Err("原生 VMM 全文文档更新未成功".to_owned());
        }
        Ok(())
    }

    /// Deletes exactly `memory_ids` and their FTS rows atomically; absent IDs are idempotent no-ops.
    /// 原子删除精确 `memory_ids` 及对应全文行；缺失 ID 为幂等空操作。
    pub async fn delete_lexical_documents(&self, memory_ids: &[String]) -> Result<(), String> {
        if memory_ids.is_empty() {
            return Ok(());
        }
        // serialized is data, never a synthesized SQL identifier or predicate.
        // serialized 是数据，绝不是拼接的 SQL 标识符或谓词。
        let serialized = serde_json::to_string(memory_ids)
            .map_err(|error| format!("序列化原生 VMM 全文删除列表失败：{error}"))?;
        // result includes the trigger-driven deletion of the associated FTS rows.
        // result 包含触发器驱动的关联全文行删除。
        let result = local_sql::execute_script(
            self.local.as_ref(),
            LogicalDatabase::VmmMetadata,
            "DELETE FROM vmm_lexical_documents WHERE memory_id IN (SELECT value FROM json_each(?));",
            &[sqlite_text(serialized)],
        )
        .map_err(|error| format!("删除原生 VMM 全文文档失败：{error}"))?;
        if !result.success {
            return Err("原生 VMM 全文文档删除未成功".to_owned());
        }
        Ok(())
    }

    /// Fetches a bounded physical BM25 page for `query`, with deterministic ID tie ordering.
    /// 为 `query` 获取有界物理 BM25 页面，同分时按 ID 确定性排序。
    /// `offset` counts physical rows, including rows later rejected by Core visibility checks.
    /// `offset` 统计物理行，包含后续被 Core 可见性检查拒绝的行。
    /// Callers must continue paging until enough admissible candidates or the end, under their read gate.
    /// 调用方须在读取门内持续分页，直到合法候选足够或读尽，不能提前截断。
    pub async fn search_lexical_page(
        &self,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<VmmLexicalHit>, String> {
        if limit == 0 || limit > MAX_LEXICAL_PAGE_SIZE {
            return Err("原生 VMM 全文分页大小须在 1 至 1024 之间".to_owned());
        }
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        // tokenized provides the upstream safe MATCH expression, not raw user query syntax.
        // tokenized 提供上游安全 MATCH 表达式，而不是原始用户查询语法。
        let tokenized = local_sql::tokenize(
            self.local.as_ref(),
            LogicalDatabase::VmmMetadata,
            TokenizerMode::Jieba,
            query,
            true,
        )
        .map_err(|error| format!("原生 VMM 全文查询分词失败：{error}"))?;
        if tokenized.tokenizer_mode != "jieba" {
            return Err("原生 VMM 全文查询未使用声明的 Jieba 分词器".to_owned());
        }
        if tokenized.fts_query.is_empty() {
            return Ok(Vec::new());
        }
        // result uses the exact upstream BM25 expression without fabricated similarity conversion.
        // result 使用精确上游 BM25 表达式，不伪造相似度换算。
        let result = local_sql::query_json(
            self.local.as_ref(),
            LogicalDatabase::VmmMetadata,
            "SELECT id AS memory_id, bm25(vmm_memory_fts, 2.0, 1.0) AS raw_score FROM vmm_memory_fts WHERE vmm_memory_fts MATCH ? ORDER BY raw_score ASC, id ASC LIMIT ? OFFSET ?;",
            &[
                sqlite_text(tokenized.fts_query),
                SqliteValue::Integer(i64::from(limit)),
                SqliteValue::Integer(i64::from(offset)),
            ],
        )
        .map_err(|error| format!("查询原生 VMM 全文索引失败：{error}"))?;
        // hits retain original negative BM25 values and exact physical ordering.
        // hits 保留原始负 BM25 值及精确物理顺序。
        let hits: Vec<VmmLexicalHit> = serde_json::from_str(&result.json_data)
            .map_err(|error| format!("解析原生 VMM 全文候选失败：{error}"))?;
        if hits.iter().any(|hit| {
            hit.memory_id.is_empty() || !hit.raw_score.is_finite() || hit.raw_score > 0.0
        }) {
            return Err("原生 VMM 全文索引返回无效身份或 BM25 分数".to_owned());
        }
        Ok(hits)
    }
}
