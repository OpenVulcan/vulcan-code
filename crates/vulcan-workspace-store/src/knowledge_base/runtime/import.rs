//! Markdown discovery, incremental source identity, and authoritative import staging.
//! Markdown 发现、增量来源身份与权威导入暂存。

use super::super::markdown::{
    chunk_markdown, extract_document_title, normalize_markdown_newlines, sha256_hex,
};
use super::super::models::{
    KnowledgeImportJobSummary, KnowledgeImportReport, KnowledgeOperationKind,
};
use super::fts::ImportIndexSyncOutcome;
use super::{
    KNOWLEDGE_OPERATION_CANCELLED_ERROR, KnowledgeBaseRuntime, MAX_WARNING_CHARS,
    PreparedKnowledgeOperation, int, metadata_is_link_or_reparse, nonnegative_u64,
    queued_operation_summary, require_operation_not_cancelled, text, truncate_chars,
};
use crate::knowledge_base::KnowledgeFileConfirmation;
use crate::local_sql;
use crate::runtime_assets;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use uuid::Uuid;
use vulcan_runtime_assets::text_decode::decode_complete_user_text;
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// Maximum number of Markdown files accepted by one import.
/// 单次导入允许的最大 Markdown 文件数量。
const MAX_IMPORT_FILES: usize = 20_000;

/// Maximum size of one Markdown source file.
/// 单个 Markdown 来源文件的最大字节数。
const MAX_MARKDOWN_FILE_BYTES: u64 = 20 * 1024 * 1024;

/// Maximum cumulative source bytes accepted by one import.
/// 单次导入允许的最大来源总字节数。
const MAX_IMPORT_BYTES: u64 = 1024 * 1024 * 1024;

/// Maximum number of warnings retained in one import report.
/// 单次导入报告保留的最大告警数量。
const MAX_IMPORT_WARNINGS: usize = 100;

/// One source row used during incremental import.
/// 增量导入期间使用的一条来源记录。
#[derive(Debug, Deserialize)]
struct SourceRow {
    /// Immutable source identifier.
    /// 不可变来源标识符。
    id: String,
}

/// Existing document identity and digest used for incremental decisions.
/// 用于增量判断的现有文档身份与摘要。
#[derive(Debug, Clone, Deserialize)]
struct ExistingDocumentRow {
    /// Immutable document identifier.
    /// 不可变文档标识符。
    id: String,
    /// Normalized relative path key.
    /// 规范化相对路径键。
    relative_path_key: String,
    /// SHA-256 source digest.
    /// SHA-256 来源摘要。
    source_sha256: String,
}

/// Existing chunk identity used to delete stale FTS rows.
/// 用于删除过期 FTS 记录的现有切片身份。
#[derive(Debug, Clone, Deserialize)]
struct ExistingChunkRow {
    /// Immutable chunk identifier.
    /// 不可变切片标识符。
    id: String,
    /// Owning document identifier.
    /// 所属文档标识符。
    document_id: String,
}

/// One scanned Markdown candidate before staging.
/// 暂存前的一份已扫描 Markdown 候选文件。
#[derive(Debug)]
struct ScannedMarkdownFile {
    /// Slash-normalized relative path.
    /// 以斜杠规范化的相对路径。
    relative_path: String,
    /// Case-normalized relative path key.
    /// 大小写规范化的相对路径键。
    relative_path_key: String,
    /// Folder classification derived from the relative path.
    /// 从相对路径派生的文件夹分类。
    folder_path: String,
    /// Source path reached only through the validated, non-reparse directory traversal.
    /// 仅通过已验证且无重解析点的目录遍历到达的来源路径。
    source_path: PathBuf,
    /// Source byte size.
    /// 来源字节大小。
    byte_size: u64,
    /// Optional source modification time.
    /// 可选来源修改时间。
    modified_at_ms: Option<i64>,
}

/// ScanProgressSnapshot carries bounded filesystem discovery progress back to the async runtime.
/// ScanProgressSnapshot 将有界文件系统发现进度传回异步运行时。
#[derive(Debug)]
struct ScanProgressSnapshot {
    /// Number of Markdown files discovered so far.
    /// 当前已发现的 Markdown 文件数。
    discovered_files: u64,
    /// Current relative directory or file path visited by the scanner.
    /// 扫描器当前访问的相对目录或文件路径。
    current_path: String,
}

impl KnowledgeBaseRuntime {
    /// Recursively imports one canonical directory, accepting only Markdown files.
    /// 递归导入一个规范目录，并且只接受 Markdown 文件。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library being imported.
    /// `library_id` 是本次导入的精确活动知识库。
    /// `source_root` is the native source directory to canonicalize before reserving work.
    /// `source_root` 是预留工作前要规范化的原生来源目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the complete import report, or its explicit operation/persistence error.
    /// 返回完整导入报告，或明确操作/持久化错误。
    pub async fn import_markdown_directory(
        &self,
        library_id: &str,
        source_root: &Path,
    ) -> Result<KnowledgeImportReport, String> {
        self.import_markdown_directory_for_source(library_id, None, source_root)
            .await
    }

    /// Imports or synchronizes one directory, optionally remapping an existing durable source identity.
    /// 导入或同步一个目录，并可选择重新映射一个现有持久化来源身份。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library being imported.
    /// `library_id` 是本次导入的精确活动知识库。
    /// `source_id` is the explicit retained or resolved durable source identity.
    /// `source_id` 是明确保留或已解析的持久来源身份。
    /// `source_root` is the native source directory to canonicalize before reserving work.
    /// `source_root` 是预留工作前要规范化的原生来源目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the complete import report, or its explicit operation/persistence error.
    /// 返回完整导入报告，或明确操作/持久化错误。
    pub async fn import_markdown_directory_for_source(
        &self,
        library_id: &str,
        source_id: Option<&str>,
        source_root: &Path,
    ) -> Result<KnowledgeImportReport, String> {
        // CanonicalRoot is validated before a durable operation row becomes visible.
        // CanonicalRoot 在持久化操作记录可见之前完成校验。
        let canonical_root = canonical_import_root(source_root)?;
        self.require_active_library(library_id).await?;
        // Operation reserves the library before the synchronous execution begins.
        // Operation 在同步执行开始前预留该知识库。
        let operation = self
            .prepare_operation(
                library_id,
                KnowledgeOperationKind::Import,
                Some(&canonical_root),
            )
            .await?;
        self.execute_prepared_import(
            operation,
            canonical_root,
            source_id.map(str::to_string),
            None,
        )
        .await
    }

