//! Sharded persistence for restart-safe VCE embedding plans.
//! 面向可恢复 VCE 向量计划的分片持久化。

use super::{VceProjectIndex, port_failed, port_invalid, port_rejected, sqlite_text};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vldb_controller_client::ControllerSqliteValue;
use vulcan_context_engine::{
    VceCodeNodeSnapshot, VceEmbeddingBatchRecord, VceEmbeddingBatchResult, VceEmbeddingBatchState,
    VceEmbeddingRunContract, VceEmbeddingRunManifest, VceEmbeddingRunState, VcePortError,
    VcePortOperation,
};

/// Legacy single-row manifest layout retained for lazy migration.
/// 为惰性迁移保留的旧版单行清单布局。
const VCE_EMBEDDING_MANIFEST_LAYOUT_LEGACY: i64 = 1;

/// Current manifest layout with an envelope, node chunks, reused Blobs, and batch rows.
/// 当前清单布局，将概要、节点分片、复用 Blob 及批次记录分别保存。
const VCE_EMBEDDING_MANIFEST_LAYOUT_SHARDED: i64 = 2;

/// Maximum UTF-8 bytes stored in one node JSON chunk.
/// 单个节点 JSON 分片保存的最大 UTF-8 字节数。
const VCE_EMBEDDING_NODE_CHUNK_BYTES: usize = 32 * 1024;

/// Maximum node chunks sent in one controller write or read page.
/// 单次控制器写入或读取页允许的最大节点分片数。
const VCE_EMBEDDING_NODE_CHUNK_PAGE_SIZE: usize = 8;

/// Maximum reused Blob identities sent in one controller write or read page.
/// 单次控制器写入或读取页允许的最大复用 Blob 身份数。
const VCE_EMBEDDING_REUSED_PAGE_SIZE: usize = 64;

/// Maximum batch records sent in one controller write or read page.
/// 单次控制器写入或读取页允许的最大批次记录数。
const VCE_EMBEDDING_BATCH_PAGE_SIZE: usize = 4;

/// Maximum serialized bytes accepted for the sharded manifest envelope.
/// 分片清单概要允许的最大序列化字节数。
const VCE_EMBEDDING_ENVELOPE_MAX_BYTES: usize = 64 * 1024;

/// Maximum optimistic attempts used to obtain one revision-consistent sharded manifest.
/// 为取得一份修订一致的分片清单而执行的最大乐观读取次数。
const VCE_EMBEDDING_MANIFEST_LOAD_ATTEMPTS: usize = 3;

/// Versioned immutable envelope stored in the parent run row.
/// 保存在父运行记录中的版本化不可变概要。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredEmbeddingManifestEnvelope {
    /// Core manifest schema version.
    /// 核心清单结构版本。
    schema_version: u32,
    /// Stable run identity.
    /// 稳定运行身份。
    run_id: String,
    /// Owning namespace identity.
    /// 所属命名空间身份。
    namespace_id: String,
    /// Frozen workspace generation.
    /// 冻结工作区代次。
    generation: u64,
    /// Optional admitted source revision.
    /// 可选已准入源码修订。
    source_revision: Option<String>,
    /// Frozen model and batching contract.
    /// 冻结模型与批处理契约。
    contract: Option<VceEmbeddingRunContract>,
    /// Optional published checkpoint used as the comparison base.
    /// 可选的已发布对比基线检查点。
    base_checkpoint_version: Option<u64>,
    /// Exact number of source-node snapshots.
    /// 源码节点快照的精确数量。
    node_count: u64,
    /// Exact number of reused Blob identities.
    /// 复用 Blob 身份的精确数量。
    reused_blob_count: u64,
    /// Exact number of provider batch records.
    /// 供应商批次记录的精确数量。
    batch_count: u64,
    /// SHA-256 identity of every immutable plan field in stable order.
    /// 按稳定顺序计算的全部不可变计划字段 SHA-256 身份。
    plan_fingerprint: String,
}

