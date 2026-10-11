//! Jieba dictionary, cancellable FTS maintenance, and exact derived-index integrity.
//! Jieba 词典、可取消 FTS 维护与精确派生索引完整性。

use super::super::models::{
    KnowledgeCustomWord, KnowledgeImportJobSummary, KnowledgeIndexIntegritySummary,
    KnowledgeOperationKind,
};
use super::int;
use super::{
    KNOWLEDGE_FTS_INDEX, KNOWLEDGE_OPERATION_CANCELLED_ERROR, KnowledgeBaseRuntime,
    PreparedKnowledgeOperation, nonnegative_u64, queued_operation_summary,
    require_operation_not_cancelled, text, unix_epoch_millis,
};
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use serde::Deserialize;
use std::sync::atomic::AtomicBool;
use vulcan_sqlite_engine::tokenizer::TokenizerMode;

/// Maximum FTS mutations committed per cancellable storage batch.
/// 每个可取消存储批次提交的最大 FTS 变更数。
const FTS_MUTATION_BATCH_SIZE: usize = 512;

/// Maximum stale FTS identifiers deleted by one parameterized set statement.
/// 单条参数化集合语句删除的最大过期 FTS 标识符数量。
const FTS_DELETE_PARAMETER_BATCH_SIZE: usize = 900;

/// Metadata values required by the fail-closed search gate.
/// 搜索失败关闭门禁需要的元数据值。
#[derive(Debug, Deserialize)]
struct IndexGateRow {
    /// Current index state.
    /// 当前索引状态。
    index_status: String,
    /// Current authoritative content revision.
    /// 当前权威内容版本。
    content_revision: String,
    /// Revision represented by FTS.
    /// FTS 表示的内容版本。
    indexed_content_revision: String,
    /// Current dictionary revision.
    /// 当前词典版本。
    dictionary_revision: String,
    /// Dictionary revision represented by FTS.
    /// FTS 表示的词典版本。
    indexed_dictionary_revision: String,
    /// Configured tokenizer identifier.
    /// 配置的分词器标识符。
    tokenizer: String,
}

/// Whether the derived FTS virtual table exists in one isolated library database.
/// 派生 FTS 虚拟表是否存在于一个隔离知识库数据库中。
#[derive(Debug, Deserialize)]
struct IndexTablePresenceRow {
    /// SQLite integer boolean indicating exact table-name presence.
    /// 表示精确表名是否存在的 SQLite 整数布尔值。
    index_exists: i64,
}

/// Exact set and source-field diagnostics for one existing FTS table.
/// 一个已存在 FTS 表的精确集合与源字段诊断。
#[derive(Debug, Deserialize)]
struct IndexIntegrityCountsRow {
    /// Number of authoritative Markdown chunks.
    /// 权威 Markdown 切片数量。
    authoritative_count: i64,
    /// Number of physical FTS rows.
    /// 物理 FTS 记录数量。
    indexed_count: i64,
    /// Authoritative chunks without one exact matching FTS source row.
    /// 没有精确匹配 FTS 源记录的权威切片数量。
    missing_or_mismatched_count: i64,
    /// FTS rows without one exact matching authoritative chunk.
    /// 没有精确匹配权威切片的 FTS 记录数量。
    orphan_or_mismatched_count: i64,
}

/// One checked scalar chunk count returned by a bounded aggregate query.
/// 一个由有界聚合查询返回的已检查切片标量计数。
#[derive(Debug, Deserialize)]
struct ChunkCountRow {
    /// Aggregate chunk count.
    /// 聚合切片数量。
    chunk_count: i64,
}

/// ImportIndexSyncOutcome reports completion or observed cancellation for replay and sync callers.
/// ImportIndexSyncOutcome 向回放及同步调用方报告完成或已观察到的取消。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImportIndexSyncOutcome {
    /// The requested replay or sync completed without observing cancellation.
    /// 请求的回放或同步已完成且未观察到取消。
    Completed,
    /// Synchronization stopped at a safe batch boundary and requires an authoritative rebuild.
    /// 同步在安全批次边界停止并需要一次权威重建。
    Cancelled,
}

impl KnowledgeBaseRuntime {
    /// Lists custom Jieba words stored in one active library database.
    /// 列出一个活动知识库数据库中存储的 Jieba 自定义词。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    ///
    /// # Returns
    /// 返回值
    /// Returns the checked custom-word list, or a storage/conversion error.
    /// 返回已校验的自定义词列表，或存储/转换错误。
    pub async fn list_custom_words(
        &self,
        library_id: &str,
    ) -> Result<Vec<KnowledgeCustomWord>, String> {
        self.require_active_library(library_id).await?;
        let (_space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let result = local_sql::list_custom_words_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
        )
        .await
        .map_err(|error| format!("读取知识库 Jieba 自定义词失败：{error}"))?;
        result
            .words
            .into_iter()
            .map(|entry| {
                Ok(KnowledgeCustomWord {
                    word: entry.word,
                    weight: u32::try_from(entry.weight)
                        .map_err(|_| String::from("Jieba 自定义词权重超出 u32 范围。"))?,
                })
            })
            .collect()
    }

