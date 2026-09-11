//! FTS5 full-text index lifecycle, document mutation and BM25 search.
//! FTS5 全文索引生命周期、文档变更与 BM25 检索。
//!
//! Responsibilities / 职责:
//!
//! - Ensure, rebuild and drop FTS5 indexes over the fixed `(id, file_path, title, content)` shape.
//! - Upsert and delete documents by business identifier.
//! - Run normalized BM25 search returning highlight and snippet fields for the retrieval layer.
//!
//! 负责：在固定的 `(id, file_path, title, content)` 结构上确保、重建与删除 FTS5 索引；按业务标识
//! 写入与删除文档；执行规范化 BM25 检索，为检索层返回高亮与片段字段。
//!
//! Two invariants worth stating explicitly / 两个需要显式说明的不变量:
//!
//! 1. **The index name is never interpolated raw.** [`sanitize_index_name`] restricts it to
//!    `[A-Za-z_][A-Za-z0-9_]*`, refuses the reserved `_vulcan_` prefix, and [`quote_identifier`]
//!    then double-quotes it. FTS5 virtual-table DDL cannot take a bound parameter for the table
//!    name, so this validation is the actual injection barrier rather than a convenience.
//! 2. **Score direction is normalized at this boundary.** SQLite `bm25()` returns lower-is-better
//!    values, so `ORDER BY raw_score ASC` yields the best matches first while
//!    [`SearchFtsHit::score`] exposes the negated value so upper layers always see
//!    "higher is better".
//!
//! 1. **索引名绝不原样拼接。** [`sanitize_index_name`] 将其限制为 `[A-Za-z_][A-Za-z0-9_]*`，拒绝保留
//!    前缀 `_vulcan_`，随后由 [`quote_identifier`] 加双引号。FTS5 虚表 DDL 的表名无法使用绑定参数，
//!    因此该校验是真正的注入屏障而非便利措施。
//! 2. **分数方向在本边界完成规范化。** SQLite `bm25()` 返回"越小越好"的值，故 `ORDER BY raw_score ASC`
//!    使最佳匹配在前，而 [`SearchFtsHit::score`] 暴露取负后的值，使上层始终看到"越大越好"。

use rusqlite::{Connection, params};

use crate::error::{EngineResult, SqliteEngineError};
use crate::tokenizer::{TokenizerMode, ensure_jieba_tokenizer_registered, tokenize_text};

/// Column count of the FTS index shape, used for `highlight` and `snippet` column indexes.
/// FTS 索引结构的列数，用于 `highlight` 与 `snippet` 的列下标。
const COLUMN_TITLE: usize = 2;

/// Zero-based index of the `content` column in the FTS index shape.
/// FTS 索引结构中 `content` 列的零基下标。
const COLUMN_CONTENT: usize = 3;

/// Reserved index-name prefix that callers must not use.
/// 调用方不得使用的保留索引名前缀。
const RESERVED_INDEX_PREFIX: &str = "_vulcan_";

/// Upper bound applied to a requested search limit.
/// 对请求检索条数施加的上限。
const MAX_SEARCH_LIMIT: u32 = 200;

/// FTS index metadata response.
/// FTS 索引元信息响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsureFtsIndexResult {
    /// Whether the operation succeeded.
    /// 操作是否成功。
    pub success: bool,
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
    /// Effective sanitized index name.
    /// 实际使用的规范化索引名。
    pub index_name: String,
    /// Effective tokenizer mode name.
    /// 实际使用的分词模式名。
    pub tokenizer_mode: String,
}

/// FTS index rebuild result.
/// FTS 索引重建结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuildFtsIndexResult {
    /// Whether the operation succeeded.
    /// 操作是否成功。
    pub success: bool,
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
    /// Effective index name.
    /// 实际使用的索引名。
    pub index_name: String,
    /// Effective tokenizer mode name.
    /// 实际使用的分词模式名。
    pub tokenizer_mode: String,
    /// Number of documents rewritten during the rebuild.
    /// 重建时重新写回的文档数。
    pub reindexed_rows: u64,
}

/// FTS document mutation result.
/// FTS 文档变更结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsMutationResult {
    /// Whether the operation succeeded.
    /// 操作是否成功。
    pub success: bool,
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
    /// Number of affected rows.
    /// 受影响行数。
    pub affected_rows: u64,
    /// Effective index name.
    /// 实际使用的索引名。
    pub index_name: String,
}

/// One FTS search hit.
/// 一条 FTS 检索命中。
#[derive(Debug, Clone, PartialEq)]
pub struct SearchFtsHit {
    /// Business identifier.
    /// 业务标识。
    pub id: String,
    /// Logical or file path of the document.
    /// 文档的逻辑路径或文件路径。
    pub file_path: String,
    /// Plain title field.
    /// 纯标题字段。
    pub title: String,
    /// Title with match highlighting markup.
    /// 带命中高亮标记的标题。
    pub title_highlight: String,
    /// Content snippet with match highlighting markup.
    /// 带命中高亮标记的正文片段。
    pub content_snippet: String,
    /// Normalized score where higher is always better.
    /// 规范化分数，始终"越大越好"。
    pub score: f64,
    /// Rank inside the current result set, starting at 1.
    /// 当前结果集中的排序名次，从 1 开始。
    pub rank: u64,
    /// Raw SQLite `bm25()` score, retained for diagnostics and fusion.
    /// SQLite `bm25()` 原始分值，保留用于诊断与融合。
    pub raw_score: f64,
}

