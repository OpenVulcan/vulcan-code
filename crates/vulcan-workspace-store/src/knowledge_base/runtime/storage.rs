//! Shared in-process bindings and host-owned knowledge storage artifacts.
//! 共享进程内绑定与宿主持有的知识库存储产物。

use super::super::schema::library_schema_script;
use super::{KNOWLEDGE_SCHEMA_VERSION, KnowledgeBaseRuntime, MAX_SEARCH_LIMIT};
use crate::local_database_runtime::{LanceDbDirectoryBinding, LogicalDatabase, SqliteFileBinding};
use crate::local_sql;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use vulcan_lancedb_engine::engine::LanceDbEngine;
use vulcan_sqlite_engine::runtime::SqliteOpenOptions;
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// Portable manifest stored beside each isolated database.
/// 存储在每个隔离数据库旁的可移植清单。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct KnowledgeManifest {
    /// Immutable library identifier.
    /// 不可变知识库标识符。
    pub(super) library_id: String,
    /// User-visible name copied for diagnostics and exportability.
    /// 为诊断和可导出性复制的用户可见名称。
    pub(super) name: String,
    /// Per-library schema version.
    /// 单知识库结构版本。
    pub(super) schema_version: i64,
}

impl KnowledgeBaseRuntime {
    /// Initializes the physical directory, manifest, host binding, and authoritative schema.
    /// 初始化物理目录、清单、宿主绑定与权威结构。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `name` is the diagnostic display name copied into the manifest.
    /// `name` 是写入清单的诊断显示名称。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub(super) async fn initialize_library_storage(
        &self,
        library_id: &str,
        name: &str,
    ) -> Result<(), String> {
        let root = self.library_root(library_id)?;
        fs::create_dir_all(&root).map_err(|error| {
            format!(
                "Failed to create the knowledge directory ({}): {error}",
                root.display()
            )
        })?;
        self.write_manifest_atomic(&KnowledgeManifest {
            library_id: library_id.to_string(),
            name: name.to_string(),
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
        })?;
        let (space_id, binding_id) = self.activate_library_binding(library_id, true).await?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            library_schema_script(library_id),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("Failed to initialize the knowledge SQLite schema: {error}"))?;
        self.validate_bound_library_metadata(library_id, &space_id, &binding_id)
            .await?;
        Ok(())
    }

    /// Attaches one stable user space and enables its isolated SQLite binding.
    /// 附着一个稳定用户空间并启用其隔离 SQLite 绑定。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    pub(super) async fn ensure_library_binding(
        &self,
        library_id: &str,
    ) -> Result<(String, String), String> {
        let identities = self.activate_library_binding(library_id, false).await?;
        self.validate_bound_library_metadata(library_id, &identities.0, &identities.1)
            .await?;
        Ok(identities)
    }

    /// Ensures the derived display SQLite binding is active without re-enabling it repeatedly.
    /// 确保派生显示库 SQLite 绑定已激活，避免重复启用同一文件锁。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    pub(super) async fn ensure_display_binding(
        &self,
        library_id: &str,
    ) -> Result<(String, String), String> {
        let identities = self.activate_display_binding(library_id, false).await?;
        self.validate_display_bound_metadata(library_id, &identities.0, &identities.1)
            .await?;
        Ok(identities)
    }

    /// Serializes content binding activation and reuses the host-shared active cache.
    /// 串行化内容绑定激活并复用宿主共享的活动缓存。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing backend artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    pub(super) async fn activate_library_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        let _activation_guard = self.binding_activation_lock.lock().await;
        if self.active_bindings.lock().await.contains(library_id) {
            return self.library_binding_identity(library_id);
        }
        let identities = self
            .enable_library_binding(library_id, allow_database_creation)
            .await?;
        self.active_bindings
            .lock()
            .await
            .insert(library_id.to_string());
        Ok(identities)
    }

    /// Releases content and derived bindings before cleanup, export, removal, or replacement.
    /// 在清理、导出、删除或替换前释放内容与派生绑定。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub(super) async fn release_library_binding(&self, library_id: &str) -> Result<(), String> {
        {
            let _activation_guard = self.binding_activation_lock.lock().await;
            if self.active_bindings.lock().await.contains(library_id) {
                // The boundary waits for accepted SQLite work before the handle and its file lock are removed.
                // 该边界会等待已接纳的 SQLite 工作完成，再移除句柄及其文件锁。
                let (_space_id, binding_id) = self.library_binding_identity(library_id)?;
                let binding = self.library_database_binding(library_id, false)?;
                local_sql::close_database_file_async(&self.host_runtime.local, &binding)
                    .await
                    .map_err(|error| {
                        format!("Failed to close the knowledge content database: {error}")
                    })?;
                self.host_runtime
                    .local
                    .unregister_named_binding(&binding_id, &binding)
                    .map_err(|error| {
                        format!("Failed to unregister the knowledge content binding: {error}")
                    })?;
                self.active_bindings.lock().await.remove(library_id);
            }
        }
        self.release_display_binding(library_id).await?;
        self.release_vector_binding(library_id).await
    }

    /// Disables one active derived display binding after a library is removed or replaced.
    /// 删除或替换资料库后停用一个活动的派生显示库绑定。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    async fn release_display_binding(&self, library_id: &str) -> Result<(), String> {
        let _activation_guard = self.binding_activation_lock.lock().await;
        if !self
            .active_display_bindings
            .lock()
            .await
            .contains(library_id)
        {
            return Ok(());
        }
        let (_space_id, binding_id) = self.display_binding_identity(library_id)?;
        let binding = self.display_database_binding(library_id, false)?;
        local_sql::close_database_file_async(&self.host_runtime.local, &binding)
            .await
            .map_err(|error| format!("Failed to close the knowledge display database: {error}"))?;
        self.host_runtime
            .local
            .unregister_named_binding(&binding_id, &binding)
            .map_err(|error| {
                format!("Failed to unregister the knowledge display binding: {error}")
            })?;
        self.active_display_bindings.lock().await.remove(library_id);
        Ok(())
    }

    /// Disables one active derived LanceDB binding after library removal or replacement.
    /// 删除或替换资料库后停用一个活动的派生 LanceDB 绑定。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    async fn release_vector_binding(&self, library_id: &str) -> Result<(), String> {
        let _activation_guard = self.binding_activation_lock.lock().await;
        if !self
            .active_vector_bindings
            .lock()
            .await
            .contains(library_id)
        {
            return Ok(());
        }
        let (_space_id, binding_id) = self.vector_binding_identity(library_id)?;
        let binding = self.vector_directory_binding(library_id, false)?;
        self.host_runtime
            .local
            .close_lance_directory(&binding)
            .map_err(|error| format!("Failed to close the knowledge LanceDB directory: {error}"))?;
        self.host_runtime
            .local
            .unregister_named_lance_binding(&binding_id, &binding)
            .map_err(|error| {
                format!("Failed to unregister the knowledge LanceDB binding: {error}")
            })?;
        self.active_vector_bindings.lock().await.remove(library_id);
        Ok(())
    }

    /// Serializes LanceDB binding activation and reuses the host-shared active cache.
    /// 串行化 LanceDB 绑定激活并复用宿主共享的活动缓存。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing backend artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    pub(super) async fn activate_vector_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        let _activation_guard = self.binding_activation_lock.lock().await;
        if self
            .active_vector_bindings
            .lock()
            .await
            .contains(library_id)
        {
            return self.vector_binding_identity(library_id);
        }
        let identities = self
            .enable_vector_binding(library_id, allow_database_creation)
            .await?;
        self.active_vector_bindings
            .lock()
            .await
            .insert(library_id.to_string());
        Ok(identities)
    }

    /// Validates and enables the host-derived LanceDB directory for one library.
    /// 校验并启用一个资料库宿主派生的 LanceDB 目录。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing backend artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    pub(super) async fn enable_vector_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<(String, String), String> {
        let (space_id, binding_id) = self.vector_binding_identity(library_id)?;
        // Register the derived directory without opening it eagerly. Consumers resolve the binding by
        // identity and the host opens the engine only for the lifetime of the actual operation.
        // 登记派生目录但不提前打开。消费者按身份解析绑定，宿主仅在真实操作生命周期内打开引擎。
        let directory = self.vector_directory_binding(library_id, allow_database_creation)?;
        self.host_runtime
            .local
            .register_named_lance_binding(&binding_id, &directory)
            .map_err(|error| {
                format!("Failed to register the knowledge LanceDB binding: {error}")
            })?;
        Ok((space_id, binding_id))
    }

    /// Serializes display binding activation and reuses the host-shared active cache.
    /// 串行化显示库绑定激活并复用宿主共享的活动缓存。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing backend artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    pub(super) async fn activate_display_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        let _activation_guard = self.binding_activation_lock.lock().await;
        if self
            .active_display_bindings
            .lock()
            .await
            .contains(library_id)
        {
            return self.display_binding_identity(library_id);
        }
        let identities = self
            .enable_display_binding(library_id, allow_database_creation)
            .await?;
        self.active_display_bindings
            .lock()
            .await
            .insert(library_id.to_string());
        Ok(identities)
    }

    /// Validates the display artifact and enables its isolated SQLite backend.
    /// 校验显示库存储产物并启用隔离 SQLite 后端。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing backend artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    async fn enable_display_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<(String, String), String> {
        let (space_id, binding_id) = self.display_binding_identity(library_id)?;
        // Register the derived database path without holding an eager connection. Identity-addressed
        // helpers open it through the host engine only for the lifetime of each operation.
        // 登记派生数据库路径但不持有提前打开的连接。按身份寻址的助手仅在每次操作生命周期内通过宿主引擎打开它。
        let database = self.display_database_binding(library_id, allow_database_creation)?;
        self.register_library_binding(&binding_id, &database)?;
        Ok((space_id, binding_id))
    }

    /// Verifies display metadata belongs to the requested library and schema version.
    /// 校验显示库元数据属于目标资料库且结构版本一致。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `space_id` is the already derived storage-space identity.
    /// `space_id` 是已经派生的存储空间身份。
    /// `binding_id` is the exact SQLite binding within that storage space.
    /// `binding_id` 是该存储空间内的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub(super) async fn validate_display_bound_metadata(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
    ) -> Result<(), String> {
        #[derive(Deserialize)]
        struct DisplayBoundMetadataRow {
            /// Stored display library identifier.
            /// 已存储的显示库资料库标识符。
            library_id: String,
            /// Stored display schema version.
            /// 已存储的显示库结构版本。
            schema_version: String,
        }
        let rows: Vec<DisplayBoundMetadataRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT MAX(CASE WHEN key = 'library_id' THEN value END) AS library_id, MAX(CASE WHEN key = 'schema_version' THEN value END) AS schema_version FROM display_metadata;",
                Vec::new(),
            )
            .await?;
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| String::from("The knowledge display-library metadata is missing."))?;
        if row.library_id != library_id
            || row.schema_version != KNOWLEDGE_SCHEMA_VERSION.to_string()
        {
            return Err(format!(
                "The knowledge display-library identity or schema version does not match: library={}, schema={}",
                row.library_id, row.schema_version
            ));
        }
        Ok(())
    }

    /// Validates storage artifacts and enables one isolated SQLite binding.
    /// 校验存储产物并启用一个隔离 SQLite 绑定。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing backend artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    async fn enable_library_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<(String, String), String> {
        let (space_id, binding_id) = self.library_binding_identity(library_id)?;
        // Register the derived database path without holding an eager connection. Identity-addressed
        // helpers open it through the host engine only for the lifetime of each operation.
        // 登记派生数据库路径但不持有提前打开的连接。按身份寻址的助手仅在每次操作生命周期内通过宿主引擎打开它。
        let database = self.library_database_binding(library_id, allow_database_creation)?;
        self.register_library_binding(&binding_id, &database)?;
        Ok((space_id, binding_id))
    }

    /// Verifies that an enabled database belongs to the requested library and schema version.
    /// 验证已启用数据库属于请求的知识库且结构版本一致。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `space_id` is the already derived storage-space identity.
    /// `space_id` 是已经派生的存储空间身份。
    /// `binding_id` is the exact SQLite binding within that storage space.
    /// `binding_id` 是该存储空间内的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub(super) async fn validate_bound_library_metadata(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
    ) -> Result<(), String> {
        #[derive(Deserialize)]
        struct BoundMetadataRow {
            /// Stored immutable library identifier.
            /// 已存储的不可变知识库标识符。
            library_id: String,
            /// Stored authoritative schema version.
            /// 已存储的权威结构版本。
            schema_version: String,
        }
        let rows: Vec<BoundMetadataRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                r#"SELECT
                    MAX(CASE WHEN key = 'library_id' THEN value END) AS library_id,
                    MAX(CASE WHEN key = 'schema_version' THEN value END) AS schema_version
                FROM knowledge_metadata;"#,
                Vec::new(),
            )
            .await?;
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| String::from("Knowledge database metadata is missing."))?;
        if row.library_id != library_id
            || row.schema_version != KNOWLEDGE_SCHEMA_VERSION.to_string()
        {
            return Err(format!(
                "Knowledge database identity or schema version mismatch: library={}, schema={}",
                row.library_id, row.schema_version
            ));
        }
        Ok(())
    }

    /// Runs SQLite quick_check and rejects packages containing structural database corruption.
    /// 运行 SQLite quick_check，并拒绝包含结构性数据库损坏的知识库包。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already derived storage-space identity.
    /// `space_id` 是已经派生的存储空间身份。
    /// `binding_id` is the exact SQLite binding within that storage space.
    /// `binding_id` 是该存储空间内的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub(super) async fn verify_sqlite_quick_check(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<(), String> {
        /// One diagnostic row returned by SQLite PRAGMA quick_check.
        /// SQLite PRAGMA quick_check 返回的一条诊断记录。
        #[derive(Deserialize)]
        struct QuickCheckRow {
            /// Exact SQLite diagnostic, equal to ok only for a successful check.
            /// 精确 SQLite 诊断，仅成功检查时等于 ok。
            quick_check: String,
        }
        // rows contains one ok row for a healthy database or one or more corruption diagnostics.
        // rows 对健康数据库包含一条 ok 记录，否则包含一条或多条损坏诊断。
        let rows: Vec<QuickCheckRow> = self
            .query_library_rows(space_id, binding_id, "PRAGMA quick_check;", Vec::new())
            .await?;
        if rows.len() != 1 || rows[0].quick_check != "ok" {
            // diagnostics bounds corruption detail before returning it to the caller.
            // diagnostics 在向调用方返回前限制损坏诊断数量。
            let diagnostics = rows
                .into_iter()
                .map(|row| row.quick_check)
                .take(8)
                .collect::<Vec<_>>()
                .join("；");
            return Err(format!(
                "The knowledge package SQLite quick_check failed: {}",
                if diagnostics.is_empty() {
                    "no diagnostics returned"
                } else {
                    diagnostics.as_str()
                }
            ));
        }
        Ok(())
    }

    /// Validates the manifest, canonical root, and database file before host attachment.
    /// 在宿主附着前校验清单、规范根目录与数据库文件。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing backend artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    /// `root` is the host-derived library directory to validate.
    /// `root` 是待校验的宿主派生知识库目录。
    /// `database_path` is the expected authoritative SQLite file.
    /// `database_path` 是预期的权威 SQLite 文件。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    fn validate_library_artifacts(
        &self,
        library_id: &str,
        root: &Path,
        database_path: &Path,
        allow_database_creation: bool,
    ) -> Result<(), String> {
        let root_metadata = fs::symlink_metadata(root).map_err(|error| {
            format!(
                "Failed to read the knowledge directory ({}): {error}",
                root.display()
            )
        })?;
        if !root_metadata.is_dir() || metadata_is_link_or_reparse(&root_metadata) {
            return Err(format!(
                "The knowledge directory is not a trusted regular directory: {}",
                root.display()
            ));
        }
        let canonical_knowledge_root = fs::canonicalize(self.knowledge_root())
            .map_err(|error| format!("Failed to canonicalize the knowledge root: {error}"))?;
        let canonical_root = fs::canonicalize(root).map_err(|error| {
            format!(
                "Failed to canonicalize the knowledge directory ({}): {error}",
                root.display()
            )
        })?;
        if canonical_root.parent() != Some(canonical_knowledge_root.as_path())
            || canonical_root.file_name().and_then(|value| value.to_str()) != Some(library_id)
        {
            return Err(format!(
                "The knowledge directory is outside the host-derived boundary: {}",
                canonical_root.display()
            ));
        }
        let manifest_path = root.join("manifest.json");
        let manifest_metadata = fs::symlink_metadata(&manifest_path).map_err(|error| {
            format!(
                "Failed to read the knowledge manifest ({}): {error}",
                manifest_path.display()
            )
        })?;
        if !manifest_metadata.is_file() || metadata_is_link_or_reparse(&manifest_metadata) {
            return Err(format!(
                "The knowledge manifest is not a trusted regular file: {}",
                manifest_path.display()
            ));
        }
        let manifest: KnowledgeManifest = serde_json::from_slice(
            &fs::read(&manifest_path)
                .map_err(|error| format!("Failed to read the knowledge manifest: {error}"))?,
        )
        .map_err(|error| format!("Failed to parse the knowledge manifest: {error}"))?;
        if manifest.library_id != library_id || manifest.schema_version != KNOWLEDGE_SCHEMA_VERSION
        {
            return Err(String::from(
                "The knowledge manifest identity or schema version does not match.",
            ));
        }
        match fs::symlink_metadata(database_path) {
            Ok(database_metadata)
                if database_metadata.is_file()
                    && !metadata_is_link_or_reparse(&database_metadata) => {}
            Ok(_) => {
                return Err(format!(
                    "The knowledge database is not a trusted regular file: {}",
                    database_path.display()
                ));
            }
            Err(error)
                if allow_database_creation && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Failed to read the knowledge database ({}): {error}",
                    database_path.display()
                ));
            }
        }
        Ok(())
    }

    /// Returns validated storage identities derived only from the library UUID.
    /// 返回仅从知识库 UUID 派生的已验证存储身份。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    /// Registers a per-library binding under the identity helpers are addressed with.
    /// 以助手寻址所用的身份登记按资料库绑定。
    ///
    /// The identity is generated by the knowledge base itself and the binding is derived from the library
    /// identifier alone, so this needs no open connection. Registering here lets a helper that only
    /// holds the identity reach the engine without eagerly taking the database lock.
    /// 身份由知识库自行生成，绑定仅从资料库标识符派生，因此不需要打开连接。在此登记使只持有身份的助手
    /// 无需提前取得数据库锁即可抵达引擎。
    fn register_library_binding(
        &self,
        binding_id: &str,
        binding: &SqliteFileBinding,
    ) -> Result<(), String> {
        self.host_runtime
            .local
            .register_named_binding(binding_id, binding)
            .map_err(|error| format!("Failed to register the knowledge binding: {error}"))
    }
    /// Resolves the per-library binding an addressing identity names.
    /// 解析寻址身份所指的按资料库绑定。
    ///
    /// Helpers are addressed by the identity pair rather than by a binding, so this is how a helper
    /// reaches the engine. It resolves through the runtime registry without eagerly opening the
    /// database or holding its file lock.
    /// 助手以身份二元组而非绑定来寻址，因此这是助手抵达引擎的方式。它通过运行时注册表解析，
    /// 不会提前打开数据库或持有其文件锁。
    ///
    /// # Parameters / 参数
    /// `binding_id` is the identity the caller was addressed with, as `String` or `&str`.
    /// `binding_id` 是调用方被寻址时所用的身份，形式为 `String` 或 `&str`。
    ///
    /// # Returns / 返回值
    /// Returns the registered binding, or an error naming the identity that was never registered.
    /// 返回已登记的绑定，或指明那个从未被登记的身份的错误。
    pub(super) fn resolve_library_binding(
        &self,
        binding_id: impl AsRef<str>,
    ) -> Result<SqliteFileBinding, String> {
        self.host_runtime
            .local
            .named_binding(binding_id.as_ref())
            .map_err(|error| format!("Failed to resolve the knowledge library binding: {error}"))
    }
    /// Resolves the LanceDB engine for the directory an addressing identity names.
    /// 解析寻址身份所指目录的 LanceDB 引擎。
    ///
    /// The runtime caches one engine per directory, so this is a cheap lookup after the first call and
    /// one directory still has exactly one engine, which the engine's own invariant requires because a
    /// second engine would hold a second set of per-table locks.
    /// 运行时按目录缓存一个引擎，因此首次调用之后这是廉价查找，且一个目录仍恰好只有一个引擎——这是引擎自身
    /// 不变量所要求的，因为第二个引擎会持有第二套表级锁。
    ///
    /// # Parameters / 参数
    /// `binding_id` is the identity the caller was addressed with.
    /// `binding_id` 是调用方被寻址时所用的身份。
    ///
    /// # Returns / 返回值
    /// Returns the cached engine, or an error naming the identity that was never registered.
    /// 返回已缓存的引擎，或指明那个从未被登记的身份的错误。
    pub(super) async fn resolve_vector_engine(
        &self,
        binding_id: impl AsRef<str>,
    ) -> Result<LanceDbEngine, String> {
        let directory = self
            .host_runtime
            .local
            .named_lance_binding(binding_id.as_ref())
            .map_err(|error| format!("Failed to resolve the knowledge vector binding: {error}"))?;
        self.host_runtime
            .local
            .lance_engine(directory)
            .await
            .map_err(|error| format!("Failed to open the knowledge LanceDB engine: {error}"))
    }
    pub(super) fn library_binding_identity(
        &self,
        library_id: &str,
    ) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        Ok((
            format!("knowledge-{library_id}"),
            format!(
                "knowledge-{}-{library_id}",
                self.host_runtime.sqlite_binding_id
            ),
        ))
    }

    /// Returns the stable storage identities for one derived display database.
    /// 返回一个派生显示库数据库使用的稳定存储身份。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    fn display_binding_identity(&self, library_id: &str) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        Ok((
            format!("knowledge-{library_id}"),
            format!(
                "knowledge-display-{}-{library_id}",
                self.host_runtime.sqlite_binding_id
            ),
        ))
    }

    /// Returns the stable storage identities for one derived vector backend.
    /// 返回一个派生向量后端使用的稳定存储身份。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/storage error.
    /// 返回精确空间与后端绑定身份，或校验/存储错误。
    pub(super) fn vector_binding_identity(
        &self,
        library_id: &str,
    ) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        Ok((
            format!("knowledge-{library_id}"),
            format!(
                "knowledge-vector-{}-{library_id}",
                self.host_runtime.sqlite_binding_id
            ),
        ))
    }

    /// Returns the in-process binding for one library's content database.
    /// 返回一个资料库内容数据库的进程内绑定。
    ///
    /// This is the single source of truth for the path and the artifact checks. The host enable
    /// paths below are built on it so that the two backends can never disagree about which file a
    /// library owns, and so that migrating a caller cannot silently change the path it reads.
    ///
    /// 这是路径与产物检查的唯一权威来源。下方宿主启用路径建立在它之上，使两种后端绝不会对某个资料库
    /// 拥有哪个文件产生分歧，也使迁移调用方不会静默改变它所读取的路径。
    ///
    /// # Parameters / 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    ///
    /// # Returns / 返回值
    /// Returns the validated content-database binding, or the validation error.
    /// 返回已验证的内容数据库绑定，或校验错误。
    pub(super) fn library_database_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<SqliteFileBinding, String> {
        let root = self.library_root(library_id)?;
        let database_path = root.join("content.sqlite3");
        self.validate_library_artifacts(
            library_id,
            &root,
            &database_path,
            allow_database_creation,
        )?;
        Ok(library_sqlite_binding(database_path))
    }

    /// Returns the in-process binding for one library's derived display database.
    /// 返回一个资料库派生显示库的进程内绑定。
    ///
    /// # Parameters / 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    ///
    /// # Returns / 返回值
    /// Returns the validated display-database binding, or the validation error.
    /// 返回已验证的显示库数据库绑定，或校验错误。
    pub(super) fn display_database_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<SqliteFileBinding, String> {
        let root = self.library_root(library_id)?;
        let database_path = root.join("display.sqlite3");
        validate_trusted_database_file(&database_path, allow_database_creation)?;
        Ok(library_sqlite_binding(database_path))
    }

    /// Returns the in-process binding for one library's derived vector directory.
    /// 返回一个资料库派生向量目录的进程内绑定。
    ///
    /// # Parameters / 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `allow_database_creation` is whether this explicit creation path may accept a missing artifact.
    /// `allow_database_creation` 是当前明确创建路径是否允许后端产物尚不存在。
    ///
    /// # Returns / 返回值
    /// Returns the validated vector-directory binding, or the validation/creation error.
    /// 返回已验证的向量目录绑定，或校验／创建错误。
    pub(super) fn vector_directory_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<LanceDbDirectoryBinding, String> {
        let root = self.library_root(library_id)?;
        let lancedb_root = root.join("lancedb");
        match fs::symlink_metadata(&lancedb_root) {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
                    return Err(format!(
                        "The knowledge LanceDB directory is not a trusted regular directory: {}",
                        lancedb_root.display()
                    ));
                }
            }
            Err(error)
                if allow_database_creation && error.kind() == std::io::ErrorKind::NotFound =>
            {
                fs::create_dir_all(&lancedb_root).map_err(|create_error| {
                    format!(
                        "Failed to create the knowledge LanceDB directory ({}): {create_error}",
                        lancedb_root.display()
                    )
                })?;
            }
            Err(error) => {
                return Err(format!(
                    "Failed to read the knowledge LanceDB directory ({}): {error}",
                    lancedb_root.display()
                ));
            }
        }
        Ok(vector_directory_binding(lancedb_root))
    }

    /// Returns the immutable knowledge root under the established application data root.
    /// 返回已建立应用数据根目录下的不可变知识库根目录。
    ///
    /// # Parameters
    /// 参数
    /// Uses the shared runtime handle; there are no explicit arguments.
    /// 使用共享运行时句柄，没有显式参数。
    ///
    /// # Returns
    /// 返回值
    /// Returns the established application data root joined with the knowledge directory.
    /// 返回既有应用数据根拼接后的知识库目录。
    pub(super) fn knowledge_root(&self) -> PathBuf {
        self.host_runtime.data_root.join("knowledge-bases")
    }

    /// Derives and validates one physical library root.
    /// 派生并验证一个物理知识库根目录。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the validated host-derived library directory, or an invalid-identity error.
    /// 返回已验证的宿主派生知识库目录，或非法身份错误。
    pub(super) fn library_root(&self, library_id: &str) -> Result<PathBuf, String> {
        validate_library_id(library_id)?;
        let root = self.knowledge_root().join(library_id);
        if root
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(String::from(
                "The knowledge path contains an invalid parent directory component.",
            ));
        }
        Ok(root)
    }

    /// Removes one exact ordinary child directory without following reparse or boundary escapes.
    /// 在不跟随重解析点或越过边界的前提下移除一个精确普通直属目录。
    ///
    /// # Parameters
    /// 参数
    /// `path` is the host-derived child path that may or may not exist.
    /// `path` 是可能存在或不存在的宿主派生子路径。
    /// `expected_name` is the exact single path component owned by the current operation.
    /// `expected_name` 是当前操作拥有的精确单路径组件。
    ///
    /// # Returns
    /// 返回值
    /// Returns success when the path is absent or an exact validated ordinary child was removed.
    /// 当路径不存在或精确校验后的普通直属目录已移除时返回成功。
    pub(super) fn remove_host_owned_child_directory_if_exists(
        &self,
        path: &Path,
        expected_name: &str,
    ) -> Result<(), String> {
        let mut components = Path::new(expected_name).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(String::from(
                "The host directory deletion name is not a single regular path component.",
            ));
        }
        let knowledge_root = self.knowledge_root();
        if path != knowledge_root.join(expected_name) {
            return Err(format!(
                "The host directory deletion path does not match the expected name: {}",
                path.display()
            ));
        }
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "Failed to read the host directory to delete ({}): {error}",
                    path.display()
                ));
            }
        };
        if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
            return Err(format!(
                "Refusing to delete a non-regular host directory: {}",
                path.display()
            ));
        }
        let canonical_knowledge_root = fs::canonicalize(&knowledge_root)
            .map_err(|error| format!("Failed to canonicalize the knowledge root: {error}"))?;
        let canonical_path = fs::canonicalize(path).map_err(|error| {
            format!("Failed to canonicalize the host directory to delete: {error}")
        })?;
        if canonical_path.parent() != Some(canonical_knowledge_root.as_path())
            || canonical_path.file_name().and_then(|value| value.to_str()) != Some(expected_name)
        {
            return Err(format!(
                "The host directory to delete is outside the knowledge root boundary: {}",
                canonical_path.display()
            ));
        }
        fs::remove_dir_all(&canonical_path).map_err(|error| {
            format!(
                "Failed to delete the host data directory ({}): {error}",
                canonical_path.display()
            )
        })
    }

    /// Writes one manifest through a same-directory atomic replacement.
    /// 通过同目录原子替换写入一份清单。
    ///
    /// # Parameters
    /// 参数
    /// `manifest` is the exact library identity, name, and schema version to persist.
    /// `manifest` 是待持久化的精确知识库身份、名称和结构版本。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub(super) fn write_manifest_atomic(&self, manifest: &KnowledgeManifest) -> Result<(), String> {
        let root = self.library_root(&manifest.library_id)?;
        fs::create_dir_all(&root).map_err(|error| {
            format!(
                "Failed to create the knowledge directory ({}): {error}",
                root.display()
            )
        })?;
        let path = root.join("manifest.json");
        let bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|error| format!("Failed to serialize the knowledge manifest: {error}"))?;
        if let Ok(metadata) = fs::symlink_metadata(&path)
            && (!metadata.is_file() || metadata_is_link_or_reparse(&metadata))
        {
            return Err(format!(
                "Refusing to replace a non-regular knowledge manifest: {}",
                path.display()
            ));
        }
        let mut temporary = tempfile::Builder::new()
            .prefix("manifest.")
            .suffix(".tmp")
            .tempfile_in(&root)
            .map_err(|error| {
                format!("Failed to create the temporary knowledge manifest: {error}")
            })?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|error| {
                format!("Failed to write and sync the temporary knowledge manifest: {error}")
            })?;
        persist_tempfile_atomically(temporary, &path)
            .map_err(|error| format!("Failed to commit the knowledge manifest: {error}"))
    }

    /// Queries typed rows from the central application database.
    /// 从中央应用数据库查询类型化记录。
    ///
    /// # Parameters
    /// 参数
    /// `sql` is the existing parameterized SQL statement.
    /// `sql` 是现有参数化 SQL 语句。
    /// `params` is the ordered typed SQL parameter values.
    /// `params` 是按顺序排列的类型化 SQL 参数值。
    ///
    /// # Returns
    /// 返回值
    /// Returns the decoded typed rows, or the storage/decoding error.
    /// 返回已解码的类型化记录，或存储/解码错误。
    pub(super) async fn query_app_rows<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: &[SqliteValue],
    ) -> Result<Vec<T>, String> {
        let result = local_sql::query_json_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
            sql,
            params.to_vec(),
        )
        .await
        .map_err(|error| format!("Failed to query the central knowledge registry: {error}"))?;
        decode_rows(&result.json_data)
    }

    /// Queries typed rows from one isolated library database.
    /// 从一个隔离知识库数据库查询类型化记录。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already derived storage-space identity.
    /// `space_id` 是已经派生的存储空间身份。
    /// `binding_id` is the exact SQLite binding within that storage space.
    /// `binding_id` 是该存储空间内的精确 SQLite 绑定。
    /// `sql` is the existing parameterized SQL statement.
    /// `sql` 是现有参数化 SQL 语句。
    /// `params` is the ordered typed SQL parameter values.
    /// `params` 是按顺序排列的类型化 SQL 参数值。
    ///
    /// # Returns
    /// 返回值
    /// Returns the decoded typed rows, or the storage/decoding error.
    /// 返回已解码的类型化记录，或存储/解码错误。
    pub(super) async fn query_library_rows<T: DeserializeOwned>(
        &self,
        _space_id: &str,
        binding_id: &str,
        sql: &str,
        params: Vec<SqliteValue>,
    ) -> Result<Vec<T>, String> {
        let result = local_sql::query_json_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            sql,
            params,
        )
        .await
        .map_err(|error| format!("Knowledge SQLite query failed: {error}"))?;
        decode_rows(&result.json_data)
    }

    /// Executes one repeated typed statement against a library database.
    /// 对知识库数据库执行一条重复的类型化语句。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already derived storage-space identity.
    /// `space_id` 是已经派生的存储空间身份。
    /// `binding_id` is the exact SQLite binding within that storage space.
    /// `binding_id` 是该存储空间内的精确 SQLite 绑定。
    /// `sql` is the existing parameterized SQL statement.
    /// `sql` 是现有参数化 SQL 语句。
    /// `items` is the ordered parameter rows for repeated statement execution.
    /// `items` 是重复语句执行的有序参数行。
    /// `context` is the operation-specific error prefix.
    /// `context` 是操作专属错误前缀。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub(super) async fn execute_library_batch(
        &self,
        _space_id: &str,
        binding_id: &str,
        sql: &str,
        items: Vec<Vec<SqliteValue>>,
        context: &str,
    ) -> Result<(), String> {
        local_sql::execute_batch_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding_id)?,
            sql,
            items,
        )
        .await
        .map_err(|error| format!("{context}：{error}"))?;
        Ok(())
    }
}

