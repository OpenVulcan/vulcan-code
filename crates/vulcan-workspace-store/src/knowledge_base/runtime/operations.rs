//! Shared knowledge-operation reservations, cancellation, durable progress, and terminal transitions.
//! 共享知识操作预留、取消、持久进度与终态迁移。

use super::super::models::{
    KnowledgeImportJobSummary, KnowledgeImportReport, KnowledgeOperationKind,
    KnowledgeOperationObservability,
};
use super::{
    KnowledgeBaseRuntime, MAX_WARNING_CHARS, nonnegative_u64, truncate_chars, unix_epoch_millis,
    validate_library_id,
};
use super::{int, text};
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use crate::runtime_assets;
use serde::Deserialize;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// Exact internal marker used to distinguish cancellation from failures.
/// 用于区分取消与失败的精确内部标记。
pub(super) const KNOWLEDGE_OPERATION_CANCELLED_ERROR: &str =
    "knowledge operation cancelled by user";

/// Cancellation identity and signal for one exact active knowledge operation.
/// 一个精确活动资料操作的取消身份与信号。
#[derive(Clone)]
pub(super) struct ActiveKnowledgeOperation {
    /// Immutable operation job identifier used to reject stale cancellation requests.
    /// 用于拒绝过期取消请求的不可变操作任务标识符。
    job_id: String,
    /// Cooperative cancellation signal observed by bounded import checkpoints.
    /// 由有界导入检查点观察的协作取消信号。
    signal: Arc<AtomicBool>,
}

/// Runtime-owned identity shared by one durable operation row and its detached executor.
/// 由运行时持有、供一条持久化操作记录与其分离执行器共享的身份。
#[derive(Clone)]
pub(super) struct PreparedKnowledgeOperation {
    /// Immutable operation job identifier.
    /// 不可变操作任务标识符。
    pub(super) job_id: String,
    /// Immutable target library identifier.
    /// 不可变目标知识库标识符。
    pub(super) library_id: String,
    /// Stable operation kind persisted before execution starts.
    /// 执行开始前持久化的稳定操作类型。
    pub(super) operation_kind: KnowledgeOperationKind,
    /// Cooperative cancellation signal shared with the detached executor.
    /// 与分离执行器共享的协作取消信号。
    pub(super) cancellation: Arc<AtomicBool>,
    /// Creation time reused by import source and authoritative row commits.
    /// 由导入来源与权威记录提交复用的创建时间。
    pub(super) created_at_ms: i64,
}

/// Typed central import and derived-operation job row.
/// 强类型中央导入与派生操作任务记录。
#[derive(Debug, Deserialize)]
struct ImportJobRow {
    /// Durable job identifier.
    /// 持久化任务标识符。
    id: String,
    /// Target library identifier.
    /// 目标知识库标识符。
    library_id: String,
    /// Stable persisted import or index-maintenance operation kind.
    /// 稳定持久化的导入或索引维护操作类型。
    operation_kind: String,
    /// Durable state.
    /// 持久化状态。
    state: String,
    /// Current phase.
    /// 当前阶段。
    phase: String,
    /// Discovered file count.
    /// 已发现文件数。
    scanned_files: i64,
    /// Total file count after discovery.
    /// 发现完成后的文件总数。
    total_files: i64,
    /// Files whose parsing work has completed.
    /// 已完成解析工作的文件数。
    processed_files: i64,
    /// Total index work items in the active index phase.
    /// 当前索引阶段的索引工作项总数。
    total_index_items: i64,
    /// Completed index work items in the active index phase.
    /// 当前索引阶段已完成的索引工作项数。
    processed_index_items: i64,
    /// Current relative path reported by the worker.
    /// 工作器报告的当前相对路径。
    current_file: Option<String>,
    /// Processed source bytes.
    /// 已处理来源字节数。
    processed_bytes: i64,
    /// JSON warnings payload.
    /// JSON 告警载荷。
    warnings_json: String,
    /// JSON request and usage observability payload.
    /// JSON 请求与用量可观测载荷。
    observability_json: String,
    /// Optional terminal error.
    /// 可选终态错误。
    error_message: Option<String>,
}