impl StoredEmbeddingManifestEnvelope {
    /// Build and validate one bounded envelope from a complete in-memory manifest.
    /// 从完整内存清单构建并校验一份有界概要。
    ///
    /// `manifest` supplies frozen fields and counts; `plan_fingerprint` supplies its digest. Returns a validated envelope.
    /// `manifest` 提供冻结字段和数量，`plan_fingerprint` 提供其摘要；返回已校验概要。
    fn from_manifest(
        manifest: &VceEmbeddingRunManifest,
        plan_fingerprint: String,
    ) -> Result<Self, String> {
        // Immutable run envelope retained independently of source-node payloads.
        // 独立于源码节点负载保存的不可变运行概要。
        let envelope = Self {
            schema_version: manifest.schema_version,
            run_id: manifest.run_id.clone(),
            namespace_id: manifest.namespace_id.clone(),
            generation: manifest.generation,
            source_revision: manifest.source_revision.clone(),
            contract: manifest.contract.clone(),
            base_checkpoint_version: manifest.base_checkpoint_version,
            node_count: u64::try_from(manifest.nodes.len())
                .map_err(|_| String::from("VCE embedding node count exceeds u64"))?,
            reused_blob_count: u64::try_from(manifest.reused_blob_names.len())
                .map_err(|_| String::from("VCE reused Blob count exceeds u64"))?,
            batch_count: u64::try_from(manifest.batches.len())
                .map_err(|_| String::from("VCE embedding batch count exceeds u64"))?,
            plan_fingerprint,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    /// Validate envelope identity, count ranges, fingerprint, and serialized size.
    /// 校验概要身份、数量范围、指纹及序列化大小。
    ///
    /// Checks this envelope without I/O; returns unit on success or an explicit metadata validation error.
    /// 不执行读写地检查本概要；成功返回空值，失败返回明确的元数据校验错误。
    fn validate(&self) -> Result<(), String> {
        if self.run_id.trim().is_empty()
            || self.run_id.len() > 512
            || self.namespace_id.trim().is_empty()
            || self.namespace_id.len() > 512
            || self.generation == 0
            || self.plan_fingerprint.len() != 64
            || !self
                .plan_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || usize::try_from(self.node_count).is_err()
            || usize::try_from(self.reused_blob_count).is_err()
            || usize::try_from(self.batch_count).is_err()
        {
            return Err(String::from("VCE sharded embedding envelope is invalid"));
        }
        // Serialized bytes used to enforce the per-record metadata limit.
        // 用于执行单条记录元数据上限的序列化字节。
        let encoded = serde_json::to_vec(self)
            .map_err(|error| format!("Encode VCE sharded embedding envelope: {error}"))?;
        if encoded.len() > VCE_EMBEDDING_ENVELOPE_MAX_BYTES {
            return Err(String::from(
                "VCE sharded embedding envelope exceeds its metadata bound",
            ));
        }
        Ok(())
    }
}

/// Parent run row used to select legacy or sharded decoding.
/// 用于选择旧版或分片解码路径的父运行记录。
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct EmbeddingRunStorageRow {
    /// Persisted workspace generation.
    /// 已持久化工作区代次。
    generation: i64,
    /// Persisted run lifecycle text.
    /// 已持久化运行生命周期文本。
    state: String,
    /// Legacy full manifest or current bounded envelope JSON.
    /// 旧版完整清单或当前有界概要 JSON。
    manifest_json: String,
    /// Physical manifest storage layout.
    /// 清单物理存储布局。
    layout_version: i64,
    /// Whether every child row is fully published.
    /// 全部子记录是否已经完整发布。
    manifest_complete: i64,
    /// Optional immutable plan fingerprint; absent only for legacy rows.
    /// 可选不可变计划指纹；仅旧版记录允许缺失。
    plan_fingerprint: Option<String>,
    /// Original run creation timestamp.
    /// 原始运行创建时间戳。
    created_at_unix_ms: i64,
    /// Last durable run or batch mutation timestamp.
    /// 最近一次持久运行或批次变更时间戳。
    updated_at_unix_ms: i64,
    /// Monotonic storage revision covering the parent and every mutable batch row.
    /// 覆盖父记录及每条可变批次记录的单调存储修订号。
    revision: i64,
}

/// One ordered node JSON fragment returned from SQLite.
/// 从 SQLite 返回的一条有序节点 JSON 分片。
#[derive(Clone, Debug, Deserialize)]
struct EmbeddingNodeChunkRow {
    /// Zero-based node position.
    /// 从零开始的节点位置。
    node_index: i64,
    /// Zero-based fragment position inside the node.
    /// 节点内部从零开始的分片位置。
    chunk_index: i64,
    /// Frozen Blob identity repeated for fragment integrity.
    /// 为分片完整性重复保存的冻结 Blob 身份。
    blob_name: String,
    /// Exact UTF-8 JSON fragment.
    /// 精确 UTF-8 JSON 分片。
    node_json_chunk: String,
}

/// One ordered reused Blob identity returned from SQLite.
/// 从 SQLite 返回的一条有序复用 Blob 身份。
#[derive(Clone, Debug, Deserialize)]
struct ReusedBlobRow {
    /// Zero-based reuse position.
    /// 从零开始的复用位置。
    reused_index: i64,
    /// Frozen Blob identity.
    /// 冻结 Blob 身份。
    blob_name: String,
}

/// One ordered mutable provider batch returned from SQLite.
/// 从 SQLite 返回的一条有序可变供应商批次。
#[derive(Clone, Debug, Deserialize)]
struct EmbeddingBatchStorageRow {
    /// Zero-based batch position.
    /// 从零开始的批次位置。
    batch_index: i64,
    /// Stable batch identity.
    /// 稳定批次身份。
    batch_id: String,
    /// Fingerprint of immutable batch request fields.
    /// 不可变批次请求字段的指纹。
    identity_fingerprint: String,
    /// Complete bounded mutable batch JSON.
    /// 完整有界可变批次 JSON。
    batch_json: String,
    /// Timestamp copied atomically to the parent by the batch-update trigger.
    /// 由批次更新触发器原子复制到父记录的时间戳。
    updated_at_unix_ms: i64,
}

/// One bounded provider-result row used to validate a batch lifecycle transition.
/// 用于校验批次生命周期转换的一条有界供应商结果记录。
#[derive(Deserialize)]
struct EmbeddingResultStorageRow {
    /// Complete durable result JSON.
    /// 完整持久结果 JSON。
    result_json: String,
}

impl VceProjectIndex {
    /// Persist a complete plan in child shards, publishing the bounded parent envelope last.
    /// 将完整计划持久化到子分片，并在最后发布有界父概要。
    ///
    /// `manifest` supplies the complete plan; returns only after all shards and the final envelope are durable.
    /// `manifest` 提供完整计划；全部分片和最终概要持久化后才返回成功。
    pub(super) async fn persist_embedding_manifest_sharded(
        &self,
        manifest: VceEmbeddingRunManifest,
    ) -> Result<(), VcePortError> {
        self.require_embedding_manifest_write_access()?;
        manifest
            .validate()
            .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
        if manifest.namespace_id != self.binding.namespace_id {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest does not belong to this physical index",
            ));
        }
        // Digest of the frozen source plan, excluding mutable lifecycle fields.
        // 冻结源码计划的摘要，不包含可变生命周期字段。
        let plan_fingerprint = manifest
            .plan_fingerprint()
            .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
        // Immutable run envelope retained independently of source-node payloads.
        // 独立于源码节点负载保存的不可变运行概要。
        let envelope =
            StoredEmbeddingManifestEnvelope::from_manifest(&manifest, plan_fingerprint.clone())
                .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
        // Bounded envelope JSON stored in the parent run row.
        // 保存在父运行记录中的有界概要 JSON。
        let envelope_json = serde_json::to_string(&envelope).map_err(|error| {
            port_invalid(VcePortOperation::EmbeddingManifest, error.to_string())
        })?;
        // Workspace generation checked for SQLite integer representation.
        // 已校验 SQLite 整数表示的工作区代次。
        let generation = sqlite_manifest_u64(manifest.generation, "VCE manifest generation")?;
        // Original run creation time checked for SQLite storage.
        // 经过 SQLite 存储范围校验的原始运行创建时间。
        let created_at =
            sqlite_manifest_u64(manifest.created_at_unix_ms, "VCE manifest creation time")?;
        // Requested diagnostic time checked for SQLite storage.
        // 经过 SQLite 存储范围校验的请求诊断时间。
        let updated_at =
            sqlite_manifest_u64(manifest.updated_at_unix_ms, "VCE manifest update time")?;

        // A new parent stays invisible until every child row has been written and verified.
        // 新父记录在全部子记录写入并校验前保持不可见。
        // An incomplete first write has admitted no provider requests, so retry may replace its freshly prepared plan.
        // 未完成的首次写入尚未准入供应商请求，因此重试可以替换重新准备的计划。
        self.data_plane
            .execute(
                r#"INSERT INTO vce_embedding_runs(
                       namespace_id, run_id, generation, state, manifest_json,
                       layout_version, manifest_complete, plan_fingerprint,
                       created_at_unix_ms, updated_at_unix_ms
                   ) VALUES (?, ?, ?, ?, ?, 2, 0, ?, ?, ?)
                   ON CONFLICT(namespace_id, run_id) DO UPDATE SET
                       generation = excluded.generation, state = excluded.state,
                       manifest_json = excluded.manifest_json,
                       plan_fingerprint = excluded.plan_fingerprint,
                       created_at_unix_ms = excluded.created_at_unix_ms,
                       updated_at_unix_ms = excluded.updated_at_unix_ms,
                       revision = vce_embedding_runs.revision + 1
                   WHERE vce_embedding_runs.layout_version = 2
                     AND vce_embedding_runs.manifest_complete = 0;"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&manifest.run_id),
                    ControllerSqliteValue::Int64(generation),
                    sqlite_text(embedding_run_state_text(manifest.state)),
                    sqlite_text(&envelope_json),
                    sqlite_text(&plan_fingerprint),
                    ControllerSqliteValue::Int64(created_at),
                    ControllerSqliteValue::Int64(updated_at),
                ],
                "Persist VCE embedding manifest parent",
            )
            .await
            .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;

        // Current durable record used to validate ownership before mutation.
        // 用于在变更前校验归属的当前持久记录。
        let existing = self
            .load_embedding_run_storage_row(&manifest.run_id)
            .await?
            .ok_or_else(|| {
                port_invalid(
                    VcePortOperation::EmbeddingManifest,
                    "persisted embedding manifest parent is missing",
                )
            })?;
        self.validate_existing_plan(&manifest, &plan_fingerprint, &existing)?;

        if existing.layout_version == VCE_EMBEDDING_MANIFEST_LAYOUT_SHARDED
            && existing.manifest_complete == 1
        {
            let existing_state = parse_embedding_run_state(&existing.state)?;
            validate_embedding_run_state_transition(existing_state, manifest.state)?;
            if updated_at < existing.updated_at_unix_ms {
                return Err(port_rejected(
                    VcePortOperation::EmbeddingManifest,
                    "embedding manifest update timestamp moved backwards",
                ));
            }
            // Current batch projections checked before a parent-only update.
            // 父记录单独更新前校验的当前批次投影。
            let stored_batches = self
                .load_embedding_batches(&manifest.run_id, envelope.batch_count)
                .await?;
            if stored_batches != manifest.batches {
                return Err(port_rejected(
                    VcePortOperation::EmbeddingManifest,
                    "published embedding batches must be updated through the granular batch API",
                ));
            }
            self.publish_embedding_manifest_parent(
                &manifest,
                &envelope_json,
                &plan_fingerprint,
                existing.revision,
            )
            .await?;
            return Ok(());
        }

        self.clear_embedding_manifest_children(&manifest.run_id)
            .await?;
        self.persist_reused_blobs(&manifest.run_id, &manifest.reused_blob_names)
            .await?;
        self.persist_embedding_nodes(&manifest.run_id, &manifest.nodes)
            .await?;
        self.persist_embedding_batches(
            &manifest.run_id,
            &manifest.batches,
            manifest.updated_at_unix_ms,
        )
        .await?;
        self.publish_embedding_manifest_parent(
            &manifest,
            &envelope_json,
            &plan_fingerprint,
            existing.revision,
        )
        .await
    }

    /// Load one complete legacy or sharded plan without exposing partial child writes.
    /// 读取一份完整旧版或分片计划，不暴露部分子记录写入。
    ///
    /// `run_id` selects one exact plan. Returns a consistent complete plan, absence for unpublished data, or an error.
    /// `run_id` 选择精确计划；返回一致的完整计划，未发布数据返回空值，失败返回错误。
    pub(super) async fn load_embedding_manifest_sharded(
        &self,
        run_id: String,
    ) -> Result<Option<VceEmbeddingRunManifest>, VcePortError> {
        validate_run_id(&run_id)?;
        for attempt in 0..VCE_EMBEDDING_MANIFEST_LOAD_ATTEMPTS {
            let Some(row) = self.load_embedding_run_storage_row(&run_id).await? else {
                return Ok(None);
            };
            if row.layout_version == VCE_EMBEDDING_MANIFEST_LAYOUT_LEGACY {
                return self.decode_legacy_manifest(&run_id, &row).map(Some);
            }
            if row.layout_version != VCE_EMBEDDING_MANIFEST_LAYOUT_SHARDED {
                return Err(port_invalid(
                    VcePortOperation::EmbeddingManifest,
                    "persisted embedding manifest layout is unsupported",
                ));
            }
            if row.manifest_complete == 0 {
                return Ok(None);
            }
            if row.manifest_complete != 1 || row.revision <= 0 {
                return Err(port_invalid(
                    VcePortOperation::EmbeddingManifest,
                    "sharded embedding manifest completion marker or revision is invalid",
                ));
            }

            // Child pages may yield between controller calls; the second parent read proves that
            // every reconstructed batch belongs to one stable revision.
            // 子记录分页会在控制器调用之间让出执行权；第二次父记录读取证明全部重建批次属于同一稳定修订。
            let loaded = self
                .load_complete_sharded_manifest(&run_id, row.clone())
                .await;
            // Second parent observation that detects mutations between child pages.
            // 用于检测子记录分页之间变更的第二次父记录观察。
            let after = self.load_embedding_run_storage_row(&run_id).await?;
            if after.as_ref() == Some(&row) {
                return loaded.map(Some);
            }
            if attempt + 1 == VCE_EMBEDDING_MANIFEST_LOAD_ATTEMPTS {
                return Err(port_failed(
                    VcePortOperation::EmbeddingManifest,
                    "embedding manifest changed during every consistent-read attempt",
                ));
            }
        }
        unreachable!("positive VCE embedding manifest load attempt bound")
    }

    /// Persist only one run state and timestamp after the complete plan has been published.
    /// 在完整计划发布后仅持久化一次运行状态与时间戳。
    ///
    /// `run_id`, `state`, and `updated_at_unix_ms` identify the lifecycle update; returns after revision-checked persistence.
    /// `run_id`、`state` 与 `updated_at_unix_ms` 标识生命周期更新；修订校验和持久化完成后返回。
    pub(super) async fn persist_embedding_run_state_sharded(
        &self,
        run_id: String,
        state: VceEmbeddingRunState,
        updated_at_unix_ms: u64,
    ) -> Result<(), VcePortError> {
        self.require_embedding_manifest_write_access()?;
        validate_run_id(&run_id)?;
        // Requested diagnostic time checked for SQLite storage.
        // 经过 SQLite 存储范围校验的请求诊断时间。
        let updated_at = sqlite_manifest_u64(updated_at_unix_ms, "VCE manifest update time")?;
        // Current durable record used to validate ownership before mutation.
        // 用于在变更前校验归属的当前持久记录。
        let existing = self.prepare_embedding_parent_for_update(&run_id).await?;
        require_complete_sharded_parent(&existing)?;
        let existing_state = parse_embedding_run_state(&existing.state)?;
        validate_embedding_run_state_transition(existing_state, state)?;
        // Exact successor revision required for this conditional update.
        // 本次条件更新要求的精确后继修订号。
        let expected_revision = next_embedding_manifest_revision(existing.revision)?;
        if updated_at < existing.updated_at_unix_ms {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest update timestamp moved backwards",
            ));
        }
        if matches!(
            state,
            VceEmbeddingRunState::CandidateReady | VceEmbeddingRunState::Succeeded
        ) {
            // Bounded child rows are sufficient to validate publication readiness without loading source nodes.
            // 有界子记录足以校验发布就绪状态，无需读取源码节点。
            let envelope = decode_envelope(&existing.manifest_json)?;
            let batches = self
                .load_embedding_batches(&run_id, envelope.batch_count)
                .await?;
            validate_embedding_run_batch_consistency(state, &batches)?;
        }
        self.data_plane
            .execute(
                r#"UPDATE vce_embedding_runs
                   SET state = ?, updated_at_unix_ms = ?, revision = revision + 1
                   WHERE namespace_id = ? AND run_id = ?
                      AND layout_version = 2 AND manifest_complete = 1
                      AND revision = ?;"#,
                vec![
                    sqlite_text(embedding_run_state_text(state)),
                    ControllerSqliteValue::Int64(updated_at),
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&run_id),
                    ControllerSqliteValue::Int64(existing.revision),
                ],
                "Persist VCE embedding run state",
            )
            .await
            .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        // Read-back record used to verify that the requested mutation was retained.
        // 用于验证请求变更已保留的回读记录。
        let stored = self
            .load_embedding_run_storage_row(&run_id)
            .await?
            .ok_or_else(|| {
                port_invalid(
                    VcePortOperation::EmbeddingManifest,
                    "updated embedding manifest run disappeared",
                )
            })?;
        if stored.revision != expected_revision {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest run changed concurrently",
            ));
        }
        if stored.state != embedding_run_state_text(state)
            || stored.updated_at_unix_ms != updated_at
            || stored.layout_version != VCE_EMBEDDING_MANIFEST_LAYOUT_SHARDED
            || stored.manifest_complete != 1
        {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest run state update was not retained",
            ));
        }
        Ok(())
    }

    /// Persist one mutable batch row and atomically advance its parent's diagnostic timestamp and revision.
    /// 持久化一条可变批次记录，并原子推进父记录的诊断时间戳与修订号。
    ///
    /// `run_id` owns `batch`; `updated_at_unix_ms` advances its diagnostic time. Returns after atomic child-parent persistence.
    /// `run_id` 拥有 `batch`，`updated_at_unix_ms` 推进其诊断时间；子父记录原子持久化后返回。
    pub(super) async fn persist_embedding_batch_record_sharded(
        &self,
        run_id: String,
        batch: VceEmbeddingBatchRecord,
        updated_at_unix_ms: u64,
    ) -> Result<(), VcePortError> {
        self.require_embedding_manifest_write_access()?;
        validate_run_id(&run_id)?;
        batch
            .validate()
            .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
        // Digest of ordered immutable batch request fields.
        // 有序不可变批次请求字段的摘要。
        let identity_fingerprint = batch
            .identity_fingerprint()
            .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
        // Validated single-batch JSON, bounded independently of project size.
        // 已校验的单批次 JSON，其上限独立于项目规模。
        let batch_json = serde_json::to_string(&batch).map_err(|error| {
            port_invalid(VcePortOperation::EmbeddingManifest, error.to_string())
        })?;
        // Requested diagnostic time checked for SQLite storage.
        // 经过 SQLite 存储范围校验的请求诊断时间。
        let updated_at = sqlite_manifest_u64(updated_at_unix_ms, "VCE manifest update time")?;
        // Owning run record that gates the requested batch or lifecycle operation.
        // 为请求批次或生命周期操作提供准入的所属运行记录。
        let parent = self.prepare_embedding_parent_for_update(&run_id).await?;
        require_complete_sharded_parent(&parent)?;
        // Exact successor revision required for this conditional update.
        // 本次条件更新要求的精确后继修订号。
        let expected_revision = next_embedding_manifest_revision(parent.revision)?;
        if updated_at < parent.updated_at_unix_ms {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest batch timestamp moved backwards",
            ));
        }
        if parse_embedding_run_state(&parent.state)? != VceEmbeddingRunState::Running {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding batches can change only while their run is active",
            ));
        }
        // Current durable record used to validate ownership before mutation.
        // 用于在变更前校验归属的当前持久记录。
        let existing = self
            .load_embedding_batch_row_by_id(&run_id, &batch.batch_id)
            .await?
            .ok_or_else(|| {
                port_rejected(
                    VcePortOperation::EmbeddingManifest,
                    "embedding batch does not belong to the published plan",
                )
            })?;
        if existing.identity_fingerprint != identity_fingerprint {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding batch immutable identity changed",
            ));
        }
        let existing_batch = decode_batch(&existing)?;
        validate_embedding_batch_transition(&existing_batch, &batch)?;
        let result = self
            .load_embedding_result_for_transition(&run_id, &batch.batch_id)
            .await?;
        if batch.state == VceEmbeddingBatchState::Succeeded {
            let result = result.ok_or_else(|| {
                port_rejected(
                    VcePortOperation::EmbeddingManifest,
                    "embedding batch cannot succeed before its durable result exists",
                )
            })?;
            let result_ids = result
                .vectors
                .iter()
                .map(|vector| vector.input_id.as_str())
                .collect::<BTreeSet<_>>();
            let batch_ids = batch
                .input_ids
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if result.vectors.len() != batch.input_ids.len() || result_ids != batch_ids {
                return Err(port_rejected(
                    VcePortOperation::EmbeddingManifest,
                    "embedding batch result does not cover its frozen request",
                ));
            }
        } else if existing_batch.state == VceEmbeddingBatchState::Succeeded
            && batch.state == VceEmbeddingBatchState::Pending
            && result.is_some()
        {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding batch success cannot reset while its durable result exists",
            ));
        }
        self.data_plane
            .execute(
                r#"UPDATE vce_embedding_run_batches
                   SET batch_json = ?, updated_at_unix_ms = ?
                   WHERE namespace_id = ? AND run_id = ? AND batch_id = ?
                      AND identity_fingerprint = ?
                      AND EXISTS(
                          SELECT 1 FROM vce_embedding_runs
                          WHERE namespace_id = ? AND run_id = ?
                            AND layout_version = 2 AND manifest_complete = 1
                            AND revision = ?
                      );"#,
                vec![
                    sqlite_text(&batch_json),
                    ControllerSqliteValue::Int64(updated_at),
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&run_id),
                    sqlite_text(&batch.batch_id),
                    sqlite_text(&identity_fingerprint),
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&run_id),
                    ControllerSqliteValue::Int64(parent.revision),
                ],
                "Persist VCE embedding batch state",
            )
            .await
            .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        // Parent read-back proving the batch trigger advanced the same transaction.
        // 证明批次触发器在同一事务中推进状态的父记录回读。
        let stored_parent = self
            .load_embedding_run_storage_row(&run_id)
            .await?
            .ok_or_else(|| {
                port_invalid(
                    VcePortOperation::EmbeddingManifest,
                    "updated embedding manifest parent disappeared",
                )
            })?;
        if stored_parent.revision != expected_revision {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest batch changed concurrently",
            ));
        }
        if stored_parent.updated_at_unix_ms != updated_at {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest batch timestamp was not retained",
            ));
        }
        // Read-back record used to verify that the requested mutation was retained.
        // 用于验证请求变更已保留的回读记录。
        let stored = self
            .load_embedding_batch_row_by_id(&run_id, &batch.batch_id)
            .await?
            .ok_or_else(|| {
                port_invalid(
                    VcePortOperation::EmbeddingManifest,
                    "updated embedding batch disappeared",
                )
            })?;
        if stored.identity_fingerprint != identity_fingerprint
            || stored.batch_json != batch_json
            || stored.updated_at_unix_ms != updated_at
        {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "embedding batch state update was not retained",
            ));
        }
        Ok(())
    }

    /// Require one exact batch to belong to a completely published legacy or current-layout plan.
    /// 要求一个精确批次属于已经完整发布的旧版或当前布局计划。
    ///
    /// `run_id` and `batch_id` select the exact persisted membership; returns unit only for a validated complete plan.
    /// `run_id` 与 `batch_id` 选择精确持久成员；仅对已校验完整计划返回成功。
    pub(super) async fn require_published_embedding_batch(
        &self,
        run_id: &str,
        batch_id: &str,
    ) -> Result<(), VcePortError> {
        self.load_published_embedding_batch(run_id, batch_id)
            .await
            .map(|_| ())
    }

    /// Load the exact validated batch that authorizes a provider-result row.
    /// 读取授权一条供应商结果记录的精确已校验批次。
    ///
    /// `run_id` and `batch_id` select frozen plan membership. Returns that batch only when its
    /// complete legacy or current-layout parent is published.
    /// `run_id` 与 `batch_id` 选择冻结计划成员。仅当其完整旧版或当前布局父记录已发布时返回该批次。
    pub(super) async fn load_published_embedding_batch(
        &self,
        run_id: &str,
        batch_id: &str,
    ) -> Result<VceEmbeddingBatchRecord, VcePortError> {
        validate_run_id(run_id)?;
        if batch_id.trim().is_empty() || batch_id.len() > 8192 {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding batch identity is empty or exceeds its bound",
            ));
        }
        // Owning run record that gates the requested batch or lifecycle operation.
        // 为请求批次或生命周期操作提供准入的所属运行记录。
        let parent = self
            .load_embedding_run_storage_row(run_id)
            .await?
            .ok_or_else(|| {
                port_rejected(
                    VcePortOperation::EmbeddingManifest,
                    "embedding manifest run does not exist",
                )
            })?;
        if parent.layout_version == VCE_EMBEDDING_MANIFEST_LAYOUT_LEGACY {
            // Validated legacy plan retained during its first-write conversion.
            // 首次写入转换期间保留的已校验旧版计划。
            let legacy = self.decode_legacy_manifest(run_id, &parent)?;
            return legacy
                .batches
                .into_iter()
                .find(|batch| batch.batch_id == batch_id)
                .ok_or_else(|| {
                    port_rejected(
                        VcePortOperation::EmbeddingManifest,
                        "embedding batch does not belong to the published legacy plan",
                    )
                });
        }
        require_complete_sharded_parent(&parent)?;
        // Exact persisted row selected by the current namespace and stable run identity.
        // 按当前命名空间和稳定运行身份选出的精确持久记录。
        let row = self
            .load_embedding_batch_row_by_id(run_id, batch_id)
            .await?
            .ok_or_else(|| {
                port_rejected(
                    VcePortOperation::EmbeddingManifest,
                    "embedding batch does not belong to the published plan",
                )
            })?;
        decode_batch(&row)
    }

    /// Reject writes through pinned read views before touching any relational rows.
    /// 在触碰任何关系记录前拒绝通过固定读取视图执行写入。
    ///
    /// Checks this index handle; returns a rejection for pinned read-only views.
    /// 检查当前索引句柄；对固定只读视图返回拒绝错误。
    fn require_embedding_manifest_write_access(&self) -> Result<(), VcePortError> {
        if self.read_view.is_some() {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "VCE pinned read views cannot persist embedding manifests",
            ));
        }
        Ok(())
    }

    /// Load the exact update target, lazily converting a complete legacy plan on its first write.
    /// 读取精确更新目标，并在首次写入时惰性转换完整旧版计划。
    ///
    /// `run_id` identifies an existing run; returns its current sharded parent or a storage error.
    /// `run_id` 标识现有运行；返回当前分片父记录，失败时返回存储错误。
    async fn prepare_embedding_parent_for_update(
        &self,
        run_id: &str,
    ) -> Result<EmbeddingRunStorageRow, VcePortError> {
        // The original parent is read before deciding whether a physical conversion is required.
        // 先读取原始父记录，再决定是否需要物理转换。
        let parent = self
            .load_embedding_run_storage_row(run_id)
            .await?
            .ok_or_else(|| {
                port_rejected(
                    VcePortOperation::EmbeddingManifest,
                    "embedding manifest run does not exist",
                )
            })?;
        if parent.layout_version != VCE_EMBEDDING_MANIFEST_LAYOUT_LEGACY {
            return Ok(parent);
        }
        // Conversion preserves the frozen plan and result rows while enabling granular updates.
        // 转换保留冻结计划和结果记录，同时启用细粒度更新。
        let legacy = self.decode_legacy_manifest(run_id, &parent)?;
        self.persist_embedding_manifest_sharded(legacy).await?;
        self.load_embedding_run_storage_row(run_id)
            .await?
            .ok_or_else(|| {
                port_invalid(
                    VcePortOperation::EmbeddingManifest,
                    "converted embedding manifest disappeared",
                )
            })
    }

    /// Decode legacy payloads using the parent lifecycle updated by atomic checkpoint commits.
    /// 使用原子检查点提交更新的父记录生命周期解码旧版负载。
    ///
    /// `run_id` and `row` must identify the same complete legacy plan; returns validated core data.
    /// `run_id` 与 `row` 必须标识同一完整旧版计划；返回已校验的核心数据。
    fn decode_legacy_manifest(
        &self,
        run_id: &str,
        row: &EmbeddingRunStorageRow,
    ) -> Result<VceEmbeddingRunManifest, VcePortError> {
        if row.manifest_complete != 1 || row.revision <= 0 {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "legacy embedding manifest completion marker or revision is invalid",
            ));
        }
        // Legacy commit SQL updates the parent state without rewriting the project-sized JSON.
        // 旧版提交 SQL 更新父记录状态，不会重写整个项目规模的 JSON。
        let mut manifest: VceEmbeddingRunManifest = serde_json::from_str(&row.manifest_json)
            .map_err(|error| {
                port_invalid(VcePortOperation::EmbeddingManifest, error.to_string())
            })?;
        if u64::try_from(row.generation).ok() != Some(manifest.generation)
            || u64::try_from(row.created_at_unix_ms).ok() != Some(manifest.created_at_unix_ms)
        {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "legacy embedding manifest identity disagrees with its parent",
            ));
        }
        manifest.state = parse_embedding_run_state(&row.state)?;
        manifest.updated_at_unix_ms =
            sqlite_manifest_timestamp(row.updated_at_unix_ms, "VCE manifest update time")?;
        self.validate_loaded_manifest(run_id, manifest)
    }

    /// Read one exact parent row under this index namespace.
    /// 在当前索引命名空间下读取一条精确父记录。
    ///
    /// `run_id` scopes the query inside this namespace. Returns an optional raw parent row without writes.
    /// `run_id` 在当前命名空间内限定查询；返回可选原始父记录，不执行写入。
    async fn load_embedding_run_storage_row(
        &self,
        run_id: &str,
    ) -> Result<Option<EmbeddingRunStorageRow>, VcePortError> {
        // One bounded controller result page, scoped to the exact manifest identity.
        // 范围限定到精确清单身份的一页有界控制器结果。
        let rows: Vec<EmbeddingRunStorageRow> = self
            .data_plane
            .query_rows(
                r#"SELECT generation, state, manifest_json, layout_version,
                          manifest_complete, plan_fingerprint,
                          created_at_unix_ms, updated_at_unix_ms, revision
                   FROM vce_embedding_runs
                   WHERE namespace_id = ? AND run_id = ? LIMIT 1;"#,
                vec![sqlite_text(&self.binding.namespace_id), sqlite_text(run_id)],
            )
            .await
            .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        Ok(rows.into_iter().next())
    }

    /// Verify an existing parent is owned by the same immutable plan before any child replacement.
    /// 在替换任何子记录前校验现有父记录属于同一不可变计划。
    ///
    /// Compares `manifest` and `plan_fingerprint` with `existing`; returns a rejection before changing a different plan.
    /// 将 `manifest` 和 `plan_fingerprint` 与 `existing` 比较；在修改不同计划前返回拒绝错误。
    fn validate_existing_plan(
        &self,
        manifest: &VceEmbeddingRunManifest,
        plan_fingerprint: &str,
        existing: &EmbeddingRunStorageRow,
    ) -> Result<(), VcePortError> {
        if existing.revision <= 0 {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "persisted embedding manifest revision is invalid",
            ));
        }
        // Persisted generation decoded before immutable-plan comparison.
        // 在不可变计划比较前解码的持久化代次。
        let stored_generation = u64::try_from(existing.generation).map_err(|_| {
            port_invalid(
                VcePortOperation::EmbeddingManifest,
                "persisted embedding manifest generation is invalid",
            )
        })?;
        if stored_generation != manifest.generation {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest run belongs to a different generation",
            ));
        }
        // Caller creation time compared against the immutable published run identity.
        // 与已发布运行不可变身份比较的调用方创建时间。
        let expected_created_at =
            sqlite_manifest_u64(manifest.created_at_unix_ms, "VCE manifest creation time")?;
        if existing.created_at_unix_ms != expected_created_at {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest run creation timestamp changed",
            ));
        }
        match existing.layout_version {
            VCE_EMBEDDING_MANIFEST_LAYOUT_LEGACY => {
                // Validated legacy plan retained during its first-write conversion.
                // 首次写入转换期间保留的已校验旧版计划。
                let legacy = self.decode_legacy_manifest(&manifest.run_id, existing)?;
                // Frozen-plan digest calculated from the original legacy payload.
                // 根据原始旧版负载计算的冻结计划摘要。
                let legacy_fingerprint = legacy
                    .plan_fingerprint()
                    .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
                if legacy_fingerprint != plan_fingerprint {
                    return Err(port_rejected(
                        VcePortOperation::EmbeddingManifest,
                        "legacy embedding manifest does not match the frozen source plan",
                    ));
                }
            }
            VCE_EMBEDDING_MANIFEST_LAYOUT_SHARDED => {
                if existing.plan_fingerprint.as_deref() != Some(plan_fingerprint) {
                    return Err(port_rejected(
                        VcePortOperation::EmbeddingManifest,
                        "embedding manifest does not match the published source plan",
                    ));
                }
                // Immutable run envelope retained independently of source-node payloads.
                // 独立于源码节点负载保存的不可变运行概要。
                let envelope = decode_envelope(&existing.manifest_json)?;
                if envelope.run_id != manifest.run_id
                    || envelope.namespace_id != self.binding.namespace_id
                    || envelope.generation != manifest.generation
                    || envelope.plan_fingerprint != plan_fingerprint
                {
                    return Err(port_invalid(
                        VcePortOperation::EmbeddingManifest,
                        "persisted embedding manifest envelope disagrees with its parent",
                    ));
                }
            }
            _ => {
                return Err(port_invalid(
                    VcePortOperation::EmbeddingManifest,
                    "persisted embedding manifest layout is unsupported",
                ));
            }
        }
        Ok(())
    }

    /// Delete only unpublished or legacy-conversion child rows for one verified run.
    /// 仅删除一个已校验运行中尚未发布或旧版转换产生的子记录。
    ///
    /// `run_id` must name an unpublished or legacy-conversion run; returns after its replaceable children are removed.
    /// `run_id` 必须标识未发布或旧版转换运行；其可替换子记录移除后返回。
    async fn clear_embedding_manifest_children(&self, run_id: &str) -> Result<(), VcePortError> {
        for (table, context) in [
            (
                "vce_embedding_run_node_chunks",
                "Clear VCE embedding node chunks",
            ),
            (
                "vce_embedding_run_reused_blobs",
                "Clear VCE reused Blob rows",
            ),
            (
                "vce_embedding_run_batches",
                "Clear VCE embedding batch rows",
            ),
        ] {
            // Deletion statement restricted to a fixed child table and one exact run.
            // 仅限固定子表和一个精确运行的删除语句。
            let sql = format!("DELETE FROM {table} WHERE namespace_id = ? AND run_id = ?;");
            self.data_plane
                .execute(
                    &sql,
                    vec![sqlite_text(&self.binding.namespace_id), sqlite_text(run_id)],
                    context,
                )
                .await
                .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        }
        Ok(())
    }

    /// Persist reused Blob identities in bounded controller batches and stable order.
    /// 以有界控制器批次和稳定顺序持久化复用 Blob 身份。
    ///
    /// Persists ordered `blob_names` for `run_id`; returns after every bounded page succeeds.
    /// 为 `run_id` 持久化有序 `blob_names`；所有有界分页成功后返回。
    async fn persist_reused_blobs(
        &self,
        run_id: &str,
        blob_names: &[String],
    ) -> Result<(), VcePortError> {
        for (page_index, page) in blob_names
            .chunks(VCE_EMBEDDING_REUSED_PAGE_SIZE)
            .enumerate()
        {
            // Checked absolute position of this bounded page in the ordered collection.
            // 本有界页在有序集合中经过校验的绝对位置。
            let page_start = page_index
                .checked_mul(VCE_EMBEDDING_REUSED_PAGE_SIZE)
                .ok_or_else(|| {
                    port_invalid(
                        VcePortOperation::EmbeddingManifest,
                        "VCE reused Blob index overflowed",
                    )
                })?;
            // Bounded typed row parameters sent through a single controller batch.
            // 通过一次控制器批次发送的有界类型化行参数。
            let mut items = Vec::with_capacity(page.len());
            for (page_offset, blob_name) in page.iter().enumerate() {
                // Checked stable position of the reused Blob within the frozen plan.
                // 复用 Blob 在冻结计划内经过校验的稳定位置。
                let reused_index = page_start.checked_add(page_offset).ok_or_else(|| {
                    port_invalid(
                        VcePortOperation::EmbeddingManifest,
                        "VCE reused Blob index overflowed",
                    )
                })?;
                items.push(vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(run_id),
                    ControllerSqliteValue::Int64(sqlite_manifest_usize(
                        reused_index,
                        "VCE reused Blob index",
                    )?),
                    sqlite_text(blob_name),
                ]);
            }
            self.data_plane
                .execute_batch(
                    r#"INSERT INTO vce_embedding_run_reused_blobs(
                           namespace_id, run_id, reused_index, blob_name
                       ) VALUES (?, ?, ?, ?);"#,
                    items,
                    "Persist VCE reused Blob rows",
                )
                .await
                .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        }
        Ok(())
    }

    /// Persist serialized source-node snapshots as bounded UTF-8 fragments.
    /// 将序列化源码节点快照持久化为有界 UTF-8 分片。
    ///
    /// Persists ordered `nodes` for `run_id`; returns only after the final UTF-8 fragment page succeeds.
    /// 为 `run_id` 持久化有序 `nodes`；最后一页 UTF-8 分片成功后才返回。
    async fn persist_embedding_nodes(
        &self,
        run_id: &str,
        nodes: &[VceCodeNodeSnapshot],
    ) -> Result<(), VcePortError> {
        // Bounded typed row parameters sent through a single controller batch.
        // 通过一次控制器批次发送的有界类型化行参数。
        let mut items = Vec::with_capacity(VCE_EMBEDDING_NODE_CHUNK_PAGE_SIZE);
        for (node_index, node) in nodes.iter().enumerate() {
            // One source-node JSON body split into bounded UTF-8 fragments.
            // 将拆分为有界 UTF-8 分片的单个源码节点 JSON 正文。
            let node_json = serde_json::to_string(node).map_err(|error| {
                port_invalid(VcePortOperation::EmbeddingManifest, error.to_string())
            })?;
            for (chunk_index, chunk) in split_utf8_chunks(&node_json).into_iter().enumerate() {
                items.push(vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(run_id),
                    ControllerSqliteValue::Int64(sqlite_manifest_usize(
                        node_index,
                        "VCE embedding node index",
                    )?),
                    ControllerSqliteValue::Int64(sqlite_manifest_usize(
                        chunk_index,
                        "VCE embedding node chunk index",
                    )?),
                    sqlite_text(&node.blob_name),
                    sqlite_text(chunk),
                ]);
                if items.len() == VCE_EMBEDDING_NODE_CHUNK_PAGE_SIZE {
                    self.flush_embedding_node_chunks(std::mem::take(&mut items))
                        .await?;
                }
            }
        }
        self.flush_embedding_node_chunks(items).await
    }

    /// Flush one bounded collection of node fragments through the controller.
    /// 通过控制器写入一组有界节点分片。
    ///
    /// `items` contains one bounded typed parameter page; returns the controller write result.
    /// `items` 包含一页有界类型化参数；返回控制器写入结果。
    async fn flush_embedding_node_chunks(
        &self,
        items: Vec<Vec<ControllerSqliteValue>>,
    ) -> Result<(), VcePortError> {
        self.data_plane
            .execute_batch(
                r#"INSERT INTO vce_embedding_run_node_chunks(
                       namespace_id, run_id, node_index, chunk_index,
                       blob_name, node_json_chunk
                   ) VALUES (?, ?, ?, ?, ?, ?);"#,
                items,
                "Persist VCE embedding node chunks",
            )
            .await
            .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))
    }

    /// Persist bounded mutable batch records in stable execution order.
    /// 以稳定执行顺序持久化有界可变批次记录。
    ///
    /// Persists ordered `batches` for `run_id` with `updated_at_unix_ms`; returns after all bounded writes.
    /// 以 `updated_at_unix_ms` 为 `run_id` 持久化有序 `batches`；所有有界写入完成后返回。
    async fn persist_embedding_batches(
        &self,
        run_id: &str,
        batches: &[VceEmbeddingBatchRecord],
        updated_at_unix_ms: u64,
    ) -> Result<(), VcePortError> {
        // Requested diagnostic time checked for SQLite storage.
        // 经过 SQLite 存储范围校验的请求诊断时间。
        let updated_at = sqlite_manifest_u64(updated_at_unix_ms, "VCE manifest update time")?;
        for (page_index, page) in batches.chunks(VCE_EMBEDDING_BATCH_PAGE_SIZE).enumerate() {
            // Checked absolute position of this bounded page in the ordered collection.
            // 本有界页在有序集合中经过校验的绝对位置。
            let page_start = page_index
                .checked_mul(VCE_EMBEDDING_BATCH_PAGE_SIZE)
                .ok_or_else(|| {
                    port_invalid(
                        VcePortOperation::EmbeddingManifest,
                        "VCE embedding batch index overflowed",
                    )
                })?;
            // Bounded typed row parameters sent through a single controller batch.
            // 通过一次控制器批次发送的有界类型化行参数。
            let mut items = Vec::with_capacity(page.len());
            for (page_offset, batch) in page.iter().enumerate() {
                batch
                    .validate()
                    .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
                // Checked stable batch position independent of mutable state.
                // 经过校验且独立于可变状态的稳定批次位置。
                let batch_index = page_start.checked_add(page_offset).ok_or_else(|| {
                    port_invalid(
                        VcePortOperation::EmbeddingManifest,
                        "VCE embedding batch index overflowed",
                    )
                })?;
                // Digest of ordered immutable batch request fields.
                // 有序不可变批次请求字段的摘要。
                let identity_fingerprint = batch
                    .identity_fingerprint()
                    .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
                // Validated single-batch JSON, bounded independently of project size.
                // 已校验的单批次 JSON，其上限独立于项目规模。
                let batch_json = serde_json::to_string(batch).map_err(|error| {
                    port_invalid(VcePortOperation::EmbeddingManifest, error.to_string())
                })?;
                items.push(vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(run_id),
                    ControllerSqliteValue::Int64(sqlite_manifest_usize(
                        batch_index,
                        "VCE embedding batch index",
                    )?),
                    sqlite_text(&batch.batch_id),
                    sqlite_text(identity_fingerprint),
                    sqlite_text(batch_json),
                    ControllerSqliteValue::Int64(updated_at),
                ]);
            }
            self.data_plane
                .execute_batch(
                    r#"INSERT INTO vce_embedding_run_batches(
                           namespace_id, run_id, batch_index, batch_id,
                           identity_fingerprint, batch_json, updated_at_unix_ms
                       ) VALUES (?, ?, ?, ?, ?, ?, ?)
                       ON CONFLICT(namespace_id, run_id, batch_index) DO UPDATE SET
                           batch_id = excluded.batch_id,
                           identity_fingerprint = excluded.identity_fingerprint,
                           batch_json = excluded.batch_json,
                           updated_at_unix_ms = excluded.updated_at_unix_ms;"#,
                    items,
                    "Persist VCE embedding batch rows",
                )
                .await
                .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        }
        Ok(())
    }

    /// Publish the bounded parent envelope only after every required child row is durable.
    /// 仅在全部必要子记录持久化后发布有界父概要。
    ///
    /// Publishes `manifest` through `envelope_json` and `plan_fingerprint` only at `current_revision`; returns verified durability.
    /// 仅在 `current_revision` 上通过 `envelope_json` 和 `plan_fingerprint` 发布 `manifest`；返回已核实的持久化结果。
    async fn publish_embedding_manifest_parent(
        &self,
        manifest: &VceEmbeddingRunManifest,
        envelope_json: &str,
        plan_fingerprint: &str,
        current_revision: i64,
    ) -> Result<(), VcePortError> {
        // Exact successor revision required for this conditional update.
        // 本次条件更新要求的精确后继修订号。
        let expected_revision = next_embedding_manifest_revision(current_revision)?;
        self.data_plane
            .execute(
                r#"UPDATE vce_embedding_runs
                   SET generation = ?, state = ?, manifest_json = ?,
                        layout_version = 2, manifest_complete = 1,
                        plan_fingerprint = ?, updated_at_unix_ms = ?,
                        revision = revision + 1
                    WHERE namespace_id = ? AND run_id = ?
                      AND revision = ?
                      AND (layout_version = 1 OR plan_fingerprint = ?)
                     AND (SELECT COUNT(DISTINCT node_index)
                          FROM vce_embedding_run_node_chunks
                          WHERE namespace_id = ? AND run_id = ?) = ?
                     AND (SELECT COUNT(*) FROM vce_embedding_run_reused_blobs
                          WHERE namespace_id = ? AND run_id = ?) = ?
                     AND (SELECT COUNT(*) FROM vce_embedding_run_batches
                          WHERE namespace_id = ? AND run_id = ?) = ?;"#,
                vec![
                    ControllerSqliteValue::Int64(sqlite_manifest_u64(
                        manifest.generation,
                        "VCE manifest generation",
                    )?),
                    sqlite_text(embedding_run_state_text(manifest.state)),
                    sqlite_text(envelope_json),
                    sqlite_text(plan_fingerprint),
                    ControllerSqliteValue::Int64(sqlite_manifest_u64(
                        manifest.updated_at_unix_ms,
                        "VCE manifest update time",
                    )?),
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&manifest.run_id),
                    ControllerSqliteValue::Int64(current_revision),
                    sqlite_text(plan_fingerprint),
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&manifest.run_id),
                    ControllerSqliteValue::Int64(sqlite_manifest_usize(
                        manifest.nodes.len(),
                        "VCE embedding node count",
                    )?),
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&manifest.run_id),
                    ControllerSqliteValue::Int64(sqlite_manifest_usize(
                        manifest.reused_blob_names.len(),
                        "VCE reused Blob count",
                    )?),
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&manifest.run_id),
                    ControllerSqliteValue::Int64(sqlite_manifest_usize(
                        manifest.batches.len(),
                        "VCE embedding batch count",
                    )?),
                ],
                "Publish VCE embedding manifest envelope",
            )
            .await
            .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        // Read-back record used to verify that the requested mutation was retained.
        // 用于验证请求变更已保留的回读记录。
        let stored = self
            .load_embedding_run_storage_row(&manifest.run_id)
            .await?
            .ok_or_else(|| {
                port_invalid(
                    VcePortOperation::EmbeddingManifest,
                    "published embedding manifest parent disappeared",
                )
            })?;
        if stored.layout_version != VCE_EMBEDDING_MANIFEST_LAYOUT_SHARDED
            || stored.manifest_complete != 1
            || stored.plan_fingerprint.as_deref() != Some(plan_fingerprint)
            || stored.state != embedding_run_state_text(manifest.state)
            || stored.revision != expected_revision
        {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "embedding manifest envelope publication was not retained",
            ));
        }
        Ok(())
    }

    /// Rebuild one complete manifest from bounded ordered child pages.
    /// 从有界有序子记录页重建一份完整清单。
    ///
    /// Reconstructs `run_id` using the captured parent `row`; returns validated data whose revision is checked by the caller.
    /// 根据捕获的父记录 `row` 重建 `run_id`；返回已校验数据，由调用者复核修订一致性。
    async fn load_complete_sharded_manifest(
        &self,
        run_id: &str,
        row: EmbeddingRunStorageRow,
    ) -> Result<VceEmbeddingRunManifest, VcePortError> {
        // Immutable run envelope retained independently of source-node payloads.
        // 独立于源码节点负载保存的不可变运行概要。
        let envelope = decode_envelope(&row.manifest_json)?;
        if envelope.run_id != run_id
            || envelope.namespace_id != self.binding.namespace_id
            || row.plan_fingerprint.as_deref() != Some(&envelope.plan_fingerprint)
            || u64::try_from(row.generation).ok() != Some(envelope.generation)
        {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "sharded embedding manifest parent and envelope disagree",
            ));
        }
        // Ordered comparison-base Blob identities reconstructed from child rows.
        // 根据子记录重建的有序对比基线 Blob 身份。
        let reused_blob_names = self
            .load_reused_blobs(run_id, envelope.reused_blob_count)
            .await?;
        // Ordered source snapshots reconstructed without allocating the declared total upfront.
        // 有序源码快照，不按声明总数一次性预分配内存。
        let nodes = self
            .load_embedding_nodes(run_id, envelope.node_count)
            .await?;
        // Ordered batch projections reconstructed with immutable identity validation.
        // 通过不可变身份校验重建的有序批次投影。
        let batches = self
            .load_embedding_batches(run_id, envelope.batch_count)
            .await?;
        // Complete core projection assembled from the envelope and durable child records.
        // 由概要和持久子记录组装的完整核心投影。
        let manifest = VceEmbeddingRunManifest {
            schema_version: envelope.schema_version,
            run_id: envelope.run_id,
            namespace_id: envelope.namespace_id,
            generation: envelope.generation,
            source_revision: envelope.source_revision,
            state: parse_embedding_run_state(&row.state)?,
            contract: envelope.contract,
            base_checkpoint_version: envelope.base_checkpoint_version,
            reused_blob_names,
            nodes,
            batches,
            created_at_unix_ms: sqlite_manifest_timestamp(
                row.created_at_unix_ms,
                "VCE manifest creation time",
            )?,
            updated_at_unix_ms: sqlite_manifest_timestamp(
                row.updated_at_unix_ms,
                "VCE manifest update time",
            )?,
        };
        // Complete core projection assembled from the envelope and durable child records.
        // 由概要和持久子记录组装的完整核心投影。
        let manifest = self.validate_loaded_manifest(run_id, manifest)?;
        // Digest of reconstructed immutable data compared with the published plan digest.
        // 与已发布计划摘要比较的重建不可变数据摘要。
        let actual_fingerprint = manifest
            .plan_fingerprint()
            .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
        if actual_fingerprint != envelope.plan_fingerprint {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "sharded embedding manifest plan fingerprint is invalid",
            ));
        }
        Ok(manifest)
    }

    /// Validate a reconstructed manifest belongs exactly to this namespace and lookup identity.
    /// 校验重建清单精确属于当前命名空间与查询身份。
    ///
    /// Checks that `manifest` matches `run_id` and this namespace; returns the validated manifest or ownership error.
    /// 检查 `manifest` 与 `run_id` 及当前命名空间匹配；返回已校验清单或归属错误。
    fn validate_loaded_manifest(
        &self,
        run_id: &str,
        manifest: VceEmbeddingRunManifest,
    ) -> Result<VceEmbeddingRunManifest, VcePortError> {
        manifest
            .validate()
            .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
        if manifest.namespace_id != self.binding.namespace_id || manifest.run_id != run_id {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "persisted embedding manifest ownership disagrees with its lookup",
            ));
        }
        Ok(manifest)
    }

    /// Load reused Blob identities in keyset pages and enforce contiguous order and count.
    /// 以键集分页读取复用 Blob 身份，并强制连续顺序与数量一致。
    ///
    /// Loads `run_id` in stable order and verifies `expected_count`; returns the exact reused Blob collection.
    /// 按稳定顺序读取 `run_id` 并校验 `expected_count`；返回精确复用 Blob 集合。
    async fn load_reused_blobs(
        &self,
        run_id: &str,
        expected_count: u64,
    ) -> Result<Vec<String>, VcePortError> {
        // Expected row count used for validation while initial allocation stays page-bounded.
        // 用于校验的预期行数，初始内存分配仍限制为一页。
        let capacity = usize::try_from(expected_count).map_err(|_| {
            port_invalid(
                VcePortOperation::EmbeddingManifest,
                "VCE reused Blob count exceeds platform limits",
            )
        })?;
        // Ordered reused Blob identities accumulated from validated pages.
        // 从已校验分页累积的有序复用 Blob 身份。
        let mut values = Vec::with_capacity(capacity.min(VCE_EMBEDDING_REUSED_PAGE_SIZE));
        // Last consumed stable key; minus one includes position zero on the first page.
        // 最近消费的稳定键；负一使第一页包含零号位置。
        let mut cursor = -1_i64;
        loop {
            // One bounded controller result page, scoped to the exact manifest identity.
            // 范围限定到精确清单身份的一页有界控制器结果。
            let rows: Vec<ReusedBlobRow> = self
                .data_plane
                .query_rows(
                    r#"SELECT reused_index, blob_name
                       FROM vce_embedding_run_reused_blobs
                       WHERE namespace_id = ? AND run_id = ? AND reused_index > ?
                       ORDER BY reused_index ASC LIMIT 64;"#,
                    vec![
                        sqlite_text(&self.binding.namespace_id),
                        sqlite_text(run_id),
                        ControllerSqliteValue::Int64(cursor),
                    ],
                )
                .await
                .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                // Next contiguous collection position required by the stored envelope.
                // 存储概要要求的下一个连续集合位置。
                let expected_index =
                    sqlite_manifest_usize(values.len(), "VCE reused Blob expected index")?;
                if row.reused_index != expected_index || values.len() >= capacity {
                    return Err(port_invalid(
                        VcePortOperation::EmbeddingManifest,
                        "VCE reused Blob rows are not contiguous",
                    ));
                }
                cursor = row.reused_index;
                values.push(row.blob_name);
            }
        }
        if values.len() != capacity {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "VCE reused Blob row count disagrees with the envelope",
            ));
        }
        Ok(values)
    }

    /// Load node fragments in keyset pages and decode only one complete node at a time.
    /// 以键集分页读取节点分片，并且每次仅解码一个完整节点。
    ///
    /// Loads `run_id` fragments and verifies `expected_count`; returns complete ordered source snapshots.
    /// 读取 `run_id` 的分片并校验 `expected_count`；返回完整有序源码快照。
    async fn load_embedding_nodes(
        &self,
        run_id: &str,
        expected_count: u64,
    ) -> Result<Vec<VceCodeNodeSnapshot>, VcePortError> {
        // Expected row count used for validation while initial allocation stays page-bounded.
        // 用于校验的预期行数，初始内存分配仍限制为一页。
        let capacity = usize::try_from(expected_count).map_err(|_| {
            port_invalid(
                VcePortOperation::EmbeddingManifest,
                "VCE embedding node count exceeds platform limits",
            )
        })?;
        // Ordered source snapshots reconstructed without allocating the declared total upfront.
        // 有序源码快照，不按声明总数一次性预分配内存。
        let mut nodes = Vec::with_capacity(capacity.min(VCE_EMBEDDING_NODE_CHUNK_PAGE_SIZE));
        // Last consumed node index for the next keyset page.
        // 下一键集页使用的最近消费节点索引。
        let mut cursor_node = -1_i64;
        // Last consumed fragment index inside the cursor node.
        // 游标节点内最近消费的分片索引。
        let mut cursor_chunk = -1_i64;
        // Node currently being assembled across controller page boundaries.
        // 当前跨控制器分页边界组装的节点。
        let mut current_node_index = None;
        // Blob identity required to match every fragment of the current node.
        // 要求当前节点的每个分片一致的 Blob 身份。
        let mut current_blob_name = String::new();
        // Single-node assembly buffer cleared at each node boundary.
        // 在每个节点边界清空的单节点组装缓冲区。
        let mut current_json = String::new();
        // Next contiguous fragment position within the current node.
        // 当前节点内下一个连续分片位置。
        let mut expected_chunk_index = 0_i64;
        loop {
            // One bounded controller result page, scoped to the exact manifest identity.
            // 范围限定到精确清单身份的一页有界控制器结果。
            let rows: Vec<EmbeddingNodeChunkRow> = self
                .data_plane
                .query_rows(
                    r#"SELECT node_index, chunk_index, blob_name, node_json_chunk
                       FROM vce_embedding_run_node_chunks
                       WHERE namespace_id = ? AND run_id = ?
                         AND (node_index > ? OR (node_index = ? AND chunk_index > ?))
                       ORDER BY node_index ASC, chunk_index ASC LIMIT 8;"#,
                    vec![
                        sqlite_text(&self.binding.namespace_id),
                        sqlite_text(run_id),
                        ControllerSqliteValue::Int64(cursor_node),
                        ControllerSqliteValue::Int64(cursor_node),
                        ControllerSqliteValue::Int64(cursor_chunk),
                    ],
                )
                .await
                .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                if current_node_index != Some(row.node_index) {
                    if let Some(previous_index) = current_node_index {
                        nodes.push(decode_node(
                            previous_index,
                            &current_blob_name,
                            &current_json,
                        )?);
                    }
                    // Next contiguous node position after decoding the preceding node.
                    // 解码前一个节点后的下一个连续节点位置。
                    let expected_node_index =
                        sqlite_manifest_usize(nodes.len(), "VCE embedding expected node index")?;
                    if row.node_index != expected_node_index
                        || row.chunk_index != 0
                        || nodes.len() >= capacity
                    {
                        return Err(port_invalid(
                            VcePortOperation::EmbeddingManifest,
                            "VCE embedding node chunks are not contiguous",
                        ));
                    }
                    current_node_index = Some(row.node_index);
                    current_blob_name = row.blob_name.clone();
                    current_json.clear();
                    expected_chunk_index = 0;
                }
                if row.chunk_index != expected_chunk_index
                    || row.blob_name != current_blob_name
                    || row.node_json_chunk.is_empty()
                {
                    return Err(port_invalid(
                        VcePortOperation::EmbeddingManifest,
                        "VCE embedding node chunk identity is inconsistent",
                    ));
                }
                current_json.push_str(&row.node_json_chunk);
                expected_chunk_index = expected_chunk_index.checked_add(1).ok_or_else(|| {
                    port_invalid(
                        VcePortOperation::EmbeddingManifest,
                        "VCE embedding node chunk index overflowed",
                    )
                })?;
                cursor_node = row.node_index;
                cursor_chunk = row.chunk_index;
            }
        }
        if let Some(previous_index) = current_node_index {
            nodes.push(decode_node(
                previous_index,
                &current_blob_name,
                &current_json,
            )?);
        }
        if nodes.len() != capacity {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "VCE embedding node row count disagrees with the envelope",
            ));
        }
        Ok(nodes)
    }

    /// Load batch rows in keyset pages and enforce order, identity, and count.
    /// 以键集分页读取批次记录，并强制顺序、身份及数量一致。
    ///
    /// Loads `run_id` batch pages and verifies `expected_count`; returns validated batch projections.
    /// 读取 `run_id` 批次分页并校验 `expected_count`；返回已校验批次投影。
    async fn load_embedding_batches(
        &self,
        run_id: &str,
        expected_count: u64,
    ) -> Result<Vec<VceEmbeddingBatchRecord>, VcePortError> {
        // Expected row count used for validation while initial allocation stays page-bounded.
        // 用于校验的预期行数，初始内存分配仍限制为一页。
        let capacity = usize::try_from(expected_count).map_err(|_| {
            port_invalid(
                VcePortOperation::EmbeddingManifest,
                "VCE embedding batch count exceeds platform limits",
            )
        })?;
        // Ordered batch projections reconstructed with immutable identity validation.
        // 通过不可变身份校验重建的有序批次投影。
        let mut batches = Vec::with_capacity(capacity.min(VCE_EMBEDDING_BATCH_PAGE_SIZE));
        // Last consumed stable key; minus one includes position zero on the first page.
        // 最近消费的稳定键；负一使第一页包含零号位置。
        let mut cursor = -1_i64;
        loop {
            // One bounded controller result page, scoped to the exact manifest identity.
            // 范围限定到精确清单身份的一页有界控制器结果。
            let rows: Vec<EmbeddingBatchStorageRow> = self
                .data_plane
                .query_rows(
                    r#"SELECT batch_index, batch_id, identity_fingerprint, batch_json,
                              updated_at_unix_ms
                       FROM vce_embedding_run_batches
                       WHERE namespace_id = ? AND run_id = ? AND batch_index > ?
                       ORDER BY batch_index ASC LIMIT 4;"#,
                    vec![
                        sqlite_text(&self.binding.namespace_id),
                        sqlite_text(run_id),
                        ControllerSqliteValue::Int64(cursor),
                    ],
                )
                .await
                .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                // Next contiguous collection position required by the stored envelope.
                // 存储概要要求的下一个连续集合位置。
                let expected_index =
                    sqlite_manifest_usize(batches.len(), "VCE embedding expected batch index")?;
                if row.batch_index != expected_index || batches.len() >= capacity {
                    return Err(port_invalid(
                        VcePortOperation::EmbeddingManifest,
                        "VCE embedding batch rows are not contiguous",
                    ));
                }
                // Validated batch projection decoded from the stored identity row.
                // 从存储身份记录中解码的已校验批次投影。
                let batch = decode_batch(&row)?;
                cursor = row.batch_index;
                batches.push(batch);
            }
        }
        if batches.len() != capacity {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "VCE embedding batch row count disagrees with the envelope",
            ));
        }
        Ok(batches)
    }

    /// Load one exact batch row by stable batch identity.
    /// 按稳定批次身份读取一条精确批次记录。
    ///
    /// Queries one `batch_id` inside `run_id`; returns an optional row without touching unrelated runs.
    /// 查询 `run_id` 内的一个 `batch_id`；返回可选记录，不触碰无关运行。
    async fn load_embedding_batch_row_by_id(
        &self,
        run_id: &str,
        batch_id: &str,
    ) -> Result<Option<EmbeddingBatchStorageRow>, VcePortError> {
        // One bounded controller result page, scoped to the exact manifest identity.
        // 范围限定到精确清单身份的一页有界控制器结果。
        let rows: Vec<EmbeddingBatchStorageRow> = self
            .data_plane
            .query_rows(
                r#"SELECT batch_index, batch_id, identity_fingerprint, batch_json,
                          updated_at_unix_ms
                   FROM vce_embedding_run_batches
                   WHERE namespace_id = ? AND run_id = ? AND batch_id = ? LIMIT 1;"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(run_id),
                    sqlite_text(batch_id),
                ],
            )
            .await
            .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        Ok(rows.into_iter().next())
    }

    /// Load and validate one optional result used by the granular batch state machine.
    /// 读取并校验细粒度批次状态机使用的一份可选结果。
    ///
    /// `run_id` and `batch_id` select the exact durable result. Returns absence or one validated
    /// identity-matching result without changing parent state.
    /// `run_id` 与 `batch_id` 选择精确持久结果。返回空值或一份已校验且身份匹配的结果，不修改父状态。
    async fn load_embedding_result_for_transition(
        &self,
        run_id: &str,
        batch_id: &str,
    ) -> Result<Option<VceEmbeddingBatchResult>, VcePortError> {
        // One exact result row is enough because the relational primary key enforces uniqueness.
        // 关系主键保证唯一性，因此只需读取一条精确结果记录。
        let rows: Vec<EmbeddingResultStorageRow> = self
            .data_plane
            .query_rows(
                "SELECT result_json FROM vce_embedding_batch_results WHERE namespace_id = ? AND run_id = ? AND batch_id = ? LIMIT 1;",
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(run_id),
                    sqlite_text(batch_id),
                ],
            )
            .await
            .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let result =
            serde_json::from_str::<VceEmbeddingBatchResult>(&row.result_json).map_err(|error| {
                port_invalid(VcePortOperation::EmbeddingManifest, error.to_string())
            })?;
        result
            .validate()
            .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
        if result.batch_id != batch_id {
            return Err(port_invalid(
                VcePortOperation::EmbeddingManifest,
                "persisted embedding batch result identity disagrees with its row",
            ));
        }
        Ok(Some(result))
    }
}

