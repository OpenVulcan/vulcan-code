//! Controller-backed VCE code-semantic persistence and search.
//! 基于控制器的 VCE 代码语义持久化与检索。

use crate::data_plane::ControllerRuntimeHandle;
use crate::runtime_assets;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use vldb_controller_client::{
    ControllerClient, ControllerLanceDbColumnDef, ControllerLanceDbColumnType,
    ControllerLanceDbEnableRequest, ControllerLanceDbInputFormat, ControllerLanceDbOutputFormat,
    ControllerSqliteEnableRequest, ControllerSqliteValue,
};
use vulcan_context_engine::{
    AuxiliaryIndexStatus, Checkpoint, Chunk, ChunkKind, CommitReceipt, DenseSearchRequest,
    ExactSearchRequest, FileStatus, FindMissingRequest, FindMissingResponse, HitKind,
    IndexAbortReceipt, IndexAbortRequest, IndexCommit, IndexStageBatch, IndexStageReceipt,
    IndexedBlob, IndexedBlobStatus, PathDocument, PathSearchRequest, RefreshMode, RefreshReason,
    SearchHit, SearchSource, SourceRange, StructureMode, SymbolKind, SymbolOccurrence,
    VceBlobDetail, VceChannelState, VceChunkSummary, VceFileSummary, VceFuture, VceIndexBinding,
    VceIndexCounts, VceIndexIntegrity, VceIndexJobSummary, VceIndexManagementSnapshot,
    VceIndexPort, VceIndexStoreStatus, VceIntegrityIssue, VceIntegrityIssueKind,
    VceMaintenanceAction, VceMaintenanceMode, VceMaintenancePlan, VceManagementQuery,
    VceManagementQueryRequest, VceManagementQueryResponse, VceManagementQueryResult,
    VceManagementRequest, VceOperationKind, VceOperationPhase, VceOperationSummary, VcePage,
    VcePageRequest, VcePathSearchResponse, VcePortError, VcePortOperation, VceSearchResponse,
    VceStateSnapshot, content_hash,
};

pub use crate::vce_storage_catalog::VceDataPlaneFactory;
pub use crate::vce_storage_paths::VceProjectWriterLease;

#[path = "vce_job_store.rs"]
mod jobs;
pub use jobs::{
    VCE_JOB_RECEIPT_SCHEMA_VERSION, VceCompletedJobUpdate, VceJobKind, VceJobReplacement,
    VceJobState, VceProjectJobStore, VceStoredJob,
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

/// Stable controller space reserved for code-semantic state.
/// 为代码语义状态保留的稳定控制器空间。
pub const VCE_SPACE_ID: &str = "vce";

/// Stable SQLite binding tag reserved for VCE relational state.
/// 为 VCE 关系状态保留的稳定 SQLite 绑定标签。
pub const VCE_SQLITE_BINDING_TAG: &str = "vce-state";

/// Stable LanceDB binding tag reserved for VCE vectors.
/// 为 VCE 向量保留的稳定 LanceDB 绑定标签。
pub const VCE_LANCEDB_BINDING_TAG: &str = "vce-code";

/// Current relational schema version for the VCE host data plane.
/// VCE 宿主数据面的当前关系结构版本。
pub const VCE_DATA_PLANE_SCHEMA_VERSION: i64 = 5;

/// Maximum result count accepted by one physical vector search.
/// 一次物理向量检索允许的最大结果数量。
const VCE_VECTOR_SEARCH_LIMIT: u32 = 10_000;

/// Process-local sequence keeping repeated controller bindings distinct.
/// 保证重复控制器绑定互不冲突的进程内序列。
static VCE_BINDING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Long-lived controller bindings for VCE SQLite and LanceDB storage.
/// VCE SQLite 与 LanceDB 存储使用的长生命周期控制器绑定。
#[derive(Clone)]
pub struct VceDataPlaneHandle {
    /// Exact stable owner of this project's isolated physical storage.
    /// 本项目隔离物理存储的精确稳定所有者。
    project_owner: String,
    /// Counts every retained clone so cache eviction cannot close an active query or writer.
    /// 统计全部保留克隆，使缓存淘汰不能关闭活动查询或写入者。
    pub(crate) lease: Arc<()>,
    /// Final read-reference releases awaiting an owned controller boundary, never detached tasks.
    /// 等待所属控制器边界处理的最终读取引用释放，绝不使用脱离管理的任务。
    read_releases: Arc<std::sync::Mutex<Vec<read_generations::VceReleasedReadLease>>>,
    /// Shared controller client owned by the VulcanCode host.
    /// 由 VulcanCode 宿主持有的共享控制器客户端。
    pub client: ControllerClient,
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
    /// Bootstrap-scoped LanceDB binding identity.
    /// 单次引导作用域内的 LanceDB 绑定身份。
    pub lancedb_binding_id: String,
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
                "VCE 项目 `{}` 的持久索引身份与当前配置不一致",
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
            .map_err(|error| format!("解析 VCE 检查点失败：{error}"))
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
    /// Path-channel status.
    /// 路径通道状态。
    path_status: String,
    /// Symbol-channel status.
    /// 符号通道状态。
    symbol_status: String,
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
) -> Result<Vec<ControllerSqliteValue>, String> {
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
        ControllerSqliteValue::Int64(i64::from(vector_ready)),
    ])
}