/// Signals cancellation only when the requested job is still the exact active operation.
/// 仅当请求任务仍是精确活动操作时发送取消信号。
fn signal_matching_operation_cancellation(
    active: Option<&ActiveKnowledgeOperation>,
    requested_job_id: &str,
) -> bool {
    let Some(active) = active else {
        return false;
    };
    if active.job_id != requested_job_id {
        return false;
    }
    active.signal.store(true, Ordering::Release);
    true
}

/// Builds the exact queued browser snapshot returned before detached execution begins.
/// 构建分离执行开始前返回的精确排队浏览器快照。
///
/// Parameters: `operation` is the immutable prepared operation identity.
/// Returns: a zero-progress queued snapshot with the same durable identifiers.
/// 参数：`operation` 是不可变预留操作身份。返回值：具有相同持久身份的零进度排队快照。
pub(super) fn queued_operation_summary(
    operation: &PreparedKnowledgeOperation,
) -> KnowledgeImportJobSummary {
    KnowledgeImportJobSummary {
        id: operation.job_id.clone(),
        library_id: operation.library_id.clone(),
        operation_kind: operation.operation_kind,
        state: String::from("queued"),
        phase: String::from("queued"),
        scanned_files: 0,
        total_files: 0,
        processed_files: 0,
        total_index_items: 0,
        processed_index_items: 0,
        current_file: None,
        processed_bytes: 0,
        warnings: Vec::new(),
        error_message: None,
        observability: Default::default(),
    }
}

/// Rejects continued import work after the active job receives cancellation.
/// 在活动任务收到取消请求后拒绝继续执行导入工作。
pub(super) fn require_operation_not_cancelled(cancellation: &AtomicBool) -> Result<(), String> {
    if cancellation.load(Ordering::Acquire) {
        Err(String::from(KNOWLEDGE_OPERATION_CANCELLED_ERROR))
    } else {
        Ok(())
    }
}

impl KnowledgeBaseRuntime {
    /// Requests cancellation only when both the library and active operation identities match.
    /// 仅在知识库与活动操作身份都匹配时请求取消。
    ///
    /// Parameters: `library_id` selects the exact library and `job_id` rejects stale requests.
    /// Returns: whether the matching live operation accepted the cancellation signal.
    /// 参数：`library_id` 选择精确知识库，`job_id` 用于拒绝过期请求。返回值：匹配活动操作是否接受取消信号。
    pub async fn cancel_operation(&self, library_id: &str, job_id: &str) -> Result<bool, String> {
        validate_library_id(library_id)?;
        Uuid::parse_str(job_id).map_err(|error| format!("知识库操作任务标识符无效：{error}"))?;
        // Operations contains the only runtime-owned cancellation identity for each library.
        // Operations 包含每个知识库唯一由运行时持有的取消身份。
        let operations = self.active_operations.lock().await;
        Ok(signal_matching_operation_cancellation(
            operations.get(library_id),
            job_id,
        ))
    }

