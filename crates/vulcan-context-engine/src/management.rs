//! Read-only management query contracts for host pages and diagnostics.
//! 面向宿主页面和诊断的只读管理查询契约。

use crate::identity::{is_sha256_identity, normalize_relative_path};
use crate::model::{ChunkKind, FileStatus, SourceRange};
use crate::ports::{RefreshMode, RefreshReason};
use std::collections::BTreeSet;

/// Bound one management page without imposing a host database implementation.
/// 在不限定宿主数据库实现的情况下约束一个管理分页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcePageRequest {
    /// Opaque host cursor from the previous page.
    /// 上一页返回的宿主不透明游标。
    pub cursor: Option<String>,
    /// Maximum number of records requested by the caller.
    /// 调用方请求的最大记录数量。
    pub limit: usize,
}

impl VcePageRequest {
    /// Construct a bounded page request for a host management query.
    /// 为宿主管理查询构造有界分页请求。
    pub fn new(cursor: Option<String>, limit: usize) -> Result<Self, VceManagementContractError> {
        if limit == 0 || limit > 500 {
            return Err(VceManagementContractError::InvalidPageLimit { limit });
        }
        if cursor.as_ref().is_some_and(|value| value.trim().is_empty()) {
            return Err(VceManagementContractError::EmptyCursor);
        }
        Ok(Self { cursor, limit })
    }
}

/// Return one deterministic page while preserving an opaque continuation cursor.
/// 返回一个确定性分页，同时保留不透明的继续游标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcePage<T> {
    /// Records in stable host-defined order.
    /// 按宿主定义的稳定顺序排列的记录。
    pub items: Vec<T>,
    /// Cursor for the next page, or `None` when complete.
    /// 下一页游标，完成时为 `None`。
    pub next_cursor: Option<String>,
    /// Optional exact total when the host can provide it cheaply.
    /// 宿主可以低成本提供时的可选精确总数。
    pub total: Option<u64>,
}

/// Filter the file inventory without exposing storage-specific predicates.
/// 在不暴露存储特定谓词的情况下筛选文件清单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceFileQuery {
    /// Optional canonical path prefix.
    /// 可选的规范路径前缀。
    pub path_prefix: Option<String>,
    /// Optional file lifecycle state.
    /// 可选的文件生命周期状态。
    pub status: Option<FileStatus>,
    /// Whether only records with an auxiliary-channel failure are requested.
    /// 是否只请求辅助通道失败的记录。
    pub auxiliary_failures_only: bool,
    /// Bounded pagination contract.
    /// 有界分页契约。
    pub page: VcePageRequest,
}

/// Project one auxiliary index channel into management-safe states.
/// 将一个辅助索引通道投影为可安全管理的状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VceChannelState {
    /// The channel is fully searchable.
    /// 通道可以完整检索。
    Ready,
    /// The channel is waiting for host work.
    /// 通道正在等待宿主作业。
    Pending,
    /// The channel does not apply to this record.
    /// 此记录不适用该通道。
    NotApplicable,
    /// The channel failed independently from primary content search.
    /// 通道独立于主内容搜索发生失败。
    Failed {
        /// Explicit auxiliary-channel failure reason.
        /// 明确的辅助通道失败原因。
        reason: String,
    },
}

/// One file or Blob row used by list and statistics pages.
/// 列表和统计页面使用的一条文件或 Blob 记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceFileSummary {
    /// Canonical workspace-relative path.
    /// 规范化的工作区相对路径。
    pub path: String,
    /// Current content-addressed Blob identity.
    /// 当前内容寻址 Blob 身份。
    pub blob_name: Option<String>,
    /// Blob identity visible in the published checkpoint.
    /// 已发布检查点中可见的 Blob 身份。
    pub committed_blob_name: Option<String>,
    /// Current file lifecycle state.
    /// 当前文件生命周期状态。
    pub status: FileStatus,
    /// Workspace generation that last changed the record.
    /// 最近修改此记录的工作区代数。
    pub generation: u64,
    /// Source size in bytes.
    /// 源码字节大小。
    pub size: u64,
    /// Persisted content chunk count.
    /// 已持久化内容代码块数量。
    pub chunk_count: u64,
    /// Persisted content vector count.
    /// 已持久化内容向量数量。
    pub vector_count: u64,
    /// Exact symbol occurrence count.
    /// 精确符号出现次数。
    pub symbol_count: u64,
    /// Path-vector channel state.
    /// 路径向量通道状态。
    pub path_channel: VceChannelState,
    /// Exact-symbol channel state.
    /// 精确符号通道状态。
    pub symbol_channel: VceChannelState,
    /// Last record-specific diagnostic.
    /// 最近一项记录级诊断。
    pub last_error: Option<String>,
}