    /// Inserts or updates one custom Jieba word and marks the derived index dirty.
    /// 新增或更新一个 Jieba 自定义词并将派生索引标记为脏。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `word` is the custom Jieba text validated after trimming.
    /// `word` 是去除首尾空白后校验的自定义 Jieba 文本。
    /// `weight` is the requested word weight, clamped to at least one on write.
    /// `weight` 是请求的词权重，写入时下限为一。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    pub async fn upsert_custom_word(
        &self,
        library_id: &str,
        word: &str,
        weight: u32,
    ) -> Result<(), String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        self.require_active_library(library_id).await?;
        let word = validate_custom_word(word)?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        local_sql::upsert_custom_word_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            word,
            weight.max(1) as usize,
        )
        .await
        .map_err(|error| format!("保存知识库 Jieba 自定义词失败：{error}"))?;
        self.mark_dictionary_dirty(library_id, &space_id, &binding_id)
            .await
    }

    /// Removes one custom Jieba word and marks the derived index dirty.
    /// 删除一个 Jieba 自定义词并将派生索引标记为脏。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `word` is the custom Jieba text validated after trimming.
    /// `word` 是去除首尾空白后校验的自定义 Jieba 文本。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    pub async fn remove_custom_word(&self, library_id: &str, word: &str) -> Result<(), String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        self.require_active_library(library_id).await?;
        let word = validate_custom_word(word)?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        local_sql::remove_custom_word_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            word,
        )
        .await
        .map_err(|error| format!("删除知识库 Jieba 自定义词失败：{error}"))?;
        self.mark_dictionary_dirty(library_id, &space_id, &binding_id)
            .await
    }

    /// Rebuilds the complete Jieba FTS index from authoritative chunk rows.
    /// 从权威切片记录完整重建 Jieba FTS 索引。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    ///
    /// # Returns
    /// 返回值
    /// Returns the checked revision or authoritative work count described by this operation, or an error.
    /// 返回当前操作所述的已校验修订或权威工作量，或错误。
    pub async fn rebuild_index(&self, library_id: &str) -> Result<u64, String> {
        self.require_active_library(library_id).await?;
        // Operation reserves the library before synchronous maintenance starts.
        // Operation 在同步维护开始前预留该知识库。
        let operation = self
            .prepare_operation(library_id, KnowledgeOperationKind::Rebuild, None)
            .await?;
        self.execute_prepared_index_operation(operation).await
    }

    /// Starts a detached destructive FTS rebuild and immediately returns its durable job snapshot.
    /// 启动一个分离式破坏性 FTS 重建，并立即返回其持久化任务快照。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    ///
    /// # Returns
    /// 返回值
    /// Returns the durable queued operation snapshot after executor startup, or an error.
    /// 返回执行器启动后的持久排队任务快照，或错误。
    pub async fn start_rebuild_index(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeImportJobSummary, String> {
        self.start_index_operation(library_id, KnowledgeOperationKind::Rebuild)
            .await
    }

    /// Repairs derived rows, rebuilding FTS when dictionary or tokenizer changes require it.
    /// 修复派生记录，并在词典或分词器变化要求时重建 FTS。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    ///
    /// # Returns
    /// 返回值
    /// Returns the checked revision or authoritative work count described by this operation, or an error.
    /// 返回当前操作所述的已校验修订或权威工作量，或错误。
    pub async fn repair_index(&self, library_id: &str) -> Result<u64, String> {
        self.require_active_library(library_id).await?;
        // Operation reserves the library before synchronous maintenance starts.
        // Operation 在同步维护开始前预留该知识库。
        let operation = self
            .prepare_operation(library_id, KnowledgeOperationKind::Repair, None)
            .await?;
        self.execute_prepared_index_operation(operation).await
    }

    /// Starts detached FTS repair with conditional retokenization and returns its durable job snapshot.
    /// 启动必要时重新分词的分离式 FTS 补全并返回持久化任务快照。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    ///
    /// # Returns
    /// 返回值
    /// Returns the durable queued operation snapshot after executor startup, or an error.
    /// 返回执行器启动后的持久排队任务快照，或错误。
    pub async fn start_repair_index(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeImportJobSummary, String> {
        self.start_index_operation(library_id, KnowledgeOperationKind::Repair)
            .await
    }

    /// Reserves and spawns one detached repair or rebuild operation.
    /// 预留并启动一个分离式补全或重建操作。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `operation_kind` is the explicit repair or rebuild kind.
    /// `operation_kind` 是明确的补全或重建类型。
    ///
    /// # Returns
    /// 返回值
    /// Returns the durable queued operation snapshot after executor startup, or an error.
    /// 返回执行器启动后的持久排队任务快照，或错误。
    async fn start_index_operation(
        &self,
        library_id: &str,
        operation_kind: KnowledgeOperationKind,
    ) -> Result<KnowledgeImportJobSummary, String> {
        if !matches!(
            operation_kind,
            KnowledgeOperationKind::Repair | KnowledgeOperationKind::Rebuild
        ) {
            return Err(String::from(
                "索引维护操作只能使用 repair 或 rebuild 类型。",
            ));
        }
        self.require_active_library(library_id).await?;
        // Operation is visible to refresh recovery before detached execution starts.
        // Operation 在分离执行开始前已对刷新恢复可见。
        let operation = self
            .prepare_operation(library_id, operation_kind, None)
            .await?;
        // Snapshot retains the exact kind even after the caller's request is released.
        // Snapshot 即使在调用请求释放后仍保留精确操作类型。
        let snapshot = queued_operation_summary(&operation);
        // Runtime owns the executor independently from the HTTP handler future.
        // Runtime 独立于 HTTP 处理器 future 持有执行器。
        let runtime = self.clone();
        tokio::spawn(async move {
            if let Err(error) = runtime.execute_prepared_index_operation(operation).await {
                eprintln!("[knowledge-base] detached index operation failed: {error}");
            }
        });
        Ok(snapshot)
    }

    /// Executes one prepared repair or rebuild and persists a terminal job and index state.
    /// 执行一个已预留补全或重建，并持久化任务与索引终态。
    ///
    /// # Parameters
    /// 参数
    /// `operation` is the already reserved job, library, kind and cooperative cancellation signal.
    /// `operation` 是已预留的任务、知识库、类型与协作取消信号。
    ///
    /// # Returns
    /// 返回值
    /// Returns the checked revision or authoritative work count described by this operation, or an error.
    /// 返回当前操作所述的已校验修订或权威工作量，或错误。
    async fn execute_prepared_index_operation(
        &self,
        operation: PreparedKnowledgeOperation,
    ) -> Result<u64, String> {
        // BindingIdentity survives outside the result only when activation succeeds.
        // BindingIdentity 仅在激活成功时于结果外继续存在。
        let mut binding_identity: Option<(String, String)> = None;
        // Result includes every lock, state, and physical index mutation failure.
        // Result 包含每个锁、状态和物理索引变更失败。
        let result = async {
            // Guard serializes this maintenance operation with all library mutations.
            // Guard 将本维护操作与全部知识库变更串行化。
            let _guard = self.library_mutation_guard(&operation.library_id).await?;
            require_operation_not_cancelled(&operation.cancellation)?;
            self.require_active_library(&operation.library_id).await?;
            // ExactBinding is retained for terminal failure projection.
            // ExactBinding 为终态失败投影保留。
            let exact_binding = self.ensure_library_binding(&operation.library_id).await?;
            binding_identity = Some(exact_binding.clone());
            // Phase is stable and directly consumable by the progress UI.
            // Phase 稳定且可由进度界面直接使用。
            let phase = match operation.operation_kind {
                KnowledgeOperationKind::Repair => "repairing",
                KnowledgeOperationKind::Rebuild => "rebuilding",
                KnowledgeOperationKind::Import => {
                    return Err(String::from("导入操作不能进入索引维护执行器。"));
                }
                KnowledgeOperationKind::DisplayBuild | KnowledgeOperationKind::VectorBuild => {
                    return Err(String::from("派生构建操作不能进入原始 FTS 维护执行器。"));
                }
            };
            self.update_operation_phase(&operation.job_id, "running", phase)
                .await?;
            self.set_library_index_state(
                &operation.library_id,
                &exact_binding.0,
                &exact_binding.1,
                "rebuilding",
                "",
            )
            .await?;
            match operation.operation_kind {
                KnowledgeOperationKind::Repair => {
                    self.repair_index_inner(
                        &operation.library_id,
                        &exact_binding.0,
                        &exact_binding.1,
                        &operation.job_id,
                        &operation.cancellation,
                    )
                    .await
                }
                KnowledgeOperationKind::Rebuild => {
                    self.rebuild_index_inner(
                        &operation.library_id,
                        &exact_binding.0,
                        &exact_binding.1,
                        Some(&operation.job_id),
                        Some(&operation.cancellation),
                    )
                    .await
                }
                KnowledgeOperationKind::Import => {
                    Err(String::from("导入操作不能进入索引维护执行器。"))
                }
                KnowledgeOperationKind::DisplayBuild | KnowledgeOperationKind::VectorBuild => {
                    Err(String::from("派生构建操作不能进入原始 FTS 维护执行器。"))
                }
            }
        }
        .await;

        // FinishResult owns the durable terminal operation transition.
        // FinishResult 持有持久化操作终态转换。
        let finish_result = match &result {
            Ok(processed_items) => {
                self.finish_index_operation_job(&operation.job_id, *processed_items)
                    .await
            }
            Err(error) if error == KNOWLEDGE_OPERATION_CANCELLED_ERROR => {
                if let Some((space_id, binding_id)) = binding_identity.as_ref() {
                    let _ = self
                        .set_library_index_state(
                            &operation.library_id,
                            space_id,
                            binding_id,
                            "dirty",
                            "",
                        )
                        .await;
                }
                self.finish_cancelled_operation_job(&operation.job_id).await
            }
            Err(error) => {
                if let Some((space_id, binding_id)) = binding_identity.as_ref() {
                    let _ = self
                        .set_library_index_state(
                            &operation.library_id,
                            space_id,
                            binding_id,
                            "failed",
                            error,
                        )
                        .await;
                }
                self.finish_failed_operation_job(&operation.job_id, error)
                    .await
            }
        };
        self.release_active_operation(&operation.library_id, &operation.job_id)
            .await;
        finish_result?;
        result
    }

    /// Synchronizes only changed and deleted chunk rows into vldb-managed FTS.
    /// 仅将变更与删除的切片记录同步到 vldb 管理的 FTS。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `job_id` is the durable operation identity used for progress when supplied.
    /// `job_id` 是提供时用于进度记录的持久操作身份。
    /// `cancellation` is the cooperative signal checked at the existing safe batch boundaries.
    /// `cancellation` 是在现有安全批次边界检查的协作取消信号。
    /// `stale_chunk_ids` is the exact removed or changed chunk identities from the committed import.
    /// `stale_chunk_ids` 是已提交导入中被删除或变更的精确切片身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the completed or cooperatively cancelled replay outcome, or a storage error.
    /// 返回完成或协作取消的回放结果，或存储错误。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn sync_changed_fts(
        &self,
        job_id: &str,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        stale_chunk_ids: &[String],
        cancellation: &AtomicBool,
    ) -> Result<ImportIndexSyncOutcome, String> {
        local_sql::ensure_fts_index_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            KNOWLEDGE_FTS_INDEX,
            TokenizerMode::Jieba,
        )
        .await
        .map_err(|error| format!("创建 Jieba FTS 索引失败：{error}"))?;
        for chunk_ids in stale_chunk_ids.chunks(FTS_DELETE_PARAMETER_BATCH_SIZE) {
            if require_operation_not_cancelled(cancellation).is_err() {
                return Ok(ImportIndexSyncOutcome::Cancelled);
            }
            // Placeholders is generated only from the bounded slice length; identifiers remain bound values.
            // Placeholders 仅根据有界切片长度生成；标识符仍以绑定值传入。
            let placeholders = std::iter::repeat_n("?", chunk_ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            local_sql::execute_script_async(
                &self.host_runtime.local,
                self.resolve_library_binding(binding_id)?,
                format!("DELETE FROM {KNOWLEDGE_FTS_INDEX} WHERE id IN ({placeholders});"),
                chunk_ids.iter().map(text).collect::<Vec<_>>(),
            )
            .await
            .map_err(|error| format!("集合式删除过期 FTS 切片失败：{error}"))?;
        }
        // TotalItems is derived from the committed authoritative rows selected by staged documents.
        // TotalItems 从暂存文档选择出的已提交权威记录派生。
        let total_items = self.staged_chunk_count(space_id, binding_id).await?;
        let write_result = self
            .replay_staged_fts_for_import(job_id, space_id, binding_id, total_items, cancellation)
            .await?;
        if write_result == ImportIndexSyncOutcome::Cancelled {
            return Ok(write_result);
        }
        if require_operation_not_cancelled(cancellation).is_err() {
            return Ok(ImportIndexSyncOutcome::Cancelled);
        }
        self.verify_index_rows_exact(space_id, binding_id).await?;
        self.mark_index_ready(space_id, binding_id).await?;
        self.set_registry_index_status(library_id, "ready").await?;
        Ok(ImportIndexSyncOutcome::Completed)
    }

    /// Rebuilds authoritative FTS rows in cancellable batches for one visible import job.
    /// 为一个可见导入任务以可取消批次重建权威 FTS 记录。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `job_id` is the durable operation identity used for progress when supplied.
    /// `job_id` 是提供时用于进度记录的持久操作身份。
    /// `cancellation` is the cooperative signal checked at the existing safe batch boundaries.
    /// `cancellation` 是在现有安全批次边界检查的协作取消信号。
    ///
    /// # Returns
    /// 返回值
    /// Returns the completed or cooperatively cancelled replay outcome, or a storage error.
    /// 返回完成或协作取消的回放结果，或存储错误。
    pub(super) async fn rebuild_index_for_import(
        &self,
        job_id: &str,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        cancellation: &AtomicBool,
    ) -> Result<ImportIndexSyncOutcome, String> {
        require_operation_not_cancelled(cancellation)?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            format!("DROP TABLE IF EXISTS {KNOWLEDGE_FTS_INDEX};"),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("删除旧 FTS 索引失败：{error}"))?;
        local_sql::ensure_fts_index_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            KNOWLEDGE_FTS_INDEX,
            TokenizerMode::Jieba,
        )
        .await
        .map_err(|error| format!("重建 Jieba FTS 索引结构失败：{error}"))?;
        // TotalItems is read without materializing all chunk bodies across the storage boundary.
        // TotalItems 无需跨存储边界物化全部切片正文即可读取。
        let total_items = nonnegative_u64(
            self.authoritative_counts(space_id, binding_id)
                .await?
                .chunk_count,
            "chunk_count",
        )?;
        let write_result = self
            .replay_authoritative_fts_for_import(
                job_id,
                space_id,
                binding_id,
                total_items,
                cancellation,
            )
            .await?;
        if write_result == ImportIndexSyncOutcome::Cancelled {
            return Ok(write_result);
        }
        self.verify_index_rows_exact(space_id, binding_id).await?;
        self.mark_index_ready(space_id, binding_id).await?;
        self.set_registry_index_status(library_id, "ready").await?;
        Ok(ImportIndexSyncOutcome::Completed)
    }

    /// Replays only committed chunks belonging to staged changed documents in set-based windows.
    /// 仅以集合式窗口回放属于暂存变更文档的已提交切片。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `job_id` is the durable operation identity used for progress when supplied.
    /// `job_id` 是提供时用于进度记录的持久操作身份。
    /// `cancellation` is the cooperative signal checked at the existing safe batch boundaries.
    /// `cancellation` 是在现有安全批次边界检查的协作取消信号。
    /// `total_items` is the fixed authoritative or committed changed-chunk work count.
    /// `total_items` 是固定的权威或已提交变更切片工作量。
    ///
    /// # Returns
    /// 返回值
    /// Returns the completed or cooperatively cancelled replay outcome, or a storage error.
    /// 返回完成或协作取消的回放结果，或存储错误。
    async fn replay_staged_fts_for_import(
        &self,
        job_id: &str,
        space_id: &str,
        binding_id: &str,
        total_items: u64,
        cancellation: &AtomicBool,
    ) -> Result<ImportIndexSyncOutcome, String> {
        self.update_import_index_progress(job_id, 0, total_items, None)
            .await?;
        // Offset advances over a fixed staged-document result set and cannot skip inserted rows.
        // Offset 在固定的暂存文档结果集上推进，不会跳过已插入记录。
        let mut offset = 0usize;
        while offset < total_items as usize {
            if require_operation_not_cancelled(cancellation).is_err() {
                return Ok(ImportIndexSyncOutcome::Cancelled);
            }
            // BatchSize bounds FTS5 write amplification while reducing storage calls fourfold from 128 rows.
            // BatchSize 限制 FTS5 写放大，同时相较 128 行窗口将存储调用次数减少四倍。
            let batch_size = FTS_MUTATION_BATCH_SIZE.min(total_items as usize - offset);
            self.replay_staged_fts_batch(space_id, binding_id, batch_size, offset)
                .await?;
            offset += batch_size;
            self.update_import_index_progress(job_id, offset as u64, total_items, None)
                .await?;
        }
        self.update_import_index_progress(job_id, total_items, total_items, None)
            .await?;
        Ok(ImportIndexSyncOutcome::Completed)
    }

    /// Replays all authoritative rows in set-based batches with durable import progress.
    /// 使用集合式批次回放全部权威记录，并持久化导入进度。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `job_id` is the durable operation identity used for progress when supplied.
    /// `job_id` 是提供时用于进度记录的持久操作身份。
    /// `cancellation` is the cooperative signal checked at the existing safe batch boundaries.
    /// `cancellation` 是在现有安全批次边界检查的协作取消信号。
    /// `total_items` is the fixed authoritative or committed changed-chunk work count.
    /// `total_items` 是固定的权威或已提交变更切片工作量。
    ///
    /// # Returns
    /// 返回值
    /// Returns the completed or cooperatively cancelled replay outcome, or a storage error.
    /// 返回完成或协作取消的回放结果，或存储错误。
    async fn replay_authoritative_fts_for_import(
        &self,
        job_id: &str,
        space_id: &str,
        binding_id: &str,
        total_items: u64,
        cancellation: &AtomicBool,
    ) -> Result<ImportIndexSyncOutcome, String> {
        self.update_import_index_progress(job_id, 0, total_items, None)
            .await?;
        // Offset advances over the immutable authoritative chunk set during the mutation lock.
        // Offset 在变更锁期间保持不变的权威切片集合上推进。
        let mut offset = 0usize;
        while offset < total_items as usize {
            if require_operation_not_cancelled(cancellation).is_err() {
                return Ok(ImportIndexSyncOutcome::Cancelled);
            }
            // BatchSize bounds one FTS transaction and in-process storage latency.
            // BatchSize 限制单次 FTS 事务与进程内存储延迟。
            let batch_size = FTS_MUTATION_BATCH_SIZE.min(total_items as usize - offset);
            self.replay_authoritative_fts_batch(space_id, binding_id, batch_size, offset)
                .await?;
            offset += batch_size;
            self.update_import_index_progress(job_id, offset as u64, total_items, None)
                .await?;
        }
        self.update_import_index_progress(job_id, total_items, total_items, None)
            .await?;
        Ok(ImportIndexSyncOutcome::Completed)
    }

    /// Replays one staged changed-document chunk window with one set-based SQL statement.
    /// 使用一条集合式 SQL 语句回放一个暂存变更文档切片窗口。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `limit` is the bounded SQL replay window size.
    /// `limit` 是有界 SQL 回放窗口大小。
    /// `offset` is the offset over the fixed authoritative or staged-document join result.
    /// `offset` 是固定权威集合或暂存文档联结结果上的偏移。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    async fn replay_staged_fts_batch(
        &self,
        _space_id: &str,
        binding_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(), String> {
        // Sql selects only committed chunks whose document identity remains in staging.
        // Sql 仅选择文档身份仍处于暂存集合中的已提交切片。
        let sql = format!(
            r#"INSERT INTO {KNOWLEDGE_FTS_INDEX}(id, file_path, title, content)
               SELECT c.id,
                      d.relative_path,
                      CASE WHEN c.heading_path = ''
                           THEN d.title
                           ELSE d.title || ' / ' || c.heading_path
                      END,
                      c.content
               FROM knowledge_chunks c
               INNER JOIN knowledge_documents d ON d.id = c.document_id
               INNER JOIN knowledge_staged_documents s ON s.id = d.id
               ORDER BY d.relative_path_key ASC, c.ordinal ASC
               LIMIT {limit} OFFSET {offset};"#
        );
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            sql,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("集合式写入变更 Jieba FTS 切片失败：{error}"))?;
        Ok(())
    }

    /// Replays one ordered authoritative chunk window into FTS with one set-based SQL statement.
    /// 使用一条集合式 SQL 语句将一个有序权威切片窗口回放到 FTS。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `limit` is the bounded SQL replay window size.
    /// `limit` 是有界 SQL 回放窗口大小。
    /// `offset` is the offset over the fixed authoritative or staged-document join result.
    /// `offset` 是固定权威集合或暂存文档联结结果上的偏移。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    async fn replay_authoritative_fts_batch(
        &self,
        _space_id: &str,
        binding_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(), String> {
        // SetBasedReplay lets SQLite tokenize the selected rows without one statement per chunk.
        // SetBasedReplay 让 SQLite 对选中行直接分词，避免每个切片执行一条语句。
        let sql = format!(
            r#"INSERT INTO {KNOWLEDGE_FTS_INDEX}(id, file_path, title, content)
               SELECT c.id,
                      d.relative_path,
                      CASE WHEN c.heading_path = ''
                           THEN d.title
                           ELSE d.title || ' / ' || c.heading_path
                      END,
                      c.content
               FROM knowledge_chunks c
               INNER JOIN knowledge_documents d ON d.id = c.document_id
               ORDER BY d.relative_path_key ASC, c.ordinal ASC
               LIMIT {limit} OFFSET {offset};"#
        );
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            sql,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("集合式回放 Jieba FTS 切片失败：{error}"))?;
        Ok(())
    }

    /// Repairs missing, mismatched, and orphaned FTS rows from authoritative chunks.
    /// 根据权威切片补全缺失、错配和孤立 FTS 记录。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `job_id` is the durable operation identity used for progress when supplied.
    /// `job_id` 是提供时用于进度记录的持久操作身份。
    /// `cancellation` is the cooperative signal checked at the existing safe batch boundaries.
    /// `cancellation` 是在现有安全批次边界检查的协作取消信号。
    ///
    /// # Returns
    /// 返回值
    /// Returns the checked revision or authoritative work count described by this operation, or an error.
    /// 返回当前操作所述的已校验修订或权威工作量，或错误。
    async fn repair_index_inner(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        job_id: &str,
        cancellation: &AtomicBool,
    ) -> Result<u64, String> {
        require_operation_not_cancelled(cancellation)?;
        // Before determines whether dictionary or tokenizer changes require complete retokenization.
        // Before 决定词典或 tokenizer 变更是否要求完整重新分词。
        let before = self
            .inspect_index_integrity_for_binding(library_id, space_id, binding_id)
            .await?;
        if before.tokenizer != "jieba"
            || before.dictionary_revision != before.indexed_dictionary_revision
        {
            self.update_operation_phase(job_id, "running", "rebuilding")
                .await?;
            return self
                .rebuild_index_inner(
                    library_id,
                    space_id,
                    binding_id,
                    Some(job_id),
                    Some(cancellation),
                )
                .await;
        }
        local_sql::ensure_fts_index_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            KNOWLEDGE_FTS_INDEX,
            TokenizerMode::Jieba,
        )
        .await
        .map_err(|error| format!("创建待补全 Jieba FTS 索引失败：{error}"))?;
        // TotalItems is authoritative work scanned by deterministic repair windows.
        // TotalItems 是确定性补全窗口扫描的权威工作量。
        let total_items = before.authoritative_chunk_count;
        self.update_import_index_progress(job_id, 0, total_items, None)
            .await?;
        // DeleteSql removes mismatched rows and every duplicate beyond the first exact physical row.
        // DeleteSql 删除错配记录，以及每组精确物理记录中第一条之外的全部重复项。
        let delete_sql = format!(
            r#"DELETE FROM {KNOWLEDGE_FTS_INDEX}
               WHERE rowid IN (
                   SELECT candidate.rowid
                     FROM (
                         SELECT f.rowid,
                                ROW_NUMBER() OVER (
                                    PARTITION BY f.id, f.file_path, f.title, f.content
                                    ORDER BY f.rowid ASC
                                ) AS duplicate_ordinal,
                                EXISTS (
                                    SELECT 1
                                      FROM knowledge_chunks c
                                      INNER JOIN knowledge_documents d ON d.id = c.document_id
                                     WHERE c.id = f.id
                                       AND f.file_path = d.relative_path
                                       AND f.title = CASE WHEN c.heading_path = ''
                                                          THEN d.title
                                                          ELSE d.title || ' / ' || c.heading_path END
                                       AND f.content = c.content
                                ) AS source_matches
                           FROM {KNOWLEDGE_FTS_INDEX} f
                     ) candidate
                    WHERE candidate.duplicate_ordinal > 1
                       OR candidate.source_matches = 0
               );"#
        );
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            delete_sql,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("删除孤立或错配 FTS 切片失败：{error}"))?;
        // Offset advances over authoritative rows rather than the shrinking missing-row subset.
        // Offset 在权威记录而非不断缩小的缺失记录子集上推进。
        let mut offset = 0usize;
        while offset < total_items as usize {
            require_operation_not_cancelled(cancellation)?;
            // BatchSize bounds each repair transaction below the maintenance deadline.
            // BatchSize 将每次补全事务限制在维护期限内。
            let batch_size = FTS_MUTATION_BATCH_SIZE.min(total_items as usize - offset);
            self.replay_missing_fts_batch(space_id, binding_id, batch_size, offset)
                .await?;
            offset += batch_size;
            self.update_import_index_progress(job_id, offset as u64, total_items, None)
                .await?;
        }
        self.verify_index_rows_exact(space_id, binding_id).await?;
        self.mark_index_ready(space_id, binding_id).await?;
        // Counts are re-read after repair so central summaries never depend on stale projections.
        // Counts 在补全后重新读取，使中央摘要绝不依赖过期投影。
        let counts = self.authoritative_counts(space_id, binding_id).await?;
        self.project_registry_authoritative_counts(
            library_id,
            &counts,
            "ready",
            unix_epoch_millis()?,
        )
        .await?;
        Ok(total_items)
    }

    /// Inserts missing FTS rows from one fixed authoritative window.
    /// 从一个固定权威窗口插入缺失 FTS 记录。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `limit` is the bounded SQL replay window size.
    /// `limit` 是有界 SQL 回放窗口大小。
    /// `offset` is the offset over the fixed authoritative or staged-document join result.
    /// `offset` 是固定权威集合或暂存文档联结结果上的偏移。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    async fn replay_missing_fts_batch(
        &self,
        _space_id: &str,
        binding_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(), String> {
        // Sql applies LIMIT/OFFSET before one EXCEPT comparison so successful inserts cannot skip work.
        // Sql 在一次 EXCEPT 比较之前应用 LIMIT/OFFSET，因此成功插入不会跳过工作。
        let sql = format!(
            r#"INSERT INTO {KNOWLEDGE_FTS_INDEX}(id, file_path, title, content)
               SELECT id, file_path, title, content
                 FROM (
                     SELECT c.id AS id,
                            d.relative_path AS file_path,
                            CASE WHEN c.heading_path = ''
                                 THEN d.title
                                 ELSE d.title || ' / ' || c.heading_path
                            END AS title,
                            c.content AS content
                       FROM knowledge_chunks c
                       INNER JOIN knowledge_documents d ON d.id = c.document_id
                       ORDER BY d.relative_path_key ASC, c.ordinal ASC
                       LIMIT {limit} OFFSET {offset}
                 )
               EXCEPT
               SELECT id, file_path, title, content FROM {KNOWLEDGE_FTS_INDEX};"#
        );
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            sql,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("集合式补全 Jieba FTS 切片失败：{error}"))?;
        Ok(())
    }

    /// Performs a destructive FTS-only rebuild followed by authoritative replay.
    /// 执行仅针对 FTS 的破坏性重建，随后从权威数据回放。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `job_id` is the durable operation identity used for progress when supplied.
    /// `job_id` 是提供时用于进度记录的持久操作身份。
    /// `cancellation` is the cooperative signal checked at the existing safe batch boundaries.
    /// `cancellation` 是在现有安全批次边界检查的协作取消信号。
    ///
    /// # Returns
    /// 返回值
    /// Returns the checked revision or authoritative work count described by this operation, or an error.
    /// 返回当前操作所述的已校验修订或权威工作量，或错误。
    async fn rebuild_index_inner(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        job_id: Option<&str>,
        cancellation: Option<&AtomicBool>,
    ) -> Result<u64, String> {
        if let Some(cancellation) = cancellation {
            require_operation_not_cancelled(cancellation)?;
        }
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            format!("DROP TABLE IF EXISTS {KNOWLEDGE_FTS_INDEX};"),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("删除旧 FTS 索引失败：{error}"))?;
        local_sql::ensure_fts_index_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            KNOWLEDGE_FTS_INDEX,
            TokenizerMode::Jieba,
        )
        .await
        .map_err(|error| format!("重建 Jieba FTS 索引结构失败：{error}"))?;
        // Counts supplies a constant authoritative denominator without materializing chunk bodies.
        // Counts 提供恒定权威分母，无需物化切片正文。
        let counts = self.authoritative_counts(space_id, binding_id).await?;
        // TotalItems is validated before conversion into pagination offsets.
        // TotalItems 在转换为分页偏移前完成校验。
        let total_items = nonnegative_u64(counts.chunk_count, "chunk_count")?;
        if let Some(job_id) = job_id {
            self.update_import_index_progress(job_id, 0, total_items, None)
                .await?;
        }
        // Offset advances over the immutable authoritative set held by the mutation guard.
        // Offset 在变更锁保护的不变权威集合上推进。
        let mut offset = 0usize;
        while offset < total_items as usize {
            if let Some(cancellation) = cancellation {
                require_operation_not_cancelled(cancellation)?;
            }
            // BatchSize keeps each storage call cancellable and below its operation deadline.
            // BatchSize 使每次存储调用可取消且保持在操作期限内。
            let batch_size = FTS_MUTATION_BATCH_SIZE.min(total_items as usize - offset);
            self.replay_authoritative_fts_batch(space_id, binding_id, batch_size, offset)
                .await?;
            offset += batch_size;
            if let Some(job_id) = job_id {
                self.update_import_index_progress(job_id, offset as u64, total_items, None)
                    .await?;
            }
        }
        self.verify_index_rows_exact(space_id, binding_id).await?;
        self.mark_index_ready(space_id, binding_id).await?;
        self.project_registry_authoritative_counts(
            library_id,
            &counts,
            "ready",
            unix_epoch_millis()?,
        )
        .await?;
        Ok(total_items)
    }

    /// Returns a complete metadata and physical-row integrity snapshot for one active library.
    /// 返回一个活动知识库的完整元数据与物理记录完整性快照。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    ///
    /// # Returns
    /// 返回值
    /// Returns the metadata/physical-row diagnostic, including an absent or dirty index, or a read error.
    /// 返回包含缺失或脏索引的元数据/物理行诊断，或读取错误。
    pub async fn inspect_index_integrity(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeIndexIntegritySummary, String> {
        self.require_active_library(library_id).await?;
        // BindingIdentity selects the only isolated SQLite database owned by this library.
        // BindingIdentity 选择该知识库拥有的唯一隔离 SQLite 数据库。
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        self.inspect_index_integrity_for_binding(library_id, &space_id, &binding_id)
            .await
    }

    /// Builds one complete integrity snapshot using an already resolved library binding.
    /// 使用已解析的知识库绑定构建一份完整完整性快照。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the metadata/physical-row diagnostic, including an absent or dirty index, or a read error.
    /// 返回包含缺失或脏索引的元数据/物理行诊断，或读取错误。
    async fn inspect_index_integrity_for_binding(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
    ) -> Result<KnowledgeIndexIntegritySummary, String> {
        // Gate contains the exact persisted revision and tokenizer state.
        // Gate 包含精确持久化的版本与 tokenizer 状态。
        let gate = self.load_index_gate(space_id, binding_id).await?;
        // PresenceRows checks sqlite_master before any SQL references the optional FTS table.
        // PresenceRows 在任何 SQL 引用可选 FTS 表之前检查 sqlite_master。
        let presence_rows: Vec<IndexTablePresenceRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT COUNT(*) AS index_exists FROM sqlite_master WHERE type = 'table' AND name = ?;",
                vec![text(KNOWLEDGE_FTS_INDEX)],
            )
            .await?;
        // IndexExists is true only for one exact sqlite_master match.
        // IndexExists 仅在 sqlite_master 精确匹配一条记录时为真。
        let index_exists = presence_rows
            .into_iter()
            .next()
            .ok_or_else(|| String::from("知识库索引结构检查未返回结果。"))?
            .index_exists
            == 1;
        // Counts uses exact source-field comparison when the FTS table exists.
        // Counts 在 FTS 表存在时使用精确来源字段比较。
        let counts = if index_exists {
            self.load_index_integrity_counts(space_id, binding_id)
                .await?
        } else {
            // AuthoritativeCount remains observable even when a cancelled rebuild dropped the FTS table.
            // AuthoritativeCount 即使取消的重建已删除 FTS 表也仍可观察。
            let authoritative_count = self
                .authoritative_counts(space_id, binding_id)
                .await?
                .chunk_count;
            IndexIntegrityCountsRow {
                authoritative_count,
                indexed_count: 0,
                missing_or_mismatched_count: authoritative_count,
                orphan_or_mismatched_count: 0,
            }
        };
        // ContentRevision is validated independently from physical row diagnostics.
        // ContentRevision 独立于物理记录诊断完成校验。
        let content_revision = parse_revision(&gate.content_revision, "content_revision")?;
        // IndexedContentRevision records the authoritative revision represented by FTS.
        // IndexedContentRevision 记录 FTS 表示的权威版本。
        let indexed_content_revision =
            parse_revision(&gate.indexed_content_revision, "indexed_content_revision")?;
        // DictionaryRevision is the current library-local Jieba dictionary version.
        // DictionaryRevision 是当前知识库局部 Jieba 词典版本。
        let dictionary_revision = parse_revision(&gate.dictionary_revision, "dictionary_revision")?;
        // IndexedDictionaryRevision is the dictionary version used to tokenize existing rows.
        // IndexedDictionaryRevision 是现有记录分词时使用的词典版本。
        let indexed_dictionary_revision = parse_revision(
            &gate.indexed_dictionary_revision,
            "indexed_dictionary_revision",
        )?;
        // AuthoritativeChunkCount is checked before public unsigned projection.
        // AuthoritativeChunkCount 在投影为公共无符号值前完成检查。
        let authoritative_chunk_count =
            nonnegative_u64(counts.authoritative_count, "authoritative_count")?;
        // IndexedChunkCount is the current physical FTS row count.
        // IndexedChunkCount 是当前物理 FTS 记录数量。
        let indexed_chunk_count = nonnegative_u64(counts.indexed_count, "indexed_count")?;
        // MissingCount includes both absent rows and rows whose source fields differ.
        // MissingCount 同时包含缺失记录与来源字段不同的记录。
        let missing_or_mismatched_count = nonnegative_u64(
            counts.missing_or_mismatched_count,
            "missing_or_mismatched_count",
        )?;
        // OrphanCount includes both orphan rows and rows whose source fields differ.
        // OrphanCount 同时包含孤立记录与来源字段不同的记录。
        let orphan_or_mismatched_count = nonnegative_u64(
            counts.orphan_or_mismatched_count,
            "orphan_or_mismatched_count",
        )?;
        // Searchable combines fail-closed metadata and exact physical set equality.
        // Searchable 组合失败关闭元数据与精确物理集合相等条件。
        let searchable = index_exists
            && gate.index_status == "ready"
            && content_revision == indexed_content_revision
            && dictionary_revision == indexed_dictionary_revision
            && gate.tokenizer == "jieba"
            && authoritative_chunk_count == indexed_chunk_count
            && missing_or_mismatched_count == 0
            && orphan_or_mismatched_count == 0;
        Ok(KnowledgeIndexIntegritySummary {
            library_id: library_id.to_string(),
            index_exists,
            searchable,
            index_status: gate.index_status,
            tokenizer: gate.tokenizer,
            content_revision,
            indexed_content_revision,
            dictionary_revision,
            indexed_dictionary_revision,
            authoritative_chunk_count,
            indexed_chunk_count,
            missing_or_mismatched_count,
            orphan_or_mismatched_count,
        })
    }

    /// Loads the exact persisted metadata gate from one isolated library.
    /// 从一个隔离知识库加载精确持久化元数据门禁。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact persisted index gate, or a metadata read error.
    /// 返回精确持久化索引门禁，或元数据读取错误。
    async fn load_index_gate(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<IndexGateRow, String> {
        // Rows is a single aggregate record over all required metadata keys.
        // Rows 是覆盖全部必需元数据键的单条聚合记录。
        let rows: Vec<IndexGateRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                r#"SELECT
                    MAX(CASE WHEN key = 'index_status' THEN value END) AS index_status,
                    MAX(CASE WHEN key = 'content_revision' THEN value END) AS content_revision,
                    MAX(CASE WHEN key = 'indexed_content_revision' THEN value END) AS indexed_content_revision,
                    MAX(CASE WHEN key = 'dictionary_revision' THEN value END) AS dictionary_revision,
                    MAX(CASE WHEN key = 'indexed_dictionary_revision' THEN value END) AS indexed_dictionary_revision,
                    MAX(CASE WHEN key = 'tokenizer' THEN value END) AS tokenizer
                FROM knowledge_metadata;"#,
                Vec::new(),
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| String::from("知识库索引元数据缺失。"))
    }

    /// Verifies all index revisions and the tokenizer before allowing search.
    /// 在允许搜索前验证全部索引版本与分词器。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    pub(super) async fn verify_index_gate(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<(), String> {
        // Gate is the lightweight hot-path metadata snapshot.
        // Gate 是轻量搜索热路径元数据快照。
        let gate = self.load_index_gate(space_id, binding_id).await?;
        if gate.index_status != "ready"
            || gate.content_revision != gate.indexed_content_revision
            || gate.dictionary_revision != gate.indexed_dictionary_revision
            || gate.tokenizer != "jieba"
        {
            return Err(format!(
                "The knowledge index is not searchable: status={}, content={}/{}, dictionary={}/{}, tokenizer={}",
                gate.index_status,
                gate.indexed_content_revision,
                gate.content_revision,
                gate.indexed_dictionary_revision,
                gate.dictionary_revision,
                gate.tokenizer
            ));
        }
        Ok(())
    }

    /// Verifies that ready metadata is backed by one exact FTS row for every authoritative chunk.
    /// 验证 ready 元数据确实由每个权威切片对应的一条精确 FTS 记录支撑。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the exact isolated storage space containing the candidate database.
    /// `space_id` 是包含候选数据库的精确隔离存储空间。
    /// `binding_id` is the exact SQLite binding used for the integrity query.
    /// `binding_id` 是执行完整性查询的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only when metadata, row counts, identifiers, and indexed source fields all match.
    /// 仅当元数据、行数、标识符和索引源字段全部一致时返回成功。
    pub(super) async fn verify_index_integrity(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<(), String> {
        self.verify_index_gate(space_id, binding_id).await?;
        self.verify_index_rows_exact(space_id, binding_id).await
    }

    /// Verifies exact physical FTS rows without requiring ready metadata first.
    /// 在不预先要求 ready 元数据的情况下验证精确物理 FTS 记录。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    async fn verify_index_rows_exact(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<(), String> {
        // Row contains exact set and source-field diagnostics for the existing FTS table.
        // Row 包含现有 FTS 表的精确集合与来源字段诊断。
        let row = self
            .load_index_integrity_counts(space_id, binding_id)
            .await?;
        if row.authoritative_count < 0
            || row.indexed_count < 0
            || row.authoritative_count != row.indexed_count
            || row.missing_or_mismatched_count != 0
            || row.orphan_or_mismatched_count != 0
        {
            return Err(format!(
                "知识库 FTS 与权威切片不一致：authoritative={}, indexed={}, missing_or_mismatched={}, orphan_or_mismatched={}",
                row.authoritative_count,
                row.indexed_count,
                row.missing_or_mismatched_count,
                row.orphan_or_mismatched_count
            ));
        }
        Ok(())
    }

    /// Loads exact physical FTS set and source-field diagnostics for an existing table.
    /// 为一个已存在表加载精确物理 FTS 集合与来源字段诊断。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact physical set/count diagnostic, or a query error.
    /// 返回精确物理集合/计数诊断，或查询错误。
    async fn load_index_integrity_counts(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<IndexIntegrityCountsRow, String> {
        // Rows combines EXCEPT and INTERSECT set comparisons so duplicate physical rows remain observable.
        // Rows 组合 EXCEPT 与 INTERSECT 集合比较，使重复物理记录仍然可被观察。
        let rows: Vec<IndexIntegrityCountsRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                &format!(
                    r#"SELECT
                        (SELECT COUNT(*) FROM knowledge_chunks) AS authoritative_count,
                        (SELECT COUNT(*) FROM {KNOWLEDGE_FTS_INDEX}) AS indexed_count,
                        (SELECT COUNT(*) FROM (
                            SELECT c.id,
                                   d.relative_path AS file_path,
                                   CASE WHEN c.heading_path = ''
                                        THEN d.title
                                        ELSE d.title || ' / ' || c.heading_path END AS title,
                                   c.content
                              FROM knowledge_chunks c
                              INNER JOIN knowledge_documents d ON d.id = c.document_id
                            EXCEPT
                            SELECT id, file_path, title, content FROM {KNOWLEDGE_FTS_INDEX}
                        )) AS missing_or_mismatched_count,
                        (SELECT COUNT(*) FROM {KNOWLEDGE_FTS_INDEX}) -
                        (SELECT COUNT(*) FROM (
                            SELECT id, file_path, title, content FROM {KNOWLEDGE_FTS_INDEX}
                            INTERSECT
                            SELECT c.id,
                                   d.relative_path AS file_path,
                                   CASE WHEN c.heading_path = ''
                                        THEN d.title
                                        ELSE d.title || ' / ' || c.heading_path END AS title,
                                   c.content
                              FROM knowledge_chunks c
                              INNER JOIN knowledge_documents d ON d.id = c.document_id
                        )) AS orphan_or_mismatched_count;"#,
                ),
                Vec::new(),
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| String::from("The knowledge FTS integrity check returned no result."))
    }

    /// Marks FTS revisions equal to authoritative revisions after successful replay.
    /// 在成功回放后将 FTS 版本标记为等同权威版本。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    async fn mark_index_ready(&self, _space_id: &str, binding_id: &str) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
                r#"UPDATE knowledge_metadata SET value = (SELECT value FROM knowledge_metadata WHERE key = 'content_revision') WHERE key = 'indexed_content_revision';
                    UPDATE knowledge_metadata SET value = (SELECT value FROM knowledge_metadata WHERE key = 'dictionary_revision') WHERE key = 'indexed_dictionary_revision';
                    UPDATE knowledge_metadata SET value = 'jieba' WHERE key = 'tokenizer';
                    UPDATE knowledge_metadata SET value = 'ready' WHERE key = 'index_status';
                    UPDATE knowledge_metadata SET value = '' WHERE key = 'last_index_error';"#,
                Vec::new(),
            ).await
            .map_err(|error| format!("提交知识库索引版本失败：{error}"))?;
        Ok(())
    }

    /// Increments the dictionary revision and makes search fail closed until rebuild succeeds.
    /// 递增词典版本，并在重建成功前让检索保持关闭。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    async fn mark_dictionary_dirty(
        &self,
        library_id: &str,
        _space_id: &str,
        binding_id: &str,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
                "UPDATE knowledge_metadata SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) WHERE key = 'dictionary_revision'; UPDATE knowledge_metadata SET value = 'dirty' WHERE key = 'index_status';",
                Vec::new(),
            ).await
            .map_err(|error| format!("更新知识库词典版本失败：{error}"))?;
        self.set_registry_index_status(library_id, "dirty").await
    }

    /// Counts committed chunks whose document identities are present in the current staging set.
    /// 统计文档身份存在于当前暂存集合中的已提交切片。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns the checked revision or authoritative work count described by this operation, or an error.
    /// 返回当前操作所述的已校验修订或权威工作量，或错误。
    async fn staged_chunk_count(&self, space_id: &str, binding_id: &str) -> Result<u64, String> {
        // Rows performs one aggregate over the authoritative/staging identity join.
        // Rows 对权威记录与暂存身份联结执行一次聚合。
        let rows: Vec<ChunkCountRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                r#"SELECT COUNT(c.id) AS chunk_count
                     FROM knowledge_chunks c
                     INNER JOIN knowledge_staged_documents s ON s.id = c.document_id;"#,
                Vec::new(),
            )
            .await?;
        // Row is mandatory because SQLite aggregate queries always return one record.
        // Row 是必需值，因为 SQLite 聚合查询始终返回一条记录。
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| String::from("读取知识库变更切片计数失败。"))?;
        nonnegative_u64(row.chunk_count, "chunk_count")
    }

    /// Sets matching per-library and central index states with an explicit error.
    /// 使用显式错误设置一致的单库与中央索引状态。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `space_id` is the already resolved isolated storage space.
    /// `space_id` 是已解析的隔离存储空间。
    /// `binding_id` is the exact SQLite binding containing authoritative and derived rows.
    /// `binding_id` 是包含权威与派生记录的精确 SQLite 绑定。
    /// `state` is the exact index lifecycle state to persist.
    /// `state` 是要持久化的精确索引生命周期状态。
    /// `error` is the explicit index diagnostic, empty when no error is being recorded.
    /// `error` 是明确的索引诊断；没有记录错误时为空。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    async fn set_library_index_state(
        &self,
        library_id: &str,
        _space_id: &str,
        binding_id: &str,
        state: &str,
        error: &str,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
                "UPDATE knowledge_metadata SET value = CASE key WHEN 'index_status' THEN ? WHEN 'last_index_error' THEN ? END WHERE key IN ('index_status', 'last_index_error');",
                vec![text(state), text(error)],
            ).await
            .map_err(|failure| format!("更新知识库索引状态失败：{failure}"))?;
        self.set_registry_index_status(library_id, state).await
    }

    /// Updates only the central registry index state.
    /// 仅更新中央注册表索引状态。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `state` is the exact index lifecycle state to persist.
    /// `state` 是要持久化的精确索引生命周期状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    async fn set_registry_index_status(&self, library_id: &str, state: &str) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
            "UPDATE knowledge_libraries SET index_status = ?, updated_at_ms = ? WHERE id = ?;",
            vec![text(state), int(unix_epoch_millis()?), text(library_id)],
        )
        .await
        .map_err(|error| format!("更新知识库中央索引状态失败：{error}"))?;
        Ok(())
    }
}