    /// Returns the latest durable import job for one library.
    /// 返回一个知识库最近的持久化导入任务。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library whose latest durable operation is requested.
    /// `library_id` 是待查询最新持久操作的精确知识库。
    ///
    /// # Returns
    /// 返回值
    /// The latest operation of any supported kind, no record, or an explicit query/decoding failure.
    /// 返回任一受支持类型的最新操作、无记录，或明确查询/解码失败。
    pub async fn latest_import_job(
        &self,
        library_id: &str,
    ) -> Result<Option<KnowledgeImportJobSummary>, String> {
        validate_library_id(library_id)?;
        let rows: Vec<ImportJobRow> = self
            .query_app_rows(
                "SELECT id, library_id, operation_kind, state, phase, scanned_files, total_files, processed_files, total_index_items, processed_index_items, current_file, processed_bytes, warnings_json, observability_json, error_message FROM knowledge_import_jobs WHERE library_id = ? ORDER BY created_at_ms DESC LIMIT 1;",
                &[text(library_id)],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(|row| {
                Ok(KnowledgeImportJobSummary {
                    id: row.id,
                    library_id: row.library_id,
                    operation_kind: KnowledgeOperationKind::parse(&row.operation_kind)?,
                    state: row.state,
                    phase: row.phase,
                    scanned_files: nonnegative_u64(row.scanned_files, "scanned_files")?,
                    total_files: nonnegative_u64(row.total_files, "total_files")?,
                    processed_files: nonnegative_u64(row.processed_files, "processed_files")?,
                    total_index_items: nonnegative_u64(row.total_index_items, "total_index_items")?,
                    processed_index_items: nonnegative_u64(
                        row.processed_index_items,
                        "processed_index_items",
                    )?,
                    current_file: row.current_file,
                    processed_bytes: nonnegative_u64(row.processed_bytes, "processed_bytes")?,
                    warnings: serde_json::from_str(&row.warnings_json)
                        .map_err(|error| format!("解析知识库导入告警失败：{error}"))?,
                    error_message: row.error_message,
                    observability: serde_json::from_str::<KnowledgeOperationObservability>(
                        &row.observability_json,
                    )
                    .map_err(|error| format!("解析知识库操作可观测状态失败：{error}"))?,
                })
            })
            .transpose()
    }

    /// Reserves one service-owned derivation operation and returns its durable running snapshot.
    /// 预留一个由服务层执行的派生操作，并返回持久化运行快照。
    ///
    /// Parameters: `library_id` selects the library, `operation_kind` must be a derivation kind,
    /// and `total_items` is the known document or chunk total used by the progress surface.
    /// Returns: the exact running operation snapshot consumed by the detached service worker.
    /// 参数：`library_id` 选择知识库，`operation_kind` 必须是派生类型，`total_items` 是进度界面使用的
    /// 已知文档或切片总数。返回值：由分离服务执行器消费的精确运行操作快照。
    pub async fn start_external_operation(
        &self,
        library_id: &str,
        operation_kind: KnowledgeOperationKind,
        total_items: u64,
    ) -> Result<KnowledgeImportJobSummary, String> {
        if !matches!(
            operation_kind,
            KnowledgeOperationKind::DisplayBuild | KnowledgeOperationKind::VectorBuild
        ) {
            return Err(String::from(
                "外部资料库操作必须使用 display_build 或 vector_build 类型。",
            ));
        }
        self.require_active_library(library_id).await?;
        let operation = self
            .prepare_operation(library_id, operation_kind, None)
            .await?;
        let phase = match operation_kind {
            KnowledgeOperationKind::DisplayBuild => "curating",
            KnowledgeOperationKind::VectorBuild => "embedding",
            _ => unreachable!("validated external knowledge operation kind"),
        };
        if let Err(error) = self
            .update_operation_phase(&operation.job_id, "running", phase)
            .await
        {
            let _ = self
                .finish_failed_operation_job(&operation.job_id, &error)
                .await;
            self.release_active_operation(library_id, &operation.job_id)
                .await;
            return Err(error);
        }
        if let Err(error) = self
            .update_import_index_progress(&operation.job_id, 0, total_items, None)
            .await
        {
            let _ = self
                .finish_failed_operation_job(&operation.job_id, &error)
                .await;
            self.release_active_operation(library_id, &operation.job_id)
                .await;
            return Err(error);
        }
        Ok(KnowledgeImportJobSummary {
            id: operation.job_id,
            library_id: operation.library_id,
            operation_kind,
            state: String::from("running"),
            phase: phase.to_string(),
            scanned_files: 0,
            total_files: 0,
            processed_files: 0,
            total_index_items: total_items,
            processed_index_items: 0,
            current_file: None,
            processed_bytes: 0,
            warnings: Vec::new(),
            error_message: None,
            observability: Default::default(),
        })
    }

    /// Reports one durable progress boundary for a service-owned derivation operation.
    /// 为由服务层执行的派生操作报告一个持久化进度边界。
    ///
    /// Parameters: `job_id` identifies the exact operation; `phase`, counters, and `current_file`
    /// are persisted atomically enough for refresh polling and cancellation checkpoints.
    /// Returns: success after the progress row is updated, or an explicit stale-operation error.
    /// 参数：`job_id` 标识精确操作；`phase`、计数器和 `current_file` 会持久化，供刷新轮询与取消检查点使用。
    /// 返回值：进度行更新成功，或明确的过期操作错误。
    pub async fn update_external_operation_progress(
        &self,
        job_id: &str,
        phase: &str,
        processed_items: u64,
        total_items: u64,
        current_file: Option<&str>,
    ) -> Result<(), String> {
        if phase.trim().is_empty() {
            return Err(String::from("资料库派生操作阶段不能为空。"));
        }
        let active = self.active_operation_for_job(job_id).await?;
        if !active {
            return Err(format!("资料库派生操作已不再活动：job_id={job_id}"));
        }
        self.update_operation_phase(job_id, "running", phase)
            .await?;
        self.update_import_index_progress(job_id, processed_items, total_items, current_file)
            .await
    }

    /// Persists one bounded live request and usage snapshot for refresh-safe progress rendering.
    /// 持久化一份有界实时请求与用量快照，供刷新后安全恢复进度展示。
    ///
    /// Parameters: `job_id` selects the active operation and `observability` is the sanitized snapshot.
    /// Returns: success after the snapshot is committed, or an explicit stale-operation error.
    /// 参数：`job_id` 选择活动操作，`observability` 是已脱敏快照。返回值：快照提交成功，或明确的过期操作错误。
    pub async fn update_operation_observability(
        &self,
        job_id: &str,
        observability: &KnowledgeOperationObservability,
    ) -> Result<(), String> {
        if !self.active_operation_for_job(job_id).await? {
            return Err(format!("资料库派生操作已不再活动：job_id={job_id}"));
        }
        let observability_json = serde_json::to_string(observability)
            .map_err(|error| format!("序列化知识库操作可观测状态失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET observability_json = ? WHERE id = ? AND state = 'running';",
                vec![text(observability_json), text(job_id)],
            ).await
            .map_err(|error| format!("更新知识库操作可观测状态失败：{error}"))?;
        Ok(())
    }

    /// Checks whether one library owns any active import or derived-index operation.
    /// 检查一个资料库是否拥有任一活动导入或派生索引操作。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` selects the shared per-library reservation.
    /// `library_id` 选择共享的每库预留。
    ///
    /// # Returns
    /// 返回值
    /// Whether that library currently owns a live reservation, or an invalid identity error.
    /// 返回该库当前是否持有活动预留，或身份无效错误。
    pub async fn operation_is_active_for_library(&self, library_id: &str) -> Result<bool, String> {
        validate_library_id(library_id)?;
        let operations = self.active_operations.lock().await;
        Ok(operations.contains_key(library_id))
    }

    /// Returns whether one exact service-owned operation has received cancellation.
    /// 返回一个精确的服务层操作是否已经收到取消请求。
    ///
    /// Parameters: `job_id` is the durable operation identity.
    /// Returns: `true` only when the active operation's cooperative signal is set.
    /// 参数：`job_id` 是持久化操作身份。返回值：仅当活动操作的协作信号已设置时为 `true`。
    pub async fn operation_is_cancelled(&self, job_id: &str) -> Result<bool, String> {
        Uuid::parse_str(job_id).map_err(|error| format!("知识库操作任务标识符无效：{error}"))?;
        let operations = self.active_operations.lock().await;
        Ok(operations
            .values()
            .find(|active| active.job_id == job_id)
            .is_some_and(|active| active.signal.load(Ordering::Acquire)))
    }

    /// Marks one service-owned operation completed and releases its exact runtime reservation.
    /// 将一个由服务层执行的操作标记为完成，并释放精确运行时预留。
    ///
    /// Parameters: `job_id` identifies the operation and `processed_items` is its verified total.
    /// Returns: success after the terminal row and in-memory reservation converge.
    /// 参数：`job_id` 标识操作，`processed_items` 是经过验证的完成总数。返回值：终态行与内存预留收敛后成功。
    pub async fn finish_external_operation(
        &self,
        job_id: &str,
        processed_items: u64,
    ) -> Result<(), String> {
        self.finish_index_operation_job(job_id, processed_items)
            .await?;
        self.release_operation_by_job(job_id).await;
        Ok(())
    }

    /// Marks one service-owned operation failed and releases its exact runtime reservation.
    /// 将一个由服务层执行的操作标记为失败，并释放精确运行时预留。
    ///
    /// Parameters: `job_id` identifies the operation and `error` is the bounded terminal detail.
    /// Returns: success after the failed row and in-memory reservation converge.
    /// 参数：`job_id` 标识操作，`error` 是有界终态错误详情。返回值：失败行与内存预留收敛后成功。
    pub async fn fail_external_operation(&self, job_id: &str, error: &str) -> Result<(), String> {
        self.finish_failed_operation_job(job_id, error).await?;
        self.release_operation_by_job(job_id).await;
        Ok(())
    }

    /// Marks one service-owned operation cancelled and releases its exact runtime reservation.
    /// 将一个由服务层执行的操作标记为取消，并释放精确运行时预留。
    ///
    /// Parameters: `job_id` identifies the operation.
    /// Returns: success after the cancelled row and in-memory reservation converge.
    /// 参数：`job_id` 标识操作。返回值：取消行与内存预留收敛后成功。
    pub async fn finish_cancelled_external_operation(&self, job_id: &str) -> Result<(), String> {
        self.finish_cancelled_operation_job(job_id).await?;
        self.release_operation_by_job(job_id).await;
        Ok(())
    }

    /// Reserves one library operation in memory and persists its queued durable row.
    /// 在内存中预留一个知识库操作，并持久化其排队记录。
    ///
    /// Parameters: `library_id` selects the exact library, `operation_kind` defines execution
    /// semantics, and `source_root` is present only for Markdown imports.
    /// Returns: the prepared identity consumed by either a synchronous or detached executor.
    /// 参数：`library_id` 选择精确知识库，`operation_kind` 定义执行语义，`source_root` 仅在 Markdown
    /// 导入时存在。返回值：由同步或分离执行器消费的预留身份。
    pub(super) async fn prepare_operation(
        &self,
        library_id: &str,
        operation_kind: KnowledgeOperationKind,
        source_root: Option<&Path>,
    ) -> Result<PreparedKnowledgeOperation, String> {
        validate_library_id(library_id)?;
        // JobId is generated before reservation so every diagnostic names the exact contender.
        // JobId 在预留前生成，使每条诊断都能命名精确竞争任务。
        let job_id = Uuid::new_v4().to_string();
        // Cancellation is shared only by this operation and its executor.
        // Cancellation 仅由本操作及其执行器共享。
        let cancellation = Arc::new(AtomicBool::new(false));
        // Operation captures every immutable value required after request completion.
        // Operation 捕获请求完成后所需的全部不可变值。
        let operation = PreparedKnowledgeOperation {
            job_id: job_id.clone(),
            library_id: library_id.to_string(),
            operation_kind,
            cancellation: Arc::clone(&cancellation),
            created_at_ms: unix_epoch_millis()?,
        };
        {
            // Operations closes the duplicate-start race before the durable insert awaits I/O.
            // Operations 在持久化插入等待 I/O 前关闭重复启动竞态。
            let mut operations = self.active_operations.lock().await;
            if let Some(active) = operations.get(library_id) {
                return Err(format!("知识库已有运行中的操作：job_id={}", active.job_id));
            }
            operations.insert(
                library_id.to_string(),
                ActiveKnowledgeOperation {
                    job_id,
                    signal: cancellation,
                },
            );
        }
        if let Err(error) = self.insert_operation_job(&operation, source_root).await {
            self.release_active_operation(library_id, &operation.job_id)
                .await;
            return Err(error);
        }
        Ok(operation)
    }

    /// Inserts one queued durable operation row using a stable kind and optional import source.
    /// 使用稳定类型和可选导入来源写入一条排队中的持久化操作记录。
    ///
    /// Parameters: `operation` owns immutable identity and timestamps; `source_root` exists only
    /// for an import. Returns: success after the central transaction commits.
    /// 参数：`operation` 持有不可变身份与时间戳；`source_root` 仅在导入时存在。返回值：中央事务提交后成功。
    async fn insert_operation_job(
        &self,
        operation: &PreparedKnowledgeOperation,
        source_root: Option<&Path>,
    ) -> Result<(), String> {
        // SourceRootText is empty for index-only operations and never doubles as a type marker.
        // SourceRootText 在仅索引操作中为空，且绝不兼作类型标记。
        let source_root_text = source_root
            .map(runtime_assets::native_host_path_text_from_path)
            .unwrap_or_default();
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
            r#"INSERT INTO knowledge_import_jobs(
                    id, library_id, source_root, operation_kind, state, phase, created_at_ms
                ) VALUES (?, ?, ?, ?, 'queued', 'queued', ?);"#,
            vec![
                text(&operation.job_id),
                text(&operation.library_id),
                text(source_root_text),
                text(operation.operation_kind.as_str()),
                int(operation.created_at_ms),
            ],
        )
        .await
        .map_err(|error| format!("创建知识库操作任务失败：{error}"))?;
        Ok(())
    }

