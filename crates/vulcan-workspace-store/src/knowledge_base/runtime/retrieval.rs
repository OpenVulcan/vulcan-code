//! Corpus-policy routing and federated knowledge search with authoritative hit projection.
//! 内容层策略路由与保留权威命中投影的联合知识库检索。

use super::super::models::{
    KnowledgeFtsQuery, KnowledgeSearchFailure, KnowledgeSearchHit, KnowledgeSearchResult,
};
use super::raw_corpus::JoinedChunkRow;
use super::{
    KNOWLEDGE_FTS_INDEX, KnowledgeBaseRuntime, MAX_SEARCH_LIMIT, RRF_K, int, nonnegative_u64, text,
};
use futures_util::{StreamExt, stream::FuturesUnordered};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use vldb_controller_client::ControllerSqliteTokenizerMode;

/// Lightweight FTS rank row used when the host materializes authoritative content itself.
/// 宿主自行物化权威内容时使用的轻量 FTS 排名记录。
#[derive(Debug, Deserialize)]
struct FtsRankRow {
    /// Immutable chunk identifier stored in the VLDB-managed FTS index.
    /// 存储在 VLDB 管理 FTS 索引中的不可变切片标识符。
    id: String,
    /// Raw ascending SQLite BM25 score.
    /// SQLite 按升序返回的原始 BM25 分值。
    raw_score: f64,
}

impl KnowledgeBaseRuntime {
    /// Searches caller-authorized libraries using each stored corpus policy and bounded ranking.
    /// 根据各库持久化内容层策略与有界排名检索调用方已授权的知识库。
    ///
    /// # Parameters
    /// 参数
    /// `library_ids` is the exact library identities already authorized by the caller.
    /// `library_ids` 是已经由调用方授权的精确知识库身份集合。
    /// `limit` is the requested result/page bound limited by this API.
    /// `limit` 是由当前接口限制的请求结果或分页上限。
    /// `query` is the caller query used by the selected retrieval policy.
    /// `query` 是选定检索策略使用的调用方查询。
    ///
    /// # Returns
    /// 返回值
    /// Returns the federated hits, FTS expressions and per-library failures, or invalid-input error.
    /// 返回联合命中、FTS 表达式及各库失败，或非法输入错误。
    pub async fn search(
        &self,
        library_ids: &[String],
        query: &str,
        limit: u32,
    ) -> Result<KnowledgeSearchResult, String> {
        self.search_with_mode(library_ids, query, limit, None).await
    }

    /// Searches authorized libraries using an explicit corpus policy when provided.
    /// 在提供显式内容层策略时，按该策略检索已授权资料库。
    ///
    /// # Parameters
    /// 参数
    /// `library_ids` is the exact library identities already authorized by the caller.
    /// `library_ids` 是已经由调用方授权的精确知识库身份集合。
    /// `limit` is the requested result/page bound limited by this API.
    /// `limit` 是由当前接口限制的请求结果或分页上限。
    /// `query` is the caller query used by the selected retrieval policy.
    /// `query` 是选定检索策略使用的调用方查询。
    /// `retrieval_mode` is the optional explicit corpus policy; absence uses the stored library policy.
    /// `retrieval_mode` 是可选的显式内容层策略；未提供时采用库内持久化策略。
    ///
    /// # Returns
    /// 返回值
    /// Returns the policy-routed hits, FTS expressions and per-library failures, or invalid-input error.
    /// 返回按策略路由的命中、FTS 表达式及各库失败，或非法输入错误。
    pub async fn search_with_mode(
        &self,
        library_ids: &[String],
        query: &str,
        limit: u32,
        retrieval_mode: Option<super::models::KnowledgeRetrievalMode>,
    ) -> Result<KnowledgeSearchResult, String> {
        let query = query.trim();
        if query.is_empty() {
            return Err(String::from("知识库搜索词不能为空。"));
        }
        let limit = limit.clamp(1, MAX_SEARCH_LIMIT);
        let mut failures = Vec::new();
        let mut per_library_hits = Vec::new();
        let mut fts_queries = Vec::new();
        let unique_ids: BTreeSet<String> = library_ids.iter().cloned().collect();

        let mut searches = FuturesUnordered::new();
        for library_id in unique_ids {
            searches.push(async move {
                let result = self
                    .search_one_library(&library_id, query, limit, retrieval_mode)
                    .await;
                (library_id, result)
            });
        }
        while let Some((library_id, result)) = searches.next().await {
            match result {
                Ok((hits, expression)) => {
                    per_library_hits.push(hits);
                    fts_queries.push(KnowledgeFtsQuery {
                        library_id,
                        expression,
                    });
                }
                Err(error) => failures.push(KnowledgeSearchFailure { library_id, error }),
            }
        }
        failures.sort_by(|left, right| left.library_id.cmp(&right.library_id));
        fts_queries.sort_by(|left, right| left.library_id.cmp(&right.library_id));

        let mut hits = Vec::new();
        let federated = per_library_hits.len() > 1;
        for library_hits in per_library_hits {
            for (rank, mut hit) in library_hits.into_iter().enumerate() {
                if federated {
                    hit.rrf_score = Some(1.0 / (RRF_K + rank as f64 + 1.0));
                }
                hits.push(hit);
            }
        }
        if federated {
            hits.sort_by(|left, right| {
                right
                    .rrf_score
                    .partial_cmp(&left.rrf_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.library_id.cmp(&right.library_id))
                    .then_with(|| left.chunk_id.cmp(&right.chunk_id))
            });
            hits.truncate(limit as usize);
        }
        Ok(KnowledgeSearchResult {
            query: query.to_string(),
            hits,
            fts_queries,
            failures,
            warnings: Vec::new(),
        })
    }

