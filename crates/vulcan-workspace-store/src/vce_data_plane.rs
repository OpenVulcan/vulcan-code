//! In-process VCE code-semantic persistence and search.
//! 进程内 VCE 代码语义持久化与检索。

use crate::data_plane::HostRuntimeHandle;
use crate::local_database_runtime::{
    LanceDbDirectoryBinding, LocalDatabaseRuntime, SqliteFileBinding,
};
use crate::local_sql;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use vulcan_context_engine::{
    AuxiliaryIndexStatus, Checkpoint, Chunk, ChunkKind, CommitReceipt, DenseSearchRequest,
    ExactSearchRequest, FileStatus, FindMissingRequest, FindMissingResponse, HitKind,
    IndexAbortReceipt, IndexAbortRequest, IndexCommit, IndexStageBatch, IndexStageReceipt,
    IndexedBlob, IndexedBlobStatus, PathDocument, PathSearchRequest, RefreshMode, RefreshReason,
    SearchHit, SearchSource, SourceRange, StructureMode, SymbolKind, SymbolOccurrence,
    VceBlobDetail, VceChannelState, VceChunkSummary, VceCompactionOptions, VceEmbeddingBatchRecord,
    VceEmbeddingBatchResult, VceEmbeddingRemoteBatchRecord, VceEmbeddingRunManifest,
    VceEmbeddingRunState, VceFileSummary, VceFuture, VceIndexBinding, VceIndexCounts,
    VceIndexIntegrity, VceIndexJobSummary, VceIndexManagementSnapshot, VceIndexPort,
    VceIndexStoreStatus, VceIntegrityIssue, VceIntegrityIssueKind, VceMaintenanceAction,
    VceMaintenanceMode, VceMaintenancePlan, VceManagementQuery, VceManagementQueryRequest,
    VceManagementQueryResponse, VceManagementQueryResult, VceManagementRequest, VceOperationKind,
    VceOperationPhase, VceOperationSummary, VcePage, VcePageRequest, VcePathSearchResponse,
    VcePhysicalCompactionMetrics, VcePhysicalCompactionOperation, VcePhysicalCompactionState,
    VcePhysicalFragmentSummary, VcePhysicalTablePlan, VcePhysicalTableRole,
    VcePhysicalTableStatistics, VcePortError, VcePortOperation, VceSearchResponse,
    VceStateSnapshot, content_hash,
};
use vulcan_lancedb_engine::engine::LanceDbEngine;
use vulcan_lancedb_engine::types::{
    LanceDbColumnDef, LanceDbColumnType, LanceDbCompactTableInput, LanceDbCompactionOptions,
    LanceDbCreateTableInput, LanceDbDeleteInput, LanceDbInputFormat, LanceDbOutputFormat,
    LanceDbSearchInput, LanceDbTableStatisticsInput, LanceDbTableStatisticsResult,
    LanceDbUpsertInput,
};
use vulcan_sqlite_engine::runtime::{
    SqliteHardeningOptions, SqliteOpenOptions, SqlitePragmaOptions,
};
use vulcan_sqlite_engine::sql_exec::SqliteValue;

pub use crate::vce_storage_catalog::VceDataPlaneFactory;
pub use crate::vce_storage_paths::VceProjectWriterLease;

#[path = "vce_job_store.rs"]
mod jobs;
pub use jobs::{
    VCE_JOB_RECEIPT_SCHEMA_VERSION, VceCompactionJobReceipt, VceCompactionMetrics,
    VceCompactionTableReceipt, VceCompactionTableState, VceCompletedJobUpdate, VceJobKind,
    VceJobReplacement, VceJobState, VceJobStoreError, VceProjectJobStore, VceStoredJob,
};

#[path = "vce_embedding_provenance.rs"]
mod embedding_provenance;
pub use embedding_provenance::VceEmbeddingProvenance;

#[path = "vce_read_generations.rs"]
mod read_generations;

#[path = "vce_candidate_allocation.rs"]
mod candidate_allocation;

#[path = "vce_initial_allocation.rs"]
mod initial_allocation;

#[path = "vce_embedding_manifest_store.rs"]
mod embedding_manifest_store;

#[path = "vce_shared_artifacts.rs"]
mod shared_artifacts;

#[path = "vce_shared_result_store.rs"]
mod shared_result_store;

#[path = "vce_artifact_retention.rs"]
mod artifact_retention;

#[cfg(test)]
#[path = "vce_shared_storage_tests.rs"]
mod shared_storage_tests;

#[path = "vce_shared_manifest_store.rs"]
mod shared_manifest_store;

/// Stable logical space reserved for code-semantic state.
/// 为代码语义状态保留的稳定逻辑空间。
pub const VCE_SPACE_ID: &str = "vce";

/// Stable SQLite binding tag reserved for VCE relational state.
/// 为 VCE 关系状态保留的稳定 SQLite 绑定标签。
pub const VCE_SQLITE_BINDING_TAG: &str = "vce-state";

/// Stable LanceDB binding tag reserved for VCE vectors.
/// 为 VCE 向量保留的稳定 LanceDB 绑定标签。
pub const VCE_LANCEDB_BINDING_TAG: &str = "vce-code";

/// Target physical rows per compacted VCE fragment.
/// 每个已整理 VCE 片段的目标物理行数。
const VCE_COMPACTION_TARGET_ROWS_PER_FRAGMENT: u64 = 100_000;

/// Maximum rows written into one Lance row group during compaction.
/// 整理期间写入单个 Lance 行组的最大行数。
const VCE_COMPACTION_MAX_ROWS_PER_GROUP: u64 = 1_024;

/// Maximum bytes written into one compacted physical file.
/// 单个已整理物理文件允许写入的最大字节数。
const VCE_COMPACTION_MAX_BYTES_PER_FILE: u64 = 256 * 1024 * 1024;

/// Bounded storage-engine worker count for one VCE compaction operation.
/// 单次 VCE 整理操作使用的有界存储引擎工作线程数。
const VCE_COMPACTION_THREADS: u64 = 2;

// The frozen worker count must satisfy the maintenance contract, and the contract must stay within
// what the storage engine accepts; this crate is the only one that sees both bounds.
// 冻结的工作线程数必须满足维护契约，契约也必须保持在存储引擎接受的范围内；只有本 crate 同时
// 看得到这两个上限。
const _: () = assert!(
    VCE_COMPACTION_THREADS <= vulcan_context_engine::VCE_MAX_COMPACTION_THREADS
        && vulcan_context_engine::VCE_MAX_COMPACTION_THREADS
            <= vulcan_lancedb_engine::engine::MAX_COMPACTION_THREADS as u64
);

/// Input scan batch size used by VCE physical compaction.
/// VCE 物理整理使用的输入扫描批次行数。
const VCE_COMPACTION_SCAN_BATCH_ROWS: u64 = 8_192;

/// Maximum source fragments rewritten by one resumable storage operation.
/// 单次可恢复存储操作重写的最大源片段数。
const VCE_COMPACTION_MAX_SOURCE_FRAGMENTS: u64 = 512;

/// Current relational schema version for the VCE host data plane.
/// VCE 宿主数据面的当前关系结构版本。
///
/// This is the only marker a v1 database can carry. There is deliberately no constant for any earlier
/// generation: pre-v1 markers are not recognized, not migrated and not repaired, so no code path needs to
/// name them. See the initialization entry for where an unrecognized marker is rejected.
/// 这是 v1 数据库唯一可能携带的标记。此处刻意不为任何更早的代次保留常量：v1 之前的标记既不被识别，也不被迁移或
/// 修补，因此没有任何代码路径需要命名它们。未被识别的标记在哪里被拒绝，见初始化入口。
pub const VCE_DATA_PLANE_SCHEMA_VERSION: i64 = 9;

/// Maximum result count accepted by one physical vector search.
/// 一次物理向量检索允许的最大结果数量。
const VCE_VECTOR_SEARCH_LIMIT: u32 = 10_000;

/// Busy timeout used by the VCE SQLite resource as a final cross-process contention buffer.
/// VCE SQLite 资源使用的忙等待超时，作为跨进程竞争的最后缓冲。
const VCE_SQLITE_BUSY_TIMEOUT_MS: u64 = 15_000;

/// Process-local sequence keeping repeated host bindings distinct.
/// 保证重复宿主绑定互不冲突的进程内序列。
static VCE_BINDING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Long-lived host bindings for VCE SQLite and LanceDB storage.
/// VCE SQLite 与 LanceDB 存储使用的长生命周期宿主绑定。
#[derive(Clone)]
pub struct VceDataPlaneHandle {
    /// Exact stable owner of this project's isolated physical storage.
    /// 本项目隔离物理存储的精确稳定所有者。
    project_owner: String,
    /// Counts every retained clone so cache eviction cannot close an active query or writer.
    /// 统计全部保留克隆，使缓存淘汰不能关闭活动查询或写入者。
    pub(crate) lease: Arc<()>,
    /// Final read-reference releases awaiting an owned host boundary, never detached tasks.
    /// 等待所属宿主边界处理的最终读取引用释放，绝不使用脱离管理的任务。
    read_releases: Arc<std::sync::Mutex<Vec<read_generations::VceReleasedReadLease>>>,
    /// Root directory dedicated to code-semantic data.
    /// 代码语义数据专用根目录。
    pub data_root: PathBuf,
    /// SQLite path containing authoritative relational state.
    /// 保存权威关系状态的 SQLite 路径。
    pub sqlite_path: PathBuf,
    /// LanceDB root containing code and path vectors.
    /// 保存代码与路径向量的 LanceDB 根目录。
    pub lancedb_path: PathBuf,
    /// Bootstrap-scoped SQLite binding identity.
    /// 单次引导作用域内的 SQLite 绑定身份。
    pub sqlite_binding_id: String,
    /// Exact in-process SQLite binding for this project's authoritative state.
    /// 本项目权威状态的精确进程内 SQLite 绑定。
    pub sqlite_binding: SqliteFileBinding,
    /// Host runtime owning the in-process engine that serves this project.
    /// 拥有服务本项目的进程内引擎的宿主运行时。
    pub local_runtime: Arc<LocalDatabaseRuntime>,
    /// Bootstrap-scoped LanceDB binding identity.
    /// 单次引导作用域内的 LanceDB 绑定身份。
    pub lancedb_binding_id: String,
    /// Completed in-process physical compaction steps, keyed by operation identity.
    /// 已完成的进程内物理整理步骤，按操作身份索引。
    ///
    /// The in-process step is synchronous, so its result is knowable only at the moment of submission.
    /// Without this record a later query could only invent a terminal state; with it, a query either reports
    /// the step that really ran or reports that the step is not knowable. Held in an `Arc` so every clone of
    /// this handle — and therefore every index opened over it — observes the same submissions.
    /// 进程内步骤是同步的，故其结果只在提交那一刻可知。没有本记录，后续查询只能编造终态；有了它，查询要么报告
    /// 确实运行过的步骤，要么报告该步骤不可知。以 `Arc` 持有，使本句柄的每个克隆——以及在其上打开的每个索引
    /// ——都观察到同一批提交。
    compaction_operations: Arc<std::sync::Mutex<HashMap<String, VcePhysicalCompactionOperation>>>,
}

/// One persisted project-binding row used for compatibility validation.
/// 一条用于兼容性校验的持久项目绑定行。
#[derive(Clone, Debug, Deserialize)]
struct ProjectBindingRow {
    /// Stored project identity.
    /// 已存项目身份。
    project_id: String,
    /// Canonical absolute project root owned by the host.
    /// 由宿主持有的规范绝对项目根目录。
    root_path: String,
    /// SHA-256 identity of the canonical project root.
    /// 规范项目根目录的 SHA-256 身份。
    root_hash: String,
    /// Stored namespace identity.
    /// 已存命名空间身份。
    namespace_id: String,
    /// Stored checkpoint-chain identity.
    /// 已存检查点链身份。
    checkpoint_chain_id: String,
    /// Stored VCE index schema version.
    /// 已存 VCE 索引结构版本。
    schema_version: i64,
    /// Stored logical vector-space identity.
    /// 已存逻辑向量空间身份。
    vector_space_id: String,
    /// Stored physical Embedding fingerprint.
    /// 已存物理向量指纹。
    physical_fingerprint: String,
    /// Host-owned preprocessing contract version.
    /// 宿主持有的预处理契约版本。
    preprocessing_version: String,
    /// Host-owned chunker contract version.
    /// 宿主持有的切块器契约版本。
    chunker_version: String,
    /// Stored vector dimension.
    /// 已存向量维度。
    dimension: i64,
    /// Stored content-vector table name.
    /// 已存内容向量表名。
    content_table: String,
    /// Stored path-vector table name.
    /// 已存路径向量表名。
    path_table: String,
    /// Persisted project configuration revision.
    /// 持久化项目配置修订号。
    config_revision: i64,
    /// Whether this namespace is the project's active binding.
    /// 此命名空间是否为项目当前活动绑定。
    active: i64,
}

/// Minimal persisted binding gate used before mutation or retrieval.
/// 在变更或检索前使用的最小持久化绑定门禁。
#[derive(Clone, Debug, Deserialize)]
struct ActiveBindingRow {
    /// Whether this namespace is active for its project.
    /// 此命名空间是否为项目活动绑定。
    active: i64,
    /// Current index lifecycle state.
    /// 当前索引生命周期状态。
    lifecycle_state: String,
    /// Persisted project configuration revision.
    /// 持久化项目配置修订号。
    config_revision: i64,
    /// Persisted physical Embedding fingerprint.
    /// 持久化物理 Embedding 指纹。
    physical_fingerprint: String,
    /// Persisted vector dimension.
    /// 持久化向量维度。
    dimension: i64,
    /// Persisted content-vector table.
    /// 持久化内容向量表。
    content_table: String,
    /// Persisted path-vector table.
    /// 持久化路径向量表。
    path_table: String,
}

impl ProjectBindingRow {
    /// Decode the stored origin and verify derived table identities without repairing it.
    /// 解码存储的原始来源并校验派生表身份，不修复或改写它。
    ///
    /// Takes the persisted row as input and returns its strictly validated original config.
    /// 以持久化行为输入，返回经过严格校验的原始配置。
    fn original_config(&self) -> Result<VceProjectIndexConfig, String> {
        self.config_for_active_state(true)
    }

    /// Decode one exact binding with its caller-declared lifecycle and validate derived vector tables.
    /// 按调用方明确声明的生命周期解码精确绑定，并校验派生向量表。
    /// `active` is true for project discovery, false for an already authorized candidate. Returns validated origin.
    /// `active` 对项目发现为真，对已经授权的候选为假。返回已校验来源。
    fn config_for_active_state(&self, active: bool) -> Result<VceProjectIndexConfig, String> {
        // dimension rejects negative, zero and oversized persisted values.
        // dimension 拒绝持久化的负数、零及越界值。
        let dimension = u32::try_from(self.dimension)
            .map_err(|error| format!("Invalid stored VCE dimension: {error}"))?;
        // schema_version preserves the exact stored format instead of selecting a fallback.
        // schema_version 保留精确存储格式，不选择兜底版本。
        let schema_version = u32::try_from(self.schema_version)
            .map_err(|error| format!("Invalid stored VCE schema: {error}"))?;
        // config_revision remains the original database write-guard revision.
        // config_revision 保留原始数据库写闸门修订。
        let config_revision = u64::try_from(self.config_revision)
            .map_err(|error| format!("Invalid stored VCE revision: {error}"))?;
        // space carries immutable physical provenance rather than a newly selected model identity.
        // space 携带不可变物理来源，而不是新选择的模型身份。
        let space = vulcan_context_engine::EmbeddingSpace {
            vector_space_id: self.vector_space_id.clone(),
            physical_fingerprint: Some(self.physical_fingerprint.clone()),
            dimension: usize::try_from(dimension)
                .map_err(|error| format!("Invalid host VCE dimension: {error}"))?,
            preprocessing_version: self.preprocessing_version.clone(),
            chunker_version: self.chunker_version.clone(),
        };
        // config reconstructs only fields present in this verified database row.
        // config 仅重建此已验证数据库行中实际存在的字段。
        let config = VceProjectIndexConfig {
            project_id: self.project_id.clone(),
            root_path: self.root_path.clone(),
            root_hash: self.root_hash.clone(),
            binding: VceIndexBinding::new(
                self.namespace_id.clone(),
                self.checkpoint_chain_id.clone(),
                schema_version,
                space,
            )
            .map_err(|error| error.to_string())?,
            vector_space_id: self.vector_space_id.clone(),
            physical_fingerprint: self.physical_fingerprint.clone(),
            preprocessing_version: self.preprocessing_version.clone(),
            chunker_version: self.chunker_version.clone(),
            dimension,
            config_revision,
        };
        config.validate()?;
        if self.active != i64::from(active)
            || !self.vector_compatibility_matches(
                &config,
                &vector_table_name(
                    "content",
                    &config.vector_space_id,
                    &config.physical_fingerprint,
                    &config.preprocessing_version,
                    &config.chunker_version,
                    dimension,
                ),
                &vector_table_name(
                    "path",
                    &config.vector_space_id,
                    &config.physical_fingerprint,
                    &config.preprocessing_version,
                    &config.chunker_version,
                    dimension,
                ),
            )
        {
            return Err(String::from(
                "VCE stored active binding or vector table identity is invalid",
            ));
        }
        Ok(config)
    }

    /// Validates immutable project and namespace identities.
    /// 校验不可变的项目与命名空间身份。
    fn validate_identity(&self, config: &VceProjectIndexConfig) -> Result<(), String> {
        let expected_schema = i64::from(config.binding.schema_version);
        if self.project_id != config.project_id
            || self.root_path != config.root_path
            || self.root_hash != config.root_hash
            || self.namespace_id != config.binding.namespace_id
            || self.checkpoint_chain_id != config.binding.checkpoint_chain_id
            || self.schema_version != expected_schema
            || self.vector_space_id != config.vector_space_id
        {
            return Err(format!(
                "The persisted index identity of VCE project `{}` disagrees with the current configuration",
                config.project_id
            ));
        }
        Ok(())
    }

    /// Reports whether the persisted physical vector contract matches the requested one.
    /// 报告持久化物理向量契约是否与请求契约一致。
    fn vector_compatibility_matches(
        &self,
        config: &VceProjectIndexConfig,
        content_table: &str,
        path_table: &str,
    ) -> bool {
        self.physical_fingerprint == config.physical_fingerprint
            && self.preprocessing_version == config.preprocessing_version
            && self.chunker_version == config.chunker_version
            && self.dimension == i64::from(config.dimension)
            && self.content_table == content_table
            && self.path_table == path_table
    }
}

/// One serialized engine-state row.
/// 一条序列化引擎状态行。
#[derive(Clone, Debug, Deserialize)]
struct StateSnapshotRow {
    /// Serialized `VceStateSnapshot` payload.
    /// 序列化的 `VceStateSnapshot` 载荷。
    snapshot_json: String,
}

/// One durable idempotency fingerprint for an already published commit operation.
/// 一条已发布提交操作的持久幂等指纹。
#[derive(Clone, Debug, Deserialize)]
struct CommitFingerprintRow {
    /// SHA-256 fingerprint of every semantic commit request field.
    /// 全部语义提交请求字段的 SHA-256 指纹。
    request_fingerprint: String,
}

/// One persisted staging-generation lifecycle row.
/// 一条持久化暂存代次生命周期记录。
#[derive(Clone, Debug, Deserialize)]
struct GenerationStateRow {
    /// Current staging lifecycle state.
    /// 当前暂存生命周期状态。
    state: String,
}

/// One Blob readiness row used by `find_missing`.
/// `find_missing` 使用的一条 Blob 就绪状态行。
#[derive(Clone, Debug, Deserialize)]
struct BlobStatusRow {
    /// Stable Blob identity.
    /// 稳定 Blob 身份。
    blob_name: String,
    /// Persisted primary status.
    /// 持久主状态。
    status: String,
}

/// One aggregate count row.
/// 一条聚合数量行。
#[derive(Clone, Debug, Default, Deserialize)]
struct CountRow {
    /// Nonnegative SQL count.
    /// 非负 SQL 数量。
    count: i64,
}

/// Exact schema marker read before an existing database may receive initialization statements.
/// 现有数据库接收初始化语句前读取的精确结构标记。
#[derive(Deserialize)]
struct StorageSchemaRow {
    /// Persisted version text from the original VCE metadata table.
    /// 原始 VCE 元数据表中的持久化版本文本。
    value: String,
}

/// One stable Blob identity row.
/// 一条稳定 Blob 身份行。
#[derive(Clone, Debug, Deserialize)]
struct BlobNameRow {
    /// Stable Blob identity.
    /// 稳定 Blob 身份。
    blob_name: String,
}

/// One authoritative persisted checkpoint row.
/// 一条权威持久检查点行。
#[derive(Clone, Debug, Deserialize)]
struct CheckpointRow {
    /// Published checkpoint version.
    /// 已发布检查点版本。
    checkpoint_version: i64,
    /// Workspace generation represented by the checkpoint.
    /// 检查点代表的工作区代数。
    generation: i64,
    /// Serialized checkpoint membership.
    /// 序列化检查点成员关系。
    checkpoint_json: String,
}

impl CheckpointRow {
    /// Decodes the exact checkpoint member set.
    /// 解码精确检查点成员集合。
    fn checkpoint_members(&self) -> Result<BTreeSet<String>, String> {
        serde_json::from_str::<Checkpoint>(&self.checkpoint_json)
            .map(|checkpoint| checkpoint.members)
            .map_err(|error| format!("Failed to parse the VCE checkpoint: {error}"))
    }
}

/// One LanceDB candidate row containing the stored normalized vector.
/// 一条包含已存归一化向量的 LanceDB 候选行。
#[derive(Clone, Debug, Deserialize)]
struct LanceHitRow {
    /// Owning VCE namespace identity.
    /// 所属 VCE 命名空间身份。
    namespace_id: String,
    /// Stable Chunk or path-document identity.
    /// 稳定 Chunk 或路径文档身份。
    id: String,
    /// Owning Blob identity.
    /// 所属 Blob 身份。
    blob_name: String,
    /// Exact producer attempt pinned by the query's checkpoint member map.
    /// 查询检查点成员映射固定的精确生产尝试。
    refresh_id: String,
    /// Generation that wrote the derived vector row.
    /// 写入派生向量行的代次。
    generation: u64,
    /// Logical vector-space identity.
    /// 逻辑向量空间身份。
    vector_space_id: String,
    /// Physical Embedding compatibility fingerprint.
    /// 物理 Embedding 兼容性指纹。
    physical_fingerprint: String,
    /// Stored normalized vector.
    /// 已存归一化向量。
    vector: Vec<f32>,
}

/// One committed Chunk materialization row.
/// 一条已提交 Chunk 物化行。
#[derive(Clone, Debug, Deserialize)]
struct ChunkHitRow {
    /// Stable Chunk identity.
    /// 稳定 Chunk 身份。
    chunk_id: String,
    /// Owning Blob identity.
    /// 所属 Blob 身份。
    blob_name: String,
    /// Canonical workspace-relative path.
    /// 规范工作区相对路径。
    path: String,
    /// Verbatim source content.
    /// 原始源码内容。
    content: String,
    /// Stable content hash.
    /// 稳定内容哈希。
    content_hash: String,
    /// One-based start line.
    /// 从一开始的起始行。
    start_line: i64,
    /// One-based end line.
    /// 从一开始的结束行。
    end_line: i64,
    /// Unicode scalar offset in the first line.
    /// 首行内 Unicode 标量偏移。
    start_char_offset: i64,
    /// Persisted Chunk kind.
    /// 持久 Chunk 类型。
    kind: String,
}

impl ChunkHitRow {
    /// Converts one relational row into a validated VCE hit.
    /// 将一条关系行转换为经过校验的 VCE 命中。
    fn to_hit(&self, score: f32, source: SearchSource) -> Result<SearchHit, VcePortError> {
        if parse_chunk_kind(&self.kind)? == ChunkKind::Path {
            return Err(port_invalid(
                VcePortOperation::DenseSearch,
                "content vector resolved to a path-only Chunk",
            ));
        }
        Ok(SearchHit {
            id: self.chunk_id.clone(),
            blob_name: self.blob_name.clone(),
            path: self.path.clone(),
            range: SourceRange::new(
                positive_usize(self.start_line, "VCE start line")?,
                positive_usize(self.end_line, "VCE end line")?,
            )
            .map_err(|error| port_invalid(VcePortOperation::DenseSearch, error.to_string()))?,
            start_char_offset: nonnegative_usize(self.start_char_offset, "VCE start offset")?,
            content: self.content.clone(),
            content_hash: self.content_hash.clone(),
            score,
            source,
            kind: HitKind::Content,
        })
    }
}

/// One committed path-document row.
/// 一条已提交路径文档行。
#[derive(Clone, Debug, Deserialize)]
struct PathHitRow {
    /// Owning Blob identity.
    /// 所属 Blob 身份。
    blob_name: String,
    /// Canonical source path.
    /// 规范源码路径。
    path: String,
    /// Persisted path document text.
    /// 持久路径文档文本。
    path_document: String,
}

/// One committed exact-symbol row.
/// 一条已提交精确符号行。
#[derive(Clone, Debug, Deserialize)]
struct SymbolHitRow {
    /// Stable occurrence identity.
    /// 稳定出现位置身份。
    occurrence_id: String,
    /// Exact identifier text.
    /// 精确标识符文本。
    identifier: String,
    /// Owning Blob identity.
    /// 所属 Blob 身份。
    blob_name: String,
    /// Canonical source path.
    /// 规范源码路径。
    path: String,
    /// One-based start line.
    /// 从一开始的起始行。
    start_line: i64,
    /// One-based end line.
    /// 从一开始的结束行。
    end_line: i64,
    /// Unicode scalar offset.
    /// Unicode 标量偏移。
    start_char_offset: i64,
    /// Owning Chunk identity.
    /// 所属 Chunk 身份。
    chunk_id: String,
    /// Verbatim source text.
    /// 原始源码文本。
    content: String,
    /// Persisted symbol kind.
    /// 持久符号类型。
    kind: String,
}

impl SymbolHitRow {
    /// Converts one exact-symbol row into a ranked VCE hit.
    /// 将一条精确符号行转换为已排序 VCE 命中。
    fn into_hit(self) -> Result<SearchHit, VcePortError> {
        if self.identifier.trim().is_empty() || self.chunk_id.trim().is_empty() {
            return Err(port_invalid(
                VcePortOperation::ExactSearch,
                "exact symbol row omitted its identifier or Chunk identity",
            ));
        }
        let kind = parse_symbol_kind(&self.kind)?;
        let hit_kind = match kind {
            SymbolKind::Endpoint => HitKind::Endpoint,
            SymbolKind::Definition => HitKind::Definition,
            SymbolKind::Reference => HitKind::Reference,
        };
        let score = match kind {
            SymbolKind::Endpoint => 1.0,
            SymbolKind::Definition => 0.95,
            SymbolKind::Reference => 0.8,
        };
        Ok(SearchHit {
            id: self.occurrence_id,
            blob_name: self.blob_name,
            path: self.path,
            range: SourceRange::new(
                positive_usize(self.start_line, "VCE symbol start line")?,
                positive_usize(self.end_line, "VCE symbol end line")?,
            )
            .map_err(|error| port_invalid(VcePortOperation::ExactSearch, error.to_string()))?,
            start_char_offset: nonnegative_usize(self.start_char_offset, "VCE symbol offset")?,
            content_hash: content_hash(&self.content),
            content: self.content,
            score,
            source: SearchSource::Exact,
            kind: hit_kind,
        })
    }
}

/// One aggregate committed-index count row.
/// 一条已提交索引聚合数量行。
#[derive(Clone, Debug, Default, Deserialize)]
struct IndexCountsRow {
    /// Blob count.
    /// Blob 数量。
    blobs: i64,
    /// Searchable file count.
    /// 可检索文件数量。
    searchable_files: i64,
    /// Chunk count.
    /// Chunk 数量。
    chunks: i64,
    /// Content and path vector count.
    /// 内容与路径向量数量。
    vectors: i64,
    /// Path-document count.
    /// 路径文档数量。
    path_documents: i64,
    /// Exact-symbol count.
    /// 精确符号数量。
    symbols: i64,
}

/// One file-inventory projection row.
/// 一条文件清单投影行。
#[derive(Clone, Debug, Deserialize)]
struct FileSummaryRow {
    /// Canonical source path.
    /// 规范源码路径。
    path: String,
    /// Stable Blob identity.
    /// 稳定 Blob 身份。
    blob_name: String,
    /// Workspace generation.
    /// 工作区代数。
    generation: i64,
    /// Persisted path-channel state.
    /// 持久路径通道状态。
    path_status: String,
    /// Optional path-channel error.
    /// 可选路径通道错误。
    path_error: Option<String>,
    /// Persisted symbol-channel state.
    /// 持久符号通道状态。
    symbol_status: String,
    /// Optional symbol-channel error.
    /// 可选符号通道错误。
    symbol_error: Option<String>,
    /// Optional primary status reason.
    /// 可选主状态原因。
    status_reason: Option<String>,
    /// Chunk count.
    /// Chunk 数量。
    chunk_count: i64,
    /// Content-vector count projected separately from the stored content-and-path total.
    /// 从持久化内容与路径总数中独立投影的内容向量数量。
    vector_count: i64,
    /// Exact-symbol count.
    /// 精确符号数量。
    symbol_count: i64,
}

impl FileSummaryRow {
    /// Converts a committed row into a management-safe file summary.
    /// 将已提交行转换为可安全管理的文件摘要。
    fn into_summary(self) -> Result<VceFileSummary, VcePortError> {
        let last_error = self
            .status_reason
            .clone()
            .or_else(|| self.path_error.clone())
            .or_else(|| self.symbol_error.clone());
        Ok(VceFileSummary {
            path: self.path,
            blob_name: Some(self.blob_name.clone()),
            committed_blob_name: Some(self.blob_name),
            status: FileStatus::Present,
            generation: nonnegative_u64(self.generation, "VCE file generation")?,
            size: 0,
            chunk_count: nonnegative_u64(self.chunk_count, "VCE file chunk count")?,
            vector_count: nonnegative_u64(self.vector_count, "VCE file vector count")?,
            symbol_count: nonnegative_u64(self.symbol_count, "VCE file symbol count")?,
            path_channel: parse_channel_state(&self.path_status, self.path_error)?,
            symbol_channel: parse_channel_state(&self.symbol_status, self.symbol_error)?,
            last_error,
        })
    }
}

/// One committed Blob detail row.
/// 一条已提交 Blob 详情行。
#[derive(Clone, Debug, Deserialize)]
struct BlobDetailRow {
    /// Stable Blob identity.
    /// 稳定 Blob 身份。
    blob_name: String,
    /// Canonical source path.
    /// 规范源码路径。
    path: String,
    /// Workspace generation.
    /// 工作区代数。
    generation: i64,
    /// Optional structure analyzer identity.
    /// 可选结构分析器身份。
    structure_analyzer_id: Option<String>,
    /// Optional recursive-fallback diagnostic.
    /// 可选递归回退诊断。
    structure_fallback_reason: Option<String>,
    /// Persisted path-channel state.
    /// 持久路径通道状态。
    path_status: String,
    /// Optional path-channel error.
    /// 可选路径通道错误。
    path_error: Option<String>,
    /// Persisted symbol-channel state.
    /// 持久符号通道状态。
    symbol_status: String,
    /// Optional symbol-channel error.
    /// 可选符号通道错误。
    symbol_error: Option<String>,
}

impl BlobDetailRow {
    /// Combines one Blob row with its ordered Chunk summaries.
    /// 将一条 Blob 行与其有序 Chunk 摘要组合。
    fn into_detail(self, chunks: Vec<VceChunkSummary>) -> Result<VceBlobDetail, VcePortError> {
        Ok(VceBlobDetail {
            blob_name: self.blob_name,
            path: self.path,
            generation: nonnegative_u64(self.generation, "VCE Blob generation")?,
            structure_analyzer_id: self.structure_analyzer_id,
            structure_fallback_reason: self.structure_fallback_reason,
            chunks,
            path_channel: parse_channel_state(&self.path_status, self.path_error)?,
            symbol_channel: parse_channel_state(&self.symbol_status, self.symbol_error)?,
        })
    }
}

/// One Chunk detail projection row.
/// 一条 Chunk 详情投影行。
#[derive(Clone, Debug, Deserialize)]
struct ChunkDetailRow {
    /// Stable Chunk identity.
    /// 稳定 Chunk 身份。
    chunk_id: String,
    /// One-based start line.
    /// 从一开始的起始行。
    start_line: i64,
    /// One-based end line.
    /// 从一开始的结束行。
    end_line: i64,
    /// Unicode scalar offset.
    /// Unicode 标量偏移。
    start_char_offset: i64,
    /// Persisted Chunk kind.
    /// 持久 Chunk 类型。
    kind: String,
    /// Bounded source preview.
    /// 有界源码预览。
    preview: String,
    /// Whether a compatible vector exists.
    /// 是否存在兼容向量。
    vector_ready: i64,
}

impl ChunkDetailRow {
    /// Converts one row into a management Chunk summary.
    /// 将一条记录转换为管理 Chunk 摘要。
    fn into_summary(self) -> Result<VceChunkSummary, VcePortError> {
        Ok(VceChunkSummary {
            chunk_id: self.chunk_id,
            range: SourceRange::new(
                positive_usize(self.start_line, "VCE chunk start line")?,
                positive_usize(self.end_line, "VCE chunk end line")?,
            )
            .map_err(|error| port_invalid(VcePortOperation::ManagementQuery, error.to_string()))?,
            start_char_offset: nonnegative_usize(self.start_char_offset, "VCE chunk offset")?,
            kind: parse_chunk_kind(&self.kind)?,
            preview: Some(self.preview),
            vector_ready: self.vector_ready == 1,
        })
    }
}

/// One durable operation row shared by status and history projections.
/// 状态与历史投影共享的一条耐久操作行。
#[derive(Clone, Debug, Deserialize)]
struct OperationRow {
    /// Stable operation identity.
    /// 稳定操作身份。
    operation_id: String,
    /// Persisted operation kind.
    /// 持久操作类型。
    kind: String,
    /// Persisted operation phase.
    /// 持久操作阶段。
    phase: String,
    /// Workspace generation.
    /// 工作区代数。
    generation: i64,
    /// Optional refresh mode.
    /// 可选刷新模式。
    refresh_mode: Option<String>,
    /// Optional refresh reason.
    /// 可选刷新原因。
    refresh_reason: Option<String>,
    /// Completed unit count.
    /// 已完成单元数量。
    completed_units: i64,
    /// Total unit count.
    /// 总单元数量。
    total_units: i64,
    /// One-based attempt count.
    /// 从一开始的尝试次数。
    attempt: i64,
    /// Coalescing flag.
    /// 合并标志。
    coalesced: i64,
    /// Optional terminal error.
    /// 可选终止错误。
    error: Option<String>,
}

impl OperationRow {
    /// Converts one operation row into the durable management projection.
    /// 将一条操作行转换为耐久管理投影。
    fn into_summary(self) -> Result<VceOperationSummary, VcePortError> {
        Ok(VceOperationSummary {
            operation_id: self.operation_id,
            kind: parse_operation_kind(&self.kind)?,
            phase: parse_operation_phase(&self.phase)?,
            generation: nonnegative_u64(self.generation, "VCE operation generation")?,
            refresh_mode: self
                .refresh_mode
                .as_deref()
                .map(parse_refresh_mode)
                .transpose()?,
            refresh_reason: self
                .refresh_reason
                .as_deref()
                .map(parse_refresh_reason)
                .transpose()?,
            completed_units: nonnegative_u64(
                self.completed_units,
                "VCE operation completed units",
            )?,
            total_units: nonnegative_u64(self.total_units, "VCE operation total units")?,
            attempt: u32::try_from(self.attempt).map_err(|_| {
                port_invalid(
                    VcePortOperation::ManagementQuery,
                    "VCE operation attempt is invalid",
                )
            })?,
            coalesced: self.coalesced == 1,
            error: self.error,
        })
    }

