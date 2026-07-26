//! Knowledge-base lifecycle, import, authorization, and BM25 runtime.
//! 知识库生命周期、导入、授权与 BM25 运行时。

use super::markdown::{
    chunk_markdown, extract_document_title, normalize_markdown_newlines, sha256_hex,
};
use super::models::{
    KNOWLEDGE_PACKAGE_FORMAT, KNOWLEDGE_PACKAGE_VERSION, KnowledgeChunk, KnowledgeCustomWord,
    KnowledgeDocumentContent, KnowledgeDocumentSummary, KnowledgeFtsQuery,
    KnowledgeImportJobSummary, KnowledgeImportReport, KnowledgeIndexIntegritySummary,
    KnowledgeLibrarySummary, KnowledgeOperationKind, KnowledgePackageExportResult,
    KnowledgePackageImportResult, KnowledgePackageInspection, KnowledgePackageManifest,
    KnowledgeProjectBinding, KnowledgeSearchFailure, KnowledgeSearchHit, KnowledgeSearchResult,
    KnowledgeSourceSummary, MAX_KNOWLEDGE_PACKAGE_BYTES,
};
use super::schema::{KNOWLEDGE_FTS_INDEX, KNOWLEDGE_SCHEMA_VERSION, library_schema_script};
use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle};
use crate::runtime_assets;
use futures_util::{StreamExt, stream::FuturesUnordered};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;
use vldb_controller_client::{
    ControllerSqliteEnableRequest, ControllerSqliteTokenizerMode, ControllerSqliteValue, SpaceKind,
    SpaceRegistration,
};
use vulcan_runtime_assets::text_decode::decode_complete_user_text;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

/// Maximum number of Markdown files accepted by one import.
/// 单次导入允许的最大 Markdown 文件数量。
const MAX_IMPORT_FILES: usize = 20_000;
/// Maximum size of one Markdown source file.
/// 单个 Markdown 来源文件的最大字节数。
const MAX_MARKDOWN_FILE_BYTES: u64 = 20 * 1024 * 1024;
/// Maximum cumulative source bytes accepted by one import.
/// 单次导入允许的最大来源总字节数。
const MAX_IMPORT_BYTES: u64 = 1024 * 1024 * 1024;
/// Maximum FTS mutations committed per cancellable controller batch.
/// 每个可取消控制器批次提交的最大 FTS 变更数。
const FTS_MUTATION_BATCH_SIZE: usize = 512;
/// Maximum stale FTS identifiers deleted by one parameterized set statement.
/// 单条参数化集合语句删除的最大过期 FTS 标识符数量。
const FTS_DELETE_PARAMETER_BATCH_SIZE: usize = 900;
/// Maximum number of warnings retained in one import report.
/// 单次导入报告保留的最大告警数量。
const MAX_IMPORT_WARNINGS: usize = 100;
/// Maximum user-visible warning length.
/// 单条用户可见告警的最大长度。
const MAX_WARNING_CHARS: usize = 1_000;
/// Maximum document page size.
/// 文档分页的最大页大小。
const MAX_DOCUMENT_PAGE_SIZE: u32 = 500;
/// Maximum federated search result size.
/// 联合检索结果的最大数量。
const MAX_SEARCH_LIMIT: u32 = 200;
/// Reciprocal-rank-fusion constant used across physical libraries.
/// 跨物理知识库使用的倒数排名融合常量。
const RRF_K: f64 = 60.0;
/// Exact internal marker used to distinguish cancellation from failures.
/// 用于区分取消与失败的精确内部标记。
const KNOWLEDGE_OPERATION_CANCELLED_ERROR: &str = "knowledge operation cancelled by user";
/// Exact portable package metadata entry.
/// 可移植包元数据的精确条目名称。
const KNOWLEDGE_PACKAGE_METADATA_ENTRY: &str = "package.json";
/// Exact portable internal manifest entry.
/// 可移植包内部清单的精确条目名称。
const KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY: &str = "library/manifest.json";
/// Exact portable SQLite entry.
/// 可移植包 SQLite 的精确条目名称。
const KNOWLEDGE_PACKAGE_DATABASE_ENTRY: &str = "library/content.sqlite3";
/// Prefix reserved for host-owned package extraction directories removed during startup recovery.
/// 宿主持有的知识库包解压目录保留前缀，启动恢复期间会清理这些目录。
const KNOWLEDGE_PACKAGE_INSPECTION_TEMP_PREFIX: &str = ".package-inspect-";

/// Version of the crash-recovery journal used by explicit package replacement.
/// 显式知识库包覆盖使用的崩溃恢复日志版本。
const KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION: u32 = 1;
/// Journal phase written before either replacement directory rename.
/// 在任一覆盖目录改名前写入的日志阶段。
const KNOWLEDGE_REPLACEMENT_PHASE_PREPARED: &str = "prepared";
/// Journal phase written after the replacement registry commit.
/// 在覆盖注册表提交后写入的日志阶段。
const KNOWLEDGE_REPLACEMENT_PHASE_COMMITTED: &str = "committed";
/// Maximum accepted JSON metadata entry size.
/// 可接受的 JSON 元数据条目最大字节数。
const MAX_KNOWLEDGE_PACKAGE_JSON_BYTES: u64 = 1024 * 1024;

/// Host-native knowledge runtime sharing the controller connection and application registry.
/// 共享控制器连接与应用注册表的宿主原生知识库运行时。
#[derive(Clone)]
pub struct KnowledgeBaseRuntime {
    /// Long-lived controller data-plane handle.
    /// 长生命周期控制器数据面句柄。
    controller: ControllerRuntimeHandle,
    /// Per-library exclusive mutation locks.
    /// 每知识库独占变更锁。
    mutation_locks: Arc<Mutex<BTreeMap<String, Arc<Mutex<()>>>>>,
    /// Serializes first-time binding activation and protects the active-binding cache.
    /// 串行化首次绑定激活并保护活动绑定缓存。
    binding_activation_lock: Arc<Mutex<()>>,
    /// Library identifiers whose controller SQLite binding is already active.
    /// 控制器 SQLite 绑定已经激活的知识库标识符。
    active_bindings: Arc<Mutex<BTreeSet<String>>>,
    /// Exact active import or index-maintenance operations with cooperative cancellation.
    /// 支持协作取消的精确活动导入或索引维护操作。
    active_operations: Arc<Mutex<BTreeMap<String, ActiveKnowledgeOperation>>>,
}

/// Process-local knowledge runtime state shared by every wrapper over one controller handle.
/// 由同一控制器句柄的所有包装运行时共享的进程内知识库状态。
#[derive(Default)]
pub(crate) struct KnowledgeBaseRuntimeSharedState {
    /// Per-library exclusive mutation locks.
    /// 每知识库独占变更锁。
    mutation_locks: Arc<Mutex<BTreeMap<String, Arc<Mutex<()>>>>>,
    /// Serializes first-time binding activation and protects the active-binding cache.
    /// 串行化首次绑定激活并保护活动绑定缓存。
    binding_activation_lock: Arc<Mutex<()>>,
    /// Library identifiers whose controller SQLite binding is already active.
    /// 控制器 SQLite 绑定已经激活的知识库标识符。
    active_bindings: Arc<Mutex<BTreeSet<String>>>,
    /// Exact active import or index-maintenance operations with cooperative cancellation.
    /// 支持协作取消的精确活动导入或索引维护操作。
    active_operations: Arc<Mutex<BTreeMap<String, ActiveKnowledgeOperation>>>,
}

/// Cancellation identity and signal for one exact active knowledge operation.
/// 一个精确活动资料操作的取消身份与信号。
#[derive(Clone)]
struct ActiveKnowledgeOperation {
    /// Immutable operation job identifier used to reject stale cancellation requests.
    /// 用于拒绝过期取消请求的不可变操作任务标识符。
    job_id: String,
    /// Cooperative cancellation signal observed by bounded import checkpoints.
    /// 由有界导入检查点观察的协作取消信号。
    signal: Arc<AtomicBool>,
}

/// Runtime-owned identity shared by one durable operation row and its detached executor.
/// 由运行时持有、供一条持久化操作记录与其分离执行器共享的身份。
#[derive(Clone)]
struct PreparedKnowledgeOperation {
    /// Immutable operation job identifier.
    /// 不可变操作任务标识符。
    job_id: String,
    /// Immutable target library identifier.
    /// 不可变目标知识库标识符。
    library_id: String,
    /// Stable operation kind persisted before execution starts.
    /// 执行开始前持久化的稳定操作类型。
    operation_kind: KnowledgeOperationKind,
    /// Cooperative cancellation signal shared with the detached executor.
    /// 与分离执行器共享的协作取消信号。
    cancellation: Arc<AtomicBool>,
    /// Creation time reused by import source and authoritative row commits.
    /// 由导入来源与权威记录提交复用的创建时间。
    created_at_ms: i64,
}

/// Signals cancellation only when the requested job is still the exact active operation.
/// 仅当请求任务仍是精确活动操作时发送取消信号。
fn signal_matching_operation_cancellation(
    active: Option<&ActiveKnowledgeOperation>,
    requested_job_id: &str,
) -> bool {
    let Some(active) = active else {
        return false;
    };
    if active.job_id != requested_job_id {
        return false;
    }
    active.signal.store(true, Ordering::Release);
    true
}

/// Portable manifest stored beside each isolated database.
/// 存储在每个隔离数据库旁的可移植清单。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct KnowledgeManifest {
    /// Immutable library identifier.
    /// 不可变知识库标识符。
    library_id: String,
    /// User-visible name copied for diagnostics and exportability.
    /// 为诊断和可导出性复制的用户可见名称。
    name: String,
    /// Per-library schema version.
    /// 单知识库结构版本。
    schema_version: i64,
}

/// Validated package extracted into one host-controlled temporary directory.
/// 已解压到宿主受控临时目录中的已校验知识库包。
struct PreparedKnowledgePackage {
    /// Temporary directory retaining extracted files for the operation lifetime.
    /// 在操作生命周期内保留解压文件的临时目录。
    temporary: tempfile::TempDir,
    /// Validated public package metadata.
    /// 已校验的公开包元数据。
    package: KnowledgePackageManifest,
    /// Validated source internal manifest.
    /// 已校验的来源内部清单。
    source_manifest: KnowledgeManifest,
}

/// Durable state required to recover an interrupted two-rename library replacement.
/// 恢复被中断的两次目录改名覆盖所需的持久化状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeReplacementJournal {
    /// Journal protocol version.
    /// 日志协议版本。
    version: u32,
    /// Exact retained target library identity.
    /// 精确保留的目标知识库身份。
    target_library_id: String,
    /// Exact sibling backup directory name derived by the host.
    /// 由宿主派生的精确同级备份目录名称。
    backup_directory_name: String,
    /// Either prepared before registry commit or committed after registry commit.
    /// 注册表提交前为 prepared，注册表提交后为 committed。
    phase: String,
    /// Complete prior registry row used by prepared-phase rollback.
    /// prepared 阶段回滚使用的完整旧注册记录。
    previous: KnowledgeLibrarySummary,
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

impl PreparedKnowledgePackage {
    /// Returns the exact extracted SQLite path.
    /// 返回精确的已解压 SQLite 路径。
    fn database_path(&self) -> PathBuf {
        self.temporary.path().join("content.sqlite3")
    }
}

/// One registry row decoded from the application database.
/// 从应用数据库解码的一条知识库注册记录。
#[derive(Debug, Deserialize)]
struct LibraryRow {
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

/// One project binding row decoded from the application database.
/// 从应用数据库解码的一条项目绑定记录。
#[derive(Debug, Deserialize)]
struct ProjectBindingRow {
    /// Immutable library identifier.
    /// 不可变知识库标识符。
    library_id: String,
    /// Immutable project identifier.
    /// 不可变项目标识符。
    project_id: String,
    /// SQLite integer boolean for default enablement.
    /// 默认启用状态对应的 SQLite 整数布尔值。
    default_enabled: i64,
}

/// One source row used during incremental import.
/// 增量导入期间使用的一条来源记录。
#[derive(Debug, Deserialize)]
struct SourceRow {
    /// Immutable source identifier.
    /// 不可变来源标识符。
    id: String,
}

/// One source row with checked aggregate document counts for management listing.
/// 用于管理列表且带已检查文档聚合数量的一条来源记录。
#[derive(Debug, Deserialize)]
struct SourceSummaryRow {
    /// Immutable source identifier.
    /// 不可变来源标识符。
    id: String,
    /// Canonical native source root.
    /// 规范原生来源根目录。
    canonical_root: String,
    /// User-visible source directory name.
    /// 用户可见来源目录名称。
    display_name: String,
    /// SQLite aggregate count of authoritative documents.
    /// 权威文档的 SQLite 聚合数量。
    document_count: i64,
    /// SQLite aggregate count of authoritative chunks.
    /// 权威切片的 SQLite 聚合数量。
    chunk_count: i64,
    /// Source identity creation timestamp.
    /// 来源身份创建时间戳。
    created_at_ms: i64,
    /// Most recent source scan timestamp.
    /// 最近一次来源扫描时间戳。
    last_scanned_at_ms: i64,
}

/// Existing document identity and digest used for incremental decisions.
/// 用于增量判断的现有文档身份与摘要。
#[derive(Debug, Clone, Deserialize)]
struct ExistingDocumentRow {
    /// Immutable document identifier.
    /// 不可变文档标识符。
    id: String,
    /// Normalized relative path key.
    /// 规范化相对路径键。
    relative_path_key: String,
    /// SHA-256 source digest.
    /// SHA-256 来源摘要。
    source_sha256: String,
}

/// Existing chunk identity used to delete stale FTS rows.
/// 用于删除过期 FTS 记录的现有切片身份。
#[derive(Debug, Clone, Deserialize)]
struct ExistingChunkRow {
    /// Immutable chunk identifier.
    /// 不可变切片标识符。
    id: String,
    /// Owning document identifier.
    /// 所属文档标识符。
    document_id: String,
}

/// Authoritative joined chunk row used for FTS replay and materialization.
/// 用于 FTS 回放与物化的权威联结切片记录。
#[derive(Debug, Clone, Deserialize)]
struct JoinedChunkRow {
    /// Immutable chunk identifier.
    /// 不可变切片标识符。
    id: String,
    /// Owning document identifier.
    /// 所属文档标识符。
    document_id: String,
    /// Zero-based ordinal.
    /// 从零开始的序号。
    ordinal: i64,
    /// Heading ancestry.
    /// 标题层级。
    heading_path: String,
    /// One-based inclusive first line.
    /// 从一开始且包含边界的起始行。
    start_line: i64,
    /// One-based inclusive last line.
    /// 从一开始且包含边界的结束行。
    end_line: i64,
    /// Authoritative chunk content.
    /// 权威切片内容。
    content: String,
    /// Mixed-language estimated token count.
    /// 中英文混合估算 token 数量。
    estimated_tokens: i64,
    /// Document title.
    /// 文档标题。
    title: String,
    /// Relative document path.
    /// 文档相对路径。
    relative_path: String,
}

/// Lightweight FTS rank row used when the host materializes authoritative content itself.
/// 宿主自行物化权威内容时使用的轻量 FTS 排名记录。
#[derive(Debug, Deserialize)]
struct FtsRankRow {
    /// Immutable chunk identifier stored in the VLDB-managed FTS index.
    /// 存储在 VLDB 管理 FTS 索引中的不可变切片标识符。
    id: String,
    /// Raw ascending SQLite BM25 score.
    /// SQLite 按升序返回的原始 BM25 分值。
    raw_score: f64,
}

/// Document list row including the derived chunk count.
/// 包含派生切片数量的文档列表记录。
#[derive(Debug, Deserialize)]
struct DocumentSummaryRow {
    /// Immutable document identifier.
    /// 不可变文档标识符。
    id: String,
    /// Owning source identifier.
    /// 所属来源标识符。
    source_id: String,
    /// Relative source path.
    /// 相对来源路径。
    relative_path: String,
    /// Folder classification.
    /// 文件夹分类。
    folder_path: String,
    /// Document title.
    /// 文档标题。
    title: String,
    /// Source byte size.
    /// 来源字节大小。
    byte_size: i64,
    /// Source line count.
    /// 来源行数。
    line_count: i64,
    /// Derived chunk count.
    /// 派生切片数量。
    chunk_count: i64,
    /// Last update timestamp.
    /// 最近更新时间戳。
    updated_at_ms: i64,
}

/// Distinct logical folder row used by lazy document navigation.
/// 延迟文档导航使用的唯一逻辑目录记录。
#[derive(Debug, Deserialize)]
struct DocumentFolderRow {
    /// Slash-normalized folder path; an empty value identifies the library root.
    /// 使用斜杠规范化的目录路径；空值表示知识库根目录。
    folder_path: String,
}

/// Full document row used by content reads.
/// 内容读取使用的完整文档记录。
#[derive(Debug, Deserialize)]
struct DocumentContentRow {
    /// Immutable document identifier.
    /// 不可变文档标识符。
    id: String,
    /// Owning source identifier.
    /// 所属来源标识符。
    source_id: String,
    /// Relative source path.
    /// 相对来源路径。
    relative_path: String,
    /// Folder classification.
    /// 文件夹分类。
    folder_path: String,
    /// Document title.
    /// 文档标题。
    title: String,
    /// Authoritative Markdown content.
    /// 权威 Markdown 内容。
    content: String,
    /// Source byte size.
    /// 来源字节大小。
    byte_size: i64,
    /// Source line count.
    /// 来源行数。
    line_count: i64,
    /// Derived chunk count.
    /// 派生切片数量。
    chunk_count: i64,
    /// Last update timestamp.
    /// 最近更新时间戳。
    updated_at_ms: i64,
}

/// Metadata values required by the fail-closed search gate.
/// 搜索失败关闭门禁需要的元数据值。
#[derive(Debug, Deserialize)]
struct IndexGateRow {
    /// Current index state.
    /// 当前索引状态。
    index_status: String,
    /// Current authoritative content revision.
    /// 当前权威内容版本。
    content_revision: String,
    /// Revision represented by FTS.
    /// FTS 表示的内容版本。
    indexed_content_revision: String,
    /// Current dictionary revision.
    /// 当前词典版本。
    dictionary_revision: String,
    /// Dictionary revision represented by FTS.
    /// FTS 表示的词典版本。
    indexed_dictionary_revision: String,
    /// Configured tokenizer identifier.
    /// 配置的分词器标识符。
    tokenizer: String,
}

/// Typed central import-job row.
/// 强类型中央导入任务记录。
#[derive(Debug, Deserialize)]
struct ImportJobRow {
    /// Durable job identifier.
    /// 持久化任务标识符。
    id: String,
    /// Target library identifier.
    /// 目标知识库标识符。
    library_id: String,
    /// Stable persisted import or index-maintenance operation kind.
    /// 稳定持久化的导入或索引维护操作类型。
    operation_kind: String,
    /// Durable state.
    /// 持久化状态。
    state: String,
    /// Current phase.
    /// 当前阶段。
    phase: String,
    /// Discovered file count.
    /// 已发现文件数。
    scanned_files: i64,
    /// Total file count after discovery.
    /// 发现完成后的文件总数。
    total_files: i64,
    /// Files whose parsing work has completed.
    /// 已完成解析工作的文件数。
    processed_files: i64,
    /// Total index work items in the active index phase.
    /// 当前索引阶段的索引工作项总数。
    total_index_items: i64,
    /// Completed index work items in the active index phase.
    /// 当前索引阶段已完成的索引工作项数。
    processed_index_items: i64,
    /// Current relative path reported by the worker.
    /// 工作器报告的当前相对路径。
    current_file: Option<String>,
    /// Processed source bytes.
    /// 已处理来源字节数。
    processed_bytes: i64,
    /// JSON warnings payload.
    /// JSON 告警载荷。
    warnings_json: String,
    /// Optional terminal error.
    /// 可选终态错误。
    error_message: Option<String>,
}

/// Whether the derived FTS virtual table exists in one isolated library database.
/// 派生 FTS 虚拟表是否存在于一个隔离知识库数据库中。
#[derive(Debug, Deserialize)]
struct IndexTablePresenceRow {
    /// SQLite integer boolean indicating exact table-name presence.
    /// 表示精确表名是否存在的 SQLite 整数布尔值。
    index_exists: i64,
}

/// Exact set and source-field diagnostics for one existing FTS table.
/// 一个已存在 FTS 表的精确集合与源字段诊断。
#[derive(Debug, Deserialize)]
struct IndexIntegrityCountsRow {
    /// Number of authoritative Markdown chunks.
    /// 权威 Markdown 切片数量。
    authoritative_count: i64,
    /// Number of physical FTS rows.
    /// 物理 FTS 记录数量。
    indexed_count: i64,
    /// Authoritative chunks without one exact matching FTS source row.
    /// 没有精确匹配 FTS 源记录的权威切片数量。
    missing_or_mismatched_count: i64,
    /// FTS rows without one exact matching authoritative chunk.
    /// 没有精确匹配权威切片的 FTS 记录数量。
    orphan_or_mismatched_count: i64,
}

/// Aggregate authoritative counts for registry projection.
/// 用于注册表投影的权威聚合计数。
#[derive(Debug, Deserialize)]
struct CountRow {
    /// Document count.
    /// 文档数量。
    document_count: i64,
    /// Chunk count.
    /// 切片数量。
    chunk_count: i64,
}

/// One checked scalar chunk count returned by a bounded aggregate query.
/// 一个由有界聚合查询返回的已检查切片标量计数。
#[derive(Debug, Deserialize)]
struct ChunkCountRow {
    /// Aggregate chunk count.
    /// 聚合切片数量。
    chunk_count: i64,
}

/// One scanned Markdown candidate before staging.
/// 暂存前的一份已扫描 Markdown 候选文件。
#[derive(Debug)]
struct ScannedMarkdownFile {
    /// Slash-normalized relative path.
    /// 以斜杠规范化的相对路径。
    relative_path: String,
    /// Case-normalized relative path key.
    /// 大小写规范化的相对路径键。
    relative_path_key: String,
    /// Folder classification derived from the relative path.
    /// 从相对路径派生的文件夹分类。
    folder_path: String,
    /// Canonical absolute source path.
    /// 规范绝对来源路径。
    /// Source path reached only through the validated, non-reparse directory traversal.
    /// 仅通过已验证且无重解析点的目录遍历到达的来源路径。
    source_path: PathBuf,
    /// Source byte size.
    /// 来源字节大小。
    byte_size: u64,
    /// Optional source modification time.
    /// 可选来源修改时间。
    modified_at_ms: Option<i64>,
}

/// ScanProgressSnapshot carries bounded filesystem discovery progress back to the async runtime.
/// ScanProgressSnapshot 将有界文件系统发现进度传回异步运行时。
#[derive(Debug)]
struct ScanProgressSnapshot {
    /// Number of Markdown files discovered so far.
    /// 当前已发现的 Markdown 文件数。
    discovered_files: u64,
    /// Current relative directory or file path visited by the scanner.
    /// 扫描器当前访问的相对目录或文件路径。
    current_path: String,
}

/// ImportIndexSyncOutcome distinguishes a complete incremental sync from an accepted cancellation.
/// ImportIndexSyncOutcome 区分完成的增量同步与已接受的取消请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportIndexSyncOutcome {
    /// Every affected FTS row and the revision gate were synchronized.
    /// 所有受影响 FTS 行与版本门均已同步。
    Completed,
    /// Synchronization stopped at a safe row boundary and requires an authoritative rebuild.
    /// 同步在安全行边界停止并需要一次权威重建。
    Cancelled,
}

impl KnowledgeBaseRuntime {
    /// Creates a knowledge runtime from the established application data plane.
    /// 从已建立的应用数据面创建知识库运行时。
    pub fn new(controller: ControllerRuntimeHandle) -> Self {
        // sharedState ensures independently reconstructed AI and WebUI runtimes coordinate the same bindings and operations.
        // sharedState 确保独立重建的 AI 与 WebUI 运行时协调同一组绑定和操作。
        let shared_state = controller.knowledge_runtime_state.clone();
        Self {
            controller,
            mutation_locks: shared_state.mutation_locks.clone(),
            binding_activation_lock: shared_state.binding_activation_lock.clone(),
            active_bindings: shared_state.active_bindings.clone(),
            active_operations: shared_state.active_operations.clone(),
        }
    }