/// Decode and validate one bounded sharded envelope.
/// 解码并校验一份有界分片概要。
///
/// Parses bounded JSON `value`; returns a validated immutable envelope or explicit decoding error.
/// 解析有界 JSON `value`；返回已校验不可变概要或明确解码错误。
fn decode_envelope(value: &str) -> Result<StoredEmbeddingManifestEnvelope, VcePortError> {
    // Immutable run envelope retained independently of source-node payloads.
    // 独立于源码节点负载保存的不可变运行概要。
    let envelope = serde_json::from_str::<StoredEmbeddingManifestEnvelope>(value)
        .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error.to_string()))?;
    envelope
        .validate()
        .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
    Ok(envelope)
}

/// Decode one complete node and verify its repeated Blob identity.
/// 解码一个完整节点并校验其重复保存的 Blob 身份。
///
/// Validates `node_index` and `blob_name` against assembled JSON `value`; returns its source snapshot.
/// 根据组装 JSON `value` 校验 `node_index` 与 `blob_name`；返回其源码快照。
fn decode_node(
    node_index: i64,
    blob_name: &str,
    value: &str,
) -> Result<VceCodeNodeSnapshot, VcePortError> {
    if node_index < 0 || value.is_empty() {
        return Err(port_invalid(
            VcePortOperation::EmbeddingManifest,
            "VCE embedding node payload is invalid",
        ));
    }
    // Source snapshot decoded after all of its ordered fragments are assembled.
    // 全部有序分片组装后解码的源码快照。
    let node = serde_json::from_str::<VceCodeNodeSnapshot>(value)
        .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error.to_string()))?;
    if node.blob_name != blob_name {
        return Err(port_invalid(
            VcePortOperation::EmbeddingManifest,
            "VCE embedding node Blob identity disagrees with its chunks",
        ));
    }
    Ok(node)
}

