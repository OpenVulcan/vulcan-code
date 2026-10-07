//! Authoritative library registry lifecycle, settings, and summary projection.
//! 权威知识库注册生命周期、设置与摘要投影。

use super::super::models::{
    KnowledgeDisplayIndexStatus, KnowledgeLibrarySettings, KnowledgeLibrarySummary,
    KnowledgeModelSnapshot, KnowledgeVectorIndexStatus,
};
use super::CountRow;
use super::{
    KNOWLEDGE_SCHEMA_VERSION, KnowledgeBaseRuntime, KnowledgeManifest, nonnegative_u64,
    unix_epoch_millis, validate_library_id,
};
use super::{int, text};
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use serde::Deserialize;
use uuid::Uuid;
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// One registry row decoded from the application database.
/// 从应用数据库解码的一条知识库注册记录。
#[derive(Debug, Deserialize)]
pub(super) struct LibraryRow {
    /// Immutable library identifier.
    /// 不可变知识库标识符。
    id: String,
    /// User-visible name.
    /// 用户可见名称。
    name: String,
    /// User-visible description.
    /// 用户可见说明。
    description: String,
    /// Durable lifecycle state.
    /// 持久化生命周期状态。
    lifecycle_state: String,
    /// Authoritative document count.
    /// 权威文档数量。
    document_count: i64,
    /// Authoritative chunk count.
    /// 权威切片数量。
    chunk_count: i64,
    /// Derived index state.
    /// 派生索引状态。
    index_status: String,
    /// JSON-encoded per-library settings.
    /// JSON 编码的按资料库配置。
    settings_json: String,
    /// Most recent completed import timestamp.
    /// 最近完成导入时间戳。
    last_completed_import_at_ms: Option<i64>,
    /// Creation timestamp.
    /// 创建时间戳。
    created_at_ms: i64,
    /// Last update timestamp.
    /// 最近更新时间戳。
    updated_at_ms: i64,
}