    /// Marks abandoned queued or running knowledge operations as interrupted during service startup.
    /// 服务启动时将遗留的排队或运行中知识库操作标记为已中断。
    pub async fn recover_interrupted_jobs(&self) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_import_jobs SET state = 'interrupted', phase = 'interrupted', error_message = 'service restarted before knowledge operation completion', finished_at_ms = ? WHERE state IN ('queued', 'running');",
                vec![int(unix_epoch_millis()?)],
            )
            .await
            .map_err(|error| format!("恢复知识库操作状态失败：{error}"))?;
        self.recover_interrupted_replacements().await?;
        self.cleanup_orphan_package_directories().await?;
        Ok(())
    }

    /// Recovers every durable replacement journal before the knowledge runtime becomes available.
    /// 在知识库运行时可用前恢复每一份持久化覆盖日志。
    async fn recover_interrupted_replacements(&self) -> Result<(), String> {
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
    async fn cleanup_orphan_package_directories(&self) -> Result<(), String> {
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

    /// Requests cancellation only when both the library and active operation identities match.
    /// 仅在知识库与活动操作身份都匹配时请求取消。
    ///
    /// Parameters: `library_id` selects the exact library and `job_id` rejects stale requests.
    /// Returns: whether the matching live operation accepted the cancellation signal.
    /// 参数：`library_id` 选择精确知识库，`job_id` 用于拒绝过期请求。返回值：匹配活动操作是否接受取消信号。
    pub async fn cancel_operation(&self, library_id: &str, job_id: &str) -> Result<bool, String> {
        validate_library_id(library_id)?;
        Uuid::parse_str(job_id).map_err(|error| format!("知识库操作任务标识符无效：{error}"))?;
        // Operations contains the only runtime-owned cancellation identity for each library.
        // Operations 包含每个知识库唯一由运行时持有的取消身份。
        let operations = self.active_operations.lock().await;
        Ok(signal_matching_operation_cancellation(
            operations.get(library_id),
            job_id,
        ))
    }

    /// Preserves the original import-only cancellation API for internal compatibility.
    /// 为内部兼容保留原有仅导入取消 API。
    ///
    /// Parameters and return value are identical to `cancel_operation`.
    /// 参数与返回值和 `cancel_operation` 完全一致。
    pub async fn cancel_import(&self, library_id: &str, job_id: &str) -> Result<bool, String> {
        self.cancel_operation(library_id, job_id).await
    }

    /// Returns the latest durable import job for one library.
    /// 返回一个知识库最近的持久化导入任务。
    pub async fn latest_import_job(
        &self,
        library_id: &str,
    ) -> Result<Option<KnowledgeImportJobSummary>, String> {
        validate_library_id(library_id)?;
        let rows: Vec<ImportJobRow> = self
            .query_app_rows(
                "SELECT id, library_id, operation_kind, state, phase, scanned_files, total_files, processed_files, total_index_items, processed_index_items, current_file, processed_bytes, warnings_json, error_message FROM knowledge_import_jobs WHERE library_id = ? ORDER BY created_at_ms DESC LIMIT 1;",
                vec![text(library_id)],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(|row| {
                Ok(KnowledgeImportJobSummary {
                    id: row.id,
                    library_id: row.library_id,
                    operation_kind: KnowledgeOperationKind::parse(&row.operation_kind)?,
                    state: row.state,
                    phase: row.phase,
                    scanned_files: nonnegative_u64(row.scanned_files, "scanned_files")?,
                    total_files: nonnegative_u64(row.total_files, "total_files")?,
                    processed_files: nonnegative_u64(row.processed_files, "processed_files")?,
                    total_index_items: nonnegative_u64(row.total_index_items, "total_index_items")?,
                    processed_index_items: nonnegative_u64(
                        row.processed_index_items,
                        "processed_index_items",
                    )?,
                    current_file: row.current_file,
                    processed_bytes: nonnegative_u64(row.processed_bytes, "processed_bytes")?,
                    warnings: serde_json::from_str(&row.warnings_json)
                        .map_err(|error| format!("解析知识库导入告警失败：{error}"))?,
                    error_message: row.error_message,
                })
            })
            .transpose()
    }

    /// Creates one isolated knowledge library and optional initial project bindings.
    /// 创建一个隔离知识库以及可选的初始项目绑定。
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
        self.controller
            .client
            .execute_sqlite_batch_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                r#"INSERT INTO knowledge_libraries(
                    id, name, name_key, description, lifecycle_state, document_count,
                    chunk_count, index_status, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, 'active', 0, 0, 'empty', ?, ?);"#,
                vec![vec![
                    text(&library_id),
                    text(&normalized_name),
                    text(&name_key),
                    text(description.trim()),
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
            let _ = self
                .controller
                .client
                .execute_sqlite_script_typed(
                    APP_SPACE_ID,
                    self.controller.sqlite_binding_id.as_str(),
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

    /// Renames one active library without moving its physical directory or rebuilding FTS.
    /// 重命名一个活动知识库，但不移动物理目录或重建 FTS。
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
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_libraries SET name = ?, name_key = ?, description = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    text(&normalized_name),
                    text(library_name_key(&normalized_name)),
                    text(description.trim()),
                    int(now),
                    text(library_id),
                ],
            )
            .await
            .map_err(|error| format!("重命名知识库失败：{error}"))?;

        let manifest = KnowledgeManifest {
            library_id: library_id.to_string(),
            name: normalized_name,
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
        };
        if let Err(error) = self.write_manifest_atomic(&manifest) {
            let _ = self
                .controller
                .client
                .execute_sqlite_script_typed(
                    APP_SPACE_ID,
                    self.controller.sqlite_binding_id.as_str(),
                    "UPDATE knowledge_libraries SET name = ?, name_key = ?, description = ?, updated_at_ms = ? WHERE id = ?;",
                    vec![
                        text(&current.name),
                        text(library_name_key(&current.name)),
                        text(&current.description),
                        int(current.updated_at_ms),
                        text(library_id),
                    ],
                )
                .await;
            return Err(error);
        }
        self.get_library(library_id).await
    }

    /// Exports one complete ready library into a portable .vkb archive.
    /// 将一个完整且就绪的知识库导出为可移植 .vkb 归档。
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
        let destination_directory = canonical_existing_directory(destination_directory)?;
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
        self.controller
            .client
            .execute_sqlite_script_typed(
                &space_id,
                &binding_id,
                "PRAGMA wal_checkpoint(TRUNCATE);",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("导出前收口知识库 SQLite 日志失败：{error}"))?;
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
        let (space_id, binding_id) = self.ensure_library_binding(target_library_id).await?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                &space_id,
                &binding_id,
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

    /// Lists all libraries or only libraries authorized for one project.
    /// 列出全部知识库，或仅列出授权给某个项目的知识库。
    pub async fn list_libraries(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<KnowledgeLibrarySummary>, String> {
        let (sql, params) = if let Some(project_id) = project_id {
            (
                r#"SELECT l.id, l.name, l.description, l.lifecycle_state, l.document_count,
                           l.chunk_count, l.index_status, l.last_completed_import_at_ms,
                           l.created_at_ms, l.updated_at_ms
                    FROM knowledge_libraries l
                    INNER JOIN knowledge_library_project_bindings b ON b.library_id = l.id
                    WHERE b.project_id = ? AND l.lifecycle_state = 'active'
                    ORDER BY l.name_key ASC;"#,
                vec![text(project_id)],
            )
        } else {
            (
                r#"SELECT id, name, description, lifecycle_state, document_count, chunk_count,
                           index_status, last_completed_import_at_ms, created_at_ms, updated_at_ms
                    FROM knowledge_libraries ORDER BY name_key ASC;"#,
                Vec::new(),
            )
        };
        let rows: Vec<LibraryRow> = self.query_app_rows(sql, params).await?;
        rows.into_iter().map(library_summary_from_row).collect()
    }

    /// Returns one library by immutable identifier.
    /// 根据不可变标识符返回一个知识库。
    pub async fn get_library(&self, library_id: &str) -> Result<KnowledgeLibrarySummary, String> {
        validate_library_id(library_id)?;
        let rows: Vec<LibraryRow> = self
            .query_app_rows(
                r#"SELECT id, name, description, lifecycle_state, document_count, chunk_count,
                           index_status, last_completed_import_at_ms, created_at_ms, updated_at_ms
                    FROM knowledge_libraries WHERE id = ?;"#,
                vec![text(library_id)],
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| format!("知识库不存在：{library_id}"))
            .and_then(library_summary_from_row)
    }

    /// Creates, updates, or removes one explicit project authorization binding.
    /// 创建、更新或移除一条显式项目授权绑定。
    pub async fn set_project_binding(
        &self,
        library_id: &str,
        project_id: &str,
        authorized: bool,
        default_enabled: bool,
    ) -> Result<(), String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        self.require_active_library(library_id).await?;
        let now = unix_epoch_millis()?;
        if authorized {
            self.controller
                .client
                .execute_sqlite_script_typed(
                    APP_SPACE_ID,
                    self.controller.sqlite_binding_id.as_str(),
                    r#"INSERT INTO knowledge_library_project_bindings(
                            library_id, project_id, default_enabled, created_at_ms, updated_at_ms
                        ) VALUES (?, ?, ?, ?, ?)
                        ON CONFLICT(library_id, project_id) DO UPDATE SET
                            default_enabled = excluded.default_enabled,
                            updated_at_ms = excluded.updated_at_ms;"#,
                    vec![
                        text(library_id),
                        text(project_id),
                        bool_value(default_enabled),
                        int(now),
                        int(now),
                    ],
                )
                .await
                .map_err(|error| format!("保存知识库项目授权失败：{error}"))?;
        } else {
            self.controller
                .client
                .execute_sqlite_script_typed(
                    APP_SPACE_ID,
                    self.controller.sqlite_binding_id.as_str(),
                    "DELETE FROM knowledge_library_project_bindings WHERE library_id = ? AND project_id = ?;",
                    vec![text(library_id), text(project_id)],
                )
                .await
                .map_err(|error| format!("移除知识库项目授权失败：{error}"))?;
        }
        Ok(())
    }

    /// Lists explicit project bindings for one active library.
    /// 列出一个活动知识库的显式项目绑定。
    pub async fn list_project_bindings(
        &self,
        library_id: &str,
    ) -> Result<Vec<KnowledgeProjectBinding>, String> {
        self.require_active_library(library_id).await?;
        let rows: Vec<ProjectBindingRow> = self
            .query_app_rows(
                "SELECT library_id, project_id, default_enabled FROM knowledge_library_project_bindings WHERE library_id = ? ORDER BY project_id;",
                vec![text(library_id)],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| KnowledgeProjectBinding {
                library_id: row.library_id,
                project_id: row.project_id,
                default_enabled: row.default_enabled != 0,
            })
            .collect())
    }

    /// Lists every active-library authorization row owned by one exact project.
    /// 列出一个精确项目拥有的全部活动知识库授权记录。
    pub async fn list_bindings_for_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<KnowledgeProjectBinding>, String> {
        let rows: Vec<ProjectBindingRow> = self
            .query_app_rows(
                r#"SELECT b.library_id, b.project_id, b.default_enabled
                   FROM knowledge_library_project_bindings b
                   INNER JOIN knowledge_libraries l ON l.id = b.library_id
                   WHERE b.project_id = ? AND l.lifecycle_state = 'active'
                   ORDER BY l.name_key ASC;"#,
                vec![text(project_id)],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| KnowledgeProjectBinding {
                library_id: row.library_id,
                project_id: row.project_id,
                default_enabled: row.default_enabled != 0,
            })
            .collect())
    }

    /// Resolves the authorized library set for one project, optionally limiting it to defaults.
    /// 解析一个项目的授权知识库集合，并可选择仅保留默认启用项。
    pub async fn authorized_library_ids(
        &self,
        project_id: &str,
        defaults_only: bool,
    ) -> Result<Vec<String>, String> {
        let predicate = if defaults_only {
            " AND b.default_enabled = 1"
        } else {
            ""
        };
        let sql = format!(
            "SELECT b.library_id, b.project_id, b.default_enabled FROM knowledge_library_project_bindings b INNER JOIN knowledge_libraries l ON l.id = b.library_id WHERE b.project_id = ? AND l.lifecycle_state = 'active'{predicate} ORDER BY b.library_id;"
        );
        let rows: Vec<ProjectBindingRow> =
            self.query_app_rows(&sql, vec![text(project_id)]).await?;
        Ok(rows.into_iter().map(|row| row.library_id).collect())
    }

    /// Lists custom Jieba words stored in one active library database.
    /// 列出一个活动知识库数据库中存储的 Jieba 自定义词。
    pub async fn list_custom_words(
        &self,
        library_id: &str,
    ) -> Result<Vec<KnowledgeCustomWord>, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let result = self
            .controller
            .client
            .list_sqlite_custom_words(&space_id, &binding_id)
            .await
            .map_err(|error| format!("读取知识库 Jieba 自定义词失败：{error}"))?;
        result
            .words
            .into_iter()
            .map(|entry| {
                Ok(KnowledgeCustomWord {
                    word: entry.word,
                    weight: u32::try_from(entry.weight)
                        .map_err(|_| String::from("Jieba 自定义词权重超出 u32 范围。"))?,
                })
            })
            .collect()
    }

    /// Inserts or updates one custom Jieba word and marks the derived index dirty.
    /// 新增或更新一个 Jieba 自定义词并将派生索引标记为脏。
    pub async fn upsert_custom_word(
        &self,
        library_id: &str,
        word: &str,
        weight: u32,
    ) -> Result<(), String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        self.require_active_library(library_id).await?;
        let word = validate_custom_word(word)?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        self.controller
            .client
            .upsert_sqlite_custom_word(&space_id, &binding_id, word, weight.max(1))
            .await
            .map_err(|error| format!("保存知识库 Jieba 自定义词失败：{error}"))?;
        self.mark_dictionary_dirty(library_id, &space_id, &binding_id)
            .await
    }

    /// Removes one custom Jieba word and marks the derived index dirty.
    /// 删除一个 Jieba 自定义词并将派生索引标记为脏。
    pub async fn remove_custom_word(&self, library_id: &str, word: &str) -> Result<(), String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        self.require_active_library(library_id).await?;
        let word = validate_custom_word(word)?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        self.controller
            .client
            .remove_sqlite_custom_word(&space_id, &binding_id, word)
            .await
            .map_err(|error| format!("删除知识库 Jieba 自定义词失败：{error}"))?;
        self.mark_dictionary_dirty(library_id, &space_id, &binding_id)
            .await
    }

    /// Recursively imports one canonical directory, accepting only Markdown files.
    /// 递归导入一个规范目录，并且只接受 Markdown 文件。
    pub async fn import_markdown_directory(
        &self,
        library_id: &str,
        source_root: &Path,
    ) -> Result<KnowledgeImportReport, String> {
        self.import_markdown_directory_for_source(library_id, None, source_root)
            .await
    }

    /// Imports or synchronizes one directory, optionally remapping an existing durable source identity.
    /// 导入或同步一个目录，并可选择重新映射一个现有持久化来源身份。
    ///
    /// Parameters: `library_id` selects the target library, `source_id` retains an existing source
    /// identity when present, and `source_root` is the new canonical directory to scan.
    /// Returns: the complete incremental import report after authoritative content and FTS converge.
    /// 参数：`library_id` 选择目标知识库，`source_id` 在存在时保留现有来源身份，`source_root`
    /// 是要扫描的新规范目录。返回值：权威内容与 FTS 收敛后的完整增量导入报告。
    pub async fn import_markdown_directory_for_source(
        &self,
        library_id: &str,
        source_id: Option<&str>,
        source_root: &Path,
    ) -> Result<KnowledgeImportReport, String> {
        // CanonicalRoot is validated before a durable operation row becomes visible.
        // CanonicalRoot 在持久化操作记录可见之前完成校验。
        let canonical_root = canonical_import_root(source_root)?;
        self.require_active_library(library_id).await?;
        // Operation reserves the library before the synchronous execution begins.
        // Operation 在同步执行开始前预留该知识库。
        let operation = self
            .prepare_operation(
                library_id,
                KnowledgeOperationKind::Import,
                Some(&canonical_root),
            )
            .await?;
        self.execute_prepared_import(operation, canonical_root, source_id.map(str::to_string))
            .await
    }

    /// Starts one detached Markdown import whose lifetime is independent from the browser request.
    /// 启动一个生命周期独立于浏览器请求的分离式 Markdown 导入。
    ///
    /// Parameters: `library_id` selects the target library, `source_id` optionally retains a durable
    /// source identity, and `source_root` is the native directory to canonicalize and scan.
    /// Returns: the durable queued operation snapshot immediately after the executor is spawned.
    /// 参数：`library_id` 选择目标知识库，`source_id` 可选择保留持久来源身份，`source_root` 是要规范化
    /// 并扫描的原生目录。返回值：执行器启动后立即返回的持久化排队操作快照。
    pub async fn start_import_markdown_directory_for_source(
        &self,
        library_id: &str,
        source_id: Option<&str>,
        source_root: &Path,
    ) -> Result<KnowledgeImportJobSummary, String> {
        // CanonicalRoot prevents an invalid or reparse-backed path from creating a ghost job.
        // CanonicalRoot 防止无效路径或重解析路径创建幽灵任务。
        let canonical_root = canonical_import_root(source_root)?;
        self.require_active_library(library_id).await?;
        // Operation is persisted and registered before the HTTP caller receives success.
        // Operation 在 HTTP 调用方收到成功前完成持久化与注册。
        let operation = self
            .prepare_operation(
                library_id,
                KnowledgeOperationKind::Import,
                Some(&canonical_root),
            )
            .await?;
        // Snapshot is immutable queued state and does not race with the detached executor's first update.
        // Snapshot 是不可变排队状态，不会与分离执行器的首次更新产生竞态。
        let snapshot = queued_operation_summary(&operation);
        // Runtime and owned inputs keep every dependency alive after the HTTP request future is dropped.
        // Runtime 与自有输入在 HTTP 请求 future 被丢弃后继续保持全部依赖存活。
        let runtime = self.clone();
        // SourceId is owned because the detached executor cannot borrow request-local memory.
        // SourceId 使用自有值，因为分离执行器不能借用请求局部内存。
        let source_id = source_id.map(str::to_string);
        tokio::spawn(async move {
            if let Err(error) = runtime
                .execute_prepared_import(operation, canonical_root, source_id)
                .await
            {
                eprintln!("[knowledge-base] detached import failed: {error}");
            }
        });
        Ok(snapshot)
    }

    /// Executes one previously reserved import and always persists a terminal job state.
    /// 执行一个已预留导入，并始终持久化任务终态。
    ///
    /// Parameters: `operation` owns the durable identity and cancellation signal, `canonical_root`
    /// is the validated directory, and `source_id` optionally remaps an existing source.
    /// Returns: the complete import report or the exact terminal failure.
    /// 参数：`operation` 持有持久身份与取消信号，`canonical_root` 是已校验目录，`source_id`
    /// 可选择重新映射现有来源。返回值：完整导入报告或精确终态失败。
    async fn execute_prepared_import(
        &self,
        operation: PreparedKnowledgeOperation,
        canonical_root: PathBuf,
        source_id: Option<String>,
    ) -> Result<KnowledgeImportReport, String> {
        // Result includes lock acquisition and binding activation so every failure reaches finalization.
        // Result 包含锁获取与绑定激活，使每个失败都进入终态处理。
        let result = async {
            // Guard serializes all authoritative and derived mutations for this library.
            // Guard 串行化该知识库的全部权威与派生变更。
            let _guard = self.library_mutation_guard(&operation.library_id).await?;
            require_operation_not_cancelled(&operation.cancellation)?;
            self.require_active_library(&operation.library_id).await?;
            // BindingIdentity is the only controller route for the isolated library database.
            // BindingIdentity 是隔离知识库数据库的唯一控制器路由。
            let (space_id, binding_id) = self.ensure_library_binding(&operation.library_id).await?;
            self.update_operation_phase(&operation.job_id, "running", "scanning")
                .await?;
            self.import_markdown_directory_inner(
                &operation.library_id,
                &space_id,
                &binding_id,
                &operation.job_id,
                &canonical_root,
                source_id.as_deref(),
                &operation.cancellation,
                operation.created_at_ms,
            )
            .await
        }
        .await;

        // FinishResult is persisted before the in-memory reservation is released.
        // FinishResult 在释放内存预留之前完成持久化。
        let finish_result = match &result {
            Ok(report) => self.finish_import_job(report, None).await,
            Err(error) if error == KNOWLEDGE_OPERATION_CANCELLED_ERROR => {
                self.finish_cancelled_operation_job(&operation.job_id).await
            }
            Err(error) => {
                self.finish_failed_operation_job(&operation.job_id, error)
                    .await
            }
        };
        self.release_active_operation(&operation.library_id, &operation.job_id)
            .await;
        finish_result?;
        result
    }

    /// Lists every durable source directory attached to one active library.
    /// 列出关联到一个活动知识库的全部持久化来源目录。
    ///
    /// Parameters: `library_id` is the immutable target library identifier.
    /// Returns: source summaries ordered by creation time and stable identity.
    /// 参数：`library_id` 是不可变目标知识库标识符。返回值：按创建时间与稳定身份排序的来源摘要。
    pub async fn list_sources(
        &self,
        library_id: &str,
    ) -> Result<Vec<KnowledgeSourceSummary>, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let rows: Vec<SourceSummaryRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                r#"SELECT s.id, s.canonical_root, s.display_name,
                          COUNT(DISTINCT d.id) AS document_count,
                          COUNT(c.id) AS chunk_count,
                          s.created_at_ms, s.last_scanned_at_ms
                   FROM knowledge_sources s
                   LEFT JOIN knowledge_documents d ON d.source_id = s.id
                   LEFT JOIN knowledge_chunks c ON c.document_id = d.id
                   GROUP BY s.id, s.canonical_root, s.display_name, s.created_at_ms, s.last_scanned_at_ms
                   ORDER BY s.created_at_ms ASC, s.id ASC;"#,
                Vec::new(),
            )
            .await?;
        rows.into_iter().map(source_summary_from_row).collect()
    }

    /// Lists documents from one active library with bounded pagination.
    /// 使用有界分页列出一个活动知识库中的文档。
    pub async fn list_documents(
        &self,
        library_id: &str,
        folder_path: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<KnowledgeDocumentSummary>, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let limit = limit.clamp(1, MAX_DOCUMENT_PAGE_SIZE);
        let (predicate, params) = if let Some(folder_path) = folder_path {
            (
                "WHERE d.folder_path = ?",
                vec![
                    text(folder_path),
                    int(i64::from(limit)),
                    int(i64::from(offset)),
                ],
            )
        } else {
            ("", vec![int(i64::from(limit)), int(i64::from(offset))])
        };
        let sql = format!(
            r#"SELECT d.id, d.source_id, d.relative_path, d.folder_path, d.title,
                       d.byte_size, d.line_count, COUNT(c.id) AS chunk_count, d.updated_at_ms
                FROM knowledge_documents d
                LEFT JOIN knowledge_chunks c ON c.document_id = d.id
                {predicate}
                GROUP BY d.id
                ORDER BY d.relative_path_key ASC
                LIMIT ? OFFSET ?;"#
        );
        let rows: Vec<DocumentSummaryRow> = self
            .query_library_rows(&space_id, &binding_id, &sql, params)
            .await?;
        rows.into_iter().map(document_summary_from_row).collect()
    }

    /// Lists only distinct logical folder paths without reading document rows or bodies.
    /// 仅列出唯一逻辑目录路径，不读取文档记录或正文。
    pub async fn list_document_folders(&self, library_id: &str) -> Result<Vec<String>, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let rows: Vec<DocumentFolderRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                "SELECT DISTINCT folder_path FROM knowledge_documents ORDER BY folder_path ASC;",
                Vec::new(),
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.folder_path).collect())
    }

    /// Reads one authoritative document by ID without exposing physical source paths.
    /// 按标识符读取一份权威文档，且不暴露物理来源路径。
    pub async fn read_document(
        &self,
        library_id: &str,
        document_id: &str,
    ) -> Result<KnowledgeDocumentContent, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let rows: Vec<DocumentContentRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                r#"SELECT d.id, d.source_id, d.relative_path, d.folder_path, d.title, d.content,
                           d.byte_size, d.line_count, COUNT(c.id) AS chunk_count, d.updated_at_ms
                    FROM knowledge_documents d
                    LEFT JOIN knowledge_chunks c ON c.document_id = d.id
                    WHERE d.id = ? GROUP BY d.id;"#,
                vec![text(document_id)],
            )
            .await?;
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| format!("资料文档不存在：{document_id}"))?;
        let document = KnowledgeDocumentSummary {
            id: row.id,
            source_id: row.source_id,
            relative_path: row.relative_path,
            folder_path: row.folder_path,
            title: row.title,
            byte_size: nonnegative_u64(row.byte_size, "byte_size")?,
            line_count: nonnegative_u64(row.line_count, "line_count")?,
            chunk_count: nonnegative_u64(row.chunk_count, "chunk_count")?,
            updated_at_ms: row.updated_at_ms,
        };
        Ok(KnowledgeDocumentContent {
            document,
            content: row.content,
        })
    }

    /// Reads one authoritative chunk by ID.
    /// 按标识符读取一个权威切片。
    pub async fn read_chunk(
        &self,
        library_id: &str,
        chunk_id: &str,
    ) -> Result<KnowledgeChunk, String> {
        self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        let row = self
            .load_joined_chunk(&space_id, &binding_id, chunk_id)
            .await?;
        chunk_from_joined_row(&row)
    }

    /// Searches one or more authorized physical libraries using Jieba FTS5 and BM25.
    /// 使用 Jieba FTS5 与 BM25 检索一个或多个已授权物理知识库。
    pub async fn search(
        &self,
        library_ids: &[String],
        query: &str,
        limit: u32,
    ) -> Result<KnowledgeSearchResult, String> {
        let query = query.trim();
        if query.is_empty() {
            return Err(String::from("知识库搜索词不能为空。"));
        }
        let limit = limit.clamp(1, MAX_SEARCH_LIMIT);
        let mut failures = Vec::new();
        let mut per_library_hits = Vec::new();
        let mut fts_queries = Vec::new();
        let unique_ids: BTreeSet<String> = library_ids.iter().cloned().collect();

        let mut searches = FuturesUnordered::new();
        for library_id in unique_ids {
            searches.push(async move {
                let result = self.search_one_library(&library_id, query, limit).await;
                (library_id, result)
            });
        }
        while let Some((library_id, result)) = searches.next().await {
            match result {
                Ok((hits, expression)) => {
                    per_library_hits.push(hits);
                    fts_queries.push(KnowledgeFtsQuery {
                        library_id,
                        expression,
                    });
                }
                Err(error) => failures.push(KnowledgeSearchFailure { library_id, error }),
            }
        }
        failures.sort_by(|left, right| left.library_id.cmp(&right.library_id));
        fts_queries.sort_by(|left, right| left.library_id.cmp(&right.library_id));

        let mut hits = Vec::new();
        let federated = per_library_hits.len() > 1;
        for library_hits in per_library_hits {
            for (rank, mut hit) in library_hits.into_iter().enumerate() {
                if federated {
                    hit.rrf_score = Some(1.0 / (RRF_K + rank as f64 + 1.0));
                }
                hits.push(hit);
            }
        }
        if federated {
            hits.sort_by(|left, right| {
                right
                    .rrf_score
                    .partial_cmp(&left.rrf_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.library_id.cmp(&right.library_id))
                    .then_with(|| left.chunk_id.cmp(&right.chunk_id))
            });
            hits.truncate(limit as usize);
        }
        Ok(KnowledgeSearchResult {
            query: query.to_string(),
            hits,
            fts_queries,
            failures,
        })
    }

    /// Rebuilds the complete Jieba FTS index from authoritative chunk rows.
    /// 从权威切片记录完整重建 Jieba FTS 索引。
    pub async fn rebuild_index(&self, library_id: &str) -> Result<u64, String> {
        self.require_active_library(library_id).await?;
        // Operation reserves the library before synchronous maintenance starts.
        // Operation 在同步维护开始前预留该知识库。
        let operation = self
            .prepare_operation(library_id, KnowledgeOperationKind::Rebuild, None)
            .await?;
        self.execute_prepared_index_operation(operation).await
    }

    /// Starts a detached destructive FTS rebuild and immediately returns its durable job snapshot.
    /// 启动一个分离式破坏性 FTS 重建，并立即返回其持久化任务快照。
    ///
    /// Parameters: `library_id` selects the exact active library.
    /// Returns: the queued rebuild snapshot after its executor has been spawned.
    /// 参数：`library_id` 选择精确活动知识库。返回值：执行器启动后的排队重建快照。
    pub async fn start_rebuild_index(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeImportJobSummary, String> {
        self.start_index_operation(library_id, KnowledgeOperationKind::Rebuild)
            .await
    }

    /// Repairs one derived index without deleting authoritative documents or healthy FTS rows.
    /// 在不删除权威文档或健康 FTS 记录的前提下补全一个派生索引。
    ///
    /// Parameters: `library_id` selects the exact active library.
    /// Returns: the authoritative chunk count inspected during repair.
    /// 参数：`library_id` 选择精确活动知识库。返回值：补全过程检查的权威切片数量。
    pub async fn repair_index(&self, library_id: &str) -> Result<u64, String> {
        self.require_active_library(library_id).await?;
        // Operation reserves the library before synchronous maintenance starts.
        // Operation 在同步维护开始前预留该知识库。
        let operation = self
            .prepare_operation(library_id, KnowledgeOperationKind::Repair, None)
            .await?;
        self.execute_prepared_index_operation(operation).await
    }

    /// Starts a detached non-destructive FTS repair and immediately returns its durable job snapshot.
    /// 启动一个分离式非破坏性 FTS 补全，并立即返回其持久化任务快照。
    ///
    /// Parameters: `library_id` selects the exact active library.
    /// Returns: the queued repair snapshot after its executor has been spawned.
    /// 参数：`library_id` 选择精确活动知识库。返回值：执行器启动后的排队补全快照。
    pub async fn start_repair_index(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeImportJobSummary, String> {
        self.start_index_operation(library_id, KnowledgeOperationKind::Repair)
            .await
    }

    /// Reserves and spawns one detached repair or rebuild operation.
    /// 预留并启动一个分离式补全或重建操作。
    ///
    /// Parameters: `library_id` selects the library and `operation_kind` must be repair or rebuild.
    /// Returns: the durable queued snapshot.
    /// 参数：`library_id` 选择知识库，`operation_kind` 必须是补全或重建。返回值：持久化排队快照。
    async fn start_index_operation(
        &self,
        library_id: &str,
        operation_kind: KnowledgeOperationKind,
    ) -> Result<KnowledgeImportJobSummary, String> {
        if operation_kind == KnowledgeOperationKind::Import {
            return Err(String::from("索引维护操作不能使用 import 类型。"));
        }
        self.require_active_library(library_id).await?;
        // Operation is visible to refresh recovery before detached execution starts.
        // Operation 在分离执行开始前已对刷新恢复可见。
        let operation = self
            .prepare_operation(library_id, operation_kind, None)
            .await?;
        // Snapshot retains the exact kind even after the caller's request is released.
        // Snapshot 即使在调用请求释放后仍保留精确操作类型。
        let snapshot = queued_operation_summary(&operation);
        // Runtime owns the executor independently from the HTTP handler future.
        // Runtime 独立于 HTTP 处理器 future 持有执行器。
        let runtime = self.clone();
        tokio::spawn(async move {
            if let Err(error) = runtime.execute_prepared_index_operation(operation).await {
                eprintln!("[knowledge-base] detached index operation failed: {error}");
            }
        });
        Ok(snapshot)
    }

    /// Executes one prepared repair or rebuild and persists a terminal job and index state.
    /// 执行一个已预留补全或重建，并持久化任务与索引终态。
    ///
    /// Parameters: `operation` contains the exact library, kind, job identity, and cancellation signal.
    /// Returns: the authoritative chunk count processed by the maintenance operation.
    /// 参数：`operation` 包含精确知识库、类型、任务身份与取消信号。返回值：维护操作处理的权威切片数量。
    async fn execute_prepared_index_operation(
        &self,
        operation: PreparedKnowledgeOperation,
    ) -> Result<u64, String> {
        // BindingIdentity survives outside the result only when activation succeeds.
        // BindingIdentity 仅在激活成功时于结果外继续存在。
        let mut binding_identity: Option<(String, String)> = None;
        // Result includes every lock, state, and physical index mutation failure.
        // Result 包含每个锁、状态和物理索引变更失败。
        let result = async {
            // Guard serializes this maintenance operation with all library mutations.
            // Guard 将本维护操作与全部知识库变更串行化。
            let _guard = self.library_mutation_guard(&operation.library_id).await?;
            require_operation_not_cancelled(&operation.cancellation)?;
            self.require_active_library(&operation.library_id).await?;
            // ExactBinding is retained for terminal failure projection.
            // ExactBinding 为终态失败投影保留。
            let exact_binding = self.ensure_library_binding(&operation.library_id).await?;
            binding_identity = Some(exact_binding.clone());
            // Phase is stable and directly consumable by the progress UI.
            // Phase 稳定且可由进度界面直接使用。
            let phase = match operation.operation_kind {
                KnowledgeOperationKind::Repair => "repairing",
                KnowledgeOperationKind::Rebuild => "rebuilding",
                KnowledgeOperationKind::Import => {
                    return Err(String::from("导入操作不能进入索引维护执行器。"));
                }
            };
            self.update_operation_phase(&operation.job_id, "running", phase)
                .await?;
            self.set_library_index_state(
                &operation.library_id,
                &exact_binding.0,
                &exact_binding.1,
                "rebuilding",
                "",
            )
            .await?;
            match operation.operation_kind {
                KnowledgeOperationKind::Repair => {
                    self.repair_index_inner(
                        &operation.library_id,
                        &exact_binding.0,
                        &exact_binding.1,
                        &operation.job_id,
                        &operation.cancellation,
                    )
                    .await
                }
                KnowledgeOperationKind::Rebuild => {
                    self.rebuild_index_inner(
                        &operation.library_id,
                        &exact_binding.0,
                        &exact_binding.1,
                        Some(&operation.job_id),
                        Some(&operation.cancellation),
                    )
                    .await
                }
                KnowledgeOperationKind::Import => {
                    Err(String::from("导入操作不能进入索引维护执行器。"))
                }
            }
        }
        .await;

        // FinishResult owns the durable terminal operation transition.
        // FinishResult 持有持久化操作终态转换。
        let finish_result = match &result {
            Ok(processed_items) => {
                self.finish_index_operation_job(&operation.job_id, *processed_items)
                    .await
            }
            Err(error) if error == KNOWLEDGE_OPERATION_CANCELLED_ERROR => {
                if let Some((space_id, binding_id)) = binding_identity.as_ref() {
                    let _ = self
                        .set_library_index_state(
                            &operation.library_id,
                            space_id,
                            binding_id,
                            "dirty",
                            "",
                        )
                        .await;
                }
                self.finish_cancelled_operation_job(&operation.job_id).await
            }
            Err(error) => {
                if let Some((space_id, binding_id)) = binding_identity.as_ref() {
                    let _ = self
                        .set_library_index_state(
                            &operation.library_id,
                            space_id,
                            binding_id,
                            "failed",
                            error,
                        )
                        .await;
                }
                self.finish_failed_operation_job(&operation.job_id, error)
                    .await
            }
        };
        self.release_active_operation(&operation.library_id, &operation.job_id)
            .await;
        finish_result?;
        result
    }

    /// Deletes one isolated library after revoking access and validating its canonical path.
    /// 在撤销访问并验证规范路径后删除一个隔离知识库。
    pub async fn delete_library(&self, library_id: &str) -> Result<(), String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        self.require_active_library(library_id).await?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_libraries SET lifecycle_state = 'deleting', updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![int(unix_epoch_millis()?), text(library_id)],
            )
            .await
            .map_err(|error| format!("撤销知识库访问失败：{error}"))?;

        let (space_id, binding_id) = self.library_binding_identity(library_id)?;
        let _ = self
            .controller
            .client
            .execute_sqlite_script_typed(
                &space_id,
                &binding_id,
                "PRAGMA wal_checkpoint(TRUNCATE);",
                Vec::new(),
            )
            .await;
        self.release_library_binding(library_id).await?;

        let root = self.library_root(library_id)?;
        let knowledge_root = self.knowledge_root();
        let canonical_knowledge_root = fs::canonicalize(&knowledge_root).map_err(|error| {
            format!(
                "规范化知识库根目录失败（{}）：{error}",
                knowledge_root.display()
            )
        })?;
        let canonical_library_root = fs::canonicalize(&root).map_err(|error| {
            format!("规范化待删除知识库目录失败（{}）：{error}", root.display())
        })?;
        if canonical_library_root.parent() != Some(canonical_knowledge_root.as_path())
            || canonical_library_root
                .file_name()
                .and_then(|name| name.to_str())
                != Some(library_id)
        {
            return Err(format!(
                "拒绝删除知识库根目录之外的路径：{}",
                canonical_library_root.display()
            ));
        }
        fs::remove_dir_all(&canonical_library_root).map_err(|error| {
            format!(
                "删除知识库目录失败（{}）：{error}",
                canonical_library_root.display()
            )
        })?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "DELETE FROM knowledge_libraries WHERE id = ? AND lifecycle_state = 'deleting';",
                vec![text(library_id)],
            )
            .await
            .map_err(|error| format!("删除知识库注册记录失败：{error}"))?;
        Ok(())
    }

    /// Runs the authoritative import state machine while the caller holds the mutation lock.
    /// 在调用方持有变更锁时运行权威导入状态机。
    #[allow(clippy::too_many_arguments)]
    async fn import_markdown_directory_inner(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        job_id: &str,
        canonical_root: &Path,
        requested_source_id: Option<&str>,
        cancellation: &Arc<AtomicBool>,
        now: i64,
    ) -> Result<KnowledgeImportReport, String> {
        let source_root_text = runtime_assets::native_host_path_text_from_path(canonical_root);
        let root_key = path_key(canonical_root);
        let display_name = canonical_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&source_root_text)
            .to_string();
        let source_id = self
            .resolve_source_id(space_id, binding_id, &root_key, requested_source_id)
            .await?;
        let candidates = self
            .scan_markdown_files_with_progress(job_id, canonical_root, cancellation)
            .await?;
        self.prepare_import_processing(job_id, candidates.len() as u64)
            .await?;

        let existing_documents: Vec<ExistingDocumentRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT id, relative_path_key, source_sha256 FROM knowledge_documents WHERE source_id = ?;",
                vec![text(&source_id)],
            )
            .await?;
        let existing_by_path: BTreeMap<String, ExistingDocumentRow> = existing_documents
            .iter()
            .cloned()
            .map(|row| (row.relative_path_key.clone(), row))
            .collect();
        let existing_chunks: Vec<ExistingChunkRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT c.id, c.document_id FROM knowledge_chunks c INNER JOIN knowledge_documents d ON d.id = c.document_id WHERE d.source_id = ?;",
                vec![text(&source_id)],
            )
            .await?;

        self.clear_staging(space_id, binding_id).await?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                "INSERT INTO knowledge_import_staging_context(singleton, job_id, source_id, committed_at_ms) VALUES (1, ?, ?, ?) ON CONFLICT(singleton) DO UPDATE SET job_id = excluded.job_id, source_id = excluded.source_id, committed_at_ms = excluded.committed_at_ms;",
                vec![text(job_id), text(&source_id), int(now)],
            )
            .await
            .map_err(|error| format!("初始化知识库导入暂存上下文失败：{error}"))?;

        let mut warnings = Vec::new();
        let mut staged_paths = Vec::with_capacity(candidates.len());
        let mut staged_documents = Vec::new();
        let mut staged_chunks = Vec::new();
        let mut changed_document_ids = BTreeSet::new();
        let mut unchanged_files = 0u64;
        let mut skipped_files = 0u64;
        let mut processed_bytes = 0u64;
        // LastProcessingProgressAt limits database writes while keeping the visible file path fresh.
        // LastProcessingProgressAt 限制数据库写入频率，同时保持可见文件路径及时更新。
        let mut last_processing_progress_at = Instant::now()
            .checked_sub(Duration::from_millis(250))
            .unwrap_or_else(Instant::now);

        for (candidate_index, candidate) in candidates.iter().enumerate() {
            require_operation_not_cancelled(cancellation)?;
            if last_processing_progress_at.elapsed() >= Duration::from_millis(100) {
                self.update_import_progress(
                    job_id,
                    candidate_index as u64,
                    processed_bytes,
                    Some(&candidate.relative_path),
                    &warnings,
                )
                .await?;
                last_processing_progress_at = Instant::now();
            }
            staged_paths.push(vec![text(&candidate.relative_path_key)]);
            let bytes = match fs::read(&candidate.source_path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    skipped_files += 1;
                    push_warning(
                        &mut warnings,
                        format!("无法读取 {}：{error}", candidate.relative_path),
                    );
                    continue;
                }
            };
            processed_bytes = processed_bytes.saturating_add(bytes.len() as u64);
            let digest = sha256_hex(&bytes);
            if existing_by_path
                .get(&candidate.relative_path_key)
                .is_some_and(|existing| existing.source_sha256 == digest)
            {
                unchanged_files += 1;
                continue;
            }
            let content = match decode_complete_user_text(&bytes) {
                Ok(decoded) => normalize_markdown_newlines(&decoded.content),
                Err(error) => {
                    skipped_files += 1;
                    push_warning(
                        &mut warnings,
                        format!(
                            "{} 不是有效 UTF-8 或 GB18030：{error}",
                            candidate.relative_path
                        ),
                    );
                    continue;
                }
            };
            let document_id = existing_by_path
                .get(&candidate.relative_path_key)
                .map(|existing| existing.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let title = extract_document_title(
                &content,
                Path::new(&candidate.relative_path)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("document"),
            );
            let line_count = content.lines().count().max(1) as i64;
            staged_documents.push(vec![
                text(&document_id),
                text(&source_id),
                text(&candidate.relative_path),
                text(&candidate.relative_path_key),
                text(&candidate.folder_path),
                text(&title),
                text(&content),
                text(&digest),
                int(candidate.byte_size as i64),
                int(line_count),
                candidate
                    .modified_at_ms
                    .map_or(ControllerSqliteValue::Null, int),
                int(now),
                int(now),
            ]);
            for parsed in chunk_markdown(&content) {
                let chunk_id = format!(
                    "{}:{}:{}",
                    document_id,
                    parsed.ordinal,
                    &parsed.content_sha256[..16]
                );
                staged_chunks.push(vec![
                    text(&chunk_id),
                    text(&document_id),
                    int(parsed.ordinal as i64),
                    text(&parsed.heading_path),
                    int(parsed.start_line as i64),
                    int(parsed.end_line as i64),
                    text(&parsed.content),
                    text(&parsed.content_sha256),
                    int(parsed.estimated_tokens as i64),
                    int(now),
                    int(now),
                ]);
            }
            changed_document_ids.insert(document_id);
        }

        require_operation_not_cancelled(cancellation)?;
        self.persist_source(
            space_id,
            binding_id,
            &source_id,
            &source_root_text,
            &root_key,
            &display_name,
            now,
        )
        .await?;
        self.stage_import_rows(
            space_id,
            binding_id,
            staged_paths,
            staged_documents,
            staged_chunks,
        )
        .await?;
        let scanned_path_keys: BTreeSet<String> = candidates
            .iter()
            .map(|candidate| candidate.relative_path_key.clone())
            .collect();
        let deleted_document_ids: BTreeSet<String> = existing_documents
            .iter()
            .filter(|document| !scanned_path_keys.contains(&document.relative_path_key))
            .map(|document| document.id.clone())
            .collect();
        let mut stale_chunk_ids = Vec::new();
        for chunk in existing_chunks {
            if changed_document_ids.contains(&chunk.document_id)
                || deleted_document_ids.contains(&chunk.document_id)
            {
                stale_chunk_ids.push(chunk.id);
            }
        }
        self.update_import_progress(
            job_id,
            candidates.len() as u64,
            processed_bytes,
            None,
            &warnings,
        )
        .await?;

        let changed_files = changed_document_ids.len() as u64;
        let deleted_files = deleted_document_ids.len() as u64;
        self.update_operation_phase(job_id, "running", "committing")
            .await?;
        if changed_files > 0 || deleted_files > 0 {
            require_operation_not_cancelled(cancellation)?;
            self.commit_staging(space_id, binding_id).await?;
            // AuthoritativeCounts must become visible even if derived indexing is later cancelled.
            // AuthoritativeCounts 即使派生索引随后被取消也必须立即可见。
            let authoritative_counts = self.authoritative_counts(space_id, binding_id).await?;
            self.project_registry_authoritative_counts(
                library_id,
                &authoritative_counts,
                "dirty",
                now,
            )
            .await?;
            if require_operation_not_cancelled(cancellation).is_err() {
                return Err(String::from(KNOWLEDGE_OPERATION_CANCELLED_ERROR));
            }
            self.update_operation_phase(job_id, "running", "indexing")
                .await?;
            let sync_result = self
                .sync_changed_fts(
                    job_id,
                    library_id,
                    space_id,
                    binding_id,
                    &stale_chunk_ids,
                    cancellation,
                )
                .await?;
            if sync_result == ImportIndexSyncOutcome::Cancelled {
                return Err(String::from(KNOWLEDGE_OPERATION_CANCELLED_ERROR));
            }
        }

        self.update_operation_phase(job_id, "running", "verifying")
            .await?;
        if self.verify_index_gate(space_id, binding_id).await.is_err() {
            self.update_operation_phase(job_id, "running", "rebuilding")
                .await?;
            if self
                .rebuild_index_for_import(job_id, library_id, space_id, binding_id, cancellation)
                .await?
                == ImportIndexSyncOutcome::Cancelled
            {
                return Err(String::from(KNOWLEDGE_OPERATION_CANCELLED_ERROR));
            }
        }

        self.update_operation_phase(job_id, "running", "finalizing")
            .await?;
        let counts = self.authoritative_counts(space_id, binding_id).await?;
        self.project_registry_counts(library_id, &counts, "ready", now)
            .await?;
        self.clear_staging(space_id, binding_id).await?;
        Ok(KnowledgeImportReport {
            job_id: job_id.to_string(),
            library_id: library_id.to_string(),
            scanned_files: candidates.len() as u64,
            changed_files,
            unchanged_files,
            deleted_files,
            skipped_files,
            processed_bytes,
            chunk_count: nonnegative_u64(counts.chunk_count, "chunk_count")?,
            warnings,
        })
    }

    /// Initializes the physical directory, manifest, controller binding, and authoritative schema.
    /// 初始化物理目录、清单、控制器绑定与权威结构。
    async fn initialize_library_storage(&self, library_id: &str, name: &str) -> Result<(), String> {
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

    /// Resolves the durable identity for one import without changing its recorded path.
    /// 在不修改已记录路径的前提下解析一次导入的持久化身份。
    ///
    /// Parameters: `space_id` and `binding_id` select the physical database, `root_key` is the
    /// canonical path key, and `requested_source_id` explicitly preserves an existing identity.
    /// Returns: the existing or newly allocated source identity after duplicate-root validation.
    /// 参数：`space_id` 与 `binding_id` 选择物理数据库，`root_key` 是规范路径键，
    /// `requested_source_id` 显式保留现有身份。返回值：完成重复根目录校验后的现有或新分配来源身份。
    async fn resolve_source_id(
        &self,
        space_id: &str,
        binding_id: &str,
        root_key: &str,
        requested_source_id: Option<&str>,
    ) -> Result<String, String> {
        if let Some(requested_source_id) = requested_source_id {
            let requested_source_id = requested_source_id.trim();
            if requested_source_id.is_empty() {
                return Err(String::from("知识库来源标识符不能为空。"));
            }
            let target_rows: Vec<SourceRow> = self
                .query_library_rows(
                    space_id,
                    binding_id,
                    "SELECT id FROM knowledge_sources WHERE id = ?;",
                    vec![text(requested_source_id)],
                )
                .await?;
            if target_rows.is_empty() {
                return Err(format!("知识库来源不存在：{requested_source_id}"));
            }
            let root_rows: Vec<SourceRow> = self
                .query_library_rows(
                    space_id,
                    binding_id,
                    "SELECT id FROM knowledge_sources WHERE root_key = ?;",
                    vec![text(root_key)],
                )
                .await?;
            if let Some(conflicting_source) = root_rows
                .into_iter()
                .find(|source| source.id != requested_source_id)
            {
                return Err(format!(
                    "所选目录已属于另一个知识库来源：{}",
                    conflicting_source.id
                ));
            }
            return Ok(requested_source_id.to_string());
        }
        let rows: Vec<SourceRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT id FROM knowledge_sources WHERE root_key = ?;",
                vec![text(root_key)],
            )
            .await?;
        Ok(rows
            .into_iter()
            .next()
            .map(|row| row.id)
            .unwrap_or_else(|| Uuid::new_v4().to_string()))
    }

    /// Persists a successfully processed source path while retaining its durable identity.
    /// 持久化已成功处理的来源路径，同时保留其持久化身份。
    ///
    /// Parameters: database identities select the library, source fields describe the canonical
    /// root, and `now` records the completed scan time. Returns: success after the row is durable.
    /// 参数：数据库身份选择知识库，来源字段描述规范根目录，`now` 记录已完成扫描时间。
    /// 返回值：来源记录持久化后的成功结果。
    #[allow(clippy::too_many_arguments)]
    async fn persist_source(
        &self,
        space_id: &str,
        binding_id: &str,
        source_id: &str,
        canonical_root: &str,
        root_key: &str,
        display_name: &str,
        now: i64,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                r#"INSERT INTO knowledge_sources(
                       id, canonical_root, root_key, display_name, created_at_ms, last_scanned_at_ms
                   ) VALUES (?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       canonical_root = excluded.canonical_root,
                       root_key = excluded.root_key,
                       display_name = excluded.display_name,
                       last_scanned_at_ms = excluded.last_scanned_at_ms;"#,
                vec![
                    text(source_id),
                    text(canonical_root),
                    text(root_key),
                    text(display_name),
                    int(now),
                    int(now),
                ],
            )
            .await
            .map_err(|error| format!("保存知识库来源目录失败：{error}"))?;
        Ok(())
    }

    /// Clears all per-library staging rows outside an authoritative transaction.
    /// 在权威事务之外清理全部单知识库暂存记录。
    async fn clear_staging(&self, space_id: &str, binding_id: &str) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                "DELETE FROM knowledge_staged_chunks; DELETE FROM knowledge_staged_documents; DELETE FROM knowledge_staged_paths; DELETE FROM knowledge_import_staging_context;",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("清理知识库导入暂存区失败：{error}"))?;
        Ok(())
    }

    /// Writes all prepared path, document, and chunk rows into staging tables.
    /// 将全部已准备路径、文档与切片记录写入暂存表。
    async fn stage_import_rows(
        &self,
        space_id: &str,
        binding_id: &str,
        paths: Vec<Vec<ControllerSqliteValue>>,
        documents: Vec<Vec<ControllerSqliteValue>>,
        chunks: Vec<Vec<ControllerSqliteValue>>,
    ) -> Result<(), String> {
        if !paths.is_empty() {
            self.execute_library_batch(
                space_id,
                binding_id,
                "INSERT INTO knowledge_staged_paths(relative_path_key) VALUES (?);",
                paths,
                "写入扫描路径暂存表失败",
            )
            .await?;
        }
        if !documents.is_empty() {
            self.execute_library_batch(
                space_id,
                binding_id,
                r#"INSERT INTO knowledge_staged_documents(
                    id, source_id, relative_path, relative_path_key, folder_path, title, content,
                    source_sha256, byte_size, line_count, source_modified_at_ms, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#,
                documents,
                "写入文档暂存表失败",
            )
            .await?;
        }
        if !chunks.is_empty() {
            self.execute_library_batch(
                space_id,
                binding_id,
                r#"INSERT INTO knowledge_staged_chunks(
                    id, document_id, ordinal, heading_path, start_line, end_line, content,
                    content_sha256, estimated_tokens, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#,
                chunks,
                "写入切片暂存表失败",
            )
            .await?;
        }
        Ok(())
    }

    /// Atomically promotes staged changes and marks the derived index dirty.
    /// 原子提升暂存变更并将派生索引标记为脏。
    async fn commit_staging(&self, space_id: &str, binding_id: &str) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                r#"