/// One Chunk row shown by a Blob detail panel.
/// Blob 详情面板显示的一条代码块记录。
#[derive(Debug, Clone, PartialEq)]
pub struct VceChunkSummary {
    /// Stable Chunk identity.
    /// 稳定代码块身份。
    pub chunk_id: String,
    /// One-based inclusive source range.
    /// 从一开始的闭区间源码范围。
    pub range: SourceRange,
    /// Unicode-scalar offset within the first source line.
    /// 首行内的 Unicode 标量偏移。
    pub start_char_offset: usize,
    /// Source chunking strategy that produced this record.
    /// 生成此记录的源码分块策略。
    pub kind: ChunkKind,
    /// Optional source preview bounded by the host.
    /// 由宿主限制长度的可选源码预览。
    pub preview: Option<String>,
    /// Whether a compatible vector exists.
    /// 是否存在兼容向量。
    pub vector_ready: bool,
}

/// Detailed read-only state for one committed or staged Blob.
/// 一个已提交或已暂存 Blob 的详细只读状态。
#[derive(Debug, Clone, PartialEq)]
pub struct VceBlobDetail {
    /// Stable Blob identity.
    /// 稳定 Blob 身份。
    pub blob_name: String,
    /// Canonical source path.
    /// 规范化源码路径。
    pub path: String,
    /// Workspace generation that produced the Blob.
    /// 生成此 Blob 的工作区代数。
    pub generation: u64,
    /// Structural analyzer identity.
    /// 结构分析器身份。
    pub structure_analyzer_id: Option<String>,
    /// Primary analyzer diagnostic when this Blob used recursive fallback.
    /// 此 Blob 使用递归回退时的主分析器诊断。
    pub structure_fallback_reason: Option<String>,
    /// Persisted chunks in stable source order.
    /// 按稳定源码顺序排列的已持久化代码块。
    pub chunks: Vec<VceChunkSummary>,
    /// Path-vector channel state.
    /// 路径向量通道状态。
    pub path_channel: VceChannelState,
    /// Exact-symbol channel state.
    /// 精确符号通道状态。
    pub symbol_channel: VceChannelState,
}

/// Identify one host lifecycle operation without using free-form phase strings.
/// 在不使用自由格式阶段字符串的情况下标识宿主生命周期操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VceOperationKind {
    /// Initial authoritative index build.
    /// 初次权威索引构建。
    InitialBuild,
    /// Incremental file or Git refresh.
    /// 增量文件或 Git 刷新。
    IncrementalRefresh,
    /// Integrity verification without mutation.
    /// 不产生修改的完整性校验。
    VerifyIntegrity,
    /// Targeted repair selected by an explicit host action.
    /// 由明确宿主操作选择的定向修复。
    Repair,
    /// Full namespace rebuild selected by an explicit host action.
    /// 由明确宿主操作选择的完整命名空间重建。
    Rebuild,
}

/// Identify one stable lifecycle phase for progress and recovery.
/// 标识一个用于进度和恢复的稳定生命周期阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VceOperationPhase {
    /// Accepted but not started.
    /// 已接收但尚未开始。
    Queued,
    /// Comparing workspace metadata and identities.
    /// 正在比较工作区元数据和身份。
    Reconciling,
    /// Reading immutable source snapshots.
    /// 正在读取不可变源码快照。
    ReadingSource,
    /// Producing chunks and exact symbols.
    /// 正在生成代码块和精确符号。
    Structuring,
    /// Calling the shared Embedding capability.
    /// 正在调用共享向量能力。
    Embedding,
    /// Persisting bounded unpublished batches.
    /// 正在持久化有界未发布批次。
    Staging,
    /// Atomically publishing a checkpoint.
    /// 正在原子发布检查点。
    Publishing,
    /// Operation completed successfully.
    /// 操作已成功完成。
    Completed,
    /// Operation was cancelled by the host.
    /// 操作已被宿主取消。
    Cancelled,
    /// Operation failed and remains observable.
    /// 操作失败并保持可观测。
    Failed,
}

/// One durable operation projection used by job and history panels.
/// 作业和历史面板使用的一项持久操作投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceOperationSummary {
    /// Stable host job identity.
    /// 稳定宿主作业身份。
    pub operation_id: String,
    /// Logical operation kind.
    /// 逻辑操作类型。
    pub kind: VceOperationKind,
    /// Current stable phase.
    /// 当前稳定阶段。
    pub phase: VceOperationPhase,
    /// Workspace generation being processed.
    /// 正在处理的工作区代数。
    pub generation: u64,
    /// Refresh mode when the operation is a refresh.
    /// 操作为刷新时使用的刷新模式。
    pub refresh_mode: Option<RefreshMode>,
    /// Refresh reason when the operation is a refresh.
    /// 操作为刷新时使用的刷新原因。
    pub refresh_reason: Option<RefreshReason>,
    /// Completed work units.
    /// 已完成工作单元数。
    pub completed_units: u64,
    /// Total planned work units.
    /// 计划工作单元总数。
    pub total_units: u64,
    /// Number of host retries already attempted.
    /// 宿主已经尝试的重试次数。
    pub attempt: u32,
    /// Whether another watcher event was coalesced into the operation.
    /// 是否有另一个监听事件被合并进此操作。
    pub coalesced: bool,
    /// Optional terminal diagnostic.
    /// 可选终止诊断。
    pub error: Option<String>,
}