/// Builds one staged exact-symbol parameter list in schema order.
/// 按 Schema 顺序构建一组暂存精确符号参数。
fn staged_symbol_params(
    namespace_id: &str,
    refresh_id: &str,
    generation: u64,
    symbol: &SymbolOccurrence,
) -> Result<Vec<ControllerSqliteValue>, String> {
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
        sqlite_text(&symbol.content),
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
    };
    let mut hasher = Sha256::new();
    hasher.update(mode.as_bytes());
    for blob_name in blob_names {
        hasher.update(b"\n");
        hasher.update(blob_name.as_bytes());
    }
    hex::encode(hasher.finalize())
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
fn vector_table_columns(dimension: u32) -> Vec<ControllerLanceDbColumnDef> {
    vec![
        ControllerLanceDbColumnDef {
            name: String::from("row_id"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("id"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("blob_name"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("namespace_id"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("refresh_id"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("generation"),
            column_type: ControllerLanceDbColumnType::Uint64,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("vector_space_id"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("physical_fingerprint"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("path"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("start_line"),
            column_type: ControllerLanceDbColumnType::Uint64,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("end_line"),
            column_type: ControllerLanceDbColumnType::Uint64,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("content_hash"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("written_at_unix_ms"),
            column_type: ControllerLanceDbColumnType::Int64,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("vector"),
            column_type: ControllerLanceDbColumnType::VectorFloat32,
            vector_dim: dimension,
            nullable: false,
        },
    ]
}

/// Returns one bootstrap-scoped controller binding suffix.
/// 返回一个单次引导作用域的控制器绑定后缀。
fn build_binding_suffix() -> String {
    let sequence = VCE_BINDING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{sequence}", std::process::id())
}

/// Converts one value into a SQLite text parameter.
/// 将一个值转换为 SQLite 文本参数。
fn sqlite_text(value: impl Into<String>) -> ControllerSqliteValue {
    ControllerSqliteValue::String(value.into())
}

/// Converts one optional value into a nullable SQLite text parameter.
/// 将一个可选值转换为可空 SQLite 文本参数。
fn sqlite_optional_text(value: Option<&str>) -> ControllerSqliteValue {
    value
        .map(sqlite_text)
        .unwrap_or(ControllerSqliteValue::Null)
}

/// Converts one u64 into a checked SQLite integer parameter.
/// 将一个 u64 转换为经过校验的 SQLite 整数参数。
fn sqlite_u64(value: u64, label: &str) -> Result<ControllerSqliteValue, VcePortError> {
    sqlite_u64_raw(value, label).map_err(|message| port_rejected(VcePortOperation::Stage, message))
}

/// Converts one u64 into a checked raw SQLite integer parameter.
/// 将一个 u64 转换为经过校验的原始 SQLite 整数参数。
fn sqlite_u64_raw(value: u64, label: &str) -> Result<ControllerSqliteValue, String> {
    i64::try_from(value)
        .map(ControllerSqliteValue::Int64)
        .map_err(|_| format!("{label} exceeds SQLite integer range"))
}

/// Converts one usize into a checked SQLite integer parameter.
/// 将一个 usize 转换为经过校验的 SQLite 整数参数。
fn sqlite_usize(value: usize, label: &str) -> Result<ControllerSqliteValue, VcePortError> {
    sqlite_usize_raw(value, label)
        .map_err(|message| port_rejected(VcePortOperation::Stage, message))
}

/// Converts one usize into a checked raw SQLite integer parameter.
/// 将一个 usize 转换为经过校验的原始 SQLite 整数参数。
fn sqlite_usize_raw(value: usize, label: &str) -> Result<ControllerSqliteValue, String> {
    i64::try_from(value)
        .map(ControllerSqliteValue::Int64)
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
        .map_err(|error| format!("读取系统时间失败：{error}"))?
        .as_millis();
    i64::try_from(millis).map_err(|_| String::from("当前时间超出 SQLite 整数范围"))
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
mod tests {
    use super::{
        VCE_SPACE_ID, VceProjectIndexConfig, bootstrap_vce_data_plane, build_vce_schema_script,
        validate_checkpoint_membership_transition, vector_table_name,
    };
    use crate::data_plane::{ControllerRuntimeHandle, ControllerRuntimeMetadata};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use uuid::Uuid;
    use vldb_controller_client::{
        ClientRegistration, ControllerClient, ControllerClientConfig, ControllerProcessMode,
    };
    use vulcan_context_engine::{
        AuxiliaryIndexStatus, Checkpoint, Chunk, ChunkKind, DenseSearchRequest, EmbeddingSpace,
        EmbeddingVector, FileObservation, FileRecord, FileSource, FileStatus, IndexAbortRequest,
        IndexCommit, IndexStageBatch, IndexedBlob, IndexedBlobStatus, RefreshMode, RefreshReason,
        RefreshRequest, RetrieveRequest, SegmentVector, VceAiBinding, VceAiCapabilitySnapshot,
        VceAiPort, VceConfig, VceEmbeddingRequest, VceEmbeddingResponse, VceEngine, VceFuture,
        VceIndexBinding, VceIndexPort, VcePortError, VcePortOperation, VceQueryRewriteRequest,
        VceQueryRewriteResponse, VceQueryStatistics, VceRefreshSummary, VceRerankRequest,
        VceRerankResponse, VceSourcePort, VceSourceReadRequest, VceSourceReadResponse,
        VceStateIdentity, VceStateSnapshot, WorkspaceSnapshot, blob_name,
    };

    /// Deterministic VCE AI boundary used by the real data-plane test.
    /// 真实 VCE 数据面测试使用的确定性 VCE AI 边界。
    struct DeterministicAi {
        /// Frozen registration identity.
        /// 冻结的注册身份。
        binding: VceAiBinding,
        /// Physical vector-space contract returned by every Embedding call.
        /// 每次向量调用返回的物理向量空间契约。
        space: EmbeddingSpace,
    }

    impl VceAiPort for DeterministicAi {
        /// Returns the fixed test registration.
        /// 返回固定测试注册。
        fn binding(&self) -> &VceAiBinding {
            &self.binding
        }

        /// Returns the complete fixed capability snapshot.
        /// 返回完整固定能力快照。
        fn capability_snapshot(&self) -> VceAiCapabilitySnapshot {
            VceAiCapabilitySnapshot {
                registration_id: self.binding.registration_id.clone(),
                consumer_profile_id: self.binding.consumer_profile_id.clone(),
                vector_space_id: self.binding.vector_space_id.clone(),
                registered: true,
                embedding_available: true,
                query_rewrite_available: true,
                rerank_available: true,
            }
        }

        /// Returns deterministic normalized vectors for every requested segment.
        /// 为每个请求分段返回确定性归一化向量。
        fn embed(
            &self,
            request: VceEmbeddingRequest,
        ) -> VceFuture<Result<VceEmbeddingResponse, VcePortError>> {
            let vectors = request
                .batch
                .segments
                .iter()
                .map(|segment| SegmentVector {
                    input_id: segment.request_input_id(),
                    values: vec![1.0, 0.0, 0.0],
                })
                .collect();
            ready(Ok(VceEmbeddingResponse {
                operation_id: request.context.operation_id,
                vectors,
                space: self.space.clone(),
            }))
        }

        /// Returns no optional query rewrites in the deterministic fixture.
        /// 在确定性夹具中不返回可选查询改写。
        fn rewrite_query(
            &self,
            request: VceQueryRewriteRequest,
        ) -> VceFuture<Result<VceQueryRewriteResponse, VcePortError>> {
            ready(Ok(VceQueryRewriteResponse {
                operation_id: request.context.operation_id,
                rewrites: Vec::new(),
            }))
        }

        /// Preserves the candidate order during deterministic Rerank.
        /// 在确定性重排中保留候选顺序。
        fn rerank(
            &self,
            request: VceRerankRequest,
        ) -> VceFuture<Result<VceRerankResponse, VcePortError>> {
            ready(Ok(VceRerankResponse {
                operation_id: request.context.operation_id,
                ordered_candidate_indexes: (0..request.candidates.len()).collect(),
            }))
        }
    }

    /// Source boundary that rejects reads because the fixture supplies immutable text observations.
    /// 拒绝源码读取的边界，因为夹具会直接提供不可变正文观察。
    struct NoReadSource;

    impl VceSourcePort for NoReadSource {
        /// Rejects every unexpected source read.
        /// 拒绝每一次非预期源码读取。
        fn read_sources(
            &self,
            _request: VceSourceReadRequest,
        ) -> VceFuture<Result<VceSourceReadResponse, VcePortError>> {
            ready(Err(VcePortError::Failed {
                operation: VcePortOperation::SourceRead,
                message: String::from("unexpected real-data-plane fixture source read"),
            }))
        }
    }

    /// Returns one immediately ready asynchronous test result.
    /// 返回一个立即就绪的异步测试结果。
    fn ready<T: Send + 'static>(value: T) -> VceFuture<T> {
        Box::pin(async move { value })
    }

    /// Verifies the VCE schema remains isolated and contains atomic-publication tables.
    /// 验证 VCE Schema 保持隔离并包含原子发布表。
    #[test]
    fn schema_is_isolated_and_contains_publication_state() {
        let schema = build_vce_schema_script();
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_project_bindings"));
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_staged_blobs"));
        assert!(schema.contains("CREATE VIEW IF NOT EXISTS vce_checkpoints"));
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_read_leases"));
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_blob_versions"));
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_commit_receipts"));
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_commit_guards"));
        assert!(!schema.contains("vmm_projects"));
        assert!(!schema.contains("knowledge_documents"));
    }

    /// Verifies vector table identities are deterministic and channel-separated.
    /// 验证向量表身份确定且按通道隔离。
    #[test]
    fn vector_tables_are_deterministic_and_channel_scoped() {
        let content = vector_table_name(
            "content",
            "code-semantic:project-1",
            "provider:model:1536",
            "pre-v1",
            "chunk-v1",
            1536,
        );
        let repeated = vector_table_name(
            "content",
            "code-semantic:project-1",
            "provider:model:1536",
            "pre-v1",
            "chunk-v1",
            1536,
        );
        let path = vector_table_name(
            "path",
            "code-semantic:project-1",
            "provider:model:1536",
            "pre-v1",
            "chunk-v1",
            1536,
        );
        assert_eq!(content, repeated);
        assert_ne!(content, path);
        assert!(content.starts_with("vce_content_"));
        assert!(content.ends_with("_d1536"));
    }

    /// Verifies checkpoint publication cannot omit, fabricate, or delete unknown Blob identities.
    /// 验证检查点发布不能遗漏、伪造成员或删除未知 Blob 身份。
    #[test]
    fn checkpoint_membership_transition_is_exact() {
        // current is the authoritative predecessor membership.
        // current 是权威前驱成员集合。
        let current = BTreeSet::from([String::from("blob-a"), String::from("blob-b")]);
        // deleted removes one existing identity from the next view.
        // deleted 从下一视图移除一个已存在身份。
        let deleted = BTreeSet::from([String::from("blob-a")]);
        // staged publishes one replacement identity.
        // staged 发布一个替换身份。
        let staged = BTreeSet::from([String::from("blob-c")]);
        // expected is the only valid next membership.
        // expected 是唯一有效的下一成员集合。
        let expected = BTreeSet::from([String::from("blob-b"), String::from("blob-c")]);

        assert_eq!(
            validate_checkpoint_membership_transition(&current, &deleted, &staged, &expected),
            Ok(())
        );
        assert!(
            validate_checkpoint_membership_transition(
                &current,
                &deleted,
                &staged,
                &BTreeSet::from([String::from("blob-c")]),
            )
            .is_err()
        );
        assert!(
            validate_checkpoint_membership_transition(
                &current,
                &deleted,
                &staged,
                &BTreeSet::from([
                    String::from("blob-b"),
                    String::from("blob-c"),
                    String::from("blob-fabricated"),
                ]),
            )
            .is_err()
        );
        assert!(
            validate_checkpoint_membership_transition(
                &current,
                &BTreeSet::from([String::from("blob-unknown")]),
                &staged,
                &expected,
            )
            .is_err()
        );
    }

    /// Exercises project binding, root rotation, and physical-table creation against the real controller.
    /// 使用真实控制器验证项目绑定、根目录轮换和物理表创建。
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires the bundled output/bin/vldb-controller.exe"]
    async fn real_controller_opens_and_rotates_project_binding() {
        let temporary = tempdir().expect("create VCE integration root");
        let data_root = temporary.path().join("data");
        let first_root = temporary.path().join("project-one");
        let second_root = temporary.path().join("project-two");
        fs::create_dir_all(&first_root).expect("create first VCE project root");
        fs::create_dir_all(&second_root).expect("create second VCE project root");
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("reserve VCE controller port");
        let port = listener
            .local_addr()
            .expect("read VCE controller port")
            .port();
        drop(listener);
        let executable = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("output")
            .join("bin")
            .join("vldb-controller.exe");
        assert!(executable.is_file(), "bundled controller must exist");
        let endpoint = format!("http://127.0.0.1:{port}");
        let client = ControllerClient::new(
            ControllerClientConfig {
                endpoint: endpoint.clone(),
                auto_spawn: true,
                spawn_executable: Some(executable.display().to_string()),
                spawn_process_mode: ControllerProcessMode::Managed,
                minimum_uptime_secs: 0,
                idle_timeout_secs: 1,
                default_lease_ttl_secs: 30,
                connect_timeout_secs: 30,
                startup_timeout_secs: 15,
                startup_retry_interval_ms: 100,
                lease_renew_interval_secs: 10,
            },
            ClientRegistration {
                client_name: format!("vce-e2e-{}", Uuid::new_v4()),
                host_kind: String::from("test"),
                process_id: std::process::id(),
                process_name: String::from("vulcan-workspace-store-test"),
                lease_ttl_secs: Some(30),
            },
        );
        client.connect().await.expect("connect VCE controller");
        let metadata_path = data_root.join("controller-placeholder.sqlite3");
        let controller = ControllerRuntimeHandle::new(
            client.clone(),
            ControllerRuntimeMetadata {
                endpoint,
                executable: executable.display().to_string(),
                data_root: data_root.clone(),
                sqlite_path: metadata_path.clone(),
                sqlite_binding_id: format!("vce-e2e-app-{}", Uuid::new_v4()),
                system_statistics_sqlite_path: metadata_path,
                system_statistics_sqlite_binding_id: format!(
                    "vce-e2e-statistics-{}",
                    Uuid::new_v4()
                ),
            },
        );
        // catalog is the new lightweight location boundary; bootstrapping it must not open projects.
        // catalog 是新的轻量位置边界；引导它不得打开项目。
        fs::create_dir_all(&data_root).expect("create isolated application data root");
        let catalog = crate::vce_storage_catalog::VceStorageCatalog::bootstrap(&controller)
            .await
            .expect("bootstrap lightweight catalog");
        assert!(catalog.root().join("catalog.sqlite3").is_file());
        assert!(!catalog.root().join("projects").exists());
        assert!(!catalog.root().join("lancedb").exists());
        for number in 0..100 {
            assert!(
                catalog
                    .project_location(&format!("inactive-project-{number}"))
                    .await
                    .expect("read cold location")
                    .is_none()
            );
        }
        assert!(!catalog.root().join("projects").exists());
        // allocated is location ownership only: neither SQLite nor vector tables are initialized.
        // allocated 仅代表位置归属：尚未初始化 SQLite 或向量表。
        let allocated = catalog
            .allocate_project("catalog-project-a")
            .await
            .expect("allocate exact project");
        assert!(allocated.root.is_dir());
        assert!(!allocated.sqlite_path.exists());
        assert!(
            catalog
                .project_location("catalog-project-b")
                .await
                .expect("peer location")
                .is_none()
        );
        // original_location must survive repeated allocation and catalog reopening unchanged.
        // original_location 必须在重复分配与目录重开后保持不变。
        let original_location = catalog
            .project_location("catalog-project-a")
            .await
            .expect("read location");
        assert_eq!(
            catalog
                .allocate_project("catalog-project-a")
                .await
                .expect("repeat allocation"),
            allocated
        );
        catalog.close().await.expect("close catalog binding only");
        // reopened verifies the durable catalog version and ownership through the same host controller.
        // reopened 通过相同宿主控制器验证持久目录版本与归属。
        let reopened = crate::vce_storage_catalog::VceStorageCatalog::bootstrap(&controller)
            .await
            .expect("reopen catalog");
        assert_eq!(
            reopened
                .project_location("catalog-project-a")
                .await
                .expect("reread location"),
            original_location
        );
        reopened.close().await.expect("close reopened catalog");
        // factory starts with no content bindings, even when the workspace contains many projects.
        // factory 起初不含内容绑定，即使工作区包含很多项目。
        let factory = super::VceDataPlaneFactory::bootstrap(&controller)
            .await
            .expect("bootstrap isolated factory");
        for number in 0..100 {
            assert!(
                factory
                    .load_active_project_config(&format!("cold-{number}"))
                    .await
                    .expect("read cold project")
                    .is_none()
            );
        }
        assert!(!data_root.join("vmm").exists());
        // isolated_a and isolated_b must use separate physical files and controller binding identities.
        // isolated_a 与 isolated_b 必须使用分离的物理文件和控制器绑定身份。
        let isolated_a = factory
            .create_project_index(test_project_config("isolated-a", &first_root, 1))
            .await
            .expect("create isolated a");
        let isolated_b = factory
            .create_project_index(test_project_config("isolated-b", &second_root, 1))
            .await
            .expect("create isolated b");
        assert_ne!(
            isolated_a.data_plane.sqlite_path,
            isolated_b.data_plane.sqlite_path
        );
        assert_ne!(
            isolated_a.data_plane.lancedb_path,
            isolated_b.data_plane.lancedb_path
        );
        assert_ne!(
            isolated_a.data_plane.sqlite_binding_id,
            isolated_b.data_plane.sqlite_binding_id
        );
        assert!(isolated_a.data_plane.sqlite_path.is_file());
        assert!(isolated_b.data_plane.sqlite_path.is_file());
        assert!(
            isolated_a.data_plane.sqlite_path.starts_with(
                fs::canonicalize(&data_root)
                    .expect("resolve fixture data root")
                    .join("vce")
                    .join("projects")
            )
        );
        assert!(!isolated_a.is_read_only());
        // jobs is the exact physical project repository; a lookup cannot create its receipt table.
        // jobs 是精确物理项目仓库；查找不能创建其回执表。
        let jobs = isolated_a.job_store();
        assert!(
            jobs.latest()
                .await
                .expect("read absent latest job")
                .is_none()
        );
        assert!(
            jobs.unfinished()
                .await
                .expect("read absent job owner")
                .is_none()
        );
        assert!(
            jobs.get("job-a")
                .await
                .expect("read absent job table")
                .is_none()
        );
        // queued freezes the original command separately from mutable project update policy.
        // queued 将原始命令与可变项目更新策略分开冻结。
        let queued = super::VceStoredJob {
            schema_version: super::VCE_JOB_RECEIPT_SCHEMA_VERSION,
            replacement: None,
            project_id: String::from("isolated-a"),
            job_id: String::from("job-a"),
            owner_run_id: String::from("host-run-a"),
            idempotency_key: String::from("request-a"),
            request_fingerprint: "a".repeat(64),
            contract_fingerprint: "b".repeat(64),
            kind: super::VceJobKind::Initialize,
            state: super::VceJobState::Queued,
            sequence: 1,
            created_at_unix_ms: 1000,
            updated_at_unix_ms: 1000,
            admitted_settings_revision: 1,
            source_checkpoint_version: None,
            retry_of_job_id: None,
            result_checkpoint_version: None,
            progress: None,
            followup_requested: false,
            completed_update: None,
            error: None,
        };
        assert_eq!(
            jobs.register(&queued)
                .await
                .expect("durably register before admission response"),
            queued
        );
        assert_eq!(
            jobs.latest().await.expect("read queued receipt"),
            Some(queued.clone())
        );
        assert_eq!(
            jobs.unfinished().await.expect("read active owner"),
            Some(queued.clone())
        );
        assert_eq!(
            jobs.register(&queued)
                .await
                .expect("replay exact admission"),
            queued
        );
        assert!(
            isolated_b
                .job_store()
                .get("job-a")
                .await
                .expect("peer job lookup")
                .is_none()
        );
        assert!(isolated_b.job_store().register(&queued).await.is_err());
        // conflict reuses the key for a different command and must preserve the first receipt.
        // conflict 为不同命令复用键，必须保留首个回执。
        let mut conflict = queued.clone();
        conflict.request_fingerprint = "c".repeat(64);
        assert!(jobs.register(&conflict).await.is_err());
        // running advances only the acknowledged prior sequence.
        // running 仅推进已确认的先前序号。
        let mut running = queued.clone();
        running.state = super::VceJobState::Running;
        running.sequence = 2;
        running.updated_at_unix_ms = 1001;
        assert_eq!(
            jobs.update(&queued, &running)
                .await
                .expect("start durable job"),
            running
        );
        assert!(jobs.update(&queued, &running).await.is_err());
        // failed remains a durable terminal receipt; retry must be a distinct job identity.
        // failed 保持持久终态回执；重试必须使用不同作业身份。
        let mut failed = running.clone();
        failed.state = super::VceJobState::Failed;
        failed.sequence = 3;
        failed.error = Some(String::from("fixture failure"));
        assert_eq!(
            jobs.update(&running, &failed)
                .await
                .expect("record terminal failure"),
            failed
        );
        assert_eq!(
            jobs.register(&queued)
                .await
                .expect("replay terminal admission without running again"),
            failed
        );
        running.sequence = 4;
        assert!(jobs.update(&failed, &running).await.is_err());
        assert_eq!(
            jobs.latest().await.expect("read latest terminal receipt"),
            Some(failed)
        );
        assert!(
            jobs.unfinished()
                .await
                .expect("terminal job releases owner")
                .is_none()
        );
        // A synchronization may retain one later batch and immutable aliases without admitting another writer.
        // 同步可保留一个后续批次和不可变别名，而不准入另一写入者。
        let mut sync_queued = queued.clone();
        sync_queued.job_id = String::from("job-sync");
        sync_queued.idempotency_key = String::from("request-sync");
        sync_queued.kind = super::VceJobKind::Synchronize;
        sync_queued.source_checkpoint_version = Some(1);
        sync_queued.created_at_unix_ms = 1100;
        sync_queued.updated_at_unix_ms = 1100;
        jobs.register(&sync_queued)
            .await
            .expect("register synchronization fixture");
        let mut sync_running = sync_queued.clone();
        sync_running.state = super::VceJobState::Running;
        sync_running.sequence += 1;
        jobs.update(&sync_queued, &sync_running)
            .await
            .expect("start synchronization fixture");
        let followup_key = "request-followup'quoted";
        let coalesced = jobs
            .coalesce(&sync_running, followup_key, &"c".repeat(64), 1110, true)
            .await
            .expect("atomically register follow-up");
        assert!(coalesced.followup_requested);
        assert_eq!(
            jobs.coalesce(&sync_running, followup_key, &"c".repeat(64), 1120, true)
                .await
                .expect("same request never adds work"),
            coalesced
        );
        assert!(
            jobs.find_submission(followup_key, &"d".repeat(64))
                .await
                .is_err()
        );
        assert!(
            isolated_b
                .job_store()
                .find_submission(followup_key, &"c".repeat(64))
                .await
                .expect("peer cannot see alias")
                .is_none()
        );
        // Stale CAS must roll back the alias too; a retry cannot masquerade as an already accepted request.
        // 过期比较交换也必须回滚别名；重试不能伪装为已受理请求。
        assert!(
            jobs.coalesce(&sync_running, "stale-followup", &"d".repeat(64), 1120, true)
                .await
                .is_err()
        );
        assert!(
            jobs.find_submission("stale-followup", &"d".repeat(64))
                .await
                .expect("query after transactional rollback")
                .is_none()
        );
        let mut premature = coalesced.clone();
        premature.sequence += 1;
        premature.followup_requested = false;
        premature.state = super::VceJobState::Queued;
        assert!(jobs.update(&coalesced, &premature).await.is_err());
        // A later failed batch must preserve the exact prior batch success timestamp and checkpoint.
        // 后续失败批次必须保留精确的先前批次成功时间及检查点。
        let mut batch_complete = coalesced.clone();
        batch_complete.sequence += 1;
        batch_complete.updated_at_unix_ms = 1200;
        batch_complete.completed_update = Some(super::VceCompletedJobUpdate {
            checkpoint_version: 1,
            at_unix_ms: 1200,
        });
        jobs.update(&coalesced, &batch_complete)
            .await
            .expect("record successful batch");
        let mut erased_success = batch_complete.clone();
        erased_success.sequence += 1;
        erased_success.completed_update = None;
        assert!(jobs.update(&batch_complete, &erased_success).await.is_err());
        // The sole receipt format requires explicit batch evidence instead of a legacy timestamp fallback.
        // 唯一回执格式要求明确批次证据，而不是旧时间戳兜底。
        let mut incomplete_success = erased_success.clone();
        incomplete_success.state = super::VceJobState::Succeeded;
        incomplete_success.result_checkpoint_version = Some(1);
        incomplete_success.followup_requested = false;
        assert!(incomplete_success.validate().is_err());
        assert!(serde_json::from_str::<super::VceJobKind>("\"migrate\"").is_err());
        let mut following = batch_complete.clone();
        following.sequence += 1;
        following.state = super::VceJobState::Queued;
        following.followup_requested = false;
        jobs.update(&batch_complete, &following)
            .await
            .expect("queue acknowledged successor");
        let mut failed_followup = following.clone();
        failed_followup.sequence += 1;
        failed_followup.state = super::VceJobState::Failed;
        failed_followup.error = Some(String::from("follow-up fixture failure"));
        failed_followup.updated_at_unix_ms = 1300;
        jobs.update(&following, &failed_followup)
            .await
            .expect("preserve partial successful publication");
        assert_eq!(
            jobs.latest_successful_update()
                .await
                .expect("read last successful batch"),
            Some(failed_followup.clone())
        );
        assert_eq!(
            jobs.find_submission(followup_key, &"c".repeat(64))
                .await
                .expect("terminal alias does not create a new job"),
            Some(failed_followup)
        );
        drop(jobs);
        assert!(
            isolated_a
                .load_state_snapshot()
                .await
                .expect("new physical store has no content checkpoint")
                .is_none()
        );
        assert!(
            isolated_a
                .data_plane
                .load_active_project_config("isolated-b")
                .await
                .is_err()
        );
        assert!(
            factory
                .create_project_index(test_project_config("isolated-a", &second_root, 2))
                .await
                .is_err()
        );
        assert!(
            factory
                .close_project_bindings()
                .await
                .expect_err("live leases protect stores")
                .contains("live project leases")
        );
        assert!(isolated_a.load_state_snapshot().await.is_ok());
        // held leases fill the physical resource budget; no live project can be evicted.
        // held 租约填满物理资源预算；任何活动项目均不得被淘汰。
        let mut held = Vec::new();
        for number in 0..6 {
            held.push(
                factory
                    .create_project_index(test_project_config(
                        &format!("budget-{number}"),
                        &first_root,
                        1,
                    ))
                    .await
                    .expect("fill bounded cache"),
            );
        }
        assert!(
            factory
                .create_project_index(test_project_config("budget-overflow", &first_root, 1))
                .await
                .expect_err("live lease budget cannot be exceeded")
                .contains("budget")
        );
        assert!(isolated_b.load_state_snapshot().await.is_ok());
        drop(held);
        // retried proves an explicitly requested creation can reuse capacity after leases drain.
        // retried 证明租约排空后明确请求的创建能够复用容量。
        let retried = factory
            .create_project_index(test_project_config("budget-overflow", &first_root, 1))
            .await
            .expect("retry after idle lease capacity returns");
        drop(retried);
        drop(isolated_a);
        drop(isolated_b);
        factory
            .close_project_bindings()
            .await
            .expect("release idle project bindings");
        assert!(
            factory
                .load_active_project_config("isolated-a")
                .await
                .expect("lazy reopen after drained shutdown")
                .is_some()
        );
        factory.shutdown().await.expect("close isolated factory");
        // Verify first-allocation failure separately before the ordinary publication fixture starts.
        // 在普通发布夹具开始之前单独验证首次分配失败。
        verify_initial_allocation_retry(&controller, &first_root).await;
        // Exercise registration rollback and cleanup failure using isolated real-controller ownership.
        // 使用隔离的真实控制器归属验证登记回滚及清理失败。
        for (project_id, interrupt_cleanup) in [
            ("initial-registration-failure", false),
            ("initial-cleanup-failure", true),
        ] {
            // Await each owned task separately so its deep controller polling does not nest in the large parent fixture.
            // 分别等待每个自有任务，避免深层控制器轮询嵌入大型父夹具栈。
            let failure_controller = controller.clone();
            // The same exact project target is moved into the sequential current-thread task.
            // 同一精确项目目标移入串行的当前线程任务。
            let failure_config = test_project_config(project_id, &first_root, 1);
            tokio::spawn(async move {
                super::initial_allocation::verify_initial_allocation_failures(
                    &failure_controller,
                    failure_config,
                    interrupt_cleanup,
                )
                .await;
            })
            .await
            .expect("complete isolated initial allocation failure assertions");
        }
        // Use the same project-isolated factory as production; no legacy shared-store fixture.
        // 使用与生产相同的项目隔离工厂，不使用旧共享库夹具。
        let current_factory = bootstrap_vce_data_plane(&controller)
            .await
            .expect("bootstrap current VCE factory");
        let first = test_project_config("project-1", &first_root, 1);
        assert!(
            current_factory
                .open_existing_project_index(&first)
                .await
                .expect("read never-created project")
                .is_none()
        );
        assert!(
            current_factory
                .load_active_project_config("project-1")
                .await
                .expect("read missing origin")
                .is_none()
        );
        let first_index = current_factory
            .create_project_index(first)
            .await
            .expect("open first VCE project binding");
        // Inspect only this project's controller-backed metadata through its actual binding.
        // 仅通过此项目的实际绑定检查由控制器支持的元数据。
        let data_plane = first_index.data_plane.clone();
        assert!(
            first_index
                .binding()
                .namespace_id
                .starts_with("vce:project:project-1:root:")
        );
        let second = test_project_config("project-1", &second_root, 2);
        let second_index = data_plane
            .open_project_index(second.clone())
            .await
            .expect("rotate VCE project binding");
        let source_content = "pub fn semantic_entry() {}";
        let blob_name = blob_name("src/lib.rs", source_content)
            .expect("build canonical VCE integration Blob identity");
        let chunk = Chunk::new(
            blob_name.clone(),
            "src/lib.rs",
            source_content,
            1,
            1,
            ChunkKind::Recursive,
        )
        .expect("build VCE integration Chunk");
        let vector = EmbeddingVector {
            input_id: chunk.chunk_id.clone(),
            values: vec![1.0, 0.0, 0.0],
        };
        let indexed_blob = IndexedBlob {
            blob_name: blob_name.clone(),
            path: String::from("src/lib.rs"),
            generation: 1,
            structure_analyzer_id: None,
            structure_mode: None,
            structure_fallback_reason: None,
            chunks: vec![chunk.clone()],
            embedding_vectors: vec![vector.clone()],
            path_document: None,
            path_embedding: None,
            symbols: Vec::new(),
            path_status: AuxiliaryIndexStatus::NotApplicable,
            symbol_status: AuxiliaryIndexStatus::NotApplicable,
            status: IndexedBlobStatus::Ready,
        };
        let stage_request = IndexStageBatch {
            operation_id: String::from("stage-one"),
            refresh_id: String::from("refresh-one"),
            generation: 1,
            blobs: vec![indexed_blob.clone()],
        };
        let stage = second_index
            .stage(stage_request.clone())
            .await
            .expect("stage VCE integration Blob");
        assert_eq!(stage.staged_blob_names, vec![blob_name.clone()]);
        // Identical content under distinct refresh IDs must still have independent cleanup ownership.
        // 不同刷新编号下的相同内容仍必须具有独立清理归属。
        for refresh_id in ["owned-panic", "owned-panic-suffix"] {
            second_index
                .stage(IndexStageBatch {
                    operation_id: format!("{refresh_id}:stage"),
                    refresh_id: refresh_id.to_owned(),
                    generation: 1,
                    blobs: vec![indexed_blob.clone()],
                })
                .await
                .expect("stage independently owned attempts");
        }
        // Only the host-observed exact attempt is in-flight, not an orphan; similar names stay visible.
        // 仅宿主观察到的精确尝试属于在途而非孤立；相似名称仍须显示。
        let observed_issues = second_index
            .management_read_for_refresh(Some("owned-panic"))
            .integrity_issues()
            .await
            .expect("inspect observed staging");
        assert_eq!(observed_issues.len(), 2);
        assert!(
            observed_issues
                .iter()
                .any(|issue| issue.message.contains("owned-panic-suffix"))
        );
        assert!(
            observed_issues
                .iter()
                .any(|issue| issue.message.contains("refresh-one"))
        );
        assert_eq!(
            observed_issues
                .iter()
                .map(|issue| &issue.issue_id)
                .collect::<BTreeSet<_>>()
                .len(),
            2
        );
        // The read-local exclusion must not change the original handle or hide unconfirmed producers.
        // 读取本地排除不能改变原句柄，也不能隐藏未确认生产者。
        let unobserved_issues = second_index
            .integrity_issues()
            .await
            .expect("inspect unconfirmed staging");
        assert_eq!(unobserved_issues.len(), 3);
        assert_eq!(
            unobserved_issues
                .iter()
                .map(|issue| &issue.issue_id)
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
        second_index
            .retry_pending_cleanup()
            .await
            .expect("retry only authorized cleanup");
        // Reopening the binding cannot infer that earlier producers have stopped.
        // 重新打开绑定不能推断先前生产者已经停止。
        let reopened = data_plane
            .open_project_index(second)
            .await
            .expect("reopen without claiming staging");
        assert!(
            reopened
                .has_unresolved_staging()
                .await
                .expect("inspect retained staging")
        );
        for refresh_id in ["refresh-one", "owned-panic", "owned-panic-suffix"] {
            assert_eq!(
                reopened
                    .staged_blob_names(refresh_id, 1)
                    .await
                    .expect("opening preserved exact attempt"),
                BTreeSet::from([blob_name.clone()])
            );
        }
        reopened
            .abort_refresh_staging("owned-panic")
            .await
            .expect("abort only the stopped owned attempt");
        assert!(
            reopened
                .staged_blob_names("owned-panic", 1)
                .await
                .expect("owned staging cleared")
                .is_empty()
        );
        assert_eq!(
            reopened
                .staged_blob_names("owned-panic-suffix", 1)
                .await
                .expect("similar foreign identity survives"),
            BTreeSet::from([blob_name.clone()])
        );
        assert_eq!(
            reopened
                .staged_blob_names("refresh-one", 1)
                .await
                .expect("primary writer survives"),
            BTreeSet::from([blob_name.clone()])
        );
        assert!(reopened.abort_refresh_staging(" ").await.is_err());
        // The test explicitly owns the second fixture too, so it may finish that exact attempt separately.
        // 测试也明确持有第二个夹具，因此可单独结束该精确尝试。
        reopened
            .abort_refresh_staging("owned-panic-suffix")
            .await
            .expect("fixture owner finishes its own attempt");
        drop(reopened);
        let checkpoint = Checkpoint::new(
            second_index.binding().checkpoint_chain_id.clone(),
            1,
            [blob_name.clone()],
        );
        let state_snapshot = VceStateSnapshot::new(
            VceStateIdentity::new(
                second_index.binding().namespace_id.clone(),
                second_index.binding().checkpoint_chain_id.clone(),
                second_index.binding().schema_version,
                second_index.binding().embedding_space.clone(),
            ),
            WorkspaceSnapshot {
                generation: 1,
                checkpoint: Some(checkpoint.clone()),
                records: vec![FileRecord {
                    path: String::from("src/lib.rs"),
                    blob_name: Some(blob_name.clone()),
                    committed_blob_name: Some(blob_name.clone()),
                    status: FileStatus::Present,
                    size: source_content.len() as u64,
                    mtime_ns: None,
                    source: FileSource::Filesystem,
                    content: None,
                    generation: 1,
                    skip_reason: None,
                }],
            },
            VceQueryStatistics {
                refresh_attempts: 1,
                refresh_successes: 1,
                ..VceQueryStatistics::default()
            },
            Some(VceRefreshSummary {
                request_id: String::from("refresh-one"),
                mode: RefreshMode::Full,
                reason: RefreshReason::Initial,
                generation: 1,
                added_blob_count: 1,
                deleted_blob_count: 0,
                indexed_blob_count: 1,
                skipped_path_count: 0,
                checkpoint_version: Some(1),
                source_revision: Some(String::from("source-one")),
            }),
            None,
            None,
        );
        let commit_request = IndexCommit {
            request_id: String::from("commit-one"),
            generation: 1,
            refresh_id: String::from("refresh-one"),
            source_revision: Some(String::from("source-one")),
            deleted_blobs: BTreeSet::new(),
            staged_blob_names: BTreeSet::from([blob_name.clone()]),
            expected_checkpoint_version: None,
            checkpoint,
            state_snapshot: state_snapshot.clone(),
        };
        // A missing host owner is rejected before any checkpoint or lease becomes durable.
        // 缺少宿主所有者时，在任何检查点或租约持久化之前拒绝。
        assert!(
            second_index
                .commit_pinned(commit_request.clone(), " ")
                .await
                .is_err()
        );
        // Inject a failure at the transaction's final lease insertion, after all checkpoint writes.
        // 在检查点全部写入之后、事务最终租约插入处注入失败。
        data_plane.execute(
            "CREATE TRIGGER vce_fixture_reject_read_lease BEFORE INSERT ON vce_read_leases BEGIN SELECT RAISE(ROLLBACK, 'fixture rejects publication lease'); END;",
            Vec::new(), "install fixture lease failure",
        ).await.expect("install lease rejection");
        assert!(
            second_index
                .commit_pinned(commit_request.clone(), "binding-fixture-run")
                .await
                .is_err()
        );
        assert!(
            second_index
                .current_checkpoint_row()
                .await
                .expect("checkpoint after rollback")
                .is_none()
        );
        // Counts prove this is atomic rollback, not a committed checkpoint reported as a failed job.
        // 计数证明这是原子回滚，而不是已提交检查点却报告作业失败。
        let rolled_back: Vec<super::CountRow> = data_plane.query_rows(
            "SELECT (SELECT COUNT(*) FROM vce_read_leases) + (SELECT COUNT(*) FROM vce_read_checkpoints) + (SELECT COUNT(*) FROM vce_blob_versions) + (SELECT COUNT(*) FROM vce_commit_receipts) AS count;",
            Vec::new(),
        ).await.expect("count rolled-back publication writes");
        assert_eq!(rolled_back[0].count, 0);
        assert_eq!(
            second_index
                .staged_blob_names("refresh-one", 1)
                .await
                .expect("retained staging"),
            commit_request.staged_blob_names
        );
        data_plane
            .execute(
                "DROP TRIGGER vce_fixture_reject_read_lease;",
                Vec::new(),
                "remove fixture lease failure",
            )
            .await
            .expect("remove lease rejection");
        // Retry the identical request after rollback; commit returns an already retained query view.
        // 回滚后重试完全相同的请求；提交直接返回已保留的查询视图。
        let (first_commit, committed_read_index) = second_index
            .commit_pinned(commit_request.clone(), "binding-fixture-run")
            .await
            .expect("commit VCE integration Blob");
        assert_eq!(
            committed_read_index
                .embedding_provenance()
                .await
                .expect("retained provenance"),
            second_index
                .embedding_provenance()
                .await
                .expect("durable provenance")
        );
        // committed_leases counts exact host ownership, including failed-attempt cleanup residue.
        // committed_leases 按精确宿主归属计数，包含失败尝试的清理残留。
        let committed_leases: Vec<super::CountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_read_leases WHERE owner_run_id = ?;",
                vec![super::sqlite_text("binding-fixture-run")],
            )
            .await
            .expect("count atomic publication lease");
        assert_eq!(committed_leases[0].count, 1);
        let replayed_commit = second_index
            .commit(commit_request.clone())
            .await
            .expect("replay exact VCE commit idempotently");
        assert_eq!(replayed_commit, first_commit);
        second_index
            .abort_refresh_staging("refresh-one")
            .await
            .expect("owned abort never rolls back a committed checkpoint");
        assert_eq!(
            second_index
                .current_checkpoint_row()
                .await
                .expect("committed checkpoint survives")
                .expect("checkpoint exists")
                .checkpoint_version,
            1
        );
        // fabricatedSource is never staged and therefore has no authoritative storage row.
        // fabricatedSource 从未暂存，因此不存在权威存储行。
        let fabricated_source = "pub fn fabricated() {}";
        // fabricatedBlob represents a canonical identity that has no staged or committed row.
        // fabricatedBlob 表示一个既无暂存行也无已提交行的规范身份。
        let fabricated_blob =
            vulcan_context_engine::blob_name("src/fabricated.rs", fabricated_source)
                .expect("build fabricated Blob identity");
        second_index
            .stage(IndexStageBatch {
                operation_id: String::from("stage-invalid-membership"),
                refresh_id: String::from("refresh-invalid-membership"),
                generation: 2,
                blobs: Vec::new(),
            })
            .await
            .expect("stage empty invalid-membership generation");
        // invalidCheckpoint attempts to add fabricatedBlob without staging it.
        // invalidCheckpoint 尝试在未暂存 fabricatedBlob 的情况下将其加入成员集合。
        let invalid_checkpoint = Checkpoint::new(
            second_index.binding().checkpoint_chain_id.clone(),
            2,
            [blob_name.clone(), fabricated_blob.clone()],
        );
        // invalidState remains internally coherent so the storage transition check is decisive.
        // invalidState 保持内部一致，使存储转换校验成为决定性拒绝条件。
        let mut invalid_state = state_snapshot.clone();
        invalid_state.workspace.generation = 2;
        invalid_state.workspace.checkpoint = Some(invalid_checkpoint.clone());
        invalid_state.workspace.records.push(FileRecord {
            path: String::from("src/fabricated.rs"),
            blob_name: Some(fabricated_blob.clone()),
            committed_blob_name: Some(fabricated_blob),
            status: FileStatus::Present,
            size: fabricated_source.len() as u64,
            mtime_ns: None,
            source: FileSource::Filesystem,
            content: None,
            generation: 2,
            skip_reason: None,
        });
        invalid_state.last_refresh = Some(VceRefreshSummary {
            request_id: String::from("refresh-invalid-membership"),
            mode: RefreshMode::Paths,
            reason: RefreshReason::Manual,
            generation: 2,
            added_blob_count: 0,
            deleted_blob_count: 0,
            indexed_blob_count: 0,
            skipped_path_count: 0,
            checkpoint_version: Some(2),
            source_revision: Some(String::from("source-invalid-membership")),
        });
        second_index
            .commit(IndexCommit {
                request_id: String::from("commit-invalid-membership"),
                generation: 2,
                refresh_id: String::from("refresh-invalid-membership"),
                source_revision: Some(String::from("source-invalid-membership")),
                deleted_blobs: BTreeSet::new(),
                staged_blob_names: BTreeSet::new(),
                expected_checkpoint_version: Some(1),
                checkpoint: invalid_checkpoint,
                state_snapshot: invalid_state,
            })
            .await
            .expect_err("fabricated checkpoint membership must be rejected");
        second_index
            .abort_stage(IndexAbortRequest {
                operation_id: String::from("abort-invalid-membership"),
                refresh_id: String::from("refresh-invalid-membership"),
                generation: 2,
            })
            .await
            .expect("abort rejected membership generation");
        second_index
            .stage(stage_request)
            .await
            .expect_err("committed VCE generation must reject staging replay");
        second_index
            .abort_stage(IndexAbortRequest {
                operation_id: String::from("abort-committed"),
                refresh_id: String::from("refresh-one"),
                generation: 1,
            })
            .await
            .expect_err("committed VCE generation must reject abort");
        let persisted_state = data_plane
            .load_state_snapshot(&second_index.binding().namespace_id)
            .await
            .expect("load atomically committed VCE state")
            .expect("committed VCE state should exist");
        assert_eq!(persisted_state, state_snapshot);
        // original is read from persisted origin rather than fabricated from the current model.
        // original 来自持久化原始来源，而不是根据当前模型编造。
        let original = data_plane
            .load_active_project_config("project-1")
            .await
            .expect("read original contract")
            .expect("existing project");
        // current_settings changes only the host settings revision; read-only restore must not save it.
        // current_settings 仅改变宿主设置修订；只读恢复不得保存它。
        let mut current_settings = original.clone();
        current_settings.config_revision += 1;
        // restored is a read-only restoration of the existing binding, without cleanup or replacement.
        // restored 是现有绑定的只读恢复，不执行清理或替换。
        let restored = data_plane
            .open_existing_project_index(&current_settings)
            .await
            .expect("restore read-only binding")
            .expect("existing index");
        assert_eq!(restored.config_revision, original.config_revision);
        assert_eq!(
            data_plane
                .load_active_project_config("project-1")
                .await
                .expect("reread original"),
            Some(original.clone())
        );
        // incompatible must fail without deleting the previous state or changing active provenance.
        // incompatible 必须失败，且不能删除之前状态或改变活动来源。
        let mut incompatible = original.clone();
        incompatible.dimension += 1;
        assert!(
            data_plane
                .open_existing_project_index(&incompatible)
                .await
                .is_err()
        );
        assert_eq!(
            data_plane
                .load_state_snapshot(&original.binding.namespace_id)
                .await
                .expect("state after rejected restore"),
            Some(state_snapshot.clone())
        );
        assert_eq!(
            data_plane
                .load_active_project_config("project-1")
                .await
                .expect("origin after rejected restore"),
            Some(original)
        );
        // Pin the same exact checkpoint for vector selection and subsequent body materialization.
        // 为向量选择及后续正文物化固定同一个精确检查点。
        let read_index = second_index
            .pin_checkpoint(
                state_snapshot
                    .workspace
                    .checkpoint
                    .clone()
                    .expect("committed checkpoint"),
                "binding-fixture-run",
            )
            .await
            .expect("pin committed storage");
        let search = read_index
            .search_dense(DenseSearchRequest {
                request_id: String::from("search-one"),
                query_index: 0,
                query_vector: EmbeddingVector {
                    input_id: String::from("query-one"),
                    values: vec![1.0, 0.0, 0.0],
                },
                allowed_blobs: BTreeSet::from([blob_name.clone()]),
                limit: 4,
            })
            .await
            .expect("search committed VCE vector");
        assert_eq!(search.hits.len(), 1);
        assert_eq!(
            committed_read_index
                .search_dense(DenseSearchRequest {
                    request_id: String::from("search-atomic-publication"),
                    query_index: 0,
                    query_vector: EmbeddingVector {
                        input_id: String::from("query-atomic"),
                        values: vec![1.0, 0.0, 0.0]
                    },
                    allowed_blobs: BTreeSet::from([blob_name.clone()]),
                    limit: 4,
                })
                .await
                .expect("query lease returned directly by commit")
                .hits
                .len(),
            1
        );
        println!(
            "VCE_STORAGE sqlite_bytes={} lancedb_bytes={} total_bytes={}",
            file_or_directory_size(&data_plane.sqlite_path),
            file_or_directory_size(&data_plane.lancedb_path),
            file_or_directory_size(&data_plane.data_root),
        );
        let mut aborted_blob = indexed_blob.clone();
        aborted_blob.generation = 2;
        second_index
            .stage(IndexStageBatch {
                operation_id: String::from("stage-two"),
                refresh_id: String::from("refresh-two"),
                generation: 2,
                blobs: vec![aborted_blob],
            })
            .await
            .expect("stage abortable VCE Blob");
        second_index
            .abort_stage(IndexAbortRequest {
                operation_id: String::from("abort-two"),
                refresh_id: String::from("refresh-two"),
                generation: 2,
            })
            .await
            .expect("abort VCE staging generation");
        #[derive(serde::Deserialize)]
        struct BindingCountRow {
            /// Number of matching rows.
            /// 匹配行数量。
            count: i64,
        }
        let active: Vec<BindingCountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_project_bindings WHERE project_id = ? AND active = 1;",
                vec![super::sqlite_text("project-1")],
            )
            .await
            .expect("count active VCE bindings");
        let orphaned: Vec<BindingCountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_project_bindings WHERE project_id = ? AND lifecycle_state = 'orphaned';",
                vec![super::sqlite_text("project-1")],
            )
            .await
            .expect("count orphaned VCE bindings");
        let pending_cleanup: Vec<BindingCountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_vector_cleanup WHERE namespace_id = ? AND state = 'pending';",
                vec![super::sqlite_text(&second_index.binding().namespace_id)],
            )
            .await
            .expect("count pending VCE vector cleanup rows");
        assert_eq!(active[0].count, 1);
        assert_eq!(orphaned[0].count, 1);
        assert_eq!(pending_cleanup[0].count, 0);
        // Exercise unchanged, replaced and removed producers through the atomic publication API.
        // 通过原子发布接口验证未变化、已替换及已移除的生产来源。
        let mut approved_space = second_index.binding.embedding_space.clone();
        approved_space.physical_fingerprint = Some(String::from("fixture-approved-producer-b"));
        // approved_writer changes actual production only under the explicit compatibility grant.
        // approved_writer 仅在明确兼容授权下改变实际生产来源。
        let approved_writer = second_index
            .clone()
            .with_embedding_execution(
                vulcan_context_engine::VceEmbeddingExecutionContract::new(
                    second_index.binding.clone(),
                    approved_space,
                    "fixture-approved-space-switch",
                )
                .expect("construct explicit fixture grant"),
            )
            .expect("bind approved writer");
        // Keep every publication alive to test source provenance independently of later commits.
        // 保持各次发布存活，以验证来源信息独立于后续提交。
        let mut retained_publications = Vec::new();
        for generation in 2..=4 {
            // Only the middle update replaces vectors; the last update removes the member entirely.
            // 仅中间一次更新替换向量；最后一次更新彻底移除成员。
            let replaced = generation == 3;
            // removed selects the initialized empty checkpoint at the final publication.
            // removed 在最终发布时选择已初始化的空检查点。
            let removed = generation == 4;
            // refresh_id distinguishes this exact attempt from previous staging batches.
            // refresh_id 将本次精确尝试与先前暂存批次区分开。
            let refresh_id = format!("atomic-publication-{generation}");
            // staged_blob preserves content identity while assigning the new producer attempt.
            // staged_blob 保持内容身份，并分配新的生产尝试。
            let mut staged_blob = indexed_blob.clone();
            staged_blob.generation = generation;
            approved_writer
                .stage(IndexStageBatch {
                    operation_id: format!("stage-{refresh_id}"),
                    refresh_id: refresh_id.clone(),
                    generation,
                    blobs: if replaced {
                        vec![staged_blob]
                    } else {
                        Vec::new()
                    },
                })
                .await
                .expect("stage exact publication attempt");
            // members is the exact surviving file set at this publication.
            // members 是本次发布精确存活的文件集合。
            let members = if removed {
                BTreeSet::new()
            } else {
                BTreeSet::from([blob_name.clone()])
            };
            // checkpoint remains in the original physical chain under the approved producer.
            // checkpoint 在已批准生产者下仍保留在原始物理链中。
            let checkpoint = Checkpoint::new(
                second_index.binding.checkpoint_chain_id.clone(),
                generation,
                members,
            );
            // next_snapshot describes the same full-refresh result as the exact commit below.
            // next_snapshot 描述与下方精确提交相同的全量刷新结果。
            let mut next_snapshot = state_snapshot.clone();
            next_snapshot.workspace.generation = generation;
            next_snapshot.workspace.checkpoint = Some(checkpoint.clone());
            if removed {
                next_snapshot.workspace.records.clear();
            }
            next_snapshot.statistics.refresh_attempts = generation;
            next_snapshot.statistics.refresh_successes = generation;
            next_snapshot.last_refresh = Some(VceRefreshSummary {
                request_id: refresh_id.clone(),
                mode: RefreshMode::Full,
                reason: RefreshReason::Manual,
                generation,
                added_blob_count: 0,
                deleted_blob_count: usize::from(removed),
                indexed_blob_count: usize::from(replaced),
                skipped_path_count: 0,
                checkpoint_version: Some(generation),
                source_revision: None,
            });
            if generation == 2 {
                // guarded_commit is valid before a binding changes between preflight and publication.
                // guarded_commit 在预检与发布之间发生绑定变化前是有效提交。
                let guarded_commit = IndexCommit {
                    request_id: format!("commit-{refresh_id}"),
                    refresh_id: refresh_id.clone(),
                    generation,
                    source_revision: None,
                    deleted_blobs: BTreeSet::new(),
                    staged_blob_names: BTreeSet::new(),
                    expected_checkpoint_version: Some(1),
                    checkpoint: checkpoint.clone(),
                    state_snapshot: next_snapshot.clone(),
                };
                // guarded_lease freezes the valid predecessor before simulating an external binding change.
                // guarded_lease 在模拟外部绑定变化前固定有效前驱。
                let guarded_lease = approved_writer
                    .prepare_publication_lease(&guarded_commit, "binding-fixture-run")
                    .await
                    .expect("prepare guarded publication");
                // guarded_script uses the real fingerprint so rejection cannot come from a malformed receipt.
                // guarded_script 使用真实指纹，使拒绝不可能来自格式错误的回执。
                let guarded_script = approved_writer
                    .build_commit_script(
                        &guarded_commit,
                        &super::commit_request_fingerprint(&guarded_commit)
                            .expect("fingerprint guarded commit"),
                        &[],
                        Some(&guarded_lease),
                    )
                    .expect("build guarded publication");
                // Each exact contract field must be fenced independently under the transaction's write lock.
                // 每个精确契约字段均须在事务写锁内独立受到保护。
                for (change, restore) in [
                    (String::from("active = 0"), String::from("active = 1")),
                    (
                        String::from("config_revision = config_revision + 1"),
                        String::from("config_revision = config_revision - 1"),
                    ),
                    (
                        String::from("physical_fingerprint = 'changed-fixture'"),
                        format!(
                            "physical_fingerprint = '{}'",
                            super::sqlite_string_literal(&approved_writer.physical_fingerprint)
                        ),
                    ),
                    (
                        String::from("dimension = dimension + 1"),
                        String::from("dimension = dimension - 1"),
                    ),
                    (
                        String::from("content_table = 'changed_content_fixture'"),
                        format!(
                            "content_table = '{}'",
                            super::sqlite_string_literal(&approved_writer.content_table)
                        ),
                    ),
                    (
                        String::from("path_table = 'changed_path_fixture'"),
                        format!(
                            "path_table = '{}'",
                            super::sqlite_string_literal(&approved_writer.path_table)
                        ),
                    ),
                ] {
                    approved_writer
                        .data_plane
                        .execute(
                            &format!(
                                "UPDATE vce_project_bindings SET {change} WHERE namespace_id = ?;"
                            ),
                            vec![super::sqlite_text(&approved_writer.binding.namespace_id)],
                            "change fixture binding after admission",
                        )
                        .await
                        .expect("change fixture binding");
                    assert!(
                        approved_writer
                            .data_plane
                            .execute(&guarded_script, Vec::new(), "reject stale fixture writer")
                            .await
                            .is_err(),
                        "publication must reject changed binding: {change}"
                    );
                    // effects proves the rejected transaction persisted neither checkpoint, receipt nor lease.
                    // effects 证明被拒绝事务既未持久化检查点，也未持久化回执或租约。
                    let effects: Vec<BindingCountRow> = approved_writer.data_plane.query_rows(
                        "SELECT (SELECT COUNT(*) FROM vce_read_checkpoints WHERE namespace_id = ? AND checkpoint_version = 2) + (SELECT COUNT(*) FROM vce_commit_receipts WHERE namespace_id = ? AND operation_id = ?) + (SELECT COUNT(*) FROM vce_read_leases WHERE namespace_id = ? AND checkpoint_version = 2) AS count;",
                        vec![super::sqlite_text(&approved_writer.binding.namespace_id), super::sqlite_text(&approved_writer.binding.namespace_id), super::sqlite_text(&guarded_commit.request_id), super::sqlite_text(&approved_writer.binding.namespace_id)],
                    ).await.expect("inspect rejected publication effects");
                    assert_eq!(
                        effects[0].count, 0,
                        "stale publication must roll back completely"
                    );
                    assert_eq!(
                        approved_writer
                            .current_checkpoint_row()
                            .await
                            .expect("read retained predecessor")
                            .expect("predecessor remains")
                            .checkpoint_version,
                        1
                    );
                    approved_writer
                        .data_plane
                        .execute(
                            &format!(
                                "UPDATE vce_project_bindings SET {restore} WHERE namespace_id = ?;"
                            ),
                            vec![super::sqlite_text(&approved_writer.binding.namespace_id)],
                            "restore exact fixture binding",
                        )
                        .await
                        .expect("restore fixture binding");
                }
                drop(guarded_lease);
                // An explicitly aborted empty attempt must not become publishable just because no blobs remain.
                // 明确中止的空尝试不能仅因没有剩余块对象就重新变为可发布。
                // stage_first separates empty staged cancellation from cancellation before any staging call.
                // stage_first 区分空暂存后的取消与任何暂存调用之前的取消。
                for (cancelled_refresh, stage_first) in [
                    ("cancelled-empty-publication", true),
                    ("cancelled-before-staging", false),
                ] {
                    if stage_first {
                        approved_writer
                            .stage(IndexStageBatch {
                                operation_id: format!("stage-{cancelled_refresh}"),
                                refresh_id: cancelled_refresh.to_owned(),
                                generation,
                                blobs: Vec::new(),
                            })
                            .await
                            .expect("stage cancelled empty attempt");
                    }
                    approved_writer
                        .abort_stage(IndexAbortRequest {
                            operation_id: format!("abort-{cancelled_refresh}"),
                            refresh_id: cancelled_refresh.to_owned(),
                            generation,
                        })
                        .await
                        .expect("cancel empty attempt");
                    // cancelled_snapshot is otherwise a valid successor so the attempt-state guard is decisive.
                    // cancelled_snapshot 在其他方面均是有效后继，使尝试状态保护成为决定性条件。
                    let mut cancelled_snapshot = next_snapshot.clone();
                    cancelled_snapshot
                        .last_refresh
                        .as_mut()
                        .expect("fixture refresh summary")
                        .request_id = cancelled_refresh.to_owned();
                    // cancelled_commit retains valid checkpoint data but names the terminal aborted attempt.
                    // cancelled_commit 保留有效检查点数据，但指向已终止的中止尝试。
                    let cancelled_commit = IndexCommit {
                        request_id: format!("commit-{cancelled_refresh}"),
                        refresh_id: cancelled_refresh.to_owned(),
                        generation,
                        source_revision: None,
                        deleted_blobs: BTreeSet::new(),
                        staged_blob_names: BTreeSet::new(),
                        expected_checkpoint_version: Some(1),
                        checkpoint: checkpoint.clone(),
                        state_snapshot: cancelled_snapshot,
                    };
                    // cancelled_script bypasses preflight to verify the transaction's independent abort fence.
                    // cancelled_script 绕过预检，验证事务自身独立的中止保护。
                    let cancelled_script = approved_writer
                        .build_commit_script(
                            &cancelled_commit,
                            &super::commit_request_fingerprint(&cancelled_commit)
                                .expect("fingerprint cancelled fixture"),
                            &[],
                            None,
                        )
                        .expect("build cancelled transaction fixture");
                    assert!(
                        approved_writer
                            .data_plane
                            .execute(
                                &cancelled_script,
                                Vec::new(),
                                "cancelled transaction fixture"
                            )
                            .await
                            .is_err(),
                        "transaction must reject an aborted empty attempt"
                    );
                    assert!(
                        approved_writer
                            .commit_pinned(cancelled_commit, "binding-fixture-run")
                            .await
                            .is_err(),
                        "aborted empty attempt must never commit"
                    );
                    assert_eq!(
                        approved_writer
                            .generation_state(
                                cancelled_refresh,
                                generation,
                                VcePortOperation::Commit
                            )
                            .await
                            .expect("read cancelled generation")
                            .as_deref(),
                        Some("aborted")
                    );
                    assert_eq!(
                        approved_writer
                            .current_checkpoint_row()
                            .await
                            .expect("read unchanged checkpoint")
                            .expect("original checkpoint")
                            .checkpoint_version,
                        1
                    );
                }
            }
            // published_read must be usable immediately without another lease admission.
            // published_read 必须无需再次准入租约即可立即使用。
            let (_, published_read) = approved_writer
                .commit_pinned(
                    IndexCommit {
                        request_id: format!("commit-{refresh_id}"),
                        refresh_id: refresh_id.clone(),
                        generation,
                        source_revision: None,
                        deleted_blobs: if removed {
                            BTreeSet::from([blob_name.clone()])
                        } else {
                            BTreeSet::new()
                        },
                        staged_blob_names: if replaced {
                            BTreeSet::from([blob_name.clone()])
                        } else {
                            BTreeSet::new()
                        },
                        expected_checkpoint_version: Some(generation - 1),
                        checkpoint,
                        state_snapshot: next_snapshot,
                    },
                    "binding-fixture-run",
                )
                .await
                .expect("commit and retain exact producers");
            // Even unstaged publications retain a terminal generation identity against later reuse.
            // 即使未经暂存的发布也保留终态代次身份，阻止后续复用。
            assert_eq!(
                approved_writer
                    .generation_state(&refresh_id, generation, VcePortOperation::Commit)
                    .await
                    .expect("read committed generation")
                    .as_deref(),
                Some("committed")
            );
            // provenance is frozen in memory and must exactly match the durable checkpoint.
            // provenance 在内存中固定，必须与持久检查点精确一致。
            let provenance = published_read
                .embedding_provenance()
                .await
                .expect("frozen producer sources");
            assert_eq!(
                provenance,
                approved_writer
                    .embedding_provenance()
                    .await
                    .expect("durable producer sources")
            );
            assert_eq!(
                provenance
                    .source_fingerprints
                    .contains("fixture-approved-producer-b"),
                replaced
            );
            assert_eq!(provenance.checkpoint_version, Some(generation));
            retained_publications.push(published_read);
        }
        assert_eq!(
            committed_read_index
                .embedding_provenance()
                .await
                .expect("original frozen sources")
                .checkpoint_version,
            Some(1)
        );
        assert!(
            retained_publications[1]
                .embedding_provenance()
                .await
                .expect("retained B sources")
                .source_fingerprints
                .contains("fixture-approved-producer-b")
        );
        assert_eq!(
            retained_publications[1]
                .search_dense(DenseSearchRequest {
                    request_id: String::from("search-retained-atomic-rebuild"),
                    query_index: 0,
                    query_vector: EmbeddingVector {
                        input_id: String::from("query-retained"),
                        values: vec![1.0, 0.0, 0.0]
                    },
                    allowed_blobs: BTreeSet::from([blob_name.clone()]),
                    limit: 4,
                })
                .await
                .expect("query replaced producer after empty publication")
                .hits
                .len(),
            1
        );
        // candidate_fixture retains exact inputs for a separately polled stage on this same runtime.
        // candidate_fixture 在同一运行时上为单独轮询的阶段保留精确输入。
        let candidate_fixture = (
            current_factory.clone(),
            data_plane.clone(),
            approved_writer.clone(),
            retained_publications.clone(),
            indexed_blob.clone(),
            commit_request.clone(),
            blob_name.clone(),
        );
        tokio::spawn(async move {
            // Separate polling bounds nested debug-stack use; this awaited stage adds no threads or concurrent indexing.
            // 单独轮询限制嵌套调试栈使用；此被等待的阶段不增加线程或并发索引。
            let (factory, data, writer, reads, blob, commit, name) = candidate_fixture;
            verify_candidate_publication(&factory, &data, &writer, &reads, &blob, &commit, &name)
                .await;
        })
        .await
        .expect("candidate publication stage completed");
        drop(retained_publications);
        drop(approved_writer);
        // Drain the published read view before the production factory closes its physical binding.
        // 在生产工厂关闭物理绑定之前排空已发布读取视图。
        drop(read_index);
        drop(committed_read_index);
        drop(restored);
        drop(first_index);
        drop(second_index);
        drop(data_plane);
        current_factory
            .shutdown()
            .await
            .expect("close current-format fixture");
        client.shutdown().await.expect("shutdown VCE controller");
        assert_eq!(VCE_SPACE_ID, "vce");
    }

    /// Verify a rejected initial table allocation preserves foreign tables and permits explicit retry.
    /// 验证被拒绝的首次表分配保留外部表，并允许明确重试。
    /// `controller` owns temporary fixture storage; `root` is the existing temporary source root.
    /// `controller` 持有临时夹具存储；`root` 是已存在的临时源码根目录。
    /// Returns boxed assertions, closing every additional binding before reporting a retry failure.
    /// 返回装箱断言，在报告重试失败前关闭全部额外绑定。
    fn verify_initial_allocation_retry<'a>(
        controller: &'a ControllerRuntimeHandle,
        root: &'a std::path::Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // Each collision owns a separate stable project so no attempt can borrow another table.
            // 每种碰撞持有独立稳定项目，使尝试不能借用其他项目的表。
            for channel in ["content", "path"] {
                // Config is the same deterministic identity used by both explicit creation attempts.
                // config 是两次明确创建尝试共同使用的确定性身份。
                let config = test_project_config(&format!("initial-collision-{channel}"), root, 1);
                // The catalog allocates directories only; it does not publish an initialized index.
                // catalog 仅分配目录，不发布已初始化索引。
                let catalog = crate::vce_storage_catalog::VceStorageCatalog::bootstrap(controller)
                    .await
                    .expect("open initial collision catalog");
                // Paths come from the real catalog rather than a guessed project directory.
                // paths 来自真实目录服务，而不是猜测的项目目录。
                let paths = catalog
                    .allocate_project(&config.project_id)
                    .await
                    .expect("allocate isolated initial collision paths");
                // The fixture's binding installs only schema and its own deliberate collision.
                // 夹具绑定仅安装结构及其自有的有意碰撞。
                let store = super::prepare_vce_storage_binding(
                    controller.client.clone(),
                    paths.root,
                    config.project_id.clone(),
                    true,
                )
                .expect("prepare initial collision binding");
                store
                    .activate_bindings(true)
                    .await
                    .expect("activate isolated initial collision schema");
                catalog
                    .close()
                    .await
                    .expect("close collision catalog binding");
                // The colliding table belongs to this fixture, never to the rejected allocator.
                // 冲突表属于本夹具，绝不属于被拒绝的分配器。
                let foreign_table = vector_table_name(
                    channel,
                    &config.vector_space_id,
                    &config.physical_fingerprint,
                    &config.preprocessing_version,
                    &config.chunker_version,
                    config.dimension,
                );
                store
                    .ensure_vector_table(&foreign_table, config.dimension)
                    .await
                    .expect("create fixture-owned initial collision table");
                // Factory is the public production creation boundary with the real project writer lock.
                // factory 是使用真实项目写入锁的公开生产创建边界。
                let factory = bootstrap_vce_data_plane(controller)
                    .await
                    .expect("open initial collision factory");
                assert!(factory.create_project_index(config.clone()).await.is_err());
                assert!(
                    store
                        .load_active_project_config(&config.project_id)
                        .await
                        .expect("inspect rejected initial allocation")
                        .is_none(),
                    "failed allocation must not publish an initial binding"
                );
                assert!(
                    store
                        .ensure_vector_table(&foreign_table, config.dimension)
                        .await
                        .is_err(),
                    "failed allocation must preserve the fixture-owned collision"
                );
                // Only the fixture removes its exact collision after proving the allocator preserved it.
                // 仅夹具在证明分配器保留冲突表后移除其精确自有冲突。
                store
                    .client
                    .drop_lancedb_table(
                        VCE_SPACE_ID,
                        store.lancedb_binding_id.as_str(),
                        foreign_table.as_str(),
                    )
                    .await
                    .expect("remove only the fixture-owned collision");
                // Capture failure before closing bindings, so a red regression leaves no live fixture owner.
                // 在关闭绑定前捕获失败，使红色回归不遗留存活夹具所有者。
                let retry_error = factory.create_project_index(config).await.err();
                factory
                    .shutdown()
                    .await
                    .expect("close initial retry factory");
                store
                    .close_bindings()
                    .await
                    .expect("close initial collision binding");
                assert!(
                    retry_error.is_none(),
                    "explicit initial retry after removing {channel} collision must succeed: {retry_error:?}"
                );
            }
        })
    }

    /// Verify isolated candidate publication while preserving the predecessor's pinned queries.
    /// 验证隔离候选发布，同时保留前驱固定查询。
    /// Inputs identify real storage, its writer, retained reads and the original blob/commit. Returns boxed assertions.
    /// 参数标识真实存储、写入方、保留读取及原始代码块／提交。返回装箱断言任务。
    fn verify_candidate_publication<'a>(
        current_factory: &'a super::VceDataPlaneFactory,
        data_plane: &'a super::VceDataPlaneHandle,
        approved_writer: &'a super::VceProjectIndex,
        retained_publications: &'a [super::VceProjectIndex],
        indexed_blob: &'a IndexedBlob,
        commit_request: &'a IndexCommit,
        blob_name: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // original is the published identity that candidate allocation must leave unchanged.
            // original 是候选分配必须保持不变的已发布身份。
            let original = data_plane
                .load_active_project_config(&approved_writer.project_id)
                .await
                .expect("load rebuild predecessor")
                .expect("published predecessor exists");
            // candidate_config intentionally selects distinct physical tables and another explicit dimension.
            // candidate_config 明确选择独立物理表及另一明确维度。
            let mut candidate_config = original.clone();
            candidate_config.binding.namespace_id.push_str(":candidate");
            candidate_config
                .binding
                .checkpoint_chain_id
                .push_str(":candidate");
            candidate_config.vector_space_id.push_str(":candidate");
            candidate_config.binding.embedding_space.vector_space_id =
                candidate_config.vector_space_id.clone();
            candidate_config.dimension = 4;
            candidate_config.binding.embedding_space.dimension = 4;
            candidate_config.physical_fingerprint.push_str(":candidate");
            candidate_config
                .binding
                .embedding_space
                .physical_fingerprint = Some(candidate_config.physical_fingerprint.clone());
            // candidate_base captures the exact last publication rather than guessing from settings revision.
            // candidate_base 捕获精确最近发布，而不是从设置修订猜测。
            let candidate_base = super::VceRebuildCandidateBase {
                namespace_id: original.binding.namespace_id.clone(),
                checkpoint_version: 4,
                job_id: String::from("candidate-rebuild-job"),
            };
            // Isolate this partial-allocation phase on an awaited task without increasing the default thread stack.
            // 将部分分配阶段隔离到被等待的任务，不增加默认线程栈。
            tokio::spawn(
                super::candidate_allocation::verify_partial_candidate_allocation(
                    current_factory.clone(),
                    data_plane.clone(),
                    approved_writer.clone(),
                    candidate_config.clone(),
                    candidate_base.clone(),
                ),
            )
            .await
            .expect("partial candidate allocation fixture completed");
            assert!(
                current_factory
                    .prepare_rebuild_candidate(candidate_config.clone(), candidate_base.clone())
                    .await
                    .expect_err("candidate must not allocate before durable admission")
                    .contains("durable replacement job")
            );
            // candidate_jobs freezes the physical target before allocation and keeps it immutable during progress updates.
            // candidate_jobs 在分配前固定物理目标，并在进度更新期间保持其不可变。
            let candidate_jobs = approved_writer.job_store();
            let queued_candidate = super::jobs::replacement_test_receipt(
                candidate_config.clone(),
                candidate_base.clone(),
            );
            candidate_jobs
                .register(&queued_candidate)
                .await
                .expect("persist exact replacement before allocation");
            assert_eq!(
                candidate_jobs
                    .get(&candidate_base.job_id)
                    .await
                    .expect("read durable replacement"),
                Some(queued_candidate.clone())
            );
            // cancelled_before_allocation owns a separate receipt and must never create the same pending target.
            // cancelled_before_allocation 持有独立回执，绝不能创建相同待分配目标。
            let mut cancelled_before_allocation = queued_candidate.clone();
            cancelled_before_allocation
                .job_id
                .push_str(":cancel-before-allocation");
            cancelled_before_allocation.idempotency_key =
                cancelled_before_allocation.job_id.clone();
            cancelled_before_allocation
                .replacement
                .as_mut()
                .expect("replacement target")
                .predecessor
                .job_id = cancelled_before_allocation.job_id.clone();
            candidate_jobs
                .register(&cancelled_before_allocation)
                .await
                .expect("register cancelled allocation fixture");
            let mut allocation_cancelled = cancelled_before_allocation.clone();
            allocation_cancelled.sequence += 1;
            allocation_cancelled.state = super::VceJobState::Cancelled;
            candidate_jobs
                .update(&cancelled_before_allocation, &allocation_cancelled)
                .await
                .expect("cancel before allocating");
            assert!(
                current_factory
                    .prepare_rebuild_candidate(
                        candidate_config.clone(),
                        allocation_cancelled
                            .replacement
                            .expect("cancelled target")
                            .predecessor,
                    )
                    .await
                    .expect_err("cancelled receipt cannot allocate")
                    .contains("admitted replacement receipt")
            );
            let unallocated: Vec<super::CountRow> = data_plane
                .query_rows(
                    "SELECT COUNT(*) AS count FROM vce_project_bindings WHERE namespace_id = ?;",
                    vec![super::sqlite_text(&candidate_config.binding.namespace_id)],
                )
                .await
                .expect("inspect absent cancelled allocation");
            assert_eq!(unallocated[0].count, 0);
            // changed_target remains structurally valid but cannot retarget an admitted job.
            // changed_target 结构仍有效，但不能改写已准入作业目标。
            let mut changed_target = queued_candidate.clone();
            changed_target.sequence += 1;
            changed_target.state = super::VceJobState::Running;
            changed_target
                .replacement
                .as_mut()
                .expect("replacement exists")
                .target
                .binding
                .namespace_id
                .push_str(":changed");
            changed_target
                .validate()
                .expect("changed identity is structurally valid");
            assert!(
                candidate_jobs
                    .update(&queued_candidate, &changed_target)
                    .await
                    .is_err()
            );
            let wrong_target = changed_target
                .replacement
                .expect("changed replacement")
                .target;
            assert!(
                current_factory
                    .prepare_rebuild_candidate(wrong_target, candidate_base.clone())
                    .await
                    .is_err()
            );
            // stale_base must be rejected before any new physical table is created.
            // stale_base 必须在创建任何新物理表之前被拒绝。
            let mut stale_base = candidate_base.clone();
            stale_base.checkpoint_version = 3;
            assert!(
                current_factory
                    .prepare_rebuild_candidate(candidate_config.clone(), stale_base)
                    .await
                    .is_err()
            );
            // candidate remains invisible to active-project discovery and cannot bypass the ordinary port gate.
            // candidate 对活动项目发现不可见，且不能绕过普通端口门禁。
            let candidate = current_factory
                .prepare_rebuild_candidate(candidate_config.clone(), candidate_base.clone())
                .await
                .expect("prepare inactive different-dimension candidate");
            assert_eq!(
                current_factory
                    .load_active_project_config(&original.project_id)
                    .await
                    .expect("read published pointer after candidate creation"),
                Some(original.clone())
            );
            assert!(
                data_plane
                    .open_project_index(candidate_config.clone())
                    .await
                    .is_err(),
                "ordinary opening must not activate a pending candidate"
            );
            assert!(
                candidate
                    .find_missing(vulcan_context_engine::FindMissingRequest {
                        request_id: String::from("candidate-ordinary-read"),
                        blob_names: BTreeSet::new(),
                    })
                    .await
                    .is_err(),
                "candidate cannot enter the ordinary active index pipeline"
            );
            // candidate_rows verifies durable exact predecessor/job ownership without changing the active pointer.
            // candidate_rows 验证持久精确前驱／作业归属，且不改变活动指针。
            let candidate_rows: Vec<super::CountRow> = data_plane.query_rows(
                "SELECT COUNT(*) AS count FROM vce_rebuild_candidates c JOIN vce_project_bindings b ON b.namespace_id = c.namespace_id WHERE c.namespace_id = ? AND c.base_namespace_id = ? AND c.base_checkpoint_version = 4 AND c.job_id = 'candidate-rebuild-job' AND c.state = 'building' AND b.active = 0;",
                vec![super::sqlite_text(&candidate_config.binding.namespace_id), super::sqlite_text(&original.binding.namespace_id)],
            ).await.expect("read candidate ownership");
            assert!(matches!(candidate_rows.as_slice(), [row] if row.count == 1));
            // foreign_base cannot borrow a candidate allocated to another durable host job.
            // foreign_base 不能借用分配给另一持久宿主作业的候选。
            let mut foreign_base = candidate_base.clone();
            foreign_base.job_id.push_str("-foreign");
            assert!(
                candidate
                    .admit_rebuild_candidate_writer(foreign_base)
                    .await
                    .is_err()
            );
            // candidate_writer is explicitly admitted; the original inactive handle remains unauthorized.
            // candidate_writer 已显式准入；原始非活动句柄仍未获授权。
            let candidate_writer = candidate
                .admit_rebuild_candidate_writer(candidate_base)
                .await
                .expect("admit exact candidate writer");
            // candidate_blob reuses content while writing vectors only into the new four-dimensional tables.
            // candidate_blob 复用内容，但仅向新的四维表写入向量。
            let mut candidate_blob = indexed_blob.clone();
            candidate_blob.generation = 1;
            for vector in &mut candidate_blob.embedding_vectors {
                vector.values.push(0.0);
            }
            if let Some(vector) = &mut candidate_blob.path_embedding {
                vector.values.push(0.0);
            }
            candidate_writer
                .stage(IndexStageBatch {
                    operation_id: String::from("stage-candidate"),
                    refresh_id: String::from("candidate-refresh"),
                    generation: 1,
                    blobs: vec![candidate_blob],
                })
                .await
                .expect("stage four-dimensional candidate");
            // candidate_commit publishes the same content under the new physical binding and checkpoint chain.
            // candidate_commit 在新物理绑定及检查点链下发布相同内容。
            let mut candidate_commit = commit_request.clone();
            candidate_commit.request_id = String::from("commit-candidate");
            candidate_commit.refresh_id = String::from("candidate-refresh");
            candidate_commit.checkpoint.chain_id =
                candidate_config.binding.checkpoint_chain_id.clone();
            candidate_commit.state_snapshot.identity = VceStateIdentity::new(
                candidate_config.binding.namespace_id.clone(),
                candidate_config.binding.checkpoint_chain_id.clone(),
                candidate_config.binding.schema_version,
                candidate_config.binding.embedding_space.clone(),
            );
            candidate_commit.state_snapshot.workspace.checkpoint =
                Some(candidate_commit.checkpoint.clone());
            candidate_commit
                .state_snapshot
                .last_refresh
                .as_mut()
                .expect("candidate summary")
                .request_id = candidate_commit.refresh_id.clone();
            assert!(
                candidate_writer
                    .commit(candidate_commit.clone())
                    .await
                    .is_err(),
                "candidate requires atomic host publication lease"
            );
            // candidate_lease and script are prepared before faults to exercise independent transactional fencing.
            // candidate_lease 及脚本在故障前准备，用于验证独立事务保护。
            let candidate_lease = candidate_writer
                .prepare_publication_lease(&candidate_commit, "candidate-run")
                .await
                .expect("prepare candidate origin and lease");
            let candidate_script = candidate_writer
                .build_commit_script(
                    &candidate_commit,
                    &super::commit_request_fingerprint(&candidate_commit)
                        .expect("candidate fingerprint"),
                    &[],
                    Some(&candidate_lease),
                )
                .expect("candidate publication script");
            // Advance the predecessor after admission, then restore it to test the cancellation ledger separately.
            // 在准入后改变前驱，再恢复，以单独测试取消台账。
            for (change, restore) in [
                (
                    format!(
                        "UPDATE vce_active_checkpoints SET checkpoint_version = 3 WHERE namespace_id = '{}';",
                        super::sqlite_string_literal(&original.binding.namespace_id)
                    ),
                    format!(
                        "UPDATE vce_active_checkpoints SET checkpoint_version = 4 WHERE namespace_id = '{}';",
                        super::sqlite_string_literal(&original.binding.namespace_id)
                    ),
                ),
                (
                    format!(
                        "UPDATE vce_rebuild_candidates SET state = 'aborted' WHERE namespace_id = '{}';",
                        super::sqlite_string_literal(&candidate_config.binding.namespace_id)
                    ),
                    format!(
                        "UPDATE vce_rebuild_candidates SET state = 'building' WHERE namespace_id = '{}';",
                        super::sqlite_string_literal(&candidate_config.binding.namespace_id)
                    ),
                ),
            ] {
                data_plane
                    .execute(&change, Vec::new(), "inject candidate admission race")
                    .await
                    .expect("inject candidate race");
                assert!(
                    data_plane
                        .execute(
                            &candidate_script,
                            Vec::new(),
                            "reject stale candidate transaction"
                        )
                        .await
                        .is_err()
                );
                assert!(
                    candidate_writer
                        .current_checkpoint_row()
                        .await
                        .expect("candidate checkpoint after rejection")
                        .is_none()
                );
                data_plane
                    .execute(&restore, Vec::new(), "restore exact candidate fixture")
                    .await
                    .expect("restore candidate fixture");
            }
            drop(candidate_lease);
            data_plane.execute(
                "CREATE TRIGGER vce_fixture_reject_candidate_publish BEFORE UPDATE OF active ON vce_project_bindings WHEN NEW.active = 1 BEGIN SELECT RAISE(ROLLBACK, 'fixture rejects candidate pointer'); END;",
                Vec::new(), "inject candidate switch failure",
            ).await.expect("install candidate switch fault");
            assert!(
                candidate_writer
                    .commit_pinned(candidate_commit.clone(), "candidate-run")
                    .await
                    .is_err()
            );
            assert_eq!(
                data_plane
                    .load_active_project_config(&original.project_id)
                    .await
                    .expect("original remains after failed switch"),
                Some(original.clone())
            );
            // rolled_back proves even the preceding lease and receipt insertions were rolled back by switch failure.
            // rolled_back 证明切换失败时连此前插入的租约及回执也一起回滚。
            let rolled_back: Vec<super::CountRow> = data_plane.query_rows(
                "SELECT (SELECT COUNT(*) FROM vce_read_checkpoints WHERE namespace_id = ?) + (SELECT COUNT(*) FROM vce_read_leases WHERE namespace_id = ?) + (SELECT COUNT(*) FROM vce_commit_receipts WHERE namespace_id = ?) AS count;",
                vec![super::sqlite_text(&candidate_config.binding.namespace_id); 3],
            ).await.expect("candidate rollback effects");
            assert_eq!(rolled_back[0].count, 0);
            assert_eq!(
                candidate_writer
                    .staged_blob_names("candidate-refresh", 1)
                    .await
                    .expect("retained candidate staging"),
                BTreeSet::from([blob_name.to_owned()])
            );
            data_plane
                .execute(
                    "DROP TRIGGER vce_fixture_reject_candidate_publish;",
                    Vec::new(),
                    "remove candidate switch fault",
                )
                .await
                .expect("remove candidate fault");
            // published_candidate becomes queryable only after the new pointer and its lease commit together.
            // published_candidate 仅在新指针与租约一同提交后才可查询。
            let (_, published_candidate) = candidate_writer
                .commit_pinned(candidate_commit.clone(), "candidate-run")
                .await
                .expect("publish candidate atomically");
            assert_eq!(
                data_plane
                    .load_active_project_config(&original.project_id)
                    .await
                    .expect("new active candidate"),
                Some(candidate_config.clone())
            );
            assert_eq!(
                published_candidate
                    .embedding_provenance()
                    .await
                    .expect("candidate frozen provenance")
                    .original,
                candidate_config
            );
            assert_eq!(
                published_candidate
                    .search_dense(DenseSearchRequest {
                        request_id: String::from("search-published-candidate"),
                        query_index: 0,
                        query_vector: EmbeddingVector {
                            input_id: String::from("new-query"),
                            values: vec![1.0, 0.0, 0.0, 0.0]
                        },
                        allowed_blobs: BTreeSet::from([blob_name.to_owned()]),
                        limit: 4,
                    })
                    .await
                    .expect("new four-dimensional query")
                    .hits
                    .len(),
                1
            );
            assert!(
                approved_writer
                    .find_missing(vulcan_context_engine::FindMissingRequest {
                        request_id: String::from("old-writer-after-switch"),
                        blob_names: BTreeSet::new(),
                    })
                    .await
                    .is_err(),
                "old writer must lose active authority"
            );
            assert!(
                candidate_writer
                    .stage(IndexStageBatch {
                        operation_id: String::from("reuse-published-candidate"),
                        refresh_id: String::from("reuse-candidate"),
                        generation: 2,
                        blobs: Vec::new(),
                    })
                    .await
                    .is_err(),
                "candidate write grant ends on publication"
            );
            // replay proves an acknowledged candidate publication is not interpreted as another rebuild.
            // replay 证明已确认候选发布不会被解释为另一次重建。
            let (_, replay) = candidate_writer
                .commit_pinned(candidate_commit.clone(), "candidate-run")
                .await
                .expect("replay candidate publication");
            drop(replay);
            assert!(
                candidate_writer
                    .cancel_rebuild_candidate(
                        candidate_writer
                            .rebuild_candidate
                            .as_ref()
                            .expect("published candidate authority")
                    )
                    .await
                    .is_err(),
                "published candidate cannot be cancelled"
            );
            assert!(
                candidate_writer
                    .cleanup_cancelled_rebuild_candidate(
                        candidate_writer
                            .rebuild_candidate
                            .as_ref()
                            .expect("published candidate identity")
                    )
                    .await
                    .is_err(),
                "published candidate tables cannot be cleaned as cancelled work"
            );
            verify_candidate_cancellation(
                current_factory.clone(),
                data_plane.clone(),
                candidate_config.clone(),
                candidate_commit.clone(),
            )
            .await
            .expect("candidate cancellation verification task");
            drop(published_candidate);
            drop(candidate_writer);
            // A retained three-dimensional read must still work after publishing the four-dimensional replacement.
            // 发布四维替换库后，已保留的三维读取必须仍然可用。
            assert_eq!(
                retained_publications[1]
                    .search_dense(DenseSearchRequest {
                        request_id: String::from("old-query-after-candidate-publication"),
                        query_index: 0,
                        query_vector: EmbeddingVector {
                            input_id: String::from("old-query"),
                            values: vec![1.0, 0.0, 0.0]
                        },
                        allowed_blobs: BTreeSet::from([blob_name.to_owned()]),
                        limit: 4,
                    })
                    .await
                    .expect("old retained vectors survive candidate publication")
                    .hits
                    .len(),
                1
            );
            drop(candidate);
        })
    }

    /// Verify exact candidate reopening, durable cancellation and post-drain cleanup on real storage.
    /// 在真实存储上验证精确候选重开、持久取消及排空后清理。
    /// Owned fixture handles/config/commit retain the same store. Returns a task that the caller immediately joins.
    /// 已拥有的夹具句柄／配置／提交保留同一存储。返回由调用方立即等待完成的任务。
    fn verify_candidate_cancellation(
        current_factory: super::VceDataPlaneFactory,
        data_plane: super::VceDataPlaneHandle,
        candidate_config: super::VceProjectIndexConfig,
        candidate_commit: IndexCommit,
    ) -> tokio::task::JoinHandle<()> {
        // A separate task unwinds predecessor poll frames without adding a thread or concurrent indexing work.
        // 独立任务使前驱轮询帧退出，不增加线程，也不并发执行索引工作。
        tokio::spawn(async move {
            // cancelled_config allocates a separate attempt so a durable cancellation is never revived in the fixture.
            // cancelled_config 分配独立尝试，使夹具绝不复活持久取消状态。
            let mut cancelled_config = candidate_config.clone();
            cancelled_config.binding.namespace_id.push_str(":cancelled");
            cancelled_config.vector_space_id.push_str(":cancelled");
            cancelled_config.binding.embedding_space.vector_space_id =
                cancelled_config.vector_space_id.clone();
            cancelled_config
                .binding
                .checkpoint_chain_id
                .push_str(":cancelled");
            cancelled_config.physical_fingerprint.push_str(":cancelled");
            cancelled_config
                .binding
                .embedding_space
                .physical_fingerprint = Some(cancelled_config.physical_fingerprint.clone());
            // cancelled_base belongs to the newly published four-dimensional predecessor.
            // cancelled_base 属于刚发布的四维前驱。
            let cancelled_base = super::VceRebuildCandidateBase {
                namespace_id: candidate_config.binding.namespace_id.clone(),
                checkpoint_version: 1,
                job_id: String::from("cancelled-candidate-job"),
            };
            // Record the second independent target before its physical allocation, without reviving the first attempt.
            // 在物理分配前记录第二个独立目标，不复活首次尝试。
            super::VceProjectJobStore::new(data_plane.clone(), cancelled_config.project_id.clone())
                .register(&super::jobs::replacement_test_receipt(
                    cancelled_config.clone(),
                    cancelled_base.clone(),
                ))
                .await
                .expect("persist cancellable candidate target");
            drop(
                current_factory
                    .prepare_rebuild_candidate(cancelled_config.clone(), cancelled_base.clone())
                    .await
                    .expect("allocate cancellable candidate"),
            );
            // reopened restores only physical ownership; it does not admit writes or create any missing tables.
            // reopened 仅恢复物理归属，不准入写入，也不创建任何缺失表。
            let reopened = current_factory
                .reopen_rebuild_candidate(&cancelled_config, &cancelled_base)
                .await
                .expect("reopen exact candidate");
            assert!(
                reopened
                    .find_missing(vulcan_context_engine::FindMissingRequest {
                        request_id: String::from("unadmitted-recovery"),
                        blob_names: BTreeSet::new(),
                    })
                    .await
                    .is_err()
            );
            // wrong_job demonstrates that neither reopening nor cancellation accepts another job's identity.
            // wrong_job 证明重新打开和取消都不接受其他作业身份。
            let mut wrong_job = cancelled_base.clone();
            wrong_job.job_id.push_str("-wrong");
            assert!(
                current_factory
                    .reopen_rebuild_candidate(&cancelled_config, &wrong_job)
                    .await
                    .is_err()
            );
            assert!(reopened.cancel_rebuild_candidate(&wrong_job).await.is_err());
            // wrong_config must not silently replace the candidate's allocated revision during recovery.
            // wrong_config 在恢复时不得静默替换候选分配修订。
            let mut wrong_config = cancelled_config.clone();
            wrong_config.config_revision += 1;
            assert!(
                current_factory
                    .reopen_rebuild_candidate(&wrong_config, &cancelled_base)
                    .await
                    .is_err()
            );
            // recovered_writer receives the separate explicit grant after exact read-only reopening.
            // recovered_writer 在精确只读重新打开之后另行取得明确授权。
            let recovered_writer = reopened
                .admit_rebuild_candidate_writer(cancelled_base.clone())
                .await
                .expect("readmit exact pending candidate");
            recovered_writer
                .stage(IndexStageBatch {
                    operation_id: String::from("stage-cancellable-candidate"),
                    refresh_id: String::from("cancellable-refresh"),
                    generation: 1,
                    blobs: Vec::new(),
                })
                .await
                .expect("stage cancellable empty candidate");
            // cancelled_commit is otherwise a valid first empty publication for the new candidate chain.
            // cancelled_commit 在其他方面是新候选链合法的首次空发布。
            let mut cancelled_commit = candidate_commit.clone();
            cancelled_commit.request_id = String::from("commit-cancelled-candidate");
            cancelled_commit.refresh_id = String::from("cancellable-refresh");
            cancelled_commit.staged_blob_names.clear();
            cancelled_commit.checkpoint.chain_id =
                cancelled_config.binding.checkpoint_chain_id.clone();
            cancelled_commit.checkpoint.members.clear();
            cancelled_commit.state_snapshot.identity = VceStateIdentity::new(
                cancelled_config.binding.namespace_id.clone(),
                cancelled_config.binding.checkpoint_chain_id.clone(),
                cancelled_config.binding.schema_version,
                cancelled_config.binding.embedding_space.clone(),
            );
            cancelled_commit.state_snapshot.workspace.checkpoint =
                Some(cancelled_commit.checkpoint.clone());
            cancelled_commit.state_snapshot.workspace.records.clear();
            // summary mirrors the explicitly empty candidate, not the populated predecessor.
            // summary 对应明确为空的候选，而不是有内容的前驱。
            let summary = cancelled_commit
                .state_snapshot
                .last_refresh
                .as_mut()
                .expect("cancelled candidate summary");
            summary.request_id = cancelled_commit.refresh_id.clone();
            summary.added_blob_count = 0;
            summary.indexed_blob_count = 0;
            recovered_writer
                .validate_commit(&cancelled_commit)
                .expect("valid candidate before cancellation");
            // lease and script are prepared before revocation so the SQL guard itself must enforce cancellation.
            // lease 与脚本在撤销前准备，因此必须由 SQL 守卫自身执行取消保护。
            let lease = recovered_writer
                .prepare_publication_lease(&cancelled_commit, "cancelled-run")
                .await
                .expect("prepare cancellable publication");
            let script = recovered_writer
                .build_commit_script(
                    &cancelled_commit,
                    &super::commit_request_fingerprint(&cancelled_commit)
                        .expect("cancelled candidate fingerprint"),
                    &[],
                    Some(&lease),
                )
                .expect("build cancellable publication");
            // Retire the predecessor after admission: cancellation must remain possible when write admission is stale.
            // 在准入后退役前驱：写入准入已过期时仍必须可以取消。
            data_plane
                .execute(
                    "UPDATE vce_project_bindings SET active = 0 WHERE namespace_id = ?;",
                    vec![super::sqlite_text(&cancelled_base.namespace_id)],
                    "retire fixture predecessor temporarily",
                )
                .await
                .expect("retire cancellation predecessor");
            reopened
                .cancel_rebuild_candidate(&cancelled_base)
                .await
                .expect("revoke exact candidate");
            data_plane
                .execute(
                    "UPDATE vce_project_bindings SET active = 1 WHERE namespace_id = ?;",
                    vec![super::sqlite_text(&cancelled_base.namespace_id)],
                    "restore fixture predecessor",
                )
                .await
                .expect("restore cancellation predecessor");
            assert!(
                data_plane
                    .execute(&script, Vec::new(), "reject revoked candidate publication")
                    .await
                    .is_err()
            );
            assert!(
                recovered_writer
                    .commit_pinned(cancelled_commit, "cancelled-run")
                    .await
                    .is_err()
            );
            assert!(
                data_plane
                    .open_project_index(cancelled_config.clone())
                    .await
                    .is_err(),
                "ordinary open cannot revive cancelled candidate"
            );
            assert!(
                reopened
                    .admit_rebuild_candidate_writer(cancelled_base.clone())
                    .await
                    .is_err()
            );
            assert_eq!(
                recovered_writer
                    .generation_state("cancellable-refresh", 1, VcePortOperation::Stage)
                    .await
                    .expect("cancel retains in-flight staging")
                    .as_deref(),
                Some("staging")
            );
            // cancelled_reopened permits exact cancellation replay after lost handles, but never write readmission.
            // cancelled_reopened 允许丢失句柄后精确重放取消，但绝不重新准入写入。
            let cancelled_reopened = current_factory
                .reopen_rebuild_candidate(&cancelled_config, &cancelled_base)
                .await
                .expect("reopen revoked candidate");
            cancelled_reopened
                .cancel_rebuild_candidate(&cancelled_base)
                .await
                .expect("replay durable cancellation");
            assert!(
                cancelled_reopened
                    .admit_rebuild_candidate_writer(cancelled_base.clone())
                    .await
                    .is_err()
            );
            assert_eq!(
                data_plane
                    .load_active_project_config(&candidate_config.project_id)
                    .await
                    .expect("cancellation preserves published index"),
                Some(candidate_config.clone())
            );
            // The fixture has no running stage task, so its exact refresh may now drain through existing cleanup.
            // 夹具不存在运行中的暂存任务，因此现在可通过既有清理排空精确刷新。
            recovered_writer
                .abort_refresh_staging("cancellable-refresh")
                .await
                .expect("drain owned cancelled staging");
            assert!(
                !recovered_writer
                    .has_unresolved_staging()
                    .await
                    .expect("cancelled staging drained")
            );
            drop(lease);
            drop(cancelled_reopened);
            drop(recovered_writer);
            drop(reopened);
        })
    }

    /// Exercises engine publication, retrieval, mutations, restart, and project isolation on real storage.
    /// 在真实存储上验证引擎发布、检索、变更、重启与项目隔离。
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires the bundled output/bin/vldb-controller.exe"]
    async fn real_controller_engine_end_to_end_preserves_checkpoint_isolation() {
        let temporary = tempdir().expect("create VCE engine integration root");
        let data_root = temporary.path().join("data");
        let first_root = temporary.path().join("project-one");
        let second_root = temporary.path().join("project-two");
        fs::create_dir_all(&first_root).expect("create first VCE engine project root");
        fs::create_dir_all(&second_root).expect("create second VCE engine project root");
        let executable = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("output")
            .join("bin")
            .join("vldb-controller.exe");
        assert!(executable.is_file(), "bundled controller must exist");
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("reserve VCE engine test port");
        let port = listener
            .local_addr()
            .expect("read VCE engine test port")
            .port();
        drop(listener);
        let endpoint = format!("http://127.0.0.1:{port}");
        let client = ControllerClient::new(
            ControllerClientConfig {
                endpoint: endpoint.clone(),
                auto_spawn: true,
                spawn_executable: Some(executable.display().to_string()),
                spawn_process_mode: ControllerProcessMode::Managed,
                minimum_uptime_secs: 0,
                idle_timeout_secs: 1,
                default_lease_ttl_secs: 30,
                connect_timeout_secs: 30,
                startup_timeout_secs: 15,
                startup_retry_interval_ms: 100,
                lease_renew_interval_secs: 10,
            },
            ClientRegistration {
                client_name: format!("vce-engine-e2e-{}", Uuid::new_v4()),
                host_kind: String::from("test"),
                process_id: std::process::id(),
                process_name: String::from("vulcan-workspace-store-test"),
                lease_ttl_secs: Some(30),
            },
        );
        client
            .connect()
            .await
            .expect("connect VCE engine controller");
        let metadata_path = data_root.join("controller-placeholder.sqlite3");
        let controller = ControllerRuntimeHandle::new(
            client.clone(),
            ControllerRuntimeMetadata {
                endpoint,
                executable: executable.display().to_string(),
                data_root: data_root.clone(),
                sqlite_path: metadata_path.clone(),
                sqlite_binding_id: format!("vce-engine-app-{}", Uuid::new_v4()),
                system_statistics_sqlite_path: metadata_path,
                system_statistics_sqlite_binding_id: format!(
                    "vce-engine-statistics-{}",
                    Uuid::new_v4()
                ),
            },
        );
        fs::create_dir_all(&data_root).expect("create test application root");
        let factory = bootstrap_vce_data_plane(&controller)
            .await
            .expect("bootstrap project-isolated VCE engine factory");
        let first_index = factory
            .create_project_index(test_project_config("project-1", &first_root, 1))
            .await
            .expect("open first VCE engine project index");
        // Metadata inspection uses this project's actual physical binding, not a shared fixture.
        // 元数据检查使用此项目的实际物理绑定，而不是共享夹具。
        let data_plane = first_index.data_plane.clone();
        let first_ai = deterministic_ai_for(&first_index);
        // The sole new layout is initialized together; passive reads never fabricate a checkpoint.
        // 唯一新布局统一初始化；被动读取绝不虚构检查点。
        let before_provenance = data_plane
            .embedding_provenance("project-1")
            .await
            .expect("read pre-initialization provenance")
            .expect("existing original binding");
        assert_eq!(before_provenance.checkpoint_version, None);
        assert_eq!(
            before_provenance.source_fingerprints,
            BTreeSet::from([first_index.physical_fingerprint.clone()])
        );
        // Origin tables belong to base schema, not a lazily installed compatibility extension.
        // 来源表属于基础结构，不是惰性安装的兼容扩展。
        let base_tables: Vec<super::CountRow> = data_plane.query_rows(
            "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name IN ('vce_embedding_stage_origins', 'vce_embedding_generation_origins', 'vce_read_leases');",
            Vec::new(),
        ).await.expect("inspect sole schema");
        assert_eq!(base_tables[0].count, 3);
        let first_config = VceConfig::new(
            first_index.binding().checkpoint_chain_id.clone(),
            first_index.binding().embedding_space.clone(),
        )
        .expect("build first VCE engine configuration");
        let mut first_engine =
            VceEngine::new(first_config.clone()).expect("create first VCE engine");
        let initial_lib = "pub fn semantic_entry() -> usize { 1 }\n";
        let initial_other = "pub fn secondary_entry() -> usize { 2 }\n";
        let initial_result = first_engine
            .refresh(
                RefreshRequest::full(
                    "e2e-initial",
                    RefreshReason::Initial,
                    vec![
                        FileObservation::present(
                            "src/lib.rs",
                            initial_lib,
                            Some(1),
                            FileSource::Filesystem,
                        ),
                        FileObservation::present(
                            "src/other.rs",
                            initial_other,
                            Some(1),
                            FileSource::Filesystem,
                        ),
                    ],
                    Some(String::from("source-1")),
                ),
                &first_index,
                &first_ai,
                &NoReadSource,
            )
            .await
            .expect("publish initial VCE engine checkpoint");
        assert_eq!(initial_result.indexed_blobs.len(), 2);
        // The first commit records actual sources in the already-initialized versioned layout.
        // 首次提交在已初始化的版本化布局中记录实际来源。
        let initial_provenance = data_plane
            .embedding_provenance("project-1")
            .await
            .expect("read committed provenance")
            .expect("committed binding");
        assert_eq!(
            initial_provenance.checkpoint_version,
            first_engine.status().checkpoint_version
        );
        assert_eq!(initial_provenance.original, before_provenance.original);
        assert_eq!(
            initial_provenance.source_fingerprints,
            before_provenance.source_fingerprints
        );
        // Blob ownership follows committed rows, not every historical source or pending stage.
        // 块对象归属跟随已提交记录，而不是每个历史来源或待提交暂存。
        let origin_rows: Vec<super::CountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_blob_embedding_origins WHERE namespace_id = ?;",
                vec![super::sqlite_text(&first_index.binding.namespace_id)],
            )
            .await
            .expect("count initial vector origins");
        assert_eq!(origin_rows[0].count, 2);
        // Keep the original publication alive across several later commits and garbage collection.
        // 在后续多次提交和垃圾回收期间保留原始发布对象。
        let initial_read = first_index
            .pin_checkpoint(
                first_engine
                    .workspace_snapshot()
                    .checkpoint
                    .expect("initial checkpoint"),
                "engine-fixture-run",
            )
            .await
            .expect("pin initial checkpoint");
        let initial_retrieval = first_engine
            .retrieve(
                RetrieveRequest::current_checkpoint("e2e-query-1", "find semantic entry"),
                &initial_read,
                &first_ai,
            )
            .await
            .expect("retrieve initial VCE context");
        assert!(!initial_retrieval.hits.is_empty());

        // Rebind only execution after explicit host approval; stored tables and snapshot identity stay original.
        // 明确宿主批准后仅重新绑定执行；存储表和快照身份保持原始值。
        let mut target_space = first_ai.space.clone();
        target_space.physical_fingerprint = Some(String::from("provider-b:same-model:3"));
        let grant = vulcan_context_engine::VceEmbeddingExecutionContract::new(
            first_index.binding().clone(),
            target_space.clone(),
            "fixture-direct-project-approval",
        )
        .expect("approve exact original-to-target space");
        let original_writer = first_index.clone();
        let first_index = first_index
            .with_embedding_execution(grant.clone())
            .expect("retain original index with a different actual producer");
        let mut first_config = first_config;
        first_config.embedding_execution = Some(grant);
        first_engine =
            VceEngine::from_state_snapshot(first_config.clone(), first_engine.state_snapshot())
                .expect("restore original checkpoint under approved target execution");
        let mut first_ai = deterministic_ai_for(&first_index);
        first_ai.space = target_space.clone();
        assert!(
            first_index
                .validate_embedding_stage_origin("e2e-initial", initial_result.generation)
                .await
                .is_err(),
            "approved B cannot relabel A's previously staged vectors"
        );
        assert_ne!(
            first_ai.space.physical_fingerprint,
            first_index.binding().embedding_space.physical_fingerprint
        );
        // A newly authorized execution still reads the same immutable original checkpoint.
        // 新授权的执行仍然读取同一个不可变原始检查点。
        let approved_read = first_index
            .pin_checkpoint(
                first_engine
                    .workspace_snapshot()
                    .checkpoint
                    .expect("approved checkpoint"),
                "engine-fixture-run",
            )
            .await
            .expect("pin approved original checkpoint");
        let old_engine =
            VceEngine::from_state_snapshot(first_config.clone(), first_engine.state_snapshot())
                .expect("retain old core publication");
        // Pause after actual vector recall; update will happen before body materialization.
        // 在实际向量召回之后暂停；更新发生在正文物化之前。
        let recalled_old_rows = approved_read
            .search_vectors(
                &approved_read.content_table,
                &[1.0, 0.0, 0.0],
                &old_engine
                    .workspace_snapshot()
                    .checkpoint
                    .expect("old members")
                    .members,
                8,
                VcePortOperation::DenseSearch,
            )
            .await
            .expect("recall original vectors");
        assert!(!recalled_old_rows.is_empty());
        assert!(
            !first_engine
                .retrieve(
                    RetrieveRequest::current_checkpoint("e2e-approved-old-index", "semantic entry"),
                    &approved_read,
                    &first_ai,
                )
                .await
                .expect("query original vectors using approved target")
                .hits
                .is_empty()
        );

        let old_lib_blob = blob_name("src/lib.rs", initial_lib).expect("build old lib Blob");
        let changed_lib = "pub fn semantic_entry() -> usize { 3 }\n";
        let changed_result = first_engine
            .refresh(
                RefreshRequest::paths(
                    "e2e-change",
                    RefreshReason::FileWatcher,
                    vec![FileObservation::present(
                        "src/lib.rs",
                        changed_lib,
                        Some(2),
                        FileSource::Filesystem,
                    )],
                    Vec::new(),
                    Some(String::from("source-2")),
                ),
                &first_index,
                &first_ai,
                &NoReadSource,
            )
            .await
            .expect("publish changed VCE Blob");
        assert_eq!(changed_result.indexed_blobs.len(), 1);
        // Materialize vectors recalled before the commit; body and line ranges remain original.
        // 物化提交前召回的向量；正文和行号仍保持原始值。
        let retained_hits = approved_read
            .materialize_chunk_hits(
                recalled_old_rows,
                &[1.0, 0.0, 0.0],
                super::SearchSource::Dense,
            )
            .await
            .expect("materialize old vectors after new commit");
        assert!(
            retained_hits
                .iter()
                .any(|hit| hit.path == "src/lib.rs" && hit.content == initial_lib)
        );
        assert!(
            retained_hits
                .iter()
                .all(|hit| !hit.content.contains("{ 3 }"))
        );
        let old_result = old_engine
            .retrieve_read_view(
                RetrieveRequest::current_checkpoint("old-query-after-publish", "semantic_entry"),
                &approved_read,
                &first_ai,
            )
            .await
            .expect("all channels query retained publication");
        assert!(
            old_result
                .hits
                .iter()
                .any(|hit| hit.path == "src/lib.rs" && hit.content == initial_lib)
        );
        let changed_read = first_index
            .pin_checkpoint(
                first_engine
                    .workspace_snapshot()
                    .checkpoint
                    .expect("changed checkpoint"),
                "engine-fixture-run",
            )
            .await
            .expect("pin changed checkpoint");
        // Each auxiliary channel must read old membership instead of current management views.
        // 每个辅助通道必须读取旧成员关系，而不是当前管理视图。
        let old_members = old_engine
            .workspace_snapshot()
            .checkpoint
            .expect("old channel scope")
            .members;
        let old_symbols = approved_read
            .search_exact(super::ExactSearchRequest {
                request_id: String::from("old-symbols-after-publish"),
                identifiers: vec![String::from("semantic_entry")],
                allowed_blobs: old_members.clone(),
                limit: 8,
            })
            .await
            .expect("read old exact symbols");
        assert!(!old_symbols.hits.is_empty());
        assert!(
            old_symbols
                .hits
                .iter()
                .all(|hit| hit.content.contains("{ 1 }") && hit.range.start_line == 1)
        );
        let old_paths = approved_read
            .search_path(super::PathSearchRequest {
                request_id: String::from("old-paths-after-publish"),
                query_index: 0,
                query_vector: EmbeddingVector {
                    input_id: String::from("old-path-vector"),
                    values: vec![1.0, 0.0, 0.0],
                },
                allowed_blobs: old_members,
                limit: 8,
            })
            .await
            .expect("read old path documents");
        assert_eq!(old_paths.hits.len(), 2);
        assert!(old_paths.hits.iter().any(|hit| hit.path == "src/other.rs"));
        assert!(approved_read.require_write_access().is_err());
        assert!(
            first_index
                .integrity_issues()
                .await
                .expect("inspect healthy retained index")
                .is_empty(),
            "a leased predecessor awaiting reclamation is not corrupt staging"
        );
        assert!(
            first_index
                .pin_checkpoint(
                    old_engine
                        .workspace_snapshot()
                        .checkpoint
                        .expect("stale checkpoint"),
                    "engine-fixture-run"
                )
                .await
                .is_err(),
            "new admissions cannot resurrect a retired checkpoint"
        );
        // Untouched A vectors and newly generated B vectors coexist without laundering their provenance.
        // 未改动的 A 向量与新生成的 B 向量共存，不混淆其来源。
        let mixed_provenance = first_index
            .embedding_provenance()
            .await
            .expect("read mixed actual sources");
        assert!(
            original_writer
                .validate_embedding_stage_origin("e2e-change", changed_result.generation)
                .await
                .is_err(),
            "old A handle cannot publish B's staged vectors"
        );
        assert_eq!(mixed_provenance.original, initial_provenance.original);
        assert_eq!(
            mixed_provenance.source_fingerprints,
            BTreeSet::from([
                initial_provenance.original.physical_fingerprint.clone(),
                target_space
                    .physical_fingerprint
                    .clone()
                    .expect("explicit B identity"),
            ])
        );
        // Corrupt only isolated fixture metadata, prove fail-closed inspection, and restore the exact row owner.
        // 仅破坏隔离夹具元数据，验证读取失败关闭，再恢复精确记录所有者。
        data_plane.execute(
            "UPDATE vce_embedding_generation_origins SET physical_fingerprint = 'fixture-invalid-producer' WHERE namespace_id = ? AND refresh_id = 'e2e-change';",
            vec![super::sqlite_text(&first_index.binding.namespace_id)], "inject fixture origin mismatch",
        ).await.expect("inject isolated origin mismatch");
        assert!(first_index.embedding_provenance().await.is_err());
        data_plane.execute(
            "UPDATE vce_embedding_generation_origins SET physical_fingerprint = 'provider-b:same-model:3' WHERE namespace_id = ? AND refresh_id = 'e2e-change';",
            vec![super::sqlite_text(&first_index.binding.namespace_id)], "restore exact fixture origin owner",
        ).await.expect("restore isolated origin metadata");
        assert_eq!(
            first_index
                .embedding_provenance()
                .await
                .expect("recheck exact restoration"),
            mixed_provenance
        );
        assert_eq!(
            changed_result.deleted_blobs,
            BTreeSet::from([old_lib_blob.clone()])
        );
        assert!(
            changed_read
                .search_dense(DenseSearchRequest {
                    request_id: String::from("e2e-search-old-lib"),
                    query_index: 0,
                    query_vector: EmbeddingVector {
                        input_id: String::from("e2e-old-lib-vector"),
                        values: vec![1.0, 0.0, 0.0],
                    },
                    allowed_blobs: BTreeSet::from([old_lib_blob]),
                    limit: 4,
                })
                .await
                .is_err()
        );

        let old_other_blob =
            blob_name("src/other.rs", initial_other).expect("build old other Blob");
        let renamed_blob = blob_name("src/renamed.rs", initial_other).expect("build renamed Blob");
        let rename_result = first_engine
            .refresh(
                RefreshRequest::paths(
                    "e2e-rename",
                    RefreshReason::FileWatcher,
                    vec![FileObservation::present(
                        "src/renamed.rs",
                        initial_other,
                        Some(3),
                        FileSource::Filesystem,
                    )],
                    vec![String::from("src/other.rs")],
                    Some(String::from("source-3")),
                ),
                &first_index,
                &first_ai,
                &NoReadSource,
            )
            .await
            .expect("publish renamed VCE Blob");
        assert_eq!(rename_result.indexed_blobs, vec![renamed_blob.clone()]);
        assert_eq!(
            rename_result.deleted_blobs,
            BTreeSet::from([old_other_blob])
        );
        let delete_result = first_engine
            .refresh(
                RefreshRequest::paths(
                    "e2e-delete",
                    RefreshReason::FileWatcher,
                    Vec::new(),
                    vec![String::from("src/renamed.rs")],
                    Some(String::from("source-4")),
                ),
                &first_index,
                &first_ai,
                &NoReadSource,
            )
            .await
            .expect("publish deleted VCE path");
        assert!(delete_result.indexed_blobs.is_empty());
        assert_eq!(delete_result.deleted_blobs, BTreeSet::from([renamed_blob]));
        // Deleted and superseded blobs lose current origin membership, while generation audit remains.
        // 删除及替换的块对象移出当前来源成员集合，但代次审计记录保留。
        let remaining_origins: Vec<super::CountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_blob_embedding_origins WHERE namespace_id = ?;",
                vec![super::sqlite_text(&first_index.binding.namespace_id)],
            )
            .await
            .expect("count remaining vector origins");
        assert_eq!(remaining_origins[0].count, 1);
        // Deletion-only publication has no new embedding source and must not claim a model call.
        // 仅删除发布没有新向量来源，不得声称发生了模型调用。
        let written_generations: Vec<super::CountRow> = data_plane.query_rows(
            "SELECT COUNT(*) AS count FROM vce_embedding_generation_origins WHERE namespace_id = ?;",
            vec![super::sqlite_text(&first_index.binding.namespace_id)],
        ).await.expect("count actual embedding generations");
        assert_eq!(written_generations[0].count, 3);

        // Management content must retain the old checkpoint after replacement, rename and deletion.
        // 替换、重命名与删除之后，管理内容仍须保留旧检查点。
        let writer_guard = first_index.mutation_lock.lock().await;
        // Holding the writer lock models an in-flight write without delaying published management reads.
        // 持有写锁模拟在途写入，不能延迟已发布管理读取。
        let old_management = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            approved_read.management_snapshot(super::VceManagementRequest {
                request_id: String::from("old-management-after-delete"),
            }),
        )
        .await
        .expect("pinned management cannot wait for the writer lock")
        .expect("read pinned old management snapshot");
        drop(writer_guard);
        assert_eq!(old_management.checkpoint_version, Some(1));
        assert_eq!(old_management.counts.files, 2);
        assert_eq!(old_management.integrity.checkpoint_blob_count, 2);
        assert_eq!(old_management.integrity.missing_blob_count, 0);
        // Aggregate statistics still include the separate ready path channel.
        // 聚合统计仍包含独立的就绪路径通道。
        assert_eq!(
            old_management.counts.vectors,
            old_management.counts.chunks + old_management.counts.path_documents
        );
        // The unpinned management handle reports the latest committed inventory.
        // 未固定管理句柄报告最新已提交清单。
        let current_management = first_index
            .management_snapshot(super::VceManagementRequest {
                request_id: String::from("current-management-after-delete"),
            })
            .await
            .expect("read current management snapshot");
        assert_eq!(current_management.counts.files, 1);
        assert_ne!(
            current_management.checkpoint_version,
            old_management.checkpoint_version
        );
        // File rows and ordered previews must share the lease, not consult active SQL views.
        // 文件行与有序预览必须共享租约，不能读取活动 SQL 视图。
        let old_files = approved_read
            .file_page(vulcan_context_engine::VceFileQuery {
                path_prefix: None,
                status: None,
                auxiliary_failures_only: false,
                page: super::VcePageRequest::new(None, 10).expect("bounded fixture page"),
            })
            .await
            .expect("read old file inventory");
        assert_eq!(old_files.total, Some(2));
        assert!(
            old_files
                .items
                .iter()
                .any(|file| file.path == "src/other.rs")
        );
        assert!(
            !old_files
                .items
                .iter()
                .any(|file| file.path == "src/renamed.rs")
        );
        // Prefixes are literal and case-sensitive, including SQL wildcard characters.
        // 前缀按字面且区分大小写匹配，包括 SQL 通配字符。
        for (prefix, expected_count) in [
            ("src", 2),
            ("src/lib.rs", 1),
            ("s_c", 0),
            ("s%c", 0),
            ("SRC", 0),
        ] {
            // query uses the same retained publication while the active index has already changed.
            // query 使用同一个保留发布，此时活动索引已经变化。
            let query = vulcan_context_engine::VceFileQuery {
                path_prefix: Some(prefix.to_owned()),
                status: None,
                auxiliary_failures_only: false,
                page: super::VcePageRequest::new(None, 10).expect("bounded prefix page"),
            };
            // page must satisfy the core's exact filter contract rather than SQLite LIKE semantics.
            // page 必须满足核心精确过滤契约，而不是 SQLite LIKE 语义。
            let page = approved_read
                .file_page(query.clone())
                .await
                .expect("read literal prefix");
            assert_eq!(page.items.len(), expected_count, "literal prefix {prefix}");
            vulcan_context_engine::validate_management_response(
                &vulcan_context_engine::VceManagementQuery::Files(query),
                &vulcan_context_engine::VceManagementQueryResult::Files(page),
            )
            .expect("host prefix result matches core contract");
        }
        // The original lib identity is no longer active but its leased chunks are still available.
        // 原始 lib 身份已不再活动，但其租约代码块仍可用。
        let old_detail = approved_read
            .blob_detail(&blob_name("src/lib.rs", initial_lib).expect("original lib identity"))
            .await
            .expect("read old blob detail")
            .expect("old blob retained");
        assert!(!old_detail.chunks.is_empty());
        assert!(
            old_detail
                .chunks
                .iter()
                .any(|chunk| chunk.preview.as_deref() == Some(initial_lib))
        );
        assert!(
            approved_read
                .integrity_issues()
                .await
                .expect("old integrity")
                .is_empty()
        );

        let persisted = data_plane
            .load_state_snapshot(&first_index.binding().namespace_id)
            .await
            .expect("load VCE restart state")
            .expect("VCE restart state exists");
        let mut restored_engine = VceEngine::from_state_snapshot(first_config, persisted)
            .expect("restore VCE engine from checkpoint state");
        let restored_read = first_index
            .pin_checkpoint(
                restored_engine
                    .workspace_snapshot()
                    .checkpoint
                    .expect("restored checkpoint"),
                "engine-fixture-run",
            )
            .await
            .expect("pin restored publication");
        let restored_retrieval = restored_engine
            .retrieve(
                RetrieveRequest::current_checkpoint("e2e-query-restored", "semantic entry"),
                &restored_read,
                &first_ai,
            )
            .await
            .expect("retrieve VCE context after restart");
        assert!(!restored_retrieval.hits.is_empty());

        let second_index = factory
            .create_project_index(test_project_config("project-2", &second_root, 1))
            .await
            .expect("open second VCE engine project index");
        let second_ai = deterministic_ai_for(&second_index);
        let mut second_engine = VceEngine::new(
            VceConfig::new(
                second_index.binding().checkpoint_chain_id.clone(),
                second_index.binding().embedding_space.clone(),
            )
            .expect("build second VCE engine configuration"),
        )
        .expect("create second VCE engine");
        let second_content = "pub fn project_two_only() -> usize { 20 }\n";
        let second_blob = blob_name("src/two.rs", second_content).expect("build project-two Blob");
        second_engine
            .refresh(
                RefreshRequest::full(
                    "e2e-project-two",
                    RefreshReason::Initial,
                    vec![FileObservation::present(
                        "src/two.rs",
                        second_content,
                        Some(1),
                        FileSource::Filesystem,
                    )],
                    Some(String::from("source-project-two")),
                ),
                &second_index,
                &second_ai,
                &NoReadSource,
            )
            .await
            .expect("publish second project checkpoint");
        let current_first_blob =
            blob_name("src/lib.rs", changed_lib).expect("build current first-project Blob");
        let second_read = second_index
            .pin_checkpoint(
                second_engine
                    .workspace_snapshot()
                    .checkpoint
                    .expect("second checkpoint"),
                "engine-fixture-run",
            )
            .await
            .expect("pin second project");
        assert!(
            restored_read
                .search_dense(DenseSearchRequest {
                    request_id: String::from("e2e-cross-search-first"),
                    query_index: 0,
                    query_vector: EmbeddingVector {
                        input_id: String::from("e2e-cross-first-vector"),
                        values: vec![1.0, 0.0, 0.0],
                    },
                    allowed_blobs: BTreeSet::from([second_blob.clone()]),
                    limit: 4,
                })
                .await
                .is_err()
        );
        assert!(
            second_read
                .search_dense(DenseSearchRequest {
                    request_id: String::from("e2e-cross-search-second"),
                    query_index: 0,
                    query_vector: EmbeddingVector {
                        input_id: String::from("e2e-cross-second-vector"),
                        values: vec![1.0, 0.0, 0.0],
                    },
                    allowed_blobs: BTreeSet::from([current_first_blob]),
                    limit: 4,
                })
                .await
                .is_err()
        );
        let second_retrieval = second_engine
            .retrieve(
                RetrieveRequest::current_checkpoint("e2e-query-project-two", "project two only"),
                &second_read,
                &second_ai,
            )
            .await
            .expect("retrieve isolated second-project context");
        assert!(
            second_retrieval
                .hits
                .iter()
                .all(|hit| hit.path == "src/two.rs")
        );
        // Identical content rebuilds create distinct producer rows while older readers remain valid.
        // 相同内容重建生成不同生产记录，同时旧读取方仍然有效。
        let rebuild_result = restored_engine
            .refresh(
                RefreshRequest::full_rebuild(
                    "e2e-same-blob-rebuild",
                    RefreshReason::Manual,
                    vec![FileObservation::present(
                        "src/lib.rs",
                        changed_lib,
                        Some(2),
                        FileSource::Filesystem,
                    )],
                    None,
                ),
                &first_index,
                &first_ai,
                &NoReadSource,
            )
            .await
            .expect("rebuild identical blob into a new attempt");
        assert_eq!(rebuild_result.indexed_blobs.len(), 1);
        let rebuilt_read = first_index
            .pin_checkpoint(
                restored_engine
                    .workspace_snapshot()
                    .checkpoint
                    .expect("rebuilt checkpoint"),
                "engine-fixture-run",
            )
            .await
            .expect("pin rebuilt publication");
        let held_attempts: Vec<super::CountRow> = data_plane.query_rows(
            "SELECT COUNT(*) AS count FROM vce_blob_versions WHERE namespace_id = ? AND blob_name = ?;",
            vec![super::sqlite_text(&first_index.binding.namespace_id), super::sqlite_text(blob_name("src/lib.rs", changed_lib).expect("unchanged blob identity"))],
        ).await.expect("count same-blob immutable attempts");
        assert_eq!(held_attempts[0].count, 2);
        // A valid empty checkpoint is published normally without erasing leased old versions.
        // 合法空检查点正常发布，不抹除已被租约保留的旧版本。
        restored_engine
            .refresh(
                RefreshRequest::full("e2e-empty-project", RefreshReason::Manual, Vec::new(), None),
                &first_index,
                &first_ai,
                &NoReadSource,
            )
            .await
            .expect("publish empty current checkpoint");
        let empty_read = first_index
            .pin_checkpoint(
                restored_engine
                    .workspace_snapshot()
                    .checkpoint
                    .expect("empty checkpoint"),
                "engine-fixture-run",
            )
            .await
            .expect("pin initialized empty publication");
        let empty_result = restored_engine
            .retrieve_read_view(
                RetrieveRequest::current_checkpoint("e2e-empty-query", "semantic entry"),
                &empty_read,
                &first_ai,
            )
            .await
            .expect("query initialized empty checkpoint");
        assert!(empty_result.hits.is_empty());
        assert!(
            !rebuilt_read
                .search_dense(DenseSearchRequest {
                    request_id: String::from("query-rebuilt-after-empty"),
                    query_index: 0,
                    query_vector: EmbeddingVector {
                        input_id: String::from("retained-rebuild-vector"),
                        values: vec![1.0, 0.0, 0.0]
                    },
                    allowed_blobs: BTreeSet::from([
                        blob_name("src/lib.rs", changed_lib).expect("rebuilt blob")
                    ]),
                    limit: 4,
                })
                .await
                .expect("query old rebuild after empty publication")
                .hits
                .is_empty()
        );
        // An old timestamp does not authorize deleting another host's read protection.
        // 较旧的时间戳不代表获准删除另一个宿主的读取保护。
        data_plane.execute(
            "INSERT INTO vce_read_leases(namespace_id, lease_id, owner_run_id, checkpoint_version, created_at_unix_ms) VALUES (?, 'foreign-fixture-lease', 'another-host-run', 1, 0);",
            vec![super::sqlite_text(&first_index.binding.namespace_id)], "insert foreign fixture lease",
        ).await.expect("retain independently owned reader");
        // Last-reference release permits precise retired-vector and metadata cleanup, not active deletion.
        // 最后引用释放后允许精确清理退役向量和元数据，而不是删除活动内容。
        drop(initial_read);
        drop(approved_read);
        drop(changed_read);
        drop(restored_read);
        drop(second_read);
        drop(rebuilt_read);
        drop(empty_read);
        first_index
            .recover_pending_vector_cleanup()
            .await
            .expect("collect unleased old versions");
        second_index
            .recover_pending_vector_cleanup()
            .await
            .expect("flush second-project leases");
        let foreign_versions: Vec<super::CountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_blob_versions WHERE namespace_id = ?;",
                vec![super::sqlite_text(&first_index.binding.namespace_id)],
            )
            .await
            .expect("count foreign-reader protected versions");
        assert_eq!(foreign_versions[0].count, 2);
        // Only explicit release by the known fixture owner permits collection of the remaining checkpoint.
        // 仅已知夹具所有者的明确释放才允许回收剩余检查点。
        data_plane.execute(
            "DELETE FROM vce_read_leases WHERE namespace_id = ? AND lease_id = 'foreign-fixture-lease' AND owner_run_id = 'another-host-run';",
            vec![super::sqlite_text(&first_index.binding.namespace_id)], "release exact foreign fixture lease",
        ).await.expect("release known fixture owner");
        first_index
            .recover_pending_vector_cleanup()
            .await
            .expect("collect released foreign checkpoint");
        let remaining_versions: Vec<super::CountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_blob_versions WHERE namespace_id = ?;",
                vec![super::sqlite_text(&first_index.binding.namespace_id)],
            )
            .await
            .expect("count retained physical versions");
        assert_eq!(remaining_versions[0].count, 0);
        let remaining_leases: Vec<super::CountRow> = data_plane
            .query_rows("SELECT COUNT(*) AS count FROM vce_read_leases;", Vec::new())
            .await
            .expect("count released read leases");
        assert_eq!(remaining_leases[0].count, 0);
        drop(original_writer);
        drop(first_index);
        drop(second_index);
        drop(data_plane);
        factory
            .shutdown()
            .await
            .expect("close project-isolated test factory");
        client
            .shutdown()
            .await
            .expect("shutdown VCE engine controller");
    }

    /// Builds a deterministic AI registration for one project index.
    /// 为一个项目索引构建确定性 AI 注册。
    fn deterministic_ai_for(index: &super::VceProjectIndex) -> DeterministicAi {
        DeterministicAi {
            binding: VceAiBinding::new(
                format!("registration:{}", index.binding().namespace_id),
                "vce-code-retrieval",
                index.binding().embedding_space.vector_space_id.clone(),
            )
            .expect("build deterministic VCE AI binding"),
            space: index.binding().embedding_space.clone(),
        }
    }

    /// Returns the recursive on-disk byte size of one test artifact path.
    /// 返回一个测试产物路径在磁盘上的递归字节大小。
    fn file_or_directory_size(path: &std::path::Path) -> u64 {
        let metadata = fs::metadata(path).expect("read VCE performance artifact metadata");
        if metadata.is_file() {
            return metadata.len();
        }
        fs::read_dir(path)
            .expect("read VCE performance artifact directory")
            .map(|entry| {
                let entry = entry.expect("read VCE performance artifact entry");
                file_or_directory_size(&entry.path())
            })
            .sum()
    }

    /// Builds one exact test configuration from a canonical project root.
    /// 从规范项目根目录构建一个精确测试配置。
    fn test_project_config(
        project_id: &str,
        root: &std::path::Path,
        revision: u64,
    ) -> VceProjectIndexConfig {
        let canonical = fs::canonicalize(root).expect("canonicalize VCE test root");
        let root_path = canonical.display().to_string();
        let root_hash = hex::encode(Sha256::digest(root_path.as_bytes()));
        let chain = format!("project:{project_id}:root:{root_hash}");
        let namespace = format!("vce:{chain}");
        let vector_space_id = format!("code-semantic:{project_id}:{root_hash}");
        let physical_fingerprint = String::from("test-provider:test-model:3");
        let preprocessing_version = String::from("vce-preprocess-v1");
        let chunker_version = String::from("vce-chunker-v1");
        let embedding_space = EmbeddingSpace {
            vector_space_id: vector_space_id.clone(),
            physical_fingerprint: Some(physical_fingerprint.clone()),
            dimension: 3,
            preprocessing_version: preprocessing_version.clone(),
            chunker_version: chunker_version.clone(),
        };
        VceProjectIndexConfig {
            project_id: project_id.to_owned(),
            root_path,
            root_hash,
            binding: VceIndexBinding::new(namespace, chain, 1, embedding_space)
                .expect("build VCE test binding"),
            vector_space_id,
            physical_fingerprint,
            preprocessing_version,
            chunker_version,
            dimension: 3,
            config_revision: revision,
        }
    }
}