    /// Searches one physical library and materializes each hit from authoritative rows.
    /// 检索一个物理知识库，并从权威记录物化每条命中。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library identity.
    /// `library_id` 是精确的活动知识库身份。
    /// `limit` is the requested result/page bound limited by this API.
    /// `limit` 是由当前接口限制的请求结果或分页上限。
    /// `query` is the caller query used by the selected retrieval policy.
    /// `query` 是选定检索策略使用的调用方查询。
    /// `retrieval_mode` is the optional explicit corpus policy; absence uses the stored library policy.
    /// `retrieval_mode` 是可选的显式内容层策略；未提供时采用库内持久化策略。
    ///
    /// # Returns
    /// 返回值
    /// Returns the ordered authoritative/display hits and exact FTS expression, or an error.
    /// 返回排序后的权威或显示命中及精确 FTS 表达式，或错误。
    async fn search_one_library(
        &self,
        library_id: &str,
        query: &str,
        limit: u32,
        retrieval_mode: Option<super::models::KnowledgeRetrievalMode>,
    ) -> Result<(Vec<KnowledgeSearchHit>, String), String> {
        let library = self.require_active_library(library_id).await?;
        let effective_mode = retrieval_mode.unwrap_or(library.settings.retrieval_mode);
        if !matches!(
            effective_mode,
            super::models::KnowledgeRetrievalMode::RawBm25
        ) {
            if !matches!(
                library.settings.display_index.status,
                super::models::KnowledgeDisplayIndexStatus::Ready
            ) {
                if retrieval_mode.is_none()
                    && matches!(
                        library.settings.fallback_policy,
                        super::models::KnowledgeFallbackPolicy::RawOnlyWhenDisplayMissing
                    )
                    && matches!(
                        library.settings.display_index.status,
                        super::models::KnowledgeDisplayIndexStatus::NotCreated
                    )
                {
                    // Explicit fallback is limited to a display corpus that has never existed.
                    // 显式回退仅适用于显示库从未创建的状态。
                } else {
                    return Err(String::from(
                        "当前资料库显示库尚未生成或索引尚未就绪；请显式切换到原始库 BM25，或先完成显示库构建。",
                    ));
                }
            } else {
                let (display_hits, expression) =
                    self.search_display(library_id, query, limit).await?;
                if matches!(
                    effective_mode,
                    super::models::KnowledgeRetrievalMode::DisplayHybrid
                ) {
                    return Err(String::from(
                        "当前版本尚未启用显示库向量融合执行器；请显式切换到显示库 BM25。",
                    ));
                }
                return Ok((display_hits, expression));
            }
        }
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        self.verify_index_gate(&space_id, &binding_id).await?;
        // tokenized_query is produced by the VLDB SQLite Jieba tokenizer and contains a safe MATCH expression.
        // tokenized_query 由 VLDB SQLite Jieba 分词器生成，并包含安全的 MATCH 表达式。
        let tokenized_query = self
            .controller
            .client
            .tokenize_sqlite_text(
                &space_id,
                &binding_id,
                ControllerSqliteTokenizerMode::Jieba,
                query,
                true,
            )
            .await
            .map_err(|error| format!("构建 Jieba FTS 查询失败：{error}"))?;
        // The generic VLDB search response computes highlights and snippets that this host discards.
        // 通用 VLDB 搜索响应会计算本宿主不使用的高亮与摘要，因此这里只读取标识符与 BM25 分值。
        let ranked_rows: Vec<FtsRankRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                &format!(
                    r#"SELECT id, bm25({KNOWLEDGE_FTS_INDEX}, 2.0, 1.0) AS raw_score
                       FROM {KNOWLEDGE_FTS_INDEX}
                       WHERE {KNOWLEDGE_FTS_INDEX} MATCH ?
                       ORDER BY raw_score ASC, file_path ASC, id ASC
                       LIMIT ?;"#,
                ),
                vec![text(&tokenized_query.fts_query), int(i64::from(limit))],
            )
            .await
            .map_err(|error| format!("执行 Jieba BM25 检索失败：{error}"))?;
        // hits preserves the exact rank order returned by the lightweight BM25 query.
        // hits 保留轻量 BM25 查询返回的精确排名顺序。
        let mut hits = Vec::with_capacity(ranked_rows.len());
        // fts_chunk_ids bounds the authoritative bulk materialization to ranked matches.
        // fts_chunk_ids 将权威批量物化限制在已排名的命中集合内。
        let fts_chunk_ids: Vec<String> = ranked_rows.iter().map(|row| row.id.clone()).collect();
        // authoritative_rows enables deterministic identifier lookup without additional controller calls.
        // authoritative_rows 支持确定性标识符查找且不再产生额外控制器调用。
        let mut authoritative_rows: BTreeMap<String, JoinedChunkRow> = self
            .load_joined_chunks_by_ids(&space_id, &binding_id, &fts_chunk_ids)
            .await?
            .into_iter()
            .map(|row| (row.id.clone(), row))
            .collect();
        for ranked_row in ranked_rows {
            // Materialize ranked hits from one bulk query to avoid one controller round trip per result.
            // 从单次批量查询物化排序命中，避免每条结果各发生一次控制器往返。
            let row = authoritative_rows
                .remove(&ranked_row.id)
                .ok_or_else(|| format!("FTS 命中的权威切片不存在：{}", ranked_row.id))?;
            hits.push(KnowledgeSearchHit {
                library_id: library.id.clone(),
                library_name: library.name.clone(),
                document_id: row.document_id,
                chunk_id: row.id,
                title: row.title,
                relative_path: row.relative_path,
                heading_path: row.heading_path,
                start_line: nonnegative_u64(row.start_line, "start_line")?,
                end_line: nonnegative_u64(row.end_line, "end_line")?,
                content: row.content,
                match_kind: String::from("bm25"),
                bm25_score: -ranked_row.raw_score,
                rrf_score: None,
            });
        }
        // SubstringFallback is intentionally cold: a successful BM25 query must never pay for a full authoritative scan.
        // SubstringFallback 刻意保持为冷路径：BM25 已有命中时绝不承担权威内容全表扫描。
        if hits.is_empty() {
            let substring_rows = self
                .load_substring_chunks(&space_id, &binding_id, query, MAX_SEARCH_LIMIT)
                .await?;
            for row in substring_rows {
                hits.push(KnowledgeSearchHit {
                    library_id: library.id.clone(),
                    library_name: library.name.clone(),
                    document_id: row.document_id,
                    chunk_id: row.id,
                    title: row.title,
                    relative_path: row.relative_path,
                    heading_path: row.heading_path,
                    start_line: nonnegative_u64(row.start_line, "start_line")?,
                    end_line: nonnegative_u64(row.end_line, "end_line")?,
                    content: row.content,
                    match_kind: String::from("substring"),
                    bm25_score: 0.0,
                    rrf_score: None,
                });
                if hits.len() >= limit as usize {
                    break;
                }
            }
        }
        Ok((hits, tokenized_query.fts_query))
    }
}