/// Classify one integrity difference without prescribing a database repair statement.
/// 在不指定数据库修复语句的情况下分类一项完整性差异。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VceIntegrityIssueKind {
    /// Checkpoint references a Blob that is absent or not ready.
    /// 检查点引用了缺失或未就绪的 Blob。
    MissingBlob,
    /// Blob has incomplete content chunks or vectors.
    /// Blob 的内容代码块或向量不完整。
    IncompleteContentIndex,
    /// Auxiliary path index is absent or incompatible.
    /// 辅助路径索引缺失或不兼容。
    PathIndexMismatch,
    /// Exact symbol index is absent or incompatible.
    /// 精确符号索引缺失或不兼容。
    SymbolIndexMismatch,
    /// Persisted vector compatibility differs from the bound namespace.
    /// 持久化向量兼容性与绑定命名空间不同。
    VectorSpaceMismatch,
    /// Unpublished staging data no longer belongs to an active operation.
    /// 未发布暂存数据不再属于活动操作。
    OrphanedStaging,
}

/// One read-only integrity issue returned by the host adapter.
/// 宿主适配器返回的一项只读完整性问题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceIntegrityIssue {
    /// Stable issue identity for page selection and repair planning.
    /// 用于页面选择和修复计划的稳定问题身份。
    pub issue_id: String,
    /// Deterministic issue category.
    /// 确定性问题类别。
    pub kind: VceIntegrityIssueKind,
    /// Optional affected path.
    /// 可选受影响路径。
    pub path: Option<String>,
    /// Optional affected Blob identity.
    /// 可选受影响 Blob 身份。
    pub blob_name: Option<String>,
    /// Management-safe diagnostic.
    /// 可安全用于管理的诊断。
    pub message: String,
}

/// Select a non-executing maintenance plan.
/// 选择一个不直接执行的维护计划。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VceMaintenanceMode {
    /// Repair only explicitly identified integrity differences.
    /// 只修复明确识别的完整性差异。
    Repair,
    /// Rebuild all derived index channels in the namespace.
    /// 重建命名空间中的全部派生索引通道。
    Rebuild,
    /// Remove only orphaned unpublished staging data.
    /// 只删除孤立的未发布暂存数据。
    CleanupStaging,
}

/// One host-executable maintenance action produced by a read-only plan.
/// 只读计划生成的一项可由宿主执行的维护操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceMaintenanceAction {
    /// Stable action identity used for explicit confirmation and idempotency.
    /// 用于明确确认和幂等处理的稳定操作身份。
    pub action_id: String,
    /// Human-readable management description.
    /// 面向管理的可读说明。
    pub description: String,
    /// Affected Blob identities in deterministic order.
    /// 按确定性顺序排列的受影响 Blob 身份。
    pub blob_names: Vec<String>,
    /// Whether executing this action removes persisted derived data.
    /// 执行此操作是否会删除已持久化派生数据。
    pub destructive: bool,
}

/// Read-only plan that a VulcanCode manager may present before execution.
/// VulcanCode 管理器可以在执行前展示的只读计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceMaintenancePlan {
    /// Requested maintenance mode.
    /// 请求的维护模式。
    pub mode: VceMaintenanceMode,
    /// Stable actions that require a separate host command to execute.
    /// 需要通过独立宿主命令执行的稳定操作。
    pub actions: Vec<VceMaintenanceAction>,
    /// Number of affected Blobs.
    /// 受影响 Blob 数量。
    pub affected_blob_count: u64,
}

/// Select one read-only management operation.
/// 选择一项只读管理操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VceManagementQuery {
    /// List file and Blob state.
    /// 列出文件和 Blob 状态。
    Files(VceFileQuery),
    /// Read one Blob detail by exact identity.
    /// 按精确身份读取一个 Blob 详情。
    BlobDetail {
        /// Exact content-addressed Blob identity to inspect.
        /// 要检查的精确内容寻址 Blob 身份。
        blob_name: String,
    },
    /// List durable operation history.
    /// 列出持久操作历史。
    Operations(VcePageRequest),
    /// List integrity issues.
    /// 列出完整性问题。
    IntegrityIssues(VcePageRequest),
    /// Build a read-only maintenance plan.
    /// 构建只读维护计划。
    MaintenancePlan {
        /// Read-only maintenance strategy to evaluate.
        /// 要评估的只读维护策略。
        mode: VceMaintenanceMode,
    },
}

/// Request one host-backed read-only management query.
/// 请求一项由宿主支持的只读管理查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceManagementQueryRequest {
    /// Stable operation identity for diagnostics and response validation.
    /// 用于诊断和响应校验的稳定操作身份。
    pub operation_id: String,
    /// Exact query selected by the manager.
    /// 管理器选择的精确查询。
    pub query: VceManagementQuery,
}

/// Return one typed management result without exposing raw SQL or LanceDB records.
/// 返回一项类型化管理结果，但不暴露原始 SQL 或 LanceDB 记录。
#[derive(Debug, Clone, PartialEq)]
pub enum VceManagementQueryResult {
    /// File and Blob inventory page.
    /// 文件和 Blob 清单分页。
    Files(VcePage<VceFileSummary>),
    /// Optional Blob detail when the identity still exists.
    /// 身份仍存在时的可选 Blob 详情。
    BlobDetail(Option<VceBlobDetail>),
    /// Durable operation history page.
    /// 持久操作历史分页。
    Operations(VcePage<VceOperationSummary>),
    /// Integrity issue page.
    /// 完整性问题分页。
    IntegrityIssues(VcePage<VceIntegrityIssue>),
    /// Read-only maintenance plan.
    /// 只读维护计划。
    MaintenancePlan(VceMaintenancePlan),
}