impl fmt::Debug for VceDataPlaneHandle {
    /// Formats non-sensitive controller binding metadata.
    /// 格式化不含敏感信息的控制器绑定元数据。
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
            return Err(String::from("VCE 项目索引配置包含空身份或无效维度"));
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
    /// Shared controller-backed data plane.
    /// 共享的控制器数据面。
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

impl fmt::Debug for VceProjectIndex {
    /// Formats one project index without exposing controller internals.
    /// 格式化一个项目索引且不暴露控制器内部对象。
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
/// `controller` is the existing host registration. Returns the lazy project storage factory.
/// `controller` 是现有宿主注册。返回惰性项目存储工厂。
pub async fn bootstrap_vce_data_plane(
    controller: &ControllerRuntimeHandle,
) -> Result<VceDataPlaneFactory, String> {
    VceDataPlaneFactory::bootstrap(controller).await
}

/// Prepare exact binding ownership before any controller activation may partially fail.
/// 在控制器激活可能部分失败前准备精确绑定归属。
///
/// `client` reuses host registration; `data_root` and `project_owner` are validated ownership.
/// `create` is explicit job authority. Returns unopened project-owned handles.
/// `client` 复用宿主注册；`data_root` 与 `project_owner` 是已校验归属。
/// `create` 是明确作业授权。返回尚未打开的项目自有句柄。
pub(crate) fn prepare_vce_storage_binding(
    client: ControllerClient,
    data_root: PathBuf,
    project_owner: String,
    create: bool,
) -> Result<VceDataPlaneHandle, String> {
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
    Ok(VceDataPlaneHandle {
        project_owner,
        lease: Arc::new(()),
        read_releases: Arc::new(std::sync::Mutex::new(Vec::new())),
        client,
        data_root,
        sqlite_path,
        lancedb_path,
        sqlite_binding_id,
        lancedb_binding_id,
    })
}

impl VceDataPlaneHandle {
    /// Activate bindings only after the factory has retained cleanup ownership.
    /// 仅在工厂保留清理归属后激活绑定。
    ///
    /// `create` permits initialization of a new database. Returns failures without losing handles.
    /// `create` 允许初始化新数据库。返回失败时不丢失句柄。
    pub(crate) async fn activate_bindings(&self, create: bool) -> Result<(), String> {
        // new_database is captured before the controller creates its own internal metadata tables.
        // new_database 在控制器创建自身内部元数据表之前获取。
        let new_database = !self.sqlite_path.exists();
        self.client
            .enable_sqlite(ControllerSqliteEnableRequest {
                space_id: String::from(VCE_SPACE_ID),
                binding_id: self.sqlite_binding_id.clone(),
                db_path: runtime_assets::native_host_path_text_from_path(&self.sqlite_path),
                read_only: false,
                connection_pool_size: 2,
                cache_size_kib: 8_192,
                mmap_size_bytes: 0,
                ..ControllerSqliteEnableRequest::default()
            })
            .await
            .map_err(|error| format!("启用 VCE SQLite 数据库失败：{error}"))?;
        // metadata_count checks ownership before initialization could overwrite a foreign schema.
        // metadata_count 在初始化可能覆盖外部结构之前检查归属。
        let metadata_count: Vec<CountRow> = self.query_rows(
            "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'vce_metadata';",
            Vec::new(),
        ).await?;
        if metadata_count.first().map(|row| row.count) == Some(1) {
            // schema is a persisted contract, never an implicit upgrade request.
            // schema 是持久化契约，绝不是隐式升级请求。
            let schema: Vec<StorageSchemaRow> = self
                .query_rows(
                    "SELECT value FROM vce_metadata WHERE key = 'schema_version';",
                    Vec::new(),
                )
                .await?;
            if schema.len() != 1 || schema[0].value != VCE_DATA_PLANE_SCHEMA_VERSION.to_string() {
                return Err(String::from(
                    "VCE project storage schema is missing or unsupported",
                ));
            }
        } else if create && new_database {
            // The factory admitted this absent file; controller-internal tables are not VCE content.
            // 工厂准入了此缺失文件；控制器内部表不属于 VCE 内容。
        } else {
            return Err(String::from(
                "VCE existing project storage has no schema marker",
            ));
        }
        self.client
            .enable_lancedb(ControllerLanceDbEnableRequest {
                space_id: String::from(VCE_SPACE_ID),
                binding_id: self.lancedb_binding_id.clone(),
                default_db_path: runtime_assets::native_host_path_text_from_path(
                    &self.lancedb_path,
                ),
                db_root: Some(runtime_assets::native_host_path_text_from_path(
                    &self.lancedb_path,
                )),
                read_consistency_interval_ms: Some(250),
                max_upsert_payload: 16 * 1024 * 1024,
                max_search_limit: VCE_VECTOR_SEARCH_LIMIT as usize,
                max_concurrent_requests: 16,
            })
            .await
            .map_err(|error| format!("启用 VCE LanceDB 数据库失败：{error}"))?;
        if create {
            // schema_result confirms actual initialization, not merely a successful transport response.
            // schema_result 确认实际初始化，而不只是传输响应成功。
            let schema_result = self
                .client
                .execute_sqlite_script_typed(
                    VCE_SPACE_ID,
                    self.sqlite_binding_id.as_str(),
                    build_vce_schema_script(),
                    Vec::new(),
                )
                .await
                .map_err(|error| format!("初始化 VCE 数据结构失败：{error}"))?;
            if !schema_result.success {
                return Err(format!(
                    "初始化 VCE 数据结构失败：{}",
                    schema_result.message
                ));
            }
        }
        Ok(())
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
                self.execute(&script, Vec::new(), "激活 VCE 项目索引失败")
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
                .execute(&script, Vec::new(), "登记 VCE 项目索引失败")
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
                self.execute(&finish, Vec::new(), "完成 VCE 候选物理分配失败")
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
                    .map_err(|error| format!("解析 VCE 状态快照失败：{error}"))
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
            return Err(String::from("VCE 状态快照命名空间不能为空"));
        }
        let snapshot_json = serde_json::to_string(snapshot)
            .map_err(|error| format!("编码 VCE 状态快照失败：{error}"))?;
        self.execute(
            r#"INSERT INTO vce_state_snapshots(namespace_id, snapshot_json, updated_at_unix_ms)
               VALUES (?, ?, ?)
               ON CONFLICT(namespace_id) DO UPDATE SET
                   snapshot_json = excluded.snapshot_json,
                   updated_at_unix_ms = excluded.updated_at_unix_ms;"#,
            vec![
                sqlite_text(namespace_id),
                sqlite_text(snapshot_json),
                ControllerSqliteValue::Int64(current_unix_epoch_millis_i64()?),
            ],
            "持久化 VCE 状态快照失败",
        )
        .await
    }