/// Validates a nonempty bounded custom Jieba word after trimming.
/// 去除首尾空白后校验非空且有界的自定义 Jieba 词。
///
/// # Parameters
/// 参数
/// `word` is the custom Jieba text validated after trimming.
/// `word` 是去除首尾空白后校验的自定义 Jieba 文本。
///
/// # Returns
/// 返回值
/// Returns the nonempty bounded trimmed word, or a validation error.
/// 返回非空且有界的去除首尾空白后的词，或校验错误。
fn validate_custom_word(word: &str) -> Result<&str, String> {
    let word = word.trim();
    if word.is_empty() {
        return Err(String::from("Jieba 自定义词不能为空。"));
    }
    if word.chars().count() > 128 {
        return Err(String::from("Jieba 自定义词不能超过 128 个字符。"));
    }
    Ok(word)
}

/// Parses one non-negative decimal metadata revision into its public unsigned representation.
/// 将一个非负十进制元数据版本解析为公共无符号表示。
///
/// # Parameters
/// 参数
/// `value` is the stored decimal revision text.
/// `value` 是已存十进制修订文本。
/// `field` is the diagnostic name identifying the revision.
/// `field` 是标识修订的诊断字段名。
///
/// # Returns
/// 返回值
/// Returns the parsed unsigned revision, or an explicit metadata-integrity error.
/// 返回解析后的无符号修订，或明确元数据完整性错误。
fn parse_revision(value: &str, field: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("知识库元数据 {field} 不是有效非负版本（{value}）：{error}"))
}