BEGIN IMMEDIATE;
DELETE FROM knowledge_chunks
WHERE document_id IN (SELECT id FROM knowledge_staged_documents);

INSERT INTO knowledge_documents(
    id, source_id, relative_path, relative_path_key, folder_path, title, content,
    source_sha256, byte_size, line_count, source_modified_at_ms, created_at_ms, updated_at_ms
)
SELECT id, source_id, relative_path, relative_path_key, folder_path, title, content,
       source_sha256, byte_size, line_count, source_modified_at_ms, created_at_ms, updated_at_ms
FROM knowledge_staged_documents WHERE 1
ON CONFLICT(id) DO UPDATE SET
    relative_path = excluded.relative_path,
    relative_path_key = excluded.relative_path_key,
    folder_path = excluded.folder_path,
    title = excluded.title,
    content = excluded.content,
    source_sha256 = excluded.source_sha256,
    byte_size = excluded.byte_size,
    line_count = excluded.line_count,
    source_modified_at_ms = excluded.source_modified_at_ms,
    updated_at_ms = excluded.updated_at_ms;

INSERT INTO knowledge_chunks(
    id, document_id, ordinal, heading_path, start_line, end_line, content,
    content_sha256, estimated_tokens, created_at_ms, updated_at_ms
)
SELECT id, document_id, ordinal, heading_path, start_line, end_line, content,
       content_sha256, estimated_tokens, created_at_ms, updated_at_ms
FROM knowledge_staged_chunks;

DELETE FROM knowledge_documents
WHERE source_id = (SELECT source_id FROM knowledge_import_staging_context WHERE singleton = 1)
  AND relative_path_key NOT IN (SELECT relative_path_key FROM knowledge_staged_paths);