/// Response for one typed management query.
/// 一项类型化管理查询的响应。
#[derive(Debug, Clone, PartialEq)]
pub struct VceManagementQueryResponse {
    /// Operation identity copied from the request.
    /// 从请求复制的操作身份。
    pub operation_id: String,
    /// Typed result corresponding exactly to the request variant.
    /// 与请求变体精确对应的类型化结果。
    pub result: VceManagementQueryResult,
}

/// Errors detected before or after a host management query.
/// 在宿主管理查询前后检测到的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VceManagementContractError {
    /// Requested page size is outside the stable safety range.
    /// 请求分页大小超出稳定安全范围。
    InvalidPageLimit {
        /// Requested page size outside the accepted range.
        /// 超出允许范围的请求分页大小。
        limit: usize,
    },
    /// An opaque cursor was present but empty.
    /// 提供了不透明游标但其内容为空。
    EmptyCursor,
    /// Request and response variants do not correspond.
    /// 请求和响应变体不对应。
    ResultKindMismatch,
    /// Host returned more records than the requested page allows.
    /// 宿主返回的记录数超过请求分页限制。
    PageOverflow {
        /// Maximum item count declared by the request.
        /// 请求声明的最大条目数量。
        limit: usize,
        /// Item count returned by the host.
        /// 宿主实际返回的条目数量。
        actual: usize,
    },
    /// A typed management request violates its declared contract.
    /// 类型化管理请求违反了其声明契约。
    InvalidRequest {
        /// Explicit request contract violation.
        /// 明确的请求契约违反原因。
        reason: String,
    },
    /// A typed management result contains malformed or unrequested data.
    /// 类型化管理结果包含畸形或未请求的数据。
    InvalidResponse {
        /// Explicit response contract violation.
        /// 明确的响应契约违反原因。
        reason: String,
    },
}

impl std::fmt::Display for VceManagementContractError {
    /// Format one deterministic management contract error.
    /// 格式化一项确定性的管理契约错误。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPageLimit { limit } => {
                write!(
                    formatter,
                    "management page limit {limit} is outside 1..=500"
                )
            }
            Self::EmptyCursor => formatter.write_str("management cursor must not be empty"),
            Self::ResultKindMismatch => {
                formatter.write_str("management response kind does not match the request")
            }
            Self::PageOverflow { limit, actual } => write!(
                formatter,
                "management response returned {actual} records above limit {limit}"
            ),
            Self::InvalidRequest { reason } => {
                write!(formatter, "invalid management request: {reason}")
            }
            Self::InvalidResponse { reason } => {
                write!(formatter, "invalid management response: {reason}")
            }
        }
    }
}

impl std::error::Error for VceManagementContractError {}

/// Validate one typed management query before it reaches host storage.
/// 在类型化管理查询到达宿主存储前校验它。
pub fn validate_management_query(
    query: &VceManagementQuery,
) -> Result<(), VceManagementContractError> {
    match query {
        VceManagementQuery::Files(request) => {
            validate_page_request(&request.page)?;
            if let Some(prefix) = request.path_prefix.as_deref() {
                validate_canonical_path(prefix, "file query path_prefix")
                    .map_err(|reason| VceManagementContractError::InvalidRequest { reason })?;
            }
        }
        VceManagementQuery::BlobDetail { blob_name } => {
            if !is_sha256_identity(blob_name) {
                return Err(VceManagementContractError::InvalidRequest {
                    reason: "Blob detail identity must be canonical lowercase SHA-256".to_owned(),
                });
            }
        }
        VceManagementQuery::Operations(page) | VceManagementQuery::IntegrityIssues(page) => {
            validate_page_request(page)?
        }
        VceManagementQuery::MaintenancePlan { .. } => {}
    }
    Ok(())
}

