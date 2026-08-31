//! Authoritative raw-corpus browsing, content reads, and bounded chunk materialization.
//! 权威原始内容层浏览、内容读取与有界切片物化。

use super::super::models::{
    KnowledgeChunk, KnowledgeDocumentContent, KnowledgeDocumentSummary, KnowledgeSourceSummary,
};
use super::{KnowledgeBaseRuntime, int, nonnegative_u64, text};
use serde::Deserialize;

/// Aggregate authoritative counts for registry projection.
/// 用于注册表投影的权威聚合计数。
#[derive(Debug, Deserialize)]
pub(super) struct CountRow {
    /// Document count.
    /// 文档数量。
    pub(super) document_count: i64,
    /// Chunk count.
    /// 切片数量。
    pub(super) chunk_count: i64,
}

/// Maximum document page size.
/// 文档分页的最大页大小。
const MAX_DOCUMENT_PAGE_SIZE: u32 = 500;

/// One source row with checked aggregate document counts for management listing.
/// 用于管理列表且带已检查文档聚合数量的一条来源记录。
#[derive(Debug, Deserialize)]
struct SourceSummaryRow {
    /// Immutable source identifier.
    /// 不可变来源标识符。
    id: String,
    /// Canonical native source root.
    /// 规范原生来源根目录。
    canonical_root: String,
    /// User-visible source directory name.
    /// 用户可见来源目录名称。
    display_name: String,
    /// SQLite aggregate count of authoritative documents.
    /// 权威文档的 SQLite 聚合数量。
    document_count: i64,
    /// SQLite aggregate count of authoritative chunks.
    /// 权威切片的 SQLite 聚合数量。
    chunk_count: i64,
    /// Source identity creation timestamp.
    /// 来源身份创建时间戳。
    created_at_ms: i64,
    /// Most recent source scan timestamp.
    /// 最近一次来源扫描时间戳。
    last_scanned_at_ms: i64,
}

/// Authoritative joined chunk row used for raw reads and ranked-hit materialization.
/// 用于原始读取与排名命中物化的权威联结切片记录。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct JoinedChunkRow {
    /// Immutable chunk identifier.
    /// 不可变切片标识符。
    pub(super) id: String,
    /// Owning document identifier.
    /// 所属文档标识符。
    pub(super) document_id: String,
    /// Zero-based ordinal.
    /// 从零开始的序号。
    ordinal: i64,
    /// Heading ancestry.
    /// 标题层级。
    pub(super) heading_path: String,
    /// One-based inclusive first line.
    /// 从一开始且包含边界的起始行。
    pub(super) start_line: i64,
    /// One-based inclusive last line.
    /// 从一开始且包含边界的结束行。
    pub(super) end_line: i64,
    /// Authoritative chunk content.
    /// 权威切片内容。
    pub(super) content: String,
    /// Mixed-language estimated token count.
    /// 中英文混合估算 token 数量。
    estimated_tokens: i64,
    /// Document title.
    /// 文档标题。
    pub(super) title: String,
    /// Relative document path.
    /// 文档相对路径。
    pub(super) relative_path: String,
}

/// Document list row including the derived chunk count.
/// 包含派生切片数量的文档列表记录。
#[derive(Debug, Deserialize)]
struct DocumentSummaryRow {
    /// Immutable document identifier.
    /// 不可变文档标识符。
    id: String,
    /// Owning source identifier.
    /// 所属来源标识符。
    source_id: String,
    /// Relative source path.
    /// 相对来源路径。
    relative_path: String,
    /// Folder classification.
    /// 文件夹分类。
    folder_path: String,
    /// Document title.
    /// 文档标题。
    title: String,
    /// Source byte size.
    /// 来源字节大小。
    byte_size: i64,
    /// Source line count.
    /// 来源行数。
    line_count: i64,
    /// Derived chunk count.
    /// 派生切片数量。
    chunk_count: i64,
    /// Last update timestamp.
    /// 最近更新时间戳。
    updated_at_ms: i64,
}

/// Distinct logical folder row used by lazy document navigation.
/// 延迟文档导航使用的唯一逻辑目录记录。
#[derive(Debug, Deserialize)]
struct DocumentFolderRow {
    /// Slash-normalized folder path; an empty value identifies the library root.
    /// 使用斜杠规范化的目录路径；空值表示知识库根目录。
    folder_path: String,
}