    /// Starts one detached Markdown import whose lifetime is independent from the browser request.
    /// 启动一个生命周期独立于浏览器请求的分离式 Markdown 导入。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library being imported.
    /// `library_id` 是本次导入的精确活动知识库。
    /// `source_id` is the explicit retained or resolved durable source identity.
    /// `source_id` 是明确保留或已解析的持久来源身份。
    /// `source_root` is the native source directory to canonicalize before reserving work.
    /// `source_root` 是预留工作前要规范化的原生来源目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the durable queued snapshot after detached executor startup, or an error.
    /// 返回分离执行器启动后的持久排队快照，或错误。
    pub async fn start_import_markdown_directory_for_source(
        &self,
        library_id: &str,
        source_id: Option<&str>,
        source_root: &Path,
    ) -> Result<KnowledgeImportJobSummary, String> {
        self.start_import_with_confirmation(library_id, source_id, source_root, None)
            .await
    }

    /// Starts only the eligible files explicitly confirmed in `confirmation`; returns a durable job.
    /// 仅启动 `confirmation` 明确确认的可用文件，返回持久化任务。
    pub async fn start_confirmed_knowledge_import(
        &self,
        library_id: &str,
        source_id: Option<&str>,
        source_root: &Path,
        confirmation: KnowledgeFileConfirmation,
    ) -> Result<KnowledgeImportJobSummary, String> {
        self.start_import_with_confirmation(library_id, source_id, source_root, Some(confirmation))
            .await
    }

    /// Reserves an import after optional preflight validation; legacy internal callers retain full-sync semantics.
    /// 可选执行前验证后预留导入，旧内部调用方保留完整同步语义。
    async fn start_import_with_confirmation(
        &self,
        library_id: &str,
        source_id: Option<&str>,
        source_root: &Path,
        confirmation: Option<KnowledgeFileConfirmation>,
    ) -> Result<KnowledgeImportJobSummary, String> {
        // CanonicalRoot resolves an existing directory before reserving work; traversal filters reparse entries.
        // CanonicalRoot 在预留工作前解析现有目录；遍历时过滤重解析项。
        let canonical_root = canonical_import_root(source_root)?;
        self.require_active_library(library_id).await?;
        if let Some(confirmation) = &confirmation {
            self.preview_knowledge_import(library_id, &canonical_root, source_id)
                .await?
                .confirmed_entries(confirmation)?;
        }
        // Operation is persisted and registered before the HTTP caller receives success.
        // Operation 在 HTTP 调用方收到成功前完成持久化与注册。
        let operation = self
            .prepare_operation(
                library_id,
                KnowledgeOperationKind::Import,
                Some(&canonical_root),
            )
            .await?;
        // Snapshot is immutable queued state and does not race with the detached executor's first update.
        // Snapshot 是不可变排队状态，不会与分离执行器的首次更新产生竞态。
        let snapshot = queued_operation_summary(&operation);
        // Runtime and owned inputs keep every dependency alive after the HTTP request future is dropped.
        // Runtime 与自有输入在 HTTP 请求 future 被丢弃后继续保持全部依赖存活。
        let runtime = self.clone();
        // SourceId is owned because the detached executor cannot borrow request-local memory.
        // SourceId 使用自有值，因为分离执行器不能借用请求局部内存。
        let source_id = source_id.map(str::to_string);
        tokio::spawn(async move {
            if let Err(error) = runtime
                .execute_prepared_import(operation, canonical_root, source_id, confirmation)
                .await
            {
                eprintln!("[knowledge-base] detached import failed: {error}");
            }
        });
        Ok(snapshot)
    }

    /// Executes one reserved import and routes every result through terminal-state persistence.
    /// 执行一个预留导入，并将每种结果交给终态持久化处理。
    ///
    /// # Parameters
    /// 参数
    /// `source_id` is the explicit retained or resolved durable source identity.
    /// `source_id` 是明确保留或已解析的持久来源身份。
    /// `operation` is the exact reserved import identity, timestamp and cancellation signal.
    /// `operation` 是精确预留的导入身份、时间戳与取消信号。
    /// `canonical_root` is the canonical source root associated with this operation.
    /// `canonical_root` 是本次操作关联的规范来源根。
    ///
    /// # Returns
    /// 返回值
    /// Returns the complete import report, or its explicit operation/persistence error.
    /// 返回完整导入报告，或明确操作/持久化错误。
    async fn execute_prepared_import(
        &self,
        operation: PreparedKnowledgeOperation,
        canonical_root: PathBuf,
        source_id: Option<String>,
        // Confirmed scope is revalidated after acquiring the library mutation lock.
        // 获得知识库变更锁后重新验证已确认范围。
        confirmation: Option<KnowledgeFileConfirmation>,
    ) -> Result<KnowledgeImportReport, String> {
        // Result includes lock acquisition and binding activation so every failure reaches finalization.
        // Result 包含锁获取与绑定激活，使每个失败都进入终态处理。
        let result = async {
            // Guard serializes all authoritative and derived mutations for this library.
            // Guard 串行化该知识库的全部权威与派生变更。
            let _guard = self.library_mutation_guard(&operation.library_id).await?;
            require_operation_not_cancelled(&operation.cancellation)?;
            self.require_active_library(&operation.library_id).await?;
            // BindingIdentity is the only host route for the isolated library database.
            // BindingIdentity 是隔离知识库数据库的唯一宿主路由。
            let (space_id, binding_id) = self.ensure_library_binding(&operation.library_id).await?;
            self.update_operation_phase(&operation.job_id, "running", "scanning")
                .await?;
            self.import_markdown_directory_inner(
                &operation.library_id,
                &space_id,
                &binding_id,
                &operation.job_id,
                &canonical_root,
                source_id.as_deref(),
                &operation.cancellation,
                operation.created_at_ms,
                confirmation.as_ref(),
            )
            .await
        }
        .await;

        // FinishResult is persisted before the in-memory reservation is released.
        // FinishResult 在释放内存预留之前完成持久化。
        let finish_result = match &result {
            Ok(report) => self.finish_import_job(report, None).await,
            Err(error) if error == KNOWLEDGE_OPERATION_CANCELLED_ERROR => {
                self.finish_cancelled_operation_job(&operation.job_id).await
            }
            Err(error) => {
                self.finish_failed_operation_job(&operation.job_id, error)
                    .await
            }
        };
        self.release_active_operation(&operation.library_id, &operation.job_id)
            .await;
        finish_result?;
        result
    }