UPDATE knowledge_metadata
SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
WHERE key = 'content_revision';
UPDATE knowledge_metadata SET value = 'dirty' WHERE key = 'index_status';
UPDATE knowledge_metadata SET value = '' WHERE key = 'last_index_error';
COMMIT;
"#,
                Vec::new(),
            )
            .await
            .map_err(|error| format!("提交知识库权威内容失败：{error}"))?;
        Ok(())
    }

    /// Synchronizes only changed and deleted chunk rows into vldb-managed FTS.
    /// 仅将变更与删除的切片记录同步到 vldb 管理的 FTS。
    #[allow(clippy::too_many_arguments)]
    async fn sync_changed_fts(
        &self,
        job_id: &str,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        stale_chunk_ids: &[String],
        cancellation: &AtomicBool,
    ) -> Result<ImportIndexSyncOutcome, String> {
        self.controller
            .client
            .ensure_sqlite_fts_index(
                space_id,
                binding_id,
                KNOWLEDGE_FTS_INDEX,
                ControllerSqliteTokenizerMode::Jieba,
            )
            .await
            .map_err(|error| format!("创建 Jieba FTS 索引失败：{error}"))?;
        for chunk_ids in stale_chunk_ids.chunks(FTS_DELETE_PARAMETER_BATCH_SIZE) {
            if require_operation_not_cancelled(cancellation).is_err() {
                return Ok(ImportIndexSyncOutcome::Cancelled);
            }
            // Placeholders is generated only from the bounded slice length; identifiers remain bound values.
            // Placeholders 仅根据有界切片长度生成；标识符仍以绑定值传入。
            let placeholders = std::iter::repeat_n("?", chunk_ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            self.controller
                .client
                .execute_sqlite_script_typed(
                    space_id,
                    binding_id,
                    format!("DELETE FROM {KNOWLEDGE_FTS_INDEX} WHERE id IN ({placeholders});"),
                    chunk_ids.iter().map(text).collect(),
                )
                .await
                .map_err(|error| format!("集合式删除过期 FTS 切片失败：{error}"))?;
        }
        // TotalItems is derived from the committed authoritative rows selected by staged documents.
        // TotalItems 从暂存文档选择出的已提交权威记录派生。
        let total_items = self.staged_chunk_count(space_id, binding_id).await?;
        let write_result = self
            .replay_staged_fts_for_import(job_id, space_id, binding_id, total_items, cancellation)
            .await?;
        if write_result == ImportIndexSyncOutcome::Cancelled {
            return Ok(write_result);
        }
        if require_operation_not_cancelled(cancellation).is_err() {
            return Ok(ImportIndexSyncOutcome::Cancelled);
        }
        self.verify_index_rows_exact(space_id, binding_id).await?;
        self.mark_index_ready(space_id, binding_id).await?;
        self.set_registry_index_status(library_id, "ready").await?;
        Ok(ImportIndexSyncOutcome::Completed)
    }

    /// Rebuilds authoritative FTS rows in cancellable batches for one visible import job.
    /// 为一个可见导入任务以可取消批次重建权威 FTS 记录。
    async fn rebuild_index_for_import(
        &self,
        job_id: &str,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        cancellation: &AtomicBool,
    ) -> Result<ImportIndexSyncOutcome, String> {
        require_operation_not_cancelled(cancellation)?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                format!("DROP TABLE IF EXISTS {KNOWLEDGE_FTS_INDEX};"),
                Vec::new(),
            )
            .await
            .map_err(|error| format!("删除旧 FTS 索引失败：{error}"))?;
        self.controller
            .client
            .ensure_sqlite_fts_index(
                space_id,
                binding_id,
                KNOWLEDGE_FTS_INDEX,
                ControllerSqliteTokenizerMode::Jieba,
            )
            .await
            .map_err(|error| format!("重建 Jieba FTS 索引结构失败：{error}"))?;
        // TotalItems is read without materializing all chunk bodies across the controller boundary.
        // TotalItems 无需跨控制器边界物化全部切片正文即可读取。
        let total_items = nonnegative_u64(
            self.authoritative_counts(space_id, binding_id)
                .await?
                .chunk_count,
            "chunk_count",
        )?;
        let write_result = self
            .replay_authoritative_fts_for_import(
                job_id,
                space_id,
                binding_id,
                total_items,
                cancellation,
            )
            .await?;
        if write_result == ImportIndexSyncOutcome::Cancelled {
            return Ok(write_result);
        }
        self.verify_index_rows_exact(space_id, binding_id).await?;
        self.mark_index_ready(space_id, binding_id).await?;
        self.set_registry_index_status(library_id, "ready").await?;
        Ok(ImportIndexSyncOutcome::Completed)
    }

    /// Replays only committed chunks belonging to staged changed documents in set-based windows.
    /// 仅以集合式窗口回放属于暂存变更文档的已提交切片。
    async fn replay_staged_fts_for_import(
        &self,
        job_id: &str,
        space_id: &str,
        binding_id: &str,
        total_items: u64,
        cancellation: &AtomicBool,
    ) -> Result<ImportIndexSyncOutcome, String> {
        self.update_import_index_progress(job_id, 0, total_items, None)
            .await?;
        // Offset advances over a fixed staged-document result set and cannot skip inserted rows.
        // Offset 在固定的暂存文档结果集上推进，不会跳过已插入记录。
        let mut offset = 0usize;
        while offset < total_items as usize {
            if require_operation_not_cancelled(cancellation).is_err() {
                return Ok(ImportIndexSyncOutcome::Cancelled);
            }
            // BatchSize bounds FTS5 write amplification while reducing controller crossings fourfold from 128 rows.
            // BatchSize 限制 FTS5 写放大，同时相较 128 行窗口将控制器跨越次数减少四倍。
            let batch_size = FTS_MUTATION_BATCH_SIZE.min(total_items as usize - offset);
            self.replay_staged_fts_batch(space_id, binding_id, batch_size, offset)
                .await?;
            offset += batch_size;
            self.update_import_index_progress(job_id, offset as u64, total_items, None)
                .await?;
        }
        self.update_import_index_progress(job_id, total_items, total_items, None)
            .await?;
        Ok(ImportIndexSyncOutcome::Completed)
    }

    /// Replays all authoritative rows in set-based batches with durable import progress.
    /// 使用集合式批次回放全部权威记录，并持久化导入进度。
    async fn replay_authoritative_fts_for_import(
        &self,
        job_id: &str,
        space_id: &str,
        binding_id: &str,
        total_items: u64,
        cancellation: &AtomicBool,
    ) -> Result<ImportIndexSyncOutcome, String> {
        self.update_import_index_progress(job_id, 0, total_items, None)
            .await?;
        // Offset advances over the immutable authoritative chunk set during the mutation lock.
        // Offset 在变更锁期间保持不变的权威切片集合上推进。
        let mut offset = 0usize;
        while offset < total_items as usize {
            if require_operation_not_cancelled(cancellation).is_err() {
                return Ok(ImportIndexSyncOutcome::Cancelled);
            }
            // BatchSize bounds one FTS transaction and controller response latency.
            // BatchSize 限制单次 FTS 事务与控制器响应延迟。
            let batch_size = FTS_MUTATION_BATCH_SIZE.min(total_items as usize - offset);
            self.replay_authoritative_fts_batch(space_id, binding_id, batch_size, offset)
                .await?;
            offset += batch_size;
            self.update_import_index_progress(job_id, offset as u64, total_items, None)
                .await?;
        }
        self.update_import_index_progress(job_id, total_items, total_items, None)
            .await?;
        Ok(ImportIndexSyncOutcome::Completed)
    }

    /// Replays one staged changed-document chunk window with one set-based SQL statement.
    /// 使用一条集合式 SQL 语句回放一个暂存变更文档切片窗口。
    async fn replay_staged_fts_batch(
        &self,
        space_id: &str,
        binding_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(), String> {
        // Sql selects only committed chunks whose document identity remains in staging.
        // Sql 仅选择文档身份仍处于暂存集合中的已提交切片。
        let sql = format!(
            r#"INSERT INTO {KNOWLEDGE_FTS_INDEX}(id, file_path, title, content)
               SELECT c.id,
                      d.relative_path,
                      CASE WHEN c.heading_path = ''
                           THEN d.title
                           ELSE d.title || ' / ' || c.heading_path
                      END,
                      c.content
               FROM knowledge_chunks c
               INNER JOIN knowledge_documents d ON d.id = c.document_id
               INNER JOIN knowledge_staged_documents s ON s.id = d.id
               ORDER BY d.relative_path_key ASC, c.ordinal ASC
               LIMIT {limit} OFFSET {offset};"#
        );
        self.controller
            .client
            .execute_sqlite_script_typed(space_id, binding_id, sql, Vec::new())
            .await
            .map_err(|error| format!("集合式写入变更 Jieba FTS 切片失败：{error}"))?;
        Ok(())
    }

    /// Replays one ordered authoritative chunk window into FTS with one set-based SQL statement.
    /// 使用一条集合式 SQL 语句将一个有序权威切片窗口回放到 FTS。
    async fn replay_authoritative_fts_batch(
        &self,
        space_id: &str,
        binding_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(), String> {
        // SetBasedReplay lets SQLite tokenize the selected rows without one statement per chunk.
        // SetBasedReplay 让 SQLite 对选中行直接分词，避免每个切片执行一条语句。
        let sql = format!(
            r#"INSERT INTO {KNOWLEDGE_FTS_INDEX}(id, file_path, title, content)
               SELECT c.id,
                      d.relative_path,
                      CASE WHEN c.heading_path = ''
                           THEN d.title
                           ELSE d.title || ' / ' || c.heading_path
                      END,
                      c.content
               FROM knowledge_chunks c
               INNER JOIN knowledge_documents d ON d.id = c.document_id
               ORDER BY d.relative_path_key ASC, c.ordinal ASC
               LIMIT {limit} OFFSET {offset};"#
        );
        self.controller
            .client
            .execute_sqlite_script_typed(space_id, binding_id, sql, Vec::new())
            .await
            .map_err(|error| format!("集合式回放 Jieba FTS 切片失败：{error}"))?;
        Ok(())
    }

    /// Repairs missing, mismatched, and orphaned FTS rows from authoritative chunks.
    /// 根据权威切片补全缺失、错配和孤立 FTS 记录。
    async fn repair_index_inner(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        job_id: &str,
        cancellation: &AtomicBool,
    ) -> Result<u64, String> {
        require_operation_not_cancelled(cancellation)?;
        // Before determines whether dictionary or tokenizer changes require complete retokenization.
        // Before 决定词典或 tokenizer 变更是否要求完整重新分词。
        let before = self
            .inspect_index_integrity_for_binding(library_id, space_id, binding_id)
            .await?;
        if before.tokenizer != "jieba"
            || before.dictionary_revision != before.indexed_dictionary_revision
        {
            self.update_operation_phase(job_id, "running", "rebuilding")
                .await?;
            return self
                .rebuild_index_inner(
                    library_id,
                    space_id,
                    binding_id,
                    Some(job_id),
                    Some(cancellation),
                )
                .await;
        }
        self.controller
            .client
            .ensure_sqlite_fts_index(
                space_id,
                binding_id,
                KNOWLEDGE_FTS_INDEX,
                ControllerSqliteTokenizerMode::Jieba,
            )
            .await
            .map_err(|error| format!("创建待补全 Jieba FTS 索引失败：{error}"))?;
        // TotalItems is authoritative work scanned by deterministic repair windows.
        // TotalItems 是确定性补全窗口扫描的权威工作量。
        let total_items = before.authoritative_chunk_count;
        self.update_import_index_progress(job_id, 0, total_items, None)
            .await?;
        // DeleteSql removes mismatched rows and every duplicate beyond the first exact physical row.
        // DeleteSql 删除错配记录，以及每组精确物理记录中第一条之外的全部重复项。
        let delete_sql = format!(
            r#"DELETE FROM {KNOWLEDGE_FTS_INDEX}
               WHERE rowid IN (
                   SELECT candidate.rowid
                     FROM (
                         SELECT f.rowid,
                                ROW_NUMBER() OVER (
                                    PARTITION BY f.id, f.file_path, f.title, f.content
                                    ORDER BY f.rowid ASC
                                ) AS duplicate_ordinal,
                                EXISTS (
                                    SELECT 1
                                      FROM knowledge_chunks c
                                      INNER JOIN knowledge_documents d ON d.id = c.document_id
                                     WHERE c.id = f.id
                                       AND f.file_path = d.relative_path
                                       AND f.title = CASE WHEN c.heading_path = ''
                                                          THEN d.title
                                                          ELSE d.title || ' / ' || c.heading_path END
                                       AND f.content = c.content
                                ) AS source_matches
                           FROM {KNOWLEDGE_FTS_INDEX} f
                     ) candidate
                    WHERE candidate.duplicate_ordinal > 1
                       OR candidate.source_matches = 0
               );"#
        );
        self.controller
            .client
            .execute_sqlite_script_typed(space_id, binding_id, delete_sql, Vec::new())
            .await
            .map_err(|error| format!("删除孤立或错配 FTS 切片失败：{error}"))?;
        // Offset advances over authoritative rows rather than the shrinking missing-row subset.
        // Offset 在权威记录而非不断缩小的缺失记录子集上推进。
        let mut offset = 0usize;
        while offset < total_items as usize {
            require_operation_not_cancelled(cancellation)?;
            // BatchSize bounds each repair transaction below the controller request deadline.
            // BatchSize 将每次补全事务限制在控制器请求期限内。
            let batch_size = FTS_MUTATION_BATCH_SIZE.min(total_items as usize - offset);
            self.replay_missing_fts_batch(space_id, binding_id, batch_size, offset)
                .await?;
            offset += batch_size;
            self.update_import_index_progress(job_id, offset as u64, total_items, None)
                .await?;
        }
        self.verify_index_rows_exact(space_id, binding_id).await?;
        self.mark_index_ready(space_id, binding_id).await?;
        // Counts are re-read after repair so central summaries never depend on stale projections.
        // Counts 在补全后重新读取，使中央摘要绝不依赖过期投影。
        let counts = self.authoritative_counts(space_id, binding_id).await?;
        self.project_registry_authoritative_counts(
            library_id,
            &counts,
            "ready",
            unix_epoch_millis()?,
        )
        .await?;
        Ok(total_items)
    }

    /// Inserts missing FTS rows from one fixed authoritative window.
    /// 从一个固定权威窗口插入缺失 FTS 记录。
    async fn replay_missing_fts_batch(
        &self,
        space_id: &str,
        binding_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(), String> {
        // Sql applies LIMIT/OFFSET before one EXCEPT comparison so successful inserts cannot skip work.
        // Sql 在一次 EXCEPT 比较之前应用 LIMIT/OFFSET，因此成功插入不会跳过工作。
        let sql = format!(
            r#"INSERT INTO {KNOWLEDGE_FTS_INDEX}(id, file_path, title, content)
               SELECT id, file_path, title, content
                 FROM (
                     SELECT c.id AS id,
                            d.relative_path AS file_path,
                            CASE WHEN c.heading_path = ''
                                 THEN d.title
                                 ELSE d.title || ' / ' || c.heading_path
                            END AS title,
                            c.content AS content
                       FROM knowledge_chunks c
                       INNER JOIN knowledge_documents d ON d.id = c.document_id
                       ORDER BY d.relative_path_key ASC, c.ordinal ASC
                       LIMIT {limit} OFFSET {offset}
                 )
               EXCEPT
               SELECT id, file_path, title, content FROM {KNOWLEDGE_FTS_INDEX};"#
        );
        self.controller
            .client
            .execute_sqlite_script_typed(space_id, binding_id, sql, Vec::new())
            .await
            .map_err(|error| format!("集合式补全 Jieba FTS 切片失败：{error}"))?;
        Ok(())
    }

    /// Performs a destructive FTS-only rebuild followed by authoritative replay.
    /// 执行仅针对 FTS 的破坏性重建，随后从权威数据回放。
    async fn rebuild_index_inner(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        job_id: Option<&str>,
        cancellation: Option<&AtomicBool>,
    ) -> Result<u64, String> {
        if let Some(cancellation) = cancellation {
            require_operation_not_cancelled(cancellation)?;
        }
        self.controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                format!("DROP TABLE IF EXISTS {KNOWLEDGE_FTS_INDEX};"),
                Vec::new(),
            )
            .await
            .map_err(|error| format!("删除旧 FTS 索引失败：{error}"))?;
        self.controller
            .client
            .ensure_sqlite_fts_index(
                space_id,
                binding_id,
                KNOWLEDGE_FTS_INDEX,
                ControllerSqliteTokenizerMode::Jieba,
            )
            .await
            .map_err(|error| format!("重建 Jieba FTS 索引结构失败：{error}"))?;
        // Counts supplies a constant authoritative denominator without materializing chunk bodies.
        // Counts 提供恒定权威分母，无需物化切片正文。
        let counts = self.authoritative_counts(space_id, binding_id).await?;
        // TotalItems is validated before conversion into pagination offsets.
        // TotalItems 在转换为分页偏移前完成校验。
        let total_items = nonnegative_u64(counts.chunk_count, "chunk_count")?;
        if let Some(job_id) = job_id {
            self.update_import_index_progress(job_id, 0, total_items, None)
                .await?;
        }
        // Offset advances over the immutable authoritative set held by the mutation guard.
        // Offset 在变更锁保护的不变权威集合上推进。
        let mut offset = 0usize;
        while offset < total_items as usize {
            if let Some(cancellation) = cancellation {
                require_operation_not_cancelled(cancellation)?;
            }
            // BatchSize keeps each controller call cancellable and below its transport deadline.
            // BatchSize 使每次控制器调用可取消且保持在传输期限内。
            let batch_size = FTS_MUTATION_BATCH_SIZE.min(total_items as usize - offset);
            self.replay_authoritative_fts_batch(space_id, binding_id, batch_size, offset)
                .await?;
            offset += batch_size;
            if let Some(job_id) = job_id {
                self.update_import_index_progress(job_id, offset as u64, total_items, None)
                    .await?;
            }
        }
        self.verify_index_rows_exact(space_id, binding_id).await?;
        self.mark_index_ready(space_id, binding_id).await?;
        self.project_registry_authoritative_counts(
            library_id,
            &counts,
            "ready",
            unix_epoch_millis()?,
        )
        .await?;
        Ok(total_items)
    }

    /// Searches one physical library and materializes each hit from authoritative rows.
    /// 检索一个物理知识库，并从权威记录物化每条命中。
    async fn search_one_library(
        &self,
        library_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<(Vec<KnowledgeSearchHit>, String), String> {
        let library = self.require_active_library(library_id).await?;
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        self.verify_index_gate(&space_id, &binding_id).await?;
        // tokenized_query is produced by the VLDB SQLite Jieba tokenizer and contains a safe MATCH expression.
        // tokenized_query 由 VLDB SQLite Jieba 分词器生成，并包含安全的 MATCH 表达式。
        let tokenized_query = self
            .controller
            .client
            .tokenize_sqlite_text(
                &space_id,
                &binding_id,
                ControllerSqliteTokenizerMode::Jieba,
                query,
                true,
            )
            .await
            .map_err(|error| format!("构建 Jieba FTS 查询失败：{error}"))?;
        // The generic VLDB search response computes highlights and snippets that this host discards.
        // 通用 VLDB 搜索响应会计算本宿主不使用的高亮与摘要，因此这里只读取标识符与 BM25 分值。
        let ranked_rows: Vec<FtsRankRow> = self
            .query_library_rows(
                &space_id,
                &binding_id,
                &format!(
                    r#"SELECT id, bm25({KNOWLEDGE_FTS_INDEX}, 2.0, 1.0) AS raw_score
                       FROM {KNOWLEDGE_FTS_INDEX}
                       WHERE {KNOWLEDGE_FTS_INDEX} MATCH ?
                       ORDER BY raw_score ASC, file_path ASC, id ASC
                       LIMIT ?;"#,
                ),
                vec![text(&tokenized_query.fts_query), int(i64::from(limit))],
            )
            .await
            .map_err(|error| format!("执行 Jieba BM25 检索失败：{error}"))?;
        // hits preserves the exact rank order returned by the lightweight BM25 query.
        // hits 保留轻量 BM25 查询返回的精确排名顺序。
        let mut hits = Vec::with_capacity(ranked_rows.len());
        // fts_chunk_ids bounds the authoritative bulk materialization to ranked matches.
        // fts_chunk_ids 将权威批量物化限制在已排名的命中集合内。
        let fts_chunk_ids: Vec<String> = ranked_rows.iter().map(|row| row.id.clone()).collect();
        // authoritative_rows enables deterministic identifier lookup without additional controller calls.
        // authoritative_rows 支持确定性标识符查找且不再产生额外控制器调用。
        let mut authoritative_rows: BTreeMap<String, JoinedChunkRow> = self
            .load_joined_chunks_by_ids(&space_id, &binding_id, &fts_chunk_ids)
            .await?
            .into_iter()
            .map(|row| (row.id.clone(), row))
            .collect();
        for ranked_row in ranked_rows {
            // Materialize ranked hits from one bulk query to avoid one controller round trip per result.
            // 从单次批量查询物化排序命中，避免每条结果各发生一次控制器往返。
            let row = authoritative_rows
                .remove(&ranked_row.id)
                .ok_or_else(|| format!("FTS 命中的权威切片不存在：{}", ranked_row.id))?;
            hits.push(KnowledgeSearchHit {
                library_id: library.id.clone(),
                library_name: library.name.clone(),
                document_id: row.document_id,
                chunk_id: row.id,
                title: row.title,
                relative_path: row.relative_path,
                heading_path: row.heading_path,
                start_line: nonnegative_u64(row.start_line, "start_line")?,
                end_line: nonnegative_u64(row.end_line, "end_line")?,
                content: row.content,
                match_kind: String::from("bm25"),
                bm25_score: -ranked_row.raw_score,
                rrf_score: None,
            });
        }
        // SubstringFallback is intentionally cold: a successful BM25 query must never pay for a full authoritative scan.
        // SubstringFallback 刻意保持为冷路径：BM25 已有命中时绝不承担权威内容全表扫描。
        if hits.is_empty() {
            let substring_rows = self
                .load_substring_chunks(&space_id, &binding_id, query, MAX_SEARCH_LIMIT)
                .await?;
            for row in substring_rows {
                hits.push(KnowledgeSearchHit {
                    library_id: library.id.clone(),
                    library_name: library.name.clone(),
                    document_id: row.document_id,
                    chunk_id: row.id,
                    title: row.title,
                    relative_path: row.relative_path,
                    heading_path: row.heading_path,
                    start_line: nonnegative_u64(row.start_line, "start_line")?,
                    end_line: nonnegative_u64(row.end_line, "end_line")?,
                    content: row.content,
                    match_kind: String::from("substring"),
                    bm25_score: 0.0,
                    rrf_score: None,
                });
                if hits.len() >= limit as usize {
                    break;
                }
            }
        }
        Ok((hits, tokenized_query.fts_query))
    }

    /// Loads deterministic case-insensitive substring matches from authoritative Markdown chunks.
    /// 从权威 Markdown 切片加载确定性的、不区分大小写的子串命中。
    async fn load_substring_chunks(
        &self,
        space_id: &str,
        binding_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<JoinedChunkRow>, String> {
        self.query_library_rows(
            space_id,
            binding_id,
            r#"WITH needle(value) AS (SELECT lower(?))
               SELECT c.id, c.document_id, c.ordinal, c.heading_path, c.start_line, c.end_line,
                      c.content, c.estimated_tokens, d.title, d.relative_path
               FROM knowledge_chunks c
               INNER JOIN knowledge_documents d ON d.id = c.document_id
               CROSS JOIN needle
               WHERE instr(lower(c.content), needle.value) > 0
                  OR instr(lower(c.heading_path), needle.value) > 0
                  OR instr(lower(d.title), needle.value) > 0
               ORDER BY CASE
                            WHEN instr(lower(d.title), needle.value) > 0 THEN 0
                            WHEN instr(lower(c.heading_path), needle.value) > 0 THEN 1
                            ELSE 2
                        END,
                        d.relative_path_key ASC,
                        c.ordinal ASC
               LIMIT ?;"#,
            vec![text(query), int(i64::from(limit))],
        )
        .await
    }

    /// Returns a complete metadata and physical-row integrity snapshot for one active library.
    /// 返回一个活动知识库的完整元数据与物理记录完整性快照。
    ///
    /// Parameters: `library_id` selects the exact library to inspect.
    /// Returns: a diagnostic snapshot even when the index is dirty, failed, or physically absent.
    /// 参数：`library_id` 选择要检查的精确知识库。返回值：即使索引为脏、失败或物理缺失也返回诊断快照。
    pub async fn inspect_index_integrity(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeIndexIntegritySummary, String> {
        self.require_active_library(library_id).await?;
        // BindingIdentity selects the only isolated SQLite database owned by this library.
        // BindingIdentity 选择该知识库拥有的唯一隔离 SQLite 数据库。
        let (space_id, binding_id) = self.ensure_library_binding(library_id).await?;
        self.inspect_index_integrity_for_binding(library_id, &space_id, &binding_id)
            .await
    }

    /// Builds one complete integrity snapshot using an already resolved library binding.
    /// 使用已解析的知识库绑定构建一份完整完整性快照。
    async fn inspect_index_integrity_for_binding(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
    ) -> Result<KnowledgeIndexIntegritySummary, String> {
        // Gate contains the exact persisted revision and tokenizer state.
        // Gate 包含精确持久化的版本与 tokenizer 状态。
        let gate = self.load_index_gate(space_id, binding_id).await?;
        // PresenceRows checks sqlite_master before any SQL references the optional FTS table.
        // PresenceRows 在任何 SQL 引用可选 FTS 表之前检查 sqlite_master。
        let presence_rows: Vec<IndexTablePresenceRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT COUNT(*) AS index_exists FROM sqlite_master WHERE type = 'table' AND name = ?;",
                vec![text(KNOWLEDGE_FTS_INDEX)],
            )
            .await?;
        // IndexExists is true only for one exact sqlite_master match.
        // IndexExists 仅在 sqlite_master 精确匹配一条记录时为真。
        let index_exists = presence_rows
            .into_iter()
            .next()
            .ok_or_else(|| String::from("知识库索引结构检查未返回结果。"))?
            .index_exists
            == 1;
        // Counts uses exact source-field comparison when the FTS table exists.
        // Counts 在 FTS 表存在时使用精确来源字段比较。
        let counts = if index_exists {
            self.load_index_integrity_counts(space_id, binding_id)
                .await?
        } else {
            // AuthoritativeCount remains observable even when a cancelled rebuild dropped the FTS table.
            // AuthoritativeCount 即使取消的重建已删除 FTS 表也仍可观察。
            let authoritative_count = self
                .authoritative_counts(space_id, binding_id)
                .await?
                .chunk_count;
            IndexIntegrityCountsRow {
                authoritative_count,
                indexed_count: 0,
                missing_or_mismatched_count: authoritative_count,
                orphan_or_mismatched_count: 0,
            }
        };
        // ContentRevision is validated independently from physical row diagnostics.
        // ContentRevision 独立于物理记录诊断完成校验。
        let content_revision = parse_revision(&gate.content_revision, "content_revision")?;
        // IndexedContentRevision records the authoritative revision represented by FTS.
        // IndexedContentRevision 记录 FTS 表示的权威版本。
        let indexed_content_revision =
            parse_revision(&gate.indexed_content_revision, "indexed_content_revision")?;
        // DictionaryRevision is the current library-local Jieba dictionary version.
        // DictionaryRevision 是当前知识库局部 Jieba 词典版本。
        let dictionary_revision = parse_revision(&gate.dictionary_revision, "dictionary_revision")?;
        // IndexedDictionaryRevision is the dictionary version used to tokenize existing rows.
        // IndexedDictionaryRevision 是现有记录分词时使用的词典版本。
        let indexed_dictionary_revision = parse_revision(
            &gate.indexed_dictionary_revision,
            "indexed_dictionary_revision",
        )?;
        // AuthoritativeChunkCount is checked before public unsigned projection.
        // AuthoritativeChunkCount 在投影为公共无符号值前完成检查。
        let authoritative_chunk_count =
            nonnegative_u64(counts.authoritative_count, "authoritative_count")?;
        // IndexedChunkCount is the current physical FTS row count.
        // IndexedChunkCount 是当前物理 FTS 记录数量。
        let indexed_chunk_count = nonnegative_u64(counts.indexed_count, "indexed_count")?;
        // MissingCount includes both absent rows and rows whose source fields differ.
        // MissingCount 同时包含缺失记录与来源字段不同的记录。
        let missing_or_mismatched_count = nonnegative_u64(
            counts.missing_or_mismatched_count,
            "missing_or_mismatched_count",
        )?;
        // OrphanCount includes both orphan rows and rows whose source fields differ.
        // OrphanCount 同时包含孤立记录与来源字段不同的记录。
        let orphan_or_mismatched_count = nonnegative_u64(
            counts.orphan_or_mismatched_count,
            "orphan_or_mismatched_count",
        )?;
        // Searchable combines fail-closed metadata and exact physical set equality.
        // Searchable 组合失败关闭元数据与精确物理集合相等条件。
        let searchable = index_exists
            && gate.index_status == "ready"
            && content_revision == indexed_content_revision
            && dictionary_revision == indexed_dictionary_revision
            && gate.tokenizer == "jieba"
            && authoritative_chunk_count == indexed_chunk_count
            && missing_or_mismatched_count == 0
            && orphan_or_mismatched_count == 0;
        Ok(KnowledgeIndexIntegritySummary {
            library_id: library_id.to_string(),
            index_exists,
            searchable,
            index_status: gate.index_status,
            tokenizer: gate.tokenizer,
            content_revision,
            indexed_content_revision,
            dictionary_revision,
            indexed_dictionary_revision,
            authoritative_chunk_count,
            indexed_chunk_count,
            missing_or_mismatched_count,
            orphan_or_mismatched_count,
        })
    }

    /// Loads the exact persisted metadata gate from one isolated library.
    /// 从一个隔离知识库加载精确持久化元数据门禁。
    async fn load_index_gate(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<IndexGateRow, String> {
        // Rows is a single aggregate record over all required metadata keys.
        // Rows 是覆盖全部必需元数据键的单条聚合记录。
        let rows: Vec<IndexGateRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                r#"SELECT
                    MAX(CASE WHEN key = 'index_status' THEN value END) AS index_status,
                    MAX(CASE WHEN key = 'content_revision' THEN value END) AS content_revision,
                    MAX(CASE WHEN key = 'indexed_content_revision' THEN value END) AS indexed_content_revision,
                    MAX(CASE WHEN key = 'dictionary_revision' THEN value END) AS dictionary_revision,
                    MAX(CASE WHEN key = 'indexed_dictionary_revision' THEN value END) AS indexed_dictionary_revision,
                    MAX(CASE WHEN key = 'tokenizer' THEN value END) AS tokenizer
                FROM knowledge_metadata;"#,
                Vec::new(),
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| String::from("知识库索引元数据缺失。"))
    }

    /// Verifies all index revisions and the tokenizer before allowing search.
    /// 在允许搜索前验证全部索引版本与分词器。
    async fn verify_index_gate(&self, space_id: &str, binding_id: &str) -> Result<(), String> {
        // Gate is the lightweight hot-path metadata snapshot.
        // Gate 是轻量搜索热路径元数据快照。
        let gate = self.load_index_gate(space_id, binding_id).await?;
        if gate.index_status != "ready"
            || gate.content_revision != gate.indexed_content_revision
            || gate.dictionary_revision != gate.indexed_dictionary_revision
            || gate.tokenizer != "jieba"
        {
            return Err(format!(
                "知识库索引当前不可检索：status={}, content={}/{}, dictionary={}/{}, tokenizer={}",
                gate.index_status,
                gate.indexed_content_revision,
                gate.content_revision,
                gate.indexed_dictionary_revision,
                gate.dictionary_revision,
                gate.tokenizer
            ));
        }
        Ok(())
    }

    /// Verifies that ready metadata is backed by one exact FTS row for every authoritative chunk.
    /// 验证 ready 元数据确实由每个权威切片对应的一条精确 FTS 记录支撑。
    ///
    /// # Parameters
    /// 参数
    /// `space_id` is the exact isolated controller space containing the candidate database.
    /// `space_id` 是包含候选数据库的精确隔离控制器空间。
    /// `binding_id` is the exact SQLite binding used for the integrity query.
    /// `binding_id` 是执行完整性查询的精确 SQLite 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only when metadata, row counts, identifiers, and indexed source fields all match.
    /// 仅当元数据、行数、标识符和索引源字段全部一致时返回成功。
    async fn verify_index_integrity(&self, space_id: &str, binding_id: &str) -> Result<(), String> {
        self.verify_index_gate(space_id, binding_id).await?;
        self.verify_index_rows_exact(space_id, binding_id).await
    }

    /// Verifies exact physical FTS rows without requiring ready metadata first.
    /// 在不预先要求 ready 元数据的情况下验证精确物理 FTS 记录。
    async fn verify_index_rows_exact(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<(), String> {
        // Row contains exact set and source-field diagnostics for the existing FTS table.
        // Row 包含现有 FTS 表的精确集合与来源字段诊断。
        let row = self
            .load_index_integrity_counts(space_id, binding_id)
            .await?;
        if row.authoritative_count < 0
            || row.indexed_count < 0
            || row.authoritative_count != row.indexed_count
            || row.missing_or_mismatched_count != 0
            || row.orphan_or_mismatched_count != 0
        {
            return Err(format!(
                "知识库 FTS 与权威切片不一致：authoritative={}, indexed={}, missing_or_mismatched={}, orphan_or_mismatched={}",
                row.authoritative_count,
                row.indexed_count,
                row.missing_or_mismatched_count,
                row.orphan_or_mismatched_count
            ));
        }
        Ok(())
    }

    /// Loads exact physical FTS set and source-field diagnostics for an existing table.
    /// 为一个已存在表加载精确物理 FTS 集合与来源字段诊断。
    async fn load_index_integrity_counts(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<IndexIntegrityCountsRow, String> {
        // Rows combines EXCEPT and INTERSECT set comparisons so duplicate physical rows remain observable.
        // Rows 组合 EXCEPT 与 INTERSECT 集合比较，使重复物理记录仍然可被观察。
        let rows: Vec<IndexIntegrityCountsRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                &format!(
                    r#"SELECT
                        (SELECT COUNT(*) FROM knowledge_chunks) AS authoritative_count,
                        (SELECT COUNT(*) FROM {KNOWLEDGE_FTS_INDEX}) AS indexed_count,
                        (SELECT COUNT(*) FROM (
                            SELECT c.id,
                                   d.relative_path AS file_path,
                                   CASE WHEN c.heading_path = ''
                                        THEN d.title
                                        ELSE d.title || ' / ' || c.heading_path END AS title,
                                   c.content
                              FROM knowledge_chunks c
                              INNER JOIN knowledge_documents d ON d.id = c.document_id
                            EXCEPT
                            SELECT id, file_path, title, content FROM {KNOWLEDGE_FTS_INDEX}
                        )) AS missing_or_mismatched_count,
                        (SELECT COUNT(*) FROM {KNOWLEDGE_FTS_INDEX}) -
                        (SELECT COUNT(*) FROM (
                            SELECT id, file_path, title, content FROM {KNOWLEDGE_FTS_INDEX}
                            INTERSECT
                            SELECT c.id,
                                   d.relative_path AS file_path,
                                   CASE WHEN c.heading_path = ''
                                        THEN d.title
                                        ELSE d.title || ' / ' || c.heading_path END AS title,
                                   c.content
                              FROM knowledge_chunks c
                              INNER JOIN knowledge_documents d ON d.id = c.document_id
                        )) AS orphan_or_mismatched_count;"#,
                ),
                Vec::new(),
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| String::from("知识库 FTS 完整性检查未返回结果。"))
    }

    /// Marks FTS revisions equal to authoritative revisions after successful replay.
    /// 在成功回放后将 FTS 版本标记为等同权威版本。
    async fn mark_index_ready(&self, space_id: &str, binding_id: &str) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                r#"UPDATE knowledge_metadata SET value = (SELECT value FROM knowledge_metadata WHERE key = 'content_revision') WHERE key = 'indexed_content_revision';
                    UPDATE knowledge_metadata SET value = (SELECT value FROM knowledge_metadata WHERE key = 'dictionary_revision') WHERE key = 'indexed_dictionary_revision';
                    UPDATE knowledge_metadata SET value = 'jieba' WHERE key = 'tokenizer';
                    UPDATE knowledge_metadata SET value = 'ready' WHERE key = 'index_status';
                    UPDATE knowledge_metadata SET value = '' WHERE key = 'last_index_error';"#,
                Vec::new(),
            )
            .await
            .map_err(|error| format!("提交知识库索引版本失败：{error}"))?;
        Ok(())
    }

    /// Increments the dictionary revision and makes search fail closed until rebuild succeeds.
    /// 递增词典版本，并在重建成功前让检索保持关闭。
    async fn mark_dictionary_dirty(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                "UPDATE knowledge_metadata SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) WHERE key = 'dictionary_revision'; UPDATE knowledge_metadata SET value = 'dirty' WHERE key = 'index_status';",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("更新知识库词典版本失败：{error}"))?;
        self.set_registry_index_status(library_id, "dirty").await
    }

    /// Loads one authoritative joined chunk or returns an explicit integrity error.
    /// 加载一个权威联结切片，若不存在则返回显式完整性错误。
    async fn load_joined_chunk(
        &self,
        space_id: &str,
        binding_id: &str,
        chunk_id: &str,
    ) -> Result<JoinedChunkRow, String> {
        let rows: Vec<JoinedChunkRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                r#"SELECT c.id, c.document_id, c.ordinal, c.heading_path, c.start_line, c.end_line,
                           c.content, c.estimated_tokens, d.title, d.relative_path
                    FROM knowledge_chunks c
                    INNER JOIN knowledge_documents d ON d.id = c.document_id
                    WHERE c.id = ?;"#,
                vec![text(chunk_id)],
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| format!("FTS 命中的权威切片不存在：{chunk_id}"))
    }

    /// Loads authoritative joined chunks for one exact bounded identifier set.
    /// 为一个精确且有界的标识符集合批量加载权威联结切片。
    async fn load_joined_chunks_by_ids(
        &self,
        space_id: &str,
        binding_id: &str,
        chunk_ids: &[String],
    ) -> Result<Vec<JoinedChunkRow>, String> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        // placeholders contains one positional parameter for each validated identifier.
        // placeholders 为每个已验证标识符包含一个位置参数。
        let placeholders = std::iter::repeat_n("?", chunk_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        // sql interpolates only the generated placeholder list; all identifiers remain bound values.
        // sql 只插入生成的占位符列表，所有标识符仍作为绑定值传入。
        let sql = format!(
            r#"SELECT c.id, c.document_id, c.ordinal, c.heading_path, c.start_line, c.end_line,
                      c.content, c.estimated_tokens, d.title, d.relative_path
               FROM knowledge_chunks c
               INNER JOIN knowledge_documents d ON d.id = c.document_id
               WHERE c.id IN ({placeholders});"#,
        );
        // params preserves the exact identifier set as typed SQLite text values.
        // params 将精确标识符集合保存为类型化 SQLite 文本值。
        let params = chunk_ids.iter().map(text).collect();
        self.query_library_rows(space_id, binding_id, &sql, params)
            .await
    }

    /// Returns authoritative document and chunk counts.
    /// 返回权威文档与切片数量。
    async fn authoritative_counts(
        &self,
        space_id: &str,
        binding_id: &str,
    ) -> Result<CountRow, String> {
        let rows: Vec<CountRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                "SELECT (SELECT COUNT(*) FROM knowledge_documents) AS document_count, (SELECT COUNT(*) FROM knowledge_chunks) AS chunk_count;",
                Vec::new(),
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| String::from("读取知识库权威计数失败。"))
    }

    /// Counts committed chunks whose document identities are present in the current staging set.
    /// 统计文档身份存在于当前暂存集合中的已提交切片。
    ///
    /// Parameters: `space_id` and `binding_id` select the exact isolated library database.
    /// Returns: a validated non-negative changed-chunk count.
    /// 参数：`space_id` 与 `binding_id` 选择精确隔离知识库数据库。返回值：已校验的非负变更切片数。
    async fn staged_chunk_count(&self, space_id: &str, binding_id: &str) -> Result<u64, String> {
        // Rows performs one aggregate over the authoritative/staging identity join.
        // Rows 对权威记录与暂存身份联结执行一次聚合。
        let rows: Vec<ChunkCountRow> = self
            .query_library_rows(
                space_id,
                binding_id,
                r#"SELECT COUNT(c.id) AS chunk_count
                     FROM knowledge_chunks c
                     INNER JOIN knowledge_staged_documents s ON s.id = c.document_id;"#,
                Vec::new(),
            )
            .await?;
        // Row is mandatory because SQLite aggregate queries always return one record.
        // Row 是必需值，因为 SQLite 聚合查询始终返回一条记录。
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| String::from("读取知识库变更切片计数失败。"))?;
        nonnegative_u64(row.chunk_count, "chunk_count")
    }

    /// Projects authoritative counts and index state without advancing the last successful import time.
    /// 投影权威计数与索引状态，但不推进最近一次成功导入时间。
    ///
    /// Parameters: `library_id` selects the registry row, `counts` contains authoritative totals,
    /// `index_status` is the exact derived-index state, and `updated_at_ms` is the projection time.
    /// Returns: success after counts are visible without changing the last successful import time.
    /// 参数：`library_id` 选择注册记录，`counts` 包含权威总数，`index_status` 是精确派生索引状态，
    /// `updated_at_ms` 是投影时间。返回值：计数可见且不改变最近成功导入时间后成功。
    async fn project_registry_authoritative_counts(
        &self,
        library_id: &str,
        counts: &CountRow,
        index_status: &str,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_libraries SET document_count = ?, chunk_count = ?, index_status = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    int(counts.document_count),
                    int(counts.chunk_count),
                    text(index_status),
                    int(updated_at_ms),
                    text(library_id),
                ],
            )
            .await
            .map_err(|error| format!("更新知识库权威摘要失败：{error}"))?;
        Ok(())
    }

    /// Projects authoritative counts and terminal index state into the central registry.
    /// 将权威计数与终态索引状态投影到中央注册表。
    async fn project_registry_counts(
        &self,
        library_id: &str,
        counts: &CountRow,
        index_status: &str,
        completed_at_ms: i64,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_libraries SET document_count = ?, chunk_count = ?, index_status = ?, last_completed_import_at_ms = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
                vec![
                    int(counts.document_count),
                    int(counts.chunk_count),
                    text(index_status),
                    int(completed_at_ms),
                    int(completed_at_ms),
                    text(library_id),
                ],
            )
            .await
            .map_err(|error| format!("更新知识库中央摘要失败：{error}"))?;
        Ok(())
    }

    /// Sets matching per-library and central index states with an explicit error.
    /// 使用显式错误设置一致的单库与中央索引状态。
    async fn set_library_index_state(
        &self,
        library_id: &str,
        space_id: &str,
        binding_id: &str,
        state: &str,
        error: &str,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                "UPDATE knowledge_metadata SET value = CASE key WHEN 'index_status' THEN ? WHEN 'last_index_error' THEN ? END WHERE key IN ('index_status', 'last_index_error');",
                vec![text(state), text(error)],
            )
            .await
            .map_err(|failure| format!("更新知识库索引状态失败：{failure}"))?;
        self.set_registry_index_status(library_id, state).await
    }

    /// Updates only the central registry index state.
    /// 仅更新中央注册表索引状态。
    async fn set_registry_index_status(&self, library_id: &str, state: &str) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_libraries SET index_status = ?, updated_at_ms = ? WHERE id = ?;",
                vec![text(state), int(unix_epoch_millis()?), text(library_id)],
            )
            .await
            .map_err(|error| format!("更新知识库中央索引状态失败：{error}"))?;
        Ok(())
    }

    /// Finds one registry row by its exact normalized user-visible name.
    /// 按精确规范化用户可见名称查找一条注册记录。
    async fn find_library_by_normalized_name(
        &self,
        name: &str,
        excluded_library_id: Option<&str>,
    ) -> Result<Option<KnowledgeLibrarySummary>, String> {
        let (predicate, params) = if let Some(excluded_library_id) = excluded_library_id {
            (
                "WHERE name_key = ? AND id <> ?",
                vec![text(library_name_key(name)), text(excluded_library_id)],
            )
        } else {
            ("WHERE name_key = ?", vec![text(library_name_key(name))])
        };
        let sql = format!(
            r#"SELECT id, name, description, lifecycle_state, document_count, chunk_count,
                      index_status, last_completed_import_at_ms, created_at_ms, updated_at_ms
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
        self.controller
            .client
            .execute_sqlite_script_typed(
                &space_id,
                &binding_id,
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
        self.controller
            .client
            .execute_sqlite_script_typed(
                &space_id,
                &binding_id,
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
    async fn cleanup_failed_package_install(&self, library_id: &str) -> Result<(), String> {
        self.release_library_binding(library_id).await?;
        let root = self.library_root(library_id)?;
        self.remove_host_owned_child_directory_if_exists(&root, library_id)
    }

    /// Inserts one imported library into the central registry after physical validation succeeds.
    /// 在物理校验成功后将一个导入知识库写入中央注册表。
    async fn insert_imported_library_registry(
        &self,
        library_id: &str,
        name: &str,
        package: &KnowledgePackageManifest,
        now: i64,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_batch_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                r#"INSERT INTO knowledge_libraries(
                    id, name, name_key, description, lifecycle_state, document_count,
                    chunk_count, index_status, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, 'active', ?, ?, 'ready', ?, ?);"#,
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
    async fn update_replaced_library_registry(
        &self,
        library_id: &str,
        name: &str,
        package: &KnowledgePackageManifest,
    ) -> Result<(), String> {
        let now = unix_epoch_millis()?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                r#"UPDATE knowledge_libraries SET name = ?, name_key = ?, description = ?,
                    lifecycle_state = 'active', document_count = ?, chunk_count = ?,
                    index_status = 'ready', last_completed_import_at_ms = NULL, updated_at_ms = ?
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
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                r#"UPDATE knowledge_libraries SET name = ?, name_key = ?, description = ?,
                    lifecycle_state = ?, document_count = ?, chunk_count = ?, index_status = ?,
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
                    previous
                        .last_completed_import_at_ms
                        .map_or(ControllerSqliteValue::Null, int),
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

    /// Ensures the central registry row exists and is active.
    /// 确保中央注册记录存在且处于活动状态。
    async fn require_active_library(
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

    /// Attaches one stable user space and enables its isolated SQLite binding.
    /// 附着一个稳定用户空间并启用其隔离 SQLite 绑定。
    async fn ensure_library_binding(&self, library_id: &str) -> Result<(String, String), String> {
        let identities = self.activate_library_binding(library_id, false).await?;
        self.validate_bound_library_metadata(library_id, &identities.0, &identities.1)
            .await?;
        Ok(identities)
    }

    /// Activates one controller binding exactly once for the lifetime of this runtime handle.
    /// 在当前运行时句柄生命周期内仅激活一次控制器绑定。
    async fn activate_library_binding(
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

    /// Disables and forgets one active controller binding after failed creation cleanup.
    /// 在创建失败清理后停用并遗忘一个活动控制器绑定。
    async fn release_library_binding(&self, library_id: &str) -> Result<(), String> {
        let _activation_guard = self.binding_activation_lock.lock().await;
        if !self.active_bindings.lock().await.contains(library_id) {
            return Ok(());
        }
        let (space_id, binding_id) = self.library_binding_identity(library_id)?;
        self.controller
            .client
            .disable_sqlite(&space_id, &binding_id)
            .await
            .map_err(|error| format!("停用知识库 SQLite 绑定失败：{error}"))?;
        self.active_bindings.lock().await.remove(library_id);
        Ok(())
    }

    /// Validates storage artifacts and enables one isolated SQLite binding.
    /// 校验存储产物并启用一个隔离 SQLite 绑定。
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
    async fn validate_bound_library_metadata(
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
    async fn verify_sqlite_quick_check(
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
    fn library_binding_identity(&self, library_id: &str) -> Result<(String, String), String> {
        validate_library_id(library_id)?;
        Ok((
            format!("knowledge-{library_id}"),
            format!(
                "knowledge-{}-{library_id}",
                self.controller.sqlite_binding_id
            ),
        ))
    }

    /// Returns the immutable knowledge root under the established application data root.
    /// 返回已建立应用数据根目录下的不可变知识库根目录。
    fn knowledge_root(&self) -> PathBuf {
        self.controller.data_root.join("knowledge-bases")
    }

    /// Derives and validates one physical library root.
    /// 派生并验证一个物理知识库根目录。
    fn library_root(&self, library_id: &str) -> Result<PathBuf, String> {
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
    fn remove_host_owned_child_directory_if_exists(
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
    fn replacement_operation_artifacts(
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
    fn write_replacement_journal_atomic(
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

    /// Writes one manifest through a same-directory atomic replacement.
    /// 通过同目录原子替换写入一份清单。
    fn write_manifest_atomic(&self, manifest: &KnowledgeManifest) -> Result<(), String> {
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

    /// Returns the per-library mutation guard after validating its UUID.
    /// 在验证 UUID 后返回每知识库变更锁守卫。
    async fn library_mutation_guard(
        &self,
        library_id: &str,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
        validate_library_id(library_id)?;
        let lock = {
            let mut locks = self.mutation_locks.lock().await;
            locks
                .entry(library_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        Ok(lock.lock_owned().await)
    }

    /// Queries typed rows from the central application database.
    /// 从中央应用数据库查询类型化记录。
    async fn query_app_rows<T: DeserializeOwned>(
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
    async fn query_library_rows<T: DeserializeOwned>(
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
    async fn execute_library_batch(
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

    /// Reserves one library operation in memory and persists its queued durable row.
    /// 在内存中预留一个知识库操作，并持久化其排队记录。
    ///
    /// Parameters: `library_id` selects the exact library, `operation_kind` defines execution
    /// semantics, and `source_root` is present only for Markdown imports.
    /// Returns: the prepared identity consumed by either a synchronous or detached executor.
    /// 参数：`library_id` 选择精确知识库，`operation_kind` 定义执行语义，`source_root` 仅在 Markdown
    /// 导入时存在。返回值：由同步或分离执行器消费的预留身份。
    async fn prepare_operation(
        &self,
        library_id: &str,
        operation_kind: KnowledgeOperationKind,
        source_root: Option<&Path>,
    ) -> Result<PreparedKnowledgeOperation, String> {
        validate_library_id(library_id)?;
        // JobId is generated before reservation so every diagnostic names the exact contender.
        // JobId 在预留前生成，使每条诊断都能命名精确竞争任务。
        let job_id = Uuid::new_v4().to_string();
        // Cancellation is shared only by this operation and its executor.
        // Cancellation 仅由本操作及其执行器共享。
        let cancellation = Arc::new(AtomicBool::new(false));
        // Operation captures every immutable value required after request completion.
        // Operation 捕获请求完成后所需的全部不可变值。
        let operation = PreparedKnowledgeOperation {
            job_id: job_id.clone(),
            library_id: library_id.to_string(),
            operation_kind,
            cancellation: Arc::clone(&cancellation),
            created_at_ms: unix_epoch_millis()?,
        };
        {
            // Operations closes the duplicate-start race before the durable insert awaits I/O.
            // Operations 在持久化插入等待 I/O 前关闭重复启动竞态。
            let mut operations = self.active_operations.lock().await;
            if let Some(active) = operations.get(library_id) {
                return Err(format!("知识库已有运行中的操作：job_id={}", active.job_id));
            }
            operations.insert(
                library_id.to_string(),
                ActiveKnowledgeOperation {
                    job_id,
                    signal: cancellation,
                },
            );
        }
        if let Err(error) = self.insert_operation_job(&operation, source_root).await {
            self.release_active_operation(library_id, &operation.job_id)
                .await;
            return Err(error);
        }
        Ok(operation)
    }

    /// Inserts one queued durable operation row using a stable kind and optional import source.
    /// 使用稳定类型和可选导入来源写入一条排队中的持久化操作记录。
    ///
    /// Parameters: `operation` owns immutable identity and timestamps; `source_root` exists only
    /// for an import. Returns: success after the central transaction commits.
    /// 参数：`operation` 持有不可变身份与时间戳；`source_root` 仅在导入时存在。返回值：中央事务提交后成功。
    async fn insert_operation_job(
        &self,
        operation: &PreparedKnowledgeOperation,
        source_root: Option<&Path>,
    ) -> Result<(), String> {
        // SourceRootText is empty for index-only operations and never doubles as a type marker.
        // SourceRootText 在仅索引操作中为空，且绝不兼作类型标记。
        let source_root_text = source_root
            .map(runtime_assets::native_host_path_text_from_path)
            .unwrap_or_default();
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                r#"INSERT INTO knowledge_import_jobs(
                    id, library_id, source_root, operation_kind, state, phase, created_at_ms
                ) VALUES (?, ?, ?, ?, 'queued', 'queued', ?);"#,
                vec![
                    text(&operation.job_id),
                    text(&operation.library_id),
                    text(source_root_text),
                    text(operation.operation_kind.as_str()),
                    int(operation.created_at_ms),
                ],
            )
            .await
            .map_err(|error| format!("创建知识库操作任务失败：{error}"))?;
        Ok(())
    }

    /// Releases an in-memory reservation only when the exact job still owns it.
    /// 仅当精确任务仍持有预留时释放内存预留。
    ///
    /// Parameters: `library_id` selects the reservation and `job_id` prevents stale cleanup.
    /// Returns: nothing after the conditional removal completes.
    /// 参数：`library_id` 选择预留，`job_id` 防止过期清理。返回值：条件移除完成后无返回内容。
    async fn release_active_operation(&self, library_id: &str, job_id: &str) {
        // Operations is held only for the exact identity comparison and removal.
        // Operations 仅在精确身份比较与移除期间持有。
        let mut operations = self.active_operations.lock().await;
        if operations
            .get(library_id)
            .is_some_and(|active| active.job_id == job_id)
        {
            operations.remove(library_id);
        }
    }

    /// Scans Markdown candidates on a blocking worker while persisting throttled discovery progress.
    /// 在阻塞工作器中扫描 Markdown 候选文件，同时持久化节流后的发现进度。
    async fn scan_markdown_files_with_progress(
        &self,
        job_id: &str,
        root: &Path,
        cancellation: &Arc<AtomicBool>,
    ) -> Result<Vec<ScannedMarkdownFile>, String> {
        // ScanRoot moves the exact canonical root into the blocking filesystem worker.
        // ScanRoot 将准确的规范根目录移入阻塞文件系统工作器。
        let scan_root = root.to_path_buf();
        // ScanCancellation shares the exact cooperative cancellation flag with the worker.
        // ScanCancellation 与工作器共享准确的协作取消标志。
        let scan_cancellation = Arc::clone(cancellation);
        // ProgressSender and ProgressReceiver bridge bounded worker snapshots into async persistence.
        // ProgressSender 与 ProgressReceiver 将有界工作器快照桥接到异步持久化。
        let (progress_sender, mut progress_receiver) = mpsc::unbounded_channel();
        // ScanTask keeps synchronous directory traversal off the async controller executor.
        // ScanTask 将同步目录遍历移出异步控制器执行器。
        let scan_task = tokio::task::spawn_blocking(move || {
            scan_markdown_files(&scan_root, &scan_cancellation, Some(&progress_sender))
        });
        tokio::pin!(scan_task);
        // ProgressChannelOpen prevents a closed progress channel from forming a busy select loop.
        // ProgressChannelOpen 防止已关闭的进度通道形成空转 select 循环。
        let mut progress_channel_open = true;

        loop {
            tokio::select! {
                scan_result = &mut scan_task => {
                    return scan_result
                        .map_err(|error| format!("知识库目录扫描工作器失败：{error}"))?;
                }
                progress = progress_receiver.recv(), if progress_channel_open => {
                    match progress {
                        Some(progress) => {
                            self.update_import_scan_progress(
                                job_id,
                                progress.discovered_files,
                                &progress.current_path,
                            )
                            .await?;
                        }
                        None => progress_channel_open = false,
                    }
                }
            }
        }
    }

    /// Persists one discovery snapshot before the final Markdown total is known.
    /// 在 Markdown 最终总数尚未确定时持久化一份发现快照。
    async fn update_import_scan_progress(
        &self,
        job_id: &str,
        discovered_files: u64,
        current_path: &str,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_import_jobs SET scanned_files = ?, current_file = ? WHERE id = ? AND state = 'running';",
                vec![int(discovered_files as i64), text(current_path), text(job_id)],
            )
            .await
            .map_err(|error| format!("更新知识库扫描进度失败：{error}"))?;
        Ok(())
    }

    /// Publishes the exact total and transitions a completed scan into file processing.
    /// 发布准确总数，并将已完成扫描转入文件处理阶段。
    async fn prepare_import_processing(
        &self,
        job_id: &str,
        total_files: u64,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_import_jobs SET state = 'running', phase = 'parsing', scanned_files = ?, total_files = ?, processed_files = 0, current_file = NULL WHERE id = ?;",
                vec![int(total_files as i64), int(total_files as i64), text(job_id)],
            )
            .await
            .map_err(|error| format!("发布知识库文件总数失败：{error}"))?;
        Ok(())
    }

    /// Updates the durable state and phase of one import or index-maintenance operation.
    /// 更新一个导入或索引维护操作的持久化状态与阶段。
    async fn update_operation_phase(
        &self,
        job_id: &str,
        state: &str,
        phase: &str,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_import_jobs SET state = ?, phase = ?, started_at_ms = CASE WHEN ? = 'running' THEN COALESCE(started_at_ms, ?) ELSE started_at_ms END WHERE id = ?;",
                vec![
                    text(state),
                    text(phase),
                    text(state),
                    int(unix_epoch_millis()?),
                    text(job_id),
                ],
            )
            .await
            .map_err(|error| format!("更新知识库操作阶段失败：{error}"))?;
        Ok(())
    }

    /// Persists bounded import counters without refreshing the full workbench snapshot.
    /// 持久化有界导入计数且不刷新完整工作台快照。
    async fn update_import_progress(
        &self,
        job_id: &str,
        processed_files: u64,
        processed_bytes: u64,
        current_file: Option<&str>,
        warnings: &[String],
    ) -> Result<(), String> {
        let warnings_json = serde_json::to_string(warnings)
            .map_err(|error| format!("序列化知识库导入进度告警失败：{error}"))?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_import_jobs SET processed_files = ?, processed_bytes = ?, current_file = ?, warnings_json = ? WHERE id = ? AND state = 'running';",
                vec![
                    int(processed_files as i64),
                    int(processed_bytes as i64),
                    current_file.map_or(ControllerSqliteValue::Null, text),
                    text(warnings_json),
                    text(job_id),
                ],
            )
            .await
            .map_err(|error| format!("更新知识库导入进度失败：{error}"))?;
        Ok(())
    }

    /// Persists one exact indexing batch boundary for UI progress and cancellation feedback.
    /// 为 UI 进度与取消反馈持久化一个准确的索引批次边界。
    async fn update_import_index_progress(
        &self,
        job_id: &str,
        processed_items: u64,
        total_items: u64,
        current_file: Option<&str>,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_import_jobs SET processed_index_items = ?, total_index_items = ?, current_file = ? WHERE id = ? AND state = 'running';",
                vec![
                    int(processed_items as i64),
                    int(total_items as i64),
                    current_file.map_or(ControllerSqliteValue::Null, text),
                    text(job_id),
                ],
            )
            .await
            .map_err(|error| format!("更新知识库索引进度失败：{error}"))?;
        Ok(())
    }

    /// Marks one import completed and persists bounded counters and warnings.
    /// 将一次导入标记为完成并持久化有界计数与告警。
    async fn finish_import_job(
        &self,
        report: &KnowledgeImportReport,
        error: Option<&str>,
    ) -> Result<(), String> {
        let now = unix_epoch_millis()?;
        let warnings_json = serde_json::to_string(&report.warnings)
            .map_err(|failure| format!("序列化知识库导入告警失败：{failure}"))?;
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                r#"UPDATE knowledge_import_jobs SET
                    state = 'completed', phase = 'completed', scanned_files = ?, total_files = ?, processed_files = ?, current_file = NULL, changed_files = ?,
                    unchanged_files = ?, deleted_files = ?, skipped_files = ?, processed_bytes = ?, warnings_json = ?,
                    error_message = ?, finished_at_ms = ? WHERE id = ?;"#,
                vec![
                    int(report.scanned_files as i64),
                    int(report.scanned_files as i64),
                    int(report.scanned_files as i64),
                    int(report.changed_files as i64),
                    int(report.unchanged_files as i64),
                    int(report.deleted_files as i64),
                    int(report.skipped_files as i64),
                    int(report.processed_bytes as i64),
                    text(warnings_json),
                    error.map_or(ControllerSqliteValue::Null, text),
                    int(now),
                    text(&report.job_id),
                ],
            )
            .await
            .map_err(|failure| format!("完成知识库导入任务失败：{failure}"))?;
        Ok(())
    }

    /// Marks one repair or rebuild completed with its exact authoritative work total.
    /// 使用精确权威工作总量将一次补全或重建标记为完成。
    ///
    /// Parameters: `job_id` selects the durable row and `processed_items` is the verified chunk total.
    /// Returns: success after the terminal state commits.
    /// 参数：`job_id` 选择持久化记录，`processed_items` 是已验证切片总量。返回值：终态提交后成功。
    async fn finish_index_operation_job(
        &self,
        job_id: &str,
        processed_items: u64,
    ) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_import_jobs SET state = 'completed', phase = 'completed', processed_index_items = ?, total_index_items = ?, current_file = NULL, error_message = NULL, finished_at_ms = ? WHERE id = ?;",
                vec![
                    int(processed_items as i64),
                    int(processed_items as i64),
                    int(unix_epoch_millis()?),
                    text(job_id),
                ],
            )
            .await
            .map_err(|failure| format!("完成知识库索引操作失败：{failure}"))?;
        Ok(())
    }

    /// Marks one import or index-maintenance operation failed with an explicit bounded error.
    /// 使用显式有界错误将一次导入或索引维护操作标记为失败。
    async fn finish_failed_operation_job(&self, job_id: &str, error: &str) -> Result<(), String> {
        let bounded = truncate_chars(error, MAX_WARNING_CHARS);
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_import_jobs SET state = 'failed', error_message = ?, finished_at_ms = ? WHERE id = ?;",
                vec![text(bounded), int(unix_epoch_millis()?), text(job_id)],
            )
            .await
            .map_err(|failure| format!("记录知识库操作失败状态失败：{failure}"))?;
        Ok(())
    }

    /// Marks one user-cancelled import or index operation with a distinct durable terminal state.
    /// 使用独立持久化终态标记一次由用户取消的导入或索引操作。
    async fn finish_cancelled_operation_job(&self, job_id: &str) -> Result<(), String> {
        self.controller
            .client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                self.controller.sqlite_binding_id.as_str(),
                "UPDATE knowledge_import_jobs SET state = 'cancelled', phase = 'cancelled', error_message = NULL, finished_at_ms = ? WHERE id = ?;",
                vec![int(unix_epoch_millis()?), text(job_id)],
            )
            .await
            .map_err(|failure| format!("记录知识库操作取消状态失败：{failure}"))?;
        Ok(())
    }
}

/// Builds the exact queued browser snapshot returned before detached execution begins.
/// 构建分离执行开始前返回的精确排队浏览器快照。
///
/// Parameters: `operation` is the immutable prepared operation identity.
/// Returns: a zero-progress queued snapshot with the same durable identifiers.
/// 参数：`operation` 是不可变预留操作身份。返回值：具有相同持久身份的零进度排队快照。
fn queued_operation_summary(operation: &PreparedKnowledgeOperation) -> KnowledgeImportJobSummary {
    KnowledgeImportJobSummary {
        id: operation.job_id.clone(),
        library_id: operation.library_id.clone(),
        operation_kind: operation.operation_kind,
        state: String::from("queued"),
        phase: String::from("queued"),
        scanned_files: 0,
        total_files: 0,
        processed_files: 0,
        total_index_items: 0,
        processed_index_items: 0,
        current_file: None,
        processed_bytes: 0,
        warnings: Vec::new(),
        error_message: None,
    }
}

/// Atomically persists one securely created temporary file over a stable destination.
/// 将一个安全创建的临时文件原子持久化并覆盖稳定目标。
///
/// # Parameters
/// 参数
/// `temporary` is a fully written and synchronized same-directory temporary file.
/// `temporary` 是已完整写入并同步的同目录临时文件。
/// `destination` is the stable manifest path to create or replace.
/// `destination` 是要创建或替换的稳定清单路径。
///
/// # Returns
/// 返回值
/// Returns success only after the platform-specific atomic persistence operation succeeds.
/// 仅在平台专属原子持久化操作成功后返回成功。
fn persist_tempfile_atomically(
    temporary: tempfile::NamedTempFile,
    destination: &Path,
) -> std::io::Result<()> {
    temporary
        .persist(destination)
        .map(|_| ())
        .map_err(|error| error.error)
}

/// Recursively scans a canonical root without following symlinks or reparse entries.
/// 在不跟随符号链接或重解析项的前提下递归扫描规范根目录。
fn scan_markdown_files(
    root: &Path,
    cancellation: &AtomicBool,
    progress_sender: Option<&mpsc::UnboundedSender<ScanProgressSnapshot>>,
) -> Result<Vec<ScannedMarkdownFile>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    // LastProgressAt throttles durable UI updates without hiding long filesystem stalls.
    // LastProgressAt 对持久化 UI 更新进行节流，但不隐藏长时间文件系统停顿。
    let mut last_progress_at = Instant::now()
        .checked_sub(Duration::from_millis(250))
        .unwrap_or_else(Instant::now);
    // LastVisitedPath retains the latest real path for the mandatory terminal discovery update.
    // LastVisitedPath 保留最新真实路径，用于必须发送的终态发现更新。
    let mut last_visited_path = String::from(".");
    while let Some(directory) = pending.pop() {
        require_operation_not_cancelled(cancellation)?;
        last_visited_path = import_progress_path(root, &directory);
        if last_progress_at.elapsed() >= Duration::from_millis(100) {
            publish_scan_progress(progress_sender, files.len() as u64, &last_visited_path);
            last_progress_at = Instant::now();
        }
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("读取导入目录失败（{}）：{error}", directory.display()))?;
        for entry in entries {
            require_operation_not_cancelled(cancellation)?;
            let entry = entry.map_err(|error| format!("读取导入目录项失败：{error}"))?;
            let entry_metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| format!("读取导入目录项类型失败：{error}"))?;
            if metadata_is_link_or_reparse(&entry_metadata) {
                continue;
            }
            let file_type = entry_metadata.file_type();
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if !file_type.is_file()
                || !path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            if files.len() >= MAX_IMPORT_FILES {
                return Err(format!("Markdown 文件数量超过上限 {MAX_IMPORT_FILES}。"));
            }
            if entry_metadata.len() > MAX_MARKDOWN_FILE_BYTES {
                return Err(format!(
                    "Markdown 文件超过单文件上限 {} 字节：{}",
                    MAX_MARKDOWN_FILE_BYTES,
                    path.display()
                ));
            }
            total_bytes = total_bytes.saturating_add(entry_metadata.len());
            if total_bytes > MAX_IMPORT_BYTES {
                return Err(format!(
                    "Markdown 导入总字节数超过上限 {MAX_IMPORT_BYTES}。"
                ));
            }
            // Relative is derived from a path emitted by read_dir under the canonical root.
            // Relative 来自规范根目录下 read_dir 直接产生的路径。
            let relative = path.strip_prefix(root).map_err(|error| {
                format!("计算 Markdown 相对路径失败（{}）：{error}", path.display())
            })?;
            let relative_path = slash_path(relative)?;
            last_visited_path.clone_from(&relative_path);
            let folder_path = relative
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(slash_path)
                .transpose()?
                .unwrap_or_default();
            files.push(ScannedMarkdownFile {
                relative_path_key: relative_path.to_lowercase(),
                relative_path,
                folder_path,
                source_path: path,
                byte_size: entry_metadata.len(),
                modified_at_ms: entry_metadata.modified().ok().and_then(system_time_millis),
            });
            if last_progress_at.elapsed() >= Duration::from_millis(100) {
                publish_scan_progress(progress_sender, files.len() as u64, &last_visited_path);
                last_progress_at = Instant::now();
            }
        }
    }
    publish_scan_progress(progress_sender, files.len() as u64, &last_visited_path);
    files.sort_by(|left, right| left.relative_path_key.cmp(&right.relative_path_key));
    if let Some(duplicate) = files
        .windows(2)
        .find(|pair| pair[0].relative_path_key == pair[1].relative_path_key)
    {
        return Err(format!(
            "Markdown 相对路径在大小写规范化后冲突：{} 与 {}",
            duplicate[0].relative_path, duplicate[1].relative_path
        ));
    }
    Ok(files)
}

/// Publishes one best-effort scan snapshot without coupling filesystem work to UI lifetime.
/// 发布一份尽力而为的扫描快照，不将文件系统工作与 UI 生命周期耦合。
fn publish_scan_progress(
    progress_sender: Option<&mpsc::UnboundedSender<ScanProgressSnapshot>>,
    discovered_files: u64,
    current_path: &str,
) {
    if let Some(progress_sender) = progress_sender {
        let _ = progress_sender.send(ScanProgressSnapshot {
            discovered_files,
            current_path: current_path.to_string(),
        });
    }
}

/// Formats one visited filesystem path relative to the authorized import root for display only.
/// 仅为展示目的，将一个已访问文件系统路径格式化为相对于已授权导入根目录的路径。
fn import_progress_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if relative.as_os_str().is_empty() {
        String::from(".")
    } else {
        relative.to_string_lossy().replace('\\', "/")
    }
}

/// Rejects continued import work after the active job receives cancellation.
/// 在活动任务收到取消请求后拒绝继续执行导入工作。
fn require_operation_not_cancelled(cancellation: &AtomicBool) -> Result<(), String> {
    if cancellation.load(Ordering::Acquire) {
        Err(String::from(KNOWLEDGE_OPERATION_CANCELLED_ERROR))
    } else {
        Ok(())
    }
}

/// Returns whether metadata identifies a symbolic link or a Windows reparse point.
/// 返回元数据是否标识符号链接或 Windows 重解析点。
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
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

/// Canonicalizes and validates an import root directory.
/// 规范化并验证一个导入根目录。
fn canonical_import_root(root: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(root)
        .map_err(|error| format!("规范化导入目录失败（{}）：{error}", root.display()))?;
    if !canonical.is_dir() {
        return Err(format!("导入路径不是目录：{}", canonical.display()));
    }
    Ok(canonical)
}

/// Converts a relative host path to the database slash convention.
/// 将相对宿主路径转换为数据库斜杠约定。
fn slash_path(path: &Path) -> Result<String, String> {
    let parts: Result<Vec<&str>, String> = path
        .components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .ok_or_else(|| format!("资料路径不是有效 UTF-8：{}", path.display())),
            _ => Err(format!("资料相对路径包含非法组件：{}", path.display())),
        })
        .collect();
    parts.map(|parts| parts.join("/"))
}