    /// Create a new binding's dimension-specific vector table without overwriting any existing table.
    /// 为新绑定创建维度固定的向量表，不覆盖任何现有表。
    /// `table_name` and `dimension` come from the admitted binding; errors remain explicit, including collisions.
    /// `table_name` 与 `dimension` 来自已准入绑定；包括名称冲突在内的错误均明确保留。
    async fn ensure_vector_table(&self, table_name: &str, dimension: u32) -> Result<(), String> {
        self.client
            .create_lancedb_table_typed(
                VCE_SPACE_ID,
                self.lancedb_binding_id.as_str(),
                table_name,
                vector_table_columns(dimension),
                // This argument is overwrite_if_exists, not an if-not-exists switch.
                // 此参数是 overwrite_if_exists，不是“仅不存在时创建”开关。
                false,
            )
            .await
            .map_err(|error| format!("创建 VCE LanceDB 表 `{table_name}` 失败：{error}"))?;
        Ok(())
    }

    /// Executes one typed SQLite statement and checks the controller receipt.
    /// 执行一条类型化 SQLite 语句并校验控制器回执。
    async fn execute(
        &self,
        sql: &str,
        params: Vec<ControllerSqliteValue>,
        context: &str,
    ) -> Result<(), String> {
        let result = self
            .client
            .execute_sqlite_script_typed(VCE_SPACE_ID, self.sqlite_binding_id.as_str(), sql, params)
            .await
            .map_err(|error| format!("{context}：{error}"))?;
        if !result.success {
            return Err(format!("{context}：{}", result.message));
        }
        Ok(())
    }