    /// Runs the authoritative import state machine while the caller holds the mutation lock.
    /// 在调用方持有变更锁时运行权威导入状态机。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact active library being imported.
    /// `library_id` 是本次导入的精确活动知识库。
    /// `canonical_root` is the canonical source root associated with this operation.
    /// `canonical_root` 是本次操作关联的规范来源根。
    /// `space_id` is the resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact authoritative SQLite binding.
    /// `binding_id` 是精确的权威 SQLite 绑定。
    /// `job_id` is the durable import job identity used for progress and staging.
    /// `job_id` 是用于进度和暂存的持久导入任务身份。
    /// `requested_source_id` is the optional existing identity explicitly selected for root remapping.
    /// `requested_source_id` 是为根目录重映射明确选择的可选现有身份。
    /// `cancellation` is the shared cooperative cancellation signal.
    /// `cancellation` 是共享协作取消信号。
    /// `now` is the fixed timestamp supplied by the prepared operation.
    /// `now` 是预备操作传入的固定时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns the complete import report, or its explicit operation/persistence error.
    /// 返回完整导入报告，或明确操作/持久化错误。
    #[allow(clippy::too_many_arguments)]
    async fn import_markdown_directory_inner(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        job_id: &str,
        canonical_root: &Path,
        requested_source_id: Option<&str>,
        cancellation: &Arc<AtomicBool>,
        now: i64,
        // None retains legacy internal full-directory synchronization; user-facing imports always confirm.
        // None 保留旧内部完整目录同步，面向用户的导入始终需要确认。
        confirmation: Option<&KnowledgeFileConfirmation>,
    ) -> Result<KnowledgeImportReport, String> {
        let source_root_text = runtime_assets::native_host_path_text_from_path(canonical_root);
        let root_key = path_key(canonical_root);
        let display_name = canonical_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&source_root_text)
            .to_string();
        let source_id = self
            .resolve_source_id(space_id, binding_id, &root_key, requested_source_id)
            .await?;
        // Validate the exact snapshot while serialized with all library mutations.
        // 与全部知识库变更串行时验证精确快照。
        let preview = if confirmation.is_some() {
            Some(
                self.preview_knowledge_import(library_id, canonical_root, requested_source_id)
                    .await?,
            )
        } else {
            None
        };
        // Expected hashes stop files changed after confirmation from entering the staging transaction.
        // 预期哈希阻止确认后变化的文件进入暂存事务。
        let mut confirmed_hashes = BTreeMap::new();
        let candidates = if let (Some(preview), Some(confirmation)) = (&preview, confirmation) {
            preview
                .confirmed_entries(confirmation)?
                .into_iter()
                .map(|entry| {
                    // Selectable rows always carry a successfully inspected byte digest.
                    // 可选记录始终携带成功检查的字节摘要。
                    let hash = entry
                        .content_sha256
                        .clone()
                        .ok_or_else(|| String::from("已确认文件缺少内容摘要。"))?;
                    confirmed_hashes.insert(entry.relative_path.to_lowercase(), hash);
                    Ok(ScannedMarkdownFile {
                        relative_path: entry.relative_path.clone(),
                        relative_path_key: entry.relative_path.to_lowercase(),
                        folder_path: Path::new(&entry.relative_path)
                            .parent()
                            .map(slash_path)
                            .transpose()?
                            .unwrap_or_default(),
                        source_path: canonical_root.join(&entry.relative_path),
                        byte_size: entry.byte_size,
                        modified_at_ms: None,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        } else {
            self.scan_markdown_files_with_progress(job_id, canonical_root, cancellation)
                .await?
        };
        self.prepare_import_processing(job_id, candidates.len() as u64)
            .await?;

        let existing_documents: Vec<ExistingDocumentRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT id, relative_path_key, source_sha256 FROM knowledge_documents WHERE source_id = ?;",
                vec![text(&source_id)],
            )
            .await?;
        let existing_by_path: BTreeMap<String, ExistingDocumentRow> = existing_documents
            .iter()
            .cloned()
            .map(|row| (row.relative_path_key.clone(), row))
            .collect();
        let existing_chunks: Vec<ExistingChunkRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT c.id, c.document_id FROM knowledge_chunks c INNER JOIN knowledge_documents d ON d.id = c.document_id WHERE d.source_id = ?;",
                vec![text(&source_id)],
            )
            .await?;

        self.clear_staging(space_id, binding_id).await?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
                "INSERT INTO knowledge_import_staging_context(singleton, job_id, source_id, committed_at_ms) VALUES (1, ?, ?, ?) ON CONFLICT(singleton) DO UPDATE SET job_id = excluded.job_id, source_id = excluded.source_id, committed_at_ms = excluded.committed_at_ms;",
                vec![text(job_id), text(&source_id), int(now)],
            ).await
            .map_err(|error| format!("初始化知识库导入暂存上下文失败：{error}"))?;

        let mut warnings = Vec::new();
        let mut staged_paths = Vec::with_capacity(candidates.len());
        // Selection is not deletion: preserve every existing source path, including currently missing files.
        // 选择不等于删除，保留全部已有来源路径，包括当前缺失文件。
        if confirmation.is_some() {
            staged_paths.extend(
                existing_documents
                    .iter()
                    .filter(|document| !confirmed_hashes.contains_key(&document.relative_path_key))
                    .map(|document| vec![text(&document.relative_path_key)]),
            );
        }
        let mut staged_documents = Vec::new();
        let mut staged_chunks = Vec::new();
        let mut changed_document_ids = BTreeSet::new();
        let mut unchanged_files = 0u64;
        let mut skipped_files = 0u64;
        let mut processed_bytes = 0u64;
        // LastProcessingProgressAt limits database writes while keeping the visible file path fresh.
        // LastProcessingProgressAt 限制数据库写入频率，同时保持可见文件路径及时更新。
        let mut last_processing_progress_at = Instant::now()
            .checked_sub(Duration::from_millis(250))
            .unwrap_or_else(Instant::now);

        for (candidate_index, candidate) in candidates.iter().enumerate() {
            require_operation_not_cancelled(cancellation)?;
            if last_processing_progress_at.elapsed() >= Duration::from_millis(100) {
                self.update_import_progress(
                    job_id,
                    candidate_index as u64,
                    processed_bytes,
                    Some(&candidate.relative_path),
                    &warnings,
                )
                .await?;
                last_processing_progress_at = Instant::now();
            }
            staged_paths.push(vec![text(&candidate.relative_path_key)]);
            let bytes = match if confirmation.is_some() {
                super::file_selection::read_preview_bytes(&candidate.source_path)
            } else {
                fs::read(&candidate.source_path)
            } {
                Ok(bytes) => bytes,
                Err(error) => {
                    if confirmation.is_some() {
                        return Err(format!(
                            "已确认文件 {} 无法读取：{error}，请重新扫描。",
                            candidate.relative_path
                        ));
                    }
                    skipped_files += 1;
                    push_warning(
                        &mut warnings,
                        format!("无法读取 {}：{error}", candidate.relative_path),
                    );
                    continue;
                }
            };
            processed_bytes = processed_bytes.saturating_add(bytes.len() as u64);
            let digest = sha256_hex(&bytes);
            if let Some(expected) = confirmed_hashes.get(&candidate.relative_path_key)
                && expected != &digest
            {
                return Err(format!(
                    "{} 在确认后发生变化，请重新扫描。",
                    candidate.relative_path
                ));
            }
            if existing_by_path
                .get(&candidate.relative_path_key)
                .is_some_and(|existing| existing.source_sha256 == digest)
            {
                unchanged_files += 1;
                continue;
            }
            let content = match decode_complete_user_text(&bytes) {
                Ok(decoded) => normalize_markdown_newlines(&decoded.content),
                Err(error) => {
                    skipped_files += 1;
                    push_warning(
                        &mut warnings,
                        format!(
                            "{} 不是有效 UTF-8 或 GB18030：{error}",
                            candidate.relative_path
                        ),
                    );
                    continue;
                }
            };
            let document_id = existing_by_path
                .get(&candidate.relative_path_key)
                .map(|existing| existing.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let title = extract_document_title(
                &content,
                Path::new(&candidate.relative_path)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("document"),
            );
            let line_count = content.lines().count().max(1) as i64;
            staged_documents.push(vec![
                text(&document_id),
                text(&source_id),
                text(&candidate.relative_path),
                text(&candidate.relative_path_key),
                text(&candidate.folder_path),
                text(&title),
                text(&content),
                text(&digest),
                int(candidate.byte_size as i64),
                int(line_count),
                candidate.modified_at_ms.map_or(SqliteValue::Null, int),
                int(now),
                int(now),
            ]);
            for parsed in chunk_markdown(&content) {
                let chunk_id = format!(
                    "{}:{}:{}",
                    document_id,
                    parsed.ordinal,
                    &parsed.content_sha256[..16]
                );
                staged_chunks.push(vec![
                    text(&chunk_id),
                    text(&document_id),
                    int(parsed.ordinal as i64),
                    text(&parsed.heading_path),
                    int(parsed.start_line as i64),
                    int(parsed.end_line as i64),
                    text(&parsed.content),
                    text(&parsed.content_sha256),
                    int(parsed.estimated_tokens as i64),
                    int(now),
                    int(now),
                ]);
            }
            changed_document_ids.insert(document_id);
        }

        require_operation_not_cancelled(cancellation)?;
        self.persist_source(
            space_id,
            binding_id,
            &source_id,
            &source_root_text,
            &root_key,
            &display_name,
            now,
        )
        .await?;
        self.stage_import_rows(
            space_id,
            binding_id,
            staged_paths,
            staged_documents,
            staged_chunks,
        )
        .await?;
        let scanned_path_keys: BTreeSet<String> = candidates
            .iter()
            .map(|candidate| candidate.relative_path_key.clone())
            .collect();
        let deleted_document_ids: BTreeSet<String> = existing_documents
            .iter()
            .filter(|document| {
                confirmation.is_none() && !scanned_path_keys.contains(&document.relative_path_key)
            })
            .map(|document| document.id.clone())
            .collect();
        let mut stale_chunk_ids = Vec::new();
        for chunk in existing_chunks {
            if changed_document_ids.contains(&chunk.document_id)
                || deleted_document_ids.contains(&chunk.document_id)
            {
                stale_chunk_ids.push(chunk.id);
            }
        }
        self.update_import_progress(
            job_id,
            candidates.len() as u64,
            processed_bytes,
            None,
            &warnings,
        )
        .await?;

        let changed_files = changed_document_ids.len() as u64;
        let deleted_files = deleted_document_ids.len() as u64;
        self.update_operation_phase(job_id, "running", "committing")
            .await?;
        if changed_files > 0 || deleted_files > 0 {
            require_operation_not_cancelled(cancellation)?;
            self.commit_staging(space_id, binding_id).await?;
            // AuthoritativeCounts must become visible even if derived indexing is later cancelled.
            // AuthoritativeCounts 即使派生索引随后被取消也必须立即可见。
            let authoritative_counts = self.authoritative_counts(space_id, binding_id).await?;
            self.project_registry_authoritative_counts(
                library_id,
                &authoritative_counts,
                "dirty",
                now,
            )
            .await?;
            self.mark_derived_indexes_stale(library_id).await?;
            if require_operation_not_cancelled(cancellation).is_err() {
                return Err(String::from(KNOWLEDGE_OPERATION_CANCELLED_ERROR));
            }
            self.update_operation_phase(job_id, "running", "indexing")
                .await?;
            let sync_result = self
                .sync_changed_fts(
                    job_id,
                    library_id,
                    space_id,
                    binding_id,
                    &stale_chunk_ids,
                    cancellation,
                )
                .await?;
            if sync_result == ImportIndexSyncOutcome::Cancelled {
                return Err(String::from(KNOWLEDGE_OPERATION_CANCELLED_ERROR));
            }
        }

        self.update_operation_phase(job_id, "running", "verifying")
            .await?;
        if self.verify_index_gate(space_id, binding_id).await.is_err() {
            self.update_operation_phase(job_id, "running", "rebuilding")
                .await?;
            if self
                .rebuild_index_for_import(job_id, library_id, space_id, binding_id, cancellation)
                .await?
                == ImportIndexSyncOutcome::Cancelled
            {
                return Err(String::from(KNOWLEDGE_OPERATION_CANCELLED_ERROR));
            }
        }

        self.update_operation_phase(job_id, "running", "finalizing")
            .await?;
        let counts = self.authoritative_counts(space_id, binding_id).await?;
        self.project_registry_counts(library_id, &counts, "ready", now)
            .await?;
        self.clear_staging(space_id, binding_id).await?;
        Ok(KnowledgeImportReport {
            job_id: job_id.to_string(),
            library_id: library_id.to_string(),
            scanned_files: candidates.len() as u64,
            changed_files,
            unchanged_files,
            deleted_files,
            skipped_files,
            processed_bytes,
            chunk_count: nonnegative_u64(counts.chunk_count, "chunk_count")?,
            warnings,
        })
    }

    /// Resolves the durable identity for one import without changing its recorded path.
    /// 在不修改已记录路径的前提下解析一次导入的持久化身份。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact authoritative SQLite binding.
    /// `binding_id` 是精确的权威 SQLite 绑定。
    /// `requested_source_id` is the optional existing identity explicitly selected for root remapping.
    /// `requested_source_id` 是为根目录重映射明确选择的可选现有身份。
    /// `root_key` is the platform-aware canonical source-root comparison key.
    /// `root_key` 是平台感知的规范来源根比较键。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact validated source identity or normalized relative path, or an error.
    /// 返回精确校验后的来源身份或规范相对路径，或错误。
    async fn resolve_source_id(
        &self,
        space_id: &str,
        binding_id: &str,
        root_key: &str,
        requested_source_id: Option<&str>,
    ) -> Result<String, String> {
        if let Some(requested_source_id) = requested_source_id {
            let requested_source_id = requested_source_id.trim();
            if requested_source_id.is_empty() {
                return Err(String::from("知识库来源标识符不能为空。"));
            }
            let target_rows: Vec<SourceRow> = self
                .query_library_rows(
                    space_id,
                    binding_id,
                    "SELECT id FROM knowledge_sources WHERE id = ?;",
                    vec![text(requested_source_id)],
                )
                .await?;
            if target_rows.is_empty() {
                return Err(format!("知识库来源不存在：{requested_source_id}"));
            }
            let root_rows: Vec<SourceRow> = self
                .query_library_rows(
                    space_id,
                    binding_id,
                    "SELECT id FROM knowledge_sources WHERE root_key = ?;",
                    vec![text(root_key)],
                )
                .await?;
            if let Some(conflicting_source) = root_rows
                .into_iter()
                .find(|source| source.id != requested_source_id)
            {
                return Err(format!(
                    "所选目录已属于另一个知识库来源：{}",
                    conflicting_source.id
                ));
            }
            return Ok(requested_source_id.to_string());
        }
        let rows: Vec<SourceRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT id FROM knowledge_sources WHERE root_key = ?;",
                vec![text(root_key)],
            )
            .await?;
        Ok(rows
            .into_iter()
            .next()
            .map(|row| row.id)
            .unwrap_or_else(|| Uuid::new_v4().to_string()))
    }