/// FTS search response.
/// FTS 检索响应。
#[derive(Debug, Clone, PartialEq)]
pub struct SearchFtsResult {
    /// Whether the operation succeeded.
    /// 操作是否成功。
    pub success: bool,
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
    /// Effective index name.
    /// 实际使用的索引名。
    pub index_name: String,
    /// Effective tokenizer mode name.
    /// 实际使用的分词模式名。
    pub tokenizer_mode: String,
    /// Normalized query text.
    /// 规范化后的查询文本。
    pub normalized_query: String,
    /// Final FTS `MATCH` expression passed to SQLite.
    /// 最终传给 SQLite 的 FTS `MATCH` 表达式。
    pub fts_query: String,
    /// Result source label for hybrid retrieval layers.
    /// 供混合检索层识别的结果来源标识。
    pub source: String,
    /// Query mode label for upper-layer orchestration.
    /// 供上层统一调度的查询模式标识。
    pub query_mode: String,
    /// Number of rows matching the query before pagination.
    /// 分页前匹配该查询的总行数。
    pub total: u64,
    /// Hits for the requested page.
    /// 本次请求页的命中列表。
    pub hits: Vec<SearchFtsHit>,
}

/// Ensures one FTS index exists for the requested logical name.
/// 确保请求的逻辑名对应的 FTS 索引存在。
pub fn ensure_fts_index(
    connection: &Connection,
    index_name: &str,
    tokenizer_mode: TokenizerMode,
) -> EngineResult<EnsureFtsIndexResult> {
    if tokenizer_mode == TokenizerMode::Jieba {
        ensure_jieba_tokenizer_registered(connection)?;
    }

    let index_name = sanitize_index_name(index_name)?;
    let quoted_index_name = quote_identifier(&index_name);
    let tokenizer_sql = tokenizer_sql(tokenizer_mode);

    connection
        .execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {quoted_index_name} USING fts5(
                id UNINDEXED,
                file_path UNINDEXED,
                title,
                content,
                tokenize={tokenizer_sql}
            );"
        ))
        .map_err(SqliteEngineError::sql)?;

    Ok(EnsureFtsIndexResult {
        success: true,
        message: "fts index ensured".to_string(),
        index_name,
        tokenizer_mode: tokenizer_mode.as_str().to_string(),
    })
}

/// Rebuilds an FTS index so existing rows pick up new tokenizer and dictionary behaviour.
/// 重建 FTS 索引，使既有行重新应用新的分词与词典策略。
///
/// The rebuild reads every document first, then drops and recreates the index, then rewrites the
/// documents **inside one `BEGIN IMMEDIATE` transaction**. Reading before opening the transaction
/// is deliberate: a `DROP` plus full reinsert is destructive, so the source rows must already be
/// in memory before anything is dropped. A failure rolls back, so a failed rebuild never leaves a
/// partially populated index.
///
/// 重建先读取全部文档，再删除并重建索引，然后在**同一个 `BEGIN IMMEDIATE` 事务**内重写文档。
/// 在开启事务前读取是刻意的：`DROP` 加全量重插具有破坏性，因此在删除任何东西之前源行必须已在
/// 内存中。失败会回滚，因此重建失败绝不会留下部分填充的索引。
pub fn rebuild_fts_index(
    connection: &Connection,
    index_name: &str,
    tokenizer_mode: TokenizerMode,
) -> EngineResult<RebuildFtsIndexResult> {
    if tokenizer_mode == TokenizerMode::Jieba {
        ensure_jieba_tokenizer_registered(connection)?;
    }

    let index_name = sanitize_index_name(index_name)?;
    let quoted_index_name = quote_identifier(&index_name);

    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![index_name.as_str()],
            |row| row.get(0),
        )
        .map_err(SqliteEngineError::sql)?;

    if exists == 0 {
        let ensured = ensure_fts_index(connection, index_name.as_str(), tokenizer_mode)?;
        return Ok(RebuildFtsIndexResult {
            success: true,
            message: "fts index created during rebuild".to_string(),
            index_name: ensured.index_name,
            tokenizer_mode: ensured.tokenizer_mode,
            reindexed_rows: 0,
        });
    }

    // Read all documents before the destructive drop.
    // 在破坏性删除前读取全部文档。
    let documents = {
        let mut statement = connection
            .prepare(&format!(
                "SELECT id, file_path, title, content FROM {quoted_index_name} ORDER BY rowid ASC"
            ))
            .map_err(SqliteEngineError::sql)?;
        let mut rows = statement.query([]).map_err(SqliteEngineError::sql)?;

        let mut collected = Vec::new();
        while let Some(row) = rows.next().map_err(SqliteEngineError::sql)? {
            collected.push((
                row.get::<_, Option<String>>(0)
                    .map_err(SqliteEngineError::sql)?
                    .unwrap_or_default(),
                row.get::<_, Option<String>>(1)
                    .map_err(SqliteEngineError::sql)?
                    .unwrap_or_default(),
                row.get::<_, Option<String>>(2)
                    .map_err(SqliteEngineError::sql)?
                    .unwrap_or_default(),
                row.get::<_, Option<String>>(3)
                    .map_err(SqliteEngineError::sql)?
                    .unwrap_or_default(),
            ));
        }
        collected
    };

    connection
        .execute_batch("BEGIN IMMEDIATE TRANSACTION;")
        .map_err(SqliteEngineError::sql)?;

    let rebuild_result = (|| -> EngineResult<RebuildFtsIndexResult> {
        connection
            .execute_batch(&format!("DROP TABLE IF EXISTS {quoted_index_name};"))
            .map_err(SqliteEngineError::sql)?;

        let ensured = ensure_fts_index(connection, index_name.as_str(), tokenizer_mode)?;
        let mut reindexed_rows = 0_u64;
        for (id, file_path, title, content) in documents {
            upsert_fts_document(
                connection,
                ensured.index_name.as_str(),
                tokenizer_mode,
                id.as_str(),
                file_path.as_str(),
                title.as_str(),
                content.as_str(),
            )?;
            reindexed_rows += 1;
        }

        Ok(RebuildFtsIndexResult {
            success: true,
            message: format!("fts index rebuilt (rows={reindexed_rows})"),
            index_name: ensured.index_name,
            tokenizer_mode: ensured.tokenizer_mode,
            reindexed_rows,
        })
    })();

    match rebuild_result {
        Ok(result) => {
            connection
                .execute_batch("COMMIT;")
                .map_err(SqliteEngineError::sql)?;
            Ok(result)
        }
        Err(error) => {
            // The rollback error is intentionally discarded: the original failure is the
            // actionable one, and a failed rollback cannot be repaired by reporting it here.
            // 回滚错误被刻意丢弃：原始失败才是可处置的那个，而回滚失败无法通过在此上报来修复。
            let _ = connection.execute_batch("ROLLBACK;");
            Err(error)
        }
    }
}

