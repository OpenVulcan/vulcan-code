//! Display generation lifecycle, curated corpus reads, and display-ranked retrieval.
//! 显示代次生命周期、整理内容读取与显示层排名检索。

use super::super::models::{
    KnowledgeDisplayChunk, KnowledgeDisplayDocument, KnowledgeDisplayIndexStatus,
    KnowledgeDocumentContent, KnowledgeDocumentSummary, KnowledgeLibrarySummary,
    KnowledgeModelSnapshot, KnowledgeSearchHit, KnowledgeVectorIndexStatus,
};
use super::super::schema::{KNOWLEDGE_DISPLAY_FTS_INDEX_PREFIX, display_schema_script};
use super::vector::vector_table_name;
use super::{
    CountRow, KnowledgeBaseRuntime, MAX_SEARCH_LIMIT, RRF_K, int, nonnegative_u64, text,
    unix_epoch_millis, validate_generation_id,
};
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use serde::Deserialize;
use std::collections::BTreeMap;
use vulcan_sqlite_engine::tokenizer::TokenizerMode;

/// One display-document count returned by a generation aggregate query.
/// 一次显示库代次聚合查询返回的文档计数。
#[derive(Debug, Deserialize)]
struct DisplayCountRow {
    /// Number of display documents in the generation.
    /// 当前代次中的显示文档数量。
    count: i64,
}

/// One source-document hash stored in a resumable display generation.
/// 可恢复显示代次中保存的一份来源文档哈希。
#[derive(Debug, Deserialize)]
struct DisplaySourceHashRow {
    /// Stable source document identifier.
    /// 稳定的原始文档标识符。
    source_document_id: String,
    /// SHA-256 hash captured when the display document was generated.
    /// 生成显示文档时记录的 SHA-256 哈希。
    source_sha256: String,
}

/// Active-generation display metadata returned by one aggregate query.
/// 一次聚合查询返回的活动显示代次元数据。
#[derive(Debug, Deserialize)]
struct DisplayMetadataRow {
    /// Active display generation, if one has been published.
    /// 已发布时的活动显示库代次。
    active_generation: Option<String>,
    /// Active generation-scoped FTS index name.
    /// 活动代次对应的 FTS 索引名称。
    active_fts_index: Option<String>,
}

/// Complete display-document row decoded from the derived SQLite database.
/// 从派生 SQLite 显示库解码的一份完整显示文档记录。
#[derive(Debug, Deserialize)]
struct DisplayDocumentRow {
    /// Stable display-document identifier.
    /// 稳定显示文档标识符。
    id: String,
    /// Source document identifier.
    /// 原始文档标识符。
    source_document_id: String,
    /// Source file hash.
    /// 来源文件哈希。
    source_sha256: String,
    /// Relative source path.
    /// 相对来源路径。
    relative_path: String,
    /// Source folder path.
    /// 来源文件夹路径。
    folder_path: String,
    /// Curated display title.
    /// 整理后的显示标题。
    title: String,
    /// Curated summary.
    /// 整理后的摘要。
    summary: String,
    /// JSON encoded keyword list.
    /// JSON 编码的关键词列表。
    keywords_json: String,
    /// JSON encoded symbol list.
    /// JSON 编码的符号列表。
    symbols_json: String,
    /// Semantic text used for embeddings.
    /// 用于向量生成的语义文本。
    embedding_text: String,
    /// Normalized display Markdown.
    /// 规范化显示 Markdown。
    markdown: String,
    /// Generation identity.
    /// 代次身份。
    generation: String,
    /// Successful generation timestamp.
    /// 生成成功时间戳。
    generated_at_ms: i64,
}

impl DisplayDocumentRow {
    /// Converts one decoded database row into the public display-document contract.
    /// 将一条已解码数据库记录转换为公开显示文档契约。
    ///
    /// Consumes this exact row; returns the public document or a keyword/symbol JSON decoding error.
    /// 消费当前精确记录；返回公开文档，或关键词/符号 JSON 解码错误。
    fn into_display_document(self) -> Result<KnowledgeDisplayDocument, String> {
        let keywords = serde_json::from_str(&self.keywords_json)
            .map_err(|error| format!("解析显示库关键词失败：{error}"))?;
        let symbols = serde_json::from_str(&self.symbols_json)
            .map_err(|error| format!("解析显示库符号失败：{error}"))?;
        Ok(KnowledgeDisplayDocument {
            id: self.id,
            source_document_id: self.source_document_id,
            source_sha256: self.source_sha256,
            relative_path: self.relative_path,
            folder_path: self.folder_path,
            title: self.title,
            summary: self.summary,
            keywords,
            symbols,
            embedding_text: self.embedding_text,
            markdown: self.markdown,
            generation: self.generation,
            generated_at_ms: self.generated_at_ms,
        })
    }
}

/// One display search row joined with its curated document metadata.
/// 一条联结整理文档元数据的显示库搜索记录。
#[derive(Debug, Deserialize)]
struct DisplaySearchRow {
    /// Stable display chunk identifier.
    /// 稳定显示库切片标识符。
    id: String,
    /// Source document identifier.
    /// 原始来源文档标识符。
    source_document_id: String,
    /// Display title.
    /// 显示标题。
    title: String,
    /// Relative source path.
    /// 相对来源路径。
    relative_path: String,
    /// Heading ancestry.
    /// 标题层级。
    heading_path: String,
    /// Display chunk start line.
    /// 显示切片起始行。
    start_line: i64,
    /// Display chunk end line.
    /// 显示切片结束行。
    end_line: i64,
    /// Display chunk content.
    /// 显示切片内容。
    content: String,
    /// Raw ascending BM25 score.
    /// 原始升序 BM25 分值。
    raw_score: f64,
}