    /// Executes one repeated SQLite statement as a bounded controller batch.
    /// 通过有界控制器批次重复执行一条 SQLite 语句。
    async fn execute_batch(
        &self,
        sql: &str,
        items: Vec<Vec<ControllerSqliteValue>>,
        context: &str,
    ) -> Result<(), String> {
        if items.is_empty() {
            return Ok(());
        }
        let result = self
            .client
            .execute_sqlite_batch_typed(VCE_SPACE_ID, self.sqlite_binding_id.as_str(), sql, items)
            .await
            .map_err(|error| format!("{context}：{error}"))?;
        if !result.success {
            return Err(format!("{context}：{}", result.message));
        }
        Ok(())
    }

    /// Queries typed SQLite rows through the controller JSON projection.
    /// 通过控制器 JSON 投影查询类型化 SQLite 行。
    async fn query_rows<T: for<'de> Deserialize<'de>>(
        &self,
        sql: &str,
        params: Vec<ControllerSqliteValue>,
    ) -> Result<Vec<T>, String> {
        let result = self
            .client
            .query_sqlite_json_typed(VCE_SPACE_ID, self.sqlite_binding_id.as_str(), sql, params)
            .await
            .map_err(|error| format!("查询 VCE SQLite 失败：{error}"))?;
        serde_json::from_str(&result.json_data)
            .map_err(|error| format!("解析 VCE SQLite 查询结果失败：{error}"))
    }