/// Atomically persists one securely created temporary file over a stable destination.
/// 将一个安全创建的临时文件原子持久化并覆盖稳定目标。
///
/// # Parameters
/// 参数
/// `temporary` is a fully written and synchronized same-directory temporary file.
/// `temporary` 是已完整写入并同步的同目录临时文件。
/// `destination` is the stable manifest, recovery-journal, or archive path to create or replace.
/// `destination` 是要创建或替换的稳定清单、恢复日志或归档路径。
///
/// # Returns
/// 返回值
/// Returns success only after the platform-specific atomic persistence operation succeeds.
/// 仅在平台专属原子持久化操作成功后返回成功。
pub(super) fn persist_tempfile_atomically(
    temporary: tempfile::NamedTempFile,
    destination: &Path,
) -> std::io::Result<()> {
    temporary
        .persist(destination)
        .map(|_| ())
        .map_err(|error| error.error)
}

/// Returns whether metadata identifies a symbolic link or a Windows reparse point.
/// 返回元数据是否标识符号链接或 Windows 重解析点。
///
/// # Parameters
/// 参数
/// `metadata` is the filesystem metadata obtained without following links.
/// `metadata` 是未跟随链接获取的文件系统元数据。
///
/// # Returns
/// 返回值
/// Returns whether the entry is a symbolic link or platform reparse point.
/// 返回该条目是否为符号链接或平台重解析点。
pub(super) fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Builds one per-library SQLite binding with the knowledge base's declared options.
/// 使用知识库声明的选项构建一个按资料库的 SQLite 绑定。
///
/// The knowledge base raised `busy_timeout_ms` to 10 s because a knowledge import holds a write
/// transaction while a concurrent reader may still be running; the engine's 5 s default is the
/// application-wide value, not this subsystem's. Passing it explicitly keeps that deliberate
/// difference visible instead of silently reverting it to the default.
///
/// 知识库把 `busy_timeout_ms` 提高到 10 秒，因为一次知识导入持有写事务时仍可能有并发读取；引擎的 5 秒
/// 默认值是应用级取值，而不是本子系统的。显式传入使这一有意差异保持可见，而不是静默回退为默认值。
fn library_sqlite_binding(path: PathBuf) -> SqliteFileBinding {
    SqliteFileBinding {
        path,
        options: SqliteOpenOptions {
            busy_timeout_ms: 10_000,
            ..SqliteOpenOptions::default()
        },
    }
}