/// Validates and normalizes a user-visible library name.
/// 验证并规范化用户可见知识库名称。
fn validate_library_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(String::from("知识库名称不能为空。"));
    }
    if trimmed.chars().count() > 120 {
        return Err(String::from("知识库名称不能超过 120 个字符。"));
    }
    Ok(trimmed.to_string())
}

/// Validates one custom Jieba word without changing its exact text.
/// 校验一个 Jieba 自定义词且不改变其精确文本。
fn validate_custom_word(word: &str) -> Result<&str, String> {
    let word = word.trim();
    if word.is_empty() {
        return Err(String::from("Jieba 自定义词不能为空。"));
    }
    if word.chars().count() > 128 {
        return Err(String::from("Jieba 自定义词不能超过 128 个字符。"));
    }
    Ok(word)
}

/// Validates that a library identifier is one canonical lowercase UUID.
/// 验证知识库标识符是一个规范小写 UUID。
fn validate_library_id(library_id: &str) -> Result<(), String> {
    let parsed = Uuid::parse_str(library_id)
        .map_err(|error| format!("知识库标识符不是有效 UUID：{error}"))?;
    if parsed.to_string() != library_id {
        return Err(String::from("知识库标识符必须使用规范小写 UUID 格式。"));
    }
    Ok(())
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

/// Produces the unique comparison key for a library name.
/// 生成知识库名称的唯一比较键。
fn library_name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Converts one public package count to the signed SQLite representation.
/// 将一个公开包计数转换为 SQLite 有符号表示。
fn checked_package_count(value: u64, field: &str) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| format!("知识库包字段超出 SQLite 范围：{field}"))
}