    /// Converts one nonterminal operation row into the compact index-job projection.
    /// 将一条未终止操作行转换为紧凑索引作业投影。
    fn into_index_job(self) -> Result<VceIndexJobSummary, VcePortError> {
        let summary = self.into_summary()?;
        Ok(VceIndexJobSummary {
            job_id: summary.operation_id,
            kind: summary.kind,
            generation: summary.generation,
            mode: summary.refresh_mode,
            reason: summary.refresh_reason,
            phase: summary.phase,
            attempt: summary.attempt,
            completed_units: summary.completed_units,
            total_units: summary.total_units,
            unit: None,
            started_at_ns: None,
            finished_at_ns: None,
            coalesced: summary.coalesced,
            error: summary.error,
        })
    }
}

/// One optional operation-error row.
/// 一条可选操作错误行。
#[derive(Clone, Debug, Deserialize)]
struct ErrorRow {
    /// Optional terminal error.
    /// 可选终止错误。
    error: Option<String>,
}

/// One committed Blob row used by integrity analysis.
/// 完整性分析使用的一条已提交 Blob 行。
#[derive(Clone, Debug, Deserialize)]
struct IntegrityBlobRow {
    /// Stable Blob identity.
    /// 稳定 Blob 身份。
    blob_name: String,
    /// Canonical source path.
    /// 规范源码路径。
    path: String,
    /// Primary semantic status.
    /// 主语义状态。
    status: String,
    /// Persisted explanation for skipped or failed content indexing.
    /// 内容索引被跳过或失败的持久原因。
    status_reason: Option<String>,
    /// Path-channel status.
    /// 路径通道状态。
    path_status: String,
    /// Symbol-channel status.
    /// 符号通道状态。
    symbol_status: String,
    /// Exact structural-analysis diagnostic retained for this Blob.
    /// 为此 Blob 保留的精确结构分析诊断。
    symbol_error: Option<String>,
    /// Chunk count.
    /// Chunk 数量。
    chunk_count: i64,
    /// Vector count.
    /// 向量数量。
    vector_count: i64,
}

/// One unpublished staging row used by integrity analysis.
/// 完整性分析使用的一条未发布暂存行。
#[derive(Clone, Debug, Deserialize)]
struct StagingIssueRow {
    /// Exact attempt identity; equal content under different attempts must remain distinguishable.
    /// 精确尝试身份；不同尝试下相同内容必须保持可区分。
    refresh_id: String,
    /// Exact generation paired with the attempt identity.
    /// 与尝试身份配套的精确代次。
    generation: i64,
    /// Stable staged Blob identity.
    /// 稳定暂存 Blob 身份。
    blob_name: String,
}

/// One durable derived-vector cleanup request.
/// 一项持久化派生向量清理请求。
#[derive(Clone, Debug, Deserialize)]
struct VectorCleanupRow {
    /// Stable idempotent cleanup identity.
    /// 稳定幂等清理身份。
    cleanup_id: String,
    /// Cleanup predicate kind.
    /// 清理谓词类型。
    cleanup_kind: String,
    /// Optional staging refresh identity.
    /// 可选暂存刷新身份。
    refresh_id: Option<String>,
    /// Optional generation selected by the cleanup.
    /// 清理选择的可选代次。
    generation: Option<i64>,
    /// Optional serialized Blob identity set.
    /// 可选序列化 Blob 身份集合。
    blob_names_json: Option<String>,
    /// Exact physical Embedding fingerprint represented by the rows.
    /// 向量行表示的精确物理 Embedding 指纹。
    physical_fingerprint: String,
    /// Physical content-vector table.
    /// 物理内容向量表。
    content_table: String,
    /// Physical path-vector table.
    /// 物理路径向量表。
    path_table: String,
}

/// One unpublished generation belonging to an exact host-owned refresh attempt.
/// 属于精确宿主自有刷新尝试的一个未发布代次。
#[derive(Clone, Debug, Deserialize)]
struct RefreshGenerationRow {
    /// Positive generation number.
    /// 正代次编号。
    generation: i64,
}

/// One pending cleanup diagnostic projected into index integrity.
/// 投影到索引完整性的一项待处理清理诊断。
#[derive(Clone, Debug, Deserialize)]
struct VectorCleanupIssueRow {
    /// Stable cleanup identity.
    /// 稳定清理身份。
    cleanup_id: String,
    /// Optional last cleanup error.
    /// 可选的最近清理错误。
    error: Option<String>,
}

/// One durable publication intent used to recover an uncertain commit boundary.
/// 用于恢复不确定提交边界的一条持久化发布意图。
#[derive(Clone, Debug, Deserialize)]
struct PublicationIntentRow {
    /// Fingerprint of the exact commit request admitted for this operation.
    /// 此操作已准入的精确提交请求指纹。
    request_fingerprint: String,
    /// Last durable publication stage.
    /// 最近一次持久化的发布阶段。
    stage: String,
}

/// Borrowed fields required to persist one host operation projection.
/// 持久化一项宿主操作投影所需的借用字段。
struct OperationRecord<'a> {
    /// Stable operation identity.
    /// 稳定操作身份。
    operation_id: &'a str,
    /// Logical operation kind.
    /// 逻辑操作类型。
    kind: VceOperationKind,
    /// Current lifecycle phase.
    /// 当前生命周期阶段。
    phase: VceOperationPhase,
    /// Workspace generation.
    /// 工作区代数。
    generation: u64,
    /// Completed work units.
    /// 已完成工作单元。
    completed_units: u64,
    /// Total work units.
    /// 工作单元总数。
    total_units: u64,
    /// Optional terminal diagnostic.
    /// 可选终止诊断。
    error: Option<&'a str>,
}

/// Builds one staged Chunk parameter list in schema order.
/// 按 Schema 顺序构建一组暂存 Chunk 参数。
fn staged_chunk_params(
    namespace_id: &str,
    refresh_id: &str,
    generation: u64,
    chunk: &Chunk,
    vector_ready: bool,
) -> Result<Vec<SqliteValue>, String> {
    Ok(vec![
        sqlite_text(namespace_id),
        sqlite_text(refresh_id),
        sqlite_u64_raw(generation, "VCE generation")?,
        sqlite_text(&chunk.blob_name),
        sqlite_text(&chunk.chunk_id),
        sqlite_text(&chunk.content_hash),
        sqlite_text(&chunk.path),
        sqlite_text(&chunk.content),
        sqlite_usize_raw(chunk.range.start_line, "VCE chunk start line")?,
        sqlite_usize_raw(chunk.range.end_line, "VCE chunk end line")?,
        sqlite_usize_raw(chunk.start_char_offset, "VCE chunk offset")?,
        sqlite_text(chunk_kind_text(chunk.kind)),
        SqliteValue::Integer(i64::from(vector_ready)),
    ])
}

/// Builds one staged exact-symbol parameter list in schema order.
/// 按 Schema 顺序构建一组暂存精确符号参数。
fn staged_symbol_params(
    namespace_id: &str,
    refresh_id: &str,
    generation: u64,
    symbol: &SymbolOccurrence,
) -> Result<Vec<SqliteValue>, String> {
    Ok(vec![
        sqlite_text(namespace_id),
        sqlite_text(refresh_id),
        sqlite_u64_raw(generation, "VCE generation")?,
        sqlite_text(&symbol.blob_name),
        sqlite_text(&symbol.occurrence_id),
        sqlite_text(&symbol.identifier),
        sqlite_text(&symbol.path),
        sqlite_usize_raw(symbol.range.start_line, "VCE symbol start line")?,
        sqlite_usize_raw(symbol.range.end_line, "VCE symbol end line")?,
        sqlite_usize_raw(symbol.start_char_offset, "VCE symbol offset")?,
        sqlite_text(&symbol.chunk_id),
        sqlite_text(symbol_kind_text(symbol.kind)),
    ])
}

/// Validates one operation identity at the VCE storage boundary.
/// 在 VCE 存储边界校验一个操作身份。
fn require_operation_id(
    operation_id: &str,
    operation: VcePortOperation,
) -> Result<(), VcePortError> {
    if operation_id.trim().is_empty() {
        return Err(port_rejected(
            operation,
            "operation identity must not be empty",
        ));
    }
    Ok(())
}

/// Validates one bounded vector-search scope.
/// 校验一个有界向量检索范围。
fn validate_search_scope(
    allowed_blobs: &BTreeSet<String>,
    limit: usize,
    vector: &[f32],
) -> Result<(), String> {
    if allowed_blobs.is_empty() {
        return Err(String::from("vector search Blob scope must not be empty"));
    }
    if limit == 0 || limit > VCE_VECTOR_SEARCH_LIMIT as usize {
        return Err(String::from(
            "vector search limit is outside the supported range",
        ));
    }
    if vector.is_empty() || vector.iter().any(|value| !value.is_finite()) {
        return Err(String::from("vector search query is invalid"));
    }
    Ok(())
}

/// Validates one exact vector dimension and finite payload.
/// 校验一个精确向量维度与有限数值载荷。
fn validate_vector(vector: &[f32], dimension: u32) -> Result<(), String> {
    let expected = usize::try_from(dimension)
        .map_err(|_| String::from("VCE vector dimension exceeds platform limits"))?;
    if vector.len() != expected || vector.iter().any(|value| !value.is_finite()) {
        return Err(format!(
            "VCE vector payload does not match dimension {dimension}"
        ));
    }
    Ok(())
}

/// Computes deterministic cosine similarity for finite equal-length vectors.
/// 为有限且等长向量计算确定性余弦相似度。
fn cosine_similarity(left: &[f32], right: &[f32]) -> Result<f32, VcePortError> {
    if left.len() != right.len() || left.is_empty() {
        return Err(port_invalid(
            VcePortOperation::DenseSearch,
            "VCE search vector dimensions do not match",
        ));
    }
    let dot = left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    if !dot.is_finite() {
        return Err(port_invalid(
            VcePortOperation::DenseSearch,
            "VCE search similarity is not finite",
        ));
    }
    Ok(dot)
}

/// Creates one failed host-port error.
/// 创建一项宿主端口执行失败错误。
fn port_failed(operation: VcePortOperation, message: impl Into<String>) -> VcePortError {
    VcePortError::Failed {
        operation,
        message: message.into(),
    }
}

/// Creates one rejected host-port error.
/// 创建一项宿主端口拒绝错误。
fn port_rejected(operation: VcePortOperation, reason: impl Into<String>) -> VcePortError {
    VcePortError::Rejected {
        operation,
        reason: reason.into(),
    }
}

/// Creates one invalid-response host-port error.
/// 创建一项宿主端口无效响应错误。
fn port_invalid(operation: VcePortOperation, reason: impl Into<String>) -> VcePortError {
    VcePortError::InvalidResponse {
        operation,
        reason: reason.into(),
    }
}

/// Converts one auxiliary status into relational fields.
/// 将一项辅助状态转换为关系字段。
fn auxiliary_status_fields(status: &AuxiliaryIndexStatus) -> (&'static str, Option<&str>) {
    match status {
        AuxiliaryIndexStatus::Ready => ("ready", None),
        AuxiliaryIndexStatus::NotApplicable => ("not_applicable", None),
        AuxiliaryIndexStatus::Failed { reason } => ("failed", Some(reason.as_str())),
    }
}

/// Converts one primary Blob status into relational fields.
/// 将一项主 Blob 状态转换为关系字段。
fn indexed_status_fields(status: &IndexedBlobStatus) -> (&'static str, Option<&str>) {
    match status {
        IndexedBlobStatus::Ready => ("ready", None),
        IndexedBlobStatus::ReadyWithoutEmbedding { reason } => {
            ("ready_without_embedding", Some(reason.as_str()))
        }
    }
}

/// Returns the stable relational text for one structure mode.
/// 返回一个结构模式的稳定关系文本。
fn structure_mode_text(mode: StructureMode) -> &'static str {
    match mode {
        StructureMode::SemanticAst => "semantic_ast",
        StructureMode::RecursiveFallback => "recursive_fallback",
    }
}

/// Returns the stable relational text for one Chunk kind.
/// 返回一个 Chunk 类型的稳定关系文本。
fn chunk_kind_text(kind: ChunkKind) -> &'static str {
    match kind {
        ChunkKind::Ast => "ast",
        ChunkKind::Recursive => "recursive",
        ChunkKind::Path => "path",
    }
}

/// Parses one persisted Chunk kind.
/// 解析一个持久 Chunk 类型。
fn parse_chunk_kind(value: &str) -> Result<ChunkKind, VcePortError> {
    match value {
        "ast" => Ok(ChunkKind::Ast),
        "recursive" => Ok(ChunkKind::Recursive),
        "path" => Ok(ChunkKind::Path),
        _ => Err(port_invalid(
            VcePortOperation::ManagementQuery,
            format!("unknown VCE Chunk kind `{value}`"),
        )),
    }
}

/// Returns the stable relational text for one symbol kind.
/// 返回一个符号类型的稳定关系文本。
fn symbol_kind_text(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Endpoint => "endpoint",
        SymbolKind::Definition => "definition",
        SymbolKind::Reference => "reference",
    }
}

/// Parses one persisted symbol kind.
/// 解析一个持久符号类型。
fn parse_symbol_kind(value: &str) -> Result<SymbolKind, VcePortError> {
    match value {
        "endpoint" => Ok(SymbolKind::Endpoint),
        "definition" => Ok(SymbolKind::Definition),
        "reference" => Ok(SymbolKind::Reference),
        _ => Err(port_invalid(
            VcePortOperation::ExactSearch,
            format!("unknown VCE symbol kind `{value}`"),
        )),
    }
}

/// Parses one persisted auxiliary-channel state.
/// 解析一个持久辅助通道状态。
fn parse_channel_state(
    value: &str,
    error: Option<String>,
) -> Result<VceChannelState, VcePortError> {
    match value {
        "ready" => Ok(VceChannelState::Ready),
        "pending" => Ok(VceChannelState::Pending),
        "not_applicable" => Ok(VceChannelState::NotApplicable),
        "failed" => Ok(VceChannelState::Failed {
            reason: error.ok_or_else(|| {
                port_invalid(
                    VcePortOperation::ManagementQuery,
                    "failed VCE channel omitted its diagnostic",
                )
            })?,
        }),
        _ => Err(port_invalid(
            VcePortOperation::ManagementQuery,
            format!("unknown VCE channel state `{value}`"),
        )),
    }
}

/// Returns stable relational text for one operation kind.
/// 返回一个操作类型的稳定关系文本。
fn operation_kind_text(kind: VceOperationKind) -> &'static str {
    match kind {
        VceOperationKind::InitialBuild => "initial_build",
        VceOperationKind::IncrementalRefresh => "incremental_refresh",
        VceOperationKind::StorageMutation => "storage_mutation",
        VceOperationKind::VerifyIntegrity => "verify_integrity",
        VceOperationKind::Repair => "repair",
        VceOperationKind::Rebuild => "rebuild",
    }
}

/// Parses stable relational text into one operation kind.
/// 将稳定关系文本解析为操作类型。
fn parse_operation_kind(value: &str) -> Result<VceOperationKind, VcePortError> {
    match value {
        "initial_build" => Ok(VceOperationKind::InitialBuild),
        "incremental_refresh" => Ok(VceOperationKind::IncrementalRefresh),
        "storage_mutation" => Ok(VceOperationKind::StorageMutation),
        "verify_integrity" => Ok(VceOperationKind::VerifyIntegrity),
        "repair" => Ok(VceOperationKind::Repair),
        "rebuild" => Ok(VceOperationKind::Rebuild),
        _ => Err(port_invalid(
            VcePortOperation::ManagementQuery,
            format!("unknown VCE operation kind `{value}`"),
        )),
    }
}

/// Returns stable relational text for one operation phase.
/// 返回一个操作阶段的稳定关系文本。
fn operation_phase_text(phase: VceOperationPhase) -> &'static str {
    match phase {
        VceOperationPhase::Queued => "queued",
        VceOperationPhase::Reconciling => "reconciling",
        VceOperationPhase::ReadingSource => "reading_source",
        VceOperationPhase::Structuring => "structuring",
        VceOperationPhase::Embedding => "embedding",
        VceOperationPhase::Staging => "staging",
        VceOperationPhase::Publishing => "publishing",
        VceOperationPhase::Completed => "completed",
        VceOperationPhase::Cancelled => "cancelled",
        VceOperationPhase::Failed => "failed",
    }
}

/// Parses stable relational text into one operation phase.
/// 将稳定关系文本解析为操作阶段。
fn parse_operation_phase(value: &str) -> Result<VceOperationPhase, VcePortError> {
    match value {
        "queued" => Ok(VceOperationPhase::Queued),
        "reconciling" => Ok(VceOperationPhase::Reconciling),
        "reading_source" => Ok(VceOperationPhase::ReadingSource),
        "structuring" => Ok(VceOperationPhase::Structuring),
        "embedding" => Ok(VceOperationPhase::Embedding),
        "staging" => Ok(VceOperationPhase::Staging),
        "publishing" => Ok(VceOperationPhase::Publishing),
        "completed" => Ok(VceOperationPhase::Completed),
        "cancelled" => Ok(VceOperationPhase::Cancelled),
        "failed" => Ok(VceOperationPhase::Failed),
        _ => Err(port_invalid(
            VcePortOperation::ManagementQuery,
            format!("unknown VCE operation phase `{value}`"),
        )),
    }
}

/// Parses one optional persisted refresh mode.
/// 解析一个可选持久刷新模式。
fn parse_refresh_mode(value: &str) -> Result<RefreshMode, VcePortError> {
    match value {
        "full" => Ok(RefreshMode::Full),
        "paths" => Ok(RefreshMode::Paths),
        _ => Err(port_invalid(
            VcePortOperation::ManagementQuery,
            format!("unknown VCE refresh mode `{value}`"),
        )),
    }
}

/// Parses one optional persisted refresh reason.
/// 解析一个可选持久刷新原因。
fn parse_refresh_reason(value: &str) -> Result<RefreshReason, VcePortError> {
    match value {
        "initial" => Ok(RefreshReason::Initial),
        "file_watcher" => Ok(RefreshReason::FileWatcher),
        "git_watcher" => Ok(RefreshReason::GitWatcher),
        "manual" => Ok(RefreshReason::Manual),
        "recovery" => Ok(RefreshReason::Recovery),
        _ => Err(port_invalid(
            VcePortOperation::ManagementQuery,
            format!("unknown VCE refresh reason `{value}`"),
        )),
    }
}