/// Inserts or replaces one FTS document, keyed by business identifier.
/// 按业务标识写入或替换一条 FTS 文档。
///
/// FTS5 virtual tables have no unique constraint to upsert against, so this deletes the existing
/// row for the identifier and inserts the new one. A savepoint makes those two statements atomic
/// both in autocommit mode and inside a caller-owned transaction such as [`rebuild_fts_index`].
///
/// FTS5 虚表没有可用于 upsert 的唯一约束，因此这里删除该标识的既有行再插入新行。保存点使这两条语句
/// 在自动提交模式和 [`rebuild_fts_index`] 之类调用方持有的事务内都保持原子性。
pub fn upsert_fts_document(
    connection: &Connection,
    index_name: &str,
    tokenizer_mode: TokenizerMode,
    id: &str,
    file_path: &str,
    title: &str,
    content: &str,
) -> EngineResult<FtsMutationResult> {
    let ensured = ensure_fts_index(connection, index_name, tokenizer_mode)?;
    let quoted_index_name = quote_identifier(&ensured.index_name);
    connection
        .execute_batch("SAVEPOINT vulcan_fts_upsert;")
        .map_err(SqliteEngineError::sql)?;

    let mutation_result = (|| -> EngineResult<(u64, u64)> {
        let deleted = connection
            .execute(
                &format!("DELETE FROM {quoted_index_name} WHERE id = ?1"),
                params![id],
            )
            .map_err(SqliteEngineError::sql)? as u64;

        let inserted = connection
            .execute(
                &format!(
                    "INSERT INTO {quoted_index_name} (id, file_path, title, content) VALUES (?1, ?2, ?3, ?4)"
                ),
                params![id, file_path, title, content],
            )
            .map_err(SqliteEngineError::sql)? as u64;

        connection
            .execute_batch("RELEASE SAVEPOINT vulcan_fts_upsert;")
            .map_err(SqliteEngineError::sql)?;
        Ok((deleted, inserted))
    })();

    let (deleted, inserted) = match mutation_result {
        Ok(counts) => counts,
        Err(error) => {
            // Preserve the original mutation failure while restoring the caller's transaction.
            // 保留原始变更错误，同时恢复调用方事务。
            let _ = connection.execute_batch(
                "ROLLBACK TO SAVEPOINT vulcan_fts_upsert; RELEASE SAVEPOINT vulcan_fts_upsert;",
            );
            return Err(error);
        }
    };

    Ok(FtsMutationResult {
        success: true,
        message: "fts document upserted".to_string(),
        affected_rows: deleted + inserted,
        index_name: ensured.index_name,
    })
}

