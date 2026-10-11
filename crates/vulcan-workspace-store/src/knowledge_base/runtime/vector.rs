//! LanceDB generation tables, vector batches, and indexed model-space state.
//! LanceDB 代次表、向量批次与已索引模型空间状态。

use super::super::models::{
    KnowledgeDisplayIndexStatus, KnowledgeLibrarySummary, KnowledgeModelSnapshot,
    KnowledgeVectorIndexStatus,
};
use super::{KnowledgeBaseRuntime, MAX_SEARCH_LIMIT, unix_epoch_millis, validate_generation_id};
use super::{int, text};
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use vulcan_lancedb_engine::types::{
    LanceDbColumnDef, LanceDbColumnType, LanceDbCreateTableInput, LanceDbDropTableInput,
    LanceDbInputFormat, LanceDbOutputFormat, LanceDbSearchInput, LanceDbUpsertInput,
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
            return Err(String::from(
                "The vector generation dimension must be greater than 0",
            ));
        }
        let previous_dimension = self
            .get_library(library_id)
            .await?
            .settings
            .vector_index
            .indexed_dimension;
        self.ensure_vector_storage(library_id).await?;
        let (_space_id, binding_id) = self.vector_binding_identity(library_id)?;
        let table_name = vector_table_name(generation)?;
        // A changed dimension cannot reuse the old Lance schema; same-dimension retries remain upserts.
        // 维度变化不能复用旧 Lance 表结构；同维度重试仍使用 upsert 保留可恢复性。
        // One engine acquisition serves both the optional drop and the create below, so the cached
        // engine is looked up once per operation rather than once per statement.
        // 一次引擎获取同时服务于下方的可选删除与创建，因此每个操作只查找一次已缓存的引擎。
        let vector_engine = self.resolve_vector_engine(&binding_id).await?;
        let drop_error = if previous_dimension != Some(dimension) {
            vector_engine
                .drop_table(LanceDbDropTableInput {
                    table_name: table_name.clone(),
                })
                .await
                .err()
        } else {
            None
        };
        let create_result = vector_engine
            .create_table(LanceDbCreateTableInput {
                table_name: table_name.clone(),
                columns: vec![
                    LanceDbColumnDef {
                        name: String::from("id"),
                        column_type: LanceDbColumnType::String,
                        vector_dim: 0,
                        nullable: false,
                    },
                    LanceDbColumnDef {
                        name: String::from("display_chunk_id"),
                        column_type: LanceDbColumnType::String,
                        vector_dim: 0,
                        nullable: false,
                    },
                    LanceDbColumnDef {
                        name: String::from("generation"),
                        column_type: LanceDbColumnType::String,
                        vector_dim: 0,
                        nullable: false,
                    },
                    LanceDbColumnDef {
                        name: String::from("vector"),
                        column_type: LanceDbColumnType::VectorFloat32,
                        vector_dim: dimension,
                        nullable: false,
                    },
                ],
                overwrite_if_exists: true,
            })
            .await;
        if let Err(error) = create_result {
            return Err(match drop_error {
                Some(drop_error) => format!(
                    "Failed to create the knowledge LanceDB vector generation: {error}; dropping the old-dimension table also failed: {drop_error}"
                ),
                None => {
                    format!("Failed to create the knowledge LanceDB vector generation: {error}")
                }
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
    /// `rows` is the embedding JSON rows required by the in-process engine contract.
    /// `rows` 是进程内引擎契约要求的 Embedding JSON 行。
    ///
    /// # Returns
    /// 返回值
    /// Returns the engine-reported input row count, or a validation/write error.
    /// 返回引擎报告的输入行数，或校验/写入错误。
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
        let expected_dimension = usize::try_from(dimension).map_err(|_| {
            String::from("The vector dimension exceeds the range supported by this platform")
        })?;
        for row in rows {
            let vector = row
                .get("vector")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| String::from("A vector write row is missing the vector array"))?;
            if vector.len() != expected_dimension {
                return Err(format!(
                    "The vector write dimension {} disagrees with the configured dimension {}",
                    vector.len(),
                    expected_dimension
                ));
            }
            if row.get("generation").and_then(serde_json::Value::as_str) != Some(generation) {
                return Err(String::from(
                    "The vector write row generation is inconsistent",
                ));
            }
        }
        // The binding is re-established here rather than assumed. In-memory activation is per runtime, so a
        // process that has just restarted has an empty activation set even though the library, the settings row
        // and the LanceDB directory all persisted. Without this, a resumed write or a hybrid search would ask
        // for a named binding the new process never registered and fail on intact storage.
        // 此处重新建立绑定而不是假定它已存在。内存中的激活状态是按运行时独立的，因此刚重启的进程其激活集合为空，
        // 即便资料库、设置行与 LanceDB 目录都已持久化。没有这一步，续写或混合检索会去索取一个本进程从未登记的
        // 具名绑定，从而在一块完好的存储上失败。
        // Creation stays disallowed: this call writes into a table `prepare_vector_generation` created, so a
        // missing directory here is a real inconsistency and must be reported rather than created.
        // 仍不允许创建：本次调用写入的是 `prepare_vector_generation` 已创建的表，因此此处目录缺失是真实的不一致，
        // 必须报出而不是被创建出来。
        let (_space_id, binding_id) = self.activate_vector_binding(library_id, false).await?;
        let data = serde_json::to_vec(rows)
            .map_err(|error| format!("Failed to encode LanceDB vector rows: {error}"))?;
        let result = self
            .resolve_vector_engine(&binding_id)
            .await?
            .vector_upsert(LanceDbUpsertInput {
                table_name: String::from(table_name),
                input_format: LanceDbInputFormat::JsonRows,
                data,
                key_columns: vec![String::from("id")],
            })
            .await
            .map_err(|error| {
                format!("Failed to write the knowledge LanceDB vector batch: {error}")
            })?;
        Ok(result.input_rows)
    }

    /// Searches one active vector generation and returns engine JSON rows.
    /// 检索一份活动向量代次并返回引擎 JSON 行。
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
    /// Returns the decoded engine result rows, or a validation/query error.
    /// 返回解码后的引擎结果行，或校验/查询错误。
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
            return Err(String::from("The vector query must not be empty"));
        }
        // This is the production hybrid-search path's read, and it must stand on its own. Activation is per
        // runtime, so after a restart the persisted `Ready` status and active generation are present while the
        // named binding is not; relying on an earlier call in the same process to have registered it is exactly
        // what made hybrid search fail on a restarted, fully intact library.
        // 这是生产混合检索路径的读取，必须自足。激活状态按运行时独立，因此重启后持久化的 `Ready` 状态与活动代次
        // 都在，而具名绑定不在；指望本进程中较早的某次调用已把它登记好，正是让混合检索在一个已重启且完好的资料库上
        // 失败的原因。
        // Creation is disallowed because a search must never bring a backend artifact into existence.
        // 不允许创建：检索绝不应让一个后端产物从无到有。
        let (_space_id, binding_id) = self.activate_vector_binding(library_id, false).await?;
        let result = self
            .resolve_vector_engine(&binding_id)
            .await?
            .vector_search(LanceDbSearchInput {
                table_name: String::from(table_name),
                vector,
                limit: limit.clamp(1, MAX_SEARCH_LIMIT),
                filter: format!("generation = '{generation}'"),
                vector_column: String::from("vector"),
                output_format: LanceDbOutputFormat::JsonRows,
            })
            .await
            .map_err(|error| format!("Failed to search the knowledge LanceDB vectors: {error}"))?;
        serde_json::from_slice(&result.data).map_err(|error| {
            format!("Failed to parse the knowledge LanceDB search results: {error}")
        })
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
            return Err(String::from(
                "The vector index dimension must be greater than 0",
            ));
        }
        let _guard = self.library_mutation_guard(library_id).await?;
        let current = self.require_active_library(library_id).await?;
        if !current.settings.embedding.enabled {
            return Err(String::from(
                "No embedding model is enabled for this library",
            ));
        }
        if current.settings.display_index.status != KnowledgeDisplayIndexStatus::Ready
            || current.settings.display_index.active_generation.as_deref() != Some(generation)
        {
            return Err(String::from(
                "The active display-library generation changed; refusing to publish the mismatched vector index",
            ));
        }
        let mut settings = current.settings.clone();
        let model_fingerprint = settings
            .embedding
            .desired_model_fingerprint
            .clone()
            .ok_or_else(|| String::from("The embedding model fingerprint is not configured yet"))?;
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
        let settings_json = serde_json::to_string(&settings).map_err(|error| {
            format!("Failed to serialize the vector index success state: {error}")
        })?;
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
            .map_err(|error| format!("Failed to save the vector index success state: {error}"))?;
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
            .map_err(|error| format!("Failed to serialize the vector index state: {error}"))?;
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
            .map_err(|error| format!("Failed to save the vector index lifecycle state: {error}"))?;
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
        .ok_or_else(|| String::from("The knowledge vector table name prefix is invalid."))?;
    if suffix.len() != 32 || !suffix.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(String::from(
            "The knowledge vector table name format is invalid.",
        ));
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