/// Builds a stable issue identity from its exact semantic fields.
/// 根据精确语义字段构建稳定问题身份。
fn integrity_issue(
    kind: VceIntegrityIssueKind,
    path: Option<String>,
    blob_name: Option<String>,
    message: &str,
) -> VceIntegrityIssue {
    let mut hasher = Sha256::new();
    hasher.update(integrity_kind_text(&kind).as_bytes());
    hasher.update(b"\n");
    hasher.update(path.as_deref().unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(blob_name.as_deref().unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(message.as_bytes());
    VceIntegrityIssue {
        issue_id: hex::encode(hasher.finalize()),
        kind,
        path,
        blob_name,
        message: String::from(message),
    }
}

/// Returns stable text for one integrity issue kind.
/// 返回一个完整性问题类型的稳定文本。
fn integrity_kind_text(kind: &VceIntegrityIssueKind) -> &'static str {
    match kind {
        VceIntegrityIssueKind::MissingBlob => "missing_blob",
        VceIntegrityIssueKind::IncompleteContentIndex => "incomplete_content_index",
        VceIntegrityIssueKind::PathIndexMismatch => "path_index_mismatch",
        VceIntegrityIssueKind::SymbolIndexMismatch => "symbol_index_mismatch",
        VceIntegrityIssueKind::VectorSpaceMismatch => "vector_space_mismatch",
        VceIntegrityIssueKind::OrphanedStaging => "orphaned_staging",
    }
}

/// Builds a stable maintenance action identity from mode and affected Blobs.
/// 根据模式与受影响 Blob 构建稳定维护操作身份。
fn maintenance_action_id(mode: VceMaintenanceMode, blob_names: &[String]) -> String {
    let mode = match mode {
        VceMaintenanceMode::Repair => "repair",
        VceMaintenanceMode::Rebuild => "rebuild",
        VceMaintenanceMode::CleanupStaging => "cleanup_staging",
        VceMaintenanceMode::Compact => "compact",
    };
    let mut hasher = Sha256::new();
    hasher.update(mode.as_bytes());
    for blob_name in blob_names {
        hasher.update(b"\n");
        hasher.update(blob_name.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Build one stable compaction action identity from the exact table version and options.
/// 根据精确表版本与参数构建一个稳定整理操作身份。
fn physical_compaction_action_id(table: &VcePhysicalTablePlan) -> String {
    let role = physical_table_role_text(table.statistics.role);
    let options = &table.options;
    let mut hasher = Sha256::new();
    for value in [
        "compact".to_owned(),
        role.to_owned(),
        table.statistics.table_name.clone(),
        table.statistics.version.to_string(),
        options.target_rows_per_fragment.to_string(),
        options.max_rows_per_group.to_string(),
        options.max_bytes_per_file.to_string(),
        options.num_threads.to_string(),
        options.batch_size.to_string(),
        options.max_source_fragments.to_string(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

/// Return the stable storage-safe text for one physical table role.
/// 返回一个物理表角色的稳定存储安全文本。
fn physical_table_role_text(role: VcePhysicalTableRole) -> &'static str {
    match role {
        VcePhysicalTableRole::Content => "content",
        VcePhysicalTableRole::Path => "path",
    }
}

/// Return the bounded options frozen into every current VCE compaction action.
/// 返回冻结到每个当前 VCE 整理操作中的有界参数。
fn physical_compaction_options() -> VceCompactionOptions {
    VceCompactionOptions {
        target_rows_per_fragment: VCE_COMPACTION_TARGET_ROWS_PER_FRAGMENT,
        max_rows_per_group: VCE_COMPACTION_MAX_ROWS_PER_GROUP,
        max_bytes_per_file: VCE_COMPACTION_MAX_BYTES_PER_FILE,
        num_threads: VCE_COMPACTION_THREADS,
        batch_size: VCE_COMPACTION_SCAN_BATCH_ROWS,
        max_source_fragments: VCE_COMPACTION_MAX_SOURCE_FRAGMENTS,
    }
}

/// Narrows VCE compaction options into the engine options used to plan and to compact.
/// 将 VCE 整理参数收窄为计划与整理共用的引擎参数。
///
/// The maintenance contract states these options as u64 while the engine takes platform-sized
/// values, so the narrowing is explicit and refuses silently truncated settings.
/// 维护契约把这些选项表述为 u64，而引擎取平台字长的值，故收窄是显式的，并拒绝被静默截断的设置。
fn engine_compaction_options(
    options: &VceCompactionOptions,
) -> Result<LanceDbCompactionOptions, String> {
    let narrowing = |value: u64, name: &str| {
        usize::try_from(value)
            .map(Some)
            .map_err(|_| format!("VCE compaction option {name} exceeds platform limits"))
    };
    Ok(LanceDbCompactionOptions {
        target_rows_per_fragment: narrowing(
            options.target_rows_per_fragment,
            "target_rows_per_fragment",
        )?,
        max_rows_per_group: narrowing(options.max_rows_per_group, "max_rows_per_group")?,
        max_bytes_per_file: narrowing(options.max_bytes_per_file, "max_bytes_per_file")?,
        num_threads: narrowing(options.num_threads, "num_threads")?,
        batch_size: narrowing(options.batch_size, "batch_size")?,
        max_source_fragments: narrowing(options.max_source_fragments, "max_source_fragments")?,
    })
}

/// Projects engine statistics into the VCE maintenance contract.
/// 将引擎统计信息投影到 VCE 维护契约。
fn map_physical_table_statistics(
    role: VcePhysicalTableRole,
    statistics: LanceDbTableStatisticsResult,
) -> VcePhysicalTableStatistics {
    VcePhysicalTableStatistics {
        role,
        table_name: statistics.table_name,
        version: statistics.version,
        total_bytes: statistics.total_bytes,
        row_count: statistics.row_count,
        index_count: statistics.index_count,
        fragment_count: statistics.fragment_count,
        small_fragment_count: statistics.small_fragment_count,
        compaction_task_count: statistics.compaction_task_count,
        fragment_rows: VcePhysicalFragmentSummary {
            minimum_rows: statistics.fragment_rows.minimum_rows,
            maximum_rows: statistics.fragment_rows.maximum_rows,
            mean_rows: statistics.fragment_rows.mean_rows,
            p25_rows: statistics.fragment_rows.p25_rows,
            p50_rows: statistics.fragment_rows.p50_rows,
            p75_rows: statistics.fragment_rows.p75_rows,
            p99_rows: statistics.fragment_rows.p99_rows,
        },
    }
}

/// Validates the immutable identity repeated by one compaction-operation response.
/// 校验一条整理操作响应重复携带的不可变身份。
///
/// `expected_operation_id` and `expected_table_name` come from the admitted host receipt;
/// `operation` is the untrusted engine projection. Returns the same projection only on an
/// exact identity match.
/// `expected_operation_id` 与 `expected_table_name` 来自已准入宿主回执；`operation` 是不受信任的
/// 存储操作投影。仅在身份精确匹配时返回原投影。
fn validate_physical_compaction_operation_identity(
    expected_operation_id: &str,
    expected_table_name: &str,
    operation: VcePhysicalCompactionOperation,
) -> Result<VcePhysicalCompactionOperation, String> {
    if operation.operation_id != expected_operation_id {
        return Err(String::from(
            "VCE physical compaction response returned a different operation identity",
        ));
    }
    if operation.table_name != expected_table_name {
        return Err(String::from(
            "VCE physical compaction operation belongs to a different table",
        ));
    }
    Ok(operation)
}

/// Applies an opaque offset cursor to one bounded in-memory management page.
/// 将不透明偏移游标应用到一个有界内存管理分页。
fn page_items<T>(items: Vec<T>, page: VcePageRequest) -> Result<VcePage<T>, VcePortError> {
    if page.limit == 0 || page.limit > 500 {
        return Err(port_rejected(
            VcePortOperation::ManagementQuery,
            "management page limit is outside the supported range",
        ));
    }
    let offset = match page.cursor.as_deref() {
        None => 0,
        Some(cursor) => cursor
            .strip_prefix("offset:")
            .ok_or_else(|| {
                port_rejected(
                    VcePortOperation::ManagementQuery,
                    "management cursor has an invalid format",
                )
            })?
            .parse::<usize>()
            .map_err(|_| {
                port_rejected(
                    VcePortOperation::ManagementQuery,
                    "management cursor has an invalid offset",
                )
            })?,
    };
    if offset > items.len() {
        return Err(port_rejected(
            VcePortOperation::ManagementQuery,
            "management cursor is outside the result set",
        ));
    }
    let total = items.len() as u64;
    let end = offset.saturating_add(page.limit).min(items.len());
    let next_cursor = (end < items.len()).then(|| format!("offset:{end}"));
    Ok(VcePage {
        items: items.into_iter().skip(offset).take(page.limit).collect(),
        next_cursor,
        total: Some(total),
    })
}

/// Builds one VCE dimension-specific physical vector table name.
/// 构建一个 VCE 维度特定的物理向量表名。
fn vector_table_name(
    channel: &str,
    vector_space_id: &str,
    physical_fingerprint: &str,
    preprocessing_version: &str,
    chunker_version: &str,
    dimension: u32,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(channel.as_bytes());
    hasher.update(b"\n");
    hasher.update(vector_space_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(physical_fingerprint.as_bytes());
    hasher.update(b"\n");
    hasher.update(preprocessing_version.as_bytes());
    hasher.update(b"\n");
    hasher.update(chunker_version.as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("vce_{channel}_{}_d{dimension}", &digest[..16])
}

/// Builds one generation-scoped physical vector-row identity.
/// 构建一个代次作用域的物理向量行身份。
fn vector_row_id(
    namespace_id: &str,
    physical_fingerprint: &str,
    // Exact attempt prevents retries from overwriting another attempt's physical row.
    // 精确尝试防止重试覆盖另一尝试的物理行。
    refresh_id: &str,
    generation: u64,
    logical_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(namespace_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(physical_fingerprint.as_bytes());
    hasher.update(b"\n");
    hasher.update(refresh_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(generation.to_string().as_bytes());
    hasher.update(b"\n");
    hasher.update(logical_id.as_bytes());
    format!("vce-vector:{}", hex::encode(hasher.finalize()))
}

/// Builds one deterministic idempotency identity for a vector cleanup request.
/// 为向量清理请求构建一个确定性的幂等身份。
fn vector_cleanup_id(namespace_id: &str, cleanup_kind: &str, seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(namespace_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(cleanup_kind.as_bytes());
    hasher.update(b"\n");
    hasher.update(seed.as_bytes());
    format!("vce-cleanup:{}", hex::encode(hasher.finalize()))
}

/// Returns the fixed LanceDB columns used by content and path vector tables.
/// 返回内容与路径向量表使用的固定 LanceDB 列。
fn vector_table_columns(dimension: u32) -> Vec<LanceDbColumnDef> {
    vec![
        LanceDbColumnDef {
            name: String::from("row_id"),
            column_type: LanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("id"),
            column_type: LanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("blob_name"),
            column_type: LanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("namespace_id"),
            column_type: LanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("refresh_id"),
            column_type: LanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("generation"),
            column_type: LanceDbColumnType::Uint64,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("vector_space_id"),
            column_type: LanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("physical_fingerprint"),
            column_type: LanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("path"),
            column_type: LanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("start_line"),
            column_type: LanceDbColumnType::Uint64,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("end_line"),
            column_type: LanceDbColumnType::Uint64,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("content_hash"),
            column_type: LanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("written_at_unix_ms"),
            column_type: LanceDbColumnType::Int64,
            vector_dim: 0,
            nullable: false,
        },
        LanceDbColumnDef {
            name: String::from("vector"),
            column_type: LanceDbColumnType::VectorFloat32,
            vector_dim: dimension,
            nullable: false,
        },
    ]
}

/// Returns one bootstrap-scoped host binding suffix.
/// 返回一个单次引导作用域的宿主绑定后缀。
fn build_binding_suffix() -> String {
    let sequence = VCE_BINDING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{sequence}", std::process::id())
}

/// Converts one value into a SQLite text parameter.
/// 将一个值转换为 SQLite 文本参数。
fn sqlite_text(value: impl Into<String>) -> SqliteValue {
    SqliteValue::Text(value.into())
}

/// Converts one optional value into a nullable SQLite text parameter.
/// 将一个可选值转换为可空 SQLite 文本参数。
fn sqlite_optional_text(value: Option<&str>) -> SqliteValue {
    value.map(sqlite_text).unwrap_or(SqliteValue::Null)
}

/// Converts one u64 into a checked SQLite integer parameter.
/// 将一个 u64 转换为经过校验的 SQLite 整数参数。
fn sqlite_u64(value: u64, label: &str) -> Result<SqliteValue, VcePortError> {
    sqlite_u64_raw(value, label).map_err(|message| port_rejected(VcePortOperation::Stage, message))
}

/// Converts one u64 into a checked raw SQLite integer parameter.
/// 将一个 u64 转换为经过校验的原始 SQLite 整数参数。
fn sqlite_u64_raw(value: u64, label: &str) -> Result<SqliteValue, String> {
    i64::try_from(value)
        .map(SqliteValue::Integer)
        .map_err(|_| format!("{label} exceeds SQLite integer range"))
}

/// Converts one usize into a checked raw SQLite integer parameter.
/// 将一个 usize 转换为经过校验的原始 SQLite 整数参数。
fn sqlite_usize_raw(value: usize, label: &str) -> Result<SqliteValue, String> {
    i64::try_from(value)
        .map(SqliteValue::Integer)
        .map_err(|_| format!("{label} exceeds SQLite integer range"))
}

/// Converts one nonnegative SQLite integer into u64.
/// 将一个非负 SQLite 整数转换为 u64。
fn nonnegative_u64(value: i64, label: &str) -> Result<u64, VcePortError> {
    u64::try_from(value).map_err(|_| {
        port_invalid(
            VcePortOperation::ManagementQuery,
            format!("{label} is negative"),
        )
    })
}

/// Converts one positive SQLite integer into usize.
/// 将一个正 SQLite 整数转换为 usize。
fn positive_usize(value: i64, label: &str) -> Result<usize, VcePortError> {
    if value <= 0 {
        return Err(port_invalid(
            VcePortOperation::ManagementQuery,
            format!("{label} must be positive"),
        ));
    }
    usize::try_from(value).map_err(|_| {
        port_invalid(
            VcePortOperation::ManagementQuery,
            format!("{label} exceeds platform limits"),
        )
    })
}

/// Converts one nonnegative SQLite integer into usize.
/// 将一个非负 SQLite 整数转换为 usize。
fn nonnegative_usize(value: i64, label: &str) -> Result<usize, VcePortError> {
    usize::try_from(value).map_err(|_| {
        port_invalid(
            VcePortOperation::ManagementQuery,
            format!("{label} is negative or exceeds platform limits"),
        )
    })
}

/// Builds one comma-separated SQLite placeholder sequence.
/// 构建一个逗号分隔的 SQLite 占位符序列。
fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Escapes one trusted value for a generated SQLite transaction literal.
/// 为生成的 SQLite 事务字面量转义一个可信值。
fn sqlite_string_literal(value: &str) -> String {
    value.replace('\'', "''")
}

/// Builds one escaped SQLite string set in deterministic order.
/// 按确定性顺序构建一个已转义 SQLite 字符串集合。
fn sql_string_set(values: &BTreeSet<String>) -> String {
    values
        .iter()
        .map(|value| format!("'{}'", sqlite_string_literal(value)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Escapes one value for a LanceDB SQL predicate.
/// 为 LanceDB SQL 谓词转义一个值。
fn lance_string_literal(value: &str) -> String {
    value.replace('\'', "''")
}

/// Builds one deterministic LanceDB string set.
/// 构建一个确定性的 LanceDB 字符串集合。
fn lance_string_set(values: &BTreeSet<String>) -> String {
    values
        .iter()
        .map(|value| format!("'{}'", lance_string_literal(value)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Returns current Unix epoch milliseconds as a checked SQLite integer.
/// 返回经过校验的当前 Unix 纪元毫秒整数。
fn current_unix_epoch_millis_i64() -> Result<i64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Failed to read the system time: {error}"))?
        .as_millis();
    i64::try_from(millis)
        .map_err(|_| String::from("The current time exceeds the SQLite integer range"))
}

/// Hash every semantic commit field into one durable idempotency identity.
/// 将每个语义提交字段散列为一个持久幂等身份。
///
/// # Parameters
/// 参数
/// `request` is the fully validated atomic publication request.
/// `request` 是已经完整校验的原子发布请求。
///
/// # Returns
/// 返回值
/// A lowercase SHA-256 digest, or a port failure when JSON encoding is impossible.
/// 返回小写 SHA-256 摘要；JSON 编码不可完成时返回端口失败。
fn commit_request_fingerprint(request: &IndexCommit) -> Result<String, VcePortError> {
    let payload = serde_json::json!({
        "request_id": &request.request_id,
        "generation": request.generation,
        "refresh_id": &request.refresh_id,
        "source_revision": &request.source_revision,
        "deleted_blobs": &request.deleted_blobs,
        "staged_blob_names": &request.staged_blob_names,
        "expected_checkpoint_version": request.expected_checkpoint_version,
        "checkpoint": &request.checkpoint,
        "state_snapshot": &request.state_snapshot,
        "finalize_embedding_manifest": request.finalize_embedding_manifest,
    });
    let encoded = serde_json::to_vec(&payload)
        .map_err(|error| port_failed(VcePortOperation::Commit, error.to_string()))?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

/// Reconstruct the deterministic receipt for a new or exactly replayed commit.
/// 为新提交或精确重放的提交重建确定性回执。
///
/// # Parameters
/// 参数
/// `request` contains the exact accepted membership and checkpoint identity.
/// `request` 包含精确接受的成员关系与检查点身份。
///
/// # Returns
/// 返回值
/// The stable receipt implied entirely by the accepted request.
/// 返回完全由已接受请求决定的稳定回执。
fn commit_receipt_from_request(request: IndexCommit) -> CommitReceipt {
    CommitReceipt {
        operation_id: request.request_id,
        refresh_id: request.refresh_id,
        checkpoint_chain_id: request.checkpoint.chain_id,
        generation: request.generation,
        checkpoint_version: request.checkpoint.version,
        indexed_count: request.staged_blob_names.len(),
        deleted_count: request.deleted_blobs.len(),
        indexed_blob_names: request.staged_blob_names.into_iter().collect(),
        deleted_blob_names: request.deleted_blobs.into_iter().collect(),
    }
}

/// Validate the exact checkpoint membership transition represented by one commit.
/// 校验一次提交所表示的精确检查点成员转换。
///
/// # Parameters
/// 参数
/// `current_members` is the authoritative membership of the current committed checkpoint.
/// `current_members` 是当前已提交检查点的权威成员集合。
/// `deleted_blobs` contains identities that must already exist and disappear from the next view.
/// `deleted_blobs` 包含必须已存在且需从下一视图消失的身份。
/// `staged_blobs` contains the complete validated staging membership for this refresh.
/// `staged_blobs` 包含本次刷新的完整已校验暂存成员集合。
/// `next_members` is the checkpoint membership requested for publication.
/// `next_members` 是请求发布的检查点成员集合。
///
/// # Returns
/// 返回值
/// Success only when `next = (current - deleted) union staged` exactly.
/// 仅当 `next = (current - deleted) union staged` 精确成立时返回成功。
fn validate_checkpoint_membership_transition(
    current_members: &BTreeSet<String>,
    deleted_blobs: &BTreeSet<String>,
    staged_blobs: &BTreeSet<String>,
    next_members: &BTreeSet<String>,
) -> Result<(), &'static str> {
    if !deleted_blobs.is_subset(current_members) {
        return Err("deleted Blob membership is not a subset of the current checkpoint");
    }
    // expectedMembers derives the only publishable next view from authoritative old and staged data.
    // expectedMembers 从权威旧数据与暂存数据推导唯一可发布的下一视图。
    let mut expected_members = current_members.clone();
    for blob_name in deleted_blobs {
        expected_members.remove(blob_name);
    }
    expected_members.extend(staged_blobs.iter().cloned());
    if expected_members != *next_members {
        return Err("checkpoint membership does not equal current minus deleted plus staged Blobs");
    }
    Ok(())
}

#[cfg(test)]
#[path = "vce_data_plane_performance_tests.rs"]
mod performance_tests;

#[cfg(test)]
mod tests;

impl fmt::Debug for VceDataPlaneHandle {
    /// Formats non-sensitive host binding metadata.
    /// 格式化不含敏感信息的宿主绑定元数据。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VceDataPlaneHandle")
            .field("data_root", &self.data_root)
            .field("sqlite_path", &self.sqlite_path)
            .field("lancedb_path", &self.lancedb_path)
            .field("sqlite_binding_id", &self.sqlite_binding_id)
            .field("lancedb_binding_id", &self.lancedb_binding_id)
            .finish_non_exhaustive()
    }
}

/// Exact host-owned identity and vector compatibility for one project index.
/// 一个项目索引使用的精确宿主持有身份与向量兼容性。
#[derive(Clone, Debug, serde::Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VceProjectIndexConfig {
    /// Stable VulcanCode project identity.
    /// 稳定的 VulcanCode 项目身份。
    pub project_id: String,
    /// Canonical absolute project root.
    /// 规范绝对项目根目录。
    pub root_path: String,
    /// SHA-256 of the canonical absolute project root.
    /// 规范绝对项目根目录的 SHA-256。
    pub root_hash: String,
    /// Immutable VCE namespace binding.
    /// 不可变的 VCE 命名空间绑定。
    pub binding: VceIndexBinding,
    /// Consumer-owned logical vector-space identity.
    /// 消费者持有的逻辑向量空间身份。
    pub vector_space_id: String,
    /// Exact physical Embedding fingerprint supplied by HostAI.
    /// HostAI 提供的精确物理向量指纹。
    pub physical_fingerprint: String,
    /// Exact preprocessing contract version used before Embedding.
    /// Embedding 前使用的精确预处理契约版本。
    pub preprocessing_version: String,
    /// Exact semantic chunker contract version.
    /// 精确语义切块器契约版本。
    pub chunker_version: String,
    /// Exact vector dimension accepted by both physical tables.
    /// 两张物理表共同接受的精确向量维度。
    pub dimension: u32,
    /// Optimistic project configuration revision owned by the host.
    /// 由宿主持有的乐观项目配置修订号。
    pub config_revision: u64,
}

impl VceProjectIndexConfig {
    /// Validates all project, namespace, and vector identities before storage opens.
    /// 在打开存储前校验全部项目、命名空间与向量身份。
    pub fn validate(&self) -> Result<(), String> {
        let computed_root_hash = hex::encode(Sha256::digest(self.root_path.as_bytes()));
        if self.project_id.trim().is_empty()
            || !Path::new(&self.root_path).is_absolute()
            || self.root_hash.len() != 64
            || self.root_hash != computed_root_hash
            || self.binding.namespace_id.trim().is_empty()
            || self.binding.checkpoint_chain_id.trim().is_empty()
            || self.binding.schema_version == 0
            || self.vector_space_id.trim().is_empty()
            || self.physical_fingerprint.trim().is_empty()
            || self.preprocessing_version.trim().is_empty()
            || self.chunker_version.trim().is_empty()
            || self.dimension == 0
            || self.config_revision == 0
        {
            return Err(String::from(
                "The VCE project index configuration contains an empty identity or invalid dimension",
            ));
        }
        Ok(())
    }
}

/// Exact predecessor and host job authorizing a separately allocated rebuild candidate.
/// 授权独立分配重建候选的精确前驱及宿主作业。
#[derive(Clone, Debug, serde::Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VceRebuildCandidateBase {
    /// Active namespace observed when the human rebuild was admitted.
    /// 人为重建准入时观察到的活动命名空间。
    pub namespace_id: String,
    /// Exact predecessor checkpoint; zero or absent checkpoints cannot authorize replacement.
    /// 精确前驱检查点；零值或缺席检查点不能授权替换。
    pub checkpoint_version: u64,
    /// Durable host job owning candidate allocation and later explicit recovery.
    /// 持有候选分配及后续明确恢复的持久宿主作业。
    pub job_id: String,
}

/// One project-scoped implementation of the VCE index port.
/// 一个项目作用域的 VCE 索引端口实现。
#[derive(Clone)]
pub struct VceProjectIndex {
    /// Shared host-owned data plane.
    /// 共享的宿主数据面。
    data_plane: VceDataPlaneHandle,
    /// Stable VulcanCode project identity.
    /// 稳定的 VulcanCode 项目身份。
    project_id: String,
    /// Canonical absolute project root.
    /// 规范绝对项目根目录。
    root_path: String,
    /// SHA-256 identity of the canonical project root.
    /// 规范项目根目录的 SHA-256 身份。
    root_hash: String,
    /// Immutable VCE namespace binding returned to the engine.
    /// 返回给引擎的不可变 VCE 命名空间绑定。
    binding: VceIndexBinding,
    /// Logical vector-space identity stored with the project binding.
    /// 与项目绑定一同保存的逻辑向量空间身份。
    vector_space_id: String,
    /// Exact physical Embedding fingerprint.
    /// 精确物理向量指纹。
    physical_fingerprint: String,
    /// Optional host-approved producer contract; the original binding and table identity never change.
    /// 可选宿主批准的生产来源契约；原始绑定和表身份永不改变。
    embedding_execution: Option<vulcan_context_engine::VceEmbeddingExecutionContract>,
    /// Exact preprocessing contract version.
    /// 精确预处理契约版本。
    preprocessing_version: String,
    /// Exact chunker contract version.
    /// 精确切块器契约版本。
    chunker_version: String,
    /// Exact vector dimension.
    /// 精确向量维度。
    dimension: u32,
    /// Project configuration revision represented by this index handle.
    /// 此索引句柄表示的项目配置修订号。
    config_revision: u64,
    /// Physical LanceDB table containing content vectors.
    /// 保存内容向量的物理 LanceDB 表。
    content_table: String,
    /// Physical LanceDB table containing path vectors.
    /// 保存路径向量的物理 LanceDB 表。
    path_table: String,
    /// Process-local serialization for stage, abort, and commit.
    /// 对暂存、中止和提交进行进程内串行化的锁。
    mutation_lock: Arc<Mutex<()>>,
    /// Optional immutable read capability; absent only on an unpinned writer/management handle.
    /// 可选不可变读取能力；仅未固定的写入／管理句柄为空。
    read_view: Option<Arc<read_generations::VceReadLease>>,
    /// Exact host-observed active refresh excluded only from this management read's orphan diagnostics.
    /// 仅从本次管理读取孤立诊断排除的、宿主已观察到的精确活动刷新。
    management_active_refresh: Option<String>,
    /// Exact durable candidate authority, admitted explicitly rather than by opening an inactive binding.
    /// 精确持久候选权限，通过显式准入获取，而非打开非活动绑定即可获取。
    rebuild_candidate: Option<VceRebuildCandidateBase>,
}

/// Fully encoded and size-validated LanceDB writes for one physical vector table.
/// 一张物理向量表已经完整编码并通过尺寸校验的 LanceDB 写入批次。
#[derive(Debug)]
struct PreparedVectorRows {
    /// Physical table receiving every encoded batch.
    /// 接收全部编码批次的物理表。
    table_name: String,
    /// JSON row arrays whose encoded payloads fit the in-process engine limit.
    /// 编码载荷均不超过进程内引擎上限的 JSON 行数组。
    batches: Vec<Vec<u8>>,
}

/// Encode every logical row before returning byte-bounded JSON-array batches for one table.
/// 在返回一张表的字节有界 JSON 数组批次前编码全部逻辑行。
///
/// `table_name` identifies the diagnostic target, `rows` is the complete stage-local row set, and
/// `max_data_bytes` is the exact data-field capacity declared by the host binding. Returns only
/// complete JSON arrays; an oversized individual row rejects the entire preparation before writes.
/// `table_name` 标识诊断目标，`rows` 是完整阶段内行集合，`max_data_bytes` 是宿主绑定声明的精确
/// 数据字段容量。仅返回完整 JSON 数组；任一单行过大都会在写入前拒绝整个准备过程。
fn partition_vector_rows_by_data_limit(
    table_name: &str,
    rows: Vec<serde_json::Value>,
    max_data_bytes: usize,
) -> Result<PreparedVectorRows, String> {
    if rows.is_empty() {
        return Ok(PreparedVectorRows {
            table_name: table_name.to_string(),
            batches: Vec::new(),
        });
    }
    if max_data_bytes < 2 {
        return Err(format!(
            "The request limit of VCE LanceDB table `{table_name}` cannot hold a JSON array"
        ));
    }

    let encoded_rows = rows
        .into_iter()
        .map(|row| {
            serde_json::to_vec(&row)
                .map_err(|error| format!("Failed to encode VCE LanceDB vector rows: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for encoded in &encoded_rows {
        let standalone_size = encoded.len().saturating_add(2);
        if standalone_size > max_data_bytes {
            return Err(format!(
                "The single-row JSON payload of VCE LanceDB table `{table_name}` is {standalone_size} bytes, exceeding the available limit of {max_data_bytes} bytes"
            ));
        }
    }

    let mut batches = Vec::new();
    let mut current = Vec::with_capacity(max_data_bytes.min(256 * 1024));
    current.push(b'[');
    let mut current_rows = 0_usize;
    for encoded in encoded_rows {
        let separator_size = usize::from(current_rows != 0);
        let projected_size = current
            .len()
            .saturating_add(separator_size)
            .saturating_add(encoded.len())
            .saturating_add(1);
        if current_rows != 0 && projected_size > max_data_bytes {
            current.push(b']');
            batches.push(current);
            current = Vec::with_capacity(max_data_bytes.min(256 * 1024));
            current.push(b'[');
            current_rows = 0;
        }
        if current_rows != 0 {
            current.push(b',');
        }
        current.extend_from_slice(&encoded);
        current_rows += 1;
    }
    if current_rows != 0 {
        current.push(b']');
        batches.push(current);
    }
    Ok(PreparedVectorRows {
        table_name: table_name.to_string(),
        batches,
    })
}

impl fmt::Debug for VceProjectIndex {
    /// Formats one project index without exposing engine internals.
    /// 格式化一个项目索引且不暴露引擎内部对象。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VceProjectIndex")
            .field("project_id", &self.project_id)
            .field("root_path", &self.root_path)
            .field("root_hash", &self.root_hash)
            .field("binding", &self.binding)
            .field("vector_space_id", &self.vector_space_id)
            .field("physical_fingerprint", &self.physical_fingerprint)
            .field("preprocessing_version", &self.preprocessing_version)
            .field("chunker_version", &self.chunker_version)
            .field("dimension", &self.dimension)
            .field("config_revision", &self.config_revision)
            .field("content_table", &self.content_table)
            .field("path_table", &self.path_table)
            .finish_non_exhaustive()
    }
}

/// Prepare the metadata-only VCE factory without opening historical project databases.
/// 准备仅元数据的 VCE 工厂，不打开历史项目数据库。
///
/// `host_runtime` is the existing host registration. Returns the lazy project storage factory.
/// `host_runtime` 是现有宿主注册。返回惰性项目存储工厂。
pub async fn bootstrap_vce_data_plane(
    host_runtime: &HostRuntimeHandle,
) -> Result<VceDataPlaneFactory, String> {
    VceDataPlaneFactory::bootstrap(host_runtime).await
}

/// Prepares exact binding ownership before any engine activation may partially fail.
/// 在引擎激活可能部分失败前准备精确绑定归属。
///
/// `client` reuses host registration; `data_root` and `project_owner` are validated ownership.
/// `create` is explicit job authority. Returns unopened project-owned handles.
/// `client` 复用宿主注册；`data_root` 与 `project_owner` 是已校验归属。
/// `create` 是明确作业授权。返回尚未打开的项目自有句柄。
pub(crate) fn prepare_vce_storage_binding(
    host_runtime: &HostRuntimeHandle,
    data_root: PathBuf,
    project_owner: String,
    create: bool,
) -> Result<VceDataPlaneHandle, String> {
    let local_runtime = host_runtime.local.clone();
    // sqlite_root, lancedb_path and sqlite_path belong to the factory-validated physical root.
    // sqlite_root、lancedb_path 与 sqlite_path 属于工厂校验过的物理根。
    let sqlite_root = data_root.join("sqlite");
    let lancedb_path = data_root.join("lancedb");
    let sqlite_path = sqlite_root.join("vce.sqlite3");
    if create {
        if !sqlite_root.is_dir() || !lancedb_path.is_dir() {
            return Err(String::from(
                "VCE project directories must be allocated by the host factory",
            ));
        }
    } else if !sqlite_path.is_file() || !lancedb_path.is_dir() {
        return Err(String::from(
            "Existing VCE storage is missing its database or vector directory",
        ));
    }

    // binding_suffix prevents collisions between independent host activations.
    // binding_suffix 防止独立宿主激活之间发生冲突。
    let binding_suffix = build_binding_suffix();
    // sqlite_binding_id and lancedb_binding_id identify exact resources even after partial failure.
    // sqlite_binding_id 与 lancedb_binding_id 即使部分失败后也能标识精确资源。
    let sqlite_binding_id = format!("{VCE_SQLITE_BINDING_TAG}-{binding_suffix}");
    let lancedb_binding_id = format!("{VCE_LANCEDB_BINDING_TAG}-{binding_suffix}");
    let sqlite_binding = vce_project_sqlite_binding(sqlite_path.clone());
    Ok(VceDataPlaneHandle {
        project_owner,
        lease: Arc::new(()),
        read_releases: Arc::new(std::sync::Mutex::new(Vec::new())),
        data_root,
        sqlite_path,
        lancedb_path,
        sqlite_binding_id,
        lancedb_binding_id,
        sqlite_binding,
        local_runtime,
        compaction_operations: Arc::new(std::sync::Mutex::new(HashMap::new())),
    })
}

/// Builds one project's exact in-process SQLite binding.
/// 构建某个项目精确的进程内 SQLite 绑定。
///
/// The binding declares only options enforced by the in-process engine. Host-side runtime
/// serialization provides the concurrency boundary instead of exposing a fictitious pool size.
/// 此绑定只声明进程内引擎实际执行的选项。并发边界由宿主运行时串行化提供，不再暴露虚假的连接池大小。
fn vce_project_sqlite_binding(path: PathBuf) -> SqliteFileBinding {
    SqliteFileBinding {
        path,
        options: SqliteOpenOptions {
            busy_timeout_ms: VCE_SQLITE_BUSY_TIMEOUT_MS,
            pragmas: SqlitePragmaOptions {
                cache_size_kib: 8_192,
                mmap_size_bytes: 0,
                ..Default::default()
            },
            hardening: SqliteHardeningOptions {
                read_only: false,
                ..Default::default()
            },
        },
    }
}

impl VceDataPlaneHandle {
    /// Returns the exact physical project owner for cache lifecycle tests.
    /// 为缓存生命周期测试返回精确物理项目所有者。
    ///
    /// Takes no parameters and returns the immutable owner identity.
    /// 无参数，返回不可变所有者身份。
    #[cfg(test)]
    pub(crate) fn project_owner_for_test(&self) -> &str {
        &self.project_owner
    }

    /// Declares the LanceDB limits the VCE vector store runs with.
    /// 声明 VCE 向量存储使用的 LanceDB 限制。
    ///
    /// These are the engine's defaults, which is what the application-level store ran with before the
    /// controller was removed. VCE states them here rather than inheriting silently, because the
    /// subsystem that owns a binding is the place that knows why its limits are what they are.
    ///
    /// 取值采用引擎默认值，也就是移除控制器前应用级存储实际使用的值。VCE 在此显式声明而非静默继承，
    /// 因为拥有绑定的子系统才是知道其限制取值理由的地方。
    pub(crate) fn vector_store_binding(&self) -> LanceDbDirectoryBinding {
        LanceDbDirectoryBinding {
            directory: self.lancedb_path.clone(),
            read_consistency_interval_ms: Some(250),
            max_upsert_payload: 16 * 1024 * 1024,
            max_search_limit: VCE_VECTOR_SEARCH_LIMIT as usize,
            max_concurrent_requests: 16,
        }
    }

    /// Returns the in-process LanceDB engine holding this project's code and path vectors.
    /// 返回保存本项目代码与路径向量的进程内 LanceDB 引擎。
    ///
    /// # Errors
    /// 错误
    /// Returns an explicit error when the vector directory cannot be opened, or when the host runtime
    /// has already shut down.
    /// 向量目录无法打开，或宿主运行时已关停时返回显式错误。
    pub(crate) async fn vector_engine(&self) -> Result<LanceDbEngine, String> {
        self.local_runtime
            .lance_engine(self.vector_store_binding())
            .await
            .map_err(|error| format!("Failed to open the native VCE LanceDB database: {error}"))
    }

    /// Activate bindings only after the factory has retained cleanup ownership.
    /// 仅在工厂保留清理归属后激活绑定。
    ///
    /// `create` permits initialization of a new database. Returns failures without losing handles.
    /// `create` 允许初始化新数据库。返回失败时不丢失句柄。
    pub(crate) async fn activate_bindings(&self, create: bool) -> Result<(), String> {
        // new_database is captured before the engine creates its own internal metadata tables.
        // new_database 在引擎创建自身内部元数据表之前获取。
        let new_database = !self.sqlite_path.exists();
        // The binding registry does not eagerly open this project's SQLite database or hold its lock.
        // The binding above is the whole activation: the engine opens the file on
        // the first statement and keeps it open until `close_bindings` releases it.
        // 没有外部服务会获知本项目的 SQLite 数据库，因此不会存在第二个打开者或锁持有者。上方的绑定就是全部激活：
        // 引擎在第一条语句时打开该文件，并保持打开直到 `close_bindings` 释放它。
        // metadata_count checks ownership before initialization could overwrite a foreign schema.
        // metadata_count 在初始化可能覆盖外部结构之前检查归属。
        let metadata_count: Vec<CountRow> = self.query_rows(
            "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'vce_metadata';",
            Vec::new(),
        ).await?;
        if metadata_count.first().map(|row| row.count) == Some(1) {
            // The stored marker must equal the one v1 value. It selects nothing: an exact match continues,
            // anything else is reported. There is no branch here that repairs, upgrades or reinterprets
            // storage, which is what makes this an assertion rather than a migration entry point.
            // 持久版本标记必须等于唯一的 v1 值。它不选择任何路径：精确匹配则继续，其他一律报出。此处不存在任何
            // 修补、升级或重新解释存储的分支，这正是它是断言而非迁移入口的原因。
            let schema: Vec<StorageSchemaRow> = self
                .query_rows(
                    "SELECT value FROM vce_metadata WHERE key = 'schema_version';",
                    Vec::new(),
                )
                .await?;
            if schema.len() != 1 {
                return Err(String::from(
                    "VCE project storage schema is missing or unsupported",
                ));
            }
            let stored_schema = schema[0]
                .value
                .parse::<i64>()
                .map_err(|_| String::from("VCE project storage schema marker is invalid"))?;
            // One v1 baseline, verified rather than assumed. A database carrying any other marker is
            // rejected by name: this build does not detect pre-v1 layouts, does not migrate them and does
            // not repair them. Reporting the observed value is what makes an unexpected database a loud,
            // diagnosable failure instead of a silent no-op or a wrong-data read.
            // 唯一 v1 基线，经校验而非假定。携带任何其他标记的数据库都会被指名拒绝：本构建不探测 v1 之前的布局、
            // 不迁移它们、也不修补它们。报出观测到的值，正是把一个意外数据库变成响亮且可诊断的失败、而不是静默
            // 无效或读错数据的原因。
            if stored_schema != VCE_DATA_PLANE_SCHEMA_VERSION {
                return Err(format!(
                    "VCE project storage schema is {stored_schema}, but this build only supports the v1 baseline \
                     marker {VCE_DATA_PLANE_SCHEMA_VERSION}; pre-v1 storage is not migrated or repaired"
                ));
            }
        } else if create && new_database {
            // The factory admitted this absent file; engine-internal tables are not VCE content.
            // 工厂准入了此缺失文件；引擎内部表不属于 VCE 内容。
        } else {
            return Err(String::from(
                "VCE existing project storage has no schema marker",
            ));
        }
        if create {
            // schema_result confirms actual initialization, not merely a successful transport response.
            // schema_result 确认实际初始化，而不只是传输响应成功。
            local_sql::execute_script_async(
                &self.local_runtime,
                &self.sqlite_binding,
                build_vce_schema_script(),
                Vec::new(),
            )
            .await
            .map_err(|error| format!("Failed to initialize the VCE data schema: {error}"))?;
        }
        self.reconcile_publication_intents().await?;
        // Reopening also reclaims settled historical work; unknown remote tasks and active jobs stay pinned.
        // 重新打开时也回收已结束历史工作；未知远端任务和活动作业仍被固定保留。
        local_sql::with_connection_async(&self.local_runtime, &self.sqlite_binding, |connection| {
            artifact_retention::collect(connection, "")
        })
        .await
        .map_err(|error| format!("Failed to reclaim expired VCE runtime material: {error}"))?;
        Ok(())
    }

    /// Reconcile publication intents whose receipt and checkpoint are already durable.
    /// 对回执和检查点都已持久化的发布意图执行恢复确认。
    async fn reconcile_publication_intents(&self) -> Result<(), String> {
        let now = current_unix_epoch_millis_i64()?;
        self.execute(
            r#"UPDATE vce_publication_intents
               SET stage = 'reconciled', error = NULL,
                   heartbeat_at_unix_ms = ?, updated_at_unix_ms = ?
               WHERE stage IN ('preparing', 'committed')
                 AND EXISTS (
                     SELECT 1 FROM vce_commit_receipts r
                     WHERE r.namespace_id = vce_publication_intents.namespace_id
                       AND r.operation_id = vce_publication_intents.operation_id
                       AND r.request_fingerprint = vce_publication_intents.request_fingerprint
                 )
                 AND EXISTS (
                     SELECT 1 FROM vce_read_checkpoints c
                     WHERE c.namespace_id = vce_publication_intents.namespace_id
                       AND c.checkpoint_version = vce_publication_intents.checkpoint_version
                       AND c.generation = vce_publication_intents.generation
                       AND c.refresh_id = vce_publication_intents.refresh_id
                 );"#,
            vec![SqliteValue::Integer(now), SqliteValue::Integer(now)],
            "Failed to recover the VCE publication intent state",
        )
        .await
    }
}

impl VceDataPlaneHandle {
    /// Read the active project's original persisted contract without creating or updating anything.
    /// 读取项目当前活动索引的原始持久化契约，不创建或更新任何内容。
    ///
    /// `project_id` is the stable host-owned identity. Returns the original contract when
    /// present, absence for a never-created project, or an explicit malformed-state error.
    /// `project_id` 是宿主持有的稳定身份。存在时返回原始契约，未创建项目返回空值，
    /// 状态格式错误时返回明确错误。
    pub async fn load_active_project_config(
        &self,
        project_id: &str,
    ) -> Result<Option<VceProjectIndexConfig>, String> {
        self.validate_project_owner(project_id)?;
        if project_id.trim().is_empty() {
            return Err(String::from("VCE project identity must not be empty"));
        }
        // rows are restricted to one exact project; multiple active bindings indicate corruption.
        // rows 仅限一个精确项目；多个活动绑定表示数据损坏。
        let rows: Vec<ProjectBindingRow> = self.query_rows(
            "SELECT project_id, root_path, root_hash, namespace_id, checkpoint_chain_id, schema_version, vector_space_id, physical_fingerprint, preprocessing_version, chunker_version, dimension, content_table, path_table, config_revision, active FROM vce_project_bindings WHERE project_id = ? AND active = 1 LIMIT 2;",
            vec![sqlite_text(project_id)],
        ).await?;
        if rows.len() > 1 {
            return Err(String::from(
                "VCE project has multiple active index bindings",
            ));
        }
        rows.first()
            .map(ProjectBindingRow::original_config)
            .transpose()
    }

    /// Read the newest explicitly orphaned binding that has never published a checkpoint.
    /// 读取最新明确弃用且从未发布检查点的绑定。
    ///
    /// This recovery-only view never activates, repairs or scans storage; callers must choose an
    /// explicit rebuild operation before using the returned identity.
    /// The newest fenced orphan is selected because every eligible row is checkpoint-free and remains immutable.
    /// 选择最新封锁的孤立绑定，因为所有候选都没有检查点且保持不可变。
    pub async fn load_uninitialized_recovery_config(
        &self,
        project_id: &str,
    ) -> Result<Option<VceProjectIndexConfig>, String> {
        self.validate_project_owner(project_id)?;
        if project_id.trim().is_empty() {
            return Err(String::from("VCE project identity must not be empty"));
        }
        let rows: Vec<ProjectBindingRow> = self
            .query_rows(
                "SELECT project_id, root_path, root_hash, namespace_id, checkpoint_chain_id, schema_version, vector_space_id, physical_fingerprint, preprocessing_version, chunker_version, dimension, content_table, path_table, config_revision, active FROM vce_project_bindings WHERE project_id = ? AND active = 0 AND lifecycle_state = 'orphaned' AND NOT EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = vce_project_bindings.namespace_id) ORDER BY updated_at_unix_ms DESC, namespace_id DESC LIMIT 1;",
                vec![sqlite_text(project_id)],
            )
            .await?;
        rows.first()
            .map(|row| row.config_for_active_state(false))
            .transpose()
    }

    /// Restore an exact existing index handle without changing bindings, tables or checkpoints.
    /// 恢复一个精确的现有索引句柄，不修改绑定、表或检查点。
    ///
    /// `config` is the desired host contract. Returns absence if the project has no index;
    /// mismatched origin identities are explicit errors and never trigger implicit rebuilding.
    /// `config` 是期望宿主契约。项目无索引时返回空值；来源身份不匹配时明确报错，
    /// 绝不触发隐式重建。
    pub async fn open_existing_project_index(
        &self,
        config: &VceProjectIndexConfig,
    ) -> Result<Option<VceProjectIndex>, String> {
        config.validate()?;
        // original is authoritative for physical provenance and the persisted write guard revision.
        // original 是物理来源与持久化写入闸门修订的权威值。
        let Some(original) = self.load_active_project_config(&config.project_id).await? else {
            return Ok(None);
        };
        if original.root_path != config.root_path
            || original.root_hash != config.root_hash
            || original.binding != config.binding
            || original.vector_space_id != config.vector_space_id
            || original.physical_fingerprint != config.physical_fingerprint
            || original.preprocessing_version != config.preprocessing_version
            || original.chunker_version != config.chunker_version
            || original.dimension != config.dimension
        {
            return Err(String::from(
                "VCE persisted index contract requires explicit compatibility confirmation or rebuilding",
            ));
        }
        // content_table and path_table are derived from the verified original physical contract.
        // content_table 与 path_table 根据已验证的原始物理契约推导。
        let content_table = vector_table_name(
            "content",
            &original.vector_space_id,
            &original.physical_fingerprint,
            &original.preprocessing_version,
            &original.chunker_version,
            original.dimension,
        );
        let path_table = vector_table_name(
            "path",
            &original.vector_space_id,
            &original.physical_fingerprint,
            &original.preprocessing_version,
            &original.chunker_version,
            original.dimension,
        );
        Ok(Some(VceProjectIndex {
            data_plane: self.clone(),
            project_id: original.project_id,
            root_path: original.root_path,
            root_hash: original.root_hash,
            binding: original.binding,
            vector_space_id: original.vector_space_id,
            physical_fingerprint: original.physical_fingerprint,
            embedding_execution: None,
            read_view: None,
            management_active_refresh: None,
            rebuild_candidate: None,
            preprocessing_version: original.preprocessing_version,
            chunker_version: original.chunker_version,
            dimension: original.dimension,
            config_revision: original.config_revision,
            content_table,
            path_table,
            mutation_lock: Arc::new(Mutex::new(())),
        }))
    }

    /// Open one explicitly orphaned uninitialized binding without activating or mutating it.
    /// 打开一个明确孤立的未初始化绑定，不激活也不修改它。
    ///
    /// `config` is the recovery identity selected by an explicit rebuild workflow.
    /// `config` 是显式重建流程选出的恢复身份。
    pub async fn open_uninitialized_recovery_project_index(
        &self,
        config: &VceProjectIndexConfig,
    ) -> Result<Option<VceProjectIndex>, String> {
        config.validate()?;
        let rows: Vec<ProjectBindingRow> = self
            .query_rows(
                "SELECT project_id, root_path, root_hash, namespace_id, checkpoint_chain_id, schema_version, vector_space_id, physical_fingerprint, preprocessing_version, chunker_version, dimension, content_table, path_table, config_revision, active FROM vce_project_bindings WHERE project_id = ? AND namespace_id = ? AND active = 0 AND lifecycle_state = 'orphaned' AND NOT EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = vce_project_bindings.namespace_id) LIMIT 1;",
                vec![
                    sqlite_text(&config.project_id),
                    sqlite_text(&config.binding.namespace_id),
                ],
            )
            .await?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        row.validate_identity(config)?;
        let content_table = vector_table_name(
            "content",
            &config.vector_space_id,
            &config.physical_fingerprint,
            &config.preprocessing_version,
            &config.chunker_version,
            config.dimension,
        );
        let path_table = vector_table_name(
            "path",
            &config.vector_space_id,
            &config.physical_fingerprint,
            &config.preprocessing_version,
            &config.chunker_version,
            config.dimension,
        );
        Ok(Some(VceProjectIndex {
            data_plane: self.clone(),
            project_id: config.project_id.clone(),
            root_path: config.root_path.clone(),
            root_hash: config.root_hash.clone(),
            binding: config.binding.clone(),
            vector_space_id: config.vector_space_id.clone(),
            physical_fingerprint: config.physical_fingerprint.clone(),
            embedding_execution: None,
            read_view: None,
            management_active_refresh: None,
            rebuild_candidate: None,
            preprocessing_version: config.preprocessing_version.clone(),
            chunker_version: config.chunker_version.clone(),
            dimension: config.dimension,
            config_revision: config.config_revision,
            content_table,
            path_table,
            mutation_lock: Arc::new(Mutex::new(())),
        }))
    }

    /// Atomically fence one active uninitialized binding before explicit recovery or replacement.
    /// 在显式恢复或替换前，原子封锁一个活动的未初始化绑定。
    ///
    /// config is the exact active identity; no checkpoint may exist and no data is deleted.
    /// config 是精确的活动身份；不得存在检查点，也不会删除数据。
    pub(crate) async fn retire_uninitialized_active_binding(
        &self,
        config: &VceProjectIndexConfig,
    ) -> Result<(), String> {
        config.validate()?;
        let now = current_unix_epoch_millis_i64()?;
        let project_id = sqlite_string_literal(&config.project_id);
        let namespace_id = sqlite_string_literal(&config.binding.namespace_id);
        let root_path = sqlite_string_literal(&config.root_path);
        let root_hash = sqlite_string_literal(&config.root_hash);
        let fingerprint = sqlite_string_literal(&config.physical_fingerprint);
        let content_table = sqlite_string_literal(&vector_table_name(
            "content",
            &config.vector_space_id,
            &config.physical_fingerprint,
            &config.preprocessing_version,
            &config.chunker_version,
            config.dimension,
        ));
        let path_table = sqlite_string_literal(&vector_table_name(
            "path",
            &config.vector_space_id,
            &config.physical_fingerprint,
            &config.preprocessing_version,
            &config.chunker_version,
            config.dimension,
        ));
        // The guard is deliberately checked inside BEGIN IMMEDIATE so late stage/commit requests
        // 该检查刻意放在 BEGIN IMMEDIATE 内执行，以使迟到的 stage/commit 请求被数据库拒绝。
        let condition = format!(
            "EXISTS(SELECT 1 FROM vce_project_bindings WHERE project_id = '{project_id}' AND namespace_id = '{namespace_id}' AND root_path = '{root_path}' AND root_hash = '{root_hash}' AND physical_fingerprint = '{fingerprint}' AND content_table = '{content_table}' AND path_table = '{path_table}' AND active = 1) AND NOT EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = '{namespace_id}')",
        );
        let script = format!(
            "BEGIN IMMEDIATE;\nINSERT OR ROLLBACK INTO vce_commit_guards(namespace_id, valid) VALUES ('{namespace_id}', CASE WHEN {condition} THEN 1 ELSE 0 END);\nUPDATE vce_project_bindings SET active = 0, lifecycle_state = 'orphaned', updated_at_unix_ms = {now} WHERE project_id = '{project_id}' AND namespace_id = '{namespace_id}' AND active = 1;\nDELETE FROM vce_commit_guards WHERE namespace_id = '{namespace_id}';\nCOMMIT;"
        );
        self.execute(&script, Vec::new(), "Retire uninitialized VCE binding")
            .await
    }

    /// Opens or validates one project-specific VCE index binding.
    /// 打开或校验一个项目专用的 VCE 索引绑定。
    pub async fn open_project_index(
        &self,
        config: VceProjectIndexConfig,
    ) -> Result<VceProjectIndex, String> {
        self.open_project_index_with_candidate(config, None).await
    }

    /// Allocate an inactive rebuild candidate without moving the published project pointer.
    /// 分配非活动重建候选，不移动已发布项目指针。
    /// `config` names a fresh namespace and physical tables; `base` freezes the admitted predecessor/job.
    /// `config` 指定全新命名空间与物理表；`base` 固定准入前驱及作业。
    /// A matching queued/running replacement receipt must already exist; configuration alone grants no allocation.
    /// 必须已经存在匹配的排队／运行中替换回执；仅有配置不授予分配权限。
    /// Returns an inactive handle; ordinary index-port operations cannot publish or search this candidate.
    /// 返回非活动句柄；普通索引端口操作不能发布或查询此候选。
    pub async fn prepare_rebuild_candidate(
        &self,
        config: VceProjectIndexConfig,
        base: VceRebuildCandidateBase,
    ) -> Result<VceProjectIndex, String> {
        self.open_project_index_with_candidate(config, Some(base))
            .await
    }

    /// Reopen an exact unpublished candidate without creating tables, admitting writes or taking over its job.
    /// 重新打开精确未发布候选，不创建表、不准入写入，也不接管所属作业。
    /// `config` and `base` must match durable allocation. Returns an unadmitted handle for explicit recovery.
    /// `config` 与 `base` 必须匹配持久分配。返回供显式恢复使用的未准入句柄。
    pub async fn reopen_rebuild_candidate(
        &self,
        config: &VceProjectIndexConfig,
        base: &VceRebuildCandidateBase,
    ) -> Result<VceProjectIndex, String> {
        self.find_rebuild_candidate(config, base)
            .await?
            .ok_or_else(|| {
                String::from("VCE unpublished candidate does not match the exact owning job")
            })
    }

    /// Find the exact owned candidate, including interrupted physical allocation, without creating or admitting it.
    /// 查找精确所属候选，包括中断的物理分配，不创建或准入候选。
    /// `config` and `base` identify one durable allocation; returns absence only when that exact allocation is missing.
    /// `config` 与 `base` 标识一次持久分配；仅在该精确分配缺失时返回无值。
    pub async fn find_rebuild_candidate(
        &self,
        config: &VceProjectIndexConfig,
        base: &VceRebuildCandidateBase,
    ) -> Result<Option<VceProjectIndex>, String> {
        self.validate_project_owner(&config.project_id)?;
        config.validate()?;
        // rows identify the exact job even when its predecessor has advanced and only cancellation remains valid.
        // rows 即使在前驱已推进、只允许取消时仍按精确作业定位。
        let rows: Vec<ProjectBindingRow> = self.query_rows(
            "SELECT b.project_id, b.root_path, b.root_hash, b.namespace_id, b.checkpoint_chain_id, b.schema_version, b.vector_space_id, b.physical_fingerprint, b.preprocessing_version, b.chunker_version, b.dimension, b.content_table, b.path_table, b.config_revision, b.active FROM vce_project_bindings b JOIN vce_rebuild_candidates c ON c.namespace_id = b.namespace_id WHERE b.namespace_id = ? AND b.project_id = ? AND b.active = 0 AND c.base_namespace_id = ? AND c.base_checkpoint_version = ? AND c.job_id = ? AND c.state IN ('allocating', 'building', 'aborted') AND NOT EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = b.namespace_id);",
            vec![sqlite_text(&config.binding.namespace_id), sqlite_text(&config.project_id), sqlite_text(&base.namespace_id), sqlite_u64_raw(base.checkpoint_version, "VCE candidate predecessor checkpoint")?, sqlite_text(&base.job_id)],
        ).await?;
        if rows.is_empty() {
            return Ok(None);
        }
        let [row] = rows.as_slice() else {
            return Err(String::from(
                "VCE candidate lookup returned multiple owning bindings",
            ));
        };
        if row.config_for_active_state(false)? != *config {
            return Err(String::from(
                "VCE candidate recovery configuration differs from its persisted allocation",
            ));
        }
        Ok(Some(VceProjectIndex {
            data_plane: self.clone(),
            project_id: config.project_id.clone(),
            root_path: config.root_path.clone(),
            root_hash: config.root_hash.clone(),
            binding: config.binding.clone(),
            vector_space_id: config.vector_space_id.clone(),
            physical_fingerprint: config.physical_fingerprint.clone(),
            embedding_execution: None,
            preprocessing_version: config.preprocessing_version.clone(),
            chunker_version: config.chunker_version.clone(),
            dimension: config.dimension,
            config_revision: config.config_revision,
            content_table: row.content_table.clone(),
            path_table: row.path_table.clone(),
            mutation_lock: Arc::new(Mutex::new(())),
            read_view: None,
            management_active_refresh: None,
            rebuild_candidate: None,
        }))
    }

    /// Share validated table allocation while keeping initialization and rebuild publication separate.
    /// 共享已验证表分配，同时保持初始化与重建发布分离。
    /// `candidate` is explicit rebuild authority, otherwise the existing initialization path applies.
    /// `candidate` 是明确重建授权，否则使用已有初始化路径。
    /// Returns the exact allocated handle or an explicit identity/allocation error.
    /// 返回精确分配句柄或明确身份／分配错误。
    async fn open_project_index_with_candidate(
        &self,
        config: VceProjectIndexConfig,
        candidate: Option<VceRebuildCandidateBase>,
    ) -> Result<VceProjectIndex, String> {
        self.validate_project_owner(&config.project_id)?;
        config.validate()?;
        // base_condition is reused inside the final SQL transaction to reject a changed predecessor.
        // base_condition 在最终 SQL 事务内复用，拒绝已经变化的前驱。
        let base_condition = if let Some(base) = &candidate {
            if base.namespace_id.trim().is_empty()
                || base.job_id.trim().is_empty()
                || base.checkpoint_version == 0
                || base.namespace_id == config.binding.namespace_id
            {
                return Err(String::from(
                    "VCE rebuild candidate requires an exact distinct predecessor and host job",
                ));
            }
            sqlite_u64_raw(
                base.checkpoint_version,
                "VCE rebuild predecessor checkpoint",
            )?;
            // job must already acknowledge this exact physical target before any vector table is allocated.
            // 分配任何向量表之前，job 必须已经确认此精确物理目标。
            let job = VceProjectJobStore::new(self.clone(), config.project_id.clone())
                .get(&base.job_id)
                .await?
                .ok_or_else(|| {
                    String::from("VCE candidate allocation requires its durable replacement job")
                })?;
            if !matches!(job.state, VceJobState::Queued | VceJobState::Running)
                || job.replacement.as_deref()
                    != Some(&VceJobReplacement {
                        predecessor: base.clone(),
                        target: config.clone(),
                    })
            {
                return Err(String::from(
                    "VCE candidate allocation differs from its admitted replacement receipt",
                ));
            }
            // job_snapshot makes cancellation or receipt drift during physical allocation revoke final binding publication.
            // job_snapshot 使物理分配期间的取消或回执漂移撤销最终绑定发布。
            let job_snapshot = serde_json::to_string(&job)
                .map_err(|error| format!("Encode VCE candidate receipt guard: {error}"))?;
            // condition also forbids changing project/root ownership under a vector-space rebuild.
            // condition 还禁止借向量空间重建改变项目／根目录归属。
            let condition = format!(
                "EXISTS(SELECT 1 FROM vce_project_bindings b JOIN vce_active_checkpoints c ON c.namespace_id = b.namespace_id WHERE b.namespace_id = '{}' AND b.project_id = '{}' AND b.root_path = '{}' AND b.root_hash = '{}' AND b.active = 1 AND c.checkpoint_version = {}) AND EXISTS(SELECT 1 FROM vce_host_jobs j WHERE j.project_id = '{}' AND j.job_id = '{}' AND j.snapshot_json = '{}')",
                sqlite_string_literal(&base.namespace_id),
                sqlite_string_literal(&config.project_id),
                sqlite_string_literal(&config.root_path),
                sqlite_string_literal(&config.root_hash),
                base.checkpoint_version,
                sqlite_string_literal(&config.project_id),
                sqlite_string_literal(&base.job_id),
                sqlite_string_literal(&job_snapshot)
            );
            // matches prevents avoidable physical allocation before the transactional recheck.
            // matches 在事务复查之前避免可预知的无效物理分配。
            let matches: Vec<CountRow> = self
                .query_rows(&format!("SELECT {condition} AS count;"), Vec::new())
                .await?;
            if !matches!(matches.as_slice(), [row] if row.count == 1) {
                return Err(String::from(
                    "VCE rebuild predecessor no longer matches the admitted project checkpoint",
                ));
            }
            Some(condition)
        } else {
            None
        };
        let config_revision = i64::try_from(config.config_revision)
            .map_err(|_| String::from("VCE config revision exceeds SQLite INTEGER range"))?;
        let content_table = vector_table_name(
            "content",
            &config.vector_space_id,
            &config.physical_fingerprint,
            &config.preprocessing_version,
            &config.chunker_version,
            config.dimension,
        );
        let path_table = vector_table_name(
            "path",
            &config.vector_space_id,
            &config.physical_fingerprint,
            &config.preprocessing_version,
            &config.chunker_version,
            config.dimension,
        );
        let rows: Vec<ProjectBindingRow> = self
            .query_rows(
                "SELECT project_id, root_path, root_hash, namespace_id, checkpoint_chain_id, schema_version, vector_space_id, physical_fingerprint, preprocessing_version, chunker_version, dimension, content_table, path_table, config_revision, active FROM vce_project_bindings WHERE namespace_id = ? LIMIT 1;",
                vec![sqlite_text(&config.binding.namespace_id)],
            )
            .await?;
        if let Some(row) = rows.first() {
            // Neither repeated preparation nor ordinary opening may publish an unfinished or cancelled candidate.
            // 重复准备与普通打开都不能发布未完成或已取消候选。
            let pending: Vec<CountRow> = self.query_rows(
                "SELECT COUNT(*) AS count FROM vce_rebuild_candidates WHERE namespace_id = ? AND state != 'published';",
                vec![sqlite_text(&config.binding.namespace_id)],
            ).await?;
            if candidate.is_some() || pending.first().is_some_and(|row| row.count != 0) {
                return Err(String::from(
                    "VCE rebuild candidate already exists; use its exact owning job for recovery or publication",
                ));
            }
            row.validate_identity(&config)?;
            if config_revision < row.config_revision {
                return Err(format!(
                    "VCE project `{}` requested stale config revision {} below persisted revision {}",
                    config.project_id, config_revision, row.config_revision
                ));
            }
            let now = current_unix_epoch_millis_i64()?;
            if !row.vector_compatibility_matches(&config, &content_table, &path_table) {
                return Err(String::from(
                    "VCE physical index identity is immutable; explicit rebuild must use a distinct namespace",
                ));
            } else if row.active != 1 || config_revision != row.config_revision {
                let project_id = sqlite_string_literal(&config.project_id);
                let namespace_id = sqlite_string_literal(&config.binding.namespace_id);
                let script = format!(
                    r#"BEGIN IMMEDIATE;
                       UPDATE vce_project_bindings
                       SET active = 0, lifecycle_state = 'orphaned', updated_at_unix_ms = {now}
                       WHERE project_id = '{project_id}' AND namespace_id <> '{namespace_id}' AND active = 1;
                       UPDATE vce_project_bindings
                       SET active = 1, config_revision = {config_revision},
                           lifecycle_state = CASE
                               WHEN EXISTS(SELECT 1 FROM vce_checkpoints WHERE namespace_id = '{namespace_id}') THEN 'ready'
                               ELSE 'rebuild_required'
                           END,
                           updated_at_unix_ms = {now}
                       WHERE namespace_id = '{namespace_id}';
                       COMMIT;"#,
                );
                self.execute(
                    &script,
                    Vec::new(),
                    "Failed to activate the VCE project index",
                )
                .await?;
            }
        } else {
            // Only a new admitted binding creates tables; existing bindings never recreate missing data.
            // 仅新准入绑定创建表；现有绑定绝不通过重新建表掩盖数据缺失。
            let now = current_unix_epoch_millis_i64()?;
            let namespace_id = sqlite_string_literal(&config.binding.namespace_id);
            let project_id = sqlite_string_literal(&config.project_id);
            let root_path = sqlite_string_literal(&config.root_path);
            let root_hash = sqlite_string_literal(&config.root_hash);
            let checkpoint_chain_id = sqlite_string_literal(&config.binding.checkpoint_chain_id);
            let vector_space_id = sqlite_string_literal(&config.vector_space_id);
            let physical_fingerprint = sqlite_string_literal(&config.physical_fingerprint);
            let preprocessing_version = sqlite_string_literal(&config.preprocessing_version);
            let chunker_version = sqlite_string_literal(&config.chunker_version);
            let content_table_sql = sqlite_string_literal(&content_table);
            let path_table_sql = sqlite_string_literal(&path_table);
            let schema_version = config.binding.schema_version;
            let dimension = config.dimension;
            // activation changes the published pointer only for the existing initialization path.
            // activation 仅在已有初始化路径中改变已发布指针。
            let activation = if candidate.is_none() {
                format!(
                    "UPDATE vce_project_bindings SET active = 0, lifecycle_state = 'orphaned', updated_at_unix_ms = {now} WHERE project_id = '{project_id}' AND active = 1;"
                )
            } else {
                String::new()
            };
            // candidate_record durably binds the allocation to its exact base and host job.
            // candidate_record 将分配持久绑定到精确基准与宿主作业。
            let candidate_record = match (&candidate, &base_condition) {
                (Some(base), Some(condition)) => format!(
                    "INSERT OR ROLLBACK INTO vce_commit_guards(namespace_id, valid) VALUES ('{namespace_id}', CASE WHEN {condition} THEN 1 ELSE 0 END);\nINSERT OR ROLLBACK INTO vce_rebuild_candidates(namespace_id, base_namespace_id, base_checkpoint_version, job_id, state, created_at_unix_ms, updated_at_unix_ms) VALUES ('{namespace_id}', '{}', {}, '{}', 'allocating', {now}, {now});\nDELETE FROM vce_commit_guards WHERE namespace_id = '{namespace_id}';",
                    sqlite_string_literal(&base.namespace_id),
                    base.checkpoint_version,
                    sqlite_string_literal(&base.job_id)
                ),
                (None, None) => String::new(),
                _ => {
                    return Err(String::from(
                        "VCE candidate authority preparation is inconsistent",
                    ));
                }
            };
            // active is false for every candidate until a separate atomic publication succeeds.
            // active 对每个候选均为假，直至独立原子发布成功。
            let active = i32::from(candidate.is_none());
            // Initial tables have independent durable ownership before the binding can become visible.
            // 初始表在绑定变得可见之前具有独立持久归属。
            let initial_allocation = if candidate.is_none() {
                Some(
                    self.allocate_initial_tables(&config, &content_table, &path_table)
                        .await?,
                )
            } else {
                None
            };
            // The receipt transitions together with the binding, never in a later acknowledgement write.
            // 回执与绑定同时转换，绝不在后续确认写入中转换。
            let initial_publication = initial_allocation
                .as_ref()
                .map(initial_allocation::InitialAllocation::publication_sql)
                .unwrap_or_default();
            let script = format!(
                r#"BEGIN IMMEDIATE;
                   {activation}
                   INSERT OR ROLLBACK INTO vce_project_bindings(
                       namespace_id, project_id, root_path, root_hash, checkpoint_chain_id, schema_version,
                       vector_space_id, physical_fingerprint, preprocessing_version,
                       chunker_version, dimension, content_table, path_table, config_revision,
                       active, lifecycle_state, created_at_unix_ms, updated_at_unix_ms
                   ) VALUES ('{namespace_id}', '{project_id}', '{root_path}', '{root_hash}',
                       '{checkpoint_chain_id}', {schema_version}, '{vector_space_id}',
                       '{physical_fingerprint}', '{preprocessing_version}', '{chunker_version}',
                       {dimension}, '{content_table_sql}', '{path_table_sql}', {config_revision},
                       {active}, 'rebuild_required', {now}, {now});
                   INSERT OR ROLLBACK INTO vce_embedding_spaces(
                       namespace_id, physical_fingerprint, preprocessing_version,
                       chunker_version, dimension, content_table, path_table,
                       lifecycle_state, created_at_unix_ms, updated_at_unix_ms
                   ) VALUES ('{namespace_id}', '{physical_fingerprint}', '{preprocessing_version}',
                       '{chunker_version}', {dimension}, '{content_table_sql}', '{path_table_sql}',
                       'active', {now}, {now});
                   {candidate_record}
                   {initial_publication}
                   COMMIT;"#,
            );
            if let Err(primary) = self
                .execute(
                    &script,
                    Vec::new(),
                    "Failed to register the VCE project index",
                )
                .await
            {
                return Err(match &initial_allocation {
                    Some(allocation) => self.fail_initial_allocation(allocation, primary).await,
                    None => primary,
                });
            }
            if let Some(condition) = &base_condition {
                // Any failure leaves an exact recoverable allocation, never an unowned pair of vector tables.
                // 任何失败都会留下精确可恢复的分配，而非无归属的一对向量表。
                self.ensure_vector_table(&content_table, config.dimension)
                    .await?;
                self.record_candidate_table_created(
                    &config.binding.namespace_id,
                    candidate_allocation::CandidateVectorTable::Content,
                )
                .await?;
                self.ensure_vector_table(&path_table, config.dimension)
                    .await?;
                self.record_candidate_table_created(
                    &config.binding.namespace_id,
                    candidate_allocation::CandidateVectorTable::Path,
                )
                .await?;
                // Allocation is not write admission: cancellation or predecessor drift still revokes the final transition.
                // 分配不是写入准入：取消或前驱漂移仍会撤销最终转换。
                let finish = format!(
                    "BEGIN IMMEDIATE;\nINSERT OR ROLLBACK INTO vce_commit_guards(namespace_id, valid) VALUES ('{namespace_id}', CASE WHEN ({condition}) AND EXISTS(SELECT 1 FROM vce_rebuild_candidates WHERE namespace_id = '{namespace_id}' AND state = 'allocating') THEN 1 ELSE 0 END);\nUPDATE vce_rebuild_candidates SET state = 'building', updated_at_unix_ms = {now} WHERE namespace_id = '{namespace_id}' AND state = 'allocating';\nDELETE FROM vce_commit_guards WHERE namespace_id = '{namespace_id}';\nCOMMIT;"
                );
                self.execute(
                    &finish,
                    Vec::new(),
                    "Failed to complete the VCE candidate physical allocation",
                )
                .await?;
            }
        }
        let index = VceProjectIndex {
            data_plane: self.clone(),
            project_id: config.project_id,
            root_path: config.root_path,
            root_hash: config.root_hash,
            binding: config.binding,
            vector_space_id: config.vector_space_id,
            physical_fingerprint: config.physical_fingerprint,
            embedding_execution: None,
            read_view: None,
            management_active_refresh: None,
            rebuild_candidate: None,
            preprocessing_version: config.preprocessing_version,
            chunker_version: config.chunker_version,
            dimension: config.dimension,
            config_revision: config.config_revision,
            content_table,
            path_table,
            mutation_lock: Arc::new(Mutex::new(())),
        };
        // Opening a binding proves no ownership of previously staged work and performs no cleanup.
        // 打开绑定不能证明持有先前暂存任务，因此不执行清理。
        Ok(index)
    }

    /// Loads a versioned engine snapshot for one exact namespace.
    /// 加载一个精确命名空间的版本化引擎快照。
    pub async fn load_state_snapshot(
        &self,
        namespace_id: &str,
    ) -> Result<Option<VceStateSnapshot>, String> {
        let rows: Vec<StateSnapshotRow> = self
            .query_rows(
                "SELECT snapshot_json FROM vce_state_snapshots WHERE namespace_id = ? LIMIT 1;",
                vec![sqlite_text(namespace_id)],
            )
            .await?;
        rows.first()
            .map(|row| {
                serde_json::from_str(&row.snapshot_json)
                    .map_err(|error| format!("Failed to parse the VCE state snapshot: {error}"))
            })
            .transpose()
    }

    /// Persists the latest validated engine snapshot for restart recovery.
    /// 持久化最近一份已校验引擎快照用于重启恢复。
    pub async fn persist_state_snapshot(
        &self,
        namespace_id: &str,
        snapshot: &VceStateSnapshot,
    ) -> Result<(), String> {
        if namespace_id.trim().is_empty() {
            return Err(String::from(
                "The VCE state snapshot namespace must not be empty",
            ));
        }
        let snapshot_json = serde_json::to_string(snapshot)
            .map_err(|error| format!("Failed to encode the VCE state snapshot: {error}"))?;
        self.execute(
            r#"INSERT INTO vce_state_snapshots(namespace_id, snapshot_json, updated_at_unix_ms)
               VALUES (?, ?, ?)
               ON CONFLICT(namespace_id) DO UPDATE SET
                   snapshot_json = excluded.snapshot_json,
                   updated_at_unix_ms = excluded.updated_at_unix_ms;"#,
            vec![
                sqlite_text(namespace_id),
                sqlite_text(snapshot_json),
                SqliteValue::Integer(current_unix_epoch_millis_i64()?),
            ],
            "Failed to persist the VCE state snapshot",
        )
        .await
    }

    /// Create a new binding's dimension-specific vector table without overwriting any existing table.
    /// 为新绑定创建维度固定的向量表，不覆盖任何现有表。
    /// `table_name` and `dimension` come from the admitted binding; errors remain explicit, including collisions.
    /// `table_name` 与 `dimension` 来自已准入绑定；包括名称冲突在内的错误均明确保留。
    async fn ensure_vector_table(&self, table_name: &str, dimension: u32) -> Result<(), String> {
        self.vector_engine()
            .await?
            .create_table(LanceDbCreateTableInput {
                table_name: table_name.to_string(),
                columns: vector_table_columns(dimension),
                // This argument is overwrite_if_exists, not an if-not-exists switch.
                // 此参数是 overwrite_if_exists，不是“仅不存在时创建”开关。
                overwrite_if_exists: false,
            })
            .await
            .map_err(|error| {
                format!("Failed to create VCE LanceDB table `{table_name}`: {error}")
            })?;
        Ok(())
    }

    /// Executes one typed SQLite statement and checks the engine result.
    /// 执行一条类型化 SQLite 语句并校验引擎结果。
    async fn execute(
        &self,
        sql: &str,
        params: Vec<SqliteValue>,
        context: &str,
    ) -> Result<(), String> {
        local_sql::execute_script_async(&self.local_runtime, &self.sqlite_binding, sql, params)
            .await
            .map_err(|error| format!("{context}：{error}"))?;
        Ok(())
    }

    /// Executes one repeated SQLite statement as a bounded in-process batch.
    /// 通过有界进程内批次重复执行一条 SQLite 语句。
    async fn execute_batch(
        &self,
        sql: &str,
        items: Vec<Vec<SqliteValue>>,
        context: &str,
    ) -> Result<(), String> {
        if items.is_empty() {
            return Ok(());
        }
        local_sql::execute_batch_async(&self.local_runtime, &self.sqlite_binding, sql, items)
            .await
            .map_err(|error| format!("{context}：{error}"))?;
        Ok(())
    }

    /// Queries typed SQLite rows through the host JSON projection.
    /// 通过宿主 JSON 投影查询类型化 SQLite 行。
    async fn query_rows<T: for<'de> Deserialize<'de>>(
        &self,
        sql: &str,
        params: Vec<SqliteValue>,
    ) -> Result<Vec<T>, String> {
        // Reads share the physical-file gate so one engine connection cannot hold a snapshot
        // while another VCE binding starts a mutation or closes the database.
        // 读取也共享物理文件闸门，避免引擎连接持有快照时另一 VCE 绑定开始变更或关闭数据库。
        let result =
            local_sql::query_json_async(&self.local_runtime, &self.sqlite_binding, sql, params)
                .await
                .map_err(|error| format!("Failed to query the VCE SQLite: {error}"))?;
        serde_json::from_str(&result.json_data)
            .map_err(|error| format!("Failed to parse the VCE SQLite query result: {error}"))
    }

    /// Executes one statement and keeps SQLite's typed failure for callers that classify it.
    /// 执行一条语句，并为需要分类的调用方保留 SQLite 类型化失败。
    async fn execute_classified(
        &self,
        sql: &str,
        params: Vec<SqliteValue>,
    ) -> Result<(), local_sql::typed::LocalSqlError> {
        local_sql::typed::execute_script_async(
            &self.local_runtime,
            &self.sqlite_binding,
            sql.to_owned(),
            params,
        )
        .await
        .map(|_| ())
    }

    /// Queries rows as JSON text and keeps SQLite's typed failure for callers that classify it.
    /// 以 JSON 文本查询行，并为需要分类的调用方保留 SQLite 类型化失败。
    async fn query_json_classified(
        &self,
        sql: &str,
        params: Vec<SqliteValue>,
    ) -> Result<String, local_sql::typed::LocalSqlError> {
        local_sql::typed::query_json_async(
            &self.local_runtime,
            &self.sqlite_binding,
            sql.to_owned(),
            params,
        )
        .await
        .map(|result| result.json_data)
    }

    /// Validate an operation belongs to the physical project's exact owner.
    /// 校验操作属于物理项目的精确所有者。
    ///
    /// `project_id` selects the requested owner.
    /// Returns an explicit error for cross-project access.
    /// `project_id` 选择请求所有者。跨项目访问返回明确错误。
    fn validate_project_owner(&self, project_id: &str) -> Result<(), String> {
        if self.project_owner != project_id {
            return Err(String::from("VCE physical project storage owner mismatch"));
        }
        Ok(())
    }

    /// Releases only these exact host bindings after every external lease has drained.
    /// 全部外部租约排空后仅释放这些精确宿主绑定。
    ///
    /// Takes no extra parameters; returns combined cleanup errors without deleting stored data.
    /// 无额外参数；返回合并清理错误，不删除存储数据。
    pub(crate) async fn close_bindings(&self) -> Result<(), String> {
        self.flush_released_read_leases().await?;
        // failures retains both independent backend cleanup diagnostics for a later retry.
        // failures 保留两个独立后端的清理诊断，供后续重试。
        let mut failures = Vec::new();
        {
            // The host registry has no external binding to disable. The in-process handle is closed
            // instead, which is what actually releases the file and lets a VCE
            // layout change delete its directory on Windows. The close goes through the boundary so it
            // cannot take the file's lock away between two statements of one operation.
            // 没有外部服务持有该数据库绑定。关闭进程内句柄才是真正释放文件、并让
            // VCE 布局变更能在 Windows 上删除其目录的动作。关闭经由边界进行，因此不会在一个操作的两条语句之间
            // 把该文件的锁拿走。
            if let Err(error) =
                local_sql::close_database_file_async(&self.local_runtime, &self.sqlite_binding)
                    .await
            {
                failures.push(format!("Close VCE metadata: {error}"));
            }
        }
        // External project leases have drained before this method is called, so removing the cached runtime
        // cannot strand an active VCE vector operation. This releases the database-root lock and makes the
        // factory's resident-store limit apply to both physical backends rather than SQLite alone.
        // 调用本方法前外部项目租约已经排空，因此移除缓存运行时不会遗留活动的 VCE 向量操作。此举释放数据库根锁，
        // 并使工厂的驻留存储上限同时约束两个物理后端，而非只约束 SQLite。
        if let Err(error) = self
            .local_runtime
            .close_lance_directory(&self.vector_store_binding())
        {
            failures.push(format!("Close VCE vector database: {error}"));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

/// Builds the complete first-install VCE relational schema.
/// 构建完整的首次安装 VCE 关系数据结构。
#[must_use]
pub fn build_vce_schema_script() -> String {
    format!(
        r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS vce_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO vce_metadata(key, value) VALUES ('schema_version', '{schema_version}')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

CREATE TABLE IF NOT EXISTS vce_project_bindings (
    namespace_id TEXT PRIMARY KEY CHECK(length(trim(namespace_id)) > 0),
    project_id TEXT NOT NULL CHECK(length(trim(project_id)) > 0),
    root_path TEXT NOT NULL CHECK(length(trim(root_path)) > 0),
    root_hash TEXT NOT NULL CHECK(length(root_hash) = 64),
    checkpoint_chain_id TEXT NOT NULL UNIQUE CHECK(length(trim(checkpoint_chain_id)) > 0),
    schema_version INTEGER NOT NULL CHECK(schema_version > 0),
    vector_space_id TEXT NOT NULL UNIQUE CHECK(length(trim(vector_space_id)) > 0),
    physical_fingerprint TEXT NOT NULL CHECK(length(trim(physical_fingerprint)) > 0),
    preprocessing_version TEXT NOT NULL CHECK(length(trim(preprocessing_version)) > 0),
    chunker_version TEXT NOT NULL CHECK(length(trim(chunker_version)) > 0),
    dimension INTEGER NOT NULL CHECK(dimension > 0),
    content_table TEXT NOT NULL CHECK(length(trim(content_table)) > 0),
    path_table TEXT NOT NULL CHECK(length(trim(path_table)) > 0),
    config_revision INTEGER NOT NULL CHECK(config_revision > 0),
    active INTEGER NOT NULL CHECK(active IN (0, 1)),
    lifecycle_state TEXT NOT NULL CHECK(lifecycle_state IN ('rebuild_required', 'ready', 'orphaned')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_vce_project_active_binding
ON vce_project_bindings(project_id) WHERE active = 1;

CREATE TABLE IF NOT EXISTS vce_embedding_spaces (
    namespace_id TEXT NOT NULL,
    physical_fingerprint TEXT NOT NULL,
    preprocessing_version TEXT NOT NULL,
    chunker_version TEXT NOT NULL,
    dimension INTEGER NOT NULL CHECK(dimension > 0),
    content_table TEXT NOT NULL,
    path_table TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL CHECK(lifecycle_state IN ('active', 'orphaned')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, physical_fingerprint, preprocessing_version, chunker_version, dimension),
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);

{read_schema}
{origin_schema}
{initial_schema}

CREATE TABLE IF NOT EXISTS vce_state_snapshots (
    namespace_id TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS vce_commit_receipts (
    namespace_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL CHECK(length(request_fingerprint) = 64),
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, operation_id),
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS vce_publication_intents (
    namespace_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    refresh_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation > 0),
    checkpoint_version INTEGER NOT NULL CHECK(checkpoint_version > 0),
    request_fingerprint TEXT NOT NULL CHECK(length(request_fingerprint) = 64),
    stage TEXT NOT NULL CHECK(stage IN ('preparing', 'committed', 'reconciled', 'failed')),
    error TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    heartbeat_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, operation_id),
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_vce_publication_intents_recent
ON vce_publication_intents(namespace_id, updated_at_unix_ms DESC, operation_id);

CREATE TABLE IF NOT EXISTS vce_commit_guards (
    namespace_id TEXT PRIMARY KEY,
    valid INTEGER NOT NULL CHECK(valid = 1),
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS vce_rebuild_candidates (
    namespace_id TEXT PRIMARY KEY,
    base_namespace_id TEXT NOT NULL,
    base_checkpoint_version INTEGER NOT NULL CHECK(base_checkpoint_version > 0),
    job_id TEXT NOT NULL UNIQUE CHECK(length(trim(job_id)) > 0),
    state TEXT NOT NULL CHECK(state IN ('allocating', 'building', 'published', 'aborted')),
    content_created INTEGER NOT NULL DEFAULT 0 CHECK(content_created IN (0, 1)),
    path_created INTEGER NOT NULL DEFAULT 0 CHECK(path_created IN (0, 1)),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE,
    FOREIGN KEY(base_namespace_id) REFERENCES vce_project_bindings(namespace_id)
);

CREATE TABLE IF NOT EXISTS vce_generations (
    namespace_id TEXT NOT NULL,
    refresh_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation > 0),
    state TEXT NOT NULL CHECK(state IN ('staging', 'committed', 'aborted')),
    operation_id TEXT NOT NULL,
    error TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, refresh_id, generation),
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS vce_embedding_runs (
    namespace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation > 0),
    state TEXT NOT NULL CHECK(state IN ('planned', 'running', 'candidate_ready', 'succeeded', 'failed', 'cancelled')),
    manifest_json TEXT NOT NULL,
    -- The layout marker is pinned to the one v1 value by the schema itself, so a row in any other layout
    -- cannot be written even by a bug. The CHECK is the anti-drift assertion, in the same spirit as the
    -- pinned Lance file version: there is no second layout this build can read.
    layout_version INTEGER NOT NULL DEFAULT 2 CHECK(layout_version = 2),
    manifest_complete INTEGER NOT NULL DEFAULT 0 CHECK(manifest_complete IN (0, 1)),
    plan_fingerprint TEXT CHECK(plan_fingerprint IS NULL OR length(plan_fingerprint) = 64),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, run_id),
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_vce_embedding_runs_recent
ON vce_embedding_runs(namespace_id, updated_at_unix_ms DESC, run_id);

{shared_artifact_schema}
{shared_manifest_schema}

CREATE TABLE IF NOT EXISTS vce_embedding_run_reused_blobs (
    namespace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    reused_index INTEGER NOT NULL CHECK(reused_index >= 0),
    blob_name TEXT NOT NULL CHECK(length(trim(blob_name)) > 0 AND length(blob_name) <= 8192),
    PRIMARY KEY(namespace_id, run_id, reused_index),
    UNIQUE(namespace_id, run_id, blob_name),
    FOREIGN KEY(namespace_id, run_id)
        REFERENCES vce_embedding_runs(namespace_id, run_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS vce_embedding_run_batches (
    namespace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    batch_index INTEGER NOT NULL CHECK(batch_index >= 0),
    batch_id TEXT NOT NULL CHECK(length(trim(batch_id)) > 0 AND length(batch_id) <= 8192),
    identity_fingerprint TEXT NOT NULL CHECK(length(identity_fingerprint) = 64),
    batch_json TEXT NOT NULL CHECK(length(CAST(batch_json AS BLOB)) BETWEEN 1 AND 1048576),
    updated_at_unix_ms INTEGER NOT NULL CHECK(updated_at_unix_ms >= 0),
    PRIMARY KEY(namespace_id, run_id, batch_index),
    UNIQUE(namespace_id, run_id, batch_id),
    FOREIGN KEY(namespace_id, run_id)
        REFERENCES vce_embedding_runs(namespace_id, run_id) ON DELETE CASCADE
);
CREATE TRIGGER IF NOT EXISTS trg_vce_embedding_run_batches_touch_parent
AFTER UPDATE OF batch_json, updated_at_unix_ms ON vce_embedding_run_batches
BEGIN
    UPDATE vce_embedding_runs
    SET updated_at_unix_ms = NEW.updated_at_unix_ms,
        revision = revision + 1
    WHERE namespace_id = NEW.namespace_id AND run_id = NEW.run_id
      AND layout_version = 2 AND manifest_complete = 1;
END;

CREATE TABLE IF NOT EXISTS vce_embedding_remote_batches (
    namespace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    remote_task_id TEXT NOT NULL CHECK(length(trim(remote_task_id)) > 0 AND length(remote_task_id) <= 512),
    purpose TEXT NOT NULL CHECK(purpose IN ('code_retrieval.document_embedding', 'code_retrieval.path_embedding')),
    first_logical_batch_index INTEGER NOT NULL CHECK(first_logical_batch_index >= 0),
    logical_batch_count INTEGER NOT NULL CHECK(logical_batch_count > 0),
    record_json TEXT NOT NULL CHECK(length(CAST(record_json AS BLOB)) BETWEEN 1 AND 65536),
    updated_at_unix_ms INTEGER NOT NULL CHECK(updated_at_unix_ms > 0),
    PRIMARY KEY(namespace_id, run_id, remote_task_id),
    UNIQUE(namespace_id, run_id, purpose, first_logical_batch_index),
    FOREIGN KEY(namespace_id, run_id)
        REFERENCES vce_embedding_runs(namespace_id, run_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_vce_embedding_remote_batches_order
ON vce_embedding_remote_batches(namespace_id, run_id, purpose, first_logical_batch_index);
CREATE TRIGGER IF NOT EXISTS trg_vce_embedding_remote_batches_touch_parent
AFTER UPDATE OF record_json, updated_at_unix_ms ON vce_embedding_remote_batches
BEGIN
    UPDATE vce_embedding_runs
    SET updated_at_unix_ms = NEW.updated_at_unix_ms,
        revision = revision + 1
    WHERE namespace_id = NEW.namespace_id AND run_id = NEW.run_id
      AND layout_version = 2 AND manifest_complete = 1;
END;

{shared_result_schema}

CREATE TABLE IF NOT EXISTS vce_staged_blobs (
    namespace_id TEXT NOT NULL,
    refresh_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    blob_name TEXT NOT NULL,
    path TEXT NOT NULL,
    structure_analyzer_id TEXT,
    structure_mode TEXT,
    structure_fallback_reason TEXT,
    path_document TEXT,
    path_status TEXT NOT NULL,
    path_error TEXT,
    symbol_status TEXT NOT NULL,
    symbol_error TEXT,
    status TEXT NOT NULL,
    status_reason TEXT,
    chunk_count INTEGER NOT NULL,
    vector_count INTEGER NOT NULL,
    symbol_count INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, refresh_id, generation, blob_name),
    FOREIGN KEY(namespace_id, refresh_id, generation)
        REFERENCES vce_generations(namespace_id, refresh_id, generation) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS vce_staged_chunks (
    namespace_id TEXT NOT NULL,
    refresh_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    blob_name TEXT NOT NULL,
    chunk_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    path TEXT NOT NULL,
    content TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    start_char_offset INTEGER NOT NULL,
    kind TEXT NOT NULL,
    vector_ready INTEGER NOT NULL CHECK(vector_ready IN (0, 1)),
    PRIMARY KEY(namespace_id, refresh_id, generation, chunk_id),
    FOREIGN KEY(namespace_id, refresh_id, generation, blob_name)
        REFERENCES vce_staged_blobs(namespace_id, refresh_id, generation, blob_name) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS vce_staged_symbols (
    namespace_id TEXT NOT NULL,
    refresh_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    blob_name TEXT NOT NULL,
    occurrence_id TEXT NOT NULL,
    identifier TEXT NOT NULL,
    path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    start_char_offset INTEGER NOT NULL,
    chunk_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    PRIMARY KEY(namespace_id, refresh_id, generation, occurrence_id),
    FOREIGN KEY(namespace_id, refresh_id, generation, blob_name)
        REFERENCES vce_staged_blobs(namespace_id, refresh_id, generation, blob_name) ON DELETE CASCADE
);

-- Cascade cleanup must locate one exact file instead of scanning its entire generation for every parent.
CREATE INDEX IF NOT EXISTS idx_vce_staged_chunk_blob
ON vce_staged_chunks(namespace_id, refresh_id, generation, blob_name);
CREATE INDEX IF NOT EXISTS idx_vce_staged_symbol_blob
ON vce_staged_symbols(namespace_id, refresh_id, generation, blob_name);

CREATE TABLE IF NOT EXISTS vce_operations (
    namespace_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    phase TEXT NOT NULL,
    generation INTEGER NOT NULL,
    refresh_mode TEXT,
    refresh_reason TEXT,
    completed_units INTEGER NOT NULL DEFAULT 0,
    total_units INTEGER NOT NULL DEFAULT 0,
    attempt INTEGER NOT NULL DEFAULT 1,
    coalesced INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, operation_id)
);
CREATE INDEX IF NOT EXISTS idx_vce_operations_recent
ON vce_operations(namespace_id, updated_at_unix_ms DESC, operation_id);

CREATE TABLE IF NOT EXISTS vce_vector_cleanup (
    namespace_id TEXT NOT NULL,
    cleanup_id TEXT NOT NULL,
    cleanup_kind TEXT NOT NULL CHECK(cleanup_kind IN ('aborted_generation', 'retired_blob')),
    refresh_id TEXT,
    generation INTEGER,
    blob_names_json TEXT,
    physical_fingerprint TEXT NOT NULL,
    content_table TEXT NOT NULL,
    path_table TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('pending', 'completed')),
    attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, cleanup_id),
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_vce_vector_cleanup_pending
ON vce_vector_cleanup(namespace_id, state, created_at_unix_ms, cleanup_id);
"#,
        schema_version = VCE_DATA_PLANE_SCHEMA_VERSION,
        read_schema = read_generations::read_generation_schema_script(),
        origin_schema = embedding_provenance::embedding_origin_schema_script(),
        initial_schema = initial_allocation::initial_allocation_schema_script(),
        shared_artifact_schema = shared_artifacts::SCHEMA,
        shared_manifest_schema = shared_manifest_store::SCHEMA,
        shared_result_schema = shared_result_store::SCHEMA,
    )
}

impl VceIndexPort for VceProjectIndex {
    /// Returns the immutable project namespace binding.
    /// 返回不可变的项目命名空间绑定。
    fn binding(&self) -> &VceIndexBinding {
        &self.binding
    }

    /// Classifies requested Blob identities against the committed relational index.
    /// 根据已提交关系索引分类请求的 Blob 身份。
    fn find_missing(
        &self,
        request: FindMissingRequest,
    ) -> VceFuture<Result<FindMissingResponse, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.find_missing_inner(request).await })
    }

    /// Stages one bounded complete Blob batch and its vectors.
    /// 暂存一个有界完整 Blob 批次及其向量。
    fn stage(
        &self,
        request: IndexStageBatch,
    ) -> VceFuture<Result<IndexStageReceipt, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.stage_inner(request).await })
    }

    /// Aborts one unpublished staging transaction.
    /// 中止一个尚未发布的暂存事务。
    fn abort_stage(
        &self,
        request: IndexAbortRequest,
    ) -> VceFuture<Result<IndexAbortReceipt, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.abort_stage_inner(request).await })
    }

    /// Publishes one staged checkpoint with transaction-enforced CAS and process-local serialization.
    /// 使用事务强制 CAS 与进程内串行化发布一个已暂存检查点。
    fn commit(&self, request: IndexCommit) -> VceFuture<Result<CommitReceipt, VcePortError>> {
        let index = self.clone();
        Box::pin(async move {
            index
                .commit_inner(request, None)
                .await
                .map(|(receipt, _)| receipt)
        })
    }

    /// Searches committed content chunks in the exact requested Blob scope.
    /// 在精确请求的 Blob 范围内检索已提交内容代码块。
    fn search_dense(
        &self,
        request: DenseSearchRequest,
    ) -> VceFuture<Result<VceSearchResponse, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.search_dense_inner(request).await })
    }

    /// Searches committed path documents in the exact requested Blob scope.
    /// 在精确请求的 Blob 范围内检索已提交路径文档。
    fn search_path(
        &self,
        request: PathSearchRequest,
    ) -> VceFuture<Result<VcePathSearchResponse, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.search_path_inner(request).await })
    }

    /// Searches committed exact symbol occurrences.
    /// 检索已提交的精确符号出现位置。
    fn search_exact(
        &self,
        request: ExactSearchRequest,
    ) -> VceFuture<Result<VceSearchResponse, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.search_exact_inner(request).await })
    }

    /// Returns aggregate committed index and integrity state.
    /// 返回已提交索引与完整性的聚合状态。
    fn management_snapshot(
        &self,
        request: VceManagementRequest,
    ) -> VceFuture<Result<VceIndexManagementSnapshot, VcePortError>> {
        let index = self.clone();
        Box::pin(async move {
            // Only unpinned reads need local write exclusion across their multiple SQL statements.
            // 仅未固定读取需要在多条 SQL 之间排斥本地写入。
            let _guard = if index.read_view.is_none() {
                Some(index.mutation_lock.lock().await)
            } else {
                None
            };
            index.management_snapshot_inner(request).await
        })
    }

    /// Executes one typed read-only management query.
    /// 执行一项类型化只读管理查询。
    fn management_query(
        &self,
        request: VceManagementQueryRequest,
    ) -> VceFuture<Result<VceManagementQueryResponse, VcePortError>> {
        let index = self.clone();
        Box::pin(async move {
            // Pinned content remains readable during writes; live maintenance plans serialize locally.
            // 固定内容在写入期间保持可读；实时维护计划在本地串行读取。
            let _guard = if index.read_view.is_none() {
                Some(index.mutation_lock.lock().await)
            } else {
                None
            };
            index.management_query_inner(request).await
        })
    }

    /// Persists the exact source snapshot and bounded request plan for restart recovery.
    /// 持久化精确源码快照与有界请求计划用于重启恢复。
    fn persist_embedding_manifest(
        &self,
        manifest: VceEmbeddingRunManifest,
    ) -> VceFuture<Result<(), VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.persist_embedding_manifest(manifest).await })
    }

    /// Persist a plan with file-level receipts from the actual bounded storage writes.
    /// 持久化计划，并从真实的有界存储写入报告文件级回执。
    /// `manifest` owns the plan; `control` receives progress. Returns the final persistence result.
    /// `manifest` 持有计划；`control` 接收进度。返回最终持久化结果。
    fn persist_embedding_manifest_observed(
        &self,
        manifest: VceEmbeddingRunManifest,
        control: vulcan_context_engine::VceRefreshControl,
    ) -> VceFuture<Result<(), VcePortError>> {
        // index preserves the exact storage lifetime across asynchronous writes.
        // index 在异步写入期间保留精确存储生命周期。
        let index = self.clone();
        Box::pin(async move {
            index
                .persist_embedding_manifest_sharded_observed(manifest, Some(&control))
                .await
        })
    }

    /// Loads one exact durable embedding manifest for host recovery.
    /// 为宿主恢复读取一份精确的持久化向量清单。
    fn load_embedding_manifest(
        &self,
        run_id: String,
    ) -> VceFuture<Result<Option<VceEmbeddingRunManifest>, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.load_embedding_manifest(run_id).await })
    }

    /// Read bounded run metadata without reloading frozen source bodies.
    /// 读取有界运行元数据，不重新加载冻结源码正文。
    /// Run identity selects the exact optional generation and lifecycle.
    /// 运行身份选择精确的可选代次与生命周期。
    fn load_embedding_run_summary(
        &self,
        run_id: String,
    ) -> VceFuture<Result<Option<(u64, VceEmbeddingRunState)>, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.load_embedding_run_summary_sharded(run_id).await })
    }

    /// Persists one run lifecycle transition without rewriting frozen source-node shards.
    /// 持久化一次运行生命周期转换，不重写冻结源码节点分片。
    fn persist_embedding_run_state(
        &self,
        run_id: String,
        state: VceEmbeddingRunState,
        updated_at_unix_ms: u64,
    ) -> VceFuture<Result<(), VcePortError>> {
        let index = self.clone();
        Box::pin(async move {
            index
                .persist_embedding_run_state_sharded(run_id, state, updated_at_unix_ms)
                .await
        })
    }

    /// Persists one mutable batch record without rewriting any source-node shard.
    /// 持久化一条可变批次记录，不重写任何源码节点分片。
    fn persist_embedding_batch_record(
        &self,
        run_id: String,
        batch: VceEmbeddingBatchRecord,
        updated_at_unix_ms: u64,
    ) -> VceFuture<Result<(), VcePortError>> {
        let index = self.clone();
        Box::pin(async move {
            index
                .persist_embedding_batch_record_sharded(run_id, batch, updated_at_unix_ms)
                .await
        })
    }

    /// Persists one remote asynchronous Batch record without rewriting source-plan shards.
    /// 持久化一条远端异步 Batch 记录，不重写源码计划分片。
    fn persist_embedding_remote_batch_record(
        &self,
        run_id: String,
        record: VceEmbeddingRemoteBatchRecord,
    ) -> VceFuture<Result<(), VcePortError>> {
        let index = self.clone();
        Box::pin(async move {
            index
                .persist_embedding_remote_batch_record(run_id, record)
                .await
        })
    }

    /// Loads one purpose's remote asynchronous Batch records in deterministic order.
    /// 按确定顺序读取一个用途的远端异步 Batch 记录。
    fn load_embedding_remote_batch_records(
        &self,
        run_id: String,
        purpose: vulcan_context_engine::VceAiPurpose,
    ) -> VceFuture<Result<Vec<VceEmbeddingRemoteBatchRecord>, VcePortError>> {
        let index = self.clone();
        Box::pin(async move {
            index
                .load_embedding_remote_batch_records(run_id, purpose)
                .await
        })
    }

    /// Load a bounded group of exact prepared artifacts, validating the producing analyzer.
    /// 读取一组有界精确准备材料，校验生产分析器。
    /// Contract, Blob IDs and generation select compatible snapshots; returns matching nodes only.
    /// 契约、Blob ID 与代次选择兼容快照；仅返回匹配节点。
    fn load_prepared_artifacts(
        &self,
        contract: vulcan_context_engine::VceEmbeddingRunContract,
        blobs: Vec<String>,
        analyzer_id: String,
        generation: u64,
    ) -> VceFuture<Result<Vec<vulcan_context_engine::VceCodeNodeSnapshot>, VcePortError>> {
        // Owned storage handle survives the blocking connection boundary.
        // 拥有的存储句柄跨越阻塞连接边界。
        let plane = self.data_plane.clone();
        Box::pin(async move {
            local_sql::with_connection_async(&plane.local_runtime,&plane.sqlite_binding,move |connection| {
                use rusqlite::OptionalExtension;
                let key = shared_manifest_store::preparation_identity(&contract)?;
                let mut nodes = Vec::new();
                let mut lookup = connection.prepare_cached("SELECT artifact_hash FROM vce_prepared_artifact_cache WHERE contract_hash=? AND blob_name=?").map_err(|error|error.to_string())?;
                for blob in blobs {
                    let artifact: Option<String> = lookup.query_row(rusqlite::params![key,blob],|row|row.get(0)).optional().map_err(|error|error.to_string())?;
                    if let Some(artifact) = artifact {
                        let node = shared_manifest_store::restore_node(connection,&artifact,&blob,generation)?;
                        if node.structure_analyzer_id.as_deref() == Some(analyzer_id.as_str()) && node.structure_fallback_reason.is_none() { nodes.push(node); }
                    }
                }
                Ok(nodes)
            }).await.map_err(|error|port_failed(VcePortOperation::EmbeddingManifest,error))
        })
    }

    /// Adopt compatible vectors for an active run before it owns remote tasks.
    /// 在活动运行持有远端任务前接纳兼容向量。
    /// Run and batch identify the frozen request; returns owned vectors or absence.
    /// 运行和批次标识冻结请求；返回已归属向量或空值。
    fn reuse_embedding_batch_result(
        &self,
        run_id: String,
        batch_id: String,
    ) -> VceFuture<Result<Option<VceEmbeddingBatchResult>, VcePortError>> {
        // Exact frozen batch ownership is checked again inside the transaction.
        // 在事务中再次核对精确冻结批次归属。
        let index = self.clone();
        Box::pin(async move {
            if index.read_view.is_some() {
                return Err(port_rejected(
                    VcePortOperation::EmbeddingManifest,
                    "Pinned views cannot adopt results",
                ));
            }
            let batch = index
                .load_published_embedding_batch(&run_id, &batch_id)
                .await?;
            let namespace = index.binding.namespace_id.clone();
            let now = current_unix_epoch_millis_i64()
                .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
            local_sql::with_connection_async(&index.data_plane.local_runtime,&index.data_plane.sqlite_binding,move |connection| {
                // Do not replace an existing remote task with a cache hit; reconcile its durable lifecycle.
                // 不以缓存命中替代已有远端任务；应对账其持久生命周期。
                let transaction = connection.transaction().map_err(|error| error.to_string())?;
                let active: bool = transaction.query_row("SELECT state='running' AND manifest_complete=1 FROM vce_embedding_runs WHERE namespace_id=? AND run_id=?", rusqlite::params![namespace,run_id], |row| row.get(0)).map_err(|error| error.to_string())?;
                let remote: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM vce_embedding_remote_batches WHERE namespace_id=? AND run_id=? AND purpose=?)",rusqlite::params![namespace,run_id,batch.purpose],|row|row.get(0)).map_err(|error|error.to_string())?;
                if !active || remote { return Ok(None); }
                let result = shared_result_store::reuse(&transaction,&namespace,&run_id,&batch,now)?;
                transaction.commit().map_err(|error|error.to_string())?;
                Ok(result)
            }).await.map_err(|error|port_failed(VcePortOperation::EmbeddingManifest,error))
        })
    }

    /// Persists one provider result for restart-safe batch replay.
    /// 持久化一份供应商结果，使批次重启后可以安全恢复。
    fn persist_embedding_batch_result(
        &self,
        run_id: String,
        result: VceEmbeddingBatchResult,
    ) -> VceFuture<Result<(), VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.persist_embedding_batch_result(run_id, result).await })
    }

    /// Loads one persisted provider result for exact batch recovery.
    /// 为精确批次恢复读取一份持久化供应商结果。
    fn load_embedding_batch_result(
        &self,
        run_id: String,
        batch_id: String,
    ) -> VceFuture<Result<Option<VceEmbeddingBatchResult>, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.load_embedding_batch_result(run_id, batch_id).await })
    }
}