impl KnowledgeBaseRuntime {
    /// Creates one isolated knowledge library and optional initial project bindings.
    /// 创建一个隔离知识库以及可选的初始项目绑定。
    ///
    /// # Parameters
    /// 参数
    /// `name` is the user-facing library name, normalized by the existing name rules.
    /// `name` 是按现有名称规则规范化的用户可见知识库名称。
    /// `description` is the user-facing description persisted after trimming.
    /// `description` 是去除首尾空白后持久化的用户可见说明。
    /// `project_ids` is the exact projects receiving initial authorized/default-enabled bindings.
    /// `project_ids` 是接收初始授权及默认启用绑定的精确项目列表。
    ///
    /// # Returns
    /// 返回值
    /// Returns the validated persisted library summary, or a validation/storage error.
    /// 返回经校验的持久化知识库摘要，或校验/存储错误。
    pub async fn create_library(
        &self,
        name: &str,
        description: &str,
        project_ids: &[String],
    ) -> Result<KnowledgeLibrarySummary, String> {
        let normalized_name = validate_library_name(name)?;
        let library_id = Uuid::new_v4().to_string();
        let now = unix_epoch_millis()?;
        let name_key = library_name_key(&normalized_name);
        local_sql::execute_batch_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
            r#"INSERT INTO knowledge_libraries(
                    id, name, name_key, description, lifecycle_state, document_count,
                    chunk_count, index_status, settings_json, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, 'active', 0, 0, 'empty', ?, ?, ?);"#,
            vec![vec![
                text(&library_id),
                text(&normalized_name),
                text(&name_key),
                text(description.trim()),
                text("{}"),
                int(now),
                int(now),
            ]],
        )
        .await
        .map_err(|error| format!("创建知识库注册记录失败：{error}"))?;

        let initialize_result = async {
            self.initialize_library_storage(&library_id, &normalized_name)
                .await?;
            for project_id in project_ids {
                self.set_project_binding(&library_id, project_id, true, true)
                    .await?;
            }
            Ok::<(), String>(())
        }
        .await;

        if let Err(error) = initialize_result {
            let _ = self.release_library_binding(&library_id).await;
            let _ = local_sql::execute_script_async(
                &self.host_runtime.local,
                LogicalDatabase::AppState,
                "DELETE FROM knowledge_libraries WHERE id = ?;",
                vec![text(&library_id)],
            )
            .await;
            let root = self.library_root(&library_id)?;
            let _ = self.remove_host_owned_child_directory_if_exists(&root, &library_id);
            return Err(error);
        }

        self.get_library(&library_id).await
    }

    /// Creates one library with an explicitly validated per-library retrieval configuration.
    /// 使用经过显式校验的按资料库检索配置创建一个资料库。
    ///
    /// # Parameters
    /// 参数
    /// `name` is the user-facing library name, normalized by the existing name rules.
    /// `name` 是按现有名称规则规范化的用户可见知识库名称。
    /// `description` is the user-facing description persisted after trimming.
    /// `description` 是去除首尾空白后持久化的用户可见说明。
    /// `project_ids` is the exact projects receiving initial authorized/default-enabled bindings.
    /// `project_ids` 是接收初始授权及默认启用绑定的精确项目列表。
    /// `settings` is the requested durable retrieval/model configuration.
    /// `settings` 是请求的持久化检索与模型配置。
    ///
    /// # Returns
    /// 返回值
    /// Returns the validated persisted library summary, or a validation/storage error.
    /// 返回经校验的持久化知识库摘要，或校验/存储错误。
    pub async fn create_library_with_settings(
        &self,
        name: &str,
        description: &str,
        project_ids: &[String],
        settings: &KnowledgeLibrarySettings,
    ) -> Result<KnowledgeLibrarySummary, String> {
        settings.validate()?;
        let normalized_name = validate_library_name(name)?;
        let library_id = Uuid::new_v4().to_string();
        let now = unix_epoch_millis()?;
        let settings_json = serde_json::to_string(settings)
            .map_err(|error| format!("序列化知识库配置失败：{error}"))?;
        local_sql::execute_batch_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
            r#"INSERT INTO knowledge_libraries(
                    id, name, name_key, description, lifecycle_state, document_count,
                    chunk_count, index_status, settings_json, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, 'active', 0, 0, 'empty', ?, ?, ?);"#,
            vec![vec![
                text(&library_id),
                text(&normalized_name),
                text(library_name_key(&normalized_name)),
                text(description.trim()),
                text(&settings_json),
                int(now),
                int(now),
            ]],
        )
        .await
        .map_err(|error| format!("创建知识库注册记录失败：{error}"))?;
        let initialize_result = async {
            self.initialize_library_storage(&library_id, &normalized_name)
                .await?;
            for project_id in project_ids {
                self.set_project_binding(&library_id, project_id, true, true)
                    .await?;
            }
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = initialize_result {
            let _ = self.release_library_binding(&library_id).await;
            let _ = local_sql::execute_script_async(
                &self.host_runtime.local,
                LogicalDatabase::AppState,
                "DELETE FROM knowledge_libraries WHERE id = ?;",
                vec![text(&library_id)],
            )
            .await;
            let root = self.library_root(&library_id)?;
            let _ = self.remove_host_owned_child_directory_if_exists(&root, &library_id);
            return Err(error);
        }
        if settings.curation.enabled {
            let mut persisted = settings.clone();
            persisted.display_index.status = KnowledgeDisplayIndexStatus::Pending;
            if persisted.embedding.enabled {
                persisted.vector_index.status = KnowledgeVectorIndexStatus::Pending;
            }
            let settings_json = serde_json::to_string(&persisted)
                .map_err(|error| format!("序列化显示库初始状态失败：{error}"))?;
            local_sql::execute_script_async(
                &self.host_runtime.local,
                LogicalDatabase::AppState,
                    "UPDATE knowledge_libraries SET settings_json = ? WHERE id = ? AND lifecycle_state = 'active';",
                    vec![text(&settings_json), text(&library_id)],
                ).await
                .map_err(|error| format!("保存显示库初始状态失败：{error}"))?;
        }
        self.get_library(&library_id).await
    }

    /// Updates and returns one library's durable retrieval and model configuration.
    /// 更新并返回一个资料库持久化的检索与模型配置。
    ///
    /// # Parameters
    /// 参数
    /// `settings` is the requested durable retrieval/model configuration.
    /// `settings` 是请求的持久化检索与模型配置。
    /// `library_id` is the canonical immutable library UUID.
    /// `library_id` 是规范且不可变的知识库 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the validated persisted library summary, or a validation/storage error.
    /// 返回经校验的持久化知识库摘要，或校验/存储错误。
    pub async fn set_library_settings(
        &self,
        library_id: &str,
        settings: &KnowledgeLibrarySettings,
    ) -> Result<KnowledgeLibrarySummary, String> {
        validate_library_id(library_id)?;
        // The settings row participates in the same mutation domain as imports and index swaps.
        // 配置记录与导入、索引切换属于同一变更域，必须使用同一把锁。
        let _guard = self.library_mutation_guard(library_id).await?;
        let current = self.get_library(library_id).await?;
        let mut next = settings.clone();
        // Only explicit file-management actions can change ignore rules.
        // 只有明确的文件管理操作才能修改忽略规则。
        next.ignored_files = current.settings.ignored_files.clone();
        // Preserve immutable model identity while allowing profile canonicalization to update its owner.
        // 保留不可变模型身份，同时允许 profile 规范化更新快照所属者。
        let retain_snapshot_owner = |current_snapshot: &Option<KnowledgeModelSnapshot>,
                                     requested_snapshot: &Option<KnowledgeModelSnapshot>|
         -> Option<KnowledgeModelSnapshot> {
            match (current_snapshot, requested_snapshot) {
                (Some(current_snapshot), Some(requested_snapshot)) => {
                    let mut retained = current_snapshot.clone();
                    retained.route.consumer_profile_id =
                        requested_snapshot.route.consumer_profile_id.clone();
                    Some(retained)
                }
                (Some(current_snapshot), None) => Some(current_snapshot.clone()),
                (None, Some(requested_snapshot)) => Some(requested_snapshot.clone()),
                (None, None) => None,
            }
        };
        if next.curation.last_successful_sync_snapshot.is_none() {
            next.curation.last_successful_sync_snapshot = current
                .settings
                .curation
                .last_successful_sync_snapshot
                .clone();
        }
        if next.embedding.last_successful_sync_snapshot.is_none() {
            next.embedding.last_successful_sync_snapshot = current
                .settings
                .embedding
                .last_successful_sync_snapshot
                .clone();
        }
        // The creation snapshot is immutable after the first successful configuration.
        // 创建快照在首次成功配置后不可变，后续设置请求不能覆盖历史模型。
        next.curation.created_model_snapshot = retain_snapshot_owner(
            &current.settings.curation.created_model_snapshot,
            &next.curation.created_model_snapshot,
        );
        next.embedding.created_model_snapshot = retain_snapshot_owner(
            &current.settings.embedding.created_model_snapshot,
            &next.embedding.created_model_snapshot,
        );
        let embedding_model_changed = current.settings.embedding.route != next.embedding.route
            || current.settings.embedding.desired_model_fingerprint
                != next.embedding.desired_model_fingerprint;
        let embedding_dimension_changed =
            current.settings.embedding.configured_dimension != next.embedding.configured_dimension;
        let curation_configuration_changed = current.settings.curation.enabled
            != next.curation.enabled
            || current.settings.curation.route != next.curation.route
            || current.settings.curation.desired_model_fingerprint
                != next.curation.desired_model_fingerprint
            || current.settings.curation.prompt_version != next.curation.prompt_version;
        if !curation_configuration_changed {
            next.display_index = current.settings.display_index.clone();
        } else {
            next.display_index = current.settings.display_index.clone();
            next.display_index.status = if matches!(
                next.display_index.status,
                KnowledgeDisplayIndexStatus::NotCreated
            ) && next.curation.enabled
            {
                KnowledgeDisplayIndexStatus::Pending
            } else {
                KnowledgeDisplayIndexStatus::Stale
            };
            next.display_index.last_error = Some(String::from(
                "显示库整理模型或固定提示词发生变化，必须重新生成显示库",
            ));
            next.display_index.pending_generation = None;
            next.display_index.pending_source_content_revision = None;
            next.display_index.pending_prompt_version = None;
            next.display_index.pending_model_fingerprint = None;
        }

        if !embedding_model_changed && !embedding_dimension_changed && next.embedding.enabled {
            next.vector_index = current.settings.vector_index.clone();
            next.vector_index.compatibility_override =
                settings.vector_index.compatibility_override.clone();
            if next.vector_index.compatibility_override.is_none() {
                next.vector_index.last_error = None;
            }
        }
        if next.embedding.enabled && (embedding_model_changed || embedding_dimension_changed) {
            next.vector_index = current.settings.vector_index.clone();
            let override_note = settings
                .vector_index
                .compatibility_override
                .as_deref()
                .map(str::trim)
                .filter(|note| !note.is_empty());
            let same_dimension_override = !embedding_dimension_changed
                && current.settings.vector_index.status == KnowledgeVectorIndexStatus::Ready
                && current.settings.vector_index.indexed_dimension
                    == next.embedding.configured_dimension
                && override_note.is_some();
            if embedding_dimension_changed {
                next.vector_index.status = KnowledgeVectorIndexStatus::Disabled;
                next.vector_index.compatibility_override = None;
                next.vector_index.last_error = Some(String::from(
                    "向量配置维度发生变化，必须重新探测并重建索引后才能恢复向量检索",
                ));
            } else if same_dimension_override {
                next.vector_index.status = KnowledgeVectorIndexStatus::Ready;
                next.vector_index.compatibility_override = override_note.map(ToOwned::to_owned);
                next.vector_index.last_error = Some(format!(
                    "已人工确认同维度模型兼容，查询继续复用旧向量索引：{}",
                    next.vector_index
                        .compatibility_override
                        .as_deref()
                        .unwrap_or_default()
                ));
            } else {
                next.vector_index.status = KnowledgeVectorIndexStatus::Stale;
                next.vector_index.compatibility_override = None;
                next.vector_index.last_error = Some(String::from(
                    "向量模型路由发生变化，旧索引仍保留，必须重新构建或确认兼容性",
                ));
            }
        }
        if !next.embedding.enabled {
            next.vector_index = super::models::KnowledgeVectorIndexState::default();
        }
        next.validate()?;
        let settings_json = serde_json::to_string(&next)
            .map_err(|error| format!("序列化知识库配置失败：{error}"))?;
        let updated_at_ms = unix_epoch_millis()?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_libraries SET settings_json = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![text(&settings_json), int(updated_at_ms), text(library_id)],
            ).await
            .map_err(|error| format!("保存知识库配置失败：{error}"))?;
        self.get_library(library_id).await
    }

    /// Renames one active library without moving its physical directory or rebuilding FTS.
    /// 重命名一个活动知识库，但不移动物理目录或重建 FTS。
    ///
    /// # Parameters
    /// 参数
    /// `name` is the user-facing library name, normalized by the existing name rules.
    /// `name` 是按现有名称规则规范化的用户可见知识库名称。
    /// `description` is the user-facing description persisted after trimming.
    /// `description` 是去除首尾空白后持久化的用户可见说明。
    /// `library_id` is the canonical immutable library UUID.
    /// `library_id` 是规范且不可变的知识库 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the validated persisted library summary, or a validation/storage error.
    /// 返回经校验的持久化知识库摘要，或校验/存储错误。
    pub async fn rename_library(
        &self,
        library_id: &str,
        name: &str,
        description: &str,
    ) -> Result<KnowledgeLibrarySummary, String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        let current = self.require_active_library(library_id).await?;
        let normalized_name = validate_library_name(name)?;
        let now = unix_epoch_millis()?;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_libraries SET name = ?, name_key = ?, description = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    text(&normalized_name),
                    text(library_name_key(&normalized_name)),
                    text(description.trim()),
                    int(now),
                    text(library_id),
                ],
            ).await
            .map_err(|error| format!("重命名知识库失败：{error}"))?;

        let manifest = KnowledgeManifest {
            library_id: library_id.to_string(),
            name: normalized_name,
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
        };
        if let Err(error) = self.write_manifest_atomic(&manifest) {
            let _ = local_sql::execute_script_async(
                &self.host_runtime.local,
                LogicalDatabase::AppState,
                "UPDATE knowledge_libraries SET name = ?, name_key = ?, description = ?, updated_at_ms = ? WHERE id = ?;",
                vec![
                    text(&current.name),
                    text(library_name_key(&current.name)),
                    text(&current.description),
                    int(current.updated_at_ms),
                    text(library_id),
                ],
            ).await;
            return Err(error);
        }
        self.get_library(library_id).await
    }

    /// Lists all libraries or only libraries authorized for one project.
    /// 列出全部知识库，或仅列出授权给某个项目的知识库。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the exact project identity; an optional filter lists all lifecycle states when absent.
    /// `project_id` 是精确项目身份；可选过滤未提供时列出全部生命周期记录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the ordered registry summaries, restricted to active authorized libraries when project_id is present, or an error.
    /// 返回有序注册摘要；提供项目时只含活动且已授权库，或返回错误。
    pub async fn list_libraries(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<KnowledgeLibrarySummary>, String> {
        let (sql, params): (&str, &[SqliteValue]) = if let Some(project_id) = project_id {
            (
                r#"SELECT l.id, l.name, l.description, l.lifecycle_state, l.document_count,
                           l.chunk_count, l.index_status, l.settings_json, l.last_completed_import_at_ms,
                           l.created_at_ms, l.updated_at_ms
                    FROM knowledge_libraries l
                    INNER JOIN knowledge_library_project_bindings b ON b.library_id = l.id
                    WHERE b.project_id = ? AND l.lifecycle_state = 'active'
                    ORDER BY l.name_key ASC;"#,
                &[text(project_id)],
            )
        } else {
            (
                r#"SELECT id, name, description, lifecycle_state, document_count, chunk_count,
                           index_status, settings_json, last_completed_import_at_ms, created_at_ms, updated_at_ms
                    FROM knowledge_libraries ORDER BY name_key ASC;"#,
                &[],
            )
        };
        let rows: Vec<LibraryRow> = self.query_app_rows(sql, params).await?;
        rows.into_iter().map(library_summary_from_row).collect()
    }

    /// Returns one library by immutable identifier.
    /// 根据不可变标识符返回一个知识库。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical immutable library UUID.
    /// `library_id` 是规范且不可变的知识库 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the validated persisted library summary, or a validation/storage error.
    /// 返回经校验的持久化知识库摘要，或校验/存储错误。
    pub async fn get_library(&self, library_id: &str) -> Result<KnowledgeLibrarySummary, String> {
        validate_library_id(library_id)?;
        let rows: Vec<LibraryRow> = self
            .query_app_rows(
                r#"SELECT id, name, description, lifecycle_state, document_count, chunk_count,
                           index_status, settings_json, last_completed_import_at_ms, created_at_ms, updated_at_ms
                    FROM knowledge_libraries WHERE id = ?;"#,
                &[text(library_id)],
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| format!("知识库不存在：{library_id}"))
            .and_then(library_summary_from_row)
    }

    /// Deletes one isolated library after revoking access and validating its canonical path.
    /// 在撤销访问并验证规范路径后删除一个隔离知识库。
    ///
    /// A library left in the `deleting` state by an interrupted attempt resumes its removal, so a failed
    /// deletion is always retryable instead of locking the library out of every operation.
    /// 被中断的尝试留在 `deleting` 状态的知识库会继续移除，因此失败的删除总能重试，而不会让该知识库被所有操作拒之门外。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical immutable library UUID.
    /// `library_id` 是规范且不可变的知识库 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after revoking access, removing storage and deleting the registry row, or an explicit failure.
    /// 返回撤销访问、移除存储及删除注册记录后的成功，或明确失败。
    pub async fn delete_library(&self, library_id: &str) -> Result<(), String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        let library = self.get_library(library_id).await?;
        match library.lifecycle_state.as_str() {
            "active" => {
                local_sql::execute_script_async(
                    &self.host_runtime.local,
                    LogicalDatabase::AppState,
                    "UPDATE knowledge_libraries SET lifecycle_state = 'deleting', updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                    vec![int(unix_epoch_millis()?), text(library_id)],
                )
                .await
                .map_err(|error| format!("撤销知识库访问失败：{error}"))?;
            }
            "deleting" => {}
            state => {
                return Err(format!("知识库当前不可删除：{library_id} ({state})"));
            }
        }

        let (_space_id, binding_id) = self.library_binding_identity(library_id)?;
        // A binding registers on first use in this process, so an unregistered one holds no WAL to checkpoint.
        // 绑定在本进程首次使用时才登记，因此未登记的绑定没有需要检查点的 WAL。
        if let Ok(binding) = self.resolve_library_binding(&binding_id) {
            let _ = local_sql::execute_script_async(
                &self.host_runtime.local,
                binding,
                "PRAGMA wal_checkpoint(TRUNCATE);",
                Vec::new(),
            )
            .await;
        }
        self.release_library_binding(library_id).await?;

        // Removal tolerates a directory an earlier attempt already removed and runs off the async workers.
        // 移除容忍先前尝试已删除的目录，并在异步工作线程之外执行。
        let root = self.library_root(library_id)?;
        let runtime = self.clone();
        let expected_name = library_id.to_string();
        tokio::task::spawn_blocking(move || {
            runtime.remove_host_owned_child_directory_if_exists(&root, &expected_name)
        })
        .await
        .map_err(|error| format!("删除知识库目录任务失败：{error}"))??;
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
            "DELETE FROM knowledge_libraries WHERE id = ? AND lifecycle_state = 'deleting';",
            vec![text(library_id)],
        )
        .await
        .map_err(|error| format!("删除知识库注册记录失败：{error}"))?;
        Ok(())
    }

    /// Marks derived display and vector indexes stale after authoritative content changes.
    /// 在权威内容发生变化后将派生显示层与向量索引标记为过期。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical immutable library UUID.
    /// `library_id` 是规范且不可变的知识库 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after any necessary derived-state projection, or a validation/storage error.
    /// 返回必要派生状态投影后的成功，或校验/存储错误。
    pub(super) async fn mark_derived_indexes_stale(&self, library_id: &str) -> Result<(), String> {
        let library = self.get_library(library_id).await?;
        let mut settings = library.settings;
        let mut changed = false;
        if settings.curation.enabled
            && (matches!(
                settings.display_index.status,
                KnowledgeDisplayIndexStatus::Ready
                    | KnowledgeDisplayIndexStatus::Stale
                    | KnowledgeDisplayIndexStatus::Failed
            ) || settings.display_index.pending_generation.is_some())
        {
            settings.display_index.status = KnowledgeDisplayIndexStatus::Stale;
            // A pending generation belongs to one immutable source revision and cannot survive a content change.
            // 待完成代次只属于一个不可变来源修订，不能跨越内容变化继续使用。
            settings.display_index.pending_generation = None;
            settings.display_index.pending_source_content_revision = None;
            settings.display_index.pending_prompt_version = None;
            settings.display_index.pending_model_fingerprint = None;
            settings.display_index.last_error = Some(String::from(
                "原始 Markdown 内容发生变化，显示库需要重新生成",
            ));
            changed = true;
        }
        if settings.embedding.enabled
            && matches!(
                settings.vector_index.status,
                KnowledgeVectorIndexStatus::Ready | KnowledgeVectorIndexStatus::Failed
            )
        {
            settings.vector_index.status = KnowledgeVectorIndexStatus::Stale;
            settings.vector_index.last_error = Some(String::from(
                "原始或显示 Markdown 内容发生变化，向量索引需要重新构建",
            ));
            changed = true;
        }
        if !changed {
            return Ok(());
        }
        settings.validate()?;
        let settings_json = serde_json::to_string(&settings)
            .map_err(|error| format!("序列化派生索引过期状态失败：{error}"))?;
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
            .map_err(|error| format!("保存派生索引过期状态失败：{error}"))?;
        Ok(())
    }

    /// Ensures the central registry row exists and is active.
    /// 确保中央注册记录存在且处于活动状态。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical immutable library UUID.
    /// `library_id` 是规范且不可变的知识库 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the validated persisted library summary, or a validation/storage error.
    /// 返回经校验的持久化知识库摘要，或校验/存储错误。
    pub(super) async fn require_active_library(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeLibrarySummary, String> {
        let library = self.get_library(library_id).await?;
        if library.lifecycle_state != "active" {
            return Err(format!(
                "知识库当前不可访问：{} ({})",
                library.id, library.lifecycle_state
            ));
        }
        Ok(library)
    }

    /// Projects authoritative counts and index state without advancing the last successful import time.
    /// 投影权威计数与索引状态，但不推进最近一次成功导入时间。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `counts` is the authoritative document/chunk aggregates being projected.
    /// `counts` 是待投影的权威文档和切片聚合计数。
    /// `index_status` is the exact derived-index state accompanying this count projection.
    /// `index_status` 是本次计数投影关联的精确派生索引状态。
    /// `updated_at_ms` is the projection time without changing the last successful import time.
    /// `updated_at_ms` 是不改变最近成功导入时间的本次投影时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    pub(super) async fn project_registry_authoritative_counts(
        &self,
        library_id: &str,
        counts: &CountRow,
        index_status: &str,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_libraries SET document_count = ?, chunk_count = ?, index_status = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    int(counts.document_count),
                    int(counts.chunk_count),
                    text(index_status),
                    int(updated_at_ms),
                    text(library_id),
                ],
            ).await
            .map_err(|error| format!("更新知识库权威摘要失败：{error}"))?;
        Ok(())
    }

    /// Projects authoritative counts and terminal index state into the central registry.
    /// 将权威计数与终态索引状态投影到中央注册表。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the exact library owning this maintenance or registry projection.
    /// `library_id` 是维护或注册表投影所属的精确知识库。
    /// `counts` is the authoritative document/chunk aggregates being projected.
    /// `counts` 是待投影的权威文档和切片聚合计数。
    /// `index_status` is the exact derived-index state accompanying this count projection.
    /// `index_status` 是本次计数投影关联的精确派生索引状态。
    /// `completed_at_ms` is the caller-supplied timestamp projected to both import and update fields.
    /// `completed_at_ms` 是调用方传入、同时投影到导入和更新字段的时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation, or its explicit validation/storage error.
    /// 返回所述操作完成后的成功，或明确校验/存储错误。
    pub(super) async fn project_registry_counts(
        &self,
        library_id: &str,
        counts: &CountRow,
        index_status: &str,
        completed_at_ms: i64,
    ) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_libraries SET document_count = ?, chunk_count = ?, index_status = ?, last_completed_import_at_ms = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    int(counts.document_count),
                    int(counts.chunk_count),
                    text(index_status),
                    int(completed_at_ms),
                    int(completed_at_ms),
                    text(library_id),
                ],
            ).await
            .map_err(|error| format!("更新知识库中央摘要失败：{error}"))?;
        Ok(())
    }
}