    /// Releases an in-memory reservation only when the exact job still owns it.
    /// 仅当精确任务仍持有预留时释放内存预留。
    ///
    /// Parameters: `library_id` selects the reservation and `job_id` prevents stale cleanup.
    /// Returns: nothing after the conditional removal completes.
    /// 参数：`library_id` 选择预留，`job_id` 防止过期清理。返回值：条件移除完成后无返回内容。
    pub(super) async fn release_active_operation(&self, library_id: &str, job_id: &str) {
        // Operations is held only for the exact identity comparison and removal.
        // Operations 仅在精确身份比较与移除期间持有。
        let mut operations = self.active_operations.lock().await;
        if operations
            .get(library_id)
            .is_some_and(|active| active.job_id == job_id)
        {
            operations.remove(library_id);
        }
    }

    /// Checks whether an exact job identity still owns any library reservation.
    /// 检查精确任务身份是否仍持有任一知识库预留。
    ///
    /// # Parameters
    /// 参数
    /// `job_id` is the exact durable operation UUID to locate in the shared reservation map.
    /// `job_id` 是要在共享预留映射中定位的精确持久操作 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Whether any current reservation owns that job, or an invalid UUID error.
    /// 返回当前是否有预留持有此任务，或 UUID 无效错误。
    async fn active_operation_for_job(&self, job_id: &str) -> Result<bool, String> {
        Uuid::parse_str(job_id).map_err(|error| format!("知识库操作任务标识符无效：{error}"))?;
        let operations = self.active_operations.lock().await;
        Ok(operations.values().any(|active| active.job_id == job_id))
    }

