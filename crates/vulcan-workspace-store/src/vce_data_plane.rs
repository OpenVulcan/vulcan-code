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
    ControllerSqliteEnableRequest, ControllerSqliteValue, SpaceKind, SpaceRegistration,
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
pub const VCE_DATA_PLANE_SCHEMA_VERSION: i64 = 1;

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
    /// Vector count.
    /// 向量数量。
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
    /// Optional generation retained by superseded-row cleanup.
    /// 过期行清理保留的可选代次。
    keep_generation: Option<i64>,
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

/// One unpublished generation discovered during startup recovery.
/// 启动恢复期间发现的一个未发布代次。
#[derive(Clone, Debug, Deserialize)]
struct AbandonedGenerationRow {
    /// Stable refresh identity.
    /// 稳定刷新身份。
    refresh_id: String,
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
    generation: u64,
    logical_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(namespace_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(physical_fingerprint.as_bytes());
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

#[cfg(test)]
mod tests {
    use super::{
        VCE_SPACE_ID, VceProjectIndexConfig, bootstrap_vce_data_plane, build_vce_schema_script,
        vector_table_name,
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

    /// Deterministic shared-AI boundary used by the real VCE data-plane test.
    /// 真实 VCE 数据面测试使用的确定性共享 AI 边界。
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
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_checkpoints"));
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
        let data_plane = bootstrap_vce_data_plane(&controller)
            .await
            .expect("bootstrap VCE data plane");
        let first = test_project_config("project-1", &first_root, 1);
        let first_index = data_plane
            .open_project_index(first)
            .await
            .expect("open first VCE project binding");
        assert!(
            first_index
                .binding()
                .namespace_id
                .starts_with("vce:project:project-1:root:")
        );
        let second = test_project_config("project-1", &second_root, 2);
        let second_index = data_plane
            .open_project_index(second)
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
        let first_commit = second_index
            .commit(commit_request.clone())
            .await
            .expect("commit VCE integration Blob");
        let replayed_commit = second_index
            .commit(commit_request)
            .await
            .expect("replay exact VCE commit idempotently");
        assert_eq!(replayed_commit, first_commit);
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
        let search = second_index
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
        println!(
            "VCE_STORAGE sqlite_bytes={} lancedb_bytes={} total_bytes={}",
            file_or_directory_size(&data_plane.sqlite_path),
            file_or_directory_size(&data_plane.lancedb_path),
            file_or_directory_size(&data_plane.data_root),
        );
        let mut aborted_blob = indexed_blob;
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
        second_index
            .reset_incompatible_state()
            .await
            .expect("reset incompatible VCE checkpoint and vectors");
        assert!(
            data_plane
                .load_state_snapshot(&second_index.binding().namespace_id)
                .await
                .expect("load reset VCE state")
                .is_none()
        );
        second_index
            .search_dense(DenseSearchRequest {
                request_id: String::from("search-after-reset"),
                query_index: 0,
                query_vector: EmbeddingVector {
                    input_id: String::from("query-after-reset"),
                    values: vec![1.0, 0.0, 0.0],
                },
                allowed_blobs: BTreeSet::from([blob_name]),
                limit: 4,
            })
            .await
            .expect_err("reset VCE binding must remain unsearchable until rebuild commit");
        let pending_cleanup_after_reset: Vec<BindingCountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_vector_cleanup WHERE namespace_id = ? AND state = 'pending';",
                vec![super::sqlite_text(&second_index.binding().namespace_id)],
            )
            .await
            .expect("count reset VCE cleanup rows");
        assert_eq!(pending_cleanup_after_reset[0].count, 0);
        client.shutdown().await.expect("shutdown VCE controller");
        assert_eq!(VCE_SPACE_ID, "vce");
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
        let data_plane = bootstrap_vce_data_plane(&controller)
            .await
            .expect("bootstrap VCE engine data plane");
        let first_index = data_plane
            .open_project_index(test_project_config("project-1", &first_root, 1))
            .await
            .expect("open first VCE engine project index");
        let first_ai = deterministic_ai_for(&first_index);
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
        let initial_retrieval = first_engine
            .retrieve(
                RetrieveRequest::current_checkpoint("e2e-query-1", "find semantic entry"),
                &first_index,
                &first_ai,
            )
            .await
            .expect("retrieve initial VCE context");
        assert!(!initial_retrieval.hits.is_empty());

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
        assert_eq!(
            changed_result.deleted_blobs,
            BTreeSet::from([old_lib_blob.clone()])
        );
        assert!(
            first_index
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
                .expect("search removed lib Blob")
                .hits
                .is_empty()
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

        let persisted = data_plane
            .load_state_snapshot(&first_index.binding().namespace_id)
            .await
            .expect("load VCE restart state")
            .expect("VCE restart state exists");
        let mut restored_engine = VceEngine::from_state_snapshot(first_config, persisted)
            .expect("restore VCE engine from checkpoint state");
        let restored_retrieval = restored_engine
            .retrieve(
                RetrieveRequest::current_checkpoint("e2e-query-restored", "semantic entry"),
                &first_index,
                &first_ai,
            )
            .await
            .expect("retrieve VCE context after restart");
        assert!(!restored_retrieval.hits.is_empty());

        let second_index = data_plane
            .open_project_index(test_project_config("project-2", &second_root, 1))
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
        assert!(
            first_index
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
                .expect("search project-two Blob in project-one namespace")
                .hits
                .is_empty()
        );
        assert!(
            second_index
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
                .expect("search project-one Blob in project-two namespace")
                .hits
                .is_empty()
        );
        let second_retrieval = second_engine
            .retrieve(
                RetrieveRequest::current_checkpoint("e2e-query-project-two", "project two only"),
                &second_index,
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
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// Attaches and initializes the host-owned VCE controller space.
/// 附着并初始化宿主持有的 VCE 控制器空间。
pub async fn bootstrap_vce_data_plane(
    controller: &ControllerRuntimeHandle,
) -> Result<VceDataPlaneHandle, String> {
    let data_root = controller.data_root.join("vce");
    let sqlite_root = data_root.join("sqlite");
    let lancedb_path = data_root.join("lancedb");
    let sqlite_path = sqlite_root.join("vce.sqlite3");
    std::fs::create_dir_all(&sqlite_root)
        .map_err(|error| format!("创建 VCE SQLite 目录失败：{error}"))?;
    std::fs::create_dir_all(&lancedb_path)
        .map_err(|error| format!("创建 VCE LanceDB 目录失败：{error}"))?;

    let binding_suffix = build_binding_suffix();
    let sqlite_binding_id = format!("{VCE_SQLITE_BINDING_TAG}-{binding_suffix}");
    let lancedb_binding_id = format!("{VCE_LANCEDB_BINDING_TAG}-{binding_suffix}");
    let client = controller.client.clone();
    client
        .attach_space(SpaceRegistration {
            space_id: String::from(VCE_SPACE_ID),
            space_label: String::from("VCE Code Context"),
            space_kind: SpaceKind::User,
            space_root: runtime_assets::native_host_path_text_from_path(&data_root),
        })
        .await
        .map_err(|error| format!("附着 VCE 数据空间失败：{error}"))?;
    client
        .enable_sqlite(ControllerSqliteEnableRequest {
            space_id: String::from(VCE_SPACE_ID),
            binding_id: sqlite_binding_id.clone(),
            db_path: runtime_assets::native_host_path_text_from_path(&sqlite_path),
            ..ControllerSqliteEnableRequest::default()
        })
        .await
        .map_err(|error| format!("启用 VCE SQLite 数据库失败：{error}"))?;
    client
        .enable_lancedb(ControllerLanceDbEnableRequest {
            space_id: String::from(VCE_SPACE_ID),
            binding_id: lancedb_binding_id.clone(),
            default_db_path: runtime_assets::native_host_path_text_from_path(&lancedb_path),
            db_root: Some(runtime_assets::native_host_path_text_from_path(
                &lancedb_path,
            )),
            read_consistency_interval_ms: Some(250),
            max_upsert_payload: 16 * 1024 * 1024,
            max_search_limit: VCE_VECTOR_SEARCH_LIMIT as usize,
            max_concurrent_requests: 16,
        })
        .await
        .map_err(|error| format!("启用 VCE LanceDB 数据库失败：{error}"))?;
    let schema_result = client
        .execute_sqlite_script_typed(
            VCE_SPACE_ID,
            sqlite_binding_id.as_str(),
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
    Ok(VceDataPlaneHandle {
        client,
        data_root,
        sqlite_path,
        lancedb_path,
        sqlite_binding_id,
        lancedb_binding_id,
    })
}

impl VceDataPlaneHandle {
    /// Opens or validates one project-specific VCE index binding.
    /// 打开或校验一个项目专用的 VCE 索引绑定。
    pub async fn open_project_index(
        &self,
        config: VceProjectIndexConfig,
    ) -> Result<VceProjectIndex, String> {
        config.validate()?;
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
        self.ensure_vector_table(&content_table, config.dimension)
            .await?;
        self.ensure_vector_table(&path_table, config.dimension)
            .await?;
        let rows: Vec<ProjectBindingRow> = self
            .query_rows(
                "SELECT project_id, root_path, root_hash, namespace_id, checkpoint_chain_id, schema_version, vector_space_id, physical_fingerprint, preprocessing_version, chunker_version, dimension, content_table, path_table, config_revision, active FROM vce_project_bindings WHERE namespace_id = ? LIMIT 1;",
                vec![sqlite_text(&config.binding.namespace_id)],
            )
            .await?;
        if let Some(row) = rows.first() {
            row.validate_identity(&config)?;
            if config_revision < row.config_revision {
                return Err(format!(
                    "VCE project `{}` requested stale config revision {} below persisted revision {}",
                    config.project_id, config_revision, row.config_revision
                ));
            }
            let now = current_unix_epoch_millis_i64()?;
            if !row.vector_compatibility_matches(&config, &content_table, &path_table) {
                let previous_dimension = u32::try_from(row.dimension)
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| String::from("persisted VCE vector dimension is invalid"))?;
                let previous_config_revision = u64::try_from(row.config_revision)
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| String::from("persisted VCE config revision is invalid"))?;
                let previous_index = VceProjectIndex {
                    data_plane: self.clone(),
                    project_id: row.project_id.clone(),
                    root_path: row.root_path.clone(),
                    root_hash: row.root_hash.clone(),
                    binding: config.binding.clone(),
                    vector_space_id: row.vector_space_id.clone(),
                    physical_fingerprint: row.physical_fingerprint.clone(),
                    preprocessing_version: row.preprocessing_version.clone(),
                    chunker_version: row.chunker_version.clone(),
                    dimension: previous_dimension,
                    config_revision: previous_config_revision,
                    content_table: row.content_table.clone(),
                    path_table: row.path_table.clone(),
                    mutation_lock: Arc::new(Mutex::new(())),
                };
                previous_index
                    .enqueue_abandoned_generation_cleanup()
                    .await?;
                let _cleanup_result = previous_index.recover_pending_vector_cleanup().await;
                let project_id = sqlite_string_literal(&config.project_id);
                let namespace_id = sqlite_string_literal(&config.binding.namespace_id);
                let physical_fingerprint = sqlite_string_literal(&config.physical_fingerprint);
                let preprocessing_version = sqlite_string_literal(&config.preprocessing_version);
                let chunker_version = sqlite_string_literal(&config.chunker_version);
                let content_table_sql = sqlite_string_literal(&content_table);
                let path_table_sql = sqlite_string_literal(&path_table);
                let dimension = config.dimension;
                let script = format!(
                    r#"BEGIN IMMEDIATE;
                       UPDATE vce_project_bindings
                       SET active = 0, lifecycle_state = 'orphaned', updated_at_unix_ms = {now}
                       WHERE project_id = '{project_id}' AND namespace_id <> '{namespace_id}' AND active = 1;
                       UPDATE vce_embedding_spaces SET lifecycle_state = 'orphaned', updated_at_unix_ms = {now}
                       WHERE namespace_id = '{namespace_id}' AND lifecycle_state = 'active';
                       INSERT INTO vce_embedding_spaces(
                           namespace_id, physical_fingerprint, preprocessing_version,
                           chunker_version, dimension, content_table, path_table,
                           lifecycle_state, created_at_unix_ms, updated_at_unix_ms
                       ) VALUES ('{namespace_id}', '{physical_fingerprint}', '{preprocessing_version}',
                           '{chunker_version}', {dimension}, '{content_table_sql}', '{path_table_sql}',
                           'active', {now}, {now})
                       ON CONFLICT(namespace_id, physical_fingerprint, preprocessing_version, chunker_version, dimension)
                       DO UPDATE SET content_table = excluded.content_table,
                           path_table = excluded.path_table, lifecycle_state = 'active',
                           updated_at_unix_ms = excluded.updated_at_unix_ms;
                       UPDATE vce_project_bindings SET
                           physical_fingerprint = '{physical_fingerprint}',
                           preprocessing_version = '{preprocessing_version}',
                           chunker_version = '{chunker_version}', dimension = {dimension},
                           content_table = '{content_table_sql}', path_table = '{path_table_sql}',
                           config_revision = {config_revision}, active = 1,
                           lifecycle_state = 'rebuild_required', updated_at_unix_ms = {now}
                       WHERE namespace_id = '{namespace_id}';
                       DELETE FROM vce_state_snapshots WHERE namespace_id = '{namespace_id}';
                       DELETE FROM vce_checkpoints WHERE namespace_id = '{namespace_id}';
                       DELETE FROM vce_blobs WHERE namespace_id = '{namespace_id}';
                       DELETE FROM vce_staged_blobs WHERE namespace_id = '{namespace_id}';
                       UPDATE vce_generations SET state = 'aborted',
                           error = 'vector compatibility changed', updated_at_unix_ms = {now}
                       WHERE namespace_id = '{namespace_id}' AND state = 'staging';
                       COMMIT;"#,
                );
                self.execute(&script, Vec::new(), "切换 VCE 物理向量兼容性失败")
                    .await?;
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
            let script = format!(
                r#"BEGIN IMMEDIATE;
                   UPDATE vce_project_bindings
                   SET active = 0, lifecycle_state = 'orphaned', updated_at_unix_ms = {now}
                   WHERE project_id = '{project_id}' AND active = 1;
                   INSERT INTO vce_project_bindings(
                       namespace_id, project_id, root_path, root_hash, checkpoint_chain_id, schema_version,
                       vector_space_id, physical_fingerprint, preprocessing_version,
                       chunker_version, dimension, content_table, path_table, config_revision,
                       active, lifecycle_state, created_at_unix_ms, updated_at_unix_ms
                   ) VALUES ('{namespace_id}', '{project_id}', '{root_path}', '{root_hash}',
                       '{checkpoint_chain_id}', {schema_version}, '{vector_space_id}',
                       '{physical_fingerprint}', '{preprocessing_version}', '{chunker_version}',
                       {dimension}, '{content_table_sql}', '{path_table_sql}', {config_revision},
                       1, 'rebuild_required', {now}, {now});
                   INSERT INTO vce_embedding_spaces(
                       namespace_id, physical_fingerprint, preprocessing_version,
                       chunker_version, dimension, content_table, path_table,
                       lifecycle_state, created_at_unix_ms, updated_at_unix_ms
                   ) VALUES ('{namespace_id}', '{physical_fingerprint}', '{preprocessing_version}',
                       '{chunker_version}', {dimension}, '{content_table_sql}', '{path_table_sql}',
                       'active', {now}, {now});
                   COMMIT;"#,
            );
            self.execute(&script, Vec::new(), "登记 VCE 项目索引失败")
                .await?;
        }
        let index = VceProjectIndex {
            data_plane: self.clone(),
            project_id: config.project_id,
            root_path: config.root_path,
            root_hash: config.root_hash,
            binding: config.binding,
            vector_space_id: config.vector_space_id,
            physical_fingerprint: config.physical_fingerprint,
            preprocessing_version: config.preprocessing_version,
            chunker_version: config.chunker_version,
            dimension: config.dimension,
            config_revision: config.config_revision,
            content_table,
            path_table,
            mutation_lock: Arc::new(Mutex::new(())),
        };
        index.enqueue_abandoned_generation_cleanup().await?;
        // Cleanup failures remain durable management diagnostics and must not hide a valid checkpoint.
        // 清理失败会保留为持久管理诊断，不得遮蔽有效检查点。
        let _cleanup_result = index.recover_pending_vector_cleanup().await;
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

    /// Creates one dimension-specific vector table when absent.
    /// 在表不存在时创建一张维度固定的向量表。
    async fn ensure_vector_table(&self, table_name: &str, dimension: u32) -> Result<(), String> {
        self.client
            .create_lancedb_table_typed(
                VCE_SPACE_ID,
                self.lancedb_binding_id.as_str(),
                table_name,
                vector_table_columns(dimension),
                true,
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

CREATE TABLE IF NOT EXISTS vce_checkpoints (
    namespace_id TEXT PRIMARY KEY,
    checkpoint_version INTEGER NOT NULL CHECK(checkpoint_version > 0),
    generation INTEGER NOT NULL CHECK(generation > 0),
    refresh_id TEXT NOT NULL CHECK(length(trim(refresh_id)) > 0),
    checkpoint_json TEXT NOT NULL,
    source_revision TEXT,
    committed_at_unix_ms INTEGER NOT NULL,
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);

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

CREATE TABLE IF NOT EXISTS vce_blobs (
    namespace_id TEXT NOT NULL,
    blob_name TEXT NOT NULL,
    path TEXT NOT NULL,
    generation INTEGER NOT NULL,
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
    PRIMARY KEY(namespace_id, blob_name),
    UNIQUE(namespace_id, path)
);
CREATE INDEX IF NOT EXISTS idx_vce_blobs_path ON vce_blobs(namespace_id, path);

CREATE TABLE IF NOT EXISTS vce_chunks (
    namespace_id TEXT NOT NULL,
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
    PRIMARY KEY(namespace_id, chunk_id),
    FOREIGN KEY(namespace_id, blob_name) REFERENCES vce_blobs(namespace_id, blob_name) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_vce_chunks_blob ON vce_chunks(namespace_id, blob_name, start_line);

CREATE TABLE IF NOT EXISTS vce_symbols (
    namespace_id TEXT NOT NULL,
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
    PRIMARY KEY(namespace_id, occurrence_id),
    FOREIGN KEY(namespace_id, blob_name) REFERENCES vce_blobs(namespace_id, blob_name) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_vce_symbols_identifier
ON vce_symbols(namespace_id, identifier, kind, path, start_line);

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
    cleanup_kind TEXT NOT NULL CHECK(cleanup_kind IN ('aborted_generation', 'deleted_blobs', 'superseded_blobs')),
    refresh_id TEXT,
    generation INTEGER,
    blob_names_json TEXT,
    keep_generation INTEGER,
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
        Box::pin(async move { index.commit_inner(request).await })
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
        Box::pin(async move { index.management_snapshot_inner(request).await })
    }

    /// Executes one typed read-only management query.
    /// 执行一项类型化只读管理查询。
    fn management_query(
        &self,
        request: VceManagementQueryRequest,
    ) -> VceFuture<Result<VceManagementQueryResponse, VcePortError>> {
        let index = self.clone();
        Box::pin(async move { index.management_query_inner(request).await })
    }
}

impl VceProjectIndex {
    /// Clears one incompatible committed state before an authoritative full rebuild.
    /// 在权威全量重建前清除一份不兼容的已提交状态。
    ///
    /// This host-only recovery entry first converges older cleanup requests, then makes the
    /// incompatible checkpoint unsearchable in one SQLite transaction and durably queues removal
    /// of its derived vectors. It never touches another namespace.
    /// 此宿主专用恢复入口先收敛旧清理请求，再通过单个 SQLite 事务使不兼容检查点不可检索，
    /// 并持久排队清理其派生向量；它绝不会触碰其他命名空间。
    ///
    /// # Returns
    /// 返回值
    /// Success after relational reset and derived-vector cleanup converge.
    /// 在关系状态重置与派生向量清理均收敛后返回成功。
    pub async fn reset_incompatible_state(&self) -> Result<(), String> {
        let _guard = self.mutation_lock.lock().await;
        self.require_active_binding(VcePortOperation::ManagementSnapshot, false)
            .await
            .map_err(|error| error.to_string())?;
        self.enqueue_abandoned_generation_cleanup().await?;
        self.recover_pending_vector_cleanup().await?;

        let blob_rows: Vec<BlobNameRow> = self
            .data_plane
            .query_rows(
                "SELECT blob_name FROM vce_blobs WHERE namespace_id = ? ORDER BY blob_name;",
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await?;
        let blob_names = blob_rows
            .into_iter()
            .map(|row| row.blob_name)
            .collect::<BTreeSet<_>>();
        let checkpoint = self
            .current_checkpoint_row()
            .await
            .map_err(|error| error.to_string())?;
        let cleanup_clause = if blob_names.is_empty() {
            String::new()
        } else {
            let checkpoint_identity = checkpoint
                .as_ref()
                .map(|row| format!("{}:{}", row.checkpoint_version, row.generation))
                .unwrap_or_else(|| String::from("missing"));
            let cleanup_id = vector_cleanup_id(
                &self.binding.namespace_id,
                "deleted_blobs",
                &format!("incompatible-state:{checkpoint_identity}"),
            );
            let blob_names_json = serde_json::to_string(&blob_names)
                .map_err(|error| format!("encode incompatible VCE Blob set: {error}"))?;
            format!(
                r#"INSERT INTO vce_vector_cleanup(
                       namespace_id, cleanup_id, cleanup_kind, refresh_id, generation,
                       blob_names_json, keep_generation, physical_fingerprint,
                       content_table, path_table, state, attempt, error,
                       created_at_unix_ms, updated_at_unix_ms
                   ) VALUES ('{namespace}', '{cleanup_id}', 'deleted_blobs', NULL, NULL,
                       '{blob_names_json}', NULL, '{physical_fingerprint}', '{content_table}',
                       '{path_table}', 'pending', 0, NULL, {now}, {now})
                   ON CONFLICT(namespace_id, cleanup_id) DO NOTHING;"#,
                namespace = sqlite_string_literal(&self.binding.namespace_id),
                cleanup_id = sqlite_string_literal(&cleanup_id),
                blob_names_json = sqlite_string_literal(&blob_names_json),
                physical_fingerprint = sqlite_string_literal(&self.physical_fingerprint),
                content_table = sqlite_string_literal(&self.content_table),
                path_table = sqlite_string_literal(&self.path_table),
                now = current_unix_epoch_millis_i64()?,
            )
        };
        let namespace = sqlite_string_literal(&self.binding.namespace_id);
        let now = current_unix_epoch_millis_i64()?;
        let reset_script = format!(
            r#"BEGIN IMMEDIATE;
               {cleanup_clause}
               DELETE FROM vce_state_snapshots WHERE namespace_id = '{namespace}';
               DELETE FROM vce_checkpoints WHERE namespace_id = '{namespace}';
               DELETE FROM vce_blobs WHERE namespace_id = '{namespace}';
               DELETE FROM vce_commit_guards WHERE namespace_id = '{namespace}';
               UPDATE vce_project_bindings
               SET lifecycle_state = 'rebuild_required', updated_at_unix_ms = {now}
               WHERE namespace_id = '{namespace}';
               COMMIT;"#,
        );
        self.data_plane
            .execute(&reset_script, Vec::new(), "重置不兼容 VCE 状态失败")
            .await?;
        self.recover_pending_vector_cleanup().await
    }

    /// Clean every unpublished generation and retry its derived vector cleanup queue.
    /// 清理全部未发布代次，并重试其派生向量清理队列。
    ///
    /// This host-only entry is serialized with stage and commit operations and never removes the
    /// active committed generation.
    /// 此宿主专用入口与暂存和提交操作串行化，且绝不会移除活动已提交代次。
    ///
    /// # Returns
    /// 返回值
    /// Success after abandoned relational state is marked aborted and pending vector cleanup has
    /// either completed or returned an explicit failure.
    /// 在遗留关系状态标记为已中止且待处理向量清理完成或返回明确失败后返回成功。
    pub async fn cleanup_abandoned_staging(&self) -> Result<(), String> {
        let _guard = self.mutation_lock.lock().await;
        self.enqueue_abandoned_generation_cleanup().await?;
        self.recover_pending_vector_cleanup().await
    }

    /// Verifies this handle still represents the active persisted binding.
    /// 校验此句柄仍表示当前活动的持久化绑定。
    async fn require_active_binding(
        &self,
        operation: VcePortOperation,
        require_ready: bool,
    ) -> Result<(), VcePortError> {
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
        for blob in &request.blobs {
            self.write_blob_vectors(blob, &request.refresh_id)
                .await
                .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
            self.replace_staged_blob(blob, &request.refresh_id)
                .await
                .map_err(|message| port_failed(VcePortOperation::Stage, message))?;
        }
        let staged_count = u64::try_from(staged_blob_names.len()).unwrap_or(u64::MAX);
        self.record_operation(OperationRecord {
            operation_id: &request.operation_id,
            kind: VceOperationKind::IncrementalRefresh,
            phase: VceOperationPhase::Staging,
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
        require_operation_id(&request.operation_id, VcePortOperation::AbortStage)?;
        if request.refresh_id.trim().is_empty() || request.generation == 0 {
            return Err(port_rejected(
                VcePortOperation::AbortStage,
                "refresh_id and generation must be valid",
            ));
        }
        let _guard = self.mutation_lock.lock().await;
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
            Some("aborted") | None => {
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
               UPDATE vce_generations SET state = 'aborted', operation_id = '{operation_id}', updated_at_unix_ms = {now}
               WHERE namespace_id = '{namespace_id}' AND refresh_id = '{refresh_id}' AND generation = {generation} AND state = 'staging';
               INSERT INTO vce_vector_cleanup(
                   namespace_id, cleanup_id, cleanup_kind, refresh_id, generation,
                   blob_names_json, keep_generation, physical_fingerprint,
                   content_table, path_table, state, attempt, error,
                   created_at_unix_ms, updated_at_unix_ms
               ) VALUES ('{namespace_id}', '{cleanup_id_sql}', 'aborted_generation',
                   '{refresh_id}', {generation}, NULL, NULL, '{physical_fingerprint}',
                   '{content_table}', '{path_table}', 'pending', 0, NULL, {now}, {now})
               ON CONFLICT(namespace_id, cleanup_id) DO UPDATE SET
                   state = 'pending', error = NULL, updated_at_unix_ms = excluded.updated_at_unix_ms;
               DELETE FROM vce_staged_blobs
               WHERE namespace_id = '{namespace_id}' AND refresh_id = '{refresh_id}' AND generation = {generation};
               COMMIT;"#,
        );
        self.data_plane
            .execute(&script, Vec::new(), "中止 VCE 暂存事务失败")
            .await
            .map_err(|message| port_failed(VcePortOperation::AbortStage, message))?;
        self.record_operation(OperationRecord {
            operation_id: &request.operation_id,
            kind: VceOperationKind::IncrementalRefresh,
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

    /// Publishes staged rows and one checkpoint under a process-local CAS lock.
    /// 在进程内 CAS 锁下发布暂存行和一个检查点。
    async fn commit_inner(&self, request: IndexCommit) -> Result<CommitReceipt, VcePortError> {
        require_operation_id(&request.request_id, VcePortOperation::Commit)?;
        self.require_active_binding(VcePortOperation::Commit, false)
            .await?;
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
            return Ok(commit_receipt_from_request(request));
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
        let staged = self
            .staged_blob_names(&request.refresh_id, request.generation)
            .await?;
        if staged != request.staged_blob_names {
            return Err(port_rejected(
                VcePortOperation::Commit,
                "staged Blob membership does not match the commit",
            ));
        }
        let script = self.build_commit_script(&request, &request_fingerprint)?;
        self.data_plane
            .execute(&script, Vec::new(), "发布 VCE 检查点失败")
            .await
            .map_err(|message| port_failed(VcePortOperation::Commit, message))?;
        // Cleanup is derived and may be retried later; the durable queue was committed atomically above.
        // 清理属于派生操作且可稍后重试；其持久队列已在上方原子提交。
        let _cleanup_result = self.recover_pending_vector_cleanup().await;
        Ok(commit_receipt_from_request(request))
    }

    /// Searches LanceDB content candidates and materializes authoritative SQLite chunks.
    /// 检索 LanceDB 内容候选并物化权威 SQLite 代码块。
    async fn search_dense_inner(
        &self,
        request: DenseSearchRequest,
    ) -> Result<VceSearchResponse, VcePortError> {
        require_operation_id(&request.request_id, VcePortOperation::DenseSearch)?;
        self.require_active_binding(VcePortOperation::DenseSearch, true)
            .await?;
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
        self.require_active_binding(VcePortOperation::PathSearch, true)
            .await?;
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
                    "SELECT blob_name, path, path_document FROM vce_blobs WHERE namespace_id = ? AND blob_name = ? AND path_document IS NOT NULL LIMIT 1;",
                    vec![sqlite_text(&self.binding.namespace_id), sqlite_text(&row.blob_name)],
                )
                .await
                .map_err(|message| port_failed(VcePortOperation::PathSearch, message))?;
            if let Some(metadata) = metadata.first() {
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
        self.require_active_binding(VcePortOperation::ExactSearch, true)
            .await?;
        if request.limit == 0
            || request
                .identifiers
                .iter()
                .any(|value| value.trim().is_empty())
        {
            return Err(port_rejected(
                VcePortOperation::ExactSearch,
                "exact search identifiers and limit must be valid",
            ));
        }
        let mut hits = Vec::new();
        let mut seen = BTreeSet::new();
        for identifier in &request.identifiers {
            let rows: Vec<SymbolHitRow> = self
                .data_plane
                .query_rows(
                    r#"SELECT occurrence_id, identifier, blob_name, path, start_line, end_line,
                              start_char_offset, chunk_id, content, kind
                       FROM vce_symbols
                       WHERE namespace_id = ? AND identifier = ?
                       ORDER BY CASE kind WHEN 'endpoint' THEN 0 WHEN 'definition' THEN 1 ELSE 2 END,
                                path, start_line, occurrence_id
                       LIMIT 5000;"#,
                    vec![sqlite_text(&self.binding.namespace_id), sqlite_text(identifier)],
                )
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
        self.data_plane
            .query_rows(
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
    ) -> Result<String, VcePortError> {
        let namespace = sqlite_string_literal(&self.binding.namespace_id);
        let refresh_id = sqlite_string_literal(&request.refresh_id);
        let checkpoint_json = serde_json::to_string(&request.checkpoint)
            .map_err(|error| port_failed(VcePortOperation::Commit, error.to_string()))?;
        let checkpoint_json = sqlite_string_literal(&checkpoint_json);
        let state_snapshot_json = serde_json::to_string(&request.state_snapshot)
            .map_err(|error| port_failed(VcePortOperation::Commit, error.to_string()))?;
        let state_snapshot_json = sqlite_string_literal(&state_snapshot_json);
        let source_revision = request
            .source_revision
            .as_deref()
            .map(sqlite_string_literal)
            .map(|value| format!("'{value}'"))
            .unwrap_or_else(|| String::from("NULL"));
        let now = current_unix_epoch_millis_i64()
            .map_err(|message| port_failed(VcePortOperation::Commit, message))?;
        let generation = request.generation;
        let version = request.checkpoint.version;
        let operation_id = sqlite_string_literal(&request.request_id);
        let request_fingerprint = sqlite_string_literal(request_fingerprint);
        let cas_condition = request.expected_checkpoint_version.map_or_else(
            || {
                format!(
                    "NOT EXISTS(SELECT 1 FROM vce_checkpoints WHERE namespace_id = '{namespace}')"
                )
            },
            |expected| {
                format!(
                    "EXISTS(SELECT 1 FROM vce_checkpoints WHERE namespace_id = '{namespace}' AND checkpoint_version = {expected})"
                )
            },
        );
        let deleted = sql_string_set(&request.deleted_blobs);
        let delete_clause = if deleted.is_empty() {
            String::new()
        } else {
            format!(
                "DELETE FROM vce_blobs WHERE namespace_id = '{namespace}' AND blob_name IN ({deleted});\n"
            )
        };
        let mut vector_cleanup_clause = String::new();
        if !request.deleted_blobs.is_empty() {
            let blob_names_json = serde_json::to_string(&request.deleted_blobs)
                .map_err(|error| port_failed(VcePortOperation::Commit, error.to_string()))?;
            let cleanup_id = vector_cleanup_id(
                &self.binding.namespace_id,
                "deleted_blobs",
                &format!("{}:{blob_names_json}", request.refresh_id),
            );
            vector_cleanup_clause.push_str(&format!(
                "INSERT INTO vce_vector_cleanup(namespace_id, cleanup_id, cleanup_kind, refresh_id, generation, blob_names_json, keep_generation, physical_fingerprint, content_table, path_table, state, attempt, error, created_at_unix_ms, updated_at_unix_ms) VALUES ('{namespace}', '{}', 'deleted_blobs', '{refresh_id}', {generation}, '{}', NULL, '{}', '{}', '{}', 'pending', 0, NULL, {now}, {now}) ON CONFLICT(namespace_id, cleanup_id) DO UPDATE SET state = 'pending', error = NULL, updated_at_unix_ms = excluded.updated_at_unix_ms;\n",
                sqlite_string_literal(&cleanup_id),
                sqlite_string_literal(&blob_names_json),
                sqlite_string_literal(&self.physical_fingerprint),
                sqlite_string_literal(&self.content_table),
                sqlite_string_literal(&self.path_table),
            ));
        }
        if !request.staged_blob_names.is_empty() {
            let blob_names_json = serde_json::to_string(&request.staged_blob_names)
                .map_err(|error| port_failed(VcePortOperation::Commit, error.to_string()))?;
            let cleanup_id = vector_cleanup_id(
                &self.binding.namespace_id,
                "superseded_blobs",
                &format!("{}:{generation}:{blob_names_json}", request.refresh_id),
            );
            vector_cleanup_clause.push_str(&format!(
                "INSERT INTO vce_vector_cleanup(namespace_id, cleanup_id, cleanup_kind, refresh_id, generation, blob_names_json, keep_generation, physical_fingerprint, content_table, path_table, state, attempt, error, created_at_unix_ms, updated_at_unix_ms) VALUES ('{namespace}', '{}', 'superseded_blobs', '{refresh_id}', {generation}, '{}', {generation}, '{}', '{}', '{}', 'pending', 0, NULL, {now}, {now}) ON CONFLICT(namespace_id, cleanup_id) DO UPDATE SET state = 'pending', error = NULL, updated_at_unix_ms = excluded.updated_at_unix_ms;\n",
                sqlite_string_literal(&cleanup_id),
                sqlite_string_literal(&blob_names_json),
                sqlite_string_literal(&self.physical_fingerprint),
                sqlite_string_literal(&self.content_table),
                sqlite_string_literal(&self.path_table),
            ));
        }
        Ok(format!(
            r#"BEGIN IMMEDIATE;
INSERT INTO vce_commit_guards(namespace_id, valid)
VALUES ('{namespace}', CASE WHEN {cas_condition} THEN 1 ELSE 0 END)
ON CONFLICT(namespace_id) DO UPDATE SET valid = excluded.valid;
{delete_clause}DELETE FROM vce_blobs
WHERE namespace_id = '{namespace}'
  AND path IN (
      SELECT path FROM vce_staged_blobs
      WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation}
  );
INSERT INTO vce_blobs(
    namespace_id, blob_name, path, generation, structure_analyzer_id, structure_mode,
    structure_fallback_reason, path_document, path_status, path_error, symbol_status,
    symbol_error, status, status_reason, chunk_count, vector_count, symbol_count
)
SELECT namespace_id, blob_name, path, generation, structure_analyzer_id, structure_mode,
       structure_fallback_reason, path_document, path_status, path_error, symbol_status,
       symbol_error, status, status_reason, chunk_count, vector_count, symbol_count
FROM vce_staged_blobs
WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
INSERT INTO vce_chunks(
    namespace_id, blob_name, chunk_id, content_hash, path, content,
    start_line, end_line, start_char_offset, kind, vector_ready
)
SELECT namespace_id, blob_name, chunk_id, content_hash, path, content,
       start_line, end_line, start_char_offset, kind, vector_ready
FROM vce_staged_chunks
WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
INSERT INTO vce_symbols(
    namespace_id, blob_name, occurrence_id, identifier, path, start_line,
    end_line, start_char_offset, chunk_id, content, kind
)
SELECT namespace_id, blob_name, occurrence_id, identifier, path, start_line,
       end_line, start_char_offset, chunk_id, content, kind
FROM vce_staged_symbols
WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
INSERT INTO vce_checkpoints(
    namespace_id, checkpoint_version, generation, refresh_id,
    checkpoint_json, source_revision, committed_at_unix_ms
) VALUES (
    '{namespace}', {version}, {generation}, '{refresh_id}',
    '{checkpoint_json}', {source_revision}, {now}
)
ON CONFLICT(namespace_id) DO UPDATE SET
    checkpoint_version = excluded.checkpoint_version,
    generation = excluded.generation,
    refresh_id = excluded.refresh_id,
    checkpoint_json = excluded.checkpoint_json,
    source_revision = excluded.source_revision,
    committed_at_unix_ms = excluded.committed_at_unix_ms;
INSERT INTO vce_state_snapshots(namespace_id, snapshot_json, updated_at_unix_ms)
VALUES ('{namespace}', '{state_snapshot_json}', {now})
ON CONFLICT(namespace_id) DO UPDATE SET
    snapshot_json = excluded.snapshot_json,
    updated_at_unix_ms = excluded.updated_at_unix_ms;
INSERT INTO vce_commit_receipts(namespace_id, operation_id, request_fingerprint, created_at_unix_ms)
VALUES ('{namespace}', '{operation_id}', '{request_fingerprint}', {now});
UPDATE vce_generations SET state = 'committed', operation_id = '{operation_id}', updated_at_unix_ms = {now}
WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
INSERT INTO vce_operations(
    namespace_id, operation_id, kind, phase, generation, completed_units,
    total_units, attempt, coalesced, error, created_at_unix_ms, updated_at_unix_ms
) VALUES (
    '{namespace}', '{operation_id}', 'incremental_refresh', 'completed', {generation},
    {indexed_count}, {indexed_count}, 1, 0, NULL, {now}, {now}
)
ON CONFLICT(namespace_id, operation_id) DO UPDATE SET
    phase = 'completed', generation = excluded.generation,
    completed_units = excluded.completed_units, total_units = excluded.total_units,
    error = NULL, updated_at_unix_ms = excluded.updated_at_unix_ms;
{vector_cleanup_clause}UPDATE vce_project_bindings
SET lifecycle_state = 'ready', updated_at_unix_ms = {now}
WHERE namespace_id = '{namespace}';
DELETE FROM vce_staged_blobs
WHERE namespace_id = '{namespace}' AND refresh_id = '{refresh_id}' AND generation = {generation};
DELETE FROM vce_commit_guards WHERE namespace_id = '{namespace}';
COMMIT;"#,
            indexed_count = request.staged_blob_names.len(),
        ))
    }

    /// Converts every abandoned staging generation into a durable cleanup request.
    /// 将每个遗留暂存代次转换为持久化清理请求。
    async fn enqueue_abandoned_generation_cleanup(&self) -> Result<(), String> {
        let rows: Vec<AbandonedGenerationRow> = self
            .data_plane
            .query_rows(
                "SELECT refresh_id, generation FROM vce_generations WHERE namespace_id = ? AND state IN ('staging', 'aborted') ORDER BY generation, refresh_id;",
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await?;
        for row in rows {
            let generation = u64::try_from(row.generation)
                .map_err(|_| String::from("VCE abandoned generation is negative"))?;
            if generation == 0 || row.refresh_id.trim().is_empty() {
                return Err(String::from(
                    "VCE abandoned generation contains an invalid identity",
                ));
            }
            let cleanup_id = vector_cleanup_id(
                &self.binding.namespace_id,
                "aborted_generation",
                &format!("{}:{generation}", row.refresh_id),
            );
            let now = current_unix_epoch_millis_i64()?;
            let namespace_id = sqlite_string_literal(&self.binding.namespace_id);
            let refresh_id = sqlite_string_literal(&row.refresh_id);
            let cleanup_id_sql = sqlite_string_literal(&cleanup_id);
            let physical_fingerprint = sqlite_string_literal(&self.physical_fingerprint);
            let content_table = sqlite_string_literal(&self.content_table);
            let path_table = sqlite_string_literal(&self.path_table);
            let script = format!(
                r#"BEGIN IMMEDIATE;
                   UPDATE vce_generations SET state = 'aborted',
                       error = COALESCE(error, 'recovered unpublished generation'),
                       updated_at_unix_ms = {now}
                   WHERE namespace_id = '{namespace_id}' AND refresh_id = '{refresh_id}'
                       AND generation = {generation} AND state = 'staging';
                   INSERT INTO vce_vector_cleanup(
                       namespace_id, cleanup_id, cleanup_kind, refresh_id, generation,
                       blob_names_json, keep_generation, physical_fingerprint,
                       content_table, path_table, state, attempt, error,
                       created_at_unix_ms, updated_at_unix_ms
                   ) VALUES ('{namespace_id}', '{cleanup_id_sql}', 'aborted_generation',
                       '{refresh_id}', {generation}, NULL, NULL, '{physical_fingerprint}',
                       '{content_table}', '{path_table}', 'pending', 0, NULL, {now}, {now})
                   ON CONFLICT(namespace_id, cleanup_id) DO NOTHING;
                   DELETE FROM vce_staged_blobs
                   WHERE namespace_id = '{namespace_id}' AND refresh_id = '{refresh_id}'
                       AND generation = {generation};
                   COMMIT;"#,
            );
            self.data_plane
                .execute(&script, Vec::new(), "登记 VCE 遗留向量清理失败")
                .await?;
        }
        Ok(())
    }

    /// Executes pending derived-vector cleanup requests and preserves failures for retry.
    /// 执行待处理派生向量清理请求，并持久保留失败以供重试。
    async fn recover_pending_vector_cleanup(&self) -> Result<(), String> {
        let rows: Vec<VectorCleanupRow> = self
            .data_plane
            .query_rows(
                "SELECT cleanup_id, cleanup_kind, refresh_id, generation, blob_names_json, keep_generation, physical_fingerprint, content_table, path_table FROM vce_vector_cleanup WHERE namespace_id = ? AND state = 'pending' ORDER BY created_at_unix_ms, cleanup_id;",
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await?;
        let mut failures = Vec::new();
        for row in rows {
            let outcome = self.execute_vector_cleanup(&row).await;
            let now = current_unix_epoch_millis_i64()?;
            match outcome {
                Ok(()) => {
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
    async fn execute_vector_cleanup(&self, row: &VectorCleanupRow) -> Result<(), String> {
        if !row.content_table.starts_with("vce_content_")
            || !row.path_table.starts_with("vce_path_")
            || row.physical_fingerprint.trim().is_empty()
        {
            return Err(String::from(
                "VCE cleanup row contains an invalid physical binding",
            ));
        }
        let namespace = lance_string_literal(&self.binding.namespace_id);
        let fingerprint = lance_string_literal(&row.physical_fingerprint);
        let predicate = match row.cleanup_kind.as_str() {
            "aborted_generation" => {
                let refresh_id = row
                    .refresh_id
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| String::from("VCE generation cleanup is missing refresh_id"))?;
                let generation = row
                    .generation
                    .ok_or_else(|| String::from("VCE generation cleanup is missing generation"))?;
                if generation <= 0 {
                    return Err(String::from(
                        "VCE generation cleanup has an invalid generation",
                    ));
                }
                format!(
                    "namespace_id = '{namespace}' AND physical_fingerprint = '{fingerprint}' AND refresh_id = '{}' AND generation = {generation}",
                    lance_string_literal(refresh_id),
                )
            }
            "deleted_blobs" | "superseded_blobs" => {
                let blob_names = row
                    .blob_names_json
                    .as_deref()
                    .ok_or_else(|| String::from("VCE Blob cleanup is missing its Blob set"))?;
                let blob_names = serde_json::from_str::<BTreeSet<String>>(blob_names)
                    .map_err(|error| format!("decode VCE cleanup Blob set: {error}"))?;
                if blob_names.is_empty() || blob_names.iter().any(|value| value.trim().is_empty()) {
                    return Err(String::from(
                        "VCE Blob cleanup contains an invalid Blob set",
                    ));
                }
                let base = format!(
                    "namespace_id = '{namespace}' AND physical_fingerprint = '{fingerprint}' AND blob_name IN ({})",
                    lance_string_set(&blob_names),
                );
                if row.cleanup_kind == "superseded_blobs" {
                    let keep_generation = row.keep_generation.ok_or_else(|| {
                        String::from("VCE superseded cleanup is missing keep_generation")
                    })?;
                    if keep_generation <= 0 {
                        return Err(String::from(
                            "VCE superseded cleanup has an invalid keep_generation",
                        ));
                    }
                    format!("{base} AND generation != {keep_generation}")
                } else {
                    base
                }
            }
            other => return Err(format!("unsupported VCE cleanup kind `{other}`")),
        };
        for table in [&row.content_table, &row.path_table] {
            self.data_plane
                .client
                .delete_lancedb_typed(
                    VCE_SPACE_ID,
                    self.data_plane.lancedb_binding_id.as_str(),
                    table,
                    predicate.clone(),
                )
                .await
                .map_err(|error| format!("delete VCE vectors from `{table}`: {error}"))?;
        }
        Ok(())
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
        let physical_limit = u32::try_from(limit)
            .unwrap_or(VCE_VECTOR_SEARCH_LIMIT)
            .clamp(1, VCE_VECTOR_SEARCH_LIMIT);
        let predicate = format!(
            "namespace_id = '{}' AND vector_space_id = '{}' AND physical_fingerprint = '{}' AND blob_name IN ({})",
            lance_string_literal(&self.binding.namespace_id),
            lance_string_literal(&self.vector_space_id),
            lance_string_literal(&self.physical_fingerprint),
            lance_string_set(allowed_blobs),
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
                || row.generation == 0
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
        let identities = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let sql = format!(
            r#"SELECT chunk_id, blob_name, path, content, content_hash, start_line, end_line,
                      start_char_offset, kind
               FROM vce_chunks WHERE namespace_id = ? AND chunk_id IN ({});"#,
            placeholders(identities.len())
        );
        let mut params = vec![sqlite_text(&self.binding.namespace_id)];
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
                continue;
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
            .data_plane
            .query_rows(
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
        let mut sql = String::from(
            r#"SELECT path, blob_name, generation, path_status, path_error, symbol_status,
                      symbol_error, status_reason, chunk_count, vector_count, symbol_count
               FROM vce_blobs WHERE namespace_id = ?"#,
        );
        let mut params = vec![sqlite_text(&self.binding.namespace_id)];
        if let Some(prefix) = &query.path_prefix {
            sql.push_str(" AND (path = ? OR path LIKE ?)");
            params.push(sqlite_text(prefix));
            params.push(sqlite_text(format!("{prefix}/%")));
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
            .data_plane
            .query_rows(&sql, params)
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
            .data_plane
            .query_rows(
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
            .data_plane
            .query_rows(
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
            .data_plane
            .query_rows(
                r#"SELECT blob_name, path, status, path_status, symbol_status, chunk_count, vector_count
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
                r#"SELECT blob_name FROM vce_staged_blobs
                   WHERE namespace_id = ? ORDER BY blob_name;"#,
                vec![sqlite_text(&self.binding.namespace_id)],
            )
            .await
            .map_err(|message| port_failed(VcePortOperation::ManagementQuery, message))?;
        for row in staged {
            issues.push(integrity_issue(
                VceIntegrityIssueKind::OrphanedStaging,
                None,
                Some(row.blob_name),
                "存在尚未发布的暂存 Blob",
            ));
        }
        let pending_cleanup: Vec<VectorCleanupIssueRow> = self
            .data_plane
            .query_rows(
                "SELECT cleanup_id, error FROM vce_vector_cleanup WHERE namespace_id = ? AND state = 'pending' ORDER BY cleanup_id;",
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
            VceMaintenanceMode::Rebuild => !blob_names.is_empty(),
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