/// One display-chunk row decoded for vector generation.
/// 为向量生成解码的一条显示库切片记录。
#[derive(Debug, Deserialize)]
struct DisplayChunkRow {
    /// Stable display chunk identifier.
    /// 稳定显示库切片标识符。
    id: String,
    /// Owning display-document identifier.
    /// 所属显示文档标识符。
    display_document_id: String,
    /// Zero-based chunk ordinal.
    /// 从零开始的切片序号。
    ordinal: i64,
    /// Display Markdown heading ancestry.
    /// 显示 Markdown 标题层级。
    heading_path: String,
    /// One-based inclusive display line range start.
    /// 从一开始且包含边界的显示行范围起点。
    start_line: i64,
    /// One-based inclusive display line range end.
    /// 从一开始且包含边界的显示行范围终点。
    end_line: i64,
    /// Display chunk content.
    /// 显示库切片正文。
    content: String,
    /// Stable semantic text used for embeddings.
    /// 用于生成向量的稳定语义文本。
    embedding_text: String,
    /// Generation that owns this row.
    /// 拥有当前记录的显示库代次。
    generation: String,
}

impl DisplayChunkRow {
    /// Converts one decoded row into the public display-chunk contract.
    /// 将一条已解码记录转换为公开显示库切片契约。
    ///
    /// Consumes this exact row; returns the public chunk or an invalid negative ordinal/line error.
    /// 消费当前精确记录；返回公开切片，或序号/行号非法负值错误。
    fn into_display_chunk(self) -> Result<KnowledgeDisplayChunk, String> {
        Ok(KnowledgeDisplayChunk {
            id: self.id,
            display_document_id: self.display_document_id,
            ordinal: nonnegative_u64(self.ordinal, "display_chunk_ordinal")?,
            heading_path: self.heading_path,
            start_line: nonnegative_u64(self.start_line, "display_chunk_start_line")?,
            end_line: nonnegative_u64(self.end_line, "display_chunk_end_line")?,
            content: self.content,
            embedding_text: self.embedding_text,
            generation: self.generation,
        })
    }
}