    /// Releases the reservation owned by one exact job identity.
    /// 释放一个精确任务身份持有的预留。
    ///
    /// # Parameters
    /// 参数
    /// `job_id` identifies the sole reservation eligible for release.
    /// `job_id` 标识唯一允许释放的预留。
    ///
    /// # Returns
    /// 返回值
    /// Nothing; a missing or stale job leaves every other reservation unchanged.
    /// 无返回内容；不存在或过期任务不会改变其他预留。
    async fn release_operation_by_job(&self, job_id: &str) {
        let mut operations = self.active_operations.lock().await;
        let library_id = operations
            .iter()
            .find(|(_, active)| active.job_id == job_id)
            .map(|(library_id, _)| library_id.clone());
        if let Some(library_id) = library_id {
            operations.remove(&library_id);
        }
    }

    /// Persists one discovery snapshot before the final Markdown total is known.
    /// 在 Markdown 最终总数尚未确定时持久化一份发现快照。
    ///
    /// # Parameters
    /// 参数
    /// `job_id` identifies the running job; `discovered_files` and `current_path` report the current scan boundary.
    /// `job_id` 标识运行任务；`discovered_files` 与 `current_path` 报告当前扫描边界。
    ///
    /// # Returns
    /// 返回值
    /// Success after the running-row update, or an engine persistence failure.
    /// 更新运行记录后成功，或返回引擎持久化失败。
    pub(super) async fn update_import_scan_progress(
        &self,
        job_id: &str,
        discovered_files: u64,
        current_path: &str,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET scanned_files = ?, current_file = ? WHERE id = ? AND state = 'running';",
                vec![int(discovered_files as i64), text(current_path), text(job_id)],
            ).await
            .map_err(|error| format!("更新知识库扫描进度失败：{error}"))?;
        Ok(())
    }

    /// Publishes the exact total and transitions a completed scan into file processing.
    /// 发布准确总数，并将已完成扫描转入文件处理阶段。
    ///
    /// # Parameters
    /// 参数
    /// `job_id` identifies the completed scan and `total_files` is its exact candidate total.
    /// `job_id` 标识完成的扫描，`total_files` 是精确候选总数。
    ///
    /// # Returns
    /// 返回值
    /// Success after publishing totals and parsing state, or a persistence failure.
    /// 发布总量与解析状态后成功，或返回持久化失败。
    pub(super) async fn prepare_import_processing(
        &self,
        job_id: &str,
        total_files: u64,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET state = 'running', phase = 'parsing', scanned_files = ?, total_files = ?, processed_files = 0, current_file = NULL WHERE id = ?;",
                vec![int(total_files as i64), int(total_files as i64), text(job_id)],
            ).await
            .map_err(|error| format!("发布知识库文件总数失败：{error}"))?;
        Ok(())
    }

    /// Updates the durable state and phase of one import or index-maintenance operation.
    /// 更新一个导入或索引维护操作的持久化状态与阶段。
    ///
    /// # Parameters
    /// 参数
    /// `job_id`, `state` and `phase` identify the durable transition; the first running timestamp is retained.
    /// `job_id`、`state`、`phase` 标识持久化迁移；首次运行时间保持不变。
    ///
    /// # Returns
    /// 返回值
    /// Success after the phase write, or an explicit clock/storage failure.
    /// 写入阶段后成功，或返回明确时钟/存储错误。
    pub(super) async fn update_operation_phase(
        &self,
        job_id: &str,
        state: &str,
        phase: &str,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET state = ?, phase = ?, started_at_ms = CASE WHEN ? = 'running' THEN COALESCE(started_at_ms, ?) ELSE started_at_ms END WHERE id = ?;",
                vec![
                    text(state),
                    text(phase),
                    text(state),
                    int(unix_epoch_millis()?),
                    text(job_id),
                ],
            ).await
            .map_err(|error| format!("更新知识库操作阶段失败：{error}"))?;
        Ok(())
    }

    /// Persists bounded import counters without refreshing the full workbench snapshot.
    /// 持久化有界导入计数且不刷新完整工作台快照。
    ///
    /// # Parameters
    /// 参数
    /// `job_id` selects the running row; file/byte counters, optional current path and bounded warnings describe completed parsing work.
    /// `job_id` 选择运行记录；文件/字节计数、可选当前路径与有界告警描述已完成解析工作。
    ///
    /// # Returns
    /// 返回值
    /// Success after persisting the progress snapshot, or serialization/storage failure.
    /// 持久化进度快照后成功，或返回序列化/存储失败。
    pub(super) async fn update_import_progress(
        &self,
        job_id: &str,
        processed_files: u64,
        processed_bytes: u64,
        current_file: Option<&str>,
        warnings: &[String],
    ) -> Result<(), String> {
        let warnings_json = serde_json::to_string(warnings)
            .map_err(|error| format!("序列化知识库导入进度告警失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET processed_files = ?, processed_bytes = ?, current_file = ?, warnings_json = ? WHERE id = ? AND state = 'running';",
                vec![
                    int(processed_files as i64),
                    int(processed_bytes as i64),
                    current_file.map_or(SqliteValue::Null, text),
                    text(warnings_json),
                    text(job_id),
                ],
            ).await
            .map_err(|error| format!("更新知识库导入进度失败：{error}"))?;
        Ok(())
    }

    /// Persists one exact indexing batch boundary for UI progress and cancellation feedback.
    /// 为 UI 进度与取消反馈持久化一个准确的索引批次边界。
    ///
    /// # Parameters
    /// 参数
    /// `job_id` selects the running row; processed/total items and optional current file describe one index batch boundary.
    /// `job_id` 选择运行记录；已处理/总工作项和可选当前文件描述一次索引批次边界。
    ///
    /// # Returns
    /// 返回值
    /// Success after persisting counters, or a storage failure.
    /// 持久化计数后成功，或返回存储错误。
    pub(super) async fn update_import_index_progress(
        &self,
        job_id: &str,
        processed_items: u64,
        total_items: u64,
        current_file: Option<&str>,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET processed_index_items = ?, total_index_items = ?, current_file = ? WHERE id = ? AND state = 'running';",
                vec![
                    int(processed_items as i64),
                    int(total_items as i64),
                    current_file.map_or(SqliteValue::Null, text),
                    text(job_id),
                ],
            ).await
            .map_err(|error| format!("更新知识库索引进度失败：{error}"))?;
        Ok(())
    }

    /// Marks one import completed and persists bounded counters and warnings.
    /// 将一次导入标记为完成并持久化有界计数与告警。
    ///
    /// # Parameters
    /// 参数
    /// `report` contains verified terminal import counters; `error` is the optional retained diagnostic.
    /// `report` 包含已验证的导入终态计数；`error` 是可选保留诊断。
    ///
    /// # Returns
    /// 返回值
    /// Success after writing the completed import row, or a clock/serialization/storage failure.
    /// 写入完成导入记录后成功，或返回时钟、序列化、存储错误。
    pub(super) async fn finish_import_job(
        &self,
        report: &KnowledgeImportReport,
        error: Option<&str>,
    ) -> Result<(), String> {
        let now = unix_epoch_millis()?;
        let warnings_json = serde_json::to_string(&report.warnings)
            .map_err(|failure| format!("序列化知识库导入告警失败：{failure}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                r#"UPDATE knowledge_import_jobs SET
                    state = 'completed', phase = 'completed', scanned_files = ?, total_files = ?, processed_files = ?, current_file = NULL, changed_files = ?,
                    unchanged_files = ?, deleted_files = ?, skipped_files = ?, processed_bytes = ?, warnings_json = ?,
                    error_message = ?, finished_at_ms = ? WHERE id = ?;"#,
                vec![
                    int(report.scanned_files as i64),
                    int(report.scanned_files as i64),
                    int(report.scanned_files as i64),
                    int(report.changed_files as i64),
                    int(report.unchanged_files as i64),
                    int(report.deleted_files as i64),
                    int(report.skipped_files as i64),
                    int(report.processed_bytes as i64),
                    text(warnings_json),
                    error.map_or(SqliteValue::Null, text),
                    int(now),
                    text(&report.job_id),
                ],
            ).await
            .map_err(|failure| format!("完成知识库导入任务失败：{failure}"))?;
        Ok(())
    }

    /// Marks one repair or rebuild completed with its exact authoritative work total.
    /// 使用精确权威工作总量将一次补全或重建标记为完成。
    ///
    /// Parameters: `job_id` selects the durable row and `processed_items` is the verified chunk total.
    /// Returns: success after the terminal state commits.
    /// 参数：`job_id` 选择持久化记录，`processed_items` 是已验证切片总量。返回值：终态提交后成功。
    pub(super) async fn finish_index_operation_job(
        &self,
        job_id: &str,
        processed_items: u64,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET state = 'completed', phase = 'completed', processed_index_items = ?, total_index_items = ?, current_file = NULL, error_message = NULL, finished_at_ms = ? WHERE id = ?;",
                vec![
                    int(processed_items as i64),
                    int(processed_items as i64),
                    int(unix_epoch_millis()?),
                    text(job_id),
                ],
            ).await
            .map_err(|failure| format!("完成知识库索引操作失败：{failure}"))?;
        Ok(())
    }

    /// Marks one import or index-maintenance operation failed with an explicit bounded error.
    /// 使用显式有界错误将一次导入或索引维护操作标记为失败。
    ///
    /// # Parameters
    /// 参数
    /// `job_id` selects the durable operation; `error` is truncated to the existing warning bound before persistence.
    /// `job_id` 选择持久操作；`error` 按既有告警上限截断后持久化。
    ///
    /// # Returns
    /// 返回值
    /// Success after writing the failed terminal state, or a clock/storage failure.
    /// 写入失败终态后成功，或返回时钟/存储错误。
    pub(super) async fn finish_failed_operation_job(
        &self,
        job_id: &str,
        error: &str,
    ) -> Result<(), String> {
        let bounded = truncate_chars(error, MAX_WARNING_CHARS);
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET state = 'failed', error_message = ?, finished_at_ms = ? WHERE id = ?;",
                vec![text(bounded), int(unix_epoch_millis()?), text(job_id)],
            ).await
            .map_err(|failure| format!("记录知识库操作失败状态失败：{failure}"))?;
        Ok(())
    }

    /// Marks one user-cancelled import or index operation with a distinct durable terminal state.
    /// 使用独立持久化终态标记一次由用户取消的导入或索引操作。
    ///
    /// # Parameters
    /// 参数
    /// `job_id` identifies the exact user-cancelled durable operation.
    /// `job_id` 标识被用户取消的精确持久操作。
    ///
    /// # Returns
    /// 返回值
    /// Success after writing the distinct cancelled terminal state, or a clock/storage failure.
    /// 写入独立取消终态后成功，或返回时钟/存储错误。
    pub(super) async fn finish_cancelled_operation_job(&self, job_id: &str) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET state = 'cancelled', phase = 'cancelled', error_message = NULL, finished_at_ms = ? WHERE id = ?;",
                vec![int(unix_epoch_millis()?), text(job_id)],
            ).await
            .map_err(|failure| format!("记录知识库操作取消状态失败：{failure}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