/// Validate response type and page bounds before data reaches a management page.
/// 在数据进入管理页面前校验响应类型和分页边界。
pub fn validate_management_response(
    query: &VceManagementQuery,
    result: &VceManagementQueryResult,
) -> Result<(), VceManagementContractError> {
    validate_management_query(query)?;
    let page_size = match (query, result) {
        (VceManagementQuery::Files(request), VceManagementQueryResult::Files(page)) => {
            validate_page_response(&request.page, page)?;
            let mut paths = BTreeSet::new();
            for item in &page.items {
                validate_file_summary(item)?;
                if !paths.insert(item.path.clone()) {
                    return Err(VceManagementContractError::InvalidResponse {
                        reason: format!("duplicate file row `{}`", item.path),
                    });
                }
                if request
                    .path_prefix
                    .as_deref()
                    .is_some_and(|prefix| !path_matches_prefix(&item.path, prefix))
                    || request.status.is_some_and(|status| item.status != status)
                    || (request.auxiliary_failures_only
                        && !matches!(item.path_channel, VceChannelState::Failed { .. })
                        && !matches!(item.symbol_channel, VceChannelState::Failed { .. }))
                {
                    return Err(VceManagementContractError::InvalidResponse {
                        reason: format!("file row `{}` does not match the query filter", item.path),
                    });
                }
            }
            Some((request.page.limit, page.items.len()))
        }
        (VceManagementQuery::Operations(request), VceManagementQueryResult::Operations(page)) => {
            validate_page_response(request, page)?;
            let mut operation_ids = BTreeSet::new();
            for item in &page.items {
                validate_operation_summary(item)?;
                if !operation_ids.insert(item.operation_id.clone()) {
                    return Err(VceManagementContractError::InvalidResponse {
                        reason: format!("duplicate operation `{}`", item.operation_id),
                    });
                }
            }
            Some((request.limit, page.items.len()))
        }
        (
            VceManagementQuery::IntegrityIssues(request),
            VceManagementQueryResult::IntegrityIssues(page),
        ) => {
            validate_page_response(request, page)?;
            let mut issue_ids = BTreeSet::new();
            for item in &page.items {
                validate_integrity_issue(item)?;
                if !issue_ids.insert(item.issue_id.clone()) {
                    return Err(VceManagementContractError::InvalidResponse {
                        reason: format!("duplicate integrity issue `{}`", item.issue_id),
                    });
                }
            }
            Some((request.limit, page.items.len()))
        }
        (
            VceManagementQuery::BlobDetail { blob_name },
            VceManagementQueryResult::BlobDetail(detail),
        ) => {
            if let Some(detail) = detail {
                validate_blob_detail(blob_name, detail)?;
            }
            None
        }
        (
            VceManagementQuery::MaintenancePlan { .. },
            VceManagementQueryResult::MaintenancePlan(_),
        ) => {
            if let (
                VceManagementQuery::MaintenancePlan { mode },
                VceManagementQueryResult::MaintenancePlan(plan),
            ) = (query, result)
            {
                validate_maintenance_plan(*mode, plan)?;
            }
            None
        }
        _ => return Err(VceManagementContractError::ResultKindMismatch),
    };
    if let Some((limit, actual)) = page_size
        && actual > limit
    {
        return Err(VceManagementContractError::PageOverflow { limit, actual });
    }
    Ok(())
}

/// Revalidate a publicly constructible page request at the host boundary.
/// 在宿主边界重新校验可公开构造的分页请求。
fn validate_page_request(page: &VcePageRequest) -> Result<(), VceManagementContractError> {
    VcePageRequest::new(page.cursor.clone(), page.limit).map(|_| ())
}

/// Validate generic page metadata independently from its row type.
/// 独立于行类型校验通用分页元数据。
fn validate_page_response<T>(
    request: &VcePageRequest,
    page: &VcePage<T>,
) -> Result<(), VceManagementContractError> {
    if page.items.len() > request.limit {
        return Err(VceManagementContractError::PageOverflow {
            limit: request.limit,
            actual: page.items.len(),
        });
    }
    if page
        .next_cursor
        .as_ref()
        .is_some_and(|cursor| cursor.trim().is_empty())
        || page.next_cursor.is_some() && page.next_cursor == request.cursor
    {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "next_cursor must be non-empty and advance beyond the request cursor"
                .to_owned(),
        });
    }
    if page
        .total
        .is_some_and(|total| total < page.items.len() as u64)
    {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "page total is smaller than the returned item count".to_owned(),
        });
    }
    Ok(())
}

/// Validate one file inventory row returned by host storage.
/// 校验宿主存储返回的一条文件清单记录。
fn validate_file_summary(item: &VceFileSummary) -> Result<(), VceManagementContractError> {
    validate_canonical_path(&item.path, "file row path")
        .map_err(|reason| VceManagementContractError::InvalidResponse { reason })?;
    if (item.status == FileStatus::Present) != item.blob_name.is_some()
        || item.generation == 0
        || item.vector_count > item.chunk_count
        || (item.chunk_count == 0 && item.symbol_count > 0)
        || item
            .last_error
            .as_ref()
            .is_some_and(|error| error.trim().is_empty())
    {
        return Err(VceManagementContractError::InvalidResponse {
            reason: format!(
                "file row `{}` violates lifecycle state invariants",
                item.path
            ),
        });
    }
    for identity in [
        item.blob_name.as_deref(),
        item.committed_blob_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !is_sha256_identity(identity) {
            return Err(VceManagementContractError::InvalidResponse {
                reason: format!("file row `{}` has a noncanonical Blob identity", item.path),
            });
        }
    }
    validate_channel_state(&item.path_channel)?;
    validate_channel_state(&item.symbol_channel)?;
    Ok(())
}