/// Decode one batch row and verify both stable identity projections.
/// 解码一条批次记录并校验两个稳定身份投影。
///
/// Validates `row` against its serialized request identity; returns the decoded batch projection.
/// 根据序列化请求身份校验 `row`；返回解码后的批次投影。
fn decode_batch(row: &EmbeddingBatchStorageRow) -> Result<VceEmbeddingBatchRecord, VcePortError> {
    if row.updated_at_unix_ms < 0 {
        return Err(port_invalid(
            VcePortOperation::EmbeddingManifest,
            "VCE embedding batch update timestamp is negative",
        ));
    }
    // Validated batch projection decoded from the stored identity row.
    // 从存储身份记录中解码的已校验批次投影。
    let batch = serde_json::from_str::<VceEmbeddingBatchRecord>(&row.batch_json)
        .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error.to_string()))?;
    batch
        .validate()
        .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
    // Immutable request digest checked against the stored batch identity.
    // 与存储批次身份比较的不可变请求摘要。
    let fingerprint = batch
        .identity_fingerprint()
        .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
    if batch.batch_id != row.batch_id || fingerprint != row.identity_fingerprint {
        return Err(port_invalid(
            VcePortOperation::EmbeddingManifest,
            "VCE embedding batch identity disagrees with its row",
        ));
    }
    Ok(batch)
}

