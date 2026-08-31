//! Shared controller bindings and host-owned knowledge storage artifacts.
//! 共享控制器绑定与宿主持有的知识库存储产物。

use super::super::schema::library_schema_script;
use super::{APP_SPACE_ID, KNOWLEDGE_SCHEMA_VERSION, KnowledgeBaseRuntime, MAX_SEARCH_LIMIT};
use crate::runtime_assets;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use vldb_controller_client::{
    ControllerLanceDbEnableRequest, ControllerSqliteEnableRequest, ControllerSqliteValue,
    SpaceKind, SpaceRegistration,
};

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
    /// Initializes the physical directory, manifest, controller binding, and authoritative schema.
    /// 初始化物理目录、清单、控制器绑定与权威结构。
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
        fs::create_dir_all(&root)
            .map_err(|error| format!("创建知识库目录失败（{}）：{error}", root.display()))?;
        self.write_manifest_atomic(&KnowledgeManifest {
            library_id: library_id.to_string(),
            name: name.to_string(),
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
        })?;
        let (space_id, binding_id) = self.activate_library_binding(library_id, true).await?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                &space_id,
                &binding_id,
                library_schema_script(library_id),
                Vec::new(),
            )
            .await
            .map_err(|error| format!("初始化知识库 SQLite 结构失败：{error}"))?;
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
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
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
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
    pub(super) async fn ensure_display_binding(
        &self,
        library_id: &str,
    ) -> Result<(String, String), String> {
        let identities = self.activate_display_binding(library_id, false).await?;
        self.validate_display_bound_metadata(library_id, &identities.0, &identities.1)
            .await?;
        Ok(identities)
    }

    /// Serializes content binding activation and reuses the controller-shared active cache.
    /// 串行化内容绑定激活并复用控制器共享的活动缓存。
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
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
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
                let (space_id, binding_id) = self.library_binding_identity(library_id)?;
                self.controller
                    .client
                    .disable_sqlite(&space_id, &binding_id)
                    .await
                    .map_err(|error| format!("停用知识库 SQLite 绑定失败：{error}"))?;
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
        let (space_id, binding_id) = self.display_binding_identity(library_id)?;
        self.controller
            .client
            .disable_sqlite(&space_id, &binding_id)
            .await
            .map_err(|error| format!("停用知识库显示库 SQLite 绑定失败：{error}"))?;
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
        let (space_id, binding_id) = self.vector_binding_identity(library_id)?;
        self.controller
            .client
            .disable_lancedb(&space_id, &binding_id)
            .await
            .map_err(|error| format!("停用知识库 LanceDB 绑定失败：{error}"))?;
        self.active_vector_bindings.lock().await.remove(library_id);
        Ok(())
    }

    /// Serializes LanceDB binding activation and reuses the controller-shared active cache.
    /// 串行化 LanceDB 绑定激活并复用控制器共享的活动缓存。
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
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
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
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
    pub(super) async fn enable_vector_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<(String, String), String> {
        let (space_id, binding_id) = self.vector_binding_identity(library_id)?;
        let root = self.library_root(library_id)?;
        let lancedb_root = root.join("lancedb");
        match fs::symlink_metadata(&lancedb_root) {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
                    return Err(format!(
                        "知识库 LanceDB 目录不是可信普通目录：{}",
                        lancedb_root.display()
                    ));
                }
            }
            Err(error)
                if allow_database_creation && error.kind() == std::io::ErrorKind::NotFound =>
            {
                fs::create_dir_all(&lancedb_root).map_err(|create_error| {
                    format!(
                        "创建知识库 LanceDB 目录失败（{}）：{create_error}",
                        lancedb_root.display()
                    )
                })?;
            }
            Err(error) => {
                return Err(format!(
                    "读取知识库 LanceDB 目录失败（{}）：{error}",
                    lancedb_root.display()
                ));
            }
        }
        self.controller
            .client
            .attach_space(SpaceRegistration {
                space_id: space_id.clone(),
                space_label: format!("Knowledge {library_id}"),
                space_kind: SpaceKind::User,
                space_root: runtime_assets::native_host_path_text_from_path(&root),
            })
            .await
            .map_err(|error| format!("附着知识库 LanceDB 控制器空间失败：{error}"))?;
        self.controller
            .client
            .enable_lancedb(ControllerLanceDbEnableRequest {
                space_id: space_id.clone(),
                binding_id: binding_id.clone(),
                default_db_path: runtime_assets::native_host_path_text_from_path(&lancedb_root),
                db_root: Some(runtime_assets::native_host_path_text_from_path(
                    &lancedb_root,
                )),
                read_consistency_interval_ms: Some(250),
                max_upsert_payload: 16 * 1024 * 1024,
                max_search_limit: MAX_SEARCH_LIMIT as usize,
                max_concurrent_requests: 16,
            })
            .await
            .map_err(|error| format!("启用知识库 LanceDB 失败：{error}"))?;
        Ok((space_id, binding_id))
    }

    /// Serializes display binding activation and reuses the controller-shared active cache.
    /// 串行化显示库绑定激活并复用控制器共享的活动缓存。
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
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
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
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
    async fn enable_display_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<(String, String), String> {
        let (space_id, binding_id) = self.display_binding_identity(library_id)?;
        let root = self.library_root(library_id)?;
        let database_path = root.join("display.sqlite3");
        match fs::symlink_metadata(&database_path) {
            Ok(metadata) => {
                if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
                    return Err(format!(
                        "知识库显示库不是可信普通文件：{}",
                        database_path.display()
                    ));
                }
            }
            Err(error)
                if allow_database_creation && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "读取知识库显示库失败（{}）：{error}",
                    database_path.display()
                ));
            }
        }
        self.controller
            .client
            .attach_space(SpaceRegistration {
                space_id: space_id.clone(),
                space_label: format!("Knowledge {library_id}"),
                space_kind: SpaceKind::User,
                space_root: runtime_assets::native_host_path_text_from_path(&root),
            })
            .await
            .map_err(|error| format!("附着知识库显示库控制器空间失败：{error}"))?;
        self.controller
            .client
            .enable_sqlite(ControllerSqliteEnableRequest {
                space_id: space_id.clone(),
                binding_id: binding_id.clone(),
                db_path: runtime_assets::native_host_path_text_from_path(&database_path),
                busy_timeout_ms: 10_000,
                ..ControllerSqliteEnableRequest::default()
            })
            .await
            .map_err(|error| format!("启用知识库显示库 SQLite 失败：{error}"))?;
        Ok((space_id, binding_id))
    }

    /// Verifies display metadata belongs to the requested library and schema version.
    /// 校验显示库元数据属于目标资料库且结构版本一致。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `space_id` is the already derived controller space identity.
    /// `space_id` 是已经派生的控制器空间身份。
    /// `binding_id` is the exact SQLite binding within that controller space.
    /// `binding_id` 是该控制器空间内的精确 SQLite 绑定。
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
            .ok_or_else(|| String::from("知识库显示库元数据缺失。"))?;
        if row.library_id != library_id
            || row.schema_version != KNOWLEDGE_SCHEMA_VERSION.to_string()
        {
            return Err(format!(
                "知识库显示库身份或结构版本不匹配：library={}, schema={}",
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
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
    async fn enable_library_binding(
        &self,
        library_id: &str,
        allow_database_creation: bool,
    ) -> Result<(String, String), String> {
        let (space_id, binding_id) = self.library_binding_identity(library_id)?;
        let root = self.library_root(library_id)?;
        let database_path = root.join("content.sqlite3");
        self.validate_library_artifacts(
            library_id,
            &root,
            &database_path,
            allow_database_creation,
        )?;
        self.controller
            .client
            .attach_space(SpaceRegistration {
                space_id: space_id.clone(),
                space_label: format!("Knowledge {library_id}"),
                space_kind: SpaceKind::User,
                space_root: runtime_assets::native_host_path_text_from_path(&root),
            })
            .await
            .map_err(|error| format!("附着知识库控制器空间失败：{error}"))?;
        self.controller
            .client
            .enable_sqlite(ControllerSqliteEnableRequest {
                space_id: space_id.clone(),
                binding_id: binding_id.clone(),
                db_path: runtime_assets::native_host_path_text_from_path(&database_path),
                busy_timeout_ms: 10_000,
                ..ControllerSqliteEnableRequest::default()
            })
            .await
            .map_err(|error| format!("启用知识库 SQLite 失败：{error}"))?;
        Ok((space_id, binding_id))
    }

    /// Verifies that an enabled database belongs to the requested library and schema version.
    /// 验证已启用数据库属于请求的知识库且结构版本一致。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    /// `space_id` is the already derived controller space identity.
    /// `space_id` 是已经派生的控制器空间身份。
    /// `binding_id` is the exact SQLite binding within that controller space.
    /// `binding_id` 是该控制器空间内的精确 SQLite 绑定。
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
            .ok_or_else(|| String::from("知识库数据库元数据缺失。"))?;
        if row.library_id != library_id
            || row.schema_version != KNOWLEDGE_SCHEMA_VERSION.to_string()
        {
            return Err(format!(
                "知识库数据库身份或结构版本不匹配：library={}, schema={}",
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
    /// `space_id` is the already derived controller space identity.
    /// `space_id` 是已经派生的控制器空间身份。
    /// `binding_id` is the exact SQLite binding within that controller space.
    /// `binding_id` 是该控制器空间内的精确 SQLite 绑定。
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
                "知识库包 SQLite quick_check 未通过：{}",
                if diagnostics.is_empty() {
                    "未返回诊断"
                } else {
                    diagnostics.as_str()
                }
            ));
        }
        Ok(())
    }

    /// Validates the manifest, canonical root, and database file before controller attachment.
    /// 在控制器附着前校验清单、规范根目录与数据库文件。
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
        let root_metadata = fs::symlink_metadata(root)
            .map_err(|error| format!("读取知识库目录失败（{}）：{error}", root.display()))?;
        if !root_metadata.is_dir() || metadata_is_link_or_reparse(&root_metadata) {
            return Err(format!("知识库目录不是可信普通目录：{}", root.display()));
        }
        let canonical_knowledge_root = fs::canonicalize(self.knowledge_root())
            .map_err(|error| format!("规范化知识库根目录失败：{error}"))?;
        let canonical_root = fs::canonicalize(root)
            .map_err(|error| format!("规范化知识库目录失败（{}）：{error}", root.display()))?;
        if canonical_root.parent() != Some(canonical_knowledge_root.as_path())
            || canonical_root.file_name().and_then(|value| value.to_str()) != Some(library_id)
        {
            return Err(format!(
                "知识库目录超出宿主派生边界：{}",
                canonical_root.display()
            ));
        }
        let manifest_path = root.join("manifest.json");
        let manifest_metadata = fs::symlink_metadata(&manifest_path).map_err(|error| {
            format!("读取知识库清单失败（{}）：{error}", manifest_path.display())
        })?;
        if !manifest_metadata.is_file() || metadata_is_link_or_reparse(&manifest_metadata) {
            return Err(format!(
                "知识库清单不是可信普通文件：{}",
                manifest_path.display()
            ));
        }
        let manifest: KnowledgeManifest = serde_json::from_slice(
            &fs::read(&manifest_path).map_err(|error| format!("读取知识库清单失败：{error}"))?,
        )
        .map_err(|error| format!("解析知识库清单失败：{error}"))?;
        if manifest.library_id != library_id || manifest.schema_version != KNOWLEDGE_SCHEMA_VERSION
        {
            return Err(String::from("知识库清单身份或结构版本不匹配。"));
        }
        match fs::symlink_metadata(database_path) {
            Ok(database_metadata)
                if database_metadata.is_file()
                    && !metadata_is_link_or_reparse(&database_metadata) => {}
            Ok(_) => {
                return Err(format!(
                    "知识库数据库不是可信普通文件：{}",
                    database_path.display()
                ));
            }
            Err(error)
                if allow_database_creation && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "读取知识库数据库失败（{}）：{error}",
                    database_path.display()
                ));
            }
        }
        Ok(())
    }

    /// Returns validated controller identities derived only from the library UUID.
    /// 返回仅从知识库 UUID 派生的已验证控制器身份。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
    pub(super) fn library_binding_identity(
        &self,
        library_id: &str,
    ) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        Ok((
            format!("knowledge-{library_id}"),
            format!(
                "knowledge-{}-{library_id}",
                self.controller.sqlite_binding_id
            ),
        ))
    }

    /// Returns the stable controller identities for one derived display database.
    /// 返回一个派生显示库数据库使用的稳定控制器身份。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
    fn display_binding_identity(&self, library_id: &str) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        Ok((
            format!("knowledge-{library_id}"),
            format!(
                "knowledge-display-{}-{library_id}",
                self.controller.sqlite_binding_id
            ),
        ))
    }

    /// Returns the stable controller identities for one derived vector backend.
    /// 返回一个派生向量后端使用的稳定控制器身份。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical lowercase UUID of the physical library.
    /// `library_id` 是物理知识库的规范小写 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact space and backend binding identities, or the validation/controller error.
    /// 返回精确空间与后端绑定身份，或校验/控制器错误。
    pub(super) fn vector_binding_identity(
        &self,
        library_id: &str,
    ) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        Ok((
            format!("knowledge-{library_id}"),
            format!(
                "knowledge-vector-{}-{library_id}",
                self.controller.sqlite_binding_id
            ),
        ))
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
        self.controller.data_root.join("knowledge-bases")
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
            return Err(String::from("知识库路径包含非法父目录组件。"));
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
            return Err(String::from("宿主目录删除名称不是单一普通路径组件。"));
        }
        let knowledge_root = self.knowledge_root();
        if path != knowledge_root.join(expected_name) {
            return Err(format!(
                "宿主目录删除路径与预期名称不匹配：{}",
                path.display()
            ));
        }
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "读取待删除宿主目录失败（{}）：{error}",
                    path.display()
                ));
            }
        };
        if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
            return Err(format!("拒绝删除非普通宿主目录：{}", path.display()));
        }
        let canonical_knowledge_root = fs::canonicalize(&knowledge_root)
            .map_err(|error| format!("规范化知识库根目录失败：{error}"))?;
        let canonical_path =
            fs::canonicalize(path).map_err(|error| format!("规范化待删除宿主目录失败：{error}"))?;
        if canonical_path.parent() != Some(canonical_knowledge_root.as_path())
            || canonical_path.file_name().and_then(|value| value.to_str()) != Some(expected_name)
        {
            return Err(format!(
                "待删除宿主目录超出知识库根边界：{}",
                canonical_path.display()
            ));
        }
        fs::remove_dir_all(&canonical_path).map_err(|error| {
            format!(
                "删除宿主资料目录失败（{}）：{error}",
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
        fs::create_dir_all(&root)
            .map_err(|error| format!("创建知识库目录失败（{}）：{error}", root.display()))?;
        let path = root.join("manifest.json");
        let bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|error| format!("序列化知识库清单失败：{error}"))?;
        if let Ok(metadata) = fs::symlink_metadata(&path)
            && (!metadata.is_file() || metadata_is_link_or_reparse(&metadata))
        {
            return Err(format!("拒绝替换非普通知识库清单：{}", path.display()));
        }
        let mut temporary = tempfile::Builder::new()
            .prefix("manifest.")
            .suffix(".tmp")
            .tempfile_in(&root)
            .map_err(|error| format!("创建知识库临时清单失败：{error}"))?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|error| format!("写入并同步知识库临时清单失败：{error}"))?;
        persist_tempfile_atomically(temporary, &path)
            .map_err(|error| format!("提交知识库清单失败：{error}"))
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
    /// Returns the decoded typed rows, or the controller/decoding error.
    /// 返回已解码的类型化记录，或控制器/解码错误。
    pub(super) async fn query_app_rows<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: Vec<ControllerSqliteValue>,
    ) -> Result<Vec<T>, String> {
        let result = self
            .controller
            .client
            .query_sqlite_json_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                sql,
                params,
            )
            .await
            .map_err(|error| format!("查询知识库中央注册表失败：{error}"))?;
        decode_rows(&result.json_data)
    }

    /// Queries typed rows from one isolated library database.
    /// 从一个隔离知识库数据库查询类型化记录。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already derived controller space identity.
    /// `space_id` 是已经派生的控制器空间身份。
    /// `binding_id` is the exact SQLite binding within that controller space.
    /// `binding_id` 是该控制器空间内的精确 SQLite 绑定。
    /// `sql` is the existing parameterized SQL statement.
    /// `sql` 是现有参数化 SQL 语句。
    /// `params` is the ordered typed SQL parameter values.
    /// `params` 是按顺序排列的类型化 SQL 参数值。
    ///
    /// # Returns
    /// 返回值
    /// Returns the decoded typed rows, or the controller/decoding error.
    /// 返回已解码的类型化记录，或控制器/解码错误。
    pub(super) async fn query_library_rows<T: DeserializeOwned>(
        &self,
        space_id: &str,
        binding_id: &str,
        sql: &str,
        params: Vec<ControllerSqliteValue>,
    ) -> Result<Vec<T>, String> {
        let result = self
            .controller
            .client
            .query_sqlite_json_typed(space_id, binding_id, sql, params)
            .await
            .map_err(|error| format!("查询知识库 SQLite 失败：{error}"))?;
        decode_rows(&result.json_data)
    }

    /// Executes one repeated typed statement against a library database.
    /// 对知识库数据库执行一条重复的类型化语句。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the already derived controller space identity.
    /// `space_id` 是已经派生的控制器空间身份。
    /// `binding_id` is the exact SQLite binding within that controller space.
    /// `binding_id` 是该控制器空间内的精确 SQLite 绑定。
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
        space_id: &str,
        binding_id: &str,
        sql: &str,
        items: Vec<Vec<ControllerSqliteValue>>,
        context: &str,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_batch_typed(space_id, binding_id, sql, items)
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
    let parsed = Uuid::parse_str(library_id)
        .map_err(|error| format!("知识库标识符不是有效 UUID：{error}"))?;
    if parsed.to_string() != library_id {
        return Err(String::from("知识库标识符必须使用规范小写 UUID 格式。"));
    }
    Ok(())
}

/// Decodes one controller JSON row set into typed rows.
/// 将一份控制器 JSON 行集解码为类型化记录。
///
/// # Parameters
/// 参数
/// `json` is the controller JSON row-set response to decode into T.
/// `json` 是控制器返回的待解码为 T 的 JSON 行集。
///
/// # Returns
/// 返回值
/// Returns the decoded typed rows, or the controller/decoding error.
/// 返回已解码的类型化记录，或控制器/解码错误。
fn decode_rows<T: DeserializeOwned>(json: &str) -> Result<Vec<T>, String> {
    serde_json::from_str(json).map_err(|error| format!("解码知识库查询结果失败：{error}"))
}

#[cfg(test)]
mod tests;