/// Canonicalizes one existing ordinary destination directory.
/// 规范化一个现有普通目标目录。
fn canonical_existing_directory(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("读取目标目录失败（{}）：{error}", path.display()))?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(format!("目标路径不是可信普通目录：{}", path.display()));
    }
    fs::canonicalize(path)
        .map_err(|error| format!("规范化目标目录失败（{}）：{error}", path.display()))
}

/// Canonicalizes and bounds one user-selected .vkb archive.
/// 规范化并限制一个用户选择的 .vkb 归档。
fn canonical_knowledge_package_path(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("读取知识库包失败（{}）：{error}", path.display()))?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(format!("知识库包不是可信普通文件：{}", path.display()));
    }
    if metadata.len() > MAX_KNOWLEDGE_PACKAGE_BYTES {
        return Err(format!(
            "知识库包超过大小上限 {} 字节。",
            MAX_KNOWLEDGE_PACKAGE_BYTES
        ));
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("vkb"))
    {
        return Err(String::from("知识库包扩展名必须为 .vkb。"));
    }
    fs::canonicalize(path)
        .map_err(|error| format!("规范化知识库包失败（{}）：{error}", path.display()))
}

/// Extracts and validates one package into a host-controlled temporary directory.
/// 将一个知识库包解压并校验到宿主受控临时目录。
fn prepare_knowledge_package(
    archive_path: &Path,
    temporary_parent: &Path,
) -> Result<PreparedKnowledgePackage, String> {
    fs::create_dir_all(temporary_parent).map_err(|error| {
        format!(
            "创建知识库临时目录父级失败（{}）：{error}",
            temporary_parent.display()
        )
    })?;
    let temporary = tempfile::Builder::new()
        .prefix(KNOWLEDGE_PACKAGE_INSPECTION_TEMP_PREFIX)
        .tempdir_in(temporary_parent)
        .map_err(|error| format!("创建知识库包检查临时目录失败：{error}"))?;
    let input = File::open(archive_path)
        .map_err(|error| format!("打开知识库包失败（{}）：{error}", archive_path.display()))?;
    let mut archive =
        ZipArchive::new(input).map_err(|error| format!("解析知识库 ZIP 包失败：{error}"))?;
    if archive.len() != 3 {
        return Err(format!(
            "知识库包条目数量无效：{}，预期为 3。",
            archive.len()
        ));
    }
    let mut package_bytes: Option<Vec<u8>> = None;
    let mut manifest_bytes: Option<Vec<u8>> = None;
    let database_path = temporary.path().join("content.sqlite3");
    let mut database_seen = false;
    let mut seen = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取知识库包条目 {index} 失败：{error}"))?;
        let name = entry.name().to_string();
        if entry.is_dir() || entry.enclosed_name().is_none() {
            return Err(format!("知识库包包含非法条目：{name}"));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("知识库包不允许符号链接条目：{name}"));
        }
        if !seen.insert(name.clone()) {
            return Err(format!("知识库包包含重复条目：{name}"));
        }
        match name.as_str() {
            KNOWLEDGE_PACKAGE_METADATA_ENTRY => {
                package_bytes = Some(read_bounded_zip_entry(
                    &mut entry,
                    MAX_KNOWLEDGE_PACKAGE_JSON_BYTES,
                    KNOWLEDGE_PACKAGE_METADATA_ENTRY,
                )?);
            }
            KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY => {
                manifest_bytes = Some(read_bounded_zip_entry(
                    &mut entry,
                    MAX_KNOWLEDGE_PACKAGE_JSON_BYTES,
                    KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY,
                )?);
            }
            KNOWLEDGE_PACKAGE_DATABASE_ENTRY => {
                // expectedBytes is the central-directory size used only as a consistency check after bounded streaming.
                // expectedBytes 是中央目录声明大小，仅在有界流式解压后用于一致性校验。
                let expected_bytes = entry.size();
                if expected_bytes > MAX_KNOWLEDGE_PACKAGE_BYTES {
                    return Err(String::from("知识库包 SQLite 条目超过大小上限。"));
                }
                let mut output = File::create(&database_path)
                    .map_err(|error| format!("创建知识库包临时 SQLite 失败：{error}"))?;
                // copied is bounded independently from untrusted ZIP size metadata to prevent expansion bombs.
                // copied 独立于不可信 ZIP 大小元数据进行限制，以防止解压膨胀攻击。
                let copied = std::io::copy(
                    &mut entry.by_ref().take(MAX_KNOWLEDGE_PACKAGE_BYTES + 1),
                    &mut output,
                )
                .map_err(|error| format!("解压知识库 SQLite 失败：{error}"))?;
                if copied > MAX_KNOWLEDGE_PACKAGE_BYTES {
                    return Err(String::from("知识库包 SQLite 解压内容超过大小上限。"));
                }
                output
                    .sync_all()
                    .map_err(|error| format!("同步知识库临时 SQLite 失败：{error}"))?;
                if copied != expected_bytes {
                    return Err(String::from("知识库 SQLite 解压字节数不一致。"));
                }
                database_seen = true;
            }
            _ => return Err(format!("知识库包包含未知条目：{name}")),
        }
    }
    let package: KnowledgePackageManifest = serde_json::from_slice(
        &package_bytes.ok_or_else(|| String::from("知识库包缺少 package.json。"))?,
    )
    .map_err(|error| format!("解析知识库 package.json 失败：{error}"))?;
    let source_manifest: KnowledgeManifest = serde_json::from_slice(
        &manifest_bytes.ok_or_else(|| String::from("知识库包缺少内部 manifest.json。"))?,
    )
    .map_err(|error| format!("解析知识库内部 manifest.json 失败：{error}"))?;
    if !database_seen {
        return Err(String::from("知识库包缺少 content.sqlite3。"));
    }
    validate_knowledge_package_metadata(&package, &source_manifest, &database_path)?;
    Ok(PreparedKnowledgePackage {
        temporary,
        package,
        source_manifest,
    })
}

/// Reads one bounded JSON entry without accepting decompression expansion beyond its limit.
/// 读取一个有界 JSON 条目，不接受超过上限的解压膨胀。
fn read_bounded_zip_entry<R: Read>(
    entry: &mut R,
    limit: u64,
    name: &str,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取知识库包条目 {name} 失败：{error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("知识库包条目超过大小上限：{name}"));
    }
    Ok(bytes)
}

/// Validates portable metadata, source identity, size, and content digest.
/// 校验可移植元数据、来源身份、大小和内容摘要。
fn validate_knowledge_package_metadata(
    package: &KnowledgePackageManifest,
    source_manifest: &KnowledgeManifest,
    database_path: &Path,
) -> Result<(), String> {
    if package.format != KNOWLEDGE_PACKAGE_FORMAT
        || package.package_version != KNOWLEDGE_PACKAGE_VERSION
    {
        return Err(String::from("知识库包格式或包版本不受支持。"));
    }
    validate_library_id(&package.source_library_id)?;
    if validate_library_name(&package.name)? != package.name {
        return Err(String::from("知识库包名称不是规范化名称。"));
    }
    if package.schema_version != KNOWLEDGE_SCHEMA_VERSION
        || source_manifest.schema_version != KNOWLEDGE_SCHEMA_VERSION
    {
        return Err(format!(
            "知识库包 Schema 版本不兼容：package={}, manifest={}, current={}",
            package.schema_version, source_manifest.schema_version, KNOWLEDGE_SCHEMA_VERSION
        ));
    }
    if source_manifest.library_id != package.source_library_id
        || source_manifest.name != package.name
    {
        return Err(String::from("知识库包来源清单与 package.json 不一致。"));
    }
    if package.index_status != "ready" {
        return Err(String::from("知识库包不包含 ready 状态的可直接使用索引。"));
    }
    if package.content_sha256.len() != 64
        || !package
            .content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(String::from("知识库包 SQLite SHA-256 字段格式无效。"));
    }
    let content_bytes = fs::metadata(database_path)
        .map_err(|error| format!("读取知识库包 SQLite 元数据失败：{error}"))?
        .len();
    if content_bytes != package.content_bytes {
        return Err(String::from("知识库包 SQLite 大小与 package.json 不一致。"));
    }
    let digest = sha256_file_hex(database_path)?;
    if digest != package.content_sha256.to_ascii_lowercase() {
        return Err(String::from("知识库包 SQLite SHA-256 校验失败。"));
    }
    Ok(())
}

/// Writes one complete package after the owning SQLite binding has been stopped.
/// 在所属 SQLite 绑定停止后写出一个完整知识库包。
fn export_knowledge_package_files(
    library: &KnowledgeLibrarySummary,
    library_root: &Path,
    destination_directory: &Path,
) -> Result<KnowledgePackageExportResult, String> {
    let database_path = library_root.join("content.sqlite3");
    let manifest_path = library_root.join("manifest.json");
    let database_metadata = fs::symlink_metadata(&database_path)
        .map_err(|error| format!("读取待导出 SQLite 失败：{error}"))?;
    let manifest_metadata = fs::symlink_metadata(&manifest_path)
        .map_err(|error| format!("读取待导出清单失败：{error}"))?;
    if !database_metadata.is_file()
        || metadata_is_link_or_reparse(&database_metadata)
        || !manifest_metadata.is_file()
        || metadata_is_link_or_reparse(&manifest_metadata)
    {
        return Err(String::from("知识库导出来源包含非普通文件。"));
    }
    if database_metadata.len() > MAX_KNOWLEDGE_PACKAGE_BYTES {
        return Err(format!(
            "知识库 SQLite 超过可移植包大小上限 {} 字节。",
            MAX_KNOWLEDGE_PACKAGE_BYTES
        ));
    }
    for transient_name in [
        "content.sqlite3-wal",
        "content.sqlite3-shm",
        "content.sqlite3-journal",
    ] {
        // transientPath must be absent after the owning SQLite binding is stopped.
        // transientPath 在所属 SQLite 绑定停止后必须不存在。
        let transient_path = library_root.join(transient_name);
        if transient_path
            .try_exists()
            .map_err(|error| format!("检查知识库 SQLite 临时日志失败：{error}"))?
        {
            return Err(format!(
                "知识库 SQLite 日志尚未完全收口，拒绝导出：{}",
                transient_path.display()
            ));
        }
    }
    let source_manifest: KnowledgeManifest = serde_json::from_slice(
        &fs::read(&manifest_path).map_err(|error| format!("读取待导出清单失败：{error}"))?,
    )
    .map_err(|error| format!("解析待导出清单失败：{error}"))?;
    if source_manifest.library_id != library.id
        || source_manifest.name != library.name
        || source_manifest.schema_version != KNOWLEDGE_SCHEMA_VERSION
    {
        return Err(String::from("待导出知识库清单与注册记录不一致。"));
    }
    let exported_at_ms = unix_epoch_millis()?;
    let package = KnowledgePackageManifest {
        format: KNOWLEDGE_PACKAGE_FORMAT.to_string(),
        package_version: KNOWLEDGE_PACKAGE_VERSION,
        source_library_id: library.id.clone(),
        name: library.name.clone(),
        description: library.description.clone(),
        schema_version: KNOWLEDGE_SCHEMA_VERSION,
        document_count: library.document_count,
        chunk_count: library.chunk_count,
        index_status: library.index_status.clone(),
        exported_at_ms,
        content_sha256: sha256_file_hex(&database_path)?,
        content_bytes: database_metadata.len(),
    };
    // exportId prevents same-millisecond or sanitized-name collisions from overwriting another package.
    // exportId 防止同毫秒或名称清理冲突覆盖另一份知识库包。
    let export_id = Uuid::new_v4().to_string();
    let archive_name = portable_knowledge_archive_name(&library.name, exported_at_ms, &export_id);
    let archive_path = destination_directory.join(archive_name);
    let mut temporary = tempfile::Builder::new()
        .prefix(".knowledge-export-")
        .suffix(".vkb.tmp")
        .tempfile_in(destination_directory)
        .map_err(|error| format!("创建知识库导出临时文件失败：{error}"))?;
    {
        let mut writer = ZipWriter::new(temporary.as_file_mut());
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        writer
            .start_file(KNOWLEDGE_PACKAGE_METADATA_ENTRY, options)
            .map_err(|error| format!("创建知识库 package.json 条目失败：{error}"))?;
        writer
            .write_all(
                &serde_json::to_vec_pretty(&package)
                    .map_err(|error| format!("序列化知识库包元数据失败：{error}"))?,
            )
            .map_err(|error| format!("写入知识库 package.json 失败：{error}"))?;
        writer
            .start_file(KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY, options)
            .map_err(|error| format!("创建知识库内部清单条目失败：{error}"))?;
        writer
            .write_all(
                &serde_json::to_vec_pretty(&source_manifest)
                    .map_err(|error| format!("序列化知识库内部清单失败：{error}"))?,
            )
            .map_err(|error| format!("写入知识库内部清单失败：{error}"))?;
        writer
            .start_file(KNOWLEDGE_PACKAGE_DATABASE_ENTRY, options)
            .map_err(|error| format!("创建知识库 SQLite 条目失败：{error}"))?;
        let mut database = File::open(&database_path)
            .map_err(|error| format!("打开待导出 SQLite 失败：{error}"))?;
        std::io::copy(&mut database, &mut writer)
            .map_err(|error| format!("写入知识库 SQLite 条目失败：{error}"))?;
        writer
            .finish()
            .map_err(|error| format!("完成知识库 ZIP 包失败：{error}"))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("同步知识库导出临时文件失败：{error}"))?;
    // archiveBytes enforces that an exported package can pass the same size gate during import.
    // archiveBytes 确保导出包能够通过导入时相同的大小门禁。
    let archive_bytes = temporary
        .as_file()
        .metadata()
        .map_err(|error| format!("读取知识库导出包大小失败：{error}"))?
        .len();
    if archive_bytes > MAX_KNOWLEDGE_PACKAGE_BYTES {
        return Err(format!(
            "知识库导出包超过大小上限 {} 字节。",
            MAX_KNOWLEDGE_PACKAGE_BYTES
        ));
    }
    persist_tempfile_atomically(temporary, &archive_path)
        .map_err(|error| format!("提交知识库导出文件失败：{error}"))?;
    Ok(KnowledgePackageExportResult {
        archive_path: runtime_assets::native_host_path_text_from_path(&archive_path),
        package,
    })
}