    /// Validate an operation belongs to the physical project's exact owner.
    /// 校验操作属于物理项目的精确所有者。
    ///
    /// `project_id` selects the requested owner. Legacy shared reads retain namespace checks.
    /// Returns an explicit error for cross-project access.
    /// `project_id` 选择请求所有者。旧共享读取保留命名空间检查。跨项目访问返回明确错误。
    fn validate_project_owner(&self, project_id: &str) -> Result<(), String> {
        if self.project_owner != project_id {
            return Err(String::from("VCE physical project storage owner mismatch"));
        }
        Ok(())
    }

    /// Release only these exact controller bindings after every external lease has drained.
    /// 全部外部租约排空后仅释放这些精确控制器绑定。
    ///
    /// Takes no extra parameters; returns combined cleanup errors without deleting stored data.
    /// 无额外参数；返回合并清理错误，不删除存储数据。
    pub(crate) async fn close_bindings(&self) -> Result<(), String> {
        self.flush_released_read_leases().await?;
        // failures retains both independent backend cleanup diagnostics for a later retry.
        // failures 保留两个独立后端的清理诊断，供后续重试。
        let mut failures = Vec::new();
        if let Err(error) = self
            .client
            .disable_lancedb(VCE_SPACE_ID, &self.lancedb_binding_id)
            .await
        {
            failures.push(format!("Close VCE vectors: {error}"));
        }
        if let Err(error) = self
            .client
            .disable_sqlite(VCE_SPACE_ID, &self.sqlite_binding_id)
            .await
        {
            failures.push(format!("Close VCE metadata: {error}"));
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
    content TEXT NOT NULL,
    kind TEXT NOT NULL,
    PRIMARY KEY(namespace_id, refresh_id, generation, occurrence_id),
    FOREIGN KEY(namespace_id, refresh_id, generation, blob_name)
        REFERENCES vce_staged_blobs(namespace_id, refresh_id, generation, blob_name) ON DELETE CASCADE
);



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
            .execute(&script, Vec::new(), "撤销 VCE 候选发布权限失败")
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
                    ControllerSqliteValue::Int64(now),
                    ControllerSqliteValue::Int64(now),
                ],
                "登记 VCE 暂存代次失败",
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
        for blob in &request.blobs {
            self.write_blob_vectors(blob, &request.refresh_id)
                .await
                .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
            self.replace_staged_blob(blob, &request.refresh_id)
                .await
                .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        }
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
                ControllerSqliteValue::Int64(abort_recorded_at),
                ControllerSqliteValue::Int64(abort_recorded_at),
            ],
            "记录 VCE 中止身份失败",
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
            .execute(&script, Vec::new(), "中止 VCE 暂存事务失败")
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
        self.data_plane
            .execute(&script, Vec::new(), "发布 VCE 检查点失败")
            .await
            .map_err(|message| port_failed(VcePortOperation::Commit, message))?;
        // Cleanup is derived and may be retried later; the durable queue was committed atomically above.
        // 清理属于派生操作且可稍后重试；其持久队列已在上方原子提交。
        let _cleanup_result = self.recover_pending_vector_cleanup().await;
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
                          s.start_char_offset, s.chunk_id, s.content, s.kind
                   FROM vce_symbol_versions s JOIN vce_read_members m
                   ON m.namespace_id = s.namespace_id AND m.refresh_id = s.refresh_id
                     AND m.generation = s.generation AND m.blob_name = s.blob_name
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
                    .then(|| format!("检测到 {} 项 VCE 索引完整性问题", issues.len())),
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