/// Split one UTF-8 JSON value without cutting a multi-byte scalar.
/// 在不截断多字节字符的前提下拆分一个 UTF-8 JSON 值。
///
/// Splits JSON `value` into ordered fragments no larger than the chunk limit; concatenation restores the input.
/// 将 JSON `value` 拆分为不超过分片上限的有序片段；拼接后恢复原始输入。
fn split_utf8_chunks(value: &str) -> Vec<String> {
    // Ordered UTF-8 fragments retained for exact JSON round-trip reconstruction.
    // 为精确 JSON 往返重建保留的有序 UTF-8 分片。
    let mut chunks = Vec::new();
    // Inclusive byte offset of the next UTF-8 fragment.
    // 下一个 UTF-8 分片的起始字节偏移。
    let mut start = 0;
    while start < value.len() {
        // Exclusive byte boundary adjusted to avoid cutting a UTF-8 scalar.
        // 为避免截断 UTF-8 字符而调整的结束字节边界。
        let mut end = start
            .saturating_add(VCE_EMBEDDING_NODE_CHUNK_BYTES)
            .min(value.len());
        while end > start && !value.is_char_boundary(end) {
            end -= 1;
        }
        debug_assert!(end > start);
        chunks.push(value[start..end].to_owned());
        start = end;
    }
    chunks
}

