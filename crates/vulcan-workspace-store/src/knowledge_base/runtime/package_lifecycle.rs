//! Portable package installation, replacement transactions, and durable crash recovery.
//! 可移植包安装、覆盖事务与持久化崩溃恢复。

use super::super::models::{
    KnowledgeDisplayIndexStatus, KnowledgeLibrarySummary, KnowledgePackageExportResult,
    KnowledgePackageImportResult, KnowledgePackageInspection, KnowledgePackageManifest,
    KnowledgeVectorIndexStatus,
};
use super::super::schema::KNOWLEDGE_SCHEMA_VERSION;
use super::{
    KNOWLEDGE_PACKAGE_INSPECTION_TEMP_PREFIX, KnowledgeBaseRuntime, KnowledgeManifest, LibraryRow,
    MAX_KNOWLEDGE_PACKAGE_JSON_BYTES, library_name_key, library_summary_from_row,
    metadata_is_link_or_reparse, nonnegative_u64,
    package_archive::{
        PreparedKnowledgePackage, canonical_existing_directory, canonical_knowledge_package_path,
        copy_prepared_derived_artifacts, export_knowledge_package_files, prepare_knowledge_package,
    },
    persist_tempfile_atomically, unix_epoch_millis, validate_library_id, validate_library_name,
};
use super::{int, text};
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use crate::runtime_assets;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// Version of the crash-recovery journal used by explicit package replacement.
/// 显式知识库包覆盖使用的崩溃恢复日志版本。
pub(super) const KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION: u32 = 1;

/// Journal phase written before either replacement directory rename.
/// 在任一覆盖目录改名前写入的日志阶段。
pub(super) const KNOWLEDGE_REPLACEMENT_PHASE_PREPARED: &str = "prepared";

/// Journal phase written after the replacement registry commit.
/// 在覆盖注册表提交后写入的日志阶段。
pub(super) const KNOWLEDGE_REPLACEMENT_PHASE_COMMITTED: &str = "committed";

/// Durable state required to recover an interrupted two-rename library replacement.
/// 恢复被中断的两次目录改名覆盖所需的持久化状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct KnowledgeReplacementJournal {
    /// Journal protocol version.
    /// 日志协议版本。
    pub(super) version: u32,
    /// Exact retained target library identity.
    /// 精确保留的目标知识库身份。
    pub(super) target_library_id: String,
    /// Exact sibling backup directory name derived by the host.
    /// 由宿主派生的精确同级备份目录名称。
    pub(super) backup_directory_name: String,
    /// Either prepared before registry commit or committed after registry commit.
    /// 注册表提交前为 prepared，注册表提交后为 committed。
    pub(super) phase: String,
    /// Complete prior registry row used by prepared-phase rollback.
    /// prepared 阶段回滚使用的完整旧注册记录。
    pub(super) previous: KnowledgeLibrarySummary,
}

/// Best-effort journal cleanup guard that deliberately survives process termination.
/// 尽力清理日志的守卫；进程终止时会刻意让日志保留在磁盘。
struct KnowledgeReplacementJournalGuard {
    /// Exact host-owned journal path.
    /// 宿主持有的精确日志路径。
    path: PathBuf,
    /// Whether Drop must retain the journal for startup recovery.
    /// Drop 是否必须保留日志供启动恢复。
    retain: bool,
}

impl KnowledgeReplacementJournalGuard {
    /// Creates a cleanup guard for one durably written journal.
    /// 为一份已持久化写入的日志创建清理守卫。
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            retain: false,
        }
    }

    /// Retains the journal because recovery work remains incomplete.
    /// 因恢复工作尚未完成而保留日志。
    fn retain(&mut self) {
        self.retain = true;
    }

    /// Removes the journal immediately after all backup cleanup succeeds.
    /// 在全部备份清理成功后立即移除日志。
    fn remove_now(mut self) -> Result<(), String> {
        self.retain = true;
        fs::remove_file(&self.path).map_err(|error| {
            format!(
                "删除知识库覆盖恢复日志失败（{}）：{error}",
                self.path.display()
            )
        })
    }
}