    /// Persists a successfully processed source path while retaining its durable identity.
    /// 持久化已成功处理的来源路径，同时保留其持久化身份。
    ///
    /// # Parameters
    /// 参数
    /// `source_id` is the explicit retained or resolved durable source identity.
    /// `source_id` 是明确保留或已解析的持久来源身份。
    /// `canonical_root` is the canonical source root associated with this operation.
    /// `canonical_root` 是本次操作关联的规范来源根。
    /// `space_id` is the resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact authoritative SQLite binding.
    /// `binding_id` 是精确的权威 SQLite 绑定。
    /// `now` is the fixed timestamp supplied by the prepared operation.
    /// `now` 是预备操作传入的固定时间戳。
    /// `root_key` is the platform-aware canonical source-root comparison key.
    /// `root_key` 是平台感知的规范来源根比较键。
    /// `display_name` is the user-facing source-root label.
    /// `display_name` 是用户可见的来源根名称。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after staging/storage completes, or an explicit storage error.
    /// 返回暂存/存储完成后的成功，或明确存储错误。
    #[allow(clippy::too_many_arguments)]
    async fn persist_source(
        &self,
        _space_id: &str,
        binding_id: &str,
        source_id: &str,
        canonical_root: &str,
        root_key: &str,
        display_name: &str,
        now: i64,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            r#"INSERT INTO knowledge_sources(
                       id, canonical_root, root_key, display_name, created_at_ms, last_scanned_at_ms
                   ) VALUES (?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       canonical_root = excluded.canonical_root,
                       root_key = excluded.root_key,
                       display_name = excluded.display_name,
                       last_scanned_at_ms = excluded.last_scanned_at_ms;"#,
            vec![
                text(source_id),
                text(canonical_root),
                text(root_key),
                text(display_name),
                int(now),
                int(now),
            ],
        )
        .await
        .map_err(|error| format!("保存知识库来源目录失败：{error}"))?;
        Ok(())
    }

    /// Clears all per-library staging rows outside an authoritative transaction.
    /// 在权威事务之外清理全部单知识库暂存记录。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact authoritative SQLite binding.
    /// `binding_id` 是精确的权威 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after staging/storage completes, or an explicit storage error.
    /// 返回暂存/存储完成后的成功，或明确存储错误。
    async fn clear_staging(&self, _space_id: &str, binding_id: &str) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
                "DELETE FROM knowledge_staged_chunks; DELETE FROM knowledge_staged_documents; DELETE FROM knowledge_staged_paths; DELETE FROM knowledge_import_staging_context;",
                Vec::new(),
            ).await
            .map_err(|error| format!("清理知识库导入暂存区失败：{error}"))?;
        Ok(())
    }

    /// Writes all prepared path, document, and chunk rows into staging tables.
    /// 将全部已准备路径、文档与切片记录写入暂存表。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact authoritative SQLite binding.
    /// `binding_id` 是精确的权威 SQLite 绑定。
    /// `paths` is the exact typed scanned-path staging rows.
    /// `paths` 是精确类型化扫描路径暂存行。
    /// `documents` is the prepared typed document staging rows.
    /// `documents` 是已预备的类型化文档暂存行。
    /// `chunks` is the prepared typed chunk staging rows.
    /// `chunks` 是已预备的类型化切片暂存行。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after staging/storage completes, or an explicit storage error.
    /// 返回暂存/存储完成后的成功，或明确存储错误。
    async fn stage_import_rows(
        &self,
        space_id: &str,
        binding_id: &str,
        paths: Vec<Vec<SqliteValue>>,
        documents: Vec<Vec<SqliteValue>>,
        chunks: Vec<Vec<SqliteValue>>,
    ) -> Result<(), String> {
        if !paths.is_empty() {
            self.execute_library_batch(
                space_id,
                binding_id,
                "INSERT INTO knowledge_staged_paths(relative_path_key) VALUES (?);",
                paths,
                "写入扫描路径暂存表失败",
            )
            .await?;
        }
        if !documents.is_empty() {
            self.execute_library_batch(
                space_id,
                binding_id,
                r#"INSERT INTO knowledge_staged_documents(
                    id, source_id, relative_path, relative_path_key, folder_path, title, content,
                    source_sha256, byte_size, line_count, source_modified_at_ms, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#,
                documents,
                "写入文档暂存表失败",
            )
            .await?;
        }
        if !chunks.is_empty() {
            self.execute_library_batch(
                space_id,
                binding_id,
                r#"INSERT INTO knowledge_staged_chunks(
                    id, document_id, ordinal, heading_path, start_line, end_line, content,
                    content_sha256, estimated_tokens, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#,
                chunks,
                "写入切片暂存表失败",
            )
            .await?;
        }
        Ok(())
    }

    /// Atomically promotes staged changes and marks the derived index dirty.
    /// 原子提升暂存变更并将派生索引标记为脏。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact authoritative SQLite binding.
    /// `binding_id` 是精确的权威 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after staging/storage completes, or an explicit storage error.
    /// 返回暂存/存储完成后的成功，或明确存储错误。
    async fn commit_staging(&self, _space_id: &str, binding_id: &str) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            r#"
