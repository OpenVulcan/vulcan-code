//! LanceDB generation tables, vector batches, and indexed model-space state.
//! LanceDB 代次表、向量批次与已索引模型空间状态。

use super::super::models::{
    KnowledgeDisplayIndexStatus, KnowledgeLibrarySummary, KnowledgeModelSnapshot,
    KnowledgeVectorIndexStatus,
};
use super::{
    APP_SPACE_ID, KnowledgeBaseRuntime, MAX_SEARCH_LIMIT, int, text, unix_epoch_millis,
    validate_generation_id,
};
use vldb_controller_client::{
    ControllerLanceDbColumnDef, ControllerLanceDbColumnType, ControllerLanceDbInputFormat,
    ControllerLanceDbOutputFormat,
};

impl KnowledgeBaseRuntime {
    /// Ensures the isolated LanceDB backend used by one library's vector generations.
    /// 确保一个资料库向量代次使用的隔离 LanceDB 后端。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the documented operation completes, or its explicit failure.
    /// 返回所述操作完成后的成功结果，或其明确错误。
    pub async fn ensure_vector_storage(&self, library_id: &str) -> Result<(), String> {
        self.require_active_library(library_id).await?;
        self.activate_vector_binding(library_id, true).await?;
        Ok(())
    }

    /// Creates one dimension-specific LanceDB vector generation table.
    /// 创建一张维度固定且按代次隔离的 LanceDB 向量表。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    /// `dimension` is the configured nonzero dimension of the generation vector column.
    /// `dimension` 是配置的代次向量列非零维度。
    ///
    /// # Returns
    /// 返回值
    /// Returns the dimension-specific generation table name, or a creation error.
    /// 返回维度特定的代次表名称，或创建错误。
    pub async fn prepare_vector_generation(
        &self,
        library_id: &str,
        generation: &str,
        dimension: u32,
    ) -> Result<String, String> {
        validate_generation_id(generation)?;
        if dimension == 0 {
            return Err(String::from("向量代次维度必须大于 0"));
        }
        let previous_dimension = self
            .get_library(library_id)
            .await?
            .settings
            .vector_index
            .indexed_dimension;
        self.ensure_vector_storage(library_id).await?;
        let (space_id, binding_id) = self.vector_binding_identity(library_id)?;
        let table_name = vector_table_name(generation)?;
        // A changed dimension cannot reuse the old Lance schema; same-dimension retries remain upserts.
        // 维度变化不能复用旧 Lance 表结构；同维度重试仍使用 upsert 保留可恢复性。
        let drop_error = if previous_dimension != Some(dimension) {
            self.controller
                .client
                .drop_lancedb_table(&space_id, &binding_id, &table_name)
                .await
                .err()
        } else {
            None
        };
        let create_result = self
            .controller
            .client
            .create_lancedb_table_typed(
                &space_id,
                &binding_id,
                &table_name,
                vec![
                    ControllerLanceDbColumnDef {
                        name: String::from("id"),
                        column_type: ControllerLanceDbColumnType::String,
                        vector_dim: 0,
                        nullable: false,
                    },
                    ControllerLanceDbColumnDef {
                        name: String::from("display_chunk_id"),
                        column_type: ControllerLanceDbColumnType::String,
                        vector_dim: 0,
                        nullable: false,
                    },
                    ControllerLanceDbColumnDef {
                        name: String::from("generation"),
                        column_type: ControllerLanceDbColumnType::String,
                        vector_dim: 0,
                        nullable: false,
                    },
                    ControllerLanceDbColumnDef {
                        name: String::from("vector"),
                        column_type: ControllerLanceDbColumnType::VectorFloat32,
                        vector_dim: dimension,
                        nullable: false,
                    },
                ],
                true,
            )
            .await;
        if let Err(error) = create_result {
            return Err(match drop_error {
                Some(drop_error) => format!(
                    "创建知识库 LanceDB 向量代次失败：{error}；清理旧维度表也失败：{drop_error}"
                ),
                None => format!("创建知识库 LanceDB 向量代次失败：{error}"),
            });
        }
        Ok(table_name)
    }