/// Builds one per-library LanceDB binding with the knowledge base's declared limits.
/// 使用知识库声明的限制构建一个按资料库的 LanceDB 绑定。
///
/// `MAX_SEARCH_LIMIT` belongs to the knowledge base rather than to the engine's defaults, so it is
/// passed explicitly for the same reason as the SQLite timeout above.
/// `MAX_SEARCH_LIMIT` 属于知识库而非引擎默认值，因此与上述 SQLite 超时同理，被显式传入。
fn vector_directory_binding(directory: PathBuf) -> LanceDbDirectoryBinding {
    LanceDbDirectoryBinding {
        directory,
        read_consistency_interval_ms: Some(250),
        max_upsert_payload: 16 * 1024 * 1024,
        max_search_limit: MAX_SEARCH_LIMIT as usize,
        max_concurrent_requests: 16,
    }
}

/// Requires one derived database file to be absent or a trusted ordinary file.
/// 要求一个派生数据库文件不存在，或是可信的普通文件。
///
/// # Parameters
/// 参数
/// `path` is the host-derived database path that may or may not exist.
/// `path` 是可能存在或不存在的宿主派生数据库路径。
/// `allow_creation` is whether a missing file is acceptable on this explicit creation path.
/// `allow_creation` 是当前明确创建路径上文件缺失是否可接受。
///
/// # Returns
/// 返回值
/// Returns success for a trusted file or an allowed absence, or an explicit trust/read error.
/// 对可信文件或允许的缺失返回成功，否则返回明确的信任／读取错误。
fn validate_trusted_database_file(path: &Path, allow_creation: bool) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
                return Err(format!(
                    "The knowledge derived database is not a trusted regular file: {}",
                    path.display()
                ));
            }
            Ok(())
        }
        Err(error) if allow_creation && error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to read the knowledge derived database ({}): {error}",
            path.display()
        )),
    }
}