impl KnowledgeBaseRuntime {
    /// Ensures the AI-curated display database and schema are available before generation setup.
    /// 在代次初始化前确保 AI 整理显示库数据库与结构可用。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the display schema is initialized, or its explicit error.
    /// 返回显示结构初始化成功，或其明确错误。
    pub async fn ensure_display_storage(&self, library_id: &str) -> Result<(), String> {
        self.require_active_library(library_id).await?;
        let (_space_id, binding_id) = self.activate_display_binding(library_id, true).await?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            display_schema_script(library_id),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("初始化知识库显示库 SQLite 结构失败：{error}"))?;
        Ok(())
    }

    /// Starts one isolated display generation and returns its FTS index identity.
    /// 启动一个隔离显示库代次并返回其 FTS 索引身份。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the prepared generation FTS index name, or a setup error.
    /// 返回已预备的代次 FTS 索引名称，或初始化错误。
    pub async fn prepare_display_generation(
        &self,
        library_id: &str,
        generation: &str,
    ) -> Result<String, String> {
        validate_generation_id(generation)?;
        self.ensure_display_storage(library_id).await?;
        let (_space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let fts_index = display_fts_index_name(generation)?;
        local_sql::ensure_fts_index_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            &fts_index,
            TokenizerMode::Jieba,
        )
        .await
        .map_err(|error| format!("初始化显示库 Jieba FTS 索引失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            "DELETE FROM display_chunks WHERE generation = ?;",
            vec![text(generation)],
        )
        .await
        .map_err(|error| format!("清理显示库代次切片失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            "DELETE FROM display_documents WHERE generation = ?;",
            vec![text(generation)],
        )
        .await
        .map_err(|error| format!("清理显示库代次文档失败：{error}"))?;
        Ok(fts_index)
    }

    /// Reopens one pending display generation without deleting its completed rows.
    /// 重新打开一个待完成显示代次，但不删除其中已经完成的记录。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the reopened FTS index name without clearing existing rows, or an error.
    /// 返回不清理已有记录的重新打开 FTS 索引名称，或错误。
    pub async fn resume_display_generation(
        &self,
        library_id: &str,
        generation: &str,
    ) -> Result<String, String> {
        validate_generation_id(generation)?;
        self.ensure_display_storage(library_id).await?;
        let (_space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let fts_index = display_fts_index_name(generation)?;
        local_sql::ensure_fts_index_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            &fts_index,
            TokenizerMode::Jieba,
        )
        .await
        .map_err(|error| format!("恢复显示库 Jieba FTS 索引失败：{error}"))?;
        Ok(fts_index)
    }

    /// Persists the pending generation identity and curation provenance before work starts.
    /// 在开始工作前持久化待完成代次身份与整理配置来源。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    /// `source_content_revision` is the authoritative raw-content revision captured by the builder.
    /// `source_content_revision` 是构建器记录的原始内容权威修订。
    ///
    /// # Returns
    /// 返回值
    /// Returns the updated persisted library summary, or a validation/storage error.
    /// 返回更新后持久化知识库摘要，或校验/存储错误。
    pub async fn begin_display_generation(
        &self,
        library_id: &str,
        generation: &str,
        source_content_revision: u64,
    ) -> Result<KnowledgeLibrarySummary, String> {
        validate_generation_id(generation)?;
        let _guard = self.library_mutation_guard(library_id).await?;
        let current = self.require_active_library(library_id).await?;
        if !current.settings.curation.enabled {
            return Err(String::from("当前资料库未启用显示库整理"));
        }
        let mut settings = current.settings.clone();
        settings.display_index.status = KnowledgeDisplayIndexStatus::Building;
        settings.display_index.pending_generation = Some(generation.to_string());
        settings.display_index.pending_source_content_revision = Some(source_content_revision);
        settings.display_index.pending_prompt_version =
            Some(settings.curation.prompt_version.clone());
        settings.display_index.pending_model_fingerprint =
            settings.curation.desired_model_fingerprint.clone();
        settings.display_index.last_error = None;
        settings.validate()?;
        let settings_json = serde_json::to_string(&settings)
            .map_err(|error| format!("序列化显示库待完成状态失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_libraries SET settings_json = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    text(&settings_json),
                    int(unix_epoch_millis()?),
                    text(library_id),
                ],
            ).await
            .map_err(|error| format!("保存显示库待完成状态失败：{error}"))?;
        self.get_library(library_id).await
    }

    /// Lists source hashes already written to one pending display generation.
    /// 列出待完成显示代次中已经写入的原始文档哈希。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    /// `fts_index` is the generation-scoped display FTS identity provided by the build workflow.
    /// `fts_index` 是构建流程提供的代次级显示 FTS 身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the source-id to hash map whose chunks have complete FTS coverage, or an error.
    /// 返回切片具有完整 FTS 覆盖的来源身份到哈希映射，或错误。
    pub async fn list_display_generation_source_hashes(
        &self,
        library_id: &str,
        generation: &str,
        fts_index: &str,
    ) -> Result<BTreeMap<String, String>, String> {
        validate_generation_id(generation)?;
        validate_display_fts_index(fts_index)?;
        let (space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let sql = format!(
            "SELECT d.source_document_id, d.source_sha256 \
                 FROM display_documents AS d \
                 WHERE d.generation = ? \
                   AND EXISTS (SELECT 1 FROM display_chunks AS c \
                               WHERE c.display_document_id = d.id AND c.generation = d.generation) \
                   AND (SELECT COUNT(*) FROM display_chunks AS c \
                        JOIN {fts_index} AS f ON f.id = c.id \
                        WHERE c.display_document_id = d.id AND c.generation = d.generation) \
                       = (SELECT COUNT(*) FROM display_chunks AS c \
                          WHERE c.display_document_id = d.id AND c.generation = d.generation);"
        );
        let rows: Vec<DisplaySourceHashRow> = self
            .query_library_rows(&space_id, &binding_id, &sql, vec![text(generation)])
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.source_document_id, row.source_sha256))
            .collect())
    }

    /// Removes one incomplete or stale source document from a pending generation before retrying it.
    /// 在重试来源文档前，从待完成代次中移除不完整或过期的记录。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    /// `fts_index` is the generation-scoped display FTS identity provided by the build workflow.
    /// `fts_index` 是构建流程提供的代次级显示 FTS 身份。
    /// `source_document_id` is the immutable original-document identity.
    /// `source_document_id` 是不可变的原始文档身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub async fn discard_display_generation_source(
        &self,
        library_id: &str,
        generation: &str,
        fts_index: &str,
        source_document_id: &str,
    ) -> Result<(), String> {
        validate_generation_id(generation)?;
        validate_display_fts_index(fts_index)?;
        let (_space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let delete_fts_sql = format!(
            "DELETE FROM {fts_index} WHERE id IN (SELECT c.id FROM display_chunks AS c JOIN display_documents AS d ON d.id = c.display_document_id WHERE d.source_document_id = ? AND d.generation = ? AND c.generation = ?);"
        );
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            &delete_fts_sql,
            vec![text(source_document_id), text(generation), text(generation)],
        )
        .await
        .map_err(|error| format!("清理显示库旧 FTS 切片失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
                "DELETE FROM display_chunks WHERE display_document_id IN (SELECT id FROM display_documents WHERE source_document_id = ? AND generation = ?);",
                vec![text(source_document_id), text(generation)],
            ).await
            .map_err(|error| format!("清理显示库旧切片失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            "DELETE FROM display_documents WHERE source_document_id = ? AND generation = ?;",
            vec![text(source_document_id), text(generation)],
        )
        .await
        .map_err(|error| format!("清理显示库旧文档失败：{error}"))?;
        Ok(())
    }

    /// Writes one validated display document and its searchable chunks into a pending generation.
    /// 将一份已校验显示文档及其可检索切片写入待发布代次。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `fts_index` is the generation-scoped display FTS identity provided by the build workflow.
    /// `fts_index` 是构建流程提供的代次级显示 FTS 身份。
    /// `document` is the curated document including its source identity and generation.
    /// `document` 是包含来源身份和代次的整理文档。
    /// `chunks` is the ordered nonempty display chunks in the document generation.
    /// `chunks` 是文档代次内有序且非空的显示切片。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub async fn write_display_document(
        &self,
        library_id: &str,
        fts_index: &str,
        document: &super::models::KnowledgeDisplayDocument,
        chunks: &[super::models::KnowledgeDisplayChunk],
    ) -> Result<(), String> {
        validate_generation_id(document.generation.as_str())?;
        if document.generation.trim().is_empty() || chunks.is_empty() {
            return Err(String::from("显示库文档代次与切片不能为空"));
        }
        if chunks
            .iter()
            .any(|chunk| chunk.generation != document.generation)
        {
            return Err(String::from("显示库文档与切片代次不一致"));
        }
        let (_space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let keywords_json = serde_json::to_string(&document.keywords)
            .map_err(|error| format!("序列化显示库关键词失败：{error}"))?;
        let symbols_json = serde_json::to_string(&document.symbols)
            .map_err(|error| format!("序列化显示库符号失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            r#"INSERT INTO display_documents(
                    id, source_document_id, source_sha256, relative_path, folder_path, title,
                    summary, keywords_json, symbols_json, embedding_text, markdown, generation,
                    generated_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#,
            vec![
                text(&document.id),
                text(&document.source_document_id),
                text(&document.source_sha256),
                text(&document.relative_path),
                text(&document.folder_path),
                text(&document.title),
                text(&document.summary),
                text(&keywords_json),
                text(&symbols_json),
                text(&document.embedding_text),
                text(&document.markdown),
                text(&document.generation),
                int(document.generated_at_ms),
            ],
        )
        .await
        .map_err(|error| format!("写入显示库文档失败：{error}"))?;
        for chunk in chunks {
            local_sql::execute_script_async(
                &self.host_runtime.local,
                self.resolve_library_binding(&binding_id)?,
                r#"INSERT INTO display_chunks(
                        id, display_document_id, ordinal, heading_path, start_line, end_line,
                        content, embedding_text, generation
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?);"#,
                vec![
                    text(&chunk.id),
                    text(&chunk.display_document_id),
                    int(i64::try_from(chunk.ordinal)
                        .map_err(|_| String::from("显示库切片序号超出 SQLite 整数范围"))?),
                    text(&chunk.heading_path),
                    int(i64::try_from(chunk.start_line)
                        .map_err(|_| String::from("显示库切片起始行超出 SQLite 整数范围"))?),
                    int(i64::try_from(chunk.end_line)
                        .map_err(|_| String::from("显示库切片结束行超出 SQLite 整数范围"))?),
                    text(&chunk.content),
                    text(&chunk.embedding_text),
                    text(&chunk.generation),
                ],
            )
            .await
            .map_err(|error| format!("写入显示库切片失败：{error}"))?;
            local_sql::upsert_fts_document_async(
                &self.host_runtime.local,
                self.resolve_library_binding(&binding_id)?,
                fts_index,
                TokenizerMode::Jieba,
                &chunk.id,
                &document.relative_path,
                &document.title,
                &format!(
                    "{}\n{}\n{}\n{}",
                    document.title,
                    document.summary,
                    document.keywords.join(" "),
                    chunk.content
                ),
            )
            .await
            .map_err(|error| format!("写入显示库 Jieba FTS 切片失败：{error}"))?;
        }
        Ok(())
    }

    /// Publishes display generation metadata after checking document presence and source revision.
    /// 检查文档存在与来源修订后发布显示库代次元数据。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    /// `source_content_revision` is the authoritative raw-content revision captured by the builder.
    /// `source_content_revision` 是构建器记录的原始内容权威修订。
    /// `fts_index` is the generation-scoped display FTS identity provided by the build workflow.
    /// `fts_index` 是构建流程提供的代次级显示 FTS 身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the published document count after revision checks and metadata writes, or an error.
    /// 返回修订检查和元数据写入后的已发布文档数，或错误。
    pub async fn activate_display_generation(
        &self,
        library_id: &str,
        generation: &str,
        fts_index: &str,
        source_content_revision: i64,
    ) -> Result<u64, String> {
        validate_generation_id(generation)?;
        let (space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let rows: Vec<DisplayCountRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT COUNT(*) AS count FROM display_documents WHERE generation = ?;",
                vec![text(generation)],
            )
            .await?;
        let document_count = rows
            .first()
            .map(|row| nonnegative_u64(row.count, "display_document_count"))
            .transpose()?
            .unwrap_or(0);
        if document_count == 0 {
            return Err(String::from("显示库代次没有可发布文档"));
        }
        let source_integrity = self.inspect_index_integrity(library_id).await?;
        if source_integrity.content_revision != source_content_revision as u64 {
            return Err(String::from(
                "原始库在显示库生成期间发生变化，已拒绝发布过期显示代次",
            ));
        }
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
                "INSERT INTO display_metadata(key, value) VALUES ('active_generation', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
                vec![text(generation)],
            ).await
            .map_err(|error| format!("发布显示库活动代次失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
                "INSERT INTO display_metadata(key, value) VALUES ('active_fts_index', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
                vec![text(fts_index)],
            ).await
            .map_err(|error| format!("发布显示库活动 FTS 索引失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
                "INSERT INTO display_metadata(key, value) VALUES ('source_content_revision', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
                vec![int(source_content_revision)],
            ).await
            .map_err(|error| format!("发布显示库来源修订失败：{error}"))?;
        Ok(document_count)
    }

    /// Persists a successful display generation without re-evaluating its unchanged model policy.
    /// 持久化成功显示库代次，但不重新计算未变化的模型策略。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    /// `source_content_revision` is the authoritative raw-content revision captured by the builder.
    /// `source_content_revision` 是构建器记录的原始内容权威修订。
    /// `curation_snapshot` is the model snapshot used for the completed curation run.
    /// `curation_snapshot` 是已完成整理任务使用的模型快照。
    /// `document_count` is the published display-document count.
    /// `document_count` 是已发布的显示文档数量。
    ///
    /// # Returns
    /// 返回值
    /// Returns the updated persisted library summary, or a validation/storage error.
    /// 返回更新后持久化知识库摘要，或校验/存储错误。
    pub async fn mark_display_generation_ready(
        &self,
        library_id: &str,
        generation: &str,
        source_content_revision: u64,
        curation_snapshot: KnowledgeModelSnapshot,
        document_count: u64,
    ) -> Result<KnowledgeLibrarySummary, String> {
        validate_generation_id(generation)?;
        let _guard = self.library_mutation_guard(library_id).await?;
        let current = self.require_active_library(library_id).await?;
        if !current.settings.curation.enabled {
            return Err(String::from("当前资料库未启用显示库整理"));
        }
        let mut settings = current.settings.clone();
        settings.display_index.status = KnowledgeDisplayIndexStatus::Ready;
        settings.display_index.active_generation = Some(generation.to_string());
        settings.display_index.pending_generation = None;
        settings.display_index.pending_source_content_revision = None;
        settings.display_index.pending_prompt_version = None;
        settings.display_index.pending_model_fingerprint = None;
        settings.display_index.source_content_revision = Some(source_content_revision);
        settings.display_index.indexed_prompt_version =
            Some(settings.curation.prompt_version.clone());
        settings.display_index.indexed_model_fingerprint =
            settings.curation.desired_model_fingerprint.clone();
        settings.display_index.generated_document_count = document_count;
        settings.display_index.last_error = None;
        if settings.embedding.enabled {
            settings.vector_index.status = KnowledgeVectorIndexStatus::Stale;
            settings.vector_index.active_generation = None;
            settings.vector_index.last_error =
                Some(String::from("显示库活动代次发生变化，必须重新构建向量索引"));
        }
        settings.curation.last_successful_sync_snapshot = Some(curation_snapshot);
        settings.validate()?;
        let settings_json = serde_json::to_string(&settings)
            .map_err(|error| format!("序列化显示库成功状态失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_libraries SET settings_json = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    text(&settings_json),
                    int(unix_epoch_millis()?),
                    text(library_id),
                ],
            ).await
            .map_err(|error| format!("保存显示库成功状态失败：{error}"))?;
        self.get_library(library_id).await
    }

    /// Persists a display build lifecycle status without changing model configuration.
    /// 持久化显示库构建生命周期状态，但不改变模型配置。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `status` is the explicit derived-build lifecycle state to persist.
    /// `status` 是待持久化的明确派生构建生命周期状态。
    /// `last_error` is the optional diagnostic associated with that state.
    /// `last_error` 是该状态关联的可选诊断。
    ///
    /// # Returns
    /// 返回值
    /// Returns the updated persisted library summary, or a validation/storage error.
    /// 返回更新后持久化知识库摘要，或校验/存储错误。
    pub async fn set_display_index_status(
        &self,
        library_id: &str,
        status: KnowledgeDisplayIndexStatus,
        last_error: Option<String>,
    ) -> Result<KnowledgeLibrarySummary, String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        let current = self.require_active_library(library_id).await?;
        let mut settings = current.settings.clone();
        settings.display_index.status = status;
        settings.display_index.last_error = last_error;
        settings.validate()?;
        let settings_json = serde_json::to_string(&settings)
            .map_err(|error| format!("序列化显示库状态失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_libraries SET settings_json = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    text(&settings_json),
                    int(unix_epoch_millis()?),
                    text(library_id),
                ],
            ).await
            .map_err(|error| format!("保存显示库生命周期状态失败：{error}"))?;
        self.get_library(library_id).await
    }

    /// Searches the active display generation with the host-managed Jieba BM25 index.
    /// 使用宿主管理的 Jieba BM25 索引检索当前活动显示库代次。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `query` is the user query passed to host-managed Jieba tokenization.
    /// `query` 是交由宿主管理的 Jieba 分词器处理的用户查询。
    /// `limit` is the requested result/page bound, clamped by this API.
    /// `limit` 是请求的结果或分页上限，由当前接口限制。
    ///
    /// # Returns
    /// 返回值
    /// Returns the ranked display hits and exact FTS expression, or a query error.
    /// 返回排名后的显示命中及精确 FTS 表达式，或查询错误。
    pub async fn search_display(
        &self,
        library_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<(Vec<KnowledgeSearchHit>, String), String> {
        let query = query.trim();
        if query.is_empty() {
            return Err(String::from("显示库搜索词不能为空。"));
        }
        let library = self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let metadata_rows: Vec<DisplayMetadataRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT MAX(CASE WHEN key = 'active_generation' THEN value END) AS active_generation, MAX(CASE WHEN key = 'active_fts_index' THEN value END) AS active_fts_index FROM display_metadata;",
                Vec::new(),
            )
            .await?;
        let metadata = metadata_rows
            .into_iter()
            .next()
            .ok_or_else(|| String::from("显示库元数据缺失。"))?;
        let generation = metadata
            .active_generation
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| String::from("显示库尚未发布活动代次。"))?;
        let fts_index = metadata
            .active_fts_index
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| String::from("显示库活动 FTS 索引缺失。"))?;
        validate_generation_id(&generation)?;
        validate_display_fts_index(&fts_index)?;
        let tokenized_query = local_sql::tokenize_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            TokenizerMode::Jieba,
            query,
            true,
        )
        .await
        .map_err(|error| format!("构建显示库 Jieba FTS 查询失败：{error}"))?;
        let limit = limit.clamp(1, MAX_SEARCH_LIMIT);
        let sql = format!(
            r#"SELECT f.id, d.source_document_id, d.title, d.relative_path,
                      c.heading_path, c.start_line, c.end_line, c.content,
                      bm25({fts_index}, 2.0, 1.0) AS raw_score
               FROM {fts_index} AS f
               JOIN display_chunks AS c ON c.id = f.id AND c.generation = ?
               JOIN display_documents AS d ON d.id = c.display_document_id
                    AND d.generation = c.generation
               WHERE {fts_index} MATCH ?
               ORDER BY raw_score ASC, d.relative_path ASC, c.id ASC
               LIMIT ?;"#,
        );
        let ranked_rows: Vec<DisplaySearchRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                &sql,
                vec![
                    text(&generation),
                    text(&tokenized_query.fts_query),
                    int(i64::from(limit)),
                ],
            )
            .await
            .map_err(|error| format!("执行显示库 Jieba BM25 检索失败：{error}"))?;
        let hits = ranked_rows
            .into_iter()
            .map(|row| {
                Ok(KnowledgeSearchHit {
                    library_id: library.id.clone(),
                    library_name: library.name.clone(),
                    document_id: row.source_document_id,
                    chunk_id: row.id,
                    title: row.title,
                    relative_path: row.relative_path,
                    heading_path: row.heading_path,
                    start_line: nonnegative_u64(row.start_line, "display_start_line")?,
                    end_line: nonnegative_u64(row.end_line, "display_end_line")?,
                    content: row.content,
                    match_kind: String::from("display_bm25"),
                    bm25_score: -row.raw_score,
                    rrf_score: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok((hits, tokenized_query.fts_query))
    }

    /// Fuses active display BM25 ranks with precomputed vector ranks using reciprocal rank fusion.
    /// 使用倒数排名融合将活动显示库 BM25 排名与预计算向量排名合并。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `query` is the user query passed to host-managed Jieba tokenization.
    /// `query` 是交由宿主管理的 Jieba 分词器处理的用户查询。
    /// `query_vector` is the embedding computed for the query in the indexed model space.
    /// `query_vector` 是按已索引模型空间计算的查询向量。
    /// `limit` is the requested result/page bound, clamped by this API.
    /// `limit` 是请求的结果或分页上限，由当前接口限制。
    ///
    /// # Returns
    /// 返回值
    /// Returns the fused display hits and query expression, or a compatibility/query error.
    /// 返回融合显示命中及查询表达式，或兼容性或查询错误。
    pub async fn search_display_hybrid_with_vector(
        &self,
        library_id: &str,
        query: &str,
        query_vector: Vec<f32>,
        limit: u32,
    ) -> Result<(Vec<KnowledgeSearchHit>, String), String> {
        let library = self.require_active_library(library_id).await?;
        if library.settings.vector_index.status != KnowledgeVectorIndexStatus::Ready {
            return Err(String::from("资料库向量索引尚未就绪"));
        }
        let generation = library
            .settings
            .vector_index
            .active_generation
            .clone()
            .ok_or_else(|| String::from("资料库向量活动代次缺失"))?;
        let display_generation = library
            .settings
            .display_index
            .active_generation
            .clone()
            .ok_or_else(|| String::from("显示库活动代次缺失"))?;
        if generation != display_generation {
            return Err(String::from("资料库向量索引与显示库代次不一致"));
        }
        let table_name = vector_table_name(&generation)?;
        let limit = limit.clamp(1, MAX_SEARCH_LIMIT);
        let vector_rows = self
            .search_vector_rows(
                library_id,
                &table_name,
                &generation,
                query_vector,
                limit.saturating_mul(2),
            )
            .await?;
        let (mut hits, expression) = self
            .search_display(library_id, query, limit.saturating_mul(2))
            .await?;
        let bm25_ranks = hits
            .iter()
            .enumerate()
            .map(|(rank, hit)| (hit.chunk_id.clone(), rank))
            .collect::<BTreeMap<_, _>>();
        let mut vector_scores = BTreeMap::<String, f64>::new();
        for (rank, row) in vector_rows.iter().enumerate() {
            let id = row
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| String::from("LanceDB 向量结果缺少 id 字段"))?;
            vector_scores.insert(id.to_string(), 1.0 / (RRF_K + rank as f64 + 1.0));
        }
        let vector_only_ids = vector_scores
            .keys()
            .filter(|id| !hits.iter().any(|hit| &hit.chunk_id == *id))
            .cloned()
            .collect::<Vec<_>>();
        if !vector_only_ids.is_empty() {
            let (space_id, binding_id) = self.ensure_display_binding(library_id).await?;
            let placeholders = (0..vector_only_ids.len())
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT c.id, d.source_document_id, d.title, d.relative_path, c.heading_path, c.start_line, c.end_line, c.content, 0.0 AS raw_score FROM display_chunks AS c JOIN display_documents AS d ON d.id = c.display_document_id AND d.generation = c.generation WHERE c.generation = ? AND c.id IN ({placeholders});"
            );
            let mut params = vec![text(&display_generation)];
            params.extend(vector_only_ids.iter().cloned().map(text));
            let rows: Vec<DisplaySearchRow> = self
                .query_library_rows(&space_id, &binding_id, &sql, params)
                .await?;
            for row in rows {
                hits.push(KnowledgeSearchHit {
                    library_id: library.id.clone(),
                    library_name: library.name.clone(),
                    document_id: row.source_document_id,
                    chunk_id: row.id,
                    title: row.title,
                    relative_path: row.relative_path,
                    heading_path: row.heading_path,
                    start_line: nonnegative_u64(row.start_line, "display_start_line")?,
                    end_line: nonnegative_u64(row.end_line, "display_end_line")?,
                    content: row.content,
                    match_kind: String::from("display_vector"),
                    bm25_score: 0.0,
                    rrf_score: None,
                });
            }
        }
        for hit in hits.iter_mut() {
            let bm25_rank_score = bm25_ranks
                .get(&hit.chunk_id)
                .map(|rank| 1.0 / (RRF_K + *rank as f64 + 1.0))
                .unwrap_or(0.0);
            let vector_rank_score = vector_scores.get(&hit.chunk_id).copied().unwrap_or(0.0);
            hit.rrf_score = Some(bm25_rank_score + vector_rank_score);
        }
        hits.sort_by(|left, right| {
            right
                .rrf_score
                .partial_cmp(&left.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.library_id.cmp(&right.library_id))
                .then_with(|| left.chunk_id.cmp(&right.chunk_id))
        });
        hits.truncate(limit as usize);
        Ok((hits, format!("{expression};vector")))
    }

    /// Reads one active display document by its stable source-document identifier.
    /// 按稳定的原始文档标识读取当前活动显示代次中的一份显示文档。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `source_document_id` is the immutable original-document identity.
    /// `source_document_id` 是不可变的原始文档身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the active-generation document for the exact source id, or an error.
    /// 返回精确来源身份对应的活动代次文档，或错误。
    pub async fn read_display_document(
        &self,
        library_id: &str,
        source_document_id: &str,
    ) -> Result<KnowledgeDisplayDocument, String> {
        let (space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let metadata_rows: Vec<DisplayMetadataRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT MAX(CASE WHEN key = 'active_generation' THEN value END) AS active_generation, MAX(CASE WHEN key = 'active_fts_index' THEN value END) AS active_fts_index FROM display_metadata;",
                Vec::new(),
            )
            .await?;
        let generation = metadata_rows
            .into_iter()
            .next()
            .and_then(|row| row.active_generation)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| String::from("显示库尚未发布活动代次。"))?;
        let rows: Vec<DisplayDocumentRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT id, source_document_id, source_sha256, relative_path, folder_path, title, summary, keywords_json, symbols_json, embedding_text, markdown, generation, generated_at_ms FROM display_documents WHERE source_document_id = ? AND generation = ? LIMIT 1;",
                vec![text(source_document_id), text(&generation)],
            )
            .await?;
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| format!("显示库文档不存在：{source_document_id}"))?;
        row.into_display_document()
    }

    /// Reads one active display document as the common Markdown viewer contract.
    /// 以通用 Markdown 阅读器契约读取当前活动显示代次中的一份显示文档。
    ///
    /// `library_id` identifies the owning library and `source_document_id` is the immutable raw
    /// document identity shared by the source tree, search results, and derived display corpus.
    /// `library_id` 标识所属知识库，`source_document_id` 是原始目录树、搜索结果与派生显示库共享的
    /// 不可变文档身份。
    ///
    /// Returns display-layer metadata and Markdown while preserving the authoritative source id.
    /// 返回显示层元数据与 Markdown，同时保留权威来源目录标识。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `source_document_id` is the immutable original-document identity.
    /// `source_document_id` 是不可变的原始文档身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the display Markdown viewer contract with authoritative source identity, or an error.
    /// 返回保留权威来源身份的显示 Markdown 阅读器契约，或错误。
    pub async fn read_display_document_content(
        &self,
        library_id: &str,
        source_document_id: &str,
    ) -> Result<KnowledgeDocumentContent, String> {
        // sourceDocument supplies metadata intentionally not duplicated in the derived display
        // database without loading the raw Markdown body.
        // sourceDocument 在不加载原始 Markdown 正文的情况下，提供派生显示库中刻意不重复存储的元数据。
        let source_document = self
            .read_document_summary(library_id, source_document_id)
            .await?;
        // displayDocument is the exact active-generation derivative for the same source document.
        // displayDocument 是同一原始文档在当前活动代次中的精确派生文档。
        let display_document = self
            .read_display_document(library_id, source_document_id)
            .await?;
        let (space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        // countRows obtains the exact active-generation display chunk count for viewer metadata.
        // countRows 获取阅读器元数据所需的当前活动代次显示切片精确数量。
        let count_rows: Vec<CountRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT 1 AS document_count, COUNT(*) AS chunk_count FROM display_chunks WHERE display_document_id = ? AND generation = ?;",
                vec![text(&display_document.id), text(&display_document.generation)],
            )
            .await?;
        // chunkCount is always produced by the aggregate query, including an exact zero.
        // chunkCount 始终由聚合查询生成，包括精确的零值。
        let chunk_count = count_rows
            .into_iter()
            .next()
            .ok_or_else(|| String::from("读取显示库文档切片数量失败。"))?
            .chunk_count;
        // markdown owns the normalized display-layer body returned to the viewer.
        // markdown 持有返回阅读器的规范化显示层正文。
        let markdown = display_document.markdown;
        // byteSize and lineCount describe the displayed Markdown rather than the raw source file.
        // byteSize 与 lineCount 描述显示 Markdown，而不是原始来源文件。
        let byte_size = u64::try_from(markdown.len())
            .map_err(|_| String::from("显示库 Markdown 字节数超出支持范围。"))?;
        let line_count = u64::try_from(markdown.lines().count().max(1))
            .map_err(|_| String::from("显示库 Markdown 行数超出支持范围。"))?;
        Ok(KnowledgeDocumentContent {
            document: KnowledgeDocumentSummary {
                id: display_document.source_document_id,
                source_id: source_document.source_id,
                relative_path: display_document.relative_path,
                folder_path: display_document.folder_path,
                title: display_document.title,
                byte_size,
                line_count,
                chunk_count: nonnegative_u64(chunk_count, "display_chunk_count")?,
                updated_at_ms: display_document.generated_at_ms,
            },
            content: markdown,
        })
    }

    /// Lists active display chunks in deterministic pages for vector generation.
    /// 按确定性分页读取活动显示库切片，供向量生成使用。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `limit` is the requested result/page bound, clamped by this API.
    /// `limit` 是请求的结果或分页上限，由当前接口限制。
    /// `offset` is the zero-based offset in deterministic chunk order.
    /// `offset` 是确定性切片顺序中的零基偏移。
    ///
    /// # Returns
    /// 返回值
    /// Returns the deterministic active-generation chunk page, or an error.
    /// 返回确定性活动代次切片分页，或错误。
    pub async fn list_display_chunks(
        &self,
        library_id: &str,
        limit: u32,
        offset: u64,
    ) -> Result<Vec<KnowledgeDisplayChunk>, String> {
        let (space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let metadata_rows: Vec<DisplayMetadataRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT MAX(CASE WHEN key = 'active_generation' THEN value END) AS active_generation, MAX(CASE WHEN key = 'active_fts_index' THEN value END) AS active_fts_index FROM display_metadata;",
                Vec::new(),
            )
            .await?;
        let generation = metadata_rows
            .into_iter()
            .next()
            .and_then(|row| row.active_generation)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| String::from("显示库尚未发布活动代次。"))?;
        validate_generation_id(&generation)?;
        let limit = limit.clamp(1, 500);
        let offset =
            i64::try_from(offset).map_err(|_| String::from("显示库切片偏移量超出支持范围"))?;
        let rows: Vec<DisplayChunkRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT id, display_document_id, ordinal, heading_path, start_line, end_line, content, embedding_text, generation FROM display_chunks WHERE generation = ? ORDER BY display_document_id ASC, ordinal ASC, id ASC LIMIT ? OFFSET ?;",
                vec![text(&generation), int(i64::from(limit)), int(offset)],
            )
            .await?;
        rows.into_iter()
            .map(DisplayChunkRow::into_display_chunk)
            .collect()
    }

    /// Counts active display chunks before vector generation starts.
    /// 在向量生成开始前统计活动显示库切片总数。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact active-generation chunk count, including zero, or an error.
    /// 返回包含零值的活动代次精确切片数，或错误。
    pub async fn count_display_chunks(&self, library_id: &str) -> Result<u64, String> {
        let (space_id, binding_id) = self.ensure_display_binding(library_id).await?;
        let metadata_rows: Vec<DisplayMetadataRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT MAX(CASE WHEN key = 'active_generation' THEN value END) AS active_generation, MAX(CASE WHEN key = 'active_fts_index' THEN value END) AS active_fts_index FROM display_metadata;",
                Vec::new(),
            )
            .await?;
        let generation = metadata_rows
            .into_iter()
            .next()
            .and_then(|row| row.active_generation)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| String::from("显示库尚未发布活动代次。"))?;
        validate_generation_id(&generation)?;
        let rows: Vec<DisplayCountRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT COUNT(*) AS count FROM display_chunks WHERE generation = ?;",
                vec![text(&generation)],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(|row| nonnegative_u64(row.count, "display_chunk_count"))
            .transpose()?
            .ok_or_else(|| String::from("显示库切片计数查询未返回结果。"))
    }
}