/// Computes a streaming lowercase SHA-256 digest for one ordinary file.
/// 为一个普通文件计算流式小写 SHA-256 摘要。
fn sha256_file_hex(path: &Path) -> Result<String, String> {
    let mut input = File::open(path)
        .map_err(|error| format!("打开 SHA-256 输入文件失败（{}）：{error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("读取 SHA-256 输入文件失败：{error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Produces a filesystem-safe timestamped archive name without overwriting prior exports.
/// 生成文件系统安全且带时间戳的归档名称，避免覆盖之前的导出。
///
/// # Parameters
/// 参数
/// `name` is the user-visible library name whose safe prefix is retained.
/// `name` 是保留安全前缀的用户可见知识库名称。
/// `timestamp_ms` is the package export timestamp used for chronological recognition.
/// `timestamp_ms` 是用于按时间识别的知识库包导出时间戳。
/// `export_id` is the canonical host-generated UUID preventing destination collisions.
/// `export_id` 是防止目标冲突的宿主生成规范 UUID。
///
/// # Returns
/// 返回值
/// Returns one bounded `.vkb` filename containing no Windows-reserved path characters.
/// 返回一个不含 Windows 保留路径字符且长度有界的 `.vkb` 文件名。
fn portable_knowledge_archive_name(name: &str, timestamp_ms: i64, export_id: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            value if value.is_control() => '_',
            value => value,
        })
        .take(48)
        .collect();
    let sanitized = sanitized.trim().trim_end_matches('.');
    let stem = if sanitized.is_empty() {
        "knowledge-base"
    } else {
        sanitized
    };
    format!("{stem}-{timestamp_ms}-{export_id}.vkb")
}

/// Produces the platform-aware comparison key for an absolute source path.
/// 生成绝对来源路径的平台感知比较键。
fn path_key(path: &Path) -> String {
    let value = runtime_assets::native_host_path_text_from_path(path);
    #[cfg(windows)]
    {
        value.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        value
    }
}

/// Decodes one controller JSON row set into typed rows.
/// 将一份控制器 JSON 行集解码为类型化记录。
fn decode_rows<T: DeserializeOwned>(json: &str) -> Result<Vec<T>, String> {
    serde_json::from_str(json).map_err(|error| format!("解码知识库查询结果失败：{error}"))
}

/// Maps one registry row into the checked public summary.
/// 将一条注册记录映射为已检查的公共摘要。
fn library_summary_from_row(row: LibraryRow) -> Result<KnowledgeLibrarySummary, String> {
    Ok(KnowledgeLibrarySummary {
        id: row.id,
        name: row.name,
        description: row.description,
        lifecycle_state: row.lifecycle_state,
        document_count: nonnegative_u64(row.document_count, "document_count")?,
        chunk_count: nonnegative_u64(row.chunk_count, "chunk_count")?,
        index_status: row.index_status,
        last_completed_import_at_ms: row.last_completed_import_at_ms,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    })
}

/// Maps one document row into the checked public summary.
/// 将一条文档记录映射为已检查的公共摘要。
fn document_summary_from_row(row: DocumentSummaryRow) -> Result<KnowledgeDocumentSummary, String> {
    Ok(KnowledgeDocumentSummary {
        id: row.id,
        source_id: row.source_id,
        relative_path: row.relative_path,
        folder_path: row.folder_path,
        title: row.title,
        byte_size: nonnegative_u64(row.byte_size, "byte_size")?,
        line_count: nonnegative_u64(row.line_count, "line_count")?,
        chunk_count: nonnegative_u64(row.chunk_count, "chunk_count")?,
        updated_at_ms: row.updated_at_ms,
    })
}

/// Maps one aggregate source row into the checked public management summary.
/// 将一条聚合来源记录映射为已检查的公共管理摘要。
///
/// Parameters: `row` is the exact SQLite aggregate row returned by the source listing query.
/// Returns: a source summary whose document and chunk counts are guaranteed to be non-negative.
/// 参数：`row` 是来源列表查询返回的精确 SQLite 聚合记录。返回值：文档与切片数量保证非负的来源摘要。
fn source_summary_from_row(row: SourceSummaryRow) -> Result<KnowledgeSourceSummary, String> {
    Ok(KnowledgeSourceSummary {
        id: row.id,
        canonical_root: row.canonical_root,
        display_name: row.display_name,
        document_count: nonnegative_u64(row.document_count, "document_count")?,
        chunk_count: nonnegative_u64(row.chunk_count, "chunk_count")?,
        created_at_ms: row.created_at_ms,
        last_scanned_at_ms: row.last_scanned_at_ms,
    })
}

/// Maps one joined row into a checked public chunk.
/// 将一条联结记录映射为已检查的公共切片。
fn chunk_from_joined_row(row: &JoinedChunkRow) -> Result<KnowledgeChunk, String> {
    Ok(KnowledgeChunk {
        id: row.id.clone(),
        document_id: row.document_id.clone(),
        ordinal: nonnegative_u64(row.ordinal, "ordinal")?,
        heading_path: row.heading_path.clone(),
        start_line: nonnegative_u64(row.start_line, "start_line")?,
        end_line: nonnegative_u64(row.end_line, "end_line")?,
        content: row.content.clone(),
        estimated_tokens: nonnegative_u64(row.estimated_tokens, "estimated_tokens")?,
    })
}

/// Converts a non-negative SQLite integer into a public unsigned value.
/// 将非负 SQLite 整数转换为公共无符号值。
fn nonnegative_u64(value: i64, field: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("知识库字段 {field} 包含非法负值：{value}"))
}

/// Parses one non-negative decimal metadata revision into its public unsigned representation.
/// 将一个非负十进制元数据版本解析为公共无符号表示。
///
/// Parameters: `value` is the persisted metadata text and `field` names diagnostics.
/// Returns: the parsed revision or an explicit metadata integrity error.
/// 参数：`value` 是持久化元数据文本，`field` 用于命名诊断。返回值：解析后的版本或明确元数据完整性错误。
fn parse_revision(value: &str, field: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("知识库元数据 {field} 不是有效非负版本（{value}）：{error}"))
}

/// Returns a typed SQLite text value.
/// 返回类型化 SQLite 文本值。
fn text(value: impl Into<String>) -> ControllerSqliteValue {
    ControllerSqliteValue::String(value.into())
}

/// Returns a typed SQLite integer value.
/// 返回类型化 SQLite 整数值。
fn int(value: i64) -> ControllerSqliteValue {
    ControllerSqliteValue::Int64(value)
}

/// Returns a typed SQLite boolean value.
/// 返回类型化 SQLite 布尔值。
fn bool_value(value: bool) -> ControllerSqliteValue {
    ControllerSqliteValue::Bool(value)
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
fn unix_epoch_millis() -> Result<i64, String> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("系统时间早于 Unix 纪元：{error}"))?
        .as_millis();
    i64::try_from(value).map_err(|_| String::from("当前 Unix 毫秒时间戳超出 i64 范围。"))
}

/// Converts one system time into Unix milliseconds when representable.
/// 在可表示时将一个系统时间转换为 Unix 毫秒时间戳。
fn system_time_millis(value: SystemTime) -> Option<i64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

/// Appends one bounded warning when report capacity remains.
/// 在报告仍有容量时追加一条有界告警。
fn push_warning(warnings: &mut Vec<String>, warning: String) {
    if warnings.len() < MAX_IMPORT_WARNINGS {
        warnings.push(truncate_chars(&warning, MAX_WARNING_CHARS));
    }
}