/// Validates that a library identifier is one canonical lowercase UUID.
/// 验证知识库标识符是一个规范小写 UUID。
///
/// # Parameters
/// 参数
/// `library_id` is the canonical lowercase UUID of the physical library.
/// `library_id` 是物理知识库的规范小写 UUID。
///
/// # Returns
/// 返回值
/// Returns success after the documented operation completes, or its explicit failure.
/// 返回所述操作完成后的成功结果，或其明确错误。
pub(super) fn validate_library_id(library_id: &str) -> Result<(), String> {
    let parsed = Uuid::parse_str(library_id).map_err(|error| {
        format!("The knowledge library identifier is not a valid UUID: {error}")
    })?;
    if parsed.to_string() != library_id {
        return Err(String::from(
            "The knowledge library identifier must use the canonical lowercase UUID format.",
        ));
    }
    Ok(())
}

/// Decodes one storage-engine JSON row set into typed rows.
/// 将一份存储引擎 JSON 行集解码为类型化记录。
///
/// # Parameters
/// 参数
/// `json` is the storage-engine JSON row-set response to decode into T.
/// `json` 是存储引擎返回的待解码为 T 的 JSON 行集。
///
/// # Returns
/// 返回值
/// Returns the decoded typed rows, or the storage/decoding error.
/// 返回已解码的类型化记录，或存储/解码错误。
fn decode_rows<T: DeserializeOwned>(json: &str) -> Result<Vec<T>, String> {
    serde_json::from_str(json)
        .map_err(|error| format!("Failed to decode the knowledge query result: {error}"))
}

#[cfg(test)]
mod tests;