/// Full document row used by content reads.
/// 内容读取使用的完整文档记录。
#[derive(Debug, Deserialize)]
struct DocumentContentRow {
    /// Immutable document identifier.
    /// 不可变文档标识符。
    id: String,
    /// Owning source identifier.
    /// 所属来源标识符。
    source_id: String,
    /// Relative source path.
    /// 相对来源路径。
    relative_path: String,
    /// Folder classification.
    /// 文件夹分类。
    folder_path: String,
    /// Document title.
    /// 文档标题。
    title: String,
    /// Authoritative Markdown content.
    /// 权威 Markdown 内容。
    content: String,
    /// Source byte size.
    /// 来源字节大小。
    byte_size: i64,
    /// Source line count.
    /// 来源行数。
    line_count: i64,
    /// Derived chunk count.
    /// 派生切片数量。
    chunk_count: i64,
    /// Last update timestamp.
    /// 最近更新时间戳。
    updated_at_ms: i64,
}

impl KnowledgeBaseRuntime {
    /// Lists every durable source directory attached to one active library.
    /// 列出关联到一个活动知识库的全部持久化来源目录。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library identity.
    /// `library_id` 是精确的活动知识库身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the source summaries in creation/identity order, or a validation/query error.
    /// 返回按创建时间及身份排序的来源摘要，或校验/查询错误。
    pub async fn list_sources(
        &self,
        library_id: &str,
    ) -> Result<Vec<KnowledgeSourceSummary>, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let rows: Vec<SourceSummaryRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                r#"SELECT s.id, s.canonical_root, s.display_name,
                          COUNT(DISTINCT d.id) AS document_count,
                          COUNT(c.id) AS chunk_count,
                          s.created_at_ms, s.last_scanned_at_ms
                   FROM knowledge_sources s
                   LEFT JOIN knowledge_documents d ON d.source_id = s.id
                   LEFT JOIN knowledge_chunks c ON c.document_id = d.id
                   GROUP BY s.id, s.canonical_root, s.display_name, s.created_at_ms, s.last_scanned_at_ms
                   ORDER BY s.created_at_ms ASC, s.id ASC;"#,
                Vec::new(),
            )
            .await?;
        rows.into_iter().map(source_summary_from_row).collect()
    }

    /// Lists documents from one active library with bounded pagination.
    /// 使用有界分页列出一个活动知识库中的文档。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library identity.
    /// `library_id` 是精确的活动知识库身份。
    /// `folder_path` is the optional exact logical folder filter.
    /// `folder_path` 是可选的精确逻辑目录过滤条件。
    /// `limit` is the requested result/page bound limited by this API.
    /// `limit` 是由当前接口限制的请求结果或分页上限。
    /// `offset` is the zero-based document page offset.
    /// `offset` 是文档分页的零基偏移。
    ///
    /// # Returns
    /// 返回值
    /// Returns the bounded source-document page, or a validation/query error.
    /// 返回有界来源文档分页，或校验/查询错误。
    pub async fn list_documents(
        &self,
        library_id: &str,
        folder_path: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<KnowledgeDocumentSummary>, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let limit = limit.clamp(1, MAX_DOCUMENT_PAGE_SIZE);
        let (predicate, params) = if let Some(folder_path) = folder_path {
            (
                "WHERE d.folder_path = ?",
                vec![
                    text(folder_path),
                    int(i64::from(limit)),
                    int(i64::from(offset)),
                ],
            )
        } else {
            ("", vec![int(i64::from(limit)), int(i64::from(offset))])
        };
        let sql = format!(
            r#"SELECT d.id, d.source_id, d.relative_path, d.folder_path, d.title,
                       d.byte_size, d.line_count, COUNT(c.id) AS chunk_count, d.updated_at_ms
                FROM knowledge_documents d
                LEFT JOIN knowledge_chunks c ON c.document_id = d.id
                {predicate}
                GROUP BY d.id
                ORDER BY d.relative_path_key ASC
                LIMIT ? OFFSET ?;"#
        );
        let rows: Vec<DocumentSummaryRow> = self
            .query_library_rows(&space_id, &binding_id, &sql, params)
            .await?;
        rows.into_iter().map(document_summary_from_row).collect()
    }

    /// Lists only distinct logical folder paths without reading document rows or bodies.
    /// 仅列出唯一逻辑目录路径，不读取文档记录或正文。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library identity.
    /// `library_id` 是精确的活动知识库身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the sorted distinct logical folders, or a validation/query error.
    /// 返回排序后的唯一逻辑目录，或校验/查询错误。
    pub async fn list_document_folders(&self, library_id: &str) -> Result<Vec<String>, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let rows: Vec<DocumentFolderRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT DISTINCT folder_path FROM knowledge_documents ORDER BY folder_path ASC;",
                Vec::new(),
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.folder_path).collect())
    }

    /// Reads one authoritative document summary without loading its Markdown body.
    /// 在不加载 Markdown 正文的情况下读取一份权威文档摘要。
    ///
    /// `library_id` identifies the active library and `document_id` identifies the exact source
    /// document shared with derived corpus rows.
    /// `library_id` 标识活动知识库，`document_id` 标识与派生内容层记录共享的精确原始文档。
    ///
    /// Returns exact source metadata and chunk counts, or an error when the document is absent.
    /// 返回精确来源元数据与切片数量；文档不存在时返回错误。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library identity.
    /// `library_id` 是精确的活动知识库身份。
    /// `document_id` is the immutable authoritative source-document identity.
    /// `document_id` 是不可变的权威来源文档身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact source metadata without loading Markdown, or an error.
    /// 返回不加载 Markdown 正文的精确来源元数据，或错误。
    pub(super) async fn read_document_summary(
        &self,
        library_id: &str,
        document_id: &str,
    ) -> Result<KnowledgeDocumentSummary, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        // rows contains at most one exact source-document summary and never its Markdown body.
        // rows 最多包含一条精确原始文档摘要，且绝不包含 Markdown 正文。
        let rows: Vec<DocumentSummaryRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                r#"SELECT d.id, d.source_id, d.relative_path, d.folder_path, d.title,
                           d.byte_size, d.line_count, COUNT(c.id) AS chunk_count, d.updated_at_ms
                    FROM knowledge_documents d
                    LEFT JOIN knowledge_chunks c ON c.document_id = d.id
                    WHERE d.id = ? GROUP BY d.id;"#,
                vec![text(document_id)],
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| format!("资料文档不存在：{document_id}"))
            .and_then(document_summary_from_row)
    }

    /// Reads one authoritative document by ID without exposing physical source paths.
    /// 按标识符读取一份权威文档，且不暴露物理来源路径。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library identity.
    /// `library_id` 是精确的活动知识库身份。
    /// `document_id` is the immutable authoritative source-document identity.
    /// `document_id` 是不可变的权威来源文档身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the authoritative Markdown and source metadata, or an error.
    /// 返回权威 Markdown 及来源元数据，或错误。
    pub async fn read_document(
        &self,
        library_id: &str,
        document_id: &str,
    ) -> Result<KnowledgeDocumentContent, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let rows: Vec<DocumentContentRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                r#"SELECT d.id, d.source_id, d.relative_path, d.folder_path, d.title, d.content,
                           d.byte_size, d.line_count, COUNT(c.id) AS chunk_count, d.updated_at_ms
                    FROM knowledge_documents d
                    LEFT JOIN knowledge_chunks c ON c.document_id = d.id
                    WHERE d.id = ? GROUP BY d.id;"#,
                vec![text(document_id)],
            )
            .await?;
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| format!("资料文档不存在：{document_id}"))?;
        let document = KnowledgeDocumentSummary {
            id: row.id,
            source_id: row.source_id,
            relative_path: row.relative_path,
            folder_path: row.folder_path,
            title: row.title,
            byte_size: nonnegative_u64(row.byte_size, "byte_size")?,
            line_count: nonnegative_u64(row.line_count, "line_count")?,
            chunk_count: nonnegative_u64(row.chunk_count, "chunk_count")?,
            updated_at_ms: row.updated_at_ms,
        };
        Ok(KnowledgeDocumentContent {
            document,
            content: row.content,
        })
    }

    /// Reads one authoritative chunk by ID.
    /// 按标识符读取一个权威切片。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library identity.
    /// `library_id` 是精确的活动知识库身份。
    /// `chunk_id` is the immutable authoritative chunk identity.
    /// `chunk_id` 是不可变的权威切片身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the checked authoritative chunk, or an error.
    /// 返回经校验的权威切片，或错误。
    pub async fn read_chunk(
        &self,
        library_id: &str,
        chunk_id: &str,
    ) -> Result<KnowledgeChunk, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let row = self
            .load_joined_chunk(&space_id, &binding_id, chunk_id)
            .await?;
        chunk_from_joined_row(&row)
    }

    /// Loads deterministic case-insensitive substring matches from authoritative Markdown chunks.
    /// 从权威 Markdown 切片加载确定性的、不区分大小写的子串命中。
    ///
    /// # Parameters
    /// 参数
    /// `limit` is the requested result/page bound limited by this API.
    /// `limit` 是由当前接口限制的请求结果或分页上限。
    /// `space_id` is the already derived controller space identity.
    /// `space_id` 是已派生的控制器空间身份。
    /// `binding_id` is the exact authoritative SQLite binding.
    /// `binding_id` 是精确权威 SQLite 绑定。
    /// `query` is the caller query used by the selected retrieval policy.
    /// `query` 是选定检索策略使用的调用方查询。
    ///
    /// # Returns
    /// 返回值
    /// Returns the deterministically ranked substring rows, or a query error.
    /// 返回确定性排名的子串记录，或查询错误。
    pub(super) async fn load_substring_chunks(
        &self,
        space_id: &str,
        binding_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<JoinedChunkRow>, String> {
        self.query_library_rows(
            space_id,
            binding_id,
            r#"WITH needle(value) AS (SELECT lower(?))
               SELECT c.id, c.document_id, c.ordinal, c.heading_path, c.start_line, c.end_line,
                      c.content, c.estimated_tokens, d.title, d.relative_path
               FROM knowledge_chunks c
               INNER JOIN knowledge_documents d ON d.id = c.document_id
               CROSS JOIN needle
               WHERE instr(lower(c.content), needle.value) > 0
                  OR instr(lower(c.heading_path), needle.value) > 0
                  OR instr(lower(d.title), needle.value) > 0
               ORDER BY CASE
                            WHEN instr(lower(d.title), needle.value) > 0 THEN 0
                            WHEN instr(lower(c.heading_path), needle.value) > 0 THEN 1
                            ELSE 2
                        END,
                        d.relative_path_key ASC,
                        c.ordinal ASC
               LIMIT ?;"#,
            vec![text(query), int(i64::from(limit))],
        )
        .await
    }

    /// Loads one authoritative joined chunk or returns an explicit integrity error.
    /// 加载一个权威联结切片，若不存在则返回显式完整性错误。
    ///
    /// # Parameters
    /// 参数
    /// `chunk_id` is the immutable authoritative chunk identity.
    /// `chunk_id` 是不可变的权威切片身份。
    /// `space_id` is the already derived controller space identity.
    /// `space_id` 是已派生的控制器空间身份。
    /// `binding_id` is the exact authoritative SQLite binding.
    /// `binding_id` 是精确权威 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact joined source row, or an explicit missing-row error.
    /// 返回精确联结来源记录，或明确的记录缺失错误。
    async fn load_joined_chunk(
        &self,
        space_id: &str,
        binding_id: &str,
        chunk_id: &str,
    ) -> Result<JoinedChunkRow, String> {
        let rows: Vec<JoinedChunkRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                r#"SELECT c.id, c.document_id, c.ordinal, c.heading_path, c.start_line, c.end_line,
                           c.content, c.estimated_tokens, d.title, d.relative_path
                    FROM knowledge_chunks c
                    INNER JOIN knowledge_documents d ON d.id = c.document_id
                    WHERE c.id = ?;"#,
                vec![text(chunk_id)],
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| format!("FTS 命中的权威切片不存在：{chunk_id}"))
    }

    /// Loads authoritative joined chunks for one exact bounded identifier set.
    /// 为一个精确且有界的标识符集合批量加载权威联结切片。
    ///
    /// # Parameters
    /// 参数
    /// `chunk_ids` is the exact bounded identifiers selected for bulk materialization.
    /// `chunk_ids` 是选定进行批量物化的精确有界身份集合。
    /// `space_id` is the already derived controller space identity.
    /// `space_id` 是已派生的控制器空间身份。
    /// `binding_id` is the exact authoritative SQLite binding.
    /// `binding_id` 是精确权威 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the joined rows for those identifiers, or a query error.
    /// 返回该身份集合对应的联结记录，或查询错误。
    pub(super) async fn load_joined_chunks_by_ids(
        &self,
        space_id: &str,
        binding_id: &str,
        chunk_ids: &[String],
    ) -> Result<Vec<JoinedChunkRow>, String> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        // placeholders contains one positional parameter for each validated identifier.
        // placeholders 为每个已验证标识符包含一个位置参数。
        let placeholders = std::iter::repeat_n("?", chunk_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        // sql interpolates only the generated placeholder list; all identifiers remain bound values.
        // sql 只插入生成的占位符列表，所有标识符仍作为绑定值传入。
        let sql = format!(
            r#"SELECT c.id, c.document_id, c.ordinal, c.heading_path, c.start_line, c.end_line,
                      c.content, c.estimated_tokens, d.title, d.relative_path
               FROM knowledge_chunks c
               INNER JOIN knowledge_documents d ON d.id = c.document_id
               WHERE c.id IN ({placeholders});"#,
        );
        // params preserves the exact identifier set as typed SQLite text values.
        // params 将精确标识符集合保存为类型化 SQLite 文本值。
        let params = chunk_ids.iter().map(text).collect();
        self.query_library_rows(space_id, binding_id, &sql, params)
            .await
    }

    /// Returns authoritative document and chunk counts.
    /// 返回权威文档与切片数量。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated controller space.
    /// `space_id` 是已解析的隔离控制器空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the authoritative document and chunk totals, or a query error.
    /// 返回权威文档和切片总数，或查询错误。
    pub(super) async fn authoritative_counts(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<CountRow, String> {
        let rows: Vec<CountRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT (SELECT COUNT(*) FROM knowledge_documents) AS document_count, (SELECT COUNT(*) FROM knowledge_chunks) AS chunk_count;",
                Vec::new(),
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| String::from("读取知识库权威计数失败。"))
    }
}