/// Require a parent row to expose one complete current-layout plan.
/// 要求父记录公开一份完整的当前布局计划。
///
/// Checks `row` layout, publication marker, fingerprint, and revision; returns unit or a rejection.
/// 检查 `row` 的布局、发布标记、指纹和修订；返回成功空值或拒绝错误。
fn require_complete_sharded_parent(row: &EmbeddingRunStorageRow) -> Result<(), VcePortError> {
    if row.layout_version != VCE_EMBEDDING_MANIFEST_LAYOUT_SHARDED
        || row.manifest_complete != 1
        || row
            .plan_fingerprint
            .as_ref()
            .is_none_or(|value| value.len() != 64)
        || row.revision <= 0
    {
        return Err(port_rejected(
            VcePortOperation::EmbeddingManifest,
            "embedding manifest plan is not completely published",
        ));
    }
    Ok(())
}

/// Return the exact next positive revision or reject exhausted persisted state.
/// 返回精确的下一个正修订号；持久化状态耗尽时予以拒绝。
///
/// `current` must be positive and unexhausted; returns its checked successor.
/// `current` 必须为正数且未耗尽；返回经过校验的后继值。
fn next_embedding_manifest_revision(current: i64) -> Result<i64, VcePortError> {
    if current <= 0 {
        return Err(port_invalid(
            VcePortOperation::EmbeddingManifest,
            "persisted embedding manifest revision is invalid",
        ));
    }
    current.checked_add(1).ok_or_else(|| {
        port_rejected(
            VcePortOperation::EmbeddingManifest,
            "embedding manifest revision is exhausted",
        )
    })
}