BEGIN IMMEDIATE;
DELETE FROM knowledge_chunks
WHERE document_id IN (SELECT id FROM knowledge_staged_documents);

INSERT INTO knowledge_documents(
    id, source_id, relative_path, relative_path_key, folder_path, title, content,
    source_sha256, byte_size, line_count, source_modified_at_ms, created_at_ms, updated_at_ms
)
SELECT id, source_id, relative_path, relative_path_key, folder_path, title, content,
       source_sha256, byte_size, line_count, source_modified_at_ms, created_at_ms, updated_at_ms
FROM knowledge_staged_documents WHERE 1
ON CONFLICT(id) DO UPDATE SET
    relative_path = excluded.relative_path,
    relative_path_key = excluded.relative_path_key,
    folder_path = excluded.folder_path,
    title = excluded.title,
    content = excluded.content,
    source_sha256 = excluded.source_sha256,
    byte_size = excluded.byte_size,
    line_count = excluded.line_count,
    source_modified_at_ms = excluded.source_modified_at_ms,
    updated_at_ms = excluded.updated_at_ms;

INSERT INTO knowledge_chunks(
    id, document_id, ordinal, heading_path, start_line, end_line, content,
    content_sha256, estimated_tokens, created_at_ms, updated_at_ms
)
SELECT id, document_id, ordinal, heading_path, start_line, end_line, content,
       content_sha256, estimated_tokens, created_at_ms, updated_at_ms