/// Validate one Blob detail result against the exact requested identity.
/// 根据精确请求身份校验一项 Blob 详情结果。
fn validate_blob_detail(
    requested_blob: &str,
    detail: &VceBlobDetail,
) -> Result<(), VceManagementContractError> {
    if detail.blob_name != requested_blob || !is_sha256_identity(&detail.blob_name) {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "Blob detail identity does not match the request".to_owned(),
        });
    }
    if detail.generation == 0 {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "Blob detail generation must be positive".to_owned(),
        });
    }
    validate_canonical_path(&detail.path, "Blob detail path")
        .map_err(|reason| VceManagementContractError::InvalidResponse { reason })?;
    validate_channel_state(&detail.path_channel)?;
    validate_channel_state(&detail.symbol_channel)?;
    if detail
        .structure_analyzer_id
        .as_ref()
        .is_some_and(|analyzer_id| analyzer_id.trim().is_empty())
        || detail
            .structure_fallback_reason
            .as_ref()
            .is_some_and(|reason| reason.trim().is_empty())
    {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "structure fallback diagnostic must not be empty".to_owned(),
        });
    }
    if detail.structure_analyzer_id.is_none()
        && (!detail.chunks.is_empty() || detail.structure_fallback_reason.is_some())
    {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "structured Blob detail must identify its analyzer".to_owned(),
        });
    }
    let mut chunk_ids = BTreeSet::new();
    let mut previous_location = None;
    for chunk in &detail.chunks {
        let location = (
            chunk.range.start_line,
            chunk.start_char_offset,
            chunk.range.end_line,
            chunk.chunk_id.as_str(),
        );
        if !is_sha256_identity(&chunk.chunk_id)
            || chunk.range.start_line == 0
            || chunk.range.end_line < chunk.range.start_line
            || chunk.kind == ChunkKind::Path
            || chunk
                .preview
                .as_ref()
                .is_some_and(|preview| preview.trim().is_empty())
            || !chunk_ids.insert(chunk.chunk_id.clone())
            || previous_location.is_some_and(|previous| location <= previous)
        {
            return Err(VceManagementContractError::InvalidResponse {
                reason: format!("Blob detail contains invalid Chunk `{}`", chunk.chunk_id),
            });
        }
        previous_location = Some(location);
    }
    Ok(())
}

/// Validate one operation history record without assuming a scheduler implementation.
/// 在不假设调度器实现的情况下校验一条操作历史记录。
fn validate_operation_summary(
    item: &VceOperationSummary,
) -> Result<(), VceManagementContractError> {
    let is_refresh = matches!(
        item.kind,
        VceOperationKind::InitialBuild | VceOperationKind::IncrementalRefresh
    );
    if item.operation_id.trim().is_empty()
        || item.attempt == 0
        || item.completed_units > item.total_units
        || (item.phase == VceOperationPhase::Completed
            && (item.completed_units != item.total_units || item.error.is_some()))
        || is_refresh != (item.refresh_mode.is_some() && item.refresh_reason.is_some())
        || (item.phase == VceOperationPhase::Failed
            && item
                .error
                .as_ref()
                .is_none_or(|error| error.trim().is_empty()))
        || item
            .error
            .as_ref()
            .is_some_and(|error| error.trim().is_empty())
    {
        return Err(VceManagementContractError::InvalidResponse {
            reason: format!(
                "operation `{}` violates the job contract",
                item.operation_id
            ),
        });
    }
    Ok(())
}

/// Validate one integrity issue returned by the host adapter.
/// 校验宿主适配器返回的一项完整性问题。
fn validate_integrity_issue(item: &VceIntegrityIssue) -> Result<(), VceManagementContractError> {
    if item.issue_id.trim().is_empty() || item.message.trim().is_empty() {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "integrity issue identity and message must not be empty".to_owned(),
        });
    }
    if let Some(path) = item.path.as_deref() {
        validate_canonical_path(path, "integrity issue path")
            .map_err(|reason| VceManagementContractError::InvalidResponse { reason })?;
    }
    if item
        .blob_name
        .as_deref()
        .is_some_and(|identity| !is_sha256_identity(identity))
    {
        return Err(VceManagementContractError::InvalidResponse {
            reason: format!("integrity issue `{}` has an invalid Blob", item.issue_id),
        });
    }
    Ok(())
}

/// Validate a read-only maintenance plan without executing any action.
/// 在不执行任何操作的情况下校验只读维护计划。
fn validate_maintenance_plan(
    requested_mode: VceMaintenanceMode,
    plan: &VceMaintenancePlan,
) -> Result<(), VceManagementContractError> {
    if plan.mode != requested_mode {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "maintenance plan mode does not match the request".to_owned(),
        });
    }
    let mut action_ids = BTreeSet::new();
    let mut affected_blobs = BTreeSet::new();
    for action in &plan.actions {
        if action.action_id.trim().is_empty()
            || action.description.trim().is_empty()
            || !action_ids.insert(action.action_id.clone())
        {
            return Err(VceManagementContractError::InvalidResponse {
                reason: "maintenance plan contains an invalid or duplicate action".to_owned(),
            });
        }
        let blobs = action.blob_names.iter().collect::<BTreeSet<_>>();
        if blobs.len() != action.blob_names.len()
            || blobs.iter().any(|identity| !is_sha256_identity(identity))
        {
            return Err(VceManagementContractError::InvalidResponse {
                reason: format!(
                    "maintenance action `{}` has invalid Blobs",
                    action.action_id
                ),
            });
        }
        affected_blobs.extend(action.blob_names.iter().cloned());
    }
    if plan.affected_blob_count != affected_blobs.len() as u64 {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "maintenance plan affected Blob count does not match its actions".to_owned(),
        });
    }
    Ok(())
}