    /// Writes content and path vectors before relational publication.
    /// 在关系状态发布前写入内容与路径向量。
    async fn write_blob_vectors(&self, blob: &IndexedBlob, refresh_id: &str) -> Result<(), String> {
        if !blob.embedding_vectors.is_empty() {
            let chunks = blob
                .chunks
                .iter()
                .map(|chunk| (chunk.chunk_id.as_str(), chunk))
                .collect::<BTreeMap<_, _>>();
            let written_at_unix_ms = current_unix_epoch_millis_i64()?;
            let mut rows = Vec::with_capacity(blob.embedding_vectors.len());
            for vector in &blob.embedding_vectors {
                let chunk = chunks.get(vector.input_id.as_str()).ok_or_else(|| {
                    format!(
                        "VCE vector `{}` has no matching Chunk in Blob `{}`",
                        vector.input_id, blob.blob_name
                    )
                })?;
                rows.push(serde_json::json!({
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
            self.upsert_vector_rows(&self.content_table, rows).await?;
        }
        if let (Some(document), Some(vector)) = (&blob.path_document, &blob.path_embedding) {
            let written_at_unix_ms = current_unix_epoch_millis_i64()?;
            self.upsert_vector_rows(
                &self.path_table,
                vec![serde_json::json!({
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
                })],
            )
            .await?;
        }
        Ok(())
    }

    /// Upserts one validated vector batch into an exact physical table.
    /// 将一个已校验向量批次写入精确物理表。
    async fn upsert_vector_rows(
        &self,
        table_name: &str,
        rows: Vec<serde_json::Value>,
    ) -> Result<(), String> {
        if rows.is_empty() {
            return Ok(());
        }
        let data = serde_json::to_vec(&rows)
            .map_err(|error| format!("编码 VCE LanceDB 向量批次失败：{error}"))?;
        self.data_plane
            .client
            .upsert_lancedb_typed(
                VCE_SPACE_ID,
                self.data_plane.lancedb_binding_id.as_str(),
                table_name,
                ControllerLanceDbInputFormat::JsonRows,
                data,
                vec![String::from("row_id")],
            )
            .await
            .map_err(|error| format!("写入 VCE LanceDB 表 `{table_name}` 失败：{error}"))?;
        Ok(())
    }

    /// Replaces one Blob's staged relational records idempotently.
    /// 幂等替换一个 Blob 的暂存关系记录。
    async fn replace_staged_blob(
        &self,
        blob: &IndexedBlob,
        refresh_id: &str,
    ) -> Result<(), String> {
        self.data_plane
            .execute(
                r#"DELETE FROM vce_staged_blobs
                   WHERE namespace_id = ? AND refresh_id = ? AND generation = ? AND blob_name = ?;"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(refresh_id),
                    sqlite_u64(blob.generation, "VCE generation")
                        .map_err(|error| error.to_string())?,
                    sqlite_text(&blob.blob_name),
                ],
                "清理 VCE 旧暂存 Blob 失败",
            )
            .await?;
        let (path_status, path_error) = auxiliary_status_fields(&blob.path_status);
        let (symbol_status, symbol_error) = auxiliary_status_fields(&blob.symbol_status);
        let (status, status_reason) = indexed_status_fields(&blob.status);
        self.data_plane
            .execute(
                r#"INSERT INTO vce_staged_blobs(
                    namespace_id, refresh_id, generation, blob_name, path,
                    structure_analyzer_id, structure_mode, structure_fallback_reason,
                    path_document, path_status, path_error, symbol_status, symbol_error,
                    status, status_reason, chunk_count, vector_count, symbol_count
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#,
                vec![
                    sqlite_text(&self.binding.namespace_id),
                    sqlite_text(refresh_id),
                    sqlite_u64(blob.generation, "VCE generation")
                        .map_err(|error| error.to_string())?,
                    sqlite_text(&blob.blob_name),
                    sqlite_text(&blob.path),
                    sqlite_optional_text(blob.structure_analyzer_id.as_deref()),
                    sqlite_optional_text(blob.structure_mode.map(structure_mode_text)),
                    sqlite_optional_text(blob.structure_fallback_reason.as_deref()),
                    sqlite_optional_text(
                        blob.path_document
                            .as_ref()
                            .map(|item| item.document.as_str()),
                    ),
                    sqlite_text(path_status),
                    sqlite_optional_text(path_error),
                    sqlite_text(symbol_status),
                    sqlite_optional_text(symbol_error),
                    sqlite_text(status),
                    sqlite_optional_text(status_reason),
                    sqlite_usize(blob.chunks.len(), "VCE chunk count")
                        .map_err(|error| error.to_string())?,
                    sqlite_usize(
                        blob.embedding_vectors.len() + usize::from(blob.path_embedding.is_some()),
                        "VCE vector count",
                    )
                    .map_err(|error| error.to_string())?,
                    sqlite_usize(blob.symbols.len(), "VCE symbol count")
                        .map_err(|error| error.to_string())?,
                ],
                "写入 VCE 暂存 Blob 失败",
            )
            .await?;
        let vector_ids = blob
            .embedding_vectors
            .iter()
            .map(|vector| vector.input_id.as_str())
            .collect::<BTreeSet<_>>();
        self.data_plane
            .execute_batch(
                r#"INSERT INTO vce_staged_chunks(
                    namespace_id, refresh_id, generation, blob_name, chunk_id, content_hash,
                    path, content, start_line, end_line, start_char_offset, kind, vector_ready
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#,
                blob.chunks
                    .iter()
                    .map(|chunk| {
                        staged_chunk_params(
                            &self.binding.namespace_id,
                            refresh_id,
                            blob.generation,
                            chunk,
                            vector_ids.contains(chunk.chunk_id.as_str()),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                "写入 VCE 暂存 Chunk 失败",
            )
            .await?;
        self.data_plane
            .execute_batch(
                r#"INSERT INTO vce_staged_symbols(
                    namespace_id, refresh_id, generation, blob_name, occurrence_id, identifier,
                    path, start_line, end_line, start_char_offset, chunk_id, content, kind
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#,
                blob.symbols
                    .iter()
                    .map(|symbol| {
                        staged_symbol_params(
                            &self.binding.namespace_id,
                            refresh_id,
                            blob.generation,
                            symbol,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                "写入 VCE 暂存符号失败",
            )
            .await?;
        Ok(())
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
VALUES ('{namespace}', CASE WHEN ({binding_condition}) AND ({cas_condition}) AND ({generation_condition}) THEN 1 ELSE 0 END);
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
 end_line, start_char_offset, chunk_id, content, kind)
SELECT namespace_id, refresh_id, generation, blob_name, occurrence_id, identifier, path, start_line,
 end_line, start_char_offset, chunk_id, content, kind
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
                                ControllerSqliteValue::Int64(now),
                                sqlite_text(&self.binding.namespace_id),
                                sqlite_text(&row.cleanup_id),
                            ],
                            "完成 VCE 向量清理记录失败",
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
                                ControllerSqliteValue::Int64(now),
                                sqlite_text(&self.binding.namespace_id),
                                sqlite_text(&row.cleanup_id),
                            ],
                            "记录 VCE 向量清理失败诊断失败",
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
            ControllerSqliteValue::Int64(generation),
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
                .client
                .delete_lancedb_typed(
                    VCE_SPACE_ID,
                    self.data_plane.lancedb_binding_id.as_str(),
                    table,
                    predicate.clone(),
                )
                .await
                .map_err(|error| format!("Delete exact VCE vectors from `{table}`: {error}"))?;
        }
        if retired_blob.is_some() {
            // Metadata follows successful vector deletion; interruptions remain safely retryable.
            // 元数据在向量成功删除之后清理；中断后仍可安全重试。
            self.data_plane.execute(
                "DELETE FROM vce_blob_versions WHERE namespace_id = ? AND refresh_id = ? AND generation = ? AND blob_name = ? AND NOT EXISTS(SELECT 1 FROM vce_read_members m WHERE m.namespace_id = vce_blob_versions.namespace_id AND m.refresh_id = vce_blob_versions.refresh_id AND m.generation = vce_blob_versions.generation AND m.blob_name = vce_blob_versions.blob_name);",
                parameters, "回收无读取引用的 VCE 代码版本失败",
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
            .client
            .search_lancedb_typed(
                VCE_SPACE_ID,
                self.data_plane.lancedb_binding_id.as_str(),
                table_name,
                query_vector.to_vec(),
                physical_limit,
                predicate,
                "vector",
                ControllerLanceDbOutputFormat::JsonRows,
            )
            .await
            .map_err(|error| port_failed(operation, format!("VCE LanceDB 检索失败：{error}")))?;
        let mut rows: Vec<LanceHitRow> = serde_json::from_slice(&result.data)
            .map_err(|error| port_invalid(operation, format!("解析 VCE 向量候选失败：{error}")))?;
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
                return Err(port_invalid(operation, "VCE LanceDB 返回了越界或无效候选"));
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
                r#"SELECT blob_name, path, status, path_status, symbol_status, chunk_count,
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
                    "检查点引用的 Blob 不存在",
                ));
                continue;
            };
            if row.status != "ready" || (row.chunk_count > 0 && row.vector_count < row.chunk_count)
            {
                issues.push(integrity_issue(
                    VceIntegrityIssueKind::IncompleteContentIndex,
                    Some(row.path.clone()),
                    Some(row.blob_name.clone()),
                    "Blob 的内容代码块或向量不完整",
                ));
            }
            if row.path_status == "failed" {
                issues.push(integrity_issue(
                    VceIntegrityIssueKind::PathIndexMismatch,
                    Some(row.path.clone()),
                    Some(row.blob_name.clone()),
                    "Blob 的路径索引通道失败",
                ));
            }
            if row.symbol_status == "failed" {
                issues.push(integrity_issue(
                    VceIntegrityIssueKind::SymbolIndexMismatch,
                    Some(row.path.clone()),
                    Some(row.blob_name.clone()),
                    "Blob 的精确符号通道失败",
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
                        .map_or(ControllerSqliteValue::Null, sqlite_text),
                    self.management_active_refresh
                        .as_ref()
                        .map_or(ControllerSqliteValue::Null, sqlite_text),
                ],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        for row in staged {
            // Ownership is unconfirmed, not proof that an external producer has stopped.
            // 活动归属尚未确认，并不证明外部生产者已经停止。
            let message = format!(
                "暂存 Blob 的活动归属尚未确认（刷新 {}，代次 {}）",
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
                || format!("派生向量清理 `{}` 等待执行", row.cleanup_id),
                |error| format!("派生向量清理 `{}` 等待重试：{error}", row.cleanup_id),
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
            VceMaintenanceMode::Repair => "修复已识别的 VCE 索引完整性问题",
            VceMaintenanceMode::Rebuild => "重建当前项目的全部 VCE 派生索引",
            VceMaintenanceMode::CleanupStaging => "清理未发布的 VCE 暂存数据",
        };
        let action_required = match mode {
            VceMaintenanceMode::Repair => !issues.is_empty(),
            // Rebuild is always an explicit destructive action, including an empty binding that needs initialization.
            // 重建总是明确的破坏性操作，包括需要初始化的空绑定。
            VceMaintenanceMode::Rebuild => true,
            VceMaintenanceMode::CleanupStaging => issues
                .iter()
                .any(|issue| issue.kind == VceIntegrityIssueKind::OrphanedStaging),
        };
        let actions = action_required
            .then(|| VceMaintenanceAction {
                action_id: maintenance_action_id(mode, &blob_names),
                description: String::from(description),
                blob_names: blob_names.clone(),
                destructive: true,
            })
            .into_iter()
            .collect();
        Ok(VceMaintenancePlan {
            mode,
            actions,
            affected_blob_count: blob_names.len() as u64,
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
                    ControllerSqliteValue::Int64(now),
                    ControllerSqliteValue::Int64(now),
                ],
                "记录 VCE 操作失败",
            )
            .await
    }
}