/// Truncates text by Unicode scalar count without splitting UTF-8.
/// 按 Unicode 标量数量截断文本且不拆分 UTF-8。
fn truncate_chars(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_plane::{
        build_application_schema_script, ensure_application_schema_migrations,
    };
    use tempfile::tempdir;
    use vldb_controller_client::{
        ClientRegistration, ControllerClient, ControllerClientConfig, ControllerProcessMode,
    };

    /// Writes one compact package fixture with caller-controlled metadata entry and optional extra entry.
    /// 写入一个紧凑知识库包夹具，允许调用方控制元数据条目及可选额外条目。
    fn write_package_fixture(
        path: &Path,
        package: &KnowledgePackageManifest,
        manifest: &KnowledgeManifest,
        database: &[u8],
        metadata_entry: &str,
        extra_entry: Option<&str>,
    ) {
        let file = File::create(path).expect("create package fixture");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer
            .start_file(metadata_entry, options)
            .expect("start package metadata");
        writer
            .write_all(&serde_json::to_vec(package).expect("serialize package metadata"))
            .expect("write package metadata");
        writer
            .start_file(KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY, options)
            .expect("start package manifest");
        writer
            .write_all(&serde_json::to_vec(manifest).expect("serialize package manifest"))
            .expect("write package manifest");
        writer
            .start_file(KNOWLEDGE_PACKAGE_DATABASE_ENTRY, options)
            .expect("start package database");
        writer.write_all(database).expect("write package database");
        if let Some(extra_entry) = extra_entry {
            writer
                .start_file(extra_entry, options)
                .expect("start extra package entry");
            writer
                .write_all(b"extra")
                .expect("write extra package entry");
        }
        writer.finish().expect("finish package fixture");
    }

    /// Builds matching metadata for one compact package fixture.
    /// 为一个紧凑知识库包夹具构建相互匹配的元数据。
    fn package_fixture_metadata(database: &[u8]) -> (KnowledgePackageManifest, KnowledgeManifest) {
        let source_library_id = String::from("550e8400-e29b-41d4-a716-446655440000");
        let package = KnowledgePackageManifest {
            format: String::from(KNOWLEDGE_PACKAGE_FORMAT),
            package_version: KNOWLEDGE_PACKAGE_VERSION,
            source_library_id: source_library_id.clone(),
            name: String::from("测试知识库"),
            description: String::from("fixture"),
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            document_count: 0,
            chunk_count: 0,
            index_status: String::from("ready"),
            exported_at_ms: 1,
            content_sha256: format!("{:x}", Sha256::digest(database)),
            content_bytes: database.len() as u64,
        };
        let manifest = KnowledgeManifest {
            library_id: source_library_id,
            name: package.name.clone(),
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
        };
        (package, manifest)
    }

    /// Rewrites only the declared source identity while retaining the original SQLite bytes and digest.
    /// 仅重写声明的来源身份，同时保留原始 SQLite 字节与摘要。
    fn write_identity_mismatch_package(source: &Path, destination: &Path) {
        // archive reads the three exact entries from a valid exported package.
        // archive 从一个有效导出包中读取三个精确条目。
        let input = File::open(source).expect("open valid source package");
        let mut archive = ZipArchive::new(input).expect("parse valid source package");
        // package changes provenance metadata without changing the database content.
        // package 修改来源元数据，但不改变数据库内容。
        let mut package: KnowledgePackageManifest = {
            let mut entry = archive
                .by_name(KNOWLEDGE_PACKAGE_METADATA_ENTRY)
                .expect("read source package metadata");
            serde_json::from_reader(&mut entry).expect("decode source package metadata")
        };
        // manifest mirrors the forged identity so the outer package checks still pass.
        // manifest 镜像伪造身份，使知识库包外层检查仍能通过。
        let mut manifest: KnowledgeManifest = {
            let mut entry = archive
                .by_name(KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY)
                .expect("read source package manifest");
            serde_json::from_reader(&mut entry).expect("decode source package manifest")
        };
        // database retains the real embedded identity and therefore must fail full inspection.
        // database 保留真实内嵌身份，因此必须无法通过完整检查。
        let mut database = Vec::new();
        archive
            .by_name(KNOWLEDGE_PACKAGE_DATABASE_ENTRY)
            .expect("read source package database")
            .read_to_end(&mut database)
            .expect("decode source package database");
        // forgedIdentity is canonical but intentionally differs from the SQLite metadata row.
        // forgedIdentity 是规范标识，但刻意不同于 SQLite 元数据记录。
        let forged_identity = String::from("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        package.source_library_id.clone_from(&forged_identity);
        manifest.library_id = forged_identity;
        write_package_fixture(
            destination,
            &package,
            &manifest,
            &database,
            KNOWLEDGE_PACKAGE_METADATA_ENTRY,
            None,
        );
    }

    /// Verifies library IDs reject non-canonical and path-like input.
    /// 验证知识库标识符拒绝非规范格式与路径式输入。
    #[test]
    fn library_id_requires_canonical_uuid() {
        assert!(validate_library_id("../../escape").is_err());
        assert!(validate_library_id("550E8400-E29B-41D4-A716-446655440000").is_err());
        assert!(validate_library_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    /// Verifies startup cleanup recognizes only reserved extraction directory names with a suffix.
    /// 验证启动清理仅识别带后缀的保留解压目录名称。
    #[test]
    fn package_inspection_temp_name_requires_reserved_prefix_and_suffix() {
        assert!(is_package_inspection_temp_directory_name(
            ".package-inspect-AbCd1234"
        ));
        assert!(!is_package_inspection_temp_directory_name(
            ".package-inspect-"
        ));
        assert!(!is_package_inspection_temp_directory_name(
            "package-inspect-AbCd1234"
        ));
    }

    /// Verifies replacement journals bind one canonical target, operation, backup name, and phase.
    /// 验证覆盖恢复日志绑定唯一的规范目标、操作、备份名称与阶段。
    #[test]
    fn replacement_journal_rejects_inconsistent_identity_or_phase() {
        // targetId and operationId are distinct canonical UUIDs encoded in every durable artifact.
        // targetId 与 operationId 是编码到每个持久化产物中的不同规范 UUID。
        let target_id = String::from("550e8400-e29b-41d4-a716-446655440000");
        let operation_id = String::from("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        // previous captures the exact active registry row that prepared recovery must restore.
        // previous 捕获 prepared 恢复必须还原的精确活动注册记录。
        let previous = KnowledgeLibrarySummary {
            id: target_id.clone(),
            name: String::from("恢复测试知识库"),
            description: String::from("fixture"),
            lifecycle_state: String::from("active"),
            document_count: 2,
            chunk_count: 3,
            index_status: String::from("ready"),
            last_completed_import_at_ms: Some(1),
            created_at_ms: 1,
            updated_at_ms: 2,
        };
        // journal is the valid baseline mutated by each negative assertion.
        // journal 是每个反向断言都会修改的有效基线。
        let journal = KnowledgeReplacementJournal {
            version: KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION,
            target_library_id: target_id.clone(),
            backup_directory_name: format!(".replace-{target_id}-{operation_id}"),
            phase: String::from(KNOWLEDGE_REPLACEMENT_PHASE_PREPARED),
            previous,
        };
        assert_eq!(
            validate_replacement_journal(&journal).expect("validate replacement journal"),
            operation_id
        );

        let mut wrong_target = journal.clone();
        wrong_target.previous.id = String::from("67e55044-10b1-426f-9247-bb680e5fe0c8");
        assert!(validate_replacement_journal(&wrong_target).is_err());

        let mut invalid_backup = journal.clone();
        invalid_backup.backup_directory_name = String::from("../outside");
        assert!(validate_replacement_journal(&invalid_backup).is_err());

        let mut invalid_phase = journal;
        invalid_phase.phase = String::from("unknown");
        assert!(validate_replacement_journal(&invalid_phase).is_err());
    }

    /// Verifies a stale cancellation request cannot stop a newer knowledge operation.
    /// 验证过期取消请求不能停止更新的知识库操作。
    #[test]
    fn cancellation_requires_exact_active_job_identity() {
        let signal = Arc::new(AtomicBool::new(false));
        let active = ActiveKnowledgeOperation {
            job_id: String::from("550e8400-e29b-41d4-a716-446655440000"),
            signal: Arc::clone(&signal),
        };
        assert!(!signal_matching_operation_cancellation(
            Some(&active),
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
        ));
        assert!(!signal.load(Ordering::Acquire));
        assert!(signal_matching_operation_cancellation(
            Some(&active),
            "550e8400-e29b-41d4-a716-446655440000"
        ));
        assert!(signal.load(Ordering::Acquire));
    }

    /// Verifies slash conversion rejects parent-directory components.
    /// 验证斜杠转换会拒绝父目录组件。
    #[test]
    fn slash_path_rejects_parent_components() {
        assert!(slash_path(Path::new("docs/guide.md")).is_ok());
        assert!(slash_path(Path::new("../guide.md")).is_err());
    }

    /// Verifies warnings remain bounded by count and character length.
    /// 验证告警数量与字符长度保持有界。
    #[test]
    fn warnings_are_bounded() {
        let mut warnings = Vec::new();
        for _ in 0..(MAX_IMPORT_WARNINGS + 5) {
            push_warning(&mut warnings, "界".repeat(MAX_WARNING_CHARS + 10));
        }
        assert_eq!(warnings.len(), MAX_IMPORT_WARNINGS);
        assert!(
            warnings
                .iter()
                .all(|warning| warning.chars().count() == MAX_WARNING_CHARS)
        );
    }

    /// Verifies atomic replacement overwrites an existing manifest on every supported platform.
    /// 验证原子替换会在所有受支持平台覆盖既有清单。
    #[test]
    fn atomic_file_replacement_overwrites_existing_destination() {
        let temporary = tempdir().expect("create atomic replacement root");
        let destination = temporary.path().join("manifest.json");
        fs::write(&destination, "old manifest").expect("write existing destination");
        let mut source =
            tempfile::NamedTempFile::new_in(temporary.path()).expect("create replacement source");
        source
            .write_all(b"new manifest")
            .expect("write replacement source");

        persist_tempfile_atomically(source, &destination).expect("replace existing destination");

        assert_eq!(
            fs::read_to_string(&destination).expect("read replaced destination"),
            "new manifest"
        );
    }

    /// Verifies package extraction accepts only the exact three-entry format and matching digest.
    /// 验证知识库包解压仅接受精确的三条目格式与匹配摘要。
    #[test]
    fn package_preparation_enforces_exact_entries_and_digest() {
        let temporary = tempdir().expect("create package fixture root");
        let database = b"SQLite format 3\0fixture";
        let (package, manifest) = package_fixture_metadata(database);
        let valid_path = temporary.path().join("valid.vkb");
        write_package_fixture(
            &valid_path,
            &package,
            &manifest,
            database,
            KNOWLEDGE_PACKAGE_METADATA_ENTRY,
            None,
        );
        let prepared = prepare_knowledge_package(&valid_path, temporary.path())
            .expect("prepare matching package");
        assert_eq!(prepared.package, package);

        let mut corrupt_package = package.clone();
        corrupt_package.content_sha256 = "0".repeat(64);
        let corrupt_path = temporary.path().join("corrupt.vkb");
        write_package_fixture(
            &corrupt_path,
            &corrupt_package,
            &manifest,
            database,
            KNOWLEDGE_PACKAGE_METADATA_ENTRY,
            None,
        );
        assert!(prepare_knowledge_package(&corrupt_path, temporary.path()).is_err());

        let traversal_path = temporary.path().join("traversal.vkb");
        write_package_fixture(
            &traversal_path,
            &package,
            &manifest,
            database,
            "../package.json",
            None,
        );
        assert!(prepare_knowledge_package(&traversal_path, temporary.path()).is_err());

        let extra_path = temporary.path().join("extra.vkb");
        write_package_fixture(
            &extra_path,
            &package,
            &manifest,
            database,
            KNOWLEDGE_PACKAGE_METADATA_ENTRY,
            Some("unexpected.txt"),
        );
        assert!(prepare_knowledge_package(&extra_path, temporary.path()).is_err());

        // unsupportedPackage proves the format protocol field is enforced.
        // unsupportedPackage 证明格式协议字段会被强制校验。
        let mut unsupported_package = package.clone();
        unsupported_package.format = String::from("unsupported-format");
        let unsupported_path = temporary.path().join("unsupported.vkb");
        write_package_fixture(
            &unsupported_path,
            &unsupported_package,
            &manifest,
            database,
            KNOWLEDGE_PACKAGE_METADATA_ENTRY,
            None,
        );
        assert!(prepare_knowledge_package(&unsupported_path, temporary.path()).is_err());

        // incompatiblePackage proves the SQLite schema version is checked independently.
        // incompatiblePackage 证明 SQLite 结构版本会被独立校验。
        let mut incompatible_package = package.clone();
        incompatible_package.schema_version += 1;
        let incompatible_path = temporary.path().join("incompatible.vkb");
        write_package_fixture(
            &incompatible_path,
            &incompatible_package,
            &manifest,
            database,
            KNOWLEDGE_PACKAGE_METADATA_ENTRY,
            None,
        );
        assert!(prepare_knowledge_package(&incompatible_path, temporary.path()).is_err());
    }

    /// Verifies bounded ZIP readers reject decompressed metadata beyond the configured limit.
    /// 验证有界 ZIP 读取器拒绝超过配置上限的解压元数据。
    #[test]
    fn bounded_package_entry_rejects_expansion_over_limit() {
        // oversized creates one byte beyond the exact test limit.
        // oversized 创建比精确测试上限多一个字节的数据。
        let mut oversized = std::io::Cursor::new(vec![0_u8; 17]);
        assert!(read_bounded_zip_entry(&mut oversized, 16, "fixture").is_err());
    }

    /// Verifies long or filesystem-reserved names remain bounded in exported archive names.
    /// 验证过长或文件系统保留字符名称生成的导出归档名保持有界。
    #[test]
    fn portable_archive_name_is_safe_and_bounded() {
        let archive_name = portable_knowledge_archive_name(
            &format!("CON:<>?*{}", "界".repeat(200)),
            123456789,
            "550e8400-e29b-41d4-a716-446655440000",
        );
        assert!(archive_name.ends_with("-123456789-550e8400-e29b-41d4-a716-446655440000.vkb"));
        assert!(
            archive_name
                .chars()
                .all(|character| !matches!(character, ':' | '<' | '>' | '?' | '*'))
        );
        assert!(
            archive_name.chars().count()
                <= 48
                    + "-123456789-550e8400-e29b-41d4-a716-446655440000.vkb"
                        .chars()
                        .count()
        );
    }

    /// Verifies cancellation stops directory scanning before any candidate is accepted.
    /// 验证取消会在接受任何候选文件前停止目录扫描。
    #[test]
    fn markdown_scan_honors_preexisting_cancellation() {
        let temporary = tempdir().expect("create cancelled scan root");
        fs::write(temporary.path().join("document.md"), "# Cancelled")
            .expect("write cancelled scan fixture");
        let cancellation = AtomicBool::new(true);

        let error = scan_markdown_files(temporary.path(), &cancellation, None)
            .expect_err("cancelled scan must stop");

        assert_eq!(error, KNOWLEDGE_OPERATION_CANCELLED_ERROR);
    }

    /// Exercises create, Markdown import, Jieba BM25 search, incremental deletion, and safe removal against a real controller.
    /// 使用真实控制器验证创建、Markdown 导入、Jieba BM25 检索、增量删除与安全移除。
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires the bundled output/bin/vldb-controller.exe"]
    async fn real_controller_markdown_bm25_end_to_end() {
        let temporary = tempdir().expect("create knowledge end-to-end root");
        let data_root = temporary.path().join("data");
        let source_root = temporary.path().join("source");
        fs::create_dir_all(source_root.join("guide")).expect("create source tree");
        fs::write(
            source_root.join("guide").join("search.md"),
            "# 检索设计\n\n火山知识库使用结巴分词和 BM25 检索。\n\ngetskilllevelup(actor, 10)\n",
        )
        .expect("write Chinese Markdown fixture");
        fs::write(source_root.join("ignored.txt"), "must not import")
            .expect("write ignored fixture");

        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("reserve an ephemeral controller port");
        let port = listener.local_addr().expect("read controller port").port();
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
                client_name: format!("knowledge-e2e-{}", Uuid::new_v4()),
                host_kind: String::from("test"),
                process_id: std::process::id(),
                process_name: String::from("vulcan-workspace-store-test"),
                lease_ttl_secs: Some(30),
            },
        );
        client.connect().await.expect("connect real controller");
        let app_path = data_root.join("sqlite").join("app-workspace.sqlite3");
        fs::create_dir_all(app_path.parent().expect("app database parent"))
            .expect("create app database parent");
        let app_binding_id = format!("knowledge-e2e-app-{}", Uuid::new_v4());
        client
            .attach_space(SpaceRegistration {
                space_id: String::from(APP_SPACE_ID),
                space_label: String::from("APP"),
                space_kind: SpaceKind::User,
                space_root: runtime_assets::native_host_path_text_from_path(&data_root),
            })
            .await
            .expect("attach app space");
        client
            .enable_sqlite(ControllerSqliteEnableRequest {
                space_id: String::from(APP_SPACE_ID),
                binding_id: app_binding_id.clone(),
                db_path: runtime_assets::native_host_path_text_from_path(&app_path),
                ..ControllerSqliteEnableRequest::default()
            })
            .await
            .expect("enable app database");
        client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                &app_binding_id,
                build_application_schema_script(),
                Vec::new(),
            )
            .await
            .expect("initialize app schema");
        ensure_application_schema_migrations(&client, &app_binding_id)
            .await
            .expect("apply app schema migrations");
        let runtime = KnowledgeBaseRuntime::new(ControllerRuntimeHandle::new(
            client.clone(),
            endpoint,
            executable.display().to_string(),
            data_root,
            app_path,
            app_binding_id,
        ));

        // abandonedExtraction simulates a package TempDir left behind by process termination.
        // abandonedExtraction 模拟进程终止后遗留的知识库包临时解压目录。
        let abandoned_extraction = runtime
            .knowledge_root()
            .join(".package-inspect-crash-fixture");
        fs::create_dir_all(&abandoned_extraction)
            .expect("create abandoned package extraction fixture");
        fs::write(abandoned_extraction.join("content.sqlite3"), b"abandoned")
            .expect("write abandoned package extraction fixture");
        runtime
            .recover_interrupted_jobs()
            .await
            .expect("clean abandoned package extraction at startup");
        assert!(!abandoned_extraction.exists());

        let library = runtime
            .create_library("端到端知识库", "", &[])
            .await
            .expect("create isolated library");
        let library = runtime
            .rename_library(&library.id, "端到端知识库已重命名", "")
            .await
            .expect("replace existing manifest during rename");
        assert_eq!(library.name, "端到端知识库已重命名");
        let first_import = runtime
            .import_markdown_directory(&library.id, &source_root)
            .await
            .expect("import Markdown through Jieba FTS");
        assert_eq!(first_import.scanned_files, 1);
        assert_eq!(first_import.changed_files, 1);
        let completed_job = runtime
            .latest_import_job(&library.id)
            .await
            .expect("load completed import job")
            .expect("completed import job exists");
        assert_eq!(completed_job.operation_kind, KnowledgeOperationKind::Import);
        assert_eq!(completed_job.total_files, 1);
        assert_eq!(completed_job.processed_files, 1);
        assert_eq!(completed_job.current_file, None);
        assert_eq!(
            runtime
                .list_documents(&library.id, None, 20, 0)
                .await
                .expect("list imported documents")
                .len(),
            1
        );
        // SourceSummary separates the authoritative document total from its searchable chunk total.
        // SourceSummary 将权威文档总数与其可检索切片总数分开。
        let source_summary = runtime
            .list_sources(&library.id)
            .await
            .expect("list durable source summary")
            .into_iter()
            .next()
            .expect("source summary exists");
        assert_eq!(source_summary.document_count, 1);
        assert_eq!(source_summary.chunk_count, first_import.chunk_count);
        let initial_integrity = runtime
            .inspect_index_integrity(&library.id)
            .await
            .expect("inspect ready index integrity");
        assert!(initial_integrity.searchable);
        assert_eq!(
            initial_integrity.authoritative_chunk_count,
            first_import.chunk_count
        );
        assert_eq!(
            initial_integrity.indexed_chunk_count,
            first_import.chunk_count
        );
        let search = runtime
            .search(std::slice::from_ref(&library.id), "火山知识库", 10)
            .await
            .expect("search Chinese content");
        assert!(!search.hits.is_empty());
        assert!(!search.fts_queries[0].expression.is_empty());
        // independentRuntime reproduces the native AI tool path that reconstructs the runtime from the same controller handle.
        // independentRuntime 复现原生 AI 工具使用同一控制器句柄重新构造运行时的路径。
        let independent_runtime = KnowledgeBaseRuntime::new(runtime.controller.clone());
        // independentSearch must reuse the controller binding instead of reopening the locked SQLite file.
        // independentSearch 必须复用控制器绑定，而不是重新打开已经锁定的 SQLite 文件。
        let independent_search = independent_runtime
            .search(std::slice::from_ref(&library.id), "火山知识库", 10)
            .await
            .expect("search Chinese content through independent runtime");
        assert!(!independent_search.hits.is_empty());
        // thirdRuntime exposes controller state corruption after one same-binding re-enable.
        // thirdRuntime 用于暴露同一绑定重复启用一次后的控制器状态损坏。
        let third_runtime = KnowledgeBaseRuntime::new(runtime.controller.clone());
        // thirdSearch must remain idempotent even after the binding has already been enabled twice.
        // thirdSearch 必须在绑定已经启用两次后仍保持幂等。
        let third_search = third_runtime
            .search(std::slice::from_ref(&library.id), "火山知识库", 10)
            .await
            .expect("search Chinese content through third runtime");
        assert!(
            third_search.failures.is_empty(),
            "third runtime search failures: {:?}",
            third_search.failures
        );
        assert!(!third_search.hits.is_empty());
        let substring_search = runtime
            .search(std::slice::from_ref(&library.id), "levelup", 10)
            .await
            .expect("supplement Jieba BM25 with identifier substring matches");
        assert_eq!(substring_search.hits.len(), 1);
        assert_eq!(substring_search.hits[0].match_kind, "substring");

        // LegacyEncodingLibrary isolates GBK ingestion from the UTF-8 incremental fixtures below.
        // LegacyEncodingLibrary 将 GBK 导入与下方 UTF-8 增量夹具隔离。
        let legacy_encoding_root = temporary.path().join("legacy-encoding-source");
        fs::create_dir_all(&legacy_encoding_root).expect("create legacy encoding source root");
        let legacy_markdown = "# 服务端等级\n\n服务端用户等级获取规则使用 GBK 文档。\n";
        let (legacy_bytes, _, legacy_encode_had_errors) = encoding_rs::GBK.encode(legacy_markdown);
        assert!(!legacy_encode_had_errors);
        fs::write(
            legacy_encoding_root.join("server-level.md"),
            legacy_bytes.as_ref(),
        )
        .expect("write GBK Markdown fixture");
        let legacy_encoding_library = runtime
            .create_library("GBK 知识库", "", &[])
            .await
            .expect("create GBK library");
        let legacy_encoding_import = runtime
            .import_markdown_directory(&legacy_encoding_library.id, &legacy_encoding_root)
            .await
            .expect("import GBK Markdown through the shared decoder");
        assert_eq!(legacy_encoding_import.changed_files, 1);
        let legacy_encoding_search = runtime
            .search(
                std::slice::from_ref(&legacy_encoding_library.id),
                "用户等级",
                10,
            )
            .await
            .expect("search GBK Markdown through Jieba BM25");
        assert!(!legacy_encoding_search.hits.is_empty());
        runtime
            .delete_library(&legacy_encoding_library.id)
            .await
            .expect("delete GBK library");

        // ImportTimestamp must describe content ingestion rather than later derived-index maintenance.
        // ImportTimestamp 必须描述内容导入，而不是后续派生索引维护。
        let import_timestamp = runtime
            .get_library(&library.id)
            .await
            .expect("load successful import timestamp")
            .last_completed_import_at_ms;
        assert!(import_timestamp.is_some());
        let rebuilt_rows = runtime
            .rebuild_index(&library.id)
            .await
            .expect("manually rebuild Jieba index through parameter-safe statements");
        assert_eq!(rebuilt_rows, first_import.chunk_count);
        let rebuilt_library = runtime
            .get_library(&library.id)
            .await
            .expect("load registry counts after manual rebuild");
        assert_eq!(rebuilt_library.document_count, 1);
        assert_eq!(rebuilt_library.chunk_count, first_import.chunk_count);
        assert_eq!(
            rebuilt_library.last_completed_import_at_ms,
            import_timestamp
        );

        // KnowledgeBinding identifies the exact physical database used by the recovery fixture.
        // KnowledgeBinding 标识恢复夹具使用的准确物理数据库。
        let (knowledge_space_id, knowledge_binding_id) = runtime
            .ensure_library_binding(&library.id)
            .await
            .expect("resolve library binding for recovery fixture");
        client
            .execute_sqlite_script_typed(
                &knowledge_space_id,
                &knowledge_binding_id,
                "UPDATE knowledge_metadata SET value = 'dirty' WHERE key = 'index_status'; UPDATE knowledge_metadata SET value = '0' WHERE key = 'indexed_content_revision';",
                Vec::new(),
            )
            .await
            .expect("mark index dirty for recovery import");
        // RecoveryImport must detect unchanged documents while repairing only derived FTS state.
        // RecoveryImport 必须识别未变化文档，同时仅修复派生 FTS 状态。
        let recovery_import = runtime
            .import_markdown_directory(&library.id, &source_root)
            .await
            .expect("recover dirty FTS through cancellable batches");
        assert_eq!(recovery_import.changed_files, 0);
        // RecoveredJob exposes the completed batched index counters.
        // RecoveredJob 暴露已完成的批量索引计数。
        let recovered_job = runtime
            .latest_import_job(&library.id)
            .await
            .expect("load recovered import job")
            .expect("recovered import job exists");
        assert!(recovered_job.total_index_items > 0);
        assert_eq!(
            recovered_job.processed_index_items,
            recovered_job.total_index_items
        );
        assert!(
            !runtime
                .search(std::slice::from_ref(&library.id), "火山知识库", 10)
                .await
                .expect("search recovered index")
                .hits
                .is_empty()
        );

        let package_directory = temporary.path().join("packages");
        fs::create_dir(&package_directory).expect("create package export directory");
        let export = runtime
            .export_package(&library.id, &package_directory)
            .await
            .expect("export complete ready library");
        // tamperedIndexBinding keeps ready metadata while removing every physical FTS row.
        // tamperedIndexBinding 在保留 ready 元数据的同时删除全部物理 FTS 记录。
        let (tampered_index_space_id, tampered_index_binding_id) = runtime
            .ensure_library_binding(&library.id)
            .await
            .expect("activate library for FTS integrity fixture");
        client
            .execute_sqlite_script_typed(
                &tampered_index_space_id,
                &tampered_index_binding_id,
                format!("DELETE FROM {KNOWLEDGE_FTS_INDEX};"),
                Vec::new(),
            )
            .await
            .expect("remove FTS rows without changing ready metadata");
        assert!(
            runtime
                .export_package(&library.id, &package_directory)
                .await
                .is_err(),
            "export must reject ready metadata without exact FTS rows"
        );
        let broken_integrity = runtime
            .inspect_index_integrity(&library.id)
            .await
            .expect("inspect physically incomplete index");
        assert!(!broken_integrity.searchable);
        assert_eq!(broken_integrity.indexed_chunk_count, 0);
        assert_eq!(
            broken_integrity.missing_or_mismatched_count,
            first_import.chunk_count
        );
        runtime
            .repair_index(&library.id)
            .await
            .expect("repair missing FTS rows after integrity fixture");
        assert!(
            runtime
                .inspect_index_integrity(&library.id)
                .await
                .expect("inspect repaired index")
                .searchable
        );
        // DuplicateFixture proves integrity accounting and repair operate on physical row cardinality.
        // DuplicateFixture 证明完整性计数与补全会处理物理记录基数。
        client
            .execute_sqlite_script_typed(
                &tampered_index_space_id,
                &tampered_index_binding_id,
                format!(
                    "INSERT INTO {KNOWLEDGE_FTS_INDEX}(id, file_path, title, content) SELECT id, file_path, title, content FROM {KNOWLEDGE_FTS_INDEX} LIMIT 1;"
                ),
                Vec::new(),
            )
            .await
            .expect("insert one duplicate physical FTS row");
        let duplicate_integrity = runtime
            .inspect_index_integrity(&library.id)
            .await
            .expect("inspect duplicate physical FTS row");
        assert!(!duplicate_integrity.searchable);
        assert_eq!(
            duplicate_integrity.indexed_chunk_count,
            first_import.chunk_count + 1
        );
        assert_eq!(duplicate_integrity.orphan_or_mismatched_count, 1);
        runtime
            .repair_index(&library.id)
            .await
            .expect("remove duplicate physical FTS row during repair");
        assert!(
            runtime
                .inspect_index_integrity(&library.id)
                .await
                .expect("inspect index repaired from duplicate row")
                .searchable
        );
        // DetachedRebuild proves the operation continues after the initiating call returns.
        // DetachedRebuild 证明操作在发起调用返回后仍会继续执行。
        let detached_rebuild = runtime
            .start_rebuild_index(&library.id)
            .await
            .expect("start detached rebuild");
        assert_eq!(
            detached_rebuild.operation_kind,
            KnowledgeOperationKind::Rebuild
        );
        assert_eq!(detached_rebuild.state, "queued");
        // TerminalJob is observed only through durable polling, mirroring a refreshed WebUI.
        // TerminalJob 仅通过持久化轮询观察，与刷新后的 WebUI 行为一致。
        let terminal_rebuild = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let job = runtime
                    .latest_import_job(&library.id)
                    .await
                    .expect("poll detached rebuild")
                    .expect("detached rebuild job exists");
                if job.id == detached_rebuild.id && job.state == "completed" {
                    break job;
                }
                assert_ne!(job.state, "failed", "detached rebuild must not fail");
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("detached rebuild reaches terminal state");
        assert_eq!(
            terminal_rebuild.processed_index_items,
            first_import.chunk_count
        );
        // identityMismatchPath contains valid hashes but forged package/manifest provenance.
        // identityMismatchPath 包含有效摘要，但伪造了包与清单来源身份。
        let identity_mismatch_path = package_directory.join("identity-mismatch.vkb");
        write_identity_mismatch_package(Path::new(&export.archive_path), &identity_mismatch_path);
        assert!(
            runtime
                .inspect_package(&identity_mismatch_path)
                .await
                .is_err(),
            "full package inspection must reject a database identity mismatch"
        );
        let inspection = runtime
            .inspect_package(Path::new(&export.archive_path))
            .await
            .expect("inspect exported package through real SQLite validation");
        assert_eq!(inspection.package.document_count, 1);
        assert_eq!(
            inspection.conflicting_library.as_ref().map(|row| &row.id),
            Some(&library.id)
        );

        let imported = runtime
            .import_package_as_new(Path::new(&export.archive_path), "端到端知识库包副本")
            .await
            .expect("import package under a unique name");
        assert_ne!(imported.library.id, library.id);
        assert_eq!(
            imported.library.document_count,
            export.package.document_count
        );
        assert!(
            !runtime
                .search(std::slice::from_ref(&imported.library.id), "火山知识库", 10)
                .await
                .expect("search imported package without rebuilding")
                .hits
                .is_empty()
        );
        let replacement_target = runtime
            .create_library("覆盖目标知识库", "", &[])
            .await
            .expect("create replacement target");
        client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                &runtime.controller.sqlite_binding_id,
                r#"INSERT INTO projects(
                    id, name, root_path, pinned, archived, sort_order, created_at, updated_at
                ) VALUES (?, ?, ?, 0, 0, 0, ?, ?);"#,
                vec![
                    text("package-replacement-project"),
                    text("Package replacement project"),
                    text(
                        runtime
                            .controller
                            .data_root
                            .join("replacement-project")
                            .display()
                            .to_string(),
                    ),
                    text("2026-07-21T00:00:00Z"),
                    text("2026-07-21T00:00:00Z"),
                ],
            )
            .await
            .expect("insert replacement project fixture");
        runtime
            .set_project_binding(
                &replacement_target.id,
                "package-replacement-project",
                true,
                true,
            )
            .await
            .expect("bind replacement target to project");
        let replaced = runtime
            .replace_library_from_package(
                Path::new(&export.archive_path),
                &replacement_target.id,
                "覆盖目标知识库",
            )
            .await
            .expect("explicitly replace target from package");
        assert_eq!(replaced.library.id, replacement_target.id);
        assert_eq!(
            runtime
                .list_bindings_for_project("package-replacement-project")
                .await
                .expect("load preserved replacement binding")
                .len(),
            1
        );
        assert!(
            !runtime
                .search(
                    std::slice::from_ref(&replacement_target.id),
                    "火山知识库",
                    10,
                )
                .await
                .expect("search replaced package without rebuilding")
                .hits
                .is_empty()
        );
        // preparedCrash simulates process termination after the original directory becomes a backup.
        // preparedCrash 模拟原目录已成为备份后发生进程终止。
        let prepared_crash_previous = runtime
            .get_library(&replacement_target.id)
            .await
            .expect("load replacement target before prepared crash");
        let prepared_crash_operation_id = Uuid::new_v4().to_string();
        let (prepared_crash_backup, prepared_crash_journal_path, prepared_crash_backup_name) =
            runtime
                .replacement_operation_artifacts(
                    &replacement_target.id,
                    &prepared_crash_operation_id,
                )
                .expect("derive prepared crash artifacts");
        let prepared_crash_journal = KnowledgeReplacementJournal {
            version: KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION,
            target_library_id: replacement_target.id.clone(),
            backup_directory_name: prepared_crash_backup_name,
            phase: String::from(KNOWLEDGE_REPLACEMENT_PHASE_PREPARED),
            previous: prepared_crash_previous,
        };
        runtime
            .write_replacement_journal_atomic(&prepared_crash_journal_path, &prepared_crash_journal)
            .expect("persist prepared crash journal");
        runtime
            .release_library_binding(&replacement_target.id)
            .await
            .expect("release replacement target before prepared crash");
        let prepared_crash_root = runtime
            .library_root(&replacement_target.id)
            .expect("resolve prepared crash target root");
        fs::rename(&prepared_crash_root, &prepared_crash_backup)
            .expect("simulate prepared crash after backup rename");
        // preparedRecovery owns a fresh active-binding cache like a restarted service runtime.
        // preparedRecovery 像重启后的服务运行时一样持有全新的活动绑定缓存。
        let prepared_recovery = KnowledgeBaseRuntime::new(runtime.controller.clone());
        prepared_recovery
            .recover_interrupted_jobs()
            .await
            .expect("recover prepared replacement crash");
        assert!(prepared_crash_root.is_dir());
        assert!(!prepared_crash_backup.exists());
        assert!(!prepared_crash_journal_path.exists());
        assert!(
            !prepared_recovery
                .search(
                    std::slice::from_ref(&replacement_target.id),
                    "火山知识库",
                    10,
                )
                .await
                .expect("search target after prepared crash recovery")
                .hits
                .is_empty()
        );
        prepared_recovery
            .release_library_binding(&replacement_target.id)
            .await
            .expect("release prepared recovery target binding");

        // committedCrash simulates termination after registry commit but before backup cleanup.
        // committedCrash 模拟注册表提交后、备份清理前发生进程终止。
        let committed_crash_previous = runtime
            .get_library(&replacement_target.id)
            .await
            .expect("load replacement target before committed crash");
        let (committed_space_id, committed_binding_id) = runtime
            .ensure_library_binding(&replacement_target.id)
            .await
            .expect("activate replacement target before committed crash");
        client
            .execute_sqlite_script_typed(
                &committed_space_id,
                &committed_binding_id,
                "PRAGMA wal_checkpoint(TRUNCATE);",
                Vec::new(),
            )
            .await
            .expect("checkpoint replacement target before committed crash");
        runtime
            .release_library_binding(&replacement_target.id)
            .await
            .expect("release replacement target before committed crash");
        let committed_crash_operation_id = Uuid::new_v4().to_string();
        let (committed_crash_backup, committed_crash_journal_path, committed_crash_backup_name) =
            runtime
                .replacement_operation_artifacts(
                    &replacement_target.id,
                    &committed_crash_operation_id,
                )
                .expect("derive committed crash artifacts");
        fs::create_dir(&committed_crash_backup).expect("create committed crash backup");
        fs::copy(
            prepared_crash_root.join("manifest.json"),
            committed_crash_backup.join("manifest.json"),
        )
        .expect("copy committed crash manifest backup");
        fs::copy(
            prepared_crash_root.join("content.sqlite3"),
            committed_crash_backup.join("content.sqlite3"),
        )
        .expect("copy committed crash database backup");
        let committed_crash_journal = KnowledgeReplacementJournal {
            version: KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION,
            target_library_id: replacement_target.id.clone(),
            backup_directory_name: committed_crash_backup_name,
            phase: String::from(KNOWLEDGE_REPLACEMENT_PHASE_COMMITTED),
            previous: committed_crash_previous,
        };
        runtime
            .write_replacement_journal_atomic(
                &committed_crash_journal_path,
                &committed_crash_journal,
            )
            .expect("persist committed crash journal");
        let committed_recovery = KnowledgeBaseRuntime::new(runtime.controller.clone());
        committed_recovery
            .recover_interrupted_jobs()
            .await
            .expect("recover committed replacement crash");
        assert!(!committed_crash_backup.exists());
        assert!(!committed_crash_journal_path.exists());
        assert_eq!(
            committed_recovery
                .list_bindings_for_project("package-replacement-project")
                .await
                .expect("load binding after committed crash recovery")
                .len(),
            1
        );
        committed_recovery
            .release_library_binding(&replacement_target.id)
            .await
            .expect("release committed recovery target binding");

        // failedReplacement injects a database-identity failure while validating the new staging copy.
        // failedReplacement 在校验新的暂存副本期间注入数据库身份失败。
        let failed_replacement = runtime
            .replace_library_from_package(
                &identity_mismatch_path,
                &replacement_target.id,
                "覆盖目标知识库",
            )
            .await;
        assert!(failed_replacement.is_err());
        assert!(
            !runtime
                .search(
                    std::slice::from_ref(&replacement_target.id),
                    "火山知识库",
                    10,
                )
                .await
                .expect("search original target after failed replacement rollback")
                .hits
                .is_empty()
        );
        assert_eq!(
            runtime
                .list_bindings_for_project("package-replacement-project")
                .await
                .expect("load binding after failed replacement rollback")
                .len(),
            1
        );

        // OptionalPerfSource measures the production-size host path only when explicitly requested.
        // OptionalPerfSource 仅在显式请求时测量生产规模宿主路径。
        if let Some(performance_source) = std::env::var_os("VULCAN_KNOWLEDGE_PERF_SOURCE") {
            // PerformanceLibrary remains isolated from deterministic fixtures and is deleted before teardown.
            // PerformanceLibrary 与确定性夹具隔离，并在清理前删除。
            let performance_library = runtime
                .create_library("知识库集合式索引性能测试", "", &[])
                .await
                .expect("create performance library");
            let performance_started_at = Instant::now();
            let performance_report = runtime
                .import_markdown_directory(&performance_library.id, Path::new(&performance_source))
                .await
                .expect("import production-size performance source");
            let performance_elapsed = performance_started_at.elapsed();
            eprintln!(
                "knowledge performance: files={}, chunks={}, elapsed_ms={}",
                performance_report.scanned_files,
                performance_report.chunk_count,
                performance_elapsed.as_millis()
            );
            assert!(performance_report.scanned_files > 0);
            // SearchTiming verifies the formerly slow production keyword path after tokenizer caching.
            // SearchTiming 在 tokenizer 缓存后验证此前缓慢的真实关键词路径。
            let search_started_at = Instant::now();
            let performance_search = runtime
                .search(std::slice::from_ref(&performance_library.id), "等级", 20)
                .await
                .expect("search production keyword through Jieba BM25");
            let search_elapsed = search_started_at.elapsed();
            eprintln!(
                "knowledge search performance: query=等级, hits={}, elapsed_ms={}",
                performance_search.hits.len(),
                search_elapsed.as_millis()
            );
            assert!(!performance_search.hits.is_empty());
            assert!(
                search_elapsed < Duration::from_secs(3),
                "production keyword search exceeded three seconds: {search_elapsed:?}"
            );
            // RebuildTiming covers the same authoritative corpus through the progress-aware replay path.
            // RebuildTiming 通过带进度的回放路径覆盖同一份权威语料。
            let rebuild_started_at = Instant::now();
            let rebuilt_chunks = runtime
                .rebuild_index(&performance_library.id)
                .await
                .expect("rebuild production-size Jieba index");
            let rebuild_elapsed = rebuild_started_at.elapsed();
            eprintln!(
                "knowledge rebuild performance: chunks={}, elapsed_ms={}",
                rebuilt_chunks,
                rebuild_elapsed.as_millis()
            );
            assert_eq!(rebuilt_chunks, performance_report.chunk_count);
            assert!(
                runtime
                    .inspect_index_integrity(&performance_library.id)
                    .await
                    .expect("inspect performance library")
                    .searchable
            );
            runtime
                .delete_library(&performance_library.id)
                .await
                .expect("delete performance library");
        }

        runtime
            .delete_library(&imported.library.id)
            .await
            .expect("delete imported package copy");
        runtime
            .delete_library(&replacement_target.id)
            .await
            .expect("delete replacement target");

        fs::remove_file(source_root.join("guide").join("search.md"))
            .expect("remove imported source");
        let second_import = runtime
            .import_markdown_directory(&library.id, &source_root)
            .await
            .expect("synchronize source deletion");
        assert_eq!(second_import.deleted_files, 1);
        assert!(
            runtime
                .search(std::slice::from_ref(&library.id), "火山知识库", 10)
                .await
                .expect("search rebuilt empty index")
                .hits
                .is_empty()
        );
        runtime
            .delete_library(&library.id)
            .await
            .expect("delete isolated library");

        // multiSourceLibrary isolates multi-root and source-remapping assertions from package fixtures.
        // multiSourceLibrary 将多根目录与来源重新映射断言和知识库包夹具隔离。
        let multi_source_library = runtime
            .create_library("多来源增量知识库", "", &[])
            .await
            .expect("create multi-source library");
        // sourceARoot is the first independently tracked Markdown root.
        // sourceARoot 是第一个独立跟踪的 Markdown 根目录。
        let source_a_root = temporary.path().join("source-a");
        // sourceBRoot is the second independently tracked Markdown root.
        // sourceBRoot 是第二个独立跟踪的 Markdown 根目录。
        let source_b_root = temporary.path().join("source-b");
        // relocatedSourceARoot simulates the first source being moved to another disk location.
        // relocatedSourceARoot 模拟第一个来源移动到另一个磁盘位置。
        let relocated_source_a_root = temporary.path().join("source-a-relocated");
        fs::create_dir_all(&source_a_root).expect("create first source root");
        fs::create_dir_all(&source_b_root).expect("create second source root");
        fs::create_dir_all(&relocated_source_a_root).expect("create relocated source root");
        fs::write(source_a_root.join("shared.md"), "# 来源 A\n\n稳定内容。\n")
            .expect("write first source Markdown");
        fs::write(source_b_root.join("shared.md"), "# 来源 B\n\n独立内容。\n")
            .expect("write second source Markdown");
        runtime
            .import_markdown_directory(&multi_source_library.id, &source_a_root)
            .await
            .expect("import first source root");
        runtime
            .import_markdown_directory(&multi_source_library.id, &source_b_root)
            .await
            .expect("import second source root");
        // sourceARootText is the canonical host representation persisted in knowledge_sources.
        // sourceARootText 是持久化到 knowledge_sources 的规范宿主表示。
        let source_a_root_text = runtime_assets::native_host_path_text_from_path(
            &fs::canonicalize(&source_a_root).expect("canonicalize first source root"),
        );
        // initialSources proves two independent roots coexist even with the same relative file path.
        // initialSources 证明即使相对文件路径相同，两个独立根目录仍可共存。
        let initial_sources = runtime
            .list_sources(&multi_source_library.id)
            .await
            .expect("list initial multi-source roots");
        assert_eq!(initial_sources.len(), 2);
        assert!(
            initial_sources
                .iter()
                .all(|source| source.document_count == 1)
        );
        // sourceA retains the durable identity that must survive a physical path remap.
        // sourceA 保留在物理路径重新映射后仍必须不变的持久化身份。
        let source_a = initial_sources
            .iter()
            .find(|source| source.canonical_root == source_a_root_text)
            .expect("find first durable source")
            .clone();
        fs::copy(
            source_a_root.join("shared.md"),
            relocated_source_a_root.join("shared.md"),
        )
        .expect("copy unchanged Markdown into relocated root");
        // relocationReport proves byte-identical content is skipped by SHA-256 after remapping.
        // relocationReport 证明重新映射后字节完全相同的内容会被 SHA-256 跳过。
        let relocation_report = runtime
            .import_markdown_directory_for_source(
                &multi_source_library.id,
                Some(&source_a.id),
                &relocated_source_a_root,
            )
            .await
            .expect("remap first source and run incremental synchronization");
        assert_eq!(relocation_report.changed_files, 0);
        assert_eq!(relocation_report.unchanged_files, 1);
        assert_eq!(relocation_report.deleted_files, 0);
        // relocatedRootText is the new canonical path expected in the durable source row.
        // relocatedRootText 是持久化来源记录中预期的新规范路径。
        let relocated_root_text = runtime_assets::native_host_path_text_from_path(
            &fs::canonicalize(&relocated_source_a_root)
                .expect("canonicalize relocated source root"),
        );
        // remappedSources verifies only the selected source identity changed its physical root.
        // remappedSources 验证只有选中的来源身份修改了物理根目录。
        let remapped_sources = runtime
            .list_sources(&multi_source_library.id)
            .await
            .expect("list remapped source roots");
        assert_eq!(remapped_sources.len(), 2);
        assert_eq!(
            remapped_sources
                .iter()
                .find(|source| source.id == source_a.id)
                .expect("find remapped durable source")
                .canonical_root,
            relocated_root_text
        );
        // conflictingRemap proves one physical root cannot be assigned to two durable sources.
        // conflictingRemap 证明一个物理根目录不能分配给两个持久化来源。
        let conflicting_remap = runtime
            .import_markdown_directory_for_source(
                &multi_source_library.id,
                Some(&source_a.id),
                &source_b_root,
            )
            .await;
        assert!(conflicting_remap.is_err());
        fs::write(
            relocated_source_a_root.join("shared.md"),
            "# 来源 A\n\n内容已经变化。\n",
        )
        .expect("change relocated Markdown content");
        // changedHashReport proves only the modified source document is reparsed and reindexed.
        // changedHashReport 证明只有内容变化的来源文档会重新解析并建立索引。
        let changed_hash_report = runtime
            .import_markdown_directory_for_source(
                &multi_source_library.id,
                Some(&source_a.id),
                &relocated_source_a_root,
            )
            .await
            .expect("synchronize changed relocated source");
        assert_eq!(changed_hash_report.changed_files, 1);
        assert_eq!(changed_hash_report.unchanged_files, 0);
        runtime
            .delete_library(&multi_source_library.id)
            .await
            .expect("delete multi-source library");
        client.shutdown().await.expect("shutdown controller client");
    }
}