/// Validate one independent auxiliary-channel diagnostic.
/// 校验一项独立辅助通道诊断。
fn validate_channel_state(state: &VceChannelState) -> Result<(), VceManagementContractError> {
    if matches!(state, VceChannelState::Failed { reason } if reason.trim().is_empty()) {
        return Err(VceManagementContractError::InvalidResponse {
            reason: "failed auxiliary channel must include a diagnostic".to_owned(),
        });
    }
    Ok(())
}

/// Validate that one path is already in canonical workspace-relative form.
/// 校验一个路径已经采用规范工作区相对形式。
fn validate_canonical_path(path: &str, field: &str) -> Result<(), String> {
    match normalize_relative_path(path) {
        Ok(normalized) if normalized == path => Ok(()),
        Ok(_) => Err(format!("{field} `{path}` is not canonical")),
        Err(error) => Err(format!("{field} `{path}` is invalid: {error}")),
    }
}

/// Match one canonical path against an exact path-or-directory prefix.
/// 根据精确路径或目录前缀匹配一个规范路径。
fn path_matches_prefix(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Verify page limits reject unbounded management queries.
    /// 验证分页限制会拒绝无界管理查询。
    #[test]
    fn page_request_is_bounded() {
        assert!(VcePageRequest::new(None, 1).is_ok());
        assert!(VcePageRequest::new(None, 0).is_err());
        assert!(VcePageRequest::new(None, 501).is_err());
    }

    /// Verify a host page cannot return the same opaque cursor indefinitely.
    /// 验证宿主分页不能无限返回同一个不透明游标。
    #[test]
    fn page_response_cursor_must_advance() {
        let request = VcePageRequest::new(Some("cursor-1".to_owned()), 1).unwrap();
        let page = VcePage::<VceFileSummary> {
            items: Vec::new(),
            next_cursor: Some("cursor-1".to_owned()),
            total: Some(0),
        };

        assert!(validate_page_response(&request, &page).is_err());
    }

    /// Verify response variants cannot be projected into the wrong page.
    /// 验证响应变体不能被投影到错误页面。
    #[test]
    fn response_kind_must_match_query() {
        let query = VceManagementQuery::BlobDetail {
            blob_name: crate::identity::content_hash("blob"),
        };
        let result = VceManagementQueryResult::IntegrityIssues(VcePage {
            items: Vec::new(),
            next_cursor: None,
            total: Some(0),
        });
        assert_eq!(
            validate_management_response(&query, &result),
            Err(VceManagementContractError::ResultKindMismatch)
        );
    }

    /// Verify public struct construction cannot bypass page and Blob request validation.
    /// 验证公开结构体构造不能绕过分页与 Blob 请求校验。
    #[test]
    fn query_validation_rechecks_public_fields() {
        let files = VceManagementQuery::Files(VceFileQuery {
            path_prefix: Some("../src".to_owned()),
            status: None,
            auxiliary_failures_only: false,
            page: VcePageRequest {
                cursor: None,
                limit: 1,
            },
        });
        assert!(matches!(
            validate_management_query(&files),
            Err(VceManagementContractError::InvalidRequest { .. })
        ));
        let operations = VceManagementQuery::Operations(VcePageRequest {
            cursor: None,
            limit: 0,
        });
        assert_eq!(
            validate_management_query(&operations),
            Err(VceManagementContractError::InvalidPageLimit { limit: 0 })
        );
    }

    /// Verify host file rows cannot escape typed path filters.
    /// 验证宿主文件记录不能逃逸类型化路径筛选条件。
    #[test]
    fn file_response_must_match_requested_filter() {
        let query = VceManagementQuery::Files(VceFileQuery {
            path_prefix: Some("src".to_owned()),
            status: Some(FileStatus::Present),
            auxiliary_failures_only: false,
            page: VcePageRequest::new(None, 1).unwrap(),
        });
        let result = VceManagementQueryResult::Files(VcePage {
            items: vec![VceFileSummary {
                path: "tests/test.rs".to_owned(),
                blob_name: Some(crate::identity::content_hash("source")),
                committed_blob_name: None,
                status: FileStatus::Present,
                generation: 1,
                size: 6,
                chunk_count: 1,
                vector_count: 1,
                symbol_count: 0,
                path_channel: VceChannelState::Ready,
                symbol_channel: VceChannelState::NotApplicable,
                last_error: None,
            }],
            next_cursor: None,
            total: Some(1),
        });
        assert!(matches!(
            validate_management_response(&query, &result),
            Err(VceManagementContractError::InvalidResponse { .. })
        ));
    }

    /// Verify management rows, failed jobs, and maintenance counts remain self-consistent.
    /// 验证管理行、失败作业和维护计数保持内部一致。
    #[test]
    fn rejects_internally_inconsistent_management_state() {
        let file = VceFileSummary {
            path: "src/lib.rs".to_owned(),
            blob_name: None,
            committed_blob_name: None,
            status: FileStatus::Present,
            generation: 1,
            size: 1,
            chunk_count: 0,
            vector_count: 0,
            symbol_count: 0,
            path_channel: VceChannelState::Pending,
            symbol_channel: VceChannelState::Pending,
            last_error: None,
        };
        assert!(validate_file_summary(&file).is_err());

        let mut file = file;
        file.blob_name = Some(crate::identity::content_hash("file blob"));
        file.symbol_count = 1;
        assert!(validate_file_summary(&file).is_err());

        let operation = VceOperationSummary {
            operation_id: "refresh-1".to_owned(),
            kind: VceOperationKind::InitialBuild,
            phase: VceOperationPhase::Failed,
            generation: 1,
            refresh_mode: Some(RefreshMode::Full),
            refresh_reason: Some(RefreshReason::Initial),
            completed_units: 0,
            total_units: 1,
            attempt: 1,
            coalesced: false,
            error: None,
        };
        assert!(validate_operation_summary(&operation).is_err());

        let blob_name = crate::identity::content_hash("blob");
        let plan = VceMaintenancePlan {
            mode: VceMaintenanceMode::Repair,
            actions: vec![VceMaintenanceAction {
                action_id: "repair-1".to_owned(),
                description: "Repair one Blob".to_owned(),
                blob_names: vec![blob_name],
                destructive: true,
            }],
            affected_blob_count: 0,
        };
        assert!(validate_maintenance_plan(VceMaintenanceMode::Repair, &plan).is_err());

        let blob_name = crate::identity::content_hash("ordered blob");
        let mut detail = VceBlobDetail {
            blob_name: blob_name.clone(),
            path: "src/lib.rs".to_owned(),
            generation: 1,
            structure_analyzer_id: Some("recursive".to_owned()),
            structure_fallback_reason: None,
            chunks: vec![
                VceChunkSummary {
                    chunk_id: crate::identity::content_hash("second"),
                    range: SourceRange::new(2, 2).unwrap(),
                    start_char_offset: 0,
                    kind: ChunkKind::Recursive,
                    preview: None,
                    vector_ready: true,
                },
                VceChunkSummary {
                    chunk_id: crate::identity::content_hash("first"),
                    range: SourceRange::new(1, 1).unwrap(),
                    start_char_offset: 0,
                    kind: ChunkKind::Recursive,
                    preview: None,
                    vector_ready: true,
                },
            ],
            path_channel: VceChannelState::Ready,
            symbol_channel: VceChannelState::Ready,
        };
        assert!(validate_blob_detail(&blob_name, &detail).is_err());

        detail.chunks.sort_by(|left, right| {
            (
                left.range.start_line,
                left.start_char_offset,
                left.range.end_line,
                left.chunk_id.as_str(),
            )
                .cmp(&(
                    right.range.start_line,
                    right.start_char_offset,
                    right.range.end_line,
                    right.chunk_id.as_str(),
                ))
        });
        detail.chunks[0].kind = ChunkKind::Path;
        assert!(validate_blob_detail(&blob_name, &detail).is_err());

        detail.chunks[0].kind = ChunkKind::Recursive;
        detail.structure_analyzer_id = None;
        assert!(validate_blob_detail(&blob_name, &detail).is_err());

        detail.chunks.clear();
        detail.structure_fallback_reason = Some("primary analyzer failed".to_owned());
        assert!(validate_blob_detail(&blob_name, &detail).is_err());

        detail.structure_analyzer_id = Some("recursive".to_owned());
        detail.structure_fallback_reason = None;
        detail.chunks = vec![
            VceChunkSummary {
                chunk_id: crate::identity::content_hash("late offset"),
                range: SourceRange::new(1, 2).unwrap(),
                start_char_offset: 4,
                kind: ChunkKind::Recursive,
                preview: None,
                vector_ready: true,
            },
            VceChunkSummary {
                chunk_id: crate::identity::content_hash("early offset"),
                range: SourceRange::new(1, 3).unwrap(),
                start_char_offset: 0,
                kind: ChunkKind::Recursive,
                preview: None,
                vector_ready: true,
            },
        ];
        assert!(validate_blob_detail(&blob_name, &detail).is_err());
    }

    /// Verify completed operation history cannot retain unfinished work or an error.
    /// 验证已完成操作历史不能保留未完成工作或错误。
    #[test]
    fn completed_operation_requires_full_progress_without_error() {
        let mut operation = VceOperationSummary {
            operation_id: "operation-1".to_owned(),
            kind: VceOperationKind::VerifyIntegrity,
            phase: VceOperationPhase::Completed,
            generation: 1,
            refresh_mode: None,
            refresh_reason: None,
            completed_units: 1,
            total_units: 2,
            attempt: 1,
            coalesced: false,
            error: None,
        };
        assert!(validate_operation_summary(&operation).is_err());

        operation.completed_units = 2;
        operation.error = Some("stale failure".to_owned());
        assert!(validate_operation_summary(&operation).is_err());

        operation.error = None;
        assert!(validate_operation_summary(&operation).is_ok());
    }
}