FROM knowledge_staged_chunks;

DELETE FROM knowledge_documents
WHERE source_id = (SELECT source_id FROM knowledge_import_staging_context WHERE singleton = 1)
  AND relative_path_key NOT IN (SELECT relative_path_key FROM knowledge_staged_paths);

UPDATE knowledge_metadata
SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
WHERE key = 'content_revision';
UPDATE knowledge_metadata SET value = 'dirty' WHERE key = 'index_status';
UPDATE knowledge_metadata SET value = '' WHERE key = 'last_index_error';
COMMIT;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("提交知识库权威内容失败：{error}"))?;
        Ok(())
    }

    /// Scans Markdown candidates on a blocking worker while persisting throttled discovery progress.
    /// 在阻塞工作器中扫描 Markdown 候选文件，同时持久化节流后的发现进度。
    ///
    /// # Parameters
    /// 参数
    /// `job_id` is the durable import job identity used for progress and staging.
    /// `job_id` 是用于进度和暂存的持久导入任务身份。
    /// `cancellation` is the shared cooperative cancellation signal.
    /// `cancellation` 是共享协作取消信号。
    /// `root` is the canonical directory or input path described by this helper.
    /// `root` 是当前助手所述的规范目录或输入路径。
    ///
    /// # Returns
    /// 返回值
    /// Returns the sorted validated candidates, or cancellation/filesystem/limit error.
    /// 返回已排序且经校验的候选，或取消/文件系统/上限错误。
    async fn scan_markdown_files_with_progress(
        &self,
        job_id: &str,
        root: &Path,
        cancellation: &Arc<AtomicBool>,
    ) -> Result<Vec<ScannedMarkdownFile>, String> {
        // ScanRoot moves the exact canonical root into the blocking filesystem worker.
        // ScanRoot 将准确的规范根目录移入阻塞文件系统工作器。
        let scan_root = root.to_path_buf();
        // ScanCancellation shares the exact cooperative cancellation flag with the worker.
        // ScanCancellation 与工作器共享准确的协作取消标志。
        let scan_cancellation = Arc::clone(cancellation);
        // ProgressSender and ProgressReceiver relay throttled snapshots through the existing unbounded channel.
        // ProgressSender 与 ProgressReceiver 通过现有无界通道传递节流快照。
        let (progress_sender, mut progress_receiver) = mpsc::unbounded_channel();
        // ScanTask keeps synchronous directory traversal off the async host executor.
        // ScanTask 将同步目录遍历移出异步宿主执行器。
        let scan_task = tokio::task::spawn_blocking(move || {
            scan_markdown_files(&scan_root, &scan_cancellation, Some(&progress_sender))
        });
        tokio::pin!(scan_task);
        // ProgressChannelOpen prevents a closed progress channel from forming a busy select loop.
        // ProgressChannelOpen 防止已关闭的进度通道形成空转 select 循环。
        let mut progress_channel_open = true;

        loop {
            tokio::select! {
                scan_result = &mut scan_task => {
                    return scan_result
                        .map_err(|error| format!("知识库目录扫描工作器失败：{error}"))?;
                }
                progress = progress_receiver.recv(), if progress_channel_open => {
                    match progress {
                        Some(progress) => {
                            self.update_import_scan_progress(
                                job_id,
                                progress.discovered_files,
                                &progress.current_path,
                            )
                            .await?;
                        }
                        None => progress_channel_open = false,
                    }
                }
            }
        }
    }
}