    /// Writes one complete embedding batch into a validated vector generation.
    /// 将一个完整的 Embedding 批次写入已校验的向量代次。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    /// `dimension` is the configured nonzero dimension of the generation vector column.
    /// `dimension` 是配置的代次向量列非零维度。
    /// `table_name` is the validated generation-scoped LanceDB table identity.
    /// `table_name` 是待校验的代次级 LanceDB 表身份。
    /// `rows` is the embedding JSON rows required by the existing controller contract.
    /// `rows` 是既有控制器契约要求的 Embedding JSON 行。
    ///
    /// # Returns
    /// 返回值
    /// Returns the controller-reported input row count, or a validation/write error.
    /// 返回控制器报告的输入行数，或校验/写入错误。
    pub async fn write_vector_rows(
        &self,
        library_id: &str,
        table_name: &str,
        generation: &str,
        rows: &[serde_json::Value],
        dimension: u32,
    ) -> Result<u64, String> {
        validate_generation_id(generation)?;
        validate_vector_table_name(table_name)?;
        if rows.is_empty() {
            return Ok(0);
        }
        let expected_dimension =
            usize::try_from(dimension).map_err(|_| String::from("向量维度超出当前平台支持范围"))?;
        for row in rows {
            let vector = row
                .get("vector")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| String::from("向量写入行缺少 vector 数组"))?;
            if vector.len() != expected_dimension {
                return Err(format!(
                    "向量写入维度 {} 与配置维度 {} 不一致",
                    vector.len(),
                    expected_dimension
                ));
            }
            if row.get("generation").and_then(serde_json::Value::as_str) != Some(generation) {
                return Err(String::from("向量写入行代次不一致"));
            }
        }
        let (space_id, binding_id) = self.vector_binding_identity(library_id)?;
        let data = serde_json::to_vec(rows)
            .map_err(|error| format!("编码 LanceDB 向量行失败：{error}"))?;
        let result = self
            .controller
            .client
            .upsert_lancedb_typed(
                &space_id,
                &binding_id,
                table_name,
                ControllerLanceDbInputFormat::JsonRows,
                data,
                vec![String::from("id")],
            )
            .await
            .map_err(|error| format!("写入知识库 LanceDB 向量批次失败：{error}"))?;
        Ok(result.input_rows)
    }

    /// Searches one active vector generation and returns controller JSON rows.
    /// 检索一份活动向量代次并返回控制器 JSON 行。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    /// `limit` is the requested result/page bound, clamped by this API.
    /// `limit` 是请求的结果或分页上限，由当前接口限制。
    /// `table_name` is the validated generation-scoped LanceDB table identity.
    /// `table_name` 是待校验的代次级 LanceDB 表身份。
    /// `vector` is the nonempty query vector in the indexed model space.
    /// `vector` 是已索引模型空间中的非空查询向量。
    ///
    /// # Returns
    /// 返回值
    /// Returns the decoded controller result rows, or a validation/query error.
    /// 返回解码后的控制器结果行，或校验/查询错误。
    pub async fn search_vector_rows(
        &self,
        library_id: &str,
        table_name: &str,
        generation: &str,
        vector: Vec<f32>,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>, String> {
        validate_generation_id(generation)?;
        validate_vector_table_name(table_name)?;
        if vector.is_empty() {
            return Err(String::from("向量查询不能为空"));
        }
        let (space_id, binding_id) = self.vector_binding_identity(library_id)?;
        let result = self
            .controller
            .client
            .search_lancedb_typed(
                &space_id,
                &binding_id,
                table_name,
                vector,
                limit.clamp(1, MAX_SEARCH_LIMIT),
                format!("generation = '{generation}'"),
                "vector",
                ControllerLanceDbOutputFormat::JsonRows,
            )
            .await
            .map_err(|error| format!("检索知识库 LanceDB 向量失败：{error}"))?;
        serde_json::from_slice(&result.data)
            .map_err(|error| format!("解析知识库 LanceDB 检索结果失败：{error}"))
    }

    /// Persists a successful vector generation and its exact model-space snapshot.
    /// 持久化成功的向量代次及其精确模型空间快照。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical UUID of the owning library.
    /// `library_id` 是所属知识库的规范 UUID。
    /// `generation` is the canonical UUID of the exact derived generation.
    /// `generation` 是精确派生代次的规范 UUID。
    /// `dimension` is the configured nonzero dimension of the generation vector column.
    /// `dimension` 是配置的代次向量列非零维度。
    /// `model_snapshot` is the exact model-space snapshot captured for the completed vectors.
    /// `model_snapshot` 是已完成向量所记录的精确模型空间快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns the updated persisted library summary, or a validation/storage error.
    /// 返回更新后持久化知识库摘要，或校验/存储错误。
    pub async fn mark_vector_generation_ready(
        &self,
        library_id: &str,
        generation: &str,
        model_snapshot: KnowledgeModelSnapshot,
        dimension: u32,
    ) -> Result<KnowledgeLibrarySummary, String> {
        validate_generation_id(generation)?;
        if dimension == 0 {
            return Err(String::from("向量索引维度必须大于 0"));
        }
        let _guard = self.library_mutation_guard(library_id).await?;
        let current = self.require_active_library(library_id).await?;
        if !current.settings.embedding.enabled {
            return Err(String::from("当前资料库未启用向量模型"));
        }
        if current.settings.display_index.status != KnowledgeDisplayIndexStatus::Ready
            || current.settings.display_index.active_generation.as_deref() != Some(generation)
        {
            return Err(String::from(
                "显示库活动代次已变化，拒绝发布不匹配的向量索引",
            ));
        }
        let mut settings = current.settings.clone();
        let model_fingerprint = settings
            .embedding
            .desired_model_fingerprint
            .clone()
            .ok_or_else(|| String::from("向量模型指纹尚未配置"))?;
        settings.vector_index.status = KnowledgeVectorIndexStatus::Ready;
        settings.vector_index.active_generation = Some(generation.to_string());
        settings.vector_index.indexed_model_fingerprint = Some(model_fingerprint);
        settings.vector_index.indexed_dimension = Some(dimension);
        settings.vector_index.compatibility_override = None;
        settings.vector_index.last_error = None;
        settings.embedding.observed_dimension = Some(dimension);
        settings.embedding.indexed_dimension = Some(dimension);
        settings.embedding.last_successful_sync_snapshot = Some(model_snapshot);
        settings.normalize_derived_compatibility();
        settings.validate()?;
        let settings_json = serde_json::to_string(&settings)
            .map_err(|error| format!("序列化向量索引成功状态失败：{error}"))?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_libraries SET settings_json = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    text(&settings_json),
                    int(unix_epoch_millis()?),
                    text(library_id),
                ],
            )
            .await
            .map_err(|error| format!("保存向量索引成功状态失败：{error}"))?;
        self.get_library(library_id).await
    }

    /// Persists one vector-build lifecycle status without mutating model choices.
    /// 持久化向量构建生命周期状态且不改变模型选择。
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
    pub async fn set_vector_index_status(
        &self,
        library_id: &str,
        status: KnowledgeVectorIndexStatus,
        last_error: Option<String>,
    ) -> Result<KnowledgeLibrarySummary, String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        let current = self.require_active_library(library_id).await?;
        let mut settings = current.settings.clone();
        settings.vector_index.status = status;
        settings.vector_index.last_error = last_error;
        settings.normalize_derived_compatibility();
        settings.validate()?;
        let settings_json = serde_json::to_string(&settings)
            .map_err(|error| format!("序列化向量索引状态失败：{error}"))?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_libraries SET settings_json = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    text(&settings_json),
                    int(unix_epoch_millis()?),
                    text(library_id),
                ],
            )
            .await
            .map_err(|error| format!("保存向量索引生命周期状态失败：{error}"))?;
        self.get_library(library_id).await
    }
}