/// Validate a run identity before using it as an exact storage key.
/// 在将运行身份用作精确存储键前进行校验。
///
/// Rejects blank or oversized `run_id`; returns unit for a valid exact storage key.
/// 拒绝空白或超长 `run_id`；对有效精确存储键返回成功空值。
fn validate_run_id(run_id: &str) -> Result<(), VcePortError> {
    if run_id.trim().is_empty() || run_id.len() > 512 {
        return Err(port_rejected(
            VcePortOperation::EmbeddingManifest,
            "embedding manifest run identity is empty or exceeds its bound",
        ));
    }
    Ok(())
}

/// Convert one manifest usize into a checked SQLite integer.
/// 将一个清单 usize 转换为经过校验的 SQLite 整数。
///
/// Converts `value` to SQLite's signed integer range; `label` identifies conversion failures.
/// 将 `value` 转换为 SQLite 有符号整数范围；`label` 标识转换失败的字段。
fn sqlite_manifest_usize(value: usize, label: &str) -> Result<i64, VcePortError> {
    i64::try_from(value).map_err(|_| {
        port_rejected(
            VcePortOperation::EmbeddingManifest,
            format!("{label} exceeds SQLite integer range"),
        )
    })
}

/// Convert one manifest u64 into a checked SQLite integer.
/// 将一个清单 u64 转换为经过校验的 SQLite 整数。
///
/// Converts `value` to SQLite's signed integer range; `label` identifies conversion failures.
/// 将 `value` 转换为 SQLite 有符号整数范围；`label` 标识转换失败的字段。
fn sqlite_manifest_u64(value: u64, label: &str) -> Result<i64, VcePortError> {
    i64::try_from(value).map_err(|_| {
        port_rejected(
            VcePortOperation::EmbeddingManifest,
            format!("{label} exceeds SQLite integer range"),
        )
    })
}

/// Convert one nonnegative SQLite timestamp into its core representation.
/// 将一个非负 SQLite 时间戳转换为核心表示。
///
/// Converts stored `value` to a nonnegative timestamp; `label` identifies invalid persisted fields.
/// 将存储 `value` 转为非负时间戳；`label` 标识无效持久字段。
fn sqlite_manifest_timestamp(value: i64, label: &str) -> Result<u64, VcePortError> {
    u64::try_from(value).map_err(|_| {
        port_invalid(
            VcePortOperation::EmbeddingManifest,
            format!("{label} is negative"),
        )
    })
}