/// Deletes one FTS document by business identifier.
/// 按业务标识删除一条 FTS 文档。
///
/// Unlike [`upsert_fts_document`] this does **not** create the index as a side effect: a delete
/// against a missing index is reported as a validation failure so a typo cannot silently produce
/// an empty index.
///
/// 与 [`upsert_fts_document`] 不同，本函数**不**以副作用方式创建索引：对不存在的索引执行删除会
/// 按校验失败上报，使拼写错误不会静默产生一个空索引。
pub fn delete_fts_document(
    connection: &Connection,
    index_name: &str,
    id: &str,
) -> EngineResult<FtsMutationResult> {
    let index_name = sanitize_index_name(index_name)?;
    let quoted_index_name = quote_identifier(&index_name);

    let affected_rows = connection
        .execute(
            &format!("DELETE FROM {quoted_index_name} WHERE id = ?1"),
            params![id],
        )
        .map_err(SqliteEngineError::sql)? as u64;

    Ok(FtsMutationResult {
        success: true,
        message: if affected_rows > 0 {
            "fts document removed".to_string()
        } else {
            "fts document not found".to_string()
        },
        affected_rows,
        index_name,
    })
}

/// Runs normalized BM25 search with highlight and snippet fields.
/// 执行带高亮与片段字段的规范化 BM25 检索。
///
/// `total` counts every row matching the query, while `hits` covers only the requested page, so a
/// caller can report an accurate hit count without loading all rows.
///
/// `total` 统计匹配该查询的全部行数，而 `hits` 只覆盖请求页，使调用方无需加载全部行即可上报准确命中数。
pub fn search_fts(
    connection: &Connection,
    index_name: &str,
    tokenizer_mode: TokenizerMode,
    query: &str,
    limit: u32,
    offset: u32,
) -> EngineResult<SearchFtsResult> {
    let ensured = ensure_fts_index(connection, index_name, tokenizer_mode)?;
    let tokenized_query = tokenize_text(Some(connection), tokenizer_mode, query, true)?;
    let quoted_index_name = quote_identifier(&ensured.index_name);

    // A limit of zero would return no hits while `total` still reported matches, which reads as a
    // bug to callers. Clamping to at least one keeps the response self-consistent.
    // 若 limit 为零，则会返回零命中而 `total` 仍报告存在匹配，这对调用方而言等同于缺陷。
    // 至少钳制为 1 可保持响应自洽。
    let effective_limit = limit.clamp(1, MAX_SEARCH_LIMIT);

    let total: u64 = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {quoted_index_name} WHERE {quoted_index_name} MATCH ?1"),
            params![tokenized_query.fts_query.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map_err(SqliteEngineError::sql)? as u64;

    let mut statement = connection
        .prepare(&format!(
            "SELECT
                id,
                file_path,
                title,
                highlight({quoted_index_name}, {COLUMN_TITLE}, '<mark>', '</mark>') AS title_highlight,
                snippet({quoted_index_name}, {COLUMN_CONTENT}, '<mark>', '</mark>', '...', 12) AS content_snippet,
                bm25({quoted_index_name}, 2.0, 1.0) AS raw_score
             FROM {quoted_index_name}
             WHERE {quoted_index_name} MATCH ?1
             ORDER BY raw_score ASC, file_path ASC, id ASC
             LIMIT ?2 OFFSET ?3"
        ))
        .map_err(SqliteEngineError::sql)?;

    let mut rows = statement
        .query(params![
            tokenized_query.fts_query.as_str(),
            effective_limit as i64,
            offset as i64
        ])
        .map_err(SqliteEngineError::sql)?;

    let mut hits = Vec::new();
    let mut rank = u64::from(offset) + 1;
    while let Some(row) = rows.next().map_err(SqliteEngineError::sql)? {
        let raw_score = row.get::<_, f64>(5).map_err(SqliteEngineError::sql)?;
        hits.push(SearchFtsHit {
            id: row.get(0).map_err(SqliteEngineError::sql)?,
            file_path: row.get(1).map_err(SqliteEngineError::sql)?,
            title: row
                .get::<_, Option<String>>(2)
                .map_err(SqliteEngineError::sql)?
                .unwrap_or_default(),
            title_highlight: row
                .get::<_, Option<String>>(3)
                .map_err(SqliteEngineError::sql)?
                .unwrap_or_default(),
            content_snippet: row
                .get::<_, Option<String>>(4)
                .map_err(SqliteEngineError::sql)?
                .unwrap_or_default(),
            // Negated so "higher is better" holds for every consumer.
            // 取负，使"越大越好"对所有消费方成立。
            score: -raw_score,
            rank,
            raw_score,
        });
        rank += 1;
    }

    Ok(SearchFtsResult {
        success: true,
        message: format!("fts search completed (hits={})", hits.len()),
        index_name: ensured.index_name,
        tokenizer_mode: ensured.tokenizer_mode,
        normalized_query: tokenized_query.normalized_text,
        fts_query: tokenized_query.fts_query,
        source: "sqlite_fts".to_string(),
        query_mode: "fts".to_string(),
        total,
        hits,
    })
}

/// Validates and normalizes an FTS index name.
/// 校验并规范化 FTS 索引名。
///
/// This is the injection barrier for FTS5 DDL, where the table name cannot be a bound parameter.
/// The rules are deliberately narrow rather than clever: an ASCII letter or underscore first, then
/// ASCII alphanumerics and underscores only, and no reserved `_vulcan_` prefix.
///
/// 这是 FTS5 DDL 的注入屏障——该处的表名无法使用绑定参数。规则刻意保守而非"聪明"：首字符为 ASCII
/// 字母或下划线，其后仅限 ASCII 字母数字与下划线，且不得使用保留前缀 `_vulcan_`。
pub fn sanitize_index_name(index_name: &str) -> EngineResult<String> {
    let trimmed = index_name.trim();

    if trimmed.is_empty() {
        return Err(SqliteEngineError::invalid_argument(
            "index_name must not be empty",
        ));
    }

    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(SqliteEngineError::invalid_argument(
            "index_name must not be empty",
        ));
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(SqliteEngineError::invalid_argument(
            "index_name must start with [A-Za-z_]",
        ));
    }

    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(SqliteEngineError::invalid_argument(
            "index_name only supports [A-Za-z0-9_]",
        ));
    }

    if trimmed.starts_with(RESERVED_INDEX_PREFIX) {
        return Err(SqliteEngineError::invalid_argument(format!(
            "index_name must not use the reserved prefix {RESERVED_INDEX_PREFIX}"
        )));
    }

    Ok(trimmed.to_string())
}