/// Returns the generation-scoped LanceDB table name after validating its UUID.
/// 校验代次 UUID 后返回代次作用域的 LanceDB 表名称。
///
/// # Parameters
/// 参数
/// `generation` is the canonical UUID of the exact derived generation.
/// `generation` 是精确派生代次的规范 UUID。
///
/// # Returns
/// 返回值
/// Returns the generation-scoped LanceDB table name, or an invalid UUID error.
/// 返回代次级 LanceDB 表名称，或非法 UUID 错误。
pub(super) fn vector_table_name(generation: &str) -> Result<String, String> {
    validate_generation_id(generation)?;
    Ok(format!("knowledge_vectors_{}", generation.replace('-', "")))
}

/// Validates an activity pointer loaded from vector metadata before table access.
/// 在访问向量表前校验从元数据读取的活动表名称。
///
/// # Parameters
/// 参数
/// `table_name` is the validated generation-scoped LanceDB table identity.
/// `table_name` 是待校验的代次级 LanceDB 表身份。
///
/// # Returns
/// 返回值
/// Returns success after the documented operation completes, or its explicit failure.
/// 返回所述操作完成后的成功结果，或其明确错误。
fn validate_vector_table_name(table_name: &str) -> Result<(), String> {
    let prefix = "knowledge_vectors_";
    let suffix = table_name
        .strip_prefix(prefix)
        .ok_or_else(|| String::from("知识库向量表名称前缀无效。"))?;
    if suffix.len() != 32 || !suffix.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(String::from("知识库向量表名称格式无效。"));
    }
    Ok(())
}

/// Maps an abandoned vector build to the durable state exposed after restart.
/// 将遗留向量构建映射为重启后公开的持久化状态。
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
pub(super) fn recovered_vector_index_status(
    status: KnowledgeVectorIndexStatus,
    has_active_generation: bool,
) -> Option<KnowledgeVectorIndexStatus> {
    if status != KnowledgeVectorIndexStatus::Building {
        return None;
    }
    Some(if has_active_generation {
        KnowledgeVectorIndexStatus::Stale
    } else {
        KnowledgeVectorIndexStatus::Failed
    })
}