impl Drop for KnowledgeReplacementJournalGuard {
    /// Removes a no-longer-needed prepared journal during ordinary error unwinding.
    /// 在普通错误展开期间移除不再需要的 prepared 日志。
    fn drop(&mut self) {
        if !self.retain {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl KnowledgeBaseRuntime {
    /// Recovers every durable replacement journal before the knowledge runtime becomes available.
    /// 在知识库运行时可用前恢复每一份持久化覆盖日志。
    ///
    /// # Parameters
    /// 参数
    /// `self` supplies the exact knowledge root, registry and storage bindings for startup recovery.
    /// `self` 提供启动恢复使用的精确知识库根目录、注册表与存储绑定。
    ///
    /// # Returns
    /// 返回值
    /// Success after validated rollback or committed cleanup, or an explicit recovery failure with unresolved artifacts retained.
    /// 完成已校验回滚或已提交清理后成功，或返回明确恢复失败并保留未解决产物。
    pub(super) async fn recover_interrupted_replacements(&self) -> Result<(), String> {
        let knowledge_root = self.knowledge_root();
        fs::create_dir_all(&knowledge_root).map_err(|error| {
            format!(
                "创建知识库根目录用于覆盖恢复失败（{}）：{error}",
                knowledge_root.display()
            )
        })?;
        // journalPaths is sorted to make repeated recovery deterministic.
        // journalPaths 经过排序，使重复恢复保持确定性。
        let mut journal_paths = Vec::new();
        for entry in fs::read_dir(&knowledge_root)
            .map_err(|error| format!("扫描知识库覆盖恢复日志失败：{error}"))?
        {
            let entry = entry.map_err(|error| format!("读取知识库覆盖恢复目录项失败：{error}"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(".replacement-") || !name.ends_with(".json") {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| format!("读取知识库覆盖恢复日志类型失败：{error}"))?;
            if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
                return Err(format!("知识库覆盖恢复日志不是可信普通文件：{name}"));
            }
            if metadata.len() > MAX_KNOWLEDGE_PACKAGE_JSON_BYTES {
                return Err(format!("知识库覆盖恢复日志超过大小上限：{name}"));
            }
            journal_paths.push(entry.path());
        }
        journal_paths.sort();
        for journal_path in journal_paths {
            // journal is validated before any derived path is used.
            // journal 会在使用任何派生路径前完成校验。
            let journal: KnowledgeReplacementJournal = serde_json::from_slice(
                &fs::read(&journal_path)
                    .map_err(|error| format!("读取知识库覆盖恢复日志失败：{error}"))?,
            )
            .map_err(|error| format!("解析知识库覆盖恢复日志失败：{error}"))?;
            let operation_id = validate_replacement_journal(&journal)?;
            let expected_journal_name = format!(
                ".replacement-{}-{operation_id}.json",
                journal.target_library_id
            );
            if journal_path.file_name().and_then(|value| value.to_str())
                != Some(expected_journal_name.as_str())
            {
                return Err(format!(
                    "知识库覆盖恢复日志文件名与日志身份不匹配：{}",
                    journal_path.display()
                ));
            }
            let root = self.library_root(&journal.target_library_id)?;
            let backup_root = knowledge_root.join(&journal.backup_directory_name);
            let backup_exists = backup_root
                .try_exists()
                .map_err(|error| format!("检查知识库覆盖备份失败：{error}"))?;
            if backup_exists {
                let metadata = fs::symlink_metadata(&backup_root)
                    .map_err(|error| format!("读取知识库覆盖备份类型失败：{error}"))?;
                if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
                    return Err(format!(
                        "知识库覆盖备份不是可信普通目录：{}",
                        backup_root.display()
                    ));
                }
            }
            match journal.phase.as_str() {
                "prepared" if backup_exists => {
                    self.rollback_replaced_library(
                        &journal.target_library_id,
                        &root,
                        &backup_root,
                        &journal.previous,
                    )
                    .await?;
                }
                "prepared" => {
                    if !root.exists() {
                        return Err(format!(
                            "覆盖恢复缺少目标与备份目录：{}",
                            journal.target_library_id
                        ));
                    }
                    let identities = self
                        .activate_library_binding(&journal.target_library_id, false)
                        .await?;
                    self.validate_bound_library_metadata(
                        &journal.target_library_id,
                        &identities.0,
                        &identities.1,
                    )
                    .await?;
                    self.restore_library_registry(&journal.previous).await?;
                }
                "committed" => {
                    if !root.exists() {
                        return Err(format!(
                            "已提交覆盖恢复缺少目标目录：{}",
                            journal.target_library_id
                        ));
                    }
                    let (space_id, binding_id) = self
                        .ensure_library_binding(&journal.target_library_id)
                        .await?;
                    self.verify_sqlite_quick_check(&space_id, &binding_id)
                        .await?;
                    self.verify_index_integrity(&space_id, &binding_id).await?;
                    let counts = self.authoritative_counts(&space_id, &binding_id).await?;
                    let current = self.get_library(&journal.target_library_id).await?;
                    if nonnegative_u64(counts.document_count, "document_count")?
                        != current.document_count
                        || nonnegative_u64(counts.chunk_count, "chunk_count")?
                            != current.chunk_count
                    {
                        return Err(String::from(
                            "已提交知识库覆盖的注册计数与 SQLite 内容不一致。",
                        ));
                    }
                    if backup_exists {
                        self.remove_host_owned_child_directory_if_exists(
                            &backup_root,
                            &journal.backup_directory_name,
                        )?;
                    }
                }
                _ => unreachable!("validated replacement phase"),
            }
            fs::remove_file(&journal_path).map_err(|error| {
                format!(
                    "完成恢复后删除知识库覆盖日志失败（{}）：{error}",
                    journal_path.display()
                )
            })?;
        }
        Ok(())
    }

    /// Removes abandoned package extraction directories and UUID staging directories without registry rows.
    /// 移除遗留的知识库包解压目录以及没有注册记录的 UUID 暂存目录。
    ///
    /// # Parameters
    /// 参数
    /// `self` owns the knowledge root and all registered library identities across lifecycle states.
    /// `self` 持有知识库根目录及全部生命周期状态的已注册库身份。
    ///
    /// # Returns
    /// 返回值
    /// Success after removing only validated unregistered staging/extraction directories, or an explicit safety/I/O failure.
    /// 仅清理已验证的未注册暂存/解压目录后成功，或返回明确安全/I/O 错误。
    pub(super) async fn cleanup_orphan_package_directories(&self) -> Result<(), String> {
        // registeredIds includes every lifecycle state so recovery never deletes registered data.
        // registeredIds 包含所有生命周期状态，确保恢复不会删除已注册数据。
        let registered_ids: BTreeSet<String> = self
            .list_libraries(None)
            .await?
            .into_iter()
            .map(|library| library.id)
            .collect();
        let knowledge_root = self.knowledge_root();
        for entry in fs::read_dir(&knowledge_root)
            .map_err(|error| format!("扫描知识库包孤立暂存目录失败：{error}"))?
        {
            let entry = entry.map_err(|error| format!("读取知识库包孤立目录项失败：{error}"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // orphanLibrary identifies an unregistered UUID directory created by package installation.
            // orphanLibrary 标识知识库包安装创建但未注册的 UUID 目录。
            let orphan_library =
                validate_library_id(&name).is_ok() && !registered_ids.contains(&name);
            // abandonedExtraction identifies a TempDir that survived process termination before Drop cleanup.
            // abandonedExtraction 标识进程终止后未能通过 Drop 清理的临时解压目录。
            let abandoned_extraction = is_package_inspection_temp_directory_name(&name);
            if !orphan_library && !abandoned_extraction {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| format!("读取知识库包孤立目录类型失败：{error}"))?;
            if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
                return Err(format!("知识库包孤立暂存项不是可信普通目录：{name}"));
            }
            if orphan_library {
                self.release_library_binding(&name).await?;
            }
            self.remove_host_owned_child_directory_if_exists(&entry.path(), &name)?;
        }
        Ok(())
    }

    /// Exports one complete ready library into a portable .vkb archive.
    /// 将一个完整且就绪的知识库导出为可移植 .vkb 归档。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` selects the exact ready library; `destination_directory` is the existing user-selected export directory.
    /// `library_id` 选择精确就绪库；`destination_directory` 是用户选择的现有导出目录。
    ///
    /// # Returns
    /// 返回值
    /// The exported archive after binding restoration, or an explicit validation, export or restoration failure.
    /// 绑定恢复后返回导出归档，或返回明确校验、导出、恢复失败。
    pub async fn export_package(
        &self,
        library_id: &str,
        destination_directory: &Path,
    ) -> Result<KnowledgePackageExportResult, String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        let library = self.require_active_library(library_id).await?;
        if library.index_status != "ready" {
            return Err(format!(
                "知识库索引状态不是 ready，不能导出免重建包：{}",
                library.index_status
            ));
        }
        if self.operation_is_active_for_library(library_id).await? {
            return Err(String::from(
                "知识库仍有导入或派生索引操作运行中，拒绝导出。",
            ));
        }
        if matches!(
            library.settings.display_index.status,
            KnowledgeDisplayIndexStatus::Building
        ) || matches!(
            library.settings.vector_index.status,
            KnowledgeVectorIndexStatus::Building
        ) {
            return Err(String::from("知识库派生索引仍在构建中，拒绝导出。"));
        }
        let destination_directory = canonical_existing_directory(destination_directory)?;
        let display_path = self.library_root(library_id)?.join("display.sqlite3");
        if library.settings.display_index.status == KnowledgeDisplayIndexStatus::Ready
            && !display_path
                .try_exists()
                .map_err(|error| format!("检查知识库显示库是否存在失败：{error}"))?
        {
            return Err(String::from(
                "知识库显示库状态为 ready，但显示库文件不存在。",
            ));
        }
        let vector_root = self.library_root(library_id)?.join("lancedb");
        if library.settings.vector_index.status == KnowledgeVectorIndexStatus::Ready
            && !vector_root
                .try_exists()
                .map_err(|error| format!("检查知识库 LanceDB 是否存在失败：{error}"))?
        {
            return Err(String::from(
                "知识库向量索引状态为 ready，但 LanceDB 目录不存在。",
            ));
        }
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        self.verify_sqlite_quick_check(&space_id, &binding_id)
            .await?;
        self.verify_index_integrity(&space_id, &binding_id).await?;
        // authoritativeCounts prevents exporting a package whose public manifest cannot validate on reimport.
        // authoritativeCounts 防止导出公开清单在重新导入时无法通过校验的知识库包。
        let authoritative_counts = self.authoritative_counts(&space_id, &binding_id).await?;
        if nonnegative_u64(authoritative_counts.document_count, "document_count")?
            != library.document_count
            || nonnegative_u64(authoritative_counts.chunk_count, "chunk_count")?
                != library.chunk_count
        {
            return Err(String::from(
                "知识库注册计数与 SQLite 权威内容不一致，拒绝导出。",
            ));
        }
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            "PRAGMA wal_checkpoint(TRUNCATE);",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("导出前收口知识库 SQLite 日志失败：{error}"))?;
        if display_path
            .try_exists()
            .map_err(|error| format!("检查知识库显示库是否存在失败：{error}"))?
        {
            let (_display_space_id, _display_binding_id) =
                self.ensure_display_binding(library_id).await?;
            local_sql::execute_script_async(
                &self.host_runtime.local,
                self.resolve_library_binding(&binding_id)?,
                "PRAGMA wal_checkpoint(TRUNCATE);",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("导出前收口知识库显示库日志失败：{error}"))?;
        }
        self.release_library_binding(library_id).await?;
        let export_result = export_knowledge_package_files(
            &library,
            &self.library_root(library_id)?,
            &destination_directory,
        );
        let restore_result = async {
            let identities = self.activate_library_binding(library_id, false).await?;
            self.validate_bound_library_metadata(library_id, &identities.0, &identities.1)
                .await
        }
        .await;
        match (export_result, restore_result) {
            (Ok(result), Ok(())) => Ok(result),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(restore_error)) => Err(format!(
                "知识库包已经写出，但恢复 SQLite 绑定失败：{restore_error}"
            )),
            (Err(error), Err(restore_error)) => Err(format!(
                "{error}；同时恢复 SQLite 绑定失败：{restore_error}"
            )),
        }
    }

    /// Inspects one package without changing registry or physical library state.
    /// 检查一个知识库包且不改变注册表或物理知识库状态。
    ///
    /// # Parameters
    /// 参数
    /// `archive_path` identifies the selected portable package; `self` owns isolated temporary validation bindings.
    /// `archive_path` 标识选中可移植包；`self` 持有隔离临时校验绑定。
    ///
    /// # Returns
    /// 返回值
    /// Verified package metadata and a current name conflict, or a full SQLite validation/cleanup error.
    /// 返回经完整校验的包元数据及当前名称冲突，或完整 SQLite 校验/清理错误。
    pub async fn inspect_package(
        &self,
        archive_path: &Path,
    ) -> Result<KnowledgePackageInspection, String> {
        let canonical_archive = canonical_knowledge_package_path(archive_path)?;
        let prepared = prepare_knowledge_package(&canonical_archive, &self.knowledge_root())?;
        // validationLibraryId isolates transient full-database checks from every registered library.
        // validationLibraryId 将临时完整数据库检查与全部已注册知识库隔离。
        let validation_library_id = Uuid::new_v4().to_string();
        // validationResult opens the copied SQLite and validates metadata, counts, and FTS readiness.
        // validationResult 打开复制的 SQLite，并校验元数据、计数和 FTS 就绪状态。
        let validation_result = self
            .install_prepared_package_database(
                &prepared,
                &validation_library_id,
                &prepared.package.name,
            )
            .await;
        // cleanupResult guarantees the read-only inspection leaves no temporary library behind.
        // cleanupResult 保证只读检查不会遗留临时知识库。
        let cleanup_result = self
            .cleanup_failed_package_install(&validation_library_id)
            .await;
        match (validation_result, cleanup_result) {
            (Ok(()), Ok(())) => {}
            (Err(error), Ok(())) => return Err(error),
            (Ok(()), Err(cleanup_error)) => return Err(cleanup_error),
            (Err(error), Err(cleanup_error)) => {
                return Err(format!(
                    "{error}；同时清理包检查暂存知识库失败：{cleanup_error}"
                ));
            }
        }
        let conflicting_library = self
            .find_library_by_normalized_name(&prepared.package.name, None)
            .await?;
        Ok(KnowledgePackageInspection {
            archive_path: runtime_assets::native_host_path_text_from_path(&canonical_archive),
            package: prepared.package,
            conflicting_library,
        })
    }

    /// Imports one package as a new independent library without rebuilding its index.
    /// 将一个知识库包作为新的独立知识库导入且不重建索引。
    ///
    /// # Parameters
    /// 参数
    /// `archive_path` identifies the package and `name` is normalized for the new independent registry row.
    /// `archive_path` 标识包，`name` 经规范化后用于新的独立注册记录。
    ///
    /// # Returns
    /// 返回值
    /// The committed independent library, or an installation/registry error with its cleanup result.
    /// 返回已提交独立库，或包含清理结果的安装/注册失败。
    pub async fn import_package_as_new(
        &self,
        archive_path: &Path,
        name: &str,
    ) -> Result<KnowledgePackageImportResult, String> {
        let normalized_name = validate_library_name(name)?;
        if let Some(conflict) = self
            .find_library_by_normalized_name(&normalized_name, None)
            .await?
        {
            return Err(format!("知识库名称已存在：{}", conflict.name));
        }
        let canonical_archive = canonical_knowledge_package_path(archive_path)?;
        let prepared = prepare_knowledge_package(&canonical_archive, &self.knowledge_root())?;
        let library_id = Uuid::new_v4().to_string();
        let now = unix_epoch_millis()?;
        let install_result = self
            .install_prepared_package_database(&prepared, &library_id, &normalized_name)
            .await;
        if let Err(error) = install_result {
            return match self.cleanup_failed_package_install(&library_id).await {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}；同时清理失败的知识库包安装失败：{cleanup_error}"
                )),
            };
        }
        let insert_result = self
            .insert_imported_library_registry(&library_id, &normalized_name, &prepared.package, now)
            .await;
        if let Err(error) = insert_result {
            return match self.cleanup_failed_package_install(&library_id).await {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}；同时清理未注册的知识库包失败：{cleanup_error}"
                )),
            };
        }
        // library mirrors the exact row committed by insert_imported_library_registry, avoiding a post-commit read failure being misreported as an import failure.
        // library 精确镜像 insert_imported_library_registry 已提交的记录，避免提交后的读取失败被误报为导入失败。
        let library = KnowledgeLibrarySummary {
            id: library_id,
            name: normalized_name,
            description: prepared.package.description.clone(),
            settings: prepared.package.settings.clone().unwrap_or_default(),
            lifecycle_state: String::from("active"),
            document_count: prepared.package.document_count,
            chunk_count: prepared.package.chunk_count,
            index_status: String::from("ready"),
            last_completed_import_at_ms: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        Ok(KnowledgePackageImportResult {
            library,
            replaced_existing: false,
            warnings: Vec::new(),
        })
    }

    /// Explicitly replaces one library while retaining its identity, bindings, and session references.
    /// 显式替换一个知识库，同时保留其标识、绑定和会话引用。
    ///
    /// # Parameters
    /// 参数
    /// `archive_path` supplies the new contents; `target_library_id` retains identity and bindings; `name` is the requested normalized display name.
    /// `archive_path` 提供新内容；`target_library_id` 保留身份与绑定；`name` 是请求的规范化展示名称。
    ///
    /// # Returns
    /// 返回值
    /// A committed replacement with nonfatal cleanup warnings, or the original failure and exact rollback outcome.
    /// 返回带非致命清理警告的已提交覆盖，或原失败及精确回滚结果。
    pub async fn replace_library_from_package(
        &self,
        archive_path: &Path,
        target_library_id: &str,
        name: &str,
    ) -> Result<KnowledgePackageImportResult, String> {
        let _guard = self.library_mutation_guard(target_library_id).await?;
        let current = self.require_active_library(target_library_id).await?;
        let normalized_name = validate_library_name(name)?;
        if let Some(conflict) = self
            .find_library_by_normalized_name(&normalized_name, Some(target_library_id))
            .await?
        {
            return Err(format!("知识库名称已被其他知识库使用：{}", conflict.name));
        }
        let canonical_archive = canonical_knowledge_package_path(archive_path)?;
        let prepared = prepare_knowledge_package(&canonical_archive, &self.knowledge_root())?;
        let (_space_id, binding_id) = self.ensure_library_binding(target_library_id).await?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            "PRAGMA wal_checkpoint(TRUNCATE);",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("覆盖前收口知识库 SQLite 日志失败：{error}"))?;
        // stagingLibraryId owns a fully copied and validated sibling directory before the old target moves.
        // stagingLibraryId 在旧目标移动前持有一个已完整复制并校验的同级暂存目录。
        let staging_library_id = Uuid::new_v4().to_string();
        let staging_result = self
            .install_prepared_package_database(&prepared, &staging_library_id, &normalized_name)
            .await;
        if let Err(error) = staging_result {
            return match self
                .cleanup_failed_package_install(&staging_library_id)
                .await
            {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}；同时清理覆盖暂存知识库失败：{cleanup_error}"
                )),
            };
        }
        if let Err(error) = self.release_library_binding(&staging_library_id).await {
            return match self
                .cleanup_failed_package_install(&staging_library_id)
                .await
            {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}；同时清理覆盖暂存知识库失败：{cleanup_error}"
                )),
            };
        }
        // stagingRoot is already complete, identity-checked, count-checked, and search-ready.
        // stagingRoot 已完成复制、身份校验、计数校验并达到搜索就绪状态。
        let staging_root = self.library_root(&staging_library_id)?;
        let root = self.library_root(target_library_id)?;
        // operationId makes the backup and journal names an inseparable host-derived pair.
        // operationId 使备份与日志名称成为不可分割的宿主派生对。
        let operation_id = Uuid::new_v4().to_string();
        let (backup_root, journal_path, backup_directory_name) =
            self.replacement_operation_artifacts(target_library_id, &operation_id)?;
        // journal is durable before the first destructive rename, so process termination remains recoverable.
        // journal 在首次破坏性改名前持久化，确保进程终止后仍可恢复。
        let mut journal = KnowledgeReplacementJournal {
            version: KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION,
            target_library_id: target_library_id.to_string(),
            backup_directory_name,
            phase: String::from(KNOWLEDGE_REPLACEMENT_PHASE_PREPARED),
            previous: current.clone(),
        };
        if let Err(error) = self.write_replacement_journal_atomic(&journal_path, &journal) {
            return match self
                .cleanup_failed_package_install(&staging_library_id)
                .await
            {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}；同时清理覆盖暂存知识库失败：{cleanup_error}"
                )),
            };
        }
        // journalGuard removes completed prepared journals but retains unresolved crash recovery state.
        // journalGuard 会移除已处理的 prepared 日志，但保留尚未解决的崩溃恢复状态。
        let mut journal_guard = KnowledgeReplacementJournalGuard::new(journal_path.clone());
        if let Err(error) = self.release_library_binding(target_library_id).await {
            let cleanup_result = self
                .cleanup_failed_package_install(&staging_library_id)
                .await;
            return match cleanup_result {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}；同时清理覆盖暂存知识库失败：{cleanup_error}"
                )),
            };
        }
        if let Err(error) = fs::rename(&root, &backup_root) {
            // restoreResult reopens the untouched original after the backup rename itself fails.
            // restoreResult 在备份改名本身失败后重新打开未变更的原知识库。
            let restore_result = async {
                let identities = self
                    .activate_library_binding(target_library_id, false)
                    .await?;
                self.validate_bound_library_metadata(
                    target_library_id,
                    &identities.0,
                    &identities.1,
                )
                .await
            }
            .await;
            let cleanup_result = self
                .cleanup_failed_package_install(&staging_library_id)
                .await;
            if restore_result.is_err() {
                journal_guard.retain();
            }
            return match (restore_result, cleanup_result) {
                (Ok(()), Ok(())) => Err(format!(
                    "暂存待覆盖知识库目录失败（{}）：{error}",
                    root.display()
                )),
                (Ok(()), Err(cleanup_error)) => Err(format!(
                    "暂存待覆盖知识库目录失败（{}）：{error}；同时清理新库暂存目录失败：{}",
                    root.display(),
                    cleanup_error
                )),
                (Err(restore_error), Ok(())) => Err(format!(
                    "暂存待覆盖知识库目录失败（{}）：{error}；同时恢复原 SQLite 绑定失败：{restore_error}",
                    root.display()
                )),
                (Err(restore_error), Err(cleanup_error)) => Err(format!(
                    "暂存待覆盖知识库目录失败（{}）：{error}；同时恢复原 SQLite 绑定失败：{restore_error}；清理新库暂存目录失败：{}",
                    root.display(),
                    cleanup_error
                )),
            };
        }
        if let Err(error) = fs::rename(&staging_root, &root) {
            let rollback = self
                .rollback_replaced_library(target_library_id, &root, &backup_root, &current)
                .await;
            let cleanup = self
                .cleanup_failed_package_install(&staging_library_id)
                .await;
            if rollback.is_err() {
                journal_guard.retain();
            }
            return match (rollback, cleanup) {
                (Ok(()), Ok(())) => Err(format!("切换已校验知识库暂存目录失败：{error}")),
                (Err(rollback_error), Ok(())) => Err(format!(
                    "切换已校验知识库暂存目录失败：{error}；同时恢复原知识库失败：{rollback_error}"
                )),
                (Ok(()), Err(cleanup_error)) => Err(format!(
                    "切换已校验知识库暂存目录失败：{error}；同时清理新库暂存目录失败：{cleanup_error}"
                )),
                (Err(rollback_error), Err(cleanup_error)) => Err(format!(
                    "切换已校验知识库暂存目录失败：{error}；同时恢复原知识库失败：{rollback_error}；清理新库暂存目录失败：{cleanup_error}"
                )),
            };
        }
        let replacement = async {
            self.retarget_staged_package_database(
                &prepared,
                &staging_library_id,
                target_library_id,
                &normalized_name,
            )
            .await?;
            self.update_replaced_library_registry(
                target_library_id,
                &normalized_name,
                &prepared.package,
            )
            .await?;
            self.get_library(target_library_id).await
        }
        .await;
        match replacement {
            Ok(library) => {
                // committed is synchronized before backup deletion, defining the durable roll-forward point.
                // committed 在删除备份前完成同步，从而定义持久化前滚点。
                journal.phase = String::from(KNOWLEDGE_REPLACEMENT_PHASE_COMMITTED);
                if let Err(journal_error) =
                    self.write_replacement_journal_atomic(&journal_path, &journal)
                {
                    let rollback = self
                        .rollback_replaced_library(target_library_id, &root, &backup_root, &current)
                        .await;
                    if rollback.is_err() {
                        journal_guard.retain();
                    }
                    return match rollback {
                        Ok(()) => Err(journal_error),
                        Err(rollback_error) => Err(format!(
                            "{journal_error}；同时恢复原知识库失败：{rollback_error}"
                        )),
                    };
                }
                // warnings report non-fatal durable-cleanup failures without mislabeling a committed replacement as failed.
                // warnings 报告非致命持久化清理失败，避免把已提交覆盖误报为失败。
                let mut warnings = Vec::new();
                if let Err(error) = self.remove_host_owned_child_directory_if_exists(
                    &backup_root,
                    &journal.backup_directory_name,
                ) {
                    journal_guard.retain();
                    warnings.push(format!(
                        "知识库覆盖成功，但清理旧备份目录失败（{}）：{error}；服务重启时将自动重试",
                        backup_root.display()
                    ));
                } else if let Err(error) = journal_guard.remove_now() {
                    warnings.push(format!(
                        "知识库覆盖成功，但清理恢复日志失败：{error}；服务重启时将自动重试"
                    ));
                }
                Ok(KnowledgePackageImportResult {
                    library,
                    replaced_existing: true,
                    warnings,
                })
            }
            Err(error) => {
                let rollback = self
                    .rollback_replaced_library(target_library_id, &root, &backup_root, &current)
                    .await;
                if rollback.is_err() {
                    journal_guard.retain();
                }
                match rollback {
                    Ok(()) => Err(error),
                    Err(rollback_error) => {
                        Err(format!("{error}；同时恢复原知识库失败：{rollback_error}"))
                    }
                }
            }
        }
    }

    /// Finds one registry row by its exact normalized user-visible name.
    /// 按精确规范化用户可见名称查找一条注册记录。
    ///
    /// # Parameters
    /// 参数
    /// `name` supplies the normalized comparison key; `excluded_library_id` optionally excludes the retained replacement target.
    /// `name` 提供规范化比较键；`excluded_library_id` 可选排除需保留的覆盖目标。
    ///
    /// # Returns
    /// 返回值
    /// The matching registry row, no conflict, or a query/decoding failure.
    /// 返回匹配注册记录、无冲突，或查询/解码失败。
    async fn find_library_by_normalized_name(
        &self,
        name: &str,
        excluded_library_id: Option<&str>,
    ) -> Result<Option<KnowledgeLibrarySummary>, String> {
        let (predicate, params): (&str, &[SqliteValue]) =
            if let Some(excluded_library_id) = excluded_library_id {
                (
                    "WHERE name_key = ? AND id <> ?",
                    &[text(library_name_key(name)), text(excluded_library_id)],
                )
            } else {
                ("WHERE name_key = ?", &[text(library_name_key(name))])
            };
        let sql = format!(
            r#"SELECT id, name, description, lifecycle_state, document_count, chunk_count,
                      index_status, settings_json, last_completed_import_at_ms, created_at_ms, updated_at_ms
               FROM knowledge_libraries {predicate} LIMIT 1;"#
        );
        let rows: Vec<LibraryRow> = self.query_app_rows(&sql, params).await?;
        rows.into_iter()
            .next()
            .map(library_summary_from_row)
            .transpose()
    }

    /// Installs and validates one prepared SQLite file under the exact target identity.
    /// 在精确目标标识下安装并校验一份已准备的 SQLite 文件。
    ///
    /// # Parameters
    /// 参数
    /// `prepared` owns validated archive files; `target_library_id` and `target_name` identify a new isolated installation.
    /// `prepared` 持有已校验归档文件；`target_library_id` 与 `target_name` 标识新的隔离安装。
    ///
    /// # Returns
    /// 返回值
    /// Success only after copied databases pass identity, integrity and count checks; otherwise an explicit install error.
    /// 复制数据库通过身份、完整性和计数检查后成功，否则返回明确安装错误。
    async fn install_prepared_package_database(
        &self,
        prepared: &PreparedKnowledgePackage,
        target_library_id: &str,
        target_name: &str,
    ) -> Result<(), String> {
        validate_library_id(target_library_id)?;
        let root = self.library_root(target_library_id)?;
        if root.exists() {
            return Err(format!("知识库目标目录已经存在：{}", root.display()));
        }
        fs::create_dir(&root)
            .map_err(|error| format!("创建知识库目标目录失败（{}）：{error}", root.display()))?;
        let target_database = root.join("content.sqlite3");
        fs::copy(prepared.database_path(), &target_database).map_err(|error| {
            format!(
                "复制知识库 SQLite 失败（{}）：{error}",
                target_database.display()
            )
        })?;
        copy_prepared_derived_artifacts(prepared, &root)?;
        self.write_manifest_atomic(&KnowledgeManifest {
            library_id: target_library_id.to_string(),
            name: target_name.to_string(),
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
        })?;
        let (space_id, binding_id) = self
            .activate_library_binding(target_library_id, false)
            .await?;
        self.verify_sqlite_quick_check(&space_id, &binding_id)
            .await?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            "UPDATE knowledge_metadata SET value = ? WHERE key = 'library_id' AND value = ?;",
            vec![
                text(target_library_id),
                text(&prepared.source_manifest.library_id),
            ],
        )
        .await
        .map_err(|error| format!("重写导入知识库内部标识失败：{error}"))?;
        self.validate_bound_library_metadata(target_library_id, &space_id, &binding_id)
            .await?;
        self.verify_index_integrity(&space_id, &binding_id).await?;
        if prepared.display_database_path().is_some() {
            let (display_space_id, display_binding_id) = self
                .activate_display_binding(target_library_id, false)
                .await?;
            local_sql::execute_script_async(
                &self.host_runtime.local,
                self.resolve_library_binding(&binding_id)?,
                "UPDATE display_metadata SET value = ? WHERE key = 'library_id' AND value = ?;",
                vec![
                    text(target_library_id),
                    text(&prepared.source_manifest.library_id),
                ],
            )
            .await
            .map_err(|error| format!("重写导入显示库内部标识失败：{error}"))?;
            self.validate_display_bound_metadata(
                target_library_id,
                &display_space_id,
                &display_binding_id,
            )
            .await?;
        }
        if prepared.vector_root().is_some() {
            self.enable_vector_binding(target_library_id, false).await?;
        }
        let counts = self.authoritative_counts(&space_id, &binding_id).await?;
        if nonnegative_u64(counts.document_count, "document_count")?
            != prepared.package.document_count
            || nonnegative_u64(counts.chunk_count, "chunk_count")? != prepared.package.chunk_count
        {
            return Err(String::from("知识库包权威计数与 SQLite 内容不一致。"));
        }
        Ok(())
    }

    /// Retargets one already validated sibling database after its directory is atomically switched into place.
    /// 在已校验同级数据库目录原子切换到位后，将其重新定向到最终目标身份。
    ///
    /// # Parameters
    /// 参数
    /// `prepared` retains the inspected counts; `staging_library_id` is replaced by `target_library_id` and `target_name` after the directory switch.
    /// `prepared` 保留已检查计数；目录切换后以 `target_library_id`、`target_name` 替换 `staging_library_id`。
    ///
    /// # Returns
    /// 返回值
    /// Success after exact metadata retargeting and full revalidation, or an explicit failure before registry commit.
    /// 精确重定向元数据并完成复验后成功，或在注册表提交前返回明确失败。
    async fn retarget_staged_package_database(
        &self,
        prepared: &PreparedKnowledgePackage,
        staging_library_id: &str,
        target_library_id: &str,
        target_name: &str,
    ) -> Result<(), String> {
        self.write_manifest_atomic(&KnowledgeManifest {
            library_id: target_library_id.to_string(),
            name: target_name.to_string(),
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
        })?;
        let (space_id, binding_id) = self
            .activate_library_binding(target_library_id, false)
            .await?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            "UPDATE knowledge_metadata SET value = ? WHERE key = 'library_id' AND value = ?;",
            vec![text(target_library_id), text(staging_library_id)],
        )
        .await
        .map_err(|error| format!("提交覆盖知识库内部标识失败：{error}"))?;
        self.verify_sqlite_quick_check(&space_id, &binding_id)
            .await?;
        self.validate_bound_library_metadata(target_library_id, &space_id, &binding_id)
            .await?;
        self.verify_index_integrity(&space_id, &binding_id).await?;
        if prepared.display_database_path().is_some() {
            let (display_space_id, display_binding_id) = self
                .activate_display_binding(target_library_id, false)
                .await?;
            local_sql::execute_script_async(
                &self.host_runtime.local,
                self.resolve_library_binding(&binding_id)?,
                "UPDATE display_metadata SET value = ? WHERE key = 'library_id' AND value = ?;",
                vec![text(target_library_id), text(staging_library_id)],
            )
            .await
            .map_err(|error| format!("提交覆盖显示库内部标识失败：{error}"))?;
            self.validate_display_bound_metadata(
                target_library_id,
                &display_space_id,
                &display_binding_id,
            )
            .await?;
        }
        if prepared.vector_root().is_some() {
            self.enable_vector_binding(target_library_id, false).await?;
        }
        // counts proves the switched database still matches the inspected package before registry commit.
        // counts 证明切换后的数据库在注册表提交前仍与已检查知识库包匹配。
        let counts = self.authoritative_counts(&space_id, &binding_id).await?;
        if nonnegative_u64(counts.document_count, "document_count")?
            != prepared.package.document_count
            || nonnegative_u64(counts.chunk_count, "chunk_count")? != prepared.package.chunk_count
        {
            return Err(String::from("覆盖知识库权威计数与已检查包不一致。"));
        }
        Ok(())
    }

    /// Removes one failed newly installed package directory and binding.
    /// 移除一次失败的新包安装目录及绑定。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` identifies the exact failed, unregistered installation owned by this lifecycle.
    /// `library_id` 标识此生命周期持有的精确失败、未注册安装。
    ///
    /// # Returns
    /// 返回值
    /// Success after binding release and guarded directory cleanup, or an explicit cleanup failure.
    /// 释放绑定并经安全守卫清理目录后成功，或返回明确清理失败。
    async fn cleanup_failed_package_install(&self, library_id: &str) -> Result<(), String> {
        self.release_library_binding(library_id).await?;
        let root = self.library_root(library_id)?;
        self.remove_host_owned_child_directory_if_exists(&root, library_id)
    }

    /// Inserts one imported library into the central registry after physical validation succeeds.
    /// 在物理校验成功后将一个导入知识库写入中央注册表。
    ///
    /// # Parameters
    /// 参数
    /// `library_id`, `name`, `package` and `now` describe the validated installation and its exact creation timestamp.
    /// `library_id`、`name`、`package` 与 `now` 描述已校验安装及其精确创建时间。
    ///
    /// # Returns
    /// 返回值
    /// Success after inserting the normalized active registry row, or a serialization/count/SQL failure.
    /// 插入规范化活动注册记录后成功，或返回序列化、计数、SQL 错误。
    async fn insert_imported_library_registry(
        &self,
        library_id: &str,
        name: &str,
        package: &KnowledgePackageManifest,
        now: i64,
    ) -> Result<(), String> {
        let mut settings = package.settings.clone().unwrap_or_default();
        settings.normalize_derived_compatibility();
        settings.validate()?;
        let settings_json = serde_json::to_string(&settings)
            .map_err(|error| format!("序列化导入知识库配置失败：{error}"))?;
        local_sql::execute_batch_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
            r#"INSERT INTO knowledge_libraries(
                    id, name, name_key, description, lifecycle_state, document_count,
                    chunk_count, index_status, settings_json, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, 'active', ?, ?, 'ready', ?, ?, ?);"#,
            vec![vec![
                text(library_id),
                text(name),
                text(library_name_key(name)),
                text(&package.description),
                int(checked_package_count(
                    package.document_count,
                    "document_count",
                )?),
                int(checked_package_count(package.chunk_count, "chunk_count")?),
                text(&settings_json),
                int(now),
                int(now),
            ]],
        )
        .await
        .map_err(|error| format!("注册导入知识库失败：{error}"))?;
        Ok(())
    }

    /// Updates only the retained target registry row after a physical replacement succeeds.
    /// 在物理替换成功后仅更新保留的目标注册记录。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` retains the target identity; `name` and `package` supply validated replacement metadata.
    /// `library_id` 保留目标身份；`name` 与 `package` 提供已校验覆盖元数据。
    ///
    /// # Returns
    /// 返回值
    /// Success after updating only the retained registry row, or an explicit metadata/SQL failure.
    /// 仅更新所保留注册记录后成功，或返回明确元数据/SQL 错误。
    async fn update_replaced_library_registry(
        &self,
        library_id: &str,
        name: &str,
        package: &KnowledgePackageManifest,
    ) -> Result<(), String> {
        let now = unix_epoch_millis()?;
        let mut settings = package.settings.clone().unwrap_or_default();
        settings.normalize_derived_compatibility();
        settings.validate()?;
        let settings_json = serde_json::to_string(&settings)
            .map_err(|error| format!("序列化被覆盖知识库配置失败：{error}"))?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
            r#"UPDATE knowledge_libraries SET name = ?, name_key = ?, description = ?,
                    lifecycle_state = 'active', document_count = ?, chunk_count = ?,
                    settings_json = ?, index_status = 'ready',
                    last_completed_import_at_ms = NULL, updated_at_ms = ?
                    WHERE id = ?;"#,
            vec![
                text(name),
                text(library_name_key(name)),
                text(&package.description),
                int(checked_package_count(
                    package.document_count,
                    "document_count",
                )?),
                int(checked_package_count(package.chunk_count, "chunk_count")?),
                text(&settings_json),
                int(now),
                text(library_id),
            ],
        )
        .await
        .map_err(|error| format!("更新被覆盖知识库注册记录失败：{error}"))?;
        Ok(())
    }

    /// Restores both physical storage and central metadata after replacement failure.
    /// 在替换失败后同时恢复物理存储与中央元数据。
    ///
    /// # Parameters
    /// 参数
    /// `library_id`, `root` and `backup_root` identify the exact switched directories; `previous` is the complete before-image.
    /// `library_id`、`root` 与 `backup_root` 标识精确切换目录；`previous` 是完整前镜像。
    ///
    /// # Returns
    /// 返回值
    /// Success after physical, binding and exact registry restoration, or an explicit rollback failure.
    /// 恢复物理目录、绑定及精确注册记录后成功，或返回明确回滚失败。
    async fn rollback_replaced_library(
        &self,
        library_id: &str,
        root: &Path,
        backup_root: &Path,
        previous: &KnowledgeLibrarySummary,
    ) -> Result<(), String> {
        self.release_library_binding(library_id).await?;
        self.remove_host_owned_child_directory_if_exists(root, library_id)?;
        fs::rename(backup_root, root).map_err(|error| {
            format!("恢复原知识库目录失败（{}）：{error}", backup_root.display())
        })?;
        let identities = self.activate_library_binding(library_id, false).await?;
        self.validate_bound_library_metadata(library_id, &identities.0, &identities.1)
            .await?;
        self.restore_library_registry(previous).await
    }

    /// Restores the exact prior central registry row after replacement rollback.
    /// 在覆盖回滚后恢复精确的旧中央注册记录。
    ///
    /// # Parameters
    /// 参数
    /// `previous` is the complete authoritative row captured before replacement begins.
    /// `previous` 是覆盖开始前捕获的完整权威记录。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only when the persisted row reads back exactly as captured.
    /// 仅在持久化记录回读后与捕获值完全一致时返回成功。
    async fn restore_library_registry(
        &self,
        previous: &KnowledgeLibrarySummary,
    ) -> Result<(), String> {
        validate_library_id(&previous.id)?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
            r#"UPDATE knowledge_libraries SET name = ?, name_key = ?, description = ?,
                    lifecycle_state = ?, document_count = ?, chunk_count = ?, index_status = ?,
                    settings_json = ?,
                    last_completed_import_at_ms = ?, created_at_ms = ?, updated_at_ms = ?
                    WHERE id = ?;"#,
            vec![
                text(&previous.name),
                text(library_name_key(&previous.name)),
                text(&previous.description),
                text(&previous.lifecycle_state),
                int(checked_package_count(
                    previous.document_count,
                    "document_count",
                )?),
                int(checked_package_count(previous.chunk_count, "chunk_count")?),
                text(&previous.index_status),
                text(
                    &serde_json::to_string(&previous.settings)
                        .map_err(|error| format!("序列化恢复知识库配置失败：{error}"))?,
                ),
                previous
                    .last_completed_import_at_ms
                    .map_or(SqliteValue::Null, int),
                int(previous.created_at_ms),
                int(previous.updated_at_ms),
                text(&previous.id),
            ],
        )
        .await
        .map_err(|error| format!("恢复原知识库注册记录失败：{error}"))?;
        let restored = self.get_library(&previous.id).await?;
        if restored != *previous {
            return Err(String::from("恢复后的知识库注册记录与旧记录不一致。"));
        }
        Ok(())
    }

    /// Derives the exact sibling backup and journal paths for one replacement operation.
    /// 为一次覆盖操作派生精确的同级备份路径与日志路径。
    ///
    /// # Parameters
    /// 参数
    /// `target_library_id` is the canonical UUID whose identity must be retained.
    /// `target_library_id` 是必须保留身份的规范 UUID。
    /// `operation_id` is the canonical host-generated UUID shared by both artifact names.
    /// `operation_id` 是两个产物名称共享的宿主生成规范 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the backup path, journal path, and backup directory name after validating both IDs.
    /// 校验两个标识后返回备份路径、日志路径与备份目录名称。
    pub(super) fn replacement_operation_artifacts(
        &self,
        target_library_id: &str,
        operation_id: &str,
    ) -> Result<(PathBuf, PathBuf, String), String> {
        validate_library_id(target_library_id)?;
        validate_library_id(operation_id)?;
        let knowledge_root = self.knowledge_root();
        let backup_directory_name = format!(".replace-{target_library_id}-{operation_id}");
        let journal_file_name = format!(".replacement-{target_library_id}-{operation_id}.json");
        Ok((
            knowledge_root.join(&backup_directory_name),
            knowledge_root.join(journal_file_name),
            backup_directory_name,
        ))
    }

    /// Atomically writes and synchronizes one validated replacement recovery journal.
    /// 原子写入并同步一份已校验的覆盖恢复日志。
    ///
    /// # Parameters
    /// 参数
    /// `path` is the exact host-derived journal destination in the knowledge root.
    /// `path` 是知识库根目录中由宿主精确派生的日志目标路径。
    /// `journal` contains the prior registry row and current durable phase.
    /// `journal` 包含旧注册记录与当前持久化阶段。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only after a same-directory temporary file is synchronized and atomically persisted.
    /// 仅在同目录临时文件完成同步并原子提交后返回成功。
    pub(super) fn write_replacement_journal_atomic(
        &self,
        path: &Path,
        journal: &KnowledgeReplacementJournal,
    ) -> Result<(), String> {
        let operation_id = validate_replacement_journal(journal)?;
        let (_, expected_path, _) =
            self.replacement_operation_artifacts(&journal.target_library_id, &operation_id)?;
        if path != expected_path {
            return Err(String::from("知识库覆盖恢复日志路径与日志身份不匹配。"));
        }
        let knowledge_root = self.knowledge_root();
        fs::create_dir_all(&knowledge_root).map_err(|error| {
            format!(
                "创建知识库根目录用于覆盖日志失败（{}）：{error}",
                knowledge_root.display()
            )
        })?;
        if let Ok(metadata) = fs::symlink_metadata(path)
            && (!metadata.is_file() || metadata_is_link_or_reparse(&metadata))
        {
            return Err(format!(
                "拒绝替换非普通知识库覆盖恢复日志：{}",
                path.display()
            ));
        }
        let bytes = serde_json::to_vec_pretty(journal)
            .map_err(|error| format!("序列化知识库覆盖恢复日志失败：{error}"))?;
        if bytes.len() as u64 > MAX_KNOWLEDGE_PACKAGE_JSON_BYTES {
            return Err(String::from("知识库覆盖恢复日志超过大小上限。"));
        }
        let mut temporary = tempfile::Builder::new()
            .prefix("replacement-journal.")
            .suffix(".tmp")
            .tempfile_in(&knowledge_root)
            .map_err(|error| format!("创建知识库覆盖临时恢复日志失败：{error}"))?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|error| format!("写入并同步知识库覆盖临时恢复日志失败：{error}"))?;
        persist_tempfile_atomically(temporary, path)
            .map_err(|error| format!("提交知识库覆盖恢复日志失败：{error}"))?;
        File::options()
            .read(true)
            .write(true)
            .open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("同步知识库覆盖恢复日志失败：{error}"))
    }
}