/// Recursively scans a canonical root without following symlinks or reparse entries.
/// 在不跟随符号链接或重解析项的前提下递归扫描规范根目录。
///
/// # Parameters
/// 参数
/// `cancellation` is the shared cooperative cancellation signal.
/// `cancellation` 是共享协作取消信号。
/// `root` is the canonical directory or input path described by this helper.
/// `root` 是当前助手所述的规范目录或输入路径。
/// `progress_sender` is the optional best-effort channel for throttled discovery snapshots.
/// `progress_sender` 是发送节流发现快照的可选尽力而为通道。
///
/// # Returns
/// 返回值
/// Returns the sorted validated candidates, or cancellation/filesystem/limit error.
/// 返回已排序且经校验的候选，或取消/文件系统/上限错误。
fn scan_markdown_files(
    root: &Path,
    cancellation: &AtomicBool,
    progress_sender: Option<&mpsc::UnboundedSender<ScanProgressSnapshot>>,
) -> Result<Vec<ScannedMarkdownFile>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    // LastProgressAt limits snapshot emission while filesystem traversal makes progress.
    // LastProgressAt 在文件系统遍历推进时限制快照发送频率。
    let mut last_progress_at = Instant::now()
        .checked_sub(Duration::from_millis(250))
        .unwrap_or_else(Instant::now);
    // LastVisitedPath retains the latest real path for the mandatory terminal discovery update.
    // LastVisitedPath 保留最新真实路径，用于必须发送的终态发现更新。
    let mut last_visited_path = String::from(".");
    while let Some(directory) = pending.pop() {
        require_operation_not_cancelled(cancellation)?;
        last_visited_path = import_progress_path(root, &directory);
        if last_progress_at.elapsed() >= Duration::from_millis(100) {
            publish_scan_progress(progress_sender, files.len() as u64, &last_visited_path);
            last_progress_at = Instant::now();
        }
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("读取导入目录失败（{}）：{error}", directory.display()))?;
        for entry in entries {
            require_operation_not_cancelled(cancellation)?;
            let entry = entry.map_err(|error| format!("读取导入目录项失败：{error}"))?;
            let entry_metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| format!("读取导入目录项类型失败：{error}"))?;
            if metadata_is_link_or_reparse(&entry_metadata) {
                continue;
            }
            let file_type = entry_metadata.file_type();
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if !file_type.is_file()
                || !path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            if files.len() >= MAX_IMPORT_FILES {
                return Err(format!("Markdown 文件数量超过上限 {MAX_IMPORT_FILES}。"));
            }
            if entry_metadata.len() > MAX_MARKDOWN_FILE_BYTES {
                return Err(format!(
                    "Markdown 文件超过单文件上限 {} 字节：{}",
                    MAX_MARKDOWN_FILE_BYTES,
                    path.display()
                ));
            }
            total_bytes = total_bytes.saturating_add(entry_metadata.len());
            if total_bytes > MAX_IMPORT_BYTES {
                return Err(format!(
                    "Markdown 导入总字节数超过上限 {MAX_IMPORT_BYTES}。"
                ));
            }
            // Relative is derived from a path emitted by read_dir under the canonical root.
            // Relative 来自规范根目录下 read_dir 直接产生的路径。
            let relative = path.strip_prefix(root).map_err(|error| {
                format!("计算 Markdown 相对路径失败（{}）：{error}", path.display())
            })?;
            let relative_path = slash_path(relative)?;
            last_visited_path.clone_from(&relative_path);
            let folder_path = relative
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(slash_path)
                .transpose()?
                .unwrap_or_default();
            files.push(ScannedMarkdownFile {
                relative_path_key: relative_path.to_lowercase(),
                relative_path,
                folder_path,
                source_path: path,
                byte_size: entry_metadata.len(),
                modified_at_ms: entry_metadata.modified().ok().and_then(system_time_millis),
            });
            if last_progress_at.elapsed() >= Duration::from_millis(100) {
                publish_scan_progress(progress_sender, files.len() as u64, &last_visited_path);
                last_progress_at = Instant::now();
            }
        }
    }
    publish_scan_progress(progress_sender, files.len() as u64, &last_visited_path);
    files.sort_by(|left, right| left.relative_path_key.cmp(&right.relative_path_key));
    if let Some(duplicate) = files
        .windows(2)
        .find(|pair| pair[0].relative_path_key == pair[1].relative_path_key)
    {
        return Err(format!(
            "Markdown 相对路径在大小写规范化后冲突：{} 与 {}",
            duplicate[0].relative_path, duplicate[1].relative_path
        ));
    }
    Ok(files)
}