/// Return the stable relational text for one run lifecycle.
/// 返回一个运行生命周期对应的稳定关系文本。
///
/// Maps typed `state` to its stable SQLite value without allocation.
/// 将类型化 `state` 映射为稳定 SQLite 值，不分配内存。
fn embedding_run_state_text(state: VceEmbeddingRunState) -> &'static str {
    match state {
        VceEmbeddingRunState::Planned => "planned",
        VceEmbeddingRunState::Running => "running",
        VceEmbeddingRunState::CandidateReady => "candidate_ready",
        VceEmbeddingRunState::Succeeded => "succeeded",
        VceEmbeddingRunState::Failed => "failed",
        VceEmbeddingRunState::Cancelled => "cancelled",
    }
}

/// Parse one persisted run lifecycle without accepting unknown text.
/// 解析一个持久化运行生命周期，不接受未知文本。
///
/// Parses stored `value` into a known state; unknown values return an explicit error.
/// 将存储 `value` 解析为已知状态；未知值返回明确错误。
fn parse_embedding_run_state(value: &str) -> Result<VceEmbeddingRunState, VcePortError> {
    match value {
        "planned" => Ok(VceEmbeddingRunState::Planned),
        "running" => Ok(VceEmbeddingRunState::Running),
        "candidate_ready" => Ok(VceEmbeddingRunState::CandidateReady),
        "succeeded" => Ok(VceEmbeddingRunState::Succeeded),
        "failed" => Ok(VceEmbeddingRunState::Failed),
        "cancelled" => Ok(VceEmbeddingRunState::Cancelled),
        _ => Err(port_invalid(
            VcePortOperation::EmbeddingManifest,
            "persisted embedding manifest state is invalid",
        )),
    }
}

/// Validate one persisted run lifecycle transition without resurrecting a published result.
/// 校验一条持久运行生命周期转换，不复活已发布结果。
///
/// `previous` is authoritative and `next` is proposed. Returns unit only for an explicit
/// planning, recovery, publication, failure, or cancellation edge used by the core pipeline.
/// `previous` 是权威状态，`next` 是拟议状态。仅对核心流水线使用的明确规划、恢复、发布、失败或取消边返回空值。
fn validate_embedding_run_state_transition(
    previous: VceEmbeddingRunState,
    next: VceEmbeddingRunState,
) -> Result<(), VcePortError> {
    let permitted = previous == next
        || matches!(
            (previous, next),
            (VceEmbeddingRunState::Planned, VceEmbeddingRunState::Running)
                | (
                    VceEmbeddingRunState::Running,
                    VceEmbeddingRunState::CandidateReady
                )
                | (VceEmbeddingRunState::Running, VceEmbeddingRunState::Failed)
                | (
                    VceEmbeddingRunState::Running,
                    VceEmbeddingRunState::Cancelled
                )
                | (
                    VceEmbeddingRunState::CandidateReady,
                    VceEmbeddingRunState::Running
                )
                | (
                    VceEmbeddingRunState::CandidateReady,
                    VceEmbeddingRunState::Succeeded
                )
                | (VceEmbeddingRunState::Failed, VceEmbeddingRunState::Running)
                | (
                    VceEmbeddingRunState::Cancelled,
                    VceEmbeddingRunState::Running
                )
        );
    if permitted {
        Ok(())
    } else {
        Err(port_rejected(
            VcePortOperation::EmbeddingManifest,
            "embedding manifest run lifecycle transition is invalid",
        ))
    }
}

/// Validate that a publication-capable run state agrees with every persisted provider batch.
/// 校验可发布运行状态与全部持久化供应商批次一致。
///
/// `state` is the proposed parent state and `batches` are its complete ordered children.
/// Returns unit when mandatory content is complete and no request remains in flight.
/// `state` 是拟议父状态，`batches` 是其完整有序子记录。必需内容完成且无请求仍在执行时返回空值。
fn validate_embedding_run_batch_consistency(
    state: VceEmbeddingRunState,
    batches: &[VceEmbeddingBatchRecord],
) -> Result<(), VcePortError> {
    let has_unfinished = batches.iter().any(|batch| {
        matches!(
            batch.state,
            VceEmbeddingBatchState::Pending | VceEmbeddingBatchState::Running
        )
    });
    let has_incomplete_content = batches.iter().any(|batch| {
        batch.purpose == vulcan_context_engine::VceAiPurpose::CodeDocumentEmbedding.as_str()
            && batch.state != VceEmbeddingBatchState::Succeeded
    });
    if matches!(
        state,
        VceEmbeddingRunState::CandidateReady | VceEmbeddingRunState::Succeeded
    ) && (has_unfinished || has_incomplete_content)
    {
        return Err(port_rejected(
            VcePortOperation::EmbeddingManifest,
            "embedding manifest publication state disagrees with its batches",
        ));
    }
    Ok(())
}

/// Validate one granular batch successor while retaining result-loss recovery semantics.
/// 校验一条细粒度批次后继，同时保留结果回执丢失恢复语义。
///
/// `previous` is the acknowledged row and `next` is proposed. Returns unit only when attempts
/// advance exactly on admission and a success marker may reset solely for a missing durable result.
/// `previous` 是已确认记录，`next` 是拟议记录。仅当尝试次数在准入时精确推进，且成功标记只为缺失持久结果而复位时返回空值。
fn validate_embedding_batch_transition(
    previous: &VceEmbeddingBatchRecord,
    next: &VceEmbeddingBatchRecord,
) -> Result<(), VcePortError> {
    if previous == next {
        return Ok(());
    }
    let same_attempt = next.attempt == previous.attempt;
    let admitted_attempt = previous
        .attempt
        .checked_add(1)
        .is_some_and(|attempt| next.attempt == attempt);
    let permitted = match (previous.state, next.state) {
        (VceEmbeddingBatchState::Pending, VceEmbeddingBatchState::Running)
        | (VceEmbeddingBatchState::Running, VceEmbeddingBatchState::Running)
        | (VceEmbeddingBatchState::Failed, VceEmbeddingBatchState::Running) => admitted_attempt,
        (VceEmbeddingBatchState::Pending, VceEmbeddingBatchState::Failed)
        | (VceEmbeddingBatchState::Running, VceEmbeddingBatchState::Succeeded)
        | (VceEmbeddingBatchState::Running, VceEmbeddingBatchState::Failed)
        | (VceEmbeddingBatchState::Failed, VceEmbeddingBatchState::Failed)
        | (VceEmbeddingBatchState::Succeeded, VceEmbeddingBatchState::Pending) => same_attempt,
        _ => false,
    };
    if permitted {
        Ok(())
    } else {
        Err(port_rejected(
            VcePortOperation::EmbeddingManifest,
            "embedding manifest batch lifecycle transition is invalid",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one bounded content batch used by granular lifecycle tests.
    /// 构造一条供细粒度生命周期测试使用的有界内容批次。
    ///
    /// Takes no parameters and returns a fresh pending record without storage I/O.
    /// 无参数，返回一条不执行存储读写的新待处理记录。
    fn pending_content_batch() -> VceEmbeddingBatchRecord {
        VceEmbeddingBatchRecord {
            batch_id: String::from("run:content:0"),
            purpose: vulcan_context_engine::VceAiPurpose::CodeDocumentEmbedding
                .as_str()
                .to_owned(),
            input_ids: vec![String::from("input-0")],
            total_chars: 7,
            state: VceEmbeddingBatchState::Pending,
            attempt: 0,
            error: None,
        }
    }

    /// Verify UTF-8 node fragments stay bounded and reconstruct multibyte JSON exactly.
    /// 验证 UTF-8 节点分片保持有界并精确重建多字节 JSON。
    #[test]
    ///
    /// Uses a multibyte fixture and asserts bounded chunks and exact reconstruction; takes no parameters.
    /// 使用多字节夹具断言有界分片及精确重建；无参数。
    fn node_chunks_preserve_multibyte_json() {
        // Multibyte JSON fixture spanning more than one storage fragment.
        // 跨越多个存储分片的多字节 JSON 夹具。
        let value = format!("{{\"source\":\"{}终\"}}", "中".repeat(20_000));
        // Ordered UTF-8 fragments retained for exact JSON round-trip reconstruction.
        // 为精确 JSON 往返重建保留的有序 UTF-8 分片。
        let chunks = split_utf8_chunks(&value);
        assert!(chunks.len() > 1);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.len() <= VCE_EMBEDDING_NODE_CHUNK_BYTES)
        );
        assert_eq!(chunks.concat(), value);
    }

    /// Verify run states cannot regress after publication and retries use explicit active edges.
    /// 验证运行状态在发布后不能倒退，重试必须使用明确活动边。
    #[test]
    fn embedding_run_state_transition_is_monotonic() {
        validate_embedding_run_state_transition(
            VceEmbeddingRunState::Planned,
            VceEmbeddingRunState::Running,
        )
        .expect("planned run may start");
        validate_embedding_run_state_transition(
            VceEmbeddingRunState::Failed,
            VceEmbeddingRunState::Running,
        )
        .expect("failed run may explicitly resume");
        validate_embedding_run_state_transition(
            VceEmbeddingRunState::CandidateReady,
            VceEmbeddingRunState::Succeeded,
        )
        .expect("candidate may publish");
        assert!(
            validate_embedding_run_state_transition(
                VceEmbeddingRunState::Succeeded,
                VceEmbeddingRunState::Running,
            )
            .is_err()
        );
        assert!(
            validate_embedding_run_state_transition(
                VceEmbeddingRunState::Running,
                VceEmbeddingRunState::Planned,
            )
            .is_err()
        );
    }

    /// Verify batch attempts advance only on admission while missing results may reset success.
    /// 验证批次尝试次数只在准入时推进，同时允许缺失结果复位成功标记。
    #[test]
    fn embedding_batch_transition_preserves_attempts() {
        let pending = pending_content_batch();
        let mut running = pending.clone();
        running.mark_running().expect("admit first attempt");
        validate_embedding_batch_transition(&pending, &running).expect("first admission");

        let mut succeeded = running.clone();
        succeeded.mark_succeeded();
        validate_embedding_batch_transition(&running, &succeeded).expect("record result");

        let mut reset = succeeded.clone();
        reset.state = VceEmbeddingBatchState::Pending;
        validate_embedding_batch_transition(&succeeded, &reset).expect("reset missing result");

        let mut skipped_attempt = pending.clone();
        skipped_attempt.state = VceEmbeddingBatchState::Running;
        skipped_attempt.attempt = 2;
        assert!(validate_embedding_batch_transition(&pending, &skipped_attempt).is_err());

        let mut regressed = running.clone();
        regressed.attempt = 0;
        regressed.state = VceEmbeddingBatchState::Failed;
        regressed.error = Some(String::from("failed"));
        assert!(validate_embedding_batch_transition(&running, &regressed).is_err());
    }
}