/// Quotes an already validated SQLite identifier.
/// 为已校验的 SQLite 标识符加引号。
///
/// Embedded double quotes are doubled, which is SQLite's own escape rule. This is defence in
/// depth behind [`sanitize_index_name`], not a substitute for it.
///
/// 内部双引号会翻倍，这是 SQLite 自身的转义规则。这是 [`sanitize_index_name`] 之后的纵深防御，
/// 而不是它的替代品。
pub fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Builds the FTS5 tokenizer SQL fragment for one tokenizer mode.
/// 为某个分词模式构造 FTS5 分词器 SQL 片段。
fn tokenizer_sql(tokenizer_mode: TokenizerMode) -> &'static str {
    match tokenizer_mode {
        TokenizerMode::None => "'unicode61 remove_diacritics 2'",
        TokenizerMode::Jieba => "'jieba'",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::upsert_custom_word;

    /// Measures full-text search latency and reports p50/p95 as plan §10 evidence.
    /// 测量全文检索延迟，并报告中位数与 p95，作为方案 §10 的证据。
    ///
    /// Ignored by default because it is evidence rather than a correctness check: it asserts nothing about
    /// timing and prints the measurements instead, so the evidence is the recorded number.
    /// 默认忽略，因为它是证据而非正确性检查：不对耗时作任何断言，而是打印测量值，故证据就是被记录下来的数字。
    #[test]
    #[ignore = "explicit §10 SQLite engine performance evidence"]
    fn performance_fts_search_reports_p50_p95() {
        const SAMPLES: usize = 20;
        const DOCUMENTS: usize = 2_000;
        const INDEX_NAME: &str = "performance_docs";

        let connection = connection();
        ensure_fts_index(&connection, INDEX_NAME, TokenizerMode::Jieba).expect("ensure index");
        for ordinal in 0..DOCUMENTS {
            let document_id = format!("doc-{ordinal}");
            let body = format!("市民田-女士急匆匆 performance document number {ordinal}");
            upsert_fts_document(
                &connection,
                INDEX_NAME,
                TokenizerMode::Jieba,
                &document_id,
                "/performance/file.md",
                "性能标题",
                &body,
            )
            .expect("upsert document");
        }

        let mut search_micros = Vec::with_capacity(SAMPLES);
        let mut last_total = 0u64;
        for _ in 0..SAMPLES {
            let started = std::time::Instant::now();
            let result = search_fts(
                &connection,
                INDEX_NAME,
                TokenizerMode::Jieba,
                "田-女士",
                10,
                0,
            )
            .expect("search");
            search_micros.push(started.elapsed().as_micros());
            last_total = result.total;
        }

        // Percentiles use the nearest-rank method on the sorted sample list.
        // 百分位在排序后的样本列表上采用最近秩法。
        // The first iteration is a warmup: its work runs and warms the path, its sample is discarded.
        // 首次迭代为预热：其工作照常执行并预热路径，其样本被丢弃。
        search_micros.remove(0);
        search_micros.sort_unstable();
        let last_index = search_micros.len().saturating_sub(1);
        let p50 = search_micros[last_index / 2];
        let rank = ((95 * search_micros.len()).div_ceil(100)).saturating_sub(1);
        let p95 = search_micros[rank.min(last_index)];
        let env_os = std::env::consts::OS;
        let env_arch = std::env::consts::ARCH;
        eprintln!(
            "[perf-fts] samples={SAMPLES} documents={DOCUMENTS} \
         search_p50_us={p50} search_p95_us={p95} matched_documents={last_total} \
         env_os={env_os} env_arch={env_arch}"
        );
    }
    /// Opens an in-memory connection with the Jieba tokenizer registered.
    /// 打开一个已注册 Jieba 分词器的内存连接。
    fn connection() -> Connection {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        ensure_jieba_tokenizer_registered(&connection).expect("register jieba tokenizer");
        connection
    }

    /// Verifies the minimal end-to-end flow: ensure, upsert, search.
    /// 验证最小端到端闭环：确保索引、写入、检索。
    #[test]
    fn ensure_upsert_and_search_fts() {
        let connection = connection();
        let ensured =
            ensure_fts_index(&connection, "memory_docs", TokenizerMode::Jieba).expect("ensure");
        assert!(ensured.success);
        assert_eq!(ensured.index_name, "memory_docs");

        upsert_fts_document(
            &connection,
            "memory_docs",
            TokenizerMode::Jieba,
            "doc-1",
            "/demo/file.md",
            "测试标题",
            "市民田-女士急匆匆",
        )
        .expect("first upsert");

        upsert_custom_word(&connection, "田-女士", 42).expect("add custom word");

        upsert_fts_document(
            &connection,
            "memory_docs",
            TokenizerMode::Jieba,
            "doc-1",
            "/demo/file.md",
            "测试标题",
            "市民田-女士急匆匆",
        )
        .expect("second upsert");

        let result = search_fts(
            &connection,
            "memory_docs",
            TokenizerMode::Jieba,
            "田-女士",
            10,
            0,
        )
        .expect("search");

        assert!(result.success);
        assert_eq!(result.total, 1);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].id, "doc-1");
        assert_eq!(result.hits[0].file_path, "/demo/file.md");
        assert_eq!(result.hits[0].rank, 1);
        assert!(result.hits[0].content_snippet.contains("mark"));
        assert_eq!(result.source, "sqlite_fts");
        assert_eq!(result.query_mode, "fts");
    }

    /// Verifies a rebuild applies updated dictionary behaviour to previously indexed documents.
    /// 验证重建会把更新后的词典行为应用到此前已索引的文档。
    #[test]
    fn rebuild_fts_index_reindexes_existing_documents() {
        let connection = connection();
        ensure_fts_index(&connection, "memory_docs", TokenizerMode::Jieba).expect("ensure");
        upsert_fts_document(
            &connection,
            "memory_docs",
            TokenizerMode::Jieba,
            "doc-1",
            "/demo/file.md",
            "测试标题",
            "市民田-女士急匆匆",
        )
        .expect("upsert");

        let term_count = |connection: &Connection| -> i64 {
            connection
                .execute_batch(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS memory_docs_vocab USING fts5vocab(
                        memory_docs,
                        'instance'
                    );",
                )
                .expect("create vocab table");
            let count = connection
                .query_row(
                    "SELECT count(*) FROM memory_docs_vocab WHERE term = ?1",
                    params!["田-女士"],
                    |row| row.get(0),
                )
                .expect("count term");
            connection
                .execute_batch("DROP TABLE IF EXISTS memory_docs_vocab;")
                .expect("drop vocab table");
            count
        };

        assert_eq!(
            term_count(&connection),
            0,
            "term must be absent before rebuild"
        );

        upsert_custom_word(&connection, "田-女士", 42).expect("add custom word");

        let rebuild =
            rebuild_fts_index(&connection, "memory_docs", TokenizerMode::Jieba).expect("rebuild");
        assert!(rebuild.success);
        assert_eq!(rebuild.reindexed_rows, 1);

        assert_eq!(
            term_count(&connection),
            1,
            "rebuild must reindex the document under the new dictionary"
        );
    }

    /// Verifies a rebuild whose destructive drop fails rolls back and keeps the existing index.
    /// 验证重建的破坏性删除失败时会回滚，并保留既有索引。
    #[test]
    fn rebuild_failure_rolls_back_and_keeps_the_index() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("fts-rollback.db");

        // A file-backed database is required: an in-memory database cannot be locked across connections.
        // 必须使用文件型数据库：内存库无法跨连接加锁。
        let connection = Connection::open(&db_path).expect("open first connection");
        ensure_jieba_tokenizer_registered(&connection).expect("register jieba tokenizer");
        ensure_fts_index(&connection, "memory_docs", TokenizerMode::Jieba).expect("ensure");
        upsert_fts_document(
            &connection,
            "memory_docs",
            TokenizerMode::Jieba,
            "doc-1",
            "/demo/file.md",
            "测试标题",
            "市民田-女士急匆匆",
        )
        .expect("upsert");

        // A second connection holds a write transaction, so the rebuild's DROP TABLE cannot proceed.
        // 第二个连接持有写事务，因此重建的 DROP TABLE 无法进行。
        let blocker = Connection::open(&db_path).expect("open blocking connection");
        blocker
            .execute_batch("BEGIN IMMEDIATE TRANSACTION;")
            .expect("begin blocking transaction");
        blocker
            .execute_batch("CREATE TABLE blocker_holder (id INTEGER PRIMARY KEY);")
            .expect("write inside the blocking transaction");

        let rebuild = rebuild_fts_index(&connection, "memory_docs", TokenizerMode::Jieba);
        assert!(
            rebuild.is_err(),
            "the rebuild must fail while another connection holds the write lock"
        );

        blocker
            .execute_batch("ROLLBACK;")
            .expect("release the blocking transaction");
        drop(blocker);

        // The failed rebuild must not have removed the index or its rows.
        // 失败的重建不得删除索引或其行。
        let rows: i64 = connection
            .query_row("SELECT count(*) FROM memory_docs", [], |row| row.get(0))
            .expect("count indexed rows");
        assert_eq!(
            rows, 1,
            "the rollback must keep the pre-existing indexed row"
        );
    }
    /// Verifies the reserved prefix is refused.
    /// 验证保留前缀被拒绝。
    #[test]
    fn reserved_index_prefix_is_refused() {
        match sanitize_index_name("_vulcan_dict") {
            Err(SqliteEngineError::InvalidArgument { detail }) => {
                assert!(detail.contains("reserved"), "unexpected detail: {detail}");
            }
            other => panic!("expected reserved-prefix refusal, got {other:?}"),
        }
    }

    /// Verifies index-name validation rejects every injection-shaped input.
    /// 验证索引名校验拒绝所有注入形态的输入。
    #[test]
    fn index_name_validation_rejects_injection_shapes() {
        for bad in [
            "",
            "   ",
            "1starts_with_digit",
            "has space",
            "has-dash",
            "quote\"name",
            "semi;colon",
            "paren(name)",
            "unicode中文",
            "doc.name",
        ] {
            assert!(
                sanitize_index_name(bad).is_err(),
                "index name {bad:?} must be refused"
            );
        }
    }

    /// Verifies valid index names pass and are trimmed.
    /// 验证合法索引名通过并被裁剪。
    #[test]
    fn index_name_validation_accepts_valid_names() {
        assert_eq!(
            sanitize_index_name("  memory_docs  ").expect("valid"),
            "memory_docs"
        );
        assert_eq!(sanitize_index_name("_private").expect("valid"), "_private");
        assert_eq!(sanitize_index_name("Docs2").expect("valid"), "Docs2");
    }

    /// Verifies identifier quoting doubles embedded quotes.
    /// 验证标识符引用会将内部引号翻倍。
    #[test]
    fn identifier_quoting_doubles_embedded_quotes() {
        assert_eq!(quote_identifier("plain"), "\"plain\"");
        assert_eq!(quote_identifier("a\"b"), "\"a\"\"b\"");
    }

    /// Verifies the score direction is normalized so higher is better.
    /// 验证分数方向已规范化，使越大越好。
    #[test]
    fn search_score_is_negated_raw_bm25() {
        let connection = connection();
        upsert_fts_document(
            &connection,
            "score_docs",
            TokenizerMode::None,
            "doc-1",
            "/demo/a.md",
            "alpha",
            "alpha beta gamma",
        )
        .expect("upsert");

        let result = search_fts(
            &connection,
            "score_docs",
            TokenizerMode::None,
            "alpha",
            10,
            0,
        )
        .expect("search");

        assert_eq!(result.hits.len(), 1);
        let hit = &result.hits[0];
        assert_eq!(hit.score, -hit.raw_score);
    }

    /// Verifies upsert replaces rather than duplicates a document with the same identifier.
    /// 验证同一标识的写入是替换而不是重复插入。
    #[test]
    fn upsert_replaces_existing_document() {
        let connection = connection();

        for content in ["first version", "second version"] {
            upsert_fts_document(
                &connection,
                "replace_docs",
                TokenizerMode::None,
                "doc-1",
                "/demo/a.md",
                "title",
                content,
            )
            .expect("upsert");
        }

        let total: i64 = connection
            .query_row("SELECT COUNT(*) FROM replace_docs", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(total, 1, "upsert must not accumulate duplicate rows");

        let result = search_fts(
            &connection,
            "replace_docs",
            TokenizerMode::None,
            "second",
            10,
            0,
        )
        .expect("search");
        assert_eq!(result.total, 1);
    }

    /// Verifies a failed replacement restores the previous FTS row.
    /// 验证替换失败时会恢复原有 FTS 行。
    #[test]
    fn failed_upsert_keeps_the_previous_document() {
        let connection = connection();
        upsert_fts_document(
            &connection,
            "atomic_docs",
            TokenizerMode::None,
            "doc-1",
            "/demo/original.md",
            "original title",
            "original content",
        )
        .expect("insert original document");

        connection
            .execute_batch(
                "CREATE TRIGGER reject_atomic_docs_insert
                 BEFORE INSERT ON atomic_docs_content
                 BEGIN
                     SELECT RAISE(ABORT, 'forced insert failure');
                 END;",
            )
            .expect("install insert failure trigger");

        let replacement = upsert_fts_document(
            &connection,
            "atomic_docs",
            TokenizerMode::None,
            "doc-1",
            "/demo/replacement.md",
            "replacement title",
            "replacement content",
        );
        assert!(replacement.is_err(), "the forced replacement must fail");

        connection
            .execute_batch("DROP TRIGGER reject_atomic_docs_insert;")
            .expect("remove insert failure trigger");
        let preserved: (String, String, String) = connection
            .query_row(
                "SELECT file_path, title, content FROM atomic_docs WHERE id = ?1",
                params!["doc-1"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read preserved document");
        assert_eq!(
            preserved,
            (
                String::from("/demo/original.md"),
                String::from("original title"),
                String::from("original content"),
            )
        );
    }

    /// Verifies deleting an absent document reports zero affected rows rather than failing.
    /// 验证删除不存在的文档报告零影响行而不是失败。
    #[test]
    fn delete_absent_document_reports_not_found() {
        let connection = connection();
        ensure_fts_index(&connection, "delete_docs", TokenizerMode::None).expect("ensure");

        let removed =
            delete_fts_document(&connection, "delete_docs", "missing-id").expect("delete absent");
        assert!(removed.success);
        assert_eq!(removed.affected_rows, 0);
        assert!(removed.message.contains("not found"));
    }

    /// Verifies deleting from a nonexistent index fails instead of silently succeeding.
    /// 验证对不存在的索引执行删除会失败而不是静默成功。
    #[test]
    fn delete_from_missing_index_is_refused() {
        let connection = connection();
        assert!(delete_fts_document(&connection, "never_created", "doc-1").is_err());
    }

    /// Verifies pagination reports the full match count with a bounded page.
    /// 验证分页在返回有界页的同时报告完整匹配数。
    #[test]
    fn search_reports_total_across_pages() {
        let connection = connection();

        for index in 0..5 {
            upsert_fts_document(
                &connection,
                "page_docs",
                TokenizerMode::None,
                &format!("doc-{index}"),
                &format!("/demo/{index}.md"),
                "common title",
                "common token payload",
            )
            .expect("upsert");
        }

        let first_page = search_fts(
            &connection,
            "page_docs",
            TokenizerMode::None,
            "common",
            2,
            0,
        )
        .expect("first page");
        assert_eq!(first_page.total, 5, "total must cover every match");
        assert_eq!(first_page.hits.len(), 2, "page must be bounded by limit");
        assert_eq!(first_page.hits[0].rank, 1);
        assert_eq!(first_page.hits[1].rank, 2);

        let second_page = search_fts(
            &connection,
            "page_docs",
            TokenizerMode::None,
            "common",
            2,
            2,
        )
        .expect("second page");
        assert_eq!(second_page.total, 5);
        assert_eq!(second_page.hits.len(), 2);
        assert_eq!(
            second_page.hits[0].rank, 3,
            "rank must continue from the offset rather than restarting"
        );

        let first_ids = first_page
            .hits
            .iter()
            .map(|hit| hit.id.clone())
            .collect::<Vec<_>>();
        let second_ids = second_page
            .hits
            .iter()
            .map(|hit| hit.id.clone())
            .collect::<Vec<_>>();
        assert!(
            first_ids.iter().all(|id| !second_ids.contains(id)),
            "pages must not overlap"
        );
    }

    /// Verifies an oversized limit is clamped and a zero limit still returns a usable page.
    /// 验证过大 limit 被钳制，零 limit 仍返回可用页。
    #[test]
    fn search_limit_is_clamped() {
        let connection = connection();
        upsert_fts_document(
            &connection,
            "clamp_docs",
            TokenizerMode::None,
            "doc-1",
            "/demo/a.md",
            "title",
            "clampable token",
        )
        .expect("upsert");

        let zero_limit = search_fts(
            &connection,
            "clamp_docs",
            TokenizerMode::None,
            "clampable",
            0,
            0,
        )
        .expect("zero limit");
        assert_eq!(
            zero_limit.hits.len(),
            1,
            "a zero limit must not silently return an empty page while reporting matches"
        );

        let huge_limit = search_fts(
            &connection,
            "clamp_docs",
            TokenizerMode::None,
            "clampable",
            100_000,
            0,
        )
        .expect("huge limit");
        assert_eq!(huge_limit.hits.len(), 1);
    }

    /// Verifies a rebuild on a missing index creates it rather than failing.
    /// 验证对不存在的索引执行重建会创建它而不是失败。
    #[test]
    fn rebuild_on_missing_index_creates_it() {
        let connection = connection();

        let rebuild =
            rebuild_fts_index(&connection, "fresh_docs", TokenizerMode::None).expect("rebuild");
        assert!(rebuild.success);
        assert_eq!(rebuild.reindexed_rows, 0);

        let exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params!["fresh_docs"],
                |row| row.get(0),
            )
            .expect("check existence");
        assert_eq!(exists, 1);
    }

    /// Verifies plain mode still produces searchable results without the Jieba tokenizer.
    /// 验证常规模式在不使用 Jieba 分词器时仍能产生可检索结果。
    #[test]
    fn plain_mode_search_works() {
        let connection = connection();
        upsert_fts_document(
            &connection,
            "plain_docs",
            TokenizerMode::None,
            "doc-1",
            "/demo/plain.md",
            "Plain Title",
            "plain searchable content",
        )
        .expect("upsert");

        let result = search_fts(
            &connection,
            "plain_docs",
            TokenizerMode::None,
            "searchable",
            10,
            0,
        )
        .expect("search");
        assert_eq!(result.total, 1);
        assert_eq!(result.tokenizer_mode, "none");
    }
}