impl VceProjectIndex {
    /// Bind one already-approved host producer to this unchanged index handle without writing storage.
    /// 将一个已获批准的宿主生产来源绑定到不变索引句柄，不写入存储。
    /// `grant` must name this exact original binding. Returns the consuming handle or a scope error.
    /// `grant` 必须指向当前精确原始绑定。返回消费后的句柄或作用域错误。
    pub fn with_embedding_execution(
        mut self,
        grant: vulcan_context_engine::VceEmbeddingExecutionContract,
    ) -> Result<Self, String> {
        if grant.original() != &self.binding {
            return Err(String::from(
                "VCE producer authorization belongs to a different index",
            ));
        }
        self.embedding_execution = Some(grant);
        Ok(self)
    }

    /// Read authoritative physical statistics for one fixed VCE table role.
    /// 读取一个固定 VCE 表角色的权威物理统计信息。
    ///
    /// `role` selects only this index's content or path table. Returns engine statistics
    /// projected into the model-independent maintenance contract.
    /// `role` 只能选择此索引的内容表或路径表。返回投影到模型无关维护契约的引擎统计信息。
    pub async fn physical_table_statistics(
        &self,
        role: VcePhysicalTableRole,
    ) -> Result<VcePhysicalTableStatistics, String> {
        let table_name = self.physical_table_name(role);
        let statistics = self
            .data_plane
            .vector_engine()
            .await?
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: table_name.to_string(),
                compaction: engine_compaction_options(&physical_compaction_options())?,
            })
            .await
            .map_err(|error| format!("Failed to read VCE physical table statistics: {error}"))?;
        if statistics.table_name != table_name {
            return Err(String::from(
                "VCE physical statistics returned a different table identity",
            ));
        }
        Ok(map_physical_table_statistics(role, statistics))
    }

    /// Projects operation statistics after verifying the immutable table role.
    /// 校验不可变表角色后，投影存储操作返回的统计信息。
    ///
    /// `role` selects this index's fixed table and `statistics` is one operation boundary snapshot.
    /// Returns the VCE maintenance representation or rejects a cross-table result.
    /// `role` 选择此索引的固定表，`statistics` 是一次操作边界快照。
    /// 返回 VCE 维护表示，或拒绝跨表结果。
    pub fn physical_operation_statistics(
        &self,
        role: VcePhysicalTableRole,
        statistics: VcePhysicalTableStatistics,
    ) -> Result<VcePhysicalTableStatistics, String> {
        if statistics.table_name != self.physical_table_name(role) || statistics.role != role {
            return Err(String::from(
                "VCE physical compaction statistics belong to a different table",
            ));
        }
        Ok(statistics)
    }

    /// Submit one idempotent native file-compaction step for a fixed VCE table.
    /// 为一张固定 VCE 表提交一个幂等的原生文件整理步骤。
    ///
    /// `operation_id` is stable across uncertain replies; `expected_version` and `options`
    /// are frozen by the confirmed plan. Returns the host-owned operation snapshot.
    /// `operation_id` 在不确定回复间保持稳定；`expected_version` 与 `options` 由已确认计划冻结。
    /// 返回宿主持有的操作快照。
    pub async fn submit_physical_compaction(
        &self,
        role: VcePhysicalTableRole,
        operation_id: &str,
        expected_version: u64,
        options: &VceCompactionOptions,
    ) -> Result<VcePhysicalCompactionOperation, String> {
        if operation_id.trim().is_empty() || expected_version == 0 {
            return Err(String::from(
                "VCE physical compaction requires a stable operation and table version",
            ));
        }
        let table_name = self.physical_table_name(role).to_string();
        let submitted_at_unix_ms = u64::try_from(current_unix_epoch_millis_i64()?)
            .map_err(|_| String::from("VCE physical compaction timestamp is out of range"))?;
        // The in-process engine performs one synchronous compaction step, so the operation is already
        // terminal when this returns and no queued, running or cancellable window ever exists.
        // 进程内引擎执行一次同步整理步骤，故本函数返回时操作已处于终态，不存在排队、运行或可取消窗口。
        let options = engine_compaction_options(options)?;
        let outcome = self
            .data_plane
            .vector_engine()
            .await?
            .compact_table(LanceDbCompactTableInput {
                table_name: table_name.clone(),
                expected_version: Some(expected_version),
                options,
            })
            .await;
        let operation = match outcome {
            Ok(result) => VcePhysicalCompactionOperation {
                operation_id: operation_id.to_string(),
                table_name,
                state: VcePhysicalCompactionState::Succeeded,
                cancel_requested: false,
                message: result.message,
                error: None,
                submitted_at_unix_ms,
                started_at_unix_ms: Some(submitted_at_unix_ms),
                completed_at_unix_ms: Some(submitted_at_unix_ms),
                before: Some(map_physical_table_statistics(role, result.before)),
                after: Some(map_physical_table_statistics(role, result.after)),
                metrics: Some(VcePhysicalCompactionMetrics {
                    fragments_removed: result.metrics.fragments_removed,
                    fragments_added: result.metrics.fragments_added,
                    files_removed: result.metrics.files_removed,
                    files_added: result.metrics.files_added,
                }),
            },
            Err(error) => VcePhysicalCompactionOperation {
                operation_id: operation_id.to_string(),
                table_name,
                state: VcePhysicalCompactionState::Failed,
                cancel_requested: false,
                message: String::from("VCE physical compaction did not complete"),
                error: Some(error.to_string()),
                submitted_at_unix_ms,
                started_at_unix_ms: Some(submitted_at_unix_ms),
                completed_at_unix_ms: Some(submitted_at_unix_ms),
                before: None,
                after: None,
                metrics: None,
            },
        };
        // The in-process step is synchronous, so this submission is the only moment its result is knowable.
        // Recording it here is what lets a later query report the step that really ran instead of inventing a
        // terminal state. Failures are recorded too, so a consumer that re-queries a failed step observes the
        // same failure rather than a reconstructed success.
        // 进程内步骤是同步的，故本次提交是唯一能得知其结果的时刻。在此记录它，正是后续查询能够报告确实运行过的
        // 步骤、而不是编造终态的原因。失败同样被记录，使重新查询失败步骤的消费者观察到同一个失败，而不是一个被
        // 重建出来的成功。
        self.remember_compaction_operation(&operation);
        self.validate_physical_compaction_operation(role, operation_id, operation)
    }

    /// Records one completed in-process physical compaction step on this project's shared handle.
    /// 在本项目的共享句柄上记录一个已完成的进程内物理整理步骤。
    ///
    /// `operation` is the exact step this index just ran. Takes no other input and returns nothing; a poisoned
    /// registry lock is recovered rather than refused, matching the recovery convention used throughout the
    /// local database runtime.
    /// `operation` 是本索引刚刚运行的精确步骤。不接收其他输入且无返回值；注册表锁被毒化时予以恢复而非拒绝，
    /// 与本地数据库运行时全篇采用的恢复惯例一致。
    fn remember_compaction_operation(&self, operation: &VcePhysicalCompactionOperation) {
        let mut registry = self
            .data_plane
            .compaction_operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.insert(operation.operation_id.clone(), operation.clone());
    }

    /// Queries one host-owned compaction operation for this index and table role.
    /// 查询此索引及表角色所属的一项宿主整理操作。
    ///
    /// `operation_id` identifies a previously submitted step. Returns a verified table-scoped
    /// snapshot or an explicit state-loss diagnostic.
    /// `operation_id` 标识先前提交的步骤。返回经过表作用域校验的快照或明确的状态丢失诊断。
    pub async fn physical_compaction_operation(
        &self,
        role: VcePhysicalTableRole,
        operation_id: &str,
    ) -> Result<VcePhysicalCompactionOperation, String> {
        if operation_id.trim().is_empty() {
            return Err(String::from(
                "VCE physical compaction requires a stable operation identity",
            ));
        }
        // The in-process step is synchronous, so its result was fixed at submission and this query reports the
        // step that really ran, with the statistics and metrics the engine actually returned. When this
        // process holds no such submission, the honest answer is that the step is not knowable — never a
        // reconstructed terminal state.
        // 进程内步骤是同步的，故其结果在提交时即已固定；本查询报告确实运行过的步骤，并携带引擎实际返回的统计与
        // 指标。若本进程没有该提交的记录，唯一诚实的回答是该步骤不可知，绝不重建一个终态。
        let operation = {
            let registry = self
                .data_plane
                .compaction_operations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            registry.get(operation_id).cloned()
        };
        let Some(operation) = operation else {
            return Err(format!(
                "VCE physical compaction operation `{operation_id}` was not submitted by this process and its \
                 result cannot be reconstructed"
            ));
        };
        self.validate_physical_compaction_operation(role, operation_id, operation)
    }

    /// Requests cancellation at the host operation's next safe compaction boundary.
    /// 请求在宿主操作的下一个安全整理边界取消操作。
    ///
    /// `operation_id` identifies the current table step. Returns the host operation's exact
    /// cancellation state and never claims that an atomic rewrite was preempted.
    /// `operation_id` 标识当前表步骤。返回宿主操作的精确取消状态，绝不宣称已抢占原子重写。
    pub async fn cancel_physical_compaction(
        &self,
        role: VcePhysicalTableRole,
        operation_id: &str,
    ) -> Result<VcePhysicalCompactionOperation, String> {
        // An in-process synchronous step holds no cancellation window, so the only honest answer is that
        // the work already completed. This never claims that an atomic rewrite was preempted.
        // 进程内同步步骤不持有取消窗口，故唯一诚实的回答是工作已经完成。这绝不宣称已抢占原子重写。
        self.physical_compaction_operation(role, operation_id).await
    }

    /// Resolve one authorized table role to this index's immutable physical table name.
    /// 将一个已授权表角色解析为此索引不可变的物理表名。
    fn physical_table_name(&self, role: VcePhysicalTableRole) -> &str {
        match role {
            VcePhysicalTableRole::Content => &self.content_table,
            VcePhysicalTableRole::Path => &self.path_table,
        }
    }

    /// Verifies that a storage operation still belongs to the selected identity and immutable table.
    /// 校验存储操作仍属于选定身份与不可变物理表。
    fn validate_physical_compaction_operation(
        &self,
        role: VcePhysicalTableRole,
        operation_id: &str,
        operation: VcePhysicalCompactionOperation,
    ) -> Result<VcePhysicalCompactionOperation, String> {
        validate_physical_compaction_operation_identity(
            operation_id,
            self.physical_table_name(role),
            operation,
        )
    }

    /// Return the actual producer without changing the stored physical identity; takes no parameters.
    /// 返回实际生产来源而不改变已存物理身份；无参数。
    fn embedding_producer_fingerprint(&self) -> &str {
        match &self.embedding_execution {
            // Contract construction requires an explicit physical fingerprint for every approved producer.
            // 契约构建要求每个已批准生产来源均具有明确物理指纹。
            Some(grant) => grant
                .execution()
                .physical_fingerprint
                .as_deref()
                .expect("validated producer"),
            None => &self.physical_fingerprint,
        }
    }

    /// Read exact origins of the pinned publication or the unpinned current index; takes no parameters.
    /// 读取固定发布或未固定当前索引的精确来源；无参数。
    /// Returns authoritative provenance or an explicit missing-binding error, without creating storage.
    /// 返回权威来源或明确绑定缺失错误，不创建存储。
    pub async fn embedding_provenance(&self) -> Result<VceEmbeddingProvenance, String> {
        if let Some(read_view) = &self.read_view {
            return Ok(read_view.provenance.clone());
        }
        if self.rebuild_candidate.is_some()
            && self
                .current_checkpoint_row()
                .await
                .map_err(|error| error.to_string())?
                .is_none()
        {
            self.require_active_binding(VcePortOperation::Commit, false)
                .await
                .map_err(|error| error.to_string())?;
            // rows load the candidate's own persisted origin; the predecessor must never supply its dimensions.
            // rows 加载候选自身的持久来源；绝不能从前驱取得候选维度。
            let rows: Vec<ProjectBindingRow> = self.data_plane.query_rows(
                "SELECT project_id, root_path, root_hash, namespace_id, checkpoint_chain_id, schema_version, vector_space_id, physical_fingerprint, preprocessing_version, chunker_version, dimension, content_table, path_table, config_revision, active FROM vce_project_bindings WHERE namespace_id = ? AND project_id = ?;",
                vec![sqlite_text(&self.binding.namespace_id), sqlite_text(&self.project_id)],
            ).await?;
            let [row] = rows.as_slice() else {
                return Err(String::from("VCE candidate origin is missing"));
            };
            // original is immutable within this new namespace and is verified again by the commit guard.
            // original 在此新命名空间内不可变，提交守卫会再次校验。
            let original = row.config_for_active_state(false)?;
            if original.binding != self.binding {
                return Err(String::from(
                    "VCE candidate origin changed during inspection",
                ));
            }
            return Ok(VceEmbeddingProvenance {
                source_fingerprints: BTreeSet::from([original.physical_fingerprint.clone()]),
                original,
                checkpoint_version: None,
            });
        }
        let provenance = self
            .data_plane
            .embedding_provenance(&self.project_id)
            .await?
            .ok_or_else(|| String::from("VCE index provenance disappeared"))?;
        if provenance.original.binding != self.binding {
            return Err(String::from(
                "VCE active index changed before provenance inspection",
            ));
        }
        Ok(provenance)
    }

    /// Read the exact provenance of an orphaned uninitialized recovery binding.
    /// 读取孤立未初始化恢复绑定的精确来源。
    ///
    /// This path is read-only and never substitutes for active-binding provenance.
    /// 此路径只读，绝不替代活动绑定来源。
    pub async fn uninitialized_recovery_provenance(
        &self,
    ) -> Result<VceEmbeddingProvenance, String> {
        if self.read_view.is_some() {
            return Err(String::from(
                "VCE recovery provenance cannot be read from a pinned view",
            ));
        }
        let rows: Vec<ProjectBindingRow> = self
            .data_plane
            .query_rows(
                "SELECT project_id, root_path, root_hash, namespace_id, checkpoint_chain_id, schema_version, vector_space_id, physical_fingerprint, preprocessing_version, chunker_version, dimension, content_table, path_table, config_revision, active FROM vce_project_bindings WHERE project_id = ? AND namespace_id = ? AND active = 0 AND lifecycle_state = 'orphaned' AND NOT EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = vce_project_bindings.namespace_id);",
                vec![
                    sqlite_text(&self.project_id),
                    sqlite_text(&self.binding.namespace_id),
                ],
            )
            .await?;
        let [row] = rows.as_slice() else {
            return Err(String::from(
                "VCE uninitialized recovery binding is missing",
            ));
        };
        let original = row.config_for_active_state(false)?;
        if original.project_id != self.project_id
            || original.root_path != self.root_path
            || original.root_hash != self.root_hash
            || original.binding != self.binding
            || original.vector_space_id != self.vector_space_id
            || original.physical_fingerprint != self.physical_fingerprint
            || original.preprocessing_version != self.preprocessing_version
            || original.chunker_version != self.chunker_version
            || original.dimension != self.dimension
            || original.config_revision != self.config_revision
        {
            return Err(String::from(
                "VCE uninitialized recovery provenance changed during inspection",
            ));
        }
        if self
            .current_checkpoint_row()
            .await
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(String::from(
                "VCE uninitialized recovery binding already has a checkpoint",
            ));
        }
        Ok(VceEmbeddingProvenance {
            source_fingerprints: BTreeSet::from([original.physical_fingerprint.clone()]),
            original,
            checkpoint_version: None,
        })
    }

    /// Return a counted project-job repository without reading or mutating its database.
    /// 返回带计数的项目作业仓库，不读取或修改数据库。
    ///
    /// Takes no parameters. Returns exact physical ownership for durable host job receipts.
    /// 无参数。返回持久宿主作业回执的精确物理归属。
    pub fn job_store(&self) -> VceProjectJobStore {
        VceProjectJobStore::new(self.data_plane.clone(), self.project_id.clone())
    }

    /// Report whether this handle is an immutable checkpoint read view.
    /// 报告此句柄是否为不可变检查点读取视图。
    ///
    /// Takes no parameters. Returns true for pinned readers that cannot mutate storage.
    /// 无参数。无法修改存储的固定读取方返回真。
    pub fn is_read_only(&self) -> bool {
        self.read_view.is_some()
    }

    /// Check physical write authority before a caller scans source or incurs model requests.
    /// 在调用方扫描源码或产生模型请求前检查物理写入授权。
    ///
    /// Takes no parameters. Rejects immutable published read views before any mutation.
    /// 无参数。在任何修改之前拒绝不可变已发布读取视图。
    pub fn require_write_access(&self) -> Result<(), String> {
        if self.read_view.is_some() {
            return Err(String::from(
                "VCE pinned read views cannot mutate index storage",
            ));
        }
        Ok(())
    }

    /// Read the snapshot using this handle's exact project database and namespace.
    /// 使用此句柄精确的项目数据库与命名空间读取快照。
    ///
    /// Takes no parameters. Returns the persisted snapshot or confirmed absence.
    /// 无参数。返回持久化快照或已确认的缺失。
    pub async fn load_state_snapshot(&self) -> Result<Option<VceStateSnapshot>, String> {
        self.data_plane
            .load_state_snapshot(&self.binding.namespace_id)
            .await
    }

    /// Persist a no-op refresh snapshot only within this index's frozen identity.
    /// 仅在此索引冻结的身份内持久化无变更刷新快照。
    ///
    /// `snapshot` must match the exact binding identity. Returns a storage or identity error.
    /// `snapshot` 必须匹配精确绑定身份。返回存储或身份错误。
    pub async fn persist_state_snapshot(&self, snapshot: &VceStateSnapshot) -> Result<(), String> {
        self.require_write_access()?;
        if snapshot.identity.index_namespace_id != self.binding.namespace_id
            || snapshot.identity.checkpoint_chain_id != self.binding.checkpoint_chain_id
            || snapshot.identity.index_schema_version != self.binding.schema_version
            || snapshot.identity.embedding_space != self.binding.embedding_space
        {
            return Err(String::from(
                "VCE snapshot does not belong to this physical index",
            ));
        }
        self.data_plane
            .persist_state_snapshot(&self.binding.namespace_id, snapshot)
            .await
    }

    /// Persist one complete embedding manifest under the exact active namespace.
    /// 在精确活动命名空间下持久化一份完整向量清单。
    ///
    /// `manifest` contains frozen source bodies, identities, and bounded progress; provider
    /// credentials never enter this table. Returns an explicit ownership or storage error.
    /// `manifest` 包含冻结源码正文、身份及有界进度；供应商凭据绝不进入此表。
    /// 返回明确的归属或存储错误。
    async fn persist_embedding_manifest(
        &self,
        manifest: VceEmbeddingRunManifest,
    ) -> Result<(), VcePortError> {
        self.persist_embedding_manifest_sharded(manifest).await
    }

    /// Load one manifest owned by this namespace without widening the lookup scope.
    /// 在不扩大查询范围的前提下读取一个属于当前命名空间的清单。
    async fn load_embedding_manifest(
        &self,
        run_id: String,
    ) -> Result<Option<VceEmbeddingRunManifest>, VcePortError> {
        self.load_embedding_manifest_sharded(run_id).await
    }

    /// Persist one validated provider result under its exact run and batch identity.
    /// 在精确运行和批次身份下持久化一份已校验供应商结果。
    async fn persist_embedding_batch_result(
        &self,
        run_id: String,
        result: VceEmbeddingBatchResult,
    ) -> Result<(), VcePortError> {
        if self.read_view.is_some() {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "VCE pinned read views cannot persist embedding batch results",
            ));
        }
        if run_id.trim().is_empty() || run_id.len() > 512 {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding batch result run identity is empty or exceeds its bound",
            ));
        }
        result
            .validate()
            .map_err(|error| port_invalid(VcePortOperation::EmbeddingManifest, error))?;
        let batch = self
            .load_published_embedding_batch(&run_id, &result.batch_id)
            .await?;
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
        let now = current_unix_epoch_millis_i64()
            .map_err(|error| port_failed(VcePortOperation::EmbeddingManifest, error))?;
        // A result is immutable once inserted; exact replays reconcile an ambiguous write response.
        // 结果一旦插入即不可变；精确重放用于收敛结果不明确的写入响应。
        let namespace = self.binding.namespace_id.clone();
        // Move one immutable request/result pair across the blocking database boundary.
        // 将一对不可变请求和结果移交到阻塞数据库边界。
        let owned_run = run_id.clone();
        let owned_result = result.clone();
        let write_error = local_sql::with_connection_async(
            &self.data_plane.local_runtime, &self.data_plane.sqlite_binding,
            move |connection| {
                // Admission and all shared references commit together.
                // 准入和全部共享引用一同提交。
                let transaction = connection.transaction().map_err(|error| error.to_string())?;
                let active: bool = transaction.query_row("SELECT state='running' AND manifest_complete=1 FROM vce_embedding_runs WHERE namespace_id=? AND run_id=?", rusqlite::params![namespace,owned_run], |row| row.get(0)).map_err(|error| error.to_string())?;
                if active { shared_result_store::store(&transaction,&namespace,&owned_run,&batch,&owned_result,now)?; }
                transaction.commit().map_err(|error| error.to_string())
            },
        ).await.err();
        let stored = self
            .load_embedding_batch_result(run_id, result.batch_id.clone())
            .await;
        match stored {
            Ok(Some(stored)) if stored == result => Ok(()),
            Ok(Some(_)) => Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding batch result conflicts with its immutable durable value",
            )),
            Ok(None) => Err(write_error.map_or_else(
                || {
                    port_rejected(
                        VcePortOperation::EmbeddingManifest,
                        "embedding batch result requires an active published run",
                    )
                },
                |error| port_failed(VcePortOperation::EmbeddingManifest, error),
            )),
            Err(read_error) => match write_error {
                Some(write_error) => Err(port_failed(
                    VcePortOperation::EmbeddingManifest,
                    format!(
                        "embedding batch result write was uncertain: {write_error}; reconciliation failed: {read_error}"
                    ),
                )),
                None => Err(read_error),
            },
        }
    }

    /// Load one exact provider result without widening recovery to another run or namespace.
    /// 在不扩展到其他运行或命名空间的前提下读取一份精确供应商结果。
    async fn load_embedding_batch_result(
        &self,
        run_id: String,
        batch_id: String,
    ) -> Result<Option<VceEmbeddingBatchResult>, VcePortError> {
        if run_id.trim().is_empty()
            || run_id.len() > 512
            || batch_id.trim().is_empty()
            || batch_id.len() > 8192
        {
            return Err(port_rejected(
                VcePortOperation::EmbeddingManifest,
                "embedding result identity is invalid",
            ));
        }
        self.require_published_embedding_batch(&run_id, &batch_id)
            .await?;
        self.load_embedding_result_for_transition(&run_id, &batch_id)
            .await
    }

    /// Retry only cleanup already authorized by a durable abort or retirement transaction.
    /// 仅重试已由持久中止或退役事务授权的清理。
    /// No parameters. Returns cleanup errors without declaring any staging writer abandoned.
    /// 无参数。返回清理错误，不将任何暂存写入者认定为遗弃。
    pub async fn retry_pending_cleanup(&self) -> Result<(), String> {
        self.require_write_access()?;
        let _guard = self.mutation_lock.lock().await;
        self.recover_pending_vector_cleanup().await
    }

    /// Detect unresolved staging without inferring that its owning host has stopped.
    /// 检测未解决暂存，不推断其所属宿主已经退出。
    /// No parameters. Returns whether an exact namespace still has a live-or-unconfirmed attempt.
    /// 无参数。返回精确命名空间内是否仍有存活或未确证归属的尝试。
    pub async fn has_unresolved_staging(&self) -> Result<bool, String> {
        // rows uses existence rather than loading every historical generation for a status check.
        // rows 使用存在性判断，不为一次状态检查加载全部历史代次。
        let rows: Vec<CountRow> = self.data_plane.query_rows(
            "SELECT EXISTS(SELECT 1 FROM vce_generations WHERE namespace_id = ? AND state = 'staging') AS count;",
            vec![sqlite_text(&self.binding.namespace_id)],
        ).await?;
        let [row] = rows.as_slice() else {
            return Err(String::from(
                "VCE staging ownership inspection returned no result",
            ));
        };
        Ok(row.count != 0)
    }

    /// Abort only a refresh whose stopped execution is still owned by the calling host.
    /// 仅中止已停止执行、且仍由调用宿主持有的刷新。
    /// `refresh_id` must come from the admitted task, never a guessed prefix or stale project listing.
    /// Returns after exact-attempt aborts settle; committed checkpoints and other attempts survive.
    /// `refresh_id` 必须来自已准入任务，绝不能来自猜测前缀或过期项目列表。
    /// 精确尝试中止确定后返回；已提交检查点及其他尝试保持不变。
    pub async fn abort_refresh_staging(&self, refresh_id: &str) -> Result<(), String> {
        self.require_write_access()?;
        require_operation_id(refresh_id, VcePortOperation::AbortStage)
            .map_err(|error| error.to_string())?;
        // generations belongs only to this exact admitted refresh, not the namespace as a whole.
        // generations 仅属于本次精确已准入刷新，而非整个命名空间。
        let generations: Vec<RefreshGenerationRow> = self.data_plane.query_rows(
            "SELECT generation FROM vce_generations WHERE namespace_id = ? AND refresh_id = ? AND state IN ('staging', 'aborted') ORDER BY generation;",
            vec![sqlite_text(&self.binding.namespace_id), sqlite_text(refresh_id)],
        ).await?;
        for row in generations {
            // generation is loaded from the exact persisted attempt, never inferred from current state.
            // generation 从精确持久尝试加载，绝不从当前状态推断。
            let generation = u64::try_from(row.generation)
                .map_err(|_| String::from("VCE owned refresh generation is negative"))?;
            self.abort_stage_inner(IndexAbortRequest {
                operation_id: format!("{refresh_id}:host-abort:{generation}"),
                refresh_id: refresh_id.to_owned(),
                generation,
            })
            .await
            .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// Admit a writer for the exact durable candidate job and predecessor without publishing it.
    /// 为精确持久候选作业及前驱准入写入句柄，但不发布候选。
    /// `base` must match allocation. Returns a separately scoped writer or a stale/foreign-owner error.
    /// `base` 必须匹配分配记录。返回独立作用域写入句柄或过期／归属错误。
    pub async fn admit_rebuild_candidate_writer(
        &self,
        base: VceRebuildCandidateBase,
    ) -> Result<Self, String> {
        self.require_write_access()?;
        if self.rebuild_candidate.is_some()
            || base.job_id.trim().is_empty()
            || base.namespace_id.trim().is_empty()
            || base.checkpoint_version == 0
        {
            return Err(String::from(
                "VCE candidate admission requires an unadmitted handle and exact job/base",
            ));
        }
        sqlite_u64_raw(
            base.checkpoint_version,
            "VCE candidate predecessor checkpoint",
        )?;
        // writer carries explicit candidate authority; cloning the original inactive handle does not grant it.
        // writer 携带明确候选权限；克隆原始非活动句柄不会获得该权限。
        let mut writer = self.clone();
        writer.rebuild_candidate = Some(base);
        writer
            .require_active_binding(VcePortOperation::Stage, false)
            .await
            .map_err(|error| error.to_string())?;
        Ok(writer)
    }

    /// Revoke this exact candidate's publication authority without racing in-flight vector writes with deletion.
    /// 撤销此精确候选的发布权限，不让删除操作与进行中的向量写入竞争。
    /// `base` must identify the owning job. Returns after durable revocation; staged data remains until host drain.
    /// `base` 必须标识所属作业。持久撤销后返回；暂存数据保留至宿主排空写入。
    pub async fn cancel_rebuild_candidate(
        &self,
        base: &VceRebuildCandidateBase,
    ) -> Result<(), String> {
        self.require_write_access()?;
        if base.job_id.trim().is_empty()
            || base.namespace_id.trim().is_empty()
            || base.checkpoint_version == 0
            || self
                .rebuild_candidate
                .as_ref()
                .is_some_and(|admitted| admitted != base)
        {
            return Err(String::from(
                "VCE candidate cancellation requires the exact owning job and predecessor",
            ));
        }
        sqlite_u64_raw(
            base.checkpoint_version,
            "VCE candidate predecessor checkpoint",
        )?;
        // namespace and timestamp are fixed before the atomic state transition.
        // namespace 与时间戳在原子状态迁移之前固定。
        let namespace = sqlite_string_literal(&self.binding.namespace_id);
        let now = current_unix_epoch_millis_i64()?;
        // script does not require a still-current predecessor: even a superseded candidate must remain cancellable.
        // script 不要求前驱仍为当前版本：已过期候选也必须可以取消。
        let script = format!(
            "BEGIN IMMEDIATE;\nINSERT OR ROLLBACK INTO vce_commit_guards(namespace_id, valid) VALUES ('{namespace}', CASE WHEN EXISTS(SELECT 1 FROM vce_rebuild_candidates c JOIN vce_project_bindings b ON b.namespace_id = c.namespace_id WHERE c.namespace_id = '{namespace}' AND c.base_namespace_id = '{base_namespace}' AND c.base_checkpoint_version = {version} AND c.job_id = '{job}' AND c.state IN ('allocating', 'building', 'aborted') AND b.project_id = '{project}' AND b.active = 0 AND NOT EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = b.namespace_id)) THEN 1 ELSE 0 END);\nUPDATE vce_rebuild_candidates SET state = 'aborted', updated_at_unix_ms = {now} WHERE namespace_id = '{namespace}' AND state IN ('allocating', 'building');\nUPDATE vce_project_bindings SET lifecycle_state = 'orphaned', updated_at_unix_ms = {now} WHERE namespace_id = '{namespace}';\nDELETE FROM vce_commit_guards WHERE namespace_id = '{namespace}';\nCOMMIT;",
            base_namespace = sqlite_string_literal(&base.namespace_id),
            version = base.checkpoint_version,
            job = sqlite_string_literal(&base.job_id),
            project = sqlite_string_literal(&self.project_id),
        );
        self.data_plane
            .execute(
                &script,
                Vec::new(),
                "Failed to revoke the VCE candidate publication permission",
            )
            .await
    }

    /// Build the exact persisted write-authority predicate, including the candidate's live predecessor.
    /// 构造精确持久写入权限谓词，包括候选的当前有效前驱。
    /// Takes no parameters; returns escaped SQL reused by preflight and atomic commit admission.
    /// 无参数；返回预检与原子提交准入复用的已转义 SQL。
    fn write_binding_condition(&self) -> String {
        // namespace scopes both physical identity and the durable candidate job ledger.
        // namespace 同时限定物理身份和持久候选作业台账。
        let namespace = sqlite_string_literal(&self.binding.namespace_id);
        // candidate_condition cannot be satisfied by an unrelated inactive or already published binding.
        // candidate_condition 不能由无关非活动绑定或已经发布的绑定满足。
        let candidate_condition = self.rebuild_candidate.as_ref().map(|base| format!(
            " AND EXISTS(SELECT 1 FROM vce_rebuild_candidates c JOIN vce_project_bindings p ON p.namespace_id = c.base_namespace_id JOIN vce_active_checkpoints a ON a.namespace_id = p.namespace_id WHERE c.namespace_id = '{namespace}' AND c.base_namespace_id = '{base_namespace}' AND c.base_checkpoint_version = {version} AND c.job_id = '{job}' AND c.state = 'building' AND p.active = 1 AND a.checkpoint_version = c.base_checkpoint_version AND p.project_id = '{project}' AND p.root_path = '{root}' AND p.root_hash = '{root_hash}') AND NOT EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = '{namespace}')",
            base_namespace = sqlite_string_literal(&base.namespace_id), version = base.checkpoint_version,
            job = sqlite_string_literal(&base.job_id), project = sqlite_string_literal(&self.project_id),
            root = sqlite_string_literal(&self.root_path), root_hash = sqlite_string_literal(&self.root_hash),
        )).unwrap_or_default();
        format!(
            "EXISTS(SELECT 1 FROM vce_project_bindings WHERE namespace_id = '{namespace}' AND active = {active} AND config_revision = {revision} AND physical_fingerprint = '{fingerprint}' AND dimension = {dimension} AND content_table = '{content}' AND path_table = '{path}'){candidate_condition}",
            active = u8::from(self.rebuild_candidate.is_none()),
            revision = self.config_revision,
            fingerprint = sqlite_string_literal(&self.physical_fingerprint),
            dimension = self.dimension,
            content = sqlite_string_literal(&self.content_table),
            path = sqlite_string_literal(&self.path_table),
        )
    }

    /// Verify the active binding or the explicitly admitted unpublished candidate's write authority.
    /// 校验活动绑定，或明确准入的未发布候选写入权限。
    async fn require_active_binding(
        &self,
        operation: VcePortOperation,
        require_ready: bool,
    ) -> Result<(), VcePortError> {
        if self.rebuild_candidate.is_some() {
            if require_ready {
                return Err(port_rejected(
                    operation,
                    "VCE candidate has not been published",
                ));
            }
            // matches checks the same durable predicate used inside the eventual publication transaction.
            // matches 检查与最终发布事务内部相同的持久谓词。
            let matches: Vec<CountRow> = self
                .data_plane
                .query_rows(
                    &format!(
                        "SELECT CASE WHEN ({}) THEN 1 ELSE 0 END AS count;",
                        self.write_binding_condition()
                    ),
                    Vec::new(),
                )
                .await
                .map_err(|message| port_failed(operation, message))?;
            return if matches!(matches.as_slice(), [row] if row.count == 1) {
                Ok(())
            } else {
                Err(port_rejected(
                    operation,
                    "VCE candidate job or predecessor changed; write admission rejected",
                ))
            };
        }
        let rows: Vec<ActiveBindingRow> = self
            .data_plane
            .query_rows(
                "SELECT active, lifecycle_state, config_revision, physical_fingerprint, dimension, content_table, path_table FROM vce_project_bindings WHERE namespace_id = ? LIMIT 1;",
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await
            .map_err(|message| port_failed(operation, message))?;
        let row = rows
            .first()
            .ok_or_else(|| port_rejected(operation, "VCE project binding no longer exists"))?;
        let config_revision = i64::try_from(self.config_revision)
            .map_err(|_| port_rejected(operation, "VCE config revision exceeds storage range"))?;
        if row.active != 1
            || row.config_revision != config_revision
            || row.physical_fingerprint != self.physical_fingerprint
            || row.dimension != i64::from(self.dimension)
            || row.content_table != self.content_table
            || row.path_table != self.path_table
        {
            return Err(port_rejected(
                operation,
                "VCE project binding changed; this runtime handle is stale",
            ));
        }
        if require_ready && row.lifecycle_state != "ready" {
            return Err(port_rejected(
                operation,
                format!("VCE index is not searchable: {}", row.lifecycle_state),
            ));
        }
        Ok(())
    }

    /// Classifies the exact requested Blob set without widening scope.
    /// 在不扩大范围的情况下分类精确请求的 Blob 集合。
    async fn find_missing_inner(
        &self,
        request: FindMissingRequest,
    ) -> Result<FindMissingResponse, VcePortError> {
        require_operation_id(&request.request_id, VcePortOperation::FindMissing)?;
        self.require_active_binding(VcePortOperation::FindMissing, false)
            .await?;
        if request.blob_names.is_empty() {
            return Ok(FindMissingResponse {
                operation_id: request.request_id,
                unknown: BTreeSet::new(),
                nonindexed: BTreeSet::new(),
            });
        }
        let mut known = BTreeMap::<String, String>::new();
        for names in request
            .blob_names
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .chunks(400)
        {
            let sql = format!(
                "SELECT blob_name, status FROM vce_blobs WHERE namespace_id = ? AND blob_name IN ({});",
                placeholders(names.len())
            );
            let mut params = vec![sqlite_text(&self.binding.namespace_id)];
            params.extend(names.iter().map(sqlite_text));
            let rows: Vec<BlobStatusRow> = self
                .data_plane
                .query_rows(&sql, params)
                .await
                .map_err(|message| port_failed(VcePortOperation::FindMissing, message))?;
            known.extend(rows.into_iter().map(|row| (row.blob_name, row.status)));
        }
        let mut unknown = BTreeSet::new();
        let mut nonindexed = BTreeSet::new();
        for blob_name in &request.blob_names {
            match known.get(blob_name).map(String::as_str) {
                Some("ready") => {}
                Some(_) => {
                    nonindexed.insert(blob_name.clone());
                }
                None => {
                    unknown.insert(blob_name.clone());
                }
            }
        }
        Ok(FindMissingResponse {
            operation_id: request.request_id,
            unknown,
            nonindexed,
        })
    }

    /// Persists one complete staging batch and writes non-visible vector rows.
    /// 持久化一个完整暂存批次并写入尚不可见的向量行。
    async fn stage_inner(
        &self,
        request: IndexStageBatch,
    ) -> Result<IndexStageReceipt, VcePortError> {
        self.require_write_access()
            .map_err(|message| port_rejected(VcePortOperation::Stage, message))?;
        require_operation_id(&request.operation_id, VcePortOperation::Stage)?;
        self.require_active_binding(VcePortOperation::Stage, false)
            .await?;
        if request.refresh_id.trim().is_empty() || request.generation == 0 {
            return Err(port_rejected(
                VcePortOperation::Stage,
                "refresh_id and generation must be valid",
            ));
        }
        let _guard = self.mutation_lock.lock().await;
        let mut staged_blob_names = BTreeSet::new();
        for blob in &request.blobs {
            self.validate_indexed_blob(blob, request.generation)?;
            if !staged_blob_names.insert(blob.blob_name.clone()) {
                return Err(port_rejected(
                    VcePortOperation::Stage,
                    "staging batch contains duplicate Blob identities",
                ));
            }
        }
        let now = current_unix_epoch_millis_i64()
            .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        self.data_plane
            .execute(
                r#"INSERT INTO vce_generations(
                    namespace_id, refresh_id, generation, state, operation_id,
                    error, created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?, ?, ?, 'staging', ?, NULL, ?, ?)
                ON CONFLICT(namespace_id, refresh_id, generation) DO UPDATE SET
                    operation_id = excluded.operation_id,
                    updated_at_unix_ms = excluded.updated_at_unix_ms
                WHERE vce_generations.state = 'staging';"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&request.refresh_id),
                    sqlite_u64(request.generation, "VCE generation")?,
                    sqlite_text(&request.operation_id),
                    SqliteValue::Integer(now),
                    SqliteValue::Integer(now),
                ],
                "Failed to register the VCE staging generation",
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        let generation_state = self
            .generation_state(
                &request.refresh_id,
                request.generation,
                VcePortOperation::Stage,
            )
            .await?
            .ok_or_else(|| {
                port_failed(
                    VcePortOperation::Stage,
                    "staging generation was not created",
                )
            })?;
        if generation_state != "staging" {
            return Err(port_rejected(
                VcePortOperation::Stage,
                format!("staging generation is already {generation_state}"),
            ));
        }
        // Persist the actual producer before any vector row can be written; another route cannot adopt it.
        // 在写入任何向量行前持久化实际生产来源；另一路由不得接管该来源。
        self.bind_embedding_stage_origin(&request.refresh_id, request.generation)
            .await
            .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        let mut content_rows = Vec::new();
        let mut path_rows = Vec::new();
        for blob in &request.blobs {
            let (blob_content_rows, blob_path_row) = self
                .build_blob_vector_rows(blob, &request.refresh_id)
                .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
            content_rows.extend(blob_content_rows);
            path_rows.extend(blob_path_row);
        }
        // Both tables must be fully encodable before the first physical write starts.
        // 两张表都必须完整通过编码校验，随后才能开始第一次物理写入。
        let prepared_content = self
            .prepare_vector_rows(&self.content_table, content_rows)
            .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        let prepared_paths = self
            .prepare_vector_rows(&self.path_table, path_rows)
            .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        self.submit_prepared_vector_rows(prepared_content)
            .await
            .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        self.submit_prepared_vector_rows(prepared_paths)
            .await
            .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        self.replace_staged_blob(&request.blobs, &request.refresh_id)
            .await
            .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        let staged_count = u64::try_from(staged_blob_names.len()).unwrap_or(u64::MAX);
        // The bounded storage call finished; only the host receipt owns the continuing refresh job.
        // 有界存储调用已经结束；只有宿主回执持有仍在继续的刷新作业。
        self.record_operation(OperationRecord {
            operation_id: &request.operation_id,
            kind: VceOperationKind::StorageMutation,
            phase: VceOperationPhase::Completed,
            generation: request.generation,
            completed_units: staged_count,
            total_units: staged_count,
            error: None,
        })
        .await
        .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        Ok(IndexStageReceipt {
            operation_id: request.operation_id,
            refresh_id: request.refresh_id,
            generation: request.generation,
            staged_blob_names: staged_blob_names.into_iter().collect(),
        })
    }

    /// Aborts staged relational rows and best-effort removes unpublished vectors.
    /// 中止暂存关系行并尽力删除尚未发布的向量。
    async fn abort_stage_inner(
        &self,
        request: IndexAbortRequest,
    ) -> Result<IndexAbortReceipt, VcePortError> {
        self.require_write_access()
            .map_err(|message| port_rejected(VcePortOperation::AbortStage, message))?;
        require_operation_id(&request.operation_id, VcePortOperation::AbortStage)?;
        if request.refresh_id.trim().is_empty() || request.generation == 0 {
            return Err(port_rejected(
                VcePortOperation::AbortStage,
                "refresh_id and generation must be valid",
            ));
        }
        let _guard = self.mutation_lock.lock().await;
        // A successful abort receipt promises this exact attempt can never publish, even before staging.
        // 成功中止回执承诺此精确尝试永不再发布，即使中止发生在暂存之前。
        // Insert a tombstone only when absent; existing staging or committed states remain authoritative.
        // 仅在缺席时插入终止记录；已有暂存或提交状态保持权威。
        let abort_recorded_at = current_unix_epoch_millis_i64()
            .map_err(|message| port_failed(VcePortOperation::AbortStage, message))?;
        self.data_plane.execute(
            "INSERT INTO vce_generations(namespace_id, refresh_id, generation, state, operation_id, error, created_at_unix_ms, updated_at_unix_ms) VALUES (?, ?, ?, 'aborted', ?, NULL, ?, ?) ON CONFLICT(namespace_id, refresh_id, generation) DO NOTHING;",
            vec![
                sqlite_text(&self.binding.namespace_id),
                sqlite_text(&request.refresh_id),
                sqlite_u64_raw(request.generation, "VCE abort generation")
                    .map_err(|message| port_rejected(VcePortOperation::AbortStage, message))?,
                sqlite_text(&request.operation_id),
                SqliteValue::Integer(abort_recorded_at),
                SqliteValue::Integer(abort_recorded_at),
            ],
            "Failed to record the VCE abort identity",
        ).await.map_err(|message| port_failed(VcePortOperation::AbortStage, message))?;
        match self
            .generation_state(
                &request.refresh_id,
                request.generation,
                VcePortOperation::AbortStage,
            )
            .await?
            .as_deref()
        {
            Some("committed") => {
                return Err(port_rejected(
                    VcePortOperation::AbortStage,
                    "a committed generation cannot be aborted",
                ));
            }
            Some("aborted") => {
                self.recover_pending_vector_cleanup()
                    .await
                    .map_err(|message| port_failed(VcePortOperation::AbortStage, message))?;
                return Ok(IndexAbortReceipt {
                    operation_id: request.operation_id,
                    refresh_id: request.refresh_id,
                    generation: request.generation,
                    removed_blob_count: 0,
                });
            }
            Some("staging") => {}
            Some(state) => {
                return Err(port_invalid(
                    VcePortOperation::AbortStage,
                    format!("unknown staging generation state `{state}`"),
                ));
            }
            None => {
                return Err(port_invalid(
                    VcePortOperation::AbortStage,
                    "abort generation disappeared after durable identity admission",
                ));
            }
        }
        let cleanup_id = vector_cleanup_id(
            &self.binding.namespace_id,
            "aborted_generation",
            &format!("{}:{}", request.refresh_id, request.generation),
        );
        let now = current_unix_epoch_millis_i64()
            .map_err(|message| port_failed(VcePortOperation::AbortStage, message))?;
        let rows: Vec<CountRow> = self
            .data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_staged_blobs WHERE namespace_id = ? AND refresh_id = ? AND generation = ?;",
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&request.refresh_id),
                    sqlite_u64(request.generation, "VCE generation")?,
                ],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::AbortStage, message))?;
        let namespace_id = sqlite_string_literal(&self.binding.namespace_id);
        let operation_id = sqlite_string_literal(&request.operation_id);
        let refresh_id = sqlite_string_literal(&request.refresh_id);
        let cleanup_id_sql = sqlite_string_literal(&cleanup_id);
        let physical_fingerprint = sqlite_string_literal(&self.physical_fingerprint);
        let content_table = sqlite_string_literal(&self.content_table);
        let path_table = sqlite_string_literal(&self.path_table);
        let generation = request.generation;
        let script = format!(
            r#"BEGIN IMMEDIATE;
               INSERT OR ROLLBACK INTO vce_commit_guards(namespace_id, valid)
               VALUES ('{namespace_id}', CASE WHEN EXISTS(SELECT 1 FROM vce_generations
                   WHERE namespace_id = '{namespace_id}' AND refresh_id = '{refresh_id}'
                     AND generation = {generation} AND state = 'staging') THEN 1 ELSE 0 END);
               UPDATE vce_generations SET state = 'aborted', operation_id = '{operation_id}', updated_at_unix_ms = {now}
               WHERE namespace_id = '{namespace_id}' AND refresh_id = '{refresh_id}' AND generation = {generation} AND state = 'staging';
               INSERT INTO vce_vector_cleanup(
                   namespace_id, cleanup_id, cleanup_kind, refresh_id, generation,
                   blob_names_json, physical_fingerprint,
                   content_table, path_table, state, attempt, error,
                   created_at_unix_ms, updated_at_unix_ms
               ) VALUES ('{namespace_id}', '{cleanup_id_sql}', 'aborted_generation',
                   '{refresh_id}', {generation}, NULL, '{physical_fingerprint}',
                   '{content_table}', '{path_table}', 'pending', 0, NULL, {now}, {now})
               ON CONFLICT(namespace_id, cleanup_id) DO UPDATE SET
                   state = 'pending', error = NULL, updated_at_unix_ms = excluded.updated_at_unix_ms;
               DELETE FROM vce_staged_blobs
               WHERE namespace_id = '{namespace_id}' AND refresh_id = '{refresh_id}' AND generation = {generation};
               DELETE FROM vce_commit_guards WHERE namespace_id = '{namespace_id}';
               COMMIT;"#,
        );
        self.data_plane
            .execute(
                &script,
                Vec::new(),
                "Failed to abort the VCE staging transaction",
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::AbortStage, message))?;
        self.record_operation(OperationRecord {
            operation_id: &request.operation_id,
            kind: VceOperationKind::StorageMutation,
            phase: VceOperationPhase::Cancelled,
            generation: request.generation,
            completed_units: 0,
            total_units: 0,
            error: None,
        })
        .await
        .map_err(|message| port_failed(VcePortOperation::AbortStage, message))?;
        self.recover_pending_vector_cleanup()
            .await
            .map_err(|message| port_failed(VcePortOperation::AbortStage, message))?;
        Ok(IndexAbortReceipt {
            operation_id: request.operation_id,
            refresh_id: request.refresh_id,
            generation: request.generation,
            removed_blob_count: rows
                .first()
                .and_then(|row| usize::try_from(row.count).ok())
                .unwrap_or(0),
        })
    }

    /// Commit one checkpoint with its exact host publication lease in the same durable transaction.
    /// 在同一持久事务中提交一个检查点及其精确宿主发布租约。
    /// `request` owns staged state; `owner_run_id` identifies the host. Returns receipt and pinned reads.
    /// `request` 持有暂存状态；`owner_run_id` 标识宿主。返回回执和固定读取视图。
    pub async fn commit_pinned(
        &self,
        request: IndexCommit,
        owner_run_id: &str,
    ) -> Result<(CommitReceipt, Self), VcePortError> {
        // read_index is mandatory for this explicitly pinned variant, unlike the plain core port.
        // 与普通核心端口不同，此显式固定变体必需包含 read_index。
        let (receipt, read_index) = self.commit_inner(request, Some(owner_run_id)).await?;
        read_index.map(|index| (receipt, index)).ok_or_else(|| {
            port_invalid(
                VcePortOperation::Commit,
                "VCE pinned commit omitted its publication lease",
            )
        })
    }

    /// Persist the exact commit intent before the atomic publication transaction starts.
    /// 在原子发布事务开始前持久化精确的提交意图。
    async fn persist_publication_intent(
        &self,
        request: &IndexCommit,
        request_fingerprint: &str,
    ) -> Result<(), VcePortError> {
        let existing: Vec<PublicationIntentRow> = self
            .data_plane
            .query_rows(
                "SELECT request_fingerprint, stage FROM vce_publication_intents WHERE namespace_id = ? AND operation_id = ? LIMIT 1;",
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&request.request_id),
                ],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::Commit, message))?;
        if let Some(row) = existing.first() {
            if row.request_fingerprint != request_fingerprint {
                return Err(port_rejected(
                    VcePortOperation::Commit,
                    "publication operation identity was reused with a different request",
                ));
            }
            if matches!(row.stage.as_str(), "committed" | "reconciled") {
                return Err(port_rejected(
                    VcePortOperation::Commit,
                    "publication intent is already committed but its receipt is unavailable",
                ));
            }
        }
        let now = current_unix_epoch_millis_i64()
            .map_err(|message| port_failed(VcePortOperation::Commit, message))?;
        self.data_plane
            .execute(
                r#"INSERT INTO vce_publication_intents(
                       namespace_id, operation_id, refresh_id, generation,
                       checkpoint_version, request_fingerprint, stage, error,
                       created_at_unix_ms, heartbeat_at_unix_ms, updated_at_unix_ms
                   ) VALUES (?, ?, ?, ?, ?, ?, 'preparing', NULL, ?, ?, ?)
                   ON CONFLICT(namespace_id, operation_id) DO UPDATE SET
                       refresh_id = excluded.refresh_id,
                       generation = excluded.generation,
                       checkpoint_version = excluded.checkpoint_version,
                       stage = 'preparing',
                       error = NULL,
                       heartbeat_at_unix_ms = excluded.heartbeat_at_unix_ms,
                       updated_at_unix_ms = excluded.updated_at_unix_ms
                   WHERE vce_publication_intents.request_fingerprint = excluded.request_fingerprint;"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&request.request_id),
                    sqlite_text(&request.refresh_id),
                    sqlite_u64(request.generation, "VCE publication generation")
                        .map_err(|message| port_failed(VcePortOperation::Commit, message.to_string()))?,
                    sqlite_u64(
                        request.checkpoint.version,
                        "VCE publication checkpoint version",
                    )
                    .map_err(|message| port_failed(VcePortOperation::Commit, message.to_string()))?,
                    sqlite_text(request_fingerprint),
                    SqliteValue::Integer(now),
                    SqliteValue::Integer(now),
                    SqliteValue::Integer(now),
                ],
                "Failed to persist the VCE publication intent",
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::Commit, message))
    }

    /// Preserve a bounded failure diagnostic when atomic publication does not commit.
    /// 原子发布未提交时保留有界的失败诊断。
    async fn fail_publication_intent(
        &self,
        request: &IndexCommit,
        request_fingerprint: &str,
        error: &str,
    ) -> Result<(), String> {
        let diagnostic = error.chars().take(2048).collect::<String>();
        let now = current_unix_epoch_millis_i64()?;
        self.data_plane
            .execute(
                "UPDATE vce_publication_intents SET stage = 'failed', error = ?, heartbeat_at_unix_ms = ?, updated_at_unix_ms = ? WHERE namespace_id = ? AND operation_id = ? AND request_fingerprint = ? AND stage = 'preparing';",
                vec![
                    sqlite_text(&diagnostic),
                    SqliteValue::Integer(now),
                    SqliteValue::Integer(now),
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&request.request_id),
                    sqlite_text(request_fingerprint),
                ],
                "Failed to record the VCE publication intent failure state",
            )
            .await
    }

    /// Publish staged rows under CAS, optionally retaining a host publication in the same transaction.
    /// 在比较交换保护下发布暂存行，并可在同一事务中保留宿主发布租约。
    /// `request` is the core commit; `owner_run_id` is present for host publication. Returns exact outcomes.
    /// `request` 是核心提交；宿主发布时提供 `owner_run_id`。返回精确结果。
    async fn commit_inner(
        &self,
        request: IndexCommit,
        owner_run_id: Option<&str>,
    ) -> Result<(CommitReceipt, Option<Self>), VcePortError> {
        self.require_write_access()
            .map_err(|message| port_rejected(VcePortOperation::Commit, message))?;
        if owner_run_id.is_some_and(|owner| owner.trim().is_empty()) {
            return Err(port_rejected(
                VcePortOperation::Commit,
                "VCE publication requires an exact host run",
            ));
        }
        require_operation_id(&request.request_id, VcePortOperation::Commit)?;
        if self.rebuild_candidate.is_none() {
            self.require_active_binding(VcePortOperation::Commit, false)
                .await?;
        } else if owner_run_id.is_none() || request.expected_checkpoint_version.is_some() {
            return Err(port_rejected(
                VcePortOperation::Commit,
                "VCE candidate requires its first checkpoint and an atomic publication lease",
            ));
        }
        self.validate_commit(&request)?;
        let _guard = self.mutation_lock.lock().await;
        let request_fingerprint = commit_request_fingerprint(&request)?;
        let replay_rows: Vec<CommitFingerprintRow> = self
            .data_plane
            .query_rows(
                "SELECT request_fingerprint FROM vce_commit_receipts WHERE namespace_id = ? AND operation_id = ? LIMIT 1;",
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(&request.request_id),
                ],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::Commit, message))?;
        if let Some(replay) = replay_rows.first() {
            if replay.request_fingerprint != request_fingerprint {
                return Err(port_rejected(
                    VcePortOperation::Commit,
                    "commit operation identity was reused with a different request",
                ));
            }
            // Replay never executes another commit; admission still checks the exact current checkpoint.
            // 重放绝不执行另一次提交；读取准入仍校验精确的当前检查点。
            let read_index = match owner_run_id {
                Some(owner) => Some(
                    self.pin_checkpoint(request.checkpoint.clone(), owner)
                        .await
                        .map_err(|message| port_failed(VcePortOperation::Commit, message))?,
                ),
                None => None,
            };
            return Ok((commit_receipt_from_request(request), read_index));
        }
        self.require_active_binding(VcePortOperation::Commit, false)
            .await?;
        // Empty initialization and deletion-only refreshes have no stage call in the core contract.
        // 核心契约中的空库初始化及纯删除刷新不调用暂存入口。
        // A missing generation is therefore legal only without staged blobs; terminal attempts never revive.
        // 因此仅无暂存块时允许代次缺席；终态尝试绝不复活。
        match self
            .generation_state(
                &request.refresh_id,
                request.generation,
                VcePortOperation::Commit,
            )
            .await?
            .as_deref()
        {
            Some("staging") => {}
            None if request.staged_blob_names.is_empty() => {}
            _ => {
                return Err(port_rejected(
                    VcePortOperation::Commit,
                    "commit requires an active staging attempt or a new unstaged empty attempt",
                ));
            }
        }
        let current = self.current_checkpoint_row().await?;
        let actual_version = current
            .as_ref()
            .map(|row| {
                u64::try_from(row.checkpoint_version).map_err(|_| {
                    port_invalid(
                        VcePortOperation::Commit,
                        "persisted checkpoint version is negative",
                    )
                })
            })
            .transpose()?;
        if actual_version != request.expected_checkpoint_version {
            return Err(port_rejected(
                VcePortOperation::Commit,
                format!(
                    "checkpoint compare-and-swap mismatch: expected {:?}, actual {:?}",
                    request.expected_checkpoint_version, actual_version
                ),
            ));
        }
        // currentMembers decodes the authoritative predecessor selected by the checked CAS version.
        // currentMembers 解码由已校验 CAS 版本选定的权威前驱成员集合。
        let current_members = current
            .as_ref()
            .map(CheckpointRow::checkpoint_members)
            .transpose()
            .map_err(|message| port_invalid(VcePortOperation::Commit, message))?
            .unwrap_or_default();
        let staged = self
            .staged_blob_names(&request.refresh_id, request.generation)
            .await?;
        if staged != request.staged_blob_names {
            return Err(port_rejected(
                VcePortOperation::Commit,
                "staged Blob membership does not match the commit",
            ));
        }
        if !staged.is_empty() {
            self.validate_embedding_stage_origin(&request.refresh_id, request.generation)
                .await
                .map_err(|message| port_rejected(VcePortOperation::Commit, message))?;
        }
        validate_checkpoint_membership_transition(
            &current_members,
            &request.deleted_blobs,
            &staged,
            &request.checkpoint.members,
        )
        .map_err(|reason| port_rejected(VcePortOperation::Commit, reason))?;
        // retiring names exact prior attempts; cleanup must never use a broad Blob-only predicate.
        // retiring 标明精确先前尝试；清理绝不能使用仅按块对象的宽泛谓词。
        let retiring = self.retiring_members(&request).await?;
        // Prepare all fallible read-view work before the transaction becomes durable.
        // 在事务持久化之前完成读取视图的全部可失败准备工作。
        let lease = match owner_run_id {
            Some(owner) => Some(
                self.prepare_publication_lease(&request, owner)
                    .await
                    .map_err(|message| port_failed(VcePortOperation::Commit, message))?,
            ),
            None => None,
        };
        let script =
            self.build_commit_script(&request, &request_fingerprint, &retiring, lease.as_ref())?;
        self.persist_publication_intent(&request, &request_fingerprint)
            .await?;
        if let Err(message) = self
            .data_plane
            .execute(&script, Vec::new(), "Failed to publish the VCE checkpoint")
            .await
        {
            let _ = self
                .fail_publication_intent(&request, &request_fingerprint, &message)
                .await;
            return Err(port_failed(VcePortOperation::Commit, message));
        }
        // Cleanup is derived and must not delay commit acknowledgement or retain the worker lease.
        // 清理属于派生操作，不能延迟提交确认或继续持有作业租约。
        Ok((
            commit_receipt_from_request(request),
            lease.map(read_generations::VceReadLease::into_read_index),
        ))
    }

    /// Searches LanceDB content candidates and materializes authoritative SQLite chunks.
    /// 检索 LanceDB 内容候选并物化权威 SQLite 代码块。
    async fn search_dense_inner(
        &self,
        request: DenseSearchRequest,
    ) -> Result<VceSearchResponse, VcePortError> {
        require_operation_id(&request.request_id, VcePortOperation::DenseSearch)?;
        self.require_read_view(VcePortOperation::DenseSearch)?;
        validate_search_scope(
            &request.allowed_blobs,
            request.limit,
            &request.query_vector.values,
        )
        .map_err(|reason| port_rejected(VcePortOperation::DenseSearch, reason))?;
        let rows = self
            .search_vectors(
                &self.content_table,
                &request.query_vector.values,
                &request.allowed_blobs,
                request.limit,
                VcePortOperation::DenseSearch,
            )
            .await?;
        let hits = self
            .materialize_chunk_hits(rows, &request.query_vector.values, SearchSource::Dense)
            .await?;
        Ok(VceSearchResponse {
            operation_id: request.request_id,
            hits,
        })
    }

    /// Searches path vectors and converts committed path documents into hits.
    /// 检索路径向量并把已提交路径文档转换为命中。
    async fn search_path_inner(
        &self,
        request: PathSearchRequest,
    ) -> Result<VcePathSearchResponse, VcePortError> {
        require_operation_id(&request.request_id, VcePortOperation::PathSearch)?;
        // view pins every path document to the same member version as its vector candidate.
        // view 将每份路径文档固定到与其向量候选相同的成员版本。
        let view = self.require_read_view(VcePortOperation::PathSearch)?;
        validate_search_scope(
            &request.allowed_blobs,
            request.limit,
            &request.query_vector.values,
        )
        .map_err(|reason| port_rejected(VcePortOperation::PathSearch, reason))?;
        let rows = self
            .search_vectors(
                &self.path_table,
                &request.query_vector.values,
                &request.allowed_blobs,
                request.limit,
                VcePortOperation::PathSearch,
            )
            .await?;
        let mut hits = Vec::new();
        for row in rows {
            let metadata: Vec<PathHitRow> = self
                .data_plane
                .query_rows(
                    "SELECT b.blob_name, b.path, b.path_document FROM vce_blob_versions b JOIN vce_read_members m ON m.namespace_id = b.namespace_id AND m.refresh_id = b.refresh_id AND m.generation = b.generation AND m.blob_name = b.blob_name WHERE m.namespace_id = ? AND m.checkpoint_version = ? AND m.blob_name = ? AND b.path_document IS NOT NULL;",
                    vec![sqlite_text(&self.binding.namespace_id), sqlite_u64(view.checkpoint.version, "VCE path read checkpoint")?, sqlite_text(&row.blob_name)],
                )
                .await
                .map_err(|message| port_failed(VcePortOperation::PathSearch, message))?;
            if let [metadata] = metadata.as_slice() {
                let score = cosine_similarity(&request.query_vector.values, &row.vector)?;
                let document = PathDocument {
                    path_id: row.id,
                    blob_name: metadata.blob_name.clone(),
                    path: metadata.path.clone(),
                    document: metadata.path_document.clone(),
                };
                hits.push(document.to_hit(score).map_err(|error| {
                    port_invalid(VcePortOperation::PathSearch, error.to_string())
                })?);
            } else {
                return Err(port_invalid(
                    VcePortOperation::PathSearch,
                    "VCE pinned path candidate has no exact immutable document",
                ));
            }
        }
        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });
        hits.truncate(request.limit);
        Ok(VcePathSearchResponse {
            operation_id: request.request_id,
            hits,
            fallback_hits: Vec::new(),
        })
    }

    /// Searches exact identifier rows and returns bounded source occurrences.
    /// 检索精确标识符行并返回有界源码出现位置。
    async fn search_exact_inner(
        &self,
        request: ExactSearchRequest,
    ) -> Result<VceSearchResponse, VcePortError> {
        require_operation_id(&request.request_id, VcePortOperation::ExactSearch)?;
        // view protects old symbol contents and line numbers after another checkpoint is published.
        // view 在另一检查点发布后保护旧符号正文和行号。
        let view = self.require_read_view(VcePortOperation::ExactSearch)?;
        if request.limit == 0
            || request.limit > VCE_VECTOR_SEARCH_LIMIT as usize
            || request.allowed_blobs.is_empty()
            || request
                .identifiers
                .iter()
                .any(|value| value.trim().is_empty())
        {
            return Err(port_rejected(
                VcePortOperation::ExactSearch,
                "exact search identifiers, Blob scope, and limit must be valid",
            ));
        }
        // allowedBlobPlaceholders applies the caller's committed scope before SQLite's bounded row limit.
        // allowedBlobPlaceholders 在 SQLite 有界行数限制之前应用调用方的已提交范围。
        let allowed_blob_placeholders = placeholders(request.allowed_blobs.len());
        if !request.allowed_blobs.is_subset(&view.checkpoint.members) {
            return Err(port_rejected(
                VcePortOperation::ExactSearch,
                "VCE exact scope is outside the pinned checkpoint",
            ));
        }
        let mut hits = Vec::new();
        let mut seen = BTreeSet::new();
        for identifier in &request.identifiers {
            // sql keeps exact-symbol ordering deterministic without admitting out-of-scope rows.
            // sql 在不接纳范围外行的同时保持精确符号排序确定性。
            let sql = format!(
                r#"SELECT s.occurrence_id, s.identifier, s.blob_name, s.path, s.start_line, s.end_line,
                          s.start_char_offset, s.chunk_id, c.content, s.kind
                   FROM vce_symbol_versions s JOIN vce_read_members m
                   ON m.namespace_id = s.namespace_id AND m.refresh_id = s.refresh_id
                     AND m.generation = s.generation AND m.blob_name = s.blob_name
                   JOIN vce_chunk_versions c ON c.namespace_id=s.namespace_id AND c.refresh_id=s.refresh_id
                     AND c.generation=s.generation AND c.chunk_id=s.chunk_id AND c.blob_name=s.blob_name
                   WHERE m.namespace_id = ? AND m.checkpoint_version = ?
                     AND s.identifier = ? AND m.blob_name IN ({allowed_blob_placeholders})
                   ORDER BY CASE s.kind WHEN 'endpoint' THEN 0 WHEN 'definition' THEN 1 ELSE 2 END,
                            s.path, s.start_line, s.occurrence_id
                   LIMIT 5000;"#,
            );
            // params binds every scope identity as data instead of interpolating caller input.
            // params 将每个范围身份作为数据绑定，而不插值调用方输入。
            let mut params = vec![
                sqlite_text(&self.binding.namespace_id),
                sqlite_u64(view.checkpoint.version, "VCE exact read checkpoint")?,
                sqlite_text(identifier),
            ];
            params.extend(request.allowed_blobs.iter().map(sqlite_text));
            let rows: Vec<SymbolHitRow> = self
                .data_plane
                .query_rows(&sql, params)
                .await
                .map_err(|message| port_failed(VcePortOperation::ExactSearch, message))?;
            for row in rows {
                if !request.allowed_blobs.contains(&row.blob_name)
                    || !seen.insert(row.occurrence_id.clone())
                {
                    continue;
                }
                hits.push(row.into_hit()?);
                if hits.len() == request.limit {
                    break;
                }
            }
            if hits.len() == request.limit {
                break;
            }
        }
        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });
        hits.truncate(request.limit);
        Ok(VceSearchResponse {
            operation_id: request.request_id,
            hits,
        })
    }

    /// Builds aggregate counts and checkpoint integrity for management surfaces.
    /// 为管理界面构建聚合数量与检查点完整性。
    async fn management_snapshot_inner(
        &self,
        request: VceManagementRequest,
    ) -> Result<VceIndexManagementSnapshot, VcePortError> {
        require_operation_id(&request.request_id, VcePortOperation::ManagementSnapshot)?;
        let counts = self.index_counts().await?;
        let checkpoint = self.current_checkpoint_row().await?;
        let issues = self.integrity_issues().await?;
        let checkpoint_blob_count = checkpoint
            .as_ref()
            .map(|row| row.checkpoint_members().map(|members| members.len() as u64))
            .transpose()
            .map_err(|message| port_invalid(VcePortOperation::ManagementSnapshot, message))?
            .unwrap_or(0);
        let generation = checkpoint
            .as_ref()
            .map(|row| nonnegative_u64(row.generation, "VCE checkpoint generation"))
            .transpose()?
            .unwrap_or(0);
        let checkpoint_version = checkpoint
            .as_ref()
            .map(|row| nonnegative_u64(row.checkpoint_version, "VCE checkpoint version"))
            .transpose()?;
        let missing_blob_count = issues
            .iter()
            .filter(|issue| issue.kind == VceIntegrityIssueKind::MissingBlob)
            .count() as u64;
        let orphan_blob_count = issues
            .iter()
            .filter(|issue| {
                issue.kind == VceIntegrityIssueKind::OrphanedStaging && issue.blob_name.is_some()
            })
            .count() as u64;
        let active_job = self.latest_index_job().await?;
        let status = if checkpoint.is_some() && missing_blob_count == 0 {
            VceIndexStoreStatus::Ready
        } else if active_job.is_some() {
            VceIndexStoreStatus::Refreshing
        } else {
            VceIndexStoreStatus::Unknown
        };
        Ok(VceIndexManagementSnapshot {
            operation_id: request.request_id,
            status,
            generation,
            checkpoint_version,
            counts,
            integrity: VceIndexIntegrity {
                index_exists: true,
                searchable: checkpoint.is_some() && missing_blob_count == 0,
                checkpoint_blob_count,
                indexed_blob_count: counts.blobs,
                missing_blob_count,
                orphan_blob_count,
                detail: (!issues.is_empty())
                    .then(|| format!("Detected {} VCE index integrity issues", issues.len())),
            },
            active_job,
            last_error: self.latest_operation_error().await?,
        })
    }

    /// Dispatches typed management reads to deterministic SQLite projections.
    /// 将类型化管理读取分派到确定性的 SQLite 投影。
    async fn management_query_inner(
        &self,
        request: VceManagementQueryRequest,
    ) -> Result<VceManagementQueryResponse, VcePortError> {
        require_operation_id(&request.operation_id, VcePortOperation::ManagementQuery)?;
        let result = match request.query {
            VceManagementQuery::Files(query) => {
                VceManagementQueryResult::Files(self.file_page(query).await?)
            }
            VceManagementQuery::BlobDetail { blob_name } => {
                VceManagementQueryResult::BlobDetail(self.blob_detail(&blob_name).await?)
            }
            VceManagementQuery::Operations(page) => {
                VceManagementQueryResult::Operations(self.operation_page(page).await?)
            }
            VceManagementQuery::IntegrityIssues(page) => VceManagementQueryResult::IntegrityIssues(
                page_items(self.integrity_issues().await?, page)?,
            ),
            VceManagementQuery::MaintenancePlan { mode } => {
                VceManagementQueryResult::MaintenancePlan(self.maintenance_plan(mode).await?)
            }
        };
        Ok(VceManagementQueryResponse {
            operation_id: request.operation_id,
            result,
        })
    }

    /// Validates one staged Blob against its generation and vector identities.
    /// 根据代次与向量身份校验一个暂存 Blob。
    fn validate_indexed_blob(
        &self,
        blob: &IndexedBlob,
        generation: u64,
    ) -> Result<(), VcePortError> {
        if blob.blob_name.trim().is_empty()
            || blob.path.trim().is_empty()
            || blob.generation != generation
        {
            return Err(port_rejected(
                VcePortOperation::Stage,
                "indexed Blob identity or generation is invalid",
            ));
        }
        let vectors = blob
            .embedding_vectors
            .iter()
            .map(|vector| (vector.input_id.as_str(), vector.values.as_slice()))
            .collect::<BTreeMap<_, _>>();
        for chunk in &blob.chunks {
            let vector = vectors.get(chunk.chunk_id.as_str());
            if matches!(blob.status, IndexedBlobStatus::Ready) && vector.is_none() {
                return Err(port_rejected(
                    VcePortOperation::Stage,
                    format!("indexed chunk `{}` is missing its vector", chunk.chunk_id),
                ));
            }
            if let Some(vector) = vector {
                validate_vector(vector, self.dimension)
                    .map_err(|reason| port_rejected(VcePortOperation::Stage, reason))?;
            }
        }
        if let Some(vector) = &blob.path_embedding {
            validate_vector(&vector.values, self.dimension)
                .map_err(|reason| port_rejected(VcePortOperation::Stage, reason))?;
        }
        Ok(())
    }

    /// Build all content and optional path rows for one validated Blob without writing storage.
    /// 为一个已校验 Blob 构造全部内容行和可选路径行，但不写入存储。
    fn build_blob_vector_rows(
        &self,
        blob: &IndexedBlob,
        refresh_id: &str,
    ) -> Result<(Vec<serde_json::Value>, Option<serde_json::Value>), String> {
        let mut content_rows = Vec::with_capacity(blob.embedding_vectors.len());
        if !blob.embedding_vectors.is_empty() {
            let chunks = blob
                .chunks
                .iter()
                .map(|chunk| (chunk.chunk_id.as_str(), chunk))
                .collect::<BTreeMap<_, _>>();
            let written_at_unix_ms = current_unix_epoch_millis_i64()?;
            for vector in &blob.embedding_vectors {
                let chunk = chunks.get(vector.input_id.as_str()).ok_or_else(|| {
                    format!(
                        "VCE vector `{}` has no matching Chunk in Blob `{}`",
                        vector.input_id, blob.blob_name
                    )
                })?;
                content_rows.push(serde_json::json!({
                    "row_id": vector_row_id(
                        &self.binding.namespace_id,
                        &self.physical_fingerprint,
                        refresh_id,
                        blob.generation,
                        &vector.input_id,
                    ),
                    "id": vector.input_id,
                    "blob_name": blob.blob_name,
                    "namespace_id": self.binding.namespace_id,
                    "refresh_id": refresh_id,
                    "generation": blob.generation,
                    "vector_space_id": self.vector_space_id,
                    "physical_fingerprint": self.physical_fingerprint,
                    "path": chunk.path,
                    "start_line": chunk.range.start_line,
                    "end_line": chunk.range.end_line,
                    "content_hash": chunk.content_hash,
                    "written_at_unix_ms": written_at_unix_ms,
                    "vector": vector.values,
                }));
            }
        }
        let path_row =
            if let (Some(document), Some(vector)) = (&blob.path_document, &blob.path_embedding) {
                let written_at_unix_ms = current_unix_epoch_millis_i64()?;
                Some(serde_json::json!({
                    "row_id": vector_row_id(
                        &self.binding.namespace_id,
                        &self.physical_fingerprint,
                        refresh_id,
                        blob.generation,
                        &document.path_id,
                    ),
                    "id": document.path_id,
                    "blob_name": blob.blob_name,
                    "namespace_id": self.binding.namespace_id,
                    "refresh_id": refresh_id,
                    "generation": blob.generation,
                    "vector_space_id": self.vector_space_id,
                    "physical_fingerprint": self.physical_fingerprint,
                    "path": document.path,
                    "start_line": 1_u64,
                    "end_line": 1_u64,
                    "content_hash": content_hash(&document.document),
                    "written_at_unix_ms": written_at_unix_ms,
                    "vector": vector.values,
                }))
            } else {
                None
            };
        Ok((content_rows, path_row))
    }

    /// Encodes and partitions rows so every exact engine call stays within its hard limit.
    /// 编码并划分行，确保每个精确引擎调用都处于硬上限以内。
    fn prepare_vector_rows(
        &self,
        table_name: &str,
        rows: Vec<serde_json::Value>,
    ) -> Result<PreparedVectorRows, String> {
        if rows.is_empty() {
            return partition_vector_rows_by_data_limit(table_name, rows, 2);
        }
        // The engine declares this limit on the binding, so the batch size comes from the same declaration
        // the engine itself enforces rather than from a controller round trip.
        // 引擎把该上限声明在绑定上，故批次大小来自引擎自身执行的那份声明，而非一次控制器往返。
        let max_data_bytes = self.data_plane.vector_store_binding().max_upsert_payload;
        partition_vector_rows_by_data_limit(table_name, rows, max_data_bytes)
    }

    /// Submit every prevalidated row batch and wait for all physical writes to finish.
    /// 提交全部预校验行批次，并等待所有物理写入完成。
    async fn submit_prepared_vector_rows(
        &self,
        prepared: PreparedVectorRows,
    ) -> Result<(), String> {
        for data in prepared.batches {
            // The exact row identity index prevents each small file batch from scanning all previous vectors.
            // 精确行身份索引避免每个小文件批次扫描此前全部向量。
            self.data_plane
                .vector_engine()
                .await?
                .maintain_merge_key_index(&prepared.table_name, "row_id")
                .await
                .map_err(|error| error.to_string())?;
            self.data_plane
                .vector_engine()
                .await?
                .vector_upsert(LanceDbUpsertInput {
                    table_name: prepared.table_name.clone(),
                    input_format: LanceDbInputFormat::JsonRows,
                    data,
                    key_columns: vec![String::from("row_id")],
                })
                .await
                .map_err(|error| {
                    format!(
                        "Failed to write VCE LanceDB table `{}`: {error}",
                        prepared.table_name
                    )
                })?;
        }
        Ok(())
    }

    /// Replace a bounded batch of staged Blob records atomically and idempotently.
    /// 原子且幂等地替换有界批次的暂存 Blob 关系记录。
    /// Files belong to the given refresh; returns after all related rows commit together.
    /// 文件归属于指定刷新；全部关联行一同提交后返回。
    async fn replace_staged_blob(
        &self,
        blobs: &[IndexedBlob],
        refresh_id: &str,
    ) -> Result<(), String> {
        // One bounded stage batch owns one connection and one atomic relational transaction.
        // 一个有界暂存批次持有一个连接与一个原子关系事务。
        let blobs = blobs.to_vec();
        let namespace = self.binding.namespace_id.clone();
        let refresh = refresh_id.to_owned();
        local_sql::with_connection_async(&self.data_plane.local_runtime,&self.data_plane.sqlite_binding,move |connection| {
            let transaction = connection.transaction().map_err(|error|error.to_string())?;
            {
                // Prepared statements are reused across files, chunks and symbols.
                // 在文件、代码块与符号之间复用预编译语句。
                let mut delete = transaction.prepare_cached("DELETE FROM vce_staged_blobs WHERE namespace_id=? AND refresh_id=? AND generation=? AND blob_name=?").map_err(|error|error.to_string())?;
                let mut insert_blob = transaction.prepare_cached("INSERT INTO vce_staged_blobs(namespace_id,refresh_id,generation,blob_name,path,structure_analyzer_id,structure_mode,structure_fallback_reason,path_document,path_status,path_error,symbol_status,symbol_error,status,status_reason,chunk_count,vector_count,symbol_count) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").map_err(|error|error.to_string())?;
                let mut insert_chunk = transaction.prepare_cached("INSERT INTO vce_staged_chunks(namespace_id,refresh_id,generation,blob_name,chunk_id,content_hash,path,content,start_line,end_line,start_char_offset,kind,vector_ready) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)").map_err(|error|error.to_string())?;
                let mut insert_symbol = transaction.prepare_cached("INSERT INTO vce_staged_symbols(namespace_id,refresh_id,generation,blob_name,occurrence_id,identifier,path,start_line,end_line,start_char_offset,chunk_id,kind) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)").map_err(|error|error.to_string())?;
                for blob in &blobs {
                    let generation = i64::try_from(blob.generation).map_err(|error|error.to_string())?;
                    let (path_status,path_error) = auxiliary_status_fields(&blob.path_status);
                    let (symbol_status,symbol_error) = auxiliary_status_fields(&blob.symbol_status);
                    let (status,status_reason) = indexed_status_fields(&blob.status);
                    delete.execute(rusqlite::params![namespace,refresh,generation,blob.blob_name]).map_err(|error|error.to_string())?;
                    insert_blob.execute(rusqlite::params![namespace,refresh,generation,blob.blob_name,blob.path,blob.structure_analyzer_id,blob.structure_mode.map(structure_mode_text),blob.structure_fallback_reason,blob.path_document.as_ref().map(|document|document.document.as_str()),path_status,path_error,symbol_status,symbol_error,status,status_reason,blob.chunks.len() as i64,(blob.embedding_vectors.len()+usize::from(blob.path_embedding.is_some())) as i64,blob.symbols.len() as i64]).map_err(|error|error.to_string())?;
                    // Membership is built once per file; no per-row database lookup is needed.
                    // 每文件只构建一次成员集合；无需逐行数据库检索。
                    let vector_ids = blob.embedding_vectors.iter().map(|vector|vector.input_id.as_str()).collect::<BTreeSet<_>>();
                    for chunk in &blob.chunks {
                        let parameters = staged_chunk_params(&namespace,&refresh,blob.generation,chunk,vector_ids.contains(chunk.chunk_id.as_str()))?;
                        insert_chunk.execute(rusqlite::params_from_iter(parameters.iter())).map_err(|error|error.to_string())?;
                    }
                    for symbol in &blob.symbols {
                        let parameters = staged_symbol_params(&namespace,&refresh,blob.generation,symbol)?;
                        insert_symbol.execute(rusqlite::params_from_iter(parameters.iter())).map_err(|error|error.to_string())?;
                    }
                }
            }
            transaction.commit().map_err(|error|error.to_string())
        }).await
    }

    /// Validates checkpoint identity and membership before storage mutation.
    /// 在修改存储前校验检查点身份与成员关系。
    fn validate_commit(&self, request: &IndexCommit) -> Result<(), VcePortError> {
        request
            .state_snapshot
            .validate()
            .map_err(|error| port_rejected(VcePortOperation::Commit, error.to_string()))?;
        let state_identity = &request.state_snapshot.identity;
        let state_workspace = &request.state_snapshot.workspace;
        let state_refresh = request.state_snapshot.last_refresh.as_ref();
        if request.refresh_id.trim().is_empty()
            || request.generation == 0
            || request.checkpoint.chain_id != self.binding.checkpoint_chain_id
            || request.checkpoint.version == 0
            || request
                .staged_blob_names
                .iter()
                .any(|blob| !request.checkpoint.members.contains(blob))
            || request
                .deleted_blobs
                .iter()
                .any(|blob| request.checkpoint.members.contains(blob))
            || state_identity.index_namespace_id != self.binding.namespace_id
            || state_identity.checkpoint_chain_id != self.binding.checkpoint_chain_id
            || state_identity.index_schema_version != self.binding.schema_version
            || state_identity.embedding_space != self.binding.embedding_space
            || state_workspace.generation != request.generation
            || state_workspace.checkpoint.as_ref() != Some(&request.checkpoint)
            || state_refresh.is_none_or(|summary| {
                summary.request_id != request.refresh_id
                    || summary.generation != request.generation
                    || summary.checkpoint_version != Some(request.checkpoint.version)
                    || summary.deleted_blob_count != request.deleted_blobs.len()
                    || summary.indexed_blob_count != request.staged_blob_names.len()
                    || summary.source_revision != request.source_revision
            })
        {
            return Err(port_rejected(
                VcePortOperation::Commit,
                "commit identity, generation, chain, version, membership, or state snapshot is invalid",
            ));
        }
        Ok(())
    }

    /// Loads the current authoritative checkpoint row.
    /// 加载当前权威检查点行。
    async fn current_checkpoint_row(&self) -> Result<Option<CheckpointRow>, VcePortError> {
        self.query_management_rows(
                "SELECT checkpoint_version, generation, checkpoint_json FROM vce_checkpoints WHERE namespace_id = ? LIMIT 1;",
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await
            .map(|rows: Vec<CheckpointRow>| rows.into_iter().next())
            .map_err(|message| port_failed(VcePortOperation::ManagementSnapshot, message))
    }

    /// Loads the exact staged Blob membership for one transaction.
    /// 加载一个事务的精确暂存 Blob 成员关系。
    async fn staged_blob_names(
        &self,
        refresh_id: &str,
        generation: u64,
    ) -> Result<BTreeSet<String>, VcePortError> {
        let rows: Vec<BlobNameRow> = self
            .data_plane
            .query_rows(
                "SELECT blob_name FROM vce_staged_blobs WHERE namespace_id = ? AND refresh_id = ? AND generation = ? ORDER BY blob_name;",
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(refresh_id),
                    sqlite_u64(generation, "VCE generation")?,
                ],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::Commit, message))?;
        Ok(rows.into_iter().map(|row| row.blob_name).collect())
    }

    /// Loads one exact staging generation state without widening refresh identity.
    /// 在不扩大刷新身份的情况下加载一个精确暂存代次状态。
    async fn generation_state(
        &self,
        refresh_id: &str,
        generation: u64,
        operation: VcePortOperation,
    ) -> Result<Option<String>, VcePortError> {
        let rows: Vec<GenerationStateRow> = self
            .data_plane
            .query_rows(
                "SELECT state FROM vce_generations WHERE namespace_id = ? AND refresh_id = ? AND generation = ? LIMIT 1;",
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(refresh_id),
                    sqlite_u64(generation, "VCE generation")?,
                ],
            )
            .await
            .map_err(|message| port_failed(operation, message))?;
        Ok(rows.into_iter().next().map(|row| row.state))
    }

    /// Builds one SQLite transaction that promotes a fully validated generation.
    /// 构建一个提升完整已校验代次的 SQLite 事务。
    fn build_commit_script(
        &self,
        request: &IndexCommit,
        request_fingerprint: &str,
        // retiring contains immutable predecessor attempts validated before the CAS transaction.
        // retiring 包含在比较交换事务前已验证的不可变前驱尝试。
        retiring: &[read_generations::ReadMemberRow],
        // publication_lease is prepared before commit and retained only with that exact checkpoint.
        // publication_lease 在提交前准备，且仅与该精确检查点一同保留。
        publication_lease: Option<&read_generations::VceReadLease>,
    ) -> Result<String, VcePortError> {
        // Identity literals are escaped once; all counters were validated by the commit contract.
        // 身份字面量仅转义一次；全部计数已由提交契约验证。
        let namespace = sqlite_string_literal(&self.binding.namespace_id);
        let refresh_id = sqlite_string_literal(&request.refresh_id);
        let generation = request.generation;
        let version = request.checkpoint.version;
        let operation_id = sqlite_string_literal(&request.request_id);
        let request_fingerprint = sqlite_string_literal(request_fingerprint);
        let writer_fingerprint = sqlite_string_literal(self.embedding_producer_fingerprint());
        // Both persisted projections are serialized from the same core commit request.
        // 两个持久投影均从同一个核心提交请求序列化。
        let checkpoint_json = serde_json::to_string(&request.checkpoint)
            .map_err(|error| port_failed(VcePortOperation::Commit, error.to_string()))?;
        let checkpoint_json = sqlite_string_literal(&checkpoint_json);
        let state_snapshot_json = serde_json::to_string(&request.state_snapshot)
            .map_err(|error| port_failed(VcePortOperation::Commit, error.to_string()))?;
        let state_snapshot_json = sqlite_string_literal(&state_snapshot_json);
        // source_revision is genuinely optional in the core wire contract.
        // source_revision 在核心线协议中确实可选。
        let source_revision = request
            .source_revision
            .as_deref()
            .map(sqlite_string_literal)
            .map(|value| format!("'{value}'"))
            .unwrap_or_else(|| String::from("NULL"));
        let now = current_unix_epoch_millis_i64()
            .map_err(|message| port_failed(VcePortOperation::Commit, message))?;
        // cas_condition rejects another publication without mutating its checkpoint.
        // cas_condition 拒绝另一发布，不修改其检查点。
        let cas_condition = request.expected_checkpoint_version.map_or_else(
            || format!("NOT EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = '{namespace}')"),
            |expected| format!("EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = '{namespace}' AND checkpoint_version = {expected})"),
        );
        // binding_condition repeats handle admission under the same write lock as checkpoint publication.
        // binding_condition 在检查点发布的同一写锁内重复句柄准入，阻止预检后的绑定切换。
        let binding_condition = self.write_binding_condition();
        // The candidate-ready manifest is fenced by the same SQLite guard as checkpoint publication.
        // 候选就绪清单与检查点发布使用同一个 SQLite 事务栅栏保护。
        let embedding_manifest_condition = if request.finalize_embedding_manifest {
            format!(
                "EXISTS(SELECT 1 FROM vce_embedding_runs WHERE namespace_id = '{namespace}' AND run_id = '{refresh_id}' AND generation = {generation} AND state = 'candidate_ready' AND manifest_complete = 1)"
            )
        } else {
            String::from("1")
        };
        let embedding_manifest_clause = if request.finalize_embedding_manifest {
            format!(
                "UPDATE vce_embedding_runs SET state = 'succeeded', updated_at_unix_ms = {now}, revision = revision + 1 WHERE namespace_id = '{namespace}' AND run_id = '{refresh_id}' AND generation = {generation} AND state = 'candidate_ready' AND manifest_complete = 1;\n"
            )
        } else {
            String::new()
        };
        // candidate_publication switches both bindings only with the first checkpoint, receipt and read lease.
        // candidate_publication 仅随首次检查点、回执及读取租约一同切换两个绑定。
        let candidate_publication = if let Some(base) = &self.rebuild_candidate {
            if publication_lease.is_none() || request.expected_checkpoint_version.is_some() {
                return Err(port_rejected(
                    VcePortOperation::Commit,
                    "VCE candidate publication requires its first checkpoint and read lease",
                ));
            }
            format!(
                "UPDATE vce_project_bindings SET active = 0, lifecycle_state = 'orphaned', updated_at_unix_ms = {now} WHERE namespace_id = '{base_namespace}';\nUPDATE vce_project_bindings SET active = 1, updated_at_unix_ms = {now} WHERE namespace_id = '{namespace}';\nUPDATE vce_rebuild_candidates SET state = 'published', updated_at_unix_ms = {now} WHERE namespace_id = '{namespace}';\n",
                base_namespace = sqlite_string_literal(&base.namespace_id),
            )
        } else {
            String::new()
        };
        // generation_condition repeats admission under the SQLite write lock to fence a concurrent abort.
        // generation_condition 在 SQLite 写锁内重复准入校验，阻止并发中止后的发布。
        let generation_condition = format!(
            "EXISTS(SELECT 1 FROM vce_generations WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation} AND state = 'staging') OR ({empty} AND NOT EXISTS(SELECT 1 FROM vce_generations WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation}))",
            empty = u8::from(request.staged_blob_names.is_empty()),
        );
        // retained_clause copies only unchanged members with their actual original attempt identities.
        // retained_clause 仅复制未变化成员及其实际原始尝试身份。
        let retained_clause = if request.checkpoint.members.is_empty() {
            String::new()
        } else {
            format!(
                r#"INSERT OR ROLLBACK INTO vce_read_members(namespace_id, checkpoint_version, blob_name, refresh_id, generation)
SELECT m.namespace_id, {version}, m.blob_name, m.refresh_id, m.generation
FROM vce_read_members m JOIN vce_active_checkpoints a
ON a.namespace_id = m.namespace_id AND a.checkpoint_version = m.checkpoint_version
WHERE m.namespace_id = '{namespace}' AND m.blob_name IN ({members})
AND NOT EXISTS(SELECT 1 FROM vce_staged_blobs s WHERE s.namespace_id = m.namespace_id
  AND s.refresh_id = '{refresh_id}' AND s.generation = {generation} AND s.blob_name = m.blob_name);
"#,
                members = sql_string_set(&request.checkpoint.members)
            )
        };
        // retirement_clause durably records exact replaced versions before switching the active pointer.
        // retirement_clause 在切换活动指针前持久记录精确被替换版本。
        let mut retirement_clause = String::new();
        for row in retiring {
            // seed includes every immutable producer key, so retries never alias another version.
            // seed 包含全部不可变生产键，使重试绝不别名到另一版本。
            let seed = serde_json::to_string(&(&row.refresh_id, row.generation, &row.blob_name))
                .map_err(|error| port_failed(VcePortOperation::Commit, error.to_string()))?;
            let cleanup_id = vector_cleanup_id(&self.binding.namespace_id, "retired_blob", &seed);
            let blobs = serde_json::to_string(&[&row.blob_name])
                .map_err(|error| port_failed(VcePortOperation::Commit, error.to_string()))?;
            retirement_clause.push_str(&format!(
                "INSERT OR ROLLBACK INTO vce_vector_cleanup(namespace_id, cleanup_id, cleanup_kind, refresh_id, generation, blob_names_json, physical_fingerprint, content_table, path_table, state, attempt, error, created_at_unix_ms, updated_at_unix_ms) VALUES ('{namespace}', '{}', 'retired_blob', '{}', {}, '{}', '{}', '{}', '{}', 'pending', 0, NULL, {now}, {now}) ON CONFLICT(namespace_id, cleanup_id) DO NOTHING;\n",
                sqlite_string_literal(&cleanup_id), sqlite_string_literal(&row.refresh_id), row.generation,
                sqlite_string_literal(&blobs), sqlite_string_literal(&self.physical_fingerprint),
                sqlite_string_literal(&self.content_table), sqlite_string_literal(&self.path_table),
            ));
        }
        // lease_clause removes the fallible post-commit read-admission window entirely.
        // lease_clause 完全消除提交后读取准入可失败的窗口。
        let lease_clause = publication_lease
            .map(|lease| lease.commit_clause(now))
            .unwrap_or_default();
        Ok(format!(
            r#"BEGIN IMMEDIATE;
INSERT OR ROLLBACK INTO vce_commit_guards(namespace_id, valid)
VALUES ('{namespace}', CASE WHEN ({binding_condition}) AND ({cas_condition}) AND ({generation_condition}) AND ({embedding_manifest_condition}) THEN 1 ELSE 0 END);
INSERT OR ROLLBACK INTO vce_read_checkpoints(
 namespace_id, checkpoint_version, generation, refresh_id, checkpoint_json, source_revision, committed_at_unix_ms)
VALUES ('{namespace}', {version}, {generation}, '{refresh_id}', '{checkpoint_json}', {source_revision}, {now});
INSERT OR ROLLBACK INTO vce_blob_versions(
 namespace_id, refresh_id, generation, blob_name, path, structure_analyzer_id, structure_mode,
 structure_fallback_reason, path_document, path_status, path_error, symbol_status, symbol_error,
 status, status_reason, chunk_count, vector_count, symbol_count, embedding_producer_fingerprint)
SELECT namespace_id, refresh_id, generation, blob_name, path, structure_analyzer_id, structure_mode,
 structure_fallback_reason, path_document, path_status, path_error, symbol_status, symbol_error,
 status, status_reason, chunk_count, vector_count, symbol_count,
 CASE WHEN vector_count > 0 OR path_status = 'ready' THEN '{writer_fingerprint}' ELSE NULL END
FROM vce_staged_blobs WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
INSERT OR ROLLBACK INTO vce_chunk_versions(
 namespace_id, refresh_id, generation, blob_name, chunk_id, content_hash, path, content,
 start_line, end_line, start_char_offset, kind, vector_ready)
SELECT namespace_id, refresh_id, generation, blob_name, chunk_id, content_hash, path, content,
 start_line, end_line, start_char_offset, kind, vector_ready
FROM vce_staged_chunks WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
INSERT OR ROLLBACK INTO vce_symbol_versions(
 namespace_id, refresh_id, generation, blob_name, occurrence_id, identifier, path, start_line,
 end_line, start_char_offset, chunk_id, kind)
SELECT namespace_id, refresh_id, generation, blob_name, occurrence_id, identifier, path, start_line,
 end_line, start_char_offset, chunk_id, kind
FROM vce_staged_symbols WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
{retained_clause}
INSERT OR ROLLBACK INTO vce_read_members(namespace_id, checkpoint_version, blob_name, refresh_id, generation)
SELECT namespace_id, {version}, blob_name, refresh_id, generation FROM vce_staged_blobs
WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
DELETE FROM vce_commit_guards WHERE namespace_id = '{namespace}';
INSERT OR ROLLBACK INTO vce_commit_guards(namespace_id, valid)
VALUES ('{namespace}', CASE WHEN (SELECT COUNT(*) FROM vce_read_members
 WHERE namespace_id = '{namespace}' AND checkpoint_version = {version}) = {member_count} THEN 1 ELSE 0 END);
INSERT OR ROLLBACK INTO vce_embedding_generation_origins(
 namespace_id, refresh_id, generation, physical_fingerprint, checkpoint_version, committed_at_unix_ms)
SELECT '{namespace}', '{refresh_id}', {generation}, '{writer_fingerprint}', {version}, {now}
WHERE EXISTS(SELECT 1 FROM vce_staged_blobs WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}'
 AND generation = {generation} AND (vector_count > 0 OR path_status = 'ready'));
{retirement_clause}
INSERT OR ROLLBACK INTO vce_active_checkpoints(namespace_id, checkpoint_version)
VALUES ('{namespace}', {version})
ON CONFLICT(namespace_id) DO UPDATE SET checkpoint_version = excluded.checkpoint_version;
INSERT OR ROLLBACK INTO vce_state_snapshots(namespace_id, snapshot_json, updated_at_unix_ms)
VALUES ('{namespace}', '{state_snapshot_json}', {now})
ON CONFLICT(namespace_id) DO UPDATE SET snapshot_json = excluded.snapshot_json, updated_at_unix_ms = excluded.updated_at_unix_ms;
{embedding_manifest_clause}
INSERT OR ROLLBACK INTO vce_commit_receipts(namespace_id, operation_id, request_fingerprint, created_at_unix_ms)
VALUES ('{namespace}', '{operation_id}', '{request_fingerprint}', {now});
INSERT OR ROLLBACK INTO vce_generations(
 namespace_id, refresh_id, generation, state, operation_id, error, created_at_unix_ms, updated_at_unix_ms)
VALUES ('{namespace}', '{refresh_id}', {generation}, 'committed', '{operation_id}', NULL, {now}, {now})
ON CONFLICT(namespace_id, refresh_id, generation) DO UPDATE SET
 state = 'committed', operation_id = excluded.operation_id, error = NULL, updated_at_unix_ms = excluded.updated_at_unix_ms
WHERE vce_generations.state = 'staging';
INSERT OR ROLLBACK INTO vce_operations(
 namespace_id, operation_id, kind, phase, generation, completed_units, total_units,
 attempt, coalesced, error, created_at_unix_ms, updated_at_unix_ms)
VALUES ('{namespace}', '{operation_id}', 'storage_mutation', 'completed', {generation},
 {indexed_count}, {indexed_count}, 1, 0, NULL, {now}, {now})
ON CONFLICT(namespace_id, operation_id) DO UPDATE SET phase = 'completed', generation = excluded.generation,
 completed_units = excluded.completed_units, total_units = excluded.total_units, error = NULL, updated_at_unix_ms = excluded.updated_at_unix_ms;
UPDATE vce_project_bindings SET lifecycle_state = 'ready', updated_at_unix_ms = {now} WHERE namespace_id = '{namespace}';
DELETE FROM vce_staged_blobs WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
DELETE FROM vce_commit_guards WHERE namespace_id = '{namespace}';
UPDATE vce_publication_intents
SET stage = 'committed', error = NULL, heartbeat_at_unix_ms = {now}, updated_at_unix_ms = {now}
WHERE namespace_id = '{namespace}' AND operation_id = '{operation_id}'
  AND request_fingerprint = '{request_fingerprint}' AND stage = 'preparing';
{lease_clause}
{candidate_publication}
COMMIT;"#,
            member_count = request.checkpoint.members.len(),
            indexed_count = request.staged_blob_names.len(),
        ))
    }

    /// Executes pending derived-vector cleanup requests and preserves failures for retry.
    /// 执行待处理派生向量清理请求，并持久保留失败以供重试。
    async fn recover_pending_vector_cleanup(&self) -> Result<(), String> {
        self.require_write_access()?;
        self.data_plane.flush_released_read_leases().await?;
        // Only unleased predecessor mappings may be retired before exact version reclamation.
        // 只有无租约的前驱映射可以在精确版本回收前退役。
        self.retire_unleased_checkpoints().await?;
        let rows: Vec<VectorCleanupRow> = self
            .data_plane
            .query_rows(
                "SELECT cleanup_id, cleanup_kind, refresh_id, generation, blob_names_json, physical_fingerprint, content_table, path_table FROM vce_vector_cleanup WHERE namespace_id = ? AND state = 'pending' ORDER BY created_at_unix_ms, cleanup_id;",
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await?;
        let mut failures = Vec::new();
        for row in rows {
            let outcome = self.execute_vector_cleanup(&row).await;
            let now = current_unix_epoch_millis_i64()?;
            match outcome {
                Ok(true) => {
                    self.data_plane
                        .execute(
                            "UPDATE vce_vector_cleanup SET state = 'completed', attempt = attempt + 1, error = NULL, updated_at_unix_ms = ? WHERE namespace_id = ? AND cleanup_id = ?;",
                            vec![
                                SqliteValue::Integer(now),
                                sqlite_text(&self.binding.namespace_id),
                                sqlite_text(&row.cleanup_id),
                            ],
                            "Failed to complete the VCE vector cleanup record",
                        )
                        .await?;
                }
                // A live read is expected retention, not a cleanup failure or a completed deletion.
                // 活动读取是预期保留，不是清理失败，也不是删除完成。
                Ok(false) => {}
                Err(error) => {
                    self.data_plane
                        .execute(
                            "UPDATE vce_vector_cleanup SET attempt = attempt + 1, error = ?, updated_at_unix_ms = ? WHERE namespace_id = ? AND cleanup_id = ?;",
                            vec![
                                sqlite_text(&error),
                                SqliteValue::Integer(now),
                                sqlite_text(&self.binding.namespace_id),
                                sqlite_text(&row.cleanup_id),
                            ],
                            "Failed to record the VCE vector cleanup failure diagnostics",
                        )
                        .await?;
                    failures.push(format!("{}: {error}", row.cleanup_id));
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "VCE derived-vector cleanup remains pending: {}",
                failures.join("; ")
            ))
        }
    }

    /// Executes one exact, validated LanceDB cleanup predicate against both channels.
    /// 对两个通道执行一项经过精确校验的 LanceDB 清理谓词。
    async fn execute_vector_cleanup(&self, row: &VectorCleanupRow) -> Result<bool, String> {
        if row.content_table != self.content_table
            || row.path_table != self.path_table
            || row.physical_fingerprint != self.physical_fingerprint
        {
            return Err(String::from(
                "VCE cleanup row does not match its exact physical binding",
            ));
        }
        // Both cleanup kinds identify an exact attempt, never every row of the same Blob.
        // 两种清理类型均标识精确尝试，绝不针对同块对象的全部记录。
        let refresh_id = row
            .refresh_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| String::from("VCE cleanup is missing its refresh identity"))?;
        let generation = row
            .generation
            .filter(|value| *value > 0)
            .ok_or_else(|| String::from("VCE cleanup is missing its positive generation"))?;
        let namespace = lance_string_literal(&self.binding.namespace_id);
        let fingerprint = lance_string_literal(&self.physical_fingerprint);
        let base = format!(
            "namespace_id = '{namespace}' AND physical_fingerprint = '{fingerprint}' AND refresh_id = '{}' AND generation = {generation}",
            lance_string_literal(refresh_id)
        );
        // retired_blob is present only for one immutable committed version's reclamation.
        // retired_blob 仅在回收一个不可变已提交版本时存在。
        let retired_blob = match row.cleanup_kind.as_str() {
            "aborted_generation" => None,
            "retired_blob" => {
                let names = serde_json::from_str::<Vec<String>>(
                    row.blob_names_json
                        .as_deref()
                        .ok_or_else(|| String::from("VCE retired cleanup lacks its exact Blob"))?,
                )
                .map_err(|error| format!("Decode retired VCE Blob: {error}"))?;
                match names.as_slice() {
                    [name] if !name.trim().is_empty() => Some(name.clone()),
                    _ => {
                        return Err(String::from(
                            "VCE retired cleanup must name exactly one Blob",
                        ));
                    }
                }
            }
            other => return Err(format!("Unsupported VCE cleanup kind `{other}`")),
        };
        // parameters are shared by the exact relational protection check and subsequent deletion.
        // parameters 由精确关系保护检查及后续删除共享。
        let mut parameters = vec![
            sqlite_text(&self.binding.namespace_id),
            sqlite_text(refresh_id),
            SqliteValue::Integer(generation),
        ];
        let member_filter = if let Some(blob) = &retired_blob {
            parameters.push(sqlite_text(blob));
            " AND blob_name = ?"
        } else {
            ""
        };
        let protected: Vec<CountRow> = self.data_plane.query_rows(
            &format!("SELECT COUNT(*) AS count FROM vce_read_members WHERE namespace_id = ? AND refresh_id = ? AND generation = ?{member_filter};"),
            parameters.clone(),
        ).await?;
        let [protected] = protected.as_slice() else {
            return Err(String::from(
                "VCE read-protection query returned no exact count",
            ));
        };
        if protected.count != 0 {
            return Ok(false);
        }
        // predicate cannot match a later successful retry or an unchanged retained generation.
        // predicate 不能匹配后续成功重试或未变化的保留代次。
        let predicate = match &retired_blob {
            Some(blob) => format!("{base} AND blob_name = '{}'", lance_string_literal(blob)),
            None => base,
        };
        for table in [&self.content_table, &self.path_table] {
            self.data_plane
                .vector_engine()
                .await?
                .delete(LanceDbDeleteInput {
                    table_name: table.clone(),
                    condition: predicate.clone(),
                })
                .await
                .map_err(|error| format!("Delete exact VCE vectors from `{table}`: {error}"))?;
        }
        if retired_blob.is_some() {
            // Metadata follows successful vector deletion; interruptions remain safely retryable.
            // 元数据在向量成功删除之后清理；中断后仍可安全重试。
            self.data_plane.execute(
                "DELETE FROM vce_blob_versions WHERE namespace_id = ? AND refresh_id = ? AND generation = ? AND blob_name = ? AND NOT EXISTS(SELECT 1 FROM vce_read_members m WHERE m.namespace_id = vce_blob_versions.namespace_id AND m.refresh_id = vce_blob_versions.refresh_id AND m.generation = vce_blob_versions.generation AND m.blob_name = vce_blob_versions.blob_name);",
                parameters, "Failed to reclaim VCE code versions without read references",
            ).await?;
        }
        Ok(true)
    }

    /// Executes one scoped LanceDB vector search and validates returned dimensions.
    /// 执行一次限定范围的 LanceDB 向量检索并校验返回维度。
    async fn search_vectors(
        &self,
        table_name: &str,
        query_vector: &[f32],
        allowed_blobs: &BTreeSet<String>,
        limit: usize,
        operation: VcePortOperation,
    ) -> Result<Vec<LanceHitRow>, VcePortError> {
        // scope applies exact immutable attempts before vector ranking and its bounded result limit.
        // scope 在向量排序及有界结果限制之前应用精确不可变尝试。
        let view = self.require_read_view(operation)?;
        let scope = view
            .vector_scope(allowed_blobs)
            .map_err(|reason| port_rejected(operation, reason))?;
        let physical_limit = u32::try_from(limit)
            .unwrap_or(VCE_VECTOR_SEARCH_LIMIT)
            .clamp(1, VCE_VECTOR_SEARCH_LIMIT);
        let predicate = format!(
            "namespace_id = '{}' AND vector_space_id = '{}' AND physical_fingerprint = '{}' AND ({scope})",
            lance_string_literal(&self.binding.namespace_id),
            lance_string_literal(&self.vector_space_id),
            lance_string_literal(&self.physical_fingerprint),
        );
        let result = self
            .data_plane
            .vector_engine()
            .await
            .map_err(|error| {
                port_failed(
                    operation,
                    format!("Failed to open the VCE LanceDB engine: {error}"),
                )
            })?
            .vector_search(LanceDbSearchInput {
                table_name: table_name.to_string(),
                vector: query_vector.to_vec(),
                limit: physical_limit,
                filter: predicate,
                vector_column: String::from("vector"),
                output_format: LanceDbOutputFormat::JsonRows,
            })
            .await
            .map_err(|error| {
                port_failed(operation, format!("VCE LanceDB search failed: {error}"))
            })?;
        let mut rows: Vec<LanceHitRow> = serde_json::from_slice(&result.data).map_err(|error| {
            port_invalid(
                operation,
                format!("Failed to parse VCE vector candidates: {error}"),
            )
        })?;
        for row in &rows {
            if row.id.trim().is_empty()
                || row.namespace_id != self.binding.namespace_id
                || !allowed_blobs.contains(&row.blob_name)
                || !view.members.get(&row.blob_name).is_some_and(|member| {
                    member.refresh_id == row.refresh_id && member.generation == row.generation
                })
                || row.vector_space_id != self.vector_space_id
                || row.physical_fingerprint != self.physical_fingerprint
                || row.vector.len() != self.dimension as usize
                || row.vector.iter().any(|value| !value.is_finite())
            {
                return Err(port_invalid(
                    operation,
                    "VCE LanceDB returned an out-of-range or invalid candidate",
                ));
            }
        }
        let mut seen_ids = BTreeSet::new();
        rows.retain(|row| seen_ids.insert(row.id.clone()));
        rows.truncate(limit);
        Ok(rows)
    }

    /// Materializes ranked content candidates from authoritative chunk rows.
    /// 从权威代码块行物化已排序内容候选。
    async fn materialize_chunk_hits(
        &self,
        rows: Vec<LanceHitRow>,
        query_vector: &[f32],
        source: SearchSource,
    ) -> Result<Vec<SearchHit>, VcePortError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        // view keeps the candidate lookup on the same checkpoint, even after publication and cleanup.
        // view 在发布和清理之后仍将候选读取保持在同一检查点。
        let view = self.require_read_view(VcePortOperation::DenseSearch)?;
        let identities = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let sql = format!(
            r#"SELECT c.chunk_id, c.blob_name, c.path, c.content, c.content_hash, c.start_line, c.end_line,
                      c.start_char_offset, c.kind
               FROM vce_chunk_versions c JOIN vce_read_members m
               ON m.namespace_id = c.namespace_id AND m.refresh_id = c.refresh_id
                 AND m.generation = c.generation AND m.blob_name = c.blob_name
               WHERE m.namespace_id = ? AND m.checkpoint_version = ? AND c.chunk_id IN ({});"#,
            placeholders(identities.len())
        );
        let mut params = vec![
            sqlite_text(&self.binding.namespace_id),
            sqlite_u64(view.checkpoint.version, "VCE content read checkpoint")?,
        ];
        params.extend(identities.iter().map(sqlite_text));
        let chunks: Vec<ChunkHitRow> = self
            .data_plane
            .query_rows(&sql, params)
            .await
            .map_err(|message| port_failed(VcePortOperation::DenseSearch, message))?;
        let chunks = chunks
            .into_iter()
            .map(|row| (row.chunk_id.clone(), row))
            .collect::<BTreeMap<_, _>>();
        let mut hits = Vec::new();
        for row in rows {
            let Some(chunk) = chunks.get(&row.id) else {
                return Err(port_invalid(
                    VcePortOperation::DenseSearch,
                    "VCE pinned vector candidate has no immutable source chunk",
                ));
            };
            let score = cosine_similarity(query_vector, &row.vector)?;
            hits.push(chunk.to_hit(score, source)?);
        }
        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(hits)
    }

    /// Returns aggregate committed record counts.
    /// 返回已提交记录的聚合数量。
    async fn index_counts(&self) -> Result<VceIndexCounts, VcePortError> {
        let rows: Vec<IndexCountsRow> = self
            .query_management_rows(
                r#"SELECT
                    COUNT(*) AS blobs,
                    COALESCE(SUM(CASE WHEN status = 'ready' THEN 1 ELSE 0 END), 0) AS searchable_files,
                    COALESCE(SUM(chunk_count), 0) AS chunks,
                    COALESCE(SUM(vector_count), 0) AS vectors,
                    COALESCE(SUM(CASE WHEN path_document IS NOT NULL THEN 1 ELSE 0 END), 0) AS path_documents,
                    COALESCE(SUM(symbol_count), 0) AS symbols
                   FROM vce_blobs WHERE namespace_id = ?;"#,
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementSnapshot, message))?;
        let row = rows.first().cloned().unwrap_or_default();
        Ok(VceIndexCounts {
            files: nonnegative_u64(row.blobs, "VCE file count")?,
            searchable_files: nonnegative_u64(row.searchable_files, "VCE searchable file count")?,
            skipped_files: 0,
            deleted_files: 0,
            blobs: nonnegative_u64(row.blobs, "VCE Blob count")?,
            chunks: nonnegative_u64(row.chunks, "VCE chunk count")?,
            vectors: nonnegative_u64(row.vectors, "VCE vector count")?,
            path_documents: nonnegative_u64(row.path_documents, "VCE path document count")?,
            symbols: nonnegative_u64(row.symbols, "VCE symbol count")?,
        })
    }

    /// Returns the newest active indexing job when one is not terminal.
    /// 返回尚未终止的最新索引作业。
    async fn latest_index_job(
        &self,
    ) -> Result<Option<vulcan_context_engine::VceIndexJobSummary>, VcePortError> {
        let rows: Vec<OperationRow> = self
            .data_plane
            .query_rows(
                r#"SELECT operation_id, kind, phase, generation, refresh_mode, refresh_reason,
                          completed_units, total_units, attempt, coalesced, error
                   FROM vce_operations
                   WHERE namespace_id = ? AND phase NOT IN ('completed', 'cancelled', 'failed')
                   ORDER BY updated_at_unix_ms DESC, operation_id LIMIT 1;"#,
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementSnapshot, message))?;
        rows.into_iter()
            .next()
            .map(OperationRow::into_index_job)
            .transpose()
    }

    /// Returns the newest terminal operation error for diagnostics.
    /// 返回用于诊断的最新终止操作错误。
    async fn latest_operation_error(&self) -> Result<Option<String>, VcePortError> {
        let rows: Vec<ErrorRow> = self
            .data_plane
            .query_rows(
                "SELECT error FROM vce_operations WHERE namespace_id = ? AND error IS NOT NULL ORDER BY updated_at_unix_ms DESC LIMIT 1;",
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementSnapshot, message))?;
        Ok(rows.into_iter().next().and_then(|row| row.error))
    }

    /// Builds one bounded file inventory page.
    /// 构建一个有界文件清单分页。
    async fn file_page(
        &self,
        query: vulcan_context_engine::VceFileQuery,
    ) -> Result<VcePage<VceFileSummary>, VcePortError> {
        // Ready path channels contain exactly one vector; file rows expose only content vectors.
        // 就绪路径通道精确包含一条向量；文件行仅暴露内容向量。
        let mut sql = String::from(
            r#"SELECT path, blob_name, generation, path_status, path_error, symbol_status,
                      symbol_error, status_reason, chunk_count,
                      vector_count - CASE WHEN path_status = 'ready' THEN 1 ELSE 0 END AS vector_count,
                      symbol_count
               FROM vce_blobs WHERE namespace_id = ?"#,
        );
        let mut params = vec![sqlite_text(&self.binding.namespace_id)];
        if let Some(prefix) = &query.path_prefix {
            // Path identities are literal and case-sensitive; SQL wildcard matching would broaden scope.
            // 路径身份按字面且区分大小写；SQL 通配匹配会扩大范围。
            sql.push_str(" AND (path = ? OR instr(path, ?) = 1)");
            params.push(sqlite_text(prefix));
            params.push(sqlite_text(format!("{prefix}/")));
        }
        if query
            .status
            .is_some_and(|status| status != FileStatus::Present)
        {
            return Ok(VcePage {
                items: Vec::new(),
                next_cursor: None,
                total: Some(0),
            });
        }
        if query.auxiliary_failures_only {
            sql.push_str(" AND (path_status = 'failed' OR symbol_status = 'failed')");
        }
        sql.push_str(" ORDER BY path, blob_name;");
        let rows: Vec<FileSummaryRow> = self
            .query_management_rows(&sql, params)
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        let items = rows
            .into_iter()
            .map(FileSummaryRow::into_summary)
            .collect::<Result<Vec<_>, _>>()?;
        page_items(items, query.page)
    }

    /// Loads one committed Blob and its ordered chunks.
    /// 加载一个已提交 Blob 及其有序代码块。
    async fn blob_detail(&self, blob_name: &str) -> Result<Option<VceBlobDetail>, VcePortError> {
        let rows: Vec<BlobDetailRow> = self
            .query_management_rows(
                r#"SELECT blob_name, path, generation, structure_analyzer_id,
                          structure_fallback_reason, path_status, path_error,
                          symbol_status, symbol_error
                   FROM vce_blobs WHERE namespace_id = ? AND blob_name = ? LIMIT 1;"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(blob_name),
                ],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let chunk_rows: Vec<ChunkDetailRow> = self
            .query_management_rows(
                r#"SELECT chunk_id, start_line, end_line, start_char_offset, kind,
                          substr(content, 1, 500) AS preview, vector_ready
                   FROM vce_chunks WHERE namespace_id = ? AND blob_name = ?
                   ORDER BY start_line, start_char_offset, chunk_id;"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(blob_name),
                ],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        Ok(Some(
            row.into_detail(
                chunk_rows
                    .into_iter()
                    .map(ChunkDetailRow::into_summary)
                    .collect::<Result<Vec<_>, _>>()?,
            )?,
        ))
    }

    /// Loads one bounded operation-history page.
    /// 加载一个有界操作历史分页。
    async fn operation_page(
        &self,
        page: VcePageRequest,
    ) -> Result<VcePage<VceOperationSummary>, VcePortError> {
        let rows: Vec<OperationRow> = self
            .data_plane
            .query_rows(
                r#"SELECT operation_id, kind, phase, generation, refresh_mode, refresh_reason,
                          completed_units, total_units, attempt, coalesced, error
                   FROM vce_operations WHERE namespace_id = ?
                   ORDER BY updated_at_unix_ms DESC, operation_id;"#,
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        page_items(
            rows.into_iter()
                .map(OperationRow::into_summary)
                .collect::<Result<Vec<_>, _>>()?,
            page,
        )
    }

    /// Computes checkpoint, channel, and orphaned-staging integrity issues.
    /// 计算检查点、通道和孤立暂存完整性问题。
    async fn integrity_issues(&self) -> Result<Vec<VceIntegrityIssue>, VcePortError> {
        let checkpoint = self.current_checkpoint_row().await?;
        let blobs: Vec<IntegrityBlobRow> = self
            .query_management_rows(
                // Content completeness must not be satisfied by an auxiliary path vector.
                // 内容完整性不能由辅助路径向量补足。
                r#"SELECT blob_name, path, status, status_reason, path_status, symbol_status, symbol_error, chunk_count,
                          vector_count - CASE WHEN path_status = 'ready' THEN 1 ELSE 0 END AS vector_count
                   FROM vce_blobs WHERE namespace_id = ? ORDER BY path, blob_name;"#,
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        let by_name = blobs
            .iter()
            .map(|row| (row.blob_name.clone(), row))
            .collect::<BTreeMap<_, _>>();
        let members = checkpoint
            .as_ref()
            .map(CheckpointRow::checkpoint_members)
            .transpose()
            .map_err(|message| port_invalid(VcePortOperation::ManagementQuery, message))?
            .unwrap_or_default();
        let mut issues = Vec::new();
        for member in &members {
            let Some(row) = by_name.get(member) else {
                issues.push(integrity_issue(
                    VceIntegrityIssueKind::MissingBlob,
                    None,
                    Some(member.clone()),
                    "A blob referenced by the checkpoint does not exist",
                ));
                continue;
            };
            if row.status != "ready" || (row.chunk_count > 0 && row.vector_count < row.chunk_count)
            {
                issues.push(integrity_issue(
                    VceIntegrityIssueKind::IncompleteContentIndex,
                    Some(row.path.clone()),
                    Some(row.blob_name.clone()),
                    &match &row.status_reason {
                        Some(reason) => {
                            format!("The blob's content chunks or vectors are incomplete: {reason}")
                        }
                        None => String::from("The blob's content chunks or vectors are incomplete"),
                    },
                ));
            }
            if row.path_status == "failed" {
                issues.push(integrity_issue(
                    VceIntegrityIssueKind::PathIndexMismatch,
                    Some(row.path.clone()),
                    Some(row.blob_name.clone()),
                    "The blob's path index channel failed",
                ));
            }
            if row.symbol_status == "failed" {
                issues.push(integrity_issue(
                    VceIntegrityIssueKind::SymbolIndexMismatch,
                    Some(row.path.clone()),
                    Some(row.blob_name.clone()),
                    &match &row.symbol_error {
                        Some(reason) => format!("The blob's exact symbol channel failed: {reason}"),
                        None => String::from("The blob's exact symbol channel failed"),
                    },
                ));
            }
        }
        let staged: Vec<StagingIssueRow> = self
            .data_plane
            .query_rows(
                r#"SELECT blob_name, refresh_id, generation FROM vce_staged_blobs
                   WHERE namespace_id = ? AND (? IS NULL OR refresh_id != ?) ORDER BY blob_name, refresh_id, generation;"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    self.management_active_refresh
                        .as_ref()
                        .map_or(SqliteValue::Null, sqlite_text),
                    self.management_active_refresh
                        .as_ref()
                        .map_or(SqliteValue::Null, sqlite_text),
                ],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        for row in staged {
            // Ownership is unconfirmed, not proof that an external producer has stopped.
            // 活动归属尚未确认，并不证明外部生产者已经停止。
            let message = format!(
                "The active ownership of a staged blob is not confirmed yet (refresh {}, generation {})",
                row.refresh_id, row.generation
            );
            issues.push(integrity_issue(
                VceIntegrityIssueKind::OrphanedStaging,
                None,
                Some(row.blob_name),
                &message,
            ));
        }
        // Retaining a leased predecessor is expected, not an orphaned-staging integrity failure.
        // 保留已被租约引用的前驱是预期行为，不是孤立暂存完整性故障。
        let pending_cleanup: Vec<VectorCleanupIssueRow> = self
            .data_plane
            .query_rows(
                "SELECT c.cleanup_id, c.error FROM vce_vector_cleanup c WHERE c.namespace_id = ? AND c.state = 'pending' AND (c.error IS NOT NULL OR NOT EXISTS(SELECT 1 FROM vce_read_members m WHERE m.namespace_id = c.namespace_id AND m.refresh_id = c.refresh_id AND m.generation = c.generation AND (c.cleanup_kind = 'aborted_generation' OR m.blob_name = json_extract(c.blob_names_json, '$[0]')))) ORDER BY c.cleanup_id;",
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        for row in pending_cleanup {
            let detail = row.error.map_or_else(
                || {
                    format!(
                        "Derived vector cleanup `{}` is waiting to run",
                        row.cleanup_id
                    )
                },
                |error| {
                    format!(
                        "Derived vector cleanup `{}` is waiting to retry: {error}",
                        row.cleanup_id
                    )
                },
            );
            issues.push(integrity_issue(
                VceIntegrityIssueKind::OrphanedStaging,
                None,
                None,
                &detail,
            ));
        }
        Ok(issues)
    }

    /// Builds a read-only deterministic maintenance plan.
    /// 构建只读且确定性的维护计划。
    async fn maintenance_plan(
        &self,
        mode: VceMaintenanceMode,
    ) -> Result<VceMaintenancePlan, VcePortError> {
        if mode == VceMaintenanceMode::Compact {
            return self.physical_compaction_plan().await;
        }
        let issues = self.integrity_issues().await?;
        let mut blob_names = issues
            .iter()
            .filter_map(|issue| issue.blob_name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if mode == VceMaintenanceMode::Rebuild {
            let rows: Vec<BlobNameRow> = self
                .data_plane
                .query_rows(
                    "SELECT blob_name FROM vce_blobs WHERE namespace_id = ? ORDER BY blob_name;",
                    vec![sqlite_text(&self.binding.namespace_id)],
                )
                .await
                .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
            blob_names = rows.into_iter().map(|row| row.blob_name).collect();
        }
        let description = match mode {
            VceMaintenanceMode::Repair => "Repair the identified VCE index integrity issues",
            VceMaintenanceMode::Rebuild => "Rebuild all VCE derived indexes of the current project",
            VceMaintenanceMode::CleanupStaging => "Clean up unpublished VCE staging data",
            VceMaintenanceMode::Compact => unreachable!("compact mode returned above"),
        };
        let action_required = match mode {
            VceMaintenanceMode::Repair => !issues.is_empty(),
            // Rebuild is always an explicit destructive action, including an empty binding that needs initialization.
            // 重建总是明确的破坏性操作，包括需要初始化的空绑定。
            VceMaintenanceMode::Rebuild => true,
            VceMaintenanceMode::CleanupStaging => issues
                .iter()
                .any(|issue| issue.kind == VceIntegrityIssueKind::OrphanedStaging),
            VceMaintenanceMode::Compact => unreachable!("compact mode returned above"),
        };
        let actions = action_required
            .then(|| VceMaintenanceAction {
                action_id: maintenance_action_id(mode, &blob_names),
                description: String::from(description),
                blob_names: blob_names.clone(),
                destructive: true,
                physical_table: None,
            })
            .into_iter()
            .collect();
        Ok(VceMaintenancePlan {
            mode,
            actions,
            affected_blob_count: blob_names.len() as u64,
            physical_tables: Vec::new(),
        })
    }

    /// Build a model-independent preview of both physical vector tables.
    /// 构建两张物理向量表的模型无关预览。
    ///
    /// Takes no parameters. Returns authoritative table statistics and only the actions that can
    /// reduce an observed fragmented layout; it never mutates storage.
    /// 无参数。返回权威表统计及确实可以减少已观察碎片的操作；绝不修改存储。
    pub async fn physical_compaction_plan(&self) -> Result<VceMaintenancePlan, VcePortError> {
        let (content, path) = tokio::try_join!(
            self.physical_table_statistics(VcePhysicalTableRole::Content),
            self.physical_table_statistics(VcePhysicalTableRole::Path),
        )
        .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        let options = physical_compaction_options();
        let physical_tables = [content, path]
            .into_iter()
            .map(|statistics| VcePhysicalTablePlan {
                compaction_required: statistics.requires_compaction(),
                statistics,
                options: options.clone(),
            })
            .collect::<Vec<_>>();
        let actions = physical_tables
            .iter()
            .filter(|table| table.compaction_required)
            .cloned()
            .map(|table| {
                let role = physical_table_role_text(table.statistics.role);
                VceMaintenanceAction {
                    action_id: physical_compaction_action_id(&table),
                    description: format!(
                        "Compact the physical fragments of the VCE {role} vector table"
                    ),
                    blob_names: Vec::new(),
                    destructive: false,
                    physical_table: Some(table),
                }
            })
            .collect();
        Ok(VceMaintenancePlan {
            mode: VceMaintenanceMode::Compact,
            actions,
            affected_blob_count: 0,
            physical_tables,
        })
    }

    /// Records one durable host operation projection.
    /// 记录一项耐久宿主操作投影。
    async fn record_operation(&self, record: OperationRecord<'_>) -> Result<(), String> {
        let now = current_unix_epoch_millis_i64()?;
        self.data_plane
            .execute(
                r#"INSERT INTO vce_operations(
                    namespace_id, operation_id, kind, phase, generation,
                    completed_units, total_units, attempt, coalesced, error,
                    created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 1, 0, ?, ?, ?)
                ON CONFLICT(namespace_id, operation_id) DO UPDATE SET
                    kind = excluded.kind, phase = excluded.phase, generation = excluded.generation,
                    completed_units = excluded.completed_units, total_units = excluded.total_units,
                    error = excluded.error, updated_at_unix_ms = excluded.updated_at_unix_ms;"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(record.operation_id),
                    sqlite_text(operation_kind_text(record.kind)),
                    sqlite_text(operation_phase_text(record.phase)),
                    sqlite_u64(record.generation, "VCE generation")
                        .map_err(|error| error.to_string())?,
                    sqlite_u64(record.completed_units, "VCE completed units")
                        .map_err(|error| error.to_string())?,
                    sqlite_u64(record.total_units, "VCE total units")
                        .map_err(|error| error.to_string())?,
                    sqlite_optional_text(record.error),
                    SqliteValue::Integer(now),
                    SqliteValue::Integer(now),
                ],
                "Failed to record the VCE operation",
            )
            .await
    }
}