/// Returns whether one direct child name is reserved for package extraction cleanup.
/// 返回一个直属子项名称是否保留用于知识库包解压清理。
///
/// # Parameters
/// 参数
/// `name` is an exact file name obtained from the knowledge-root directory iterator.
/// `name` 是从知识库根目录迭代器取得的精确文件名。
///
/// # Returns
/// 返回值
/// Returns `true` only for the reserved prefix followed by a non-empty TempDir suffix.
/// 仅当名称由保留前缀和非空临时目录后缀组成时返回 `true`。
fn is_package_inspection_temp_directory_name(name: &str) -> bool {
    name.strip_prefix(KNOWLEDGE_PACKAGE_INSPECTION_TEMP_PREFIX)
        .is_some_and(|suffix| !suffix.is_empty())
}

/// Validates a replacement journal and returns its canonical operation UUID.
/// 校验一份覆盖恢复日志并返回其规范操作 UUID。
///
/// # Parameters
/// 参数
/// `journal` is untrusted durable JSON read during startup recovery or about to be written.
/// `journal` 是启动恢复期间读取或即将写入的不可信持久化 JSON。
///
/// # Returns
/// 返回值
/// Returns the operation UUID encoded in the exact backup name after every invariant passes.
/// 在全部不变量通过后返回精确备份名称中编码的操作 UUID。
fn validate_replacement_journal(journal: &KnowledgeReplacementJournal) -> Result<String, String> {
    if journal.version != KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION {
        return Err(format!(
            "知识库覆盖恢复日志版本不受支持：{}",
            journal.version
        ));
    }
    validate_library_id(&journal.target_library_id)?;
    if journal.previous.id != journal.target_library_id {
        return Err(String::from("知识库覆盖恢复日志的旧注册身份不匹配。"));
    }
    if journal.previous.lifecycle_state != "active" {
        return Err(String::from("知识库覆盖恢复日志的旧知识库状态无效。"));
    }
    validate_library_name(&journal.previous.name)?;
    if !matches!(
        journal.phase.as_str(),
        KNOWLEDGE_REPLACEMENT_PHASE_PREPARED | KNOWLEDGE_REPLACEMENT_PHASE_COMMITTED
    ) {
        return Err(format!("知识库覆盖恢复日志阶段无效：{}", journal.phase));
    }
    let backup_prefix = format!(".replace-{}-", journal.target_library_id);
    let operation_id = journal
        .backup_directory_name
        .strip_prefix(&backup_prefix)
        .ok_or_else(|| String::from("知识库覆盖恢复日志的备份目录名称无效。"))?;
    validate_library_id(operation_id)?;
    if journal.backup_directory_name != format!("{backup_prefix}{operation_id}") {
        return Err(String::from("知识库覆盖恢复日志的备份目录名称不规范。"));
    }
    Ok(operation_id.to_string())
}

/// Converts one public package count to the signed SQLite representation.
/// 将一个公开包计数转换为 SQLite 有符号表示。
fn checked_package_count(value: u64, field: &str) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| format!("知识库包字段超出 SQLite 范围：{field}"))
}

#[cfg(test)]
mod tests;