/// Validates and normalizes a user-visible library name.
/// 验证并规范化用户可见知识库名称。
///
/// # Parameters
/// 参数
/// `name` is the user-facing library name, normalized by the existing name rules.
/// `name` 是按现有名称规则规范化的用户可见知识库名称。
///
/// # Returns
/// 返回值
/// Returns the trimmed nonempty bounded name, or a validation error.
/// 返回去除首尾空白后的非空有界名称，或校验错误。
pub(super) fn validate_library_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(String::from("知识库名称不能为空。"));
    }
    if trimmed.chars().count() > 120 {
        return Err(String::from("知识库名称不能超过 120 个字符。"));
    }
    Ok(trimmed.to_string())
}

/// Produces the unique comparison key for a library name.
/// 生成知识库名称的唯一比较键。
///
/// # Parameters
/// 参数
/// `name` is the user-facing library name, normalized by the existing name rules.
/// `name` 是按现有名称规则规范化的用户可见知识库名称。
///
/// # Returns
/// 返回值
/// Returns the trimmed lowercase comparison key.
/// 返回去除首尾空白后的名称小写比较键。
pub(super) fn library_name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Maps one registry row into the checked public summary.
/// 将一条注册记录映射为已检查的公共摘要。
///
/// # Parameters
/// 参数
/// `row` is the decoded authoritative registry row.
/// `row` 是已解码的权威注册记录。
///
/// # Returns
/// 返回值
/// Returns the validated persisted library summary, or a validation/storage error.
/// 返回经校验的持久化知识库摘要，或校验/存储错误。
pub(super) fn library_summary_from_row(row: LibraryRow) -> Result<KnowledgeLibrarySummary, String> {
    let mut settings = if row.settings_json.trim().is_empty() || row.settings_json.trim() == "{}" {
        KnowledgeLibrarySettings::default()
    } else {
        serde_json::from_str::<KnowledgeLibrarySettings>(&row.settings_json)
            .map_err(|error| format!("解析知识库 {} 的持久化配置失败：{error}", row.id))?
    };
    settings.normalize_derived_compatibility();
    settings.validate()?;
    Ok(KnowledgeLibrarySummary {
        id: row.id,
        name: row.name,
        description: row.description,
        settings,
        lifecycle_state: row.lifecycle_state,
        document_count: nonnegative_u64(row.document_count, "document_count")?,
        chunk_count: nonnegative_u64(row.chunk_count, "chunk_count")?,
        index_status: row.index_status,
        last_completed_import_at_ms: row.last_completed_import_at_ms,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    })
}