/// Returns the generation-scoped display FTS index name after validating its UUID.
/// 校验代次 UUID 后返回代次作用域的显示库 FTS 索引名称。
///
/// # Parameters
/// 参数
/// `generation` is the canonical UUID of the exact derived generation.
/// `generation` 是精确派生代次的规范 UUID。
///
/// # Returns
/// 返回值
/// Returns the generation-scoped display FTS name, or an invalid UUID error.
/// 返回代次级显示 FTS 名称，或非法 UUID 错误。
fn display_fts_index_name(generation: &str) -> Result<String, String> {
    validate_generation_id(generation)?;
    Ok(format!(
        "{}_{}",
        KNOWLEDGE_DISPLAY_FTS_INDEX_PREFIX,
        generation.replace('-', "")
    ))
}

/// Validates an activity pointer loaded from display metadata before SQL interpolation.
/// 在将活动指针插入 SQL 前校验从显示元数据读取的索引名称。
///
/// # Parameters
/// 参数
/// `index_name` is the stored display FTS index identity to validate before SQL interpolation.
/// `index_name` 是插入 SQL 前待校验的已存显示 FTS 索引身份。
///
/// # Returns
/// 返回值
/// Returns success after the documented operation completes, or its explicit failure.
/// 返回所述操作完成后的成功结果，或其明确错误。
fn validate_display_fts_index(index_name: &str) -> Result<(), String> {
    let prefix = format!("{}{}", KNOWLEDGE_DISPLAY_FTS_INDEX_PREFIX, "_");
    let suffix = index_name
        .strip_prefix(&prefix)
        .ok_or_else(|| String::from("显示库活动 FTS 索引名称前缀无效。"))?;
    if suffix.len() != 32 || !suffix.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(String::from("显示库活动 FTS 索引名称格式无效。"));
    }
    Ok(())
}

/// Maps an abandoned display build to the durable state exposed after restart.
/// 将遗留显示库构建映射为重启后公开的持久化状态。
///
/// # Parameters
/// 参数
/// `status` is the explicit derived-build lifecycle state to persist.
/// `status` 是待持久化的明确派生构建生命周期状态。
/// `has_active_generation` is whether a previously published generation remains available.
/// `has_active_generation` 是此前发布的代次是否仍存在。
///
/// # Returns
/// 返回值
/// Returns a reconciled status only for an abandoned building state.
/// 返回仅针对遗留构建中状态的协调后状态。
pub(super) fn recovered_display_index_status(
    status: KnowledgeDisplayIndexStatus,
    has_active_generation: bool,
) -> Option<KnowledgeDisplayIndexStatus> {
    if status != KnowledgeDisplayIndexStatus::Building {
        return None;
    }
    Some(if has_active_generation {
        KnowledgeDisplayIndexStatus::Stale
    } else {
        KnowledgeDisplayIndexStatus::Failed
    })
}