/// Publishes one best-effort scan snapshot without coupling filesystem work to UI lifetime.
/// 发布一份尽力而为的扫描快照，不将文件系统工作与 UI 生命周期耦合。
///
/// # Parameters
/// 参数
/// `progress_sender` is the optional best-effort channel for throttled discovery snapshots.
/// `progress_sender` 是发送节流发现快照的可选尽力而为通道。
/// `discovered_files` is the current count of accepted Markdown candidates.
/// `discovered_files` 是当前已接受 Markdown 候选数量。
/// `current_path` is the current display-only relative traversal path.
/// `current_path` 是当前仅供显示的相对遍历路径。
///
/// # Returns
/// 返回值
/// Returns no value; a missing or closed progress receiver does not fail scanning.
/// 返回无值；进度接收者缺失或关闭不会导致扫描失败。
fn publish_scan_progress(
    progress_sender: Option<&mpsc::UnboundedSender<ScanProgressSnapshot>>,
    discovered_files: u64,
    current_path: &str,
) {
    if let Some(progress_sender) = progress_sender {
        let _ = progress_sender.send(ScanProgressSnapshot {
            discovered_files,
            current_path: current_path.to_string(),
        });
    }
}

/// Formats one visited filesystem path relative to the authorized import root for display only.
/// 仅为展示目的，将一个已访问文件系统路径格式化为相对于已授权导入根目录的路径。
///
/// # Parameters
/// 参数
/// `root` is the canonical directory or input path described by this helper.
/// `root` 是当前助手所述的规范目录或输入路径。
/// `path` is the exact filesystem path being formatted or normalized.
/// `path` 是正在格式化或规范化的精确文件系统路径。
///
/// # Returns
/// 返回值
/// Returns the deterministic path representation for the documented purpose.
/// 返回用于所述用途的确定性路径表示。
fn import_progress_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if relative.as_os_str().is_empty() {
        String::from(".")
    } else {
        relative.to_string_lossy().replace('\\', "/")
    }
}

/// Canonicalizes and validates an import root directory.
/// 规范化并验证一个导入根目录。
///
/// # Parameters
/// 参数
/// `root` is the canonical directory or input path described by this helper.
/// `root` 是当前助手所述的规范目录或输入路径。
///
/// # Returns
/// 返回值
/// Returns the existing canonical directory, or a path validation error.
/// 返回现有规范目录，或路径校验错误。
pub(super) fn canonical_import_root(root: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(root)
        .map_err(|error| format!("规范化导入目录失败（{}）：{error}", root.display()))?;
    if !canonical.is_dir() {
        return Err(format!("导入路径不是目录：{}", canonical.display()));
    }
    Ok(canonical)
}

/// Converts a relative host path to the database slash convention.
/// 将相对宿主路径转换为数据库斜杠约定。
///
/// # Parameters
/// 参数
/// `path` is the exact filesystem path being formatted or normalized.
/// `path` 是正在格式化或规范化的精确文件系统路径。
///
/// # Returns
/// 返回值
/// Returns the exact validated source identity or normalized relative path, or an error.
/// 返回精确校验后的来源身份或规范相对路径，或错误。
pub(super) fn slash_path(path: &Path) -> Result<String, String> {
    let parts: Result<Vec<&str>, String> = path
        .components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .ok_or_else(|| format!("资料路径不是有效 UTF-8：{}", path.display())),
            _ => Err(format!("资料相对路径包含非法组件：{}", path.display())),
        })
        .collect();
    parts.map(|parts| parts.join("/"))
}

/// Produces the platform-aware comparison key for an absolute source path.
/// 生成绝对来源路径的平台感知比较键。
///
/// # Parameters
/// 参数
/// `path` is the exact filesystem path being formatted or normalized.
/// `path` 是正在格式化或规范化的精确文件系统路径。
///
/// # Returns
/// 返回值
/// Returns the deterministic path representation for the documented purpose.
/// 返回用于所述用途的确定性路径表示。
pub(super) fn path_key(path: &Path) -> String {
    let value = runtime_assets::native_host_path_text_from_path(path);
    #[cfg(windows)]
    {
        value.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        value
    }
}

/// Converts one system time into Unix milliseconds when representable.
/// 在可表示时将一个系统时间转换为 Unix 毫秒时间戳。
///
/// # Parameters
/// 参数
/// `value` is the source modification time to convert if representable.
/// `value` 是可表示时要转换的来源修改时间。
///
/// # Returns
/// 返回值
/// Returns representable Unix milliseconds, or None for an out-of-range timestamp.
/// 返回可表示的 Unix 毫秒，否则对超范围时间返回 None。
fn system_time_millis(value: SystemTime) -> Option<i64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

/// Appends one bounded warning when report capacity remains.
/// 在报告仍有容量时追加一条有界告警。
///
/// # Parameters
/// 参数
/// `warnings` is the bounded import warning list to append to.
/// `warnings` 是待追加的有界导入告警列表。
/// `warning` is the warning text to truncate before retention.
/// `warning` 是保留前要截断的告警文本。
///
/// # Returns
/// 返回值
/// Returns no value after retaining a bounded warning when capacity remains.
/// 返回有剩余容量时保留一条有界告警后不返回值。
fn push_warning(warnings: &mut Vec<String>, warning: String) {
    if warnings.len() < MAX_IMPORT_WARNINGS {
        warnings.push(truncate_chars(&warning, MAX_WARNING_CHARS));
    }
}

#[cfg(test)]
mod tests;