/// Maps one document row into the checked public summary.
/// 将一条文档记录映射为已检查的公共摘要。
///
/// # Parameters
/// 参数
/// `row` is the exact typed authoritative query row to project.
/// `row` 是待投影的精确类型化权威查询记录。
///
/// # Returns
/// 返回值
/// Returns the source-document summary with nonnegative counts, or a conversion error.
/// 返回计数非负的来源文档摘要，或转换错误。
fn document_summary_from_row(row: DocumentSummaryRow) -> Result<KnowledgeDocumentSummary, String> {
    Ok(KnowledgeDocumentSummary {
        id: row.id,
        source_id: row.source_id,
        relative_path: row.relative_path,
        folder_path: row.folder_path,
        title: row.title,
        byte_size: nonnegative_u64(row.byte_size, "byte_size")?,
        line_count: nonnegative_u64(row.line_count, "line_count")?,
        chunk_count: nonnegative_u64(row.chunk_count, "chunk_count")?,
        updated_at_ms: row.updated_at_ms,
    })
}

/// Maps one aggregate source row into the checked public management summary.
/// 将一条聚合来源记录映射为已检查的公共管理摘要。
///
/// # Parameters
/// 参数
/// `row` is the exact typed authoritative query row to project.
/// `row` 是待投影的精确类型化权威查询记录。
///
/// # Returns
/// 返回值
/// Returns the source summary with nonnegative aggregate counts, or a conversion error.
/// 返回聚合计数非负的来源摘要，或转换错误。
fn source_summary_from_row(row: SourceSummaryRow) -> Result<KnowledgeSourceSummary, String> {
    Ok(KnowledgeSourceSummary {
        id: row.id,
        canonical_root: row.canonical_root,
        display_name: row.display_name,
        document_count: nonnegative_u64(row.document_count, "document_count")?,
        chunk_count: nonnegative_u64(row.chunk_count, "chunk_count")?,
        created_at_ms: row.created_at_ms,
        last_scanned_at_ms: row.last_scanned_at_ms,
    })
}

/// Maps one joined row into a checked public chunk.
/// 将一条联结记录映射为已检查的公共切片。
///
/// # Parameters
/// 参数
/// `row` is the exact typed authoritative query row to project.
/// `row` 是待投影的精确类型化权威查询记录。
///
/// # Returns
/// 返回值
/// Returns the source chunk with checked positions/counts, or a conversion error.
/// 返回位置和计数经检查的来源切片，或转换错误。
fn chunk_from_joined_row(row: &JoinedChunkRow) -> Result<KnowledgeChunk, String> {
    Ok(KnowledgeChunk {
        id: row.id.clone(),
        document_id: row.document_id.clone(),
        ordinal: nonnegative_u64(row.ordinal, "ordinal")?,
        heading_path: row.heading_path.clone(),
        start_line: nonnegative_u64(row.start_line, "start_line")?,
        end_line: nonnegative_u64(row.end_line, "end_line")?,
        content: row.content.clone(),
        estimated_tokens: nonnegative_u64(row.estimated_tokens, "estimated_tokens")?,
    })
}
