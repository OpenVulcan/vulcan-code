use super::*;

/// Event emitted when one project's code-semantic lifecycle or checkpoint changes.
/// 单个项目的代码语义生命周期或检查点变化时发送的事件。
pub const VCE_PROJECT_CHANGED_EVENT: &str = "vce.project.changed";

/// Persisted project-owned settings for the native code-semantic runtime.
/// 原生代码语义运行时使用的项目自有持久化设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectSettingsSummary {
    /// Whether semantic indexing is enabled for this project.
    /// 此项目是否启用语义索引。
    pub enabled: bool,
    /// Optimistic settings revision.
    /// 乐观设置修订号。
    pub revision: u64,
    /// Whether the initial state was migrated from the former native CBM switch.
    /// 初始状态是否由原生 CBM 旧开关迁移而来。
    pub migrated_from_codebase_memory: bool,
    /// Maximum source file size admitted for indexing.
    /// 索引允许接收的最大源码文件字节数。
    pub max_file_bytes: u64,
    /// Additional project-relative gitignore-style rules.
    /// 附加的项目相对 gitignore 风格规则。
    pub extra_ignore_patterns: Vec<String>,
    /// Whether optional LLM query rewriting is allowed.
    /// 是否允许可选 LLM 查询改写。
    pub allow_query_rewrite: bool,
    /// Whether optional host reranking is allowed.
    /// 是否允许可选宿主重排。
    pub allow_rerank: bool,
    /// Maximum concurrent embedding batches for this project.
    /// 此项目的最大并发向量批次数。
    pub embedding_max_concurrency: usize,
    /// Maximum concurrent query variants for one retrieval.
    /// 单次检索的最大并发查询变体数。
    pub query_max_concurrency: usize,
    /// Maximum selected code hits returned by one retrieval.
    /// 单次检索返回的最大代码命中数。
    pub max_results: usize,
    /// Aggregate character budget for rendered model context.
    /// 渲染模型上下文的总字符预算。
    pub max_context_chars: usize,
}

/// User-editable VCE settings without host-owned migration or revision fields.
/// 不含宿主持有迁移标记和修订字段的用户可编辑 VCE 设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectSettingsInput {
    /// Whether semantic indexing should be enabled.
    /// 是否应启用语义索引。
    pub enabled: bool,
    /// Maximum source file size admitted for indexing.
    /// 索引允许接收的最大源码文件字节数。
    pub max_file_bytes: u64,
    /// Additional project-relative gitignore-style rules.
    /// 附加的项目相对 gitignore 风格规则。
    pub extra_ignore_patterns: Vec<String>,
    /// Whether optional LLM query rewriting is allowed.
    /// 是否允许可选 LLM 查询改写。
    pub allow_query_rewrite: bool,
    /// Whether optional host reranking is allowed.
    /// 是否允许可选宿主重排。
    pub allow_rerank: bool,
    /// Maximum concurrent embedding batches for this project.
    /// 此项目的最大并发向量批次数。
    pub embedding_max_concurrency: usize,
    /// Maximum concurrent query variants for one retrieval.
    /// 单次检索的最大并发查询变体数。
    pub query_max_concurrency: usize,
    /// Maximum selected code hits returned by one retrieval.
    /// 单次检索返回的最大代码命中数。
    pub max_results: usize,
    /// Aggregate character budget for rendered model context.
    /// 渲染模型上下文的总字符预算。
    pub max_context_chars: usize,
}

/// Optimistic command that replaces one project's complete editable VCE settings.
/// 以乐观锁替换单个项目完整可编辑 VCE 设置的命令。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectSettingsUpdateRequest {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Settings revision observed by the caller.
    /// 调用方观察到的设置修订号。
    pub expected_revision: u64,
    /// Complete next editable settings document.
    /// 完整的下一版可编辑设置文档。
    pub settings: VceProjectSettingsInput,
}

/// Exact project selection used by read-only VCE service methods.
/// 只读 VCE 服务方法使用的精确项目选择。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectRequest {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
}

/// Lifecycle state of one project-owned VCE runtime.
/// 单个项目自有 VCE 运行时的生命周期状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VceProjectLifecycleState {
    /// Semantic indexing is disabled by project settings.
    /// 项目设置已禁用语义索引。
    Disabled,
    /// Runtime construction and prerequisite validation are in progress.
    /// 正在构造运行时并校验前置条件。
    Starting,
    /// Persisted state and committed checkpoint are being restored.
    /// 正在恢复持久化状态与已提交检查点。
    Restoring,
    /// A private full or incremental refresh is running.
    /// 正在执行私有全量或增量刷新。
    Indexing,
    /// A committed checkpoint is available for retrieval.
    /// 已有可用于检索的已提交检查点。
    Ready,
    /// Retrieval is available while optional AI stages are degraded.
    /// 检索可用，但可选 AI 阶段处于降级状态。
    DegradedOptionalAi,
    /// Required host AI or project configuration is unavailable.
    /// 必需宿主 AI 或项目配置不可用。
    BlockedConfiguration,
    /// Persisted state is incompatible and requires explicit rebuilding.
    /// 持久化状态不兼容，需要显式重建。
    RebuildRequired,
    /// The latest host or functional operation failed.
    /// 最近一次宿主或功能操作失败。
    Failed,
    /// The runtime is rejecting new work while shutting down.
    /// 运行时正在关闭并拒绝新工作。
    Stopping,
}

/// Lightweight engine state nested in management-safe project status.
/// 嵌入管理安全项目状态的轻量引擎状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VceEngineStateSummary {
    /// No committed checkpoint exists yet.
    /// 尚无已提交检查点。
    Cold,
    /// A committed snapshot is available.
    /// 已有可用的已提交快照。
    Ready,
    /// Files changed after the last commit.
    /// 文件在最近一次提交后发生变化。
    PendingRefresh,
    /// A refresh is in progress.
    /// 正在执行刷新。
    Refreshing,
    /// A retrieval is in progress.
    /// 正在执行检索。
    Retrieving,
    /// The latest functional operation failed.
    /// 最近一次功能操作失败。
    Failed,
}

/// Small project summary safe to embed in the main workbench snapshot.
/// 可安全嵌入工作台主快照的小型项目摘要。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectStatusSummary {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Whether project settings enable VCE.
    /// 项目设置是否启用 VCE。
    pub enabled: bool,
    /// Settings revision represented by this status.
    /// 此状态表示的设置修订号。
    pub settings_revision: u64,
    /// Current host lifecycle state.
    /// 当前宿主生命周期状态。
    pub lifecycle: VceProjectLifecycleState,
    /// Current functional engine state when active.
    /// 激活时的当前功能引擎状态。
    pub engine_state: Option<VceEngineStateSummary>,
    /// Current workspace generation when active.
    /// 激活时的当前工作区代数。
    pub workspace_generation: Option<u64>,
    /// Published checkpoint version when available.
    /// 可用时的已发布检查点版本。
    pub checkpoint_version: Option<u64>,
    /// Number of currently present source files.
    /// 当前存在的源码文件数量。
    pub present_files: u64,
    /// Number of Blob members in the published checkpoint.
    /// 已发布检查点中的 Blob 成员数量。
    pub checkpoint_blobs: u64,
    /// Latest bounded project error.
    /// 最近一条有界项目错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Host AI capability summary without provider or credential details.
/// 不含 Provider 或凭据详情的宿主 AI 能力摘要。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceAiCapabilitySummary {
    /// Whether the shared capability registration is active.
    /// 共享能力注册是否处于活动状态。
    pub registered: bool,
    /// Whether required Embedding is available.
    /// 必需的向量能力是否可用。
    pub embedding_available: bool,
    /// Whether optional query rewriting is available.
    /// 可选查询改写是否可用。
    pub query_rewrite_available: bool,
    /// Whether optional reranking is available.
    /// 可选重排是否可用。
    pub rerank_available: bool,
}

/// Aggregate persisted VCE index counts.
/// VCE 持久化索引聚合计数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceIndexCountsSummary {
    /// Retained file records.
    /// 保留的文件记录数。
    pub files: u64,
    /// Searchable files.
    /// 可检索文件数。
    pub searchable_files: u64,
    /// Intentionally skipped files.
    /// 主动跳过的文件数。
    pub skipped_files: u64,
    /// Deleted records retained for reconciliation.
    /// 为协调保留的删除记录数。
    pub deleted_files: u64,
    /// Committed Blob records.
    /// 已提交 Blob 记录数。
    pub blobs: u64,
    /// Persisted content chunks.
    /// 持久化内容代码块数。
    pub chunks: u64,
    /// Persisted content and path vectors.
    /// 持久化内容与路径向量数。
    pub vectors: u64,
    /// Persisted path documents.
    /// 持久化路径文档数。
    pub path_documents: u64,
    /// Persisted exact symbol occurrences.
    /// 持久化精确符号出现次数。
    pub symbols: u64,
}

/// Integrity counters for one active VCE namespace.
/// 单个活动 VCE 命名空间的完整性计数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceIndexIntegritySummary {
    /// Whether the namespace exists.
    /// 命名空间是否存在。
    pub index_exists: bool,
    /// Whether its committed data is searchable.
    /// 已提交数据是否可检索。
    pub searchable: bool,
    /// Expected checkpoint Blob count.
    /// 预期检查点 Blob 数量。
    pub checkpoint_blob_count: u64,
    /// Indexed Blob count.
    /// 已索引 Blob 数量。
    pub indexed_blob_count: u64,
    /// Missing checkpoint Blob count.
    /// 缺失检查点 Blob 数量。
    pub missing_blob_count: u64,
    /// Orphan indexed Blob count.
    /// 孤立已索引 Blob 数量。
    pub orphan_blob_count: u64,
    /// Optional bounded host detail.
    /// 可选的有界宿主详情。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Full management-safe project overview loaded outside the workbench snapshot.
/// 在工作台快照之外加载的完整管理安全项目概览。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectOverviewResponse {
    /// Complete persisted settings.
    /// 完整持久化设置。
    pub settings: VceProjectSettingsSummary,
    /// Current runtime status.
    /// 当前运行时状态。
    pub status: VceProjectStatusSummary,
    /// Host AI capability state when active.
    /// 激活时的宿主 AI 能力状态。
    pub ai: Option<VceAiCapabilitySummary>,
    /// Persisted index counts when active.
    /// 激活时的持久化索引计数。
    pub counts: Option<VceIndexCountsSummary>,
    /// Current namespace integrity when active.
    /// 激活时的当前命名空间完整性。
    pub integrity: Option<VceIndexIntegritySummary>,
}

/// Bounded page request shared by VCE management lists.
/// VCE 管理列表共享的有界分页请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VcePageRequest {
    /// Opaque cursor returned by the previous page.
    /// 上一页返回的不透明游标。
    #[serde(default)]
    pub cursor: Option<String>,
    /// Maximum records to return.
    /// 最大返回记录数。
    pub limit: usize,
}

/// Generic deterministic page returned by VCE management RPCs.
/// VCE 管理 RPC 返回的通用确定性分页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VcePageResponse<T> {
    /// Records in stable order.
    /// 按稳定顺序排列的记录。
    pub items: Vec<T>,
    /// Opaque continuation cursor.
    /// 不透明继续游标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Optional exact total.
    /// 可选精确总数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

/// Project-scoped file inventory request.
/// 项目作用域文件清单请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceFileListRequest {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Optional canonical relative path prefix.
    /// 可选规范相对路径前缀。
    #[serde(default)]
    pub path_prefix: Option<String>,
    /// Optional file state filter: present, skipped, or deleted.
    /// 可选文件状态过滤：present、skipped 或 deleted。
    #[serde(default)]
    pub status: Option<String>,
    /// Whether only rows with auxiliary-channel failures are returned.
    /// 是否只返回存在辅助通道失败的记录。
    #[serde(default)]
    pub auxiliary_failures_only: bool,
    /// Bounded page selection.
    /// 有界分页选择。
    pub page: VcePageRequest,
}

/// One optional index channel state in a file or Blob projection.
/// 文件或 Blob 投影中的单项可选索引通道状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceChannelStateSummary {
    /// Stable channel state.
    /// 稳定通道状态。
    pub state: String,
    /// Optional bounded failure reason.
    /// 可选有界失败原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One file row shown by the code-context manager.
/// 代码上下文管理器显示的一条文件记录。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceFileSummary {
    /// Canonical project-relative path.
    /// 规范项目相对路径。
    pub path: String,
    /// Current Blob identity.
    /// 当前 Blob 身份。
    pub blob_name: Option<String>,
    /// Blob identity in the published checkpoint.
    /// 已发布检查点中的 Blob 身份。
    pub committed_blob_name: Option<String>,
    /// Stable file state.
    /// 稳定文件状态。
    pub status: String,
    /// Workspace generation that last changed the row.
    /// 最近修改该记录的工作区代数。
    pub generation: u64,
    /// Source bytes.
    /// 源码字节数。
    pub size: u64,
    /// Persisted chunk count.
    /// 持久化代码块数。
    pub chunk_count: u64,
    /// Persisted vector count.
    /// 持久化向量数。
    pub vector_count: u64,
    /// Exact symbol occurrence count.
    /// 精确符号出现次数。
    pub symbol_count: u64,
    /// Path semantic channel state.
    /// 路径语义通道状态。
    pub path_channel: VceChannelStateSummary,
    /// Exact symbol channel state.
    /// 精确符号通道状态。
    pub symbol_channel: VceChannelStateSummary,
    /// Latest bounded row error.
    /// 最近一条有界记录错误。
    pub last_error: Option<String>,
}

/// Exact Blob-detail request.
/// 精确 Blob 详情请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceBlobDetailRequest {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Exact content-addressed Blob identity.
    /// 精确内容寻址 Blob 身份。
    pub blob_name: String,
}

/// One bounded source chunk shown in Blob details.
/// Blob 详情中显示的一段有界源码代码块。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceChunkSummary {
    /// Stable Chunk identity.
    /// 稳定代码块身份。
    pub chunk_id: String,
    /// One-based inclusive start line.
    /// 从一开始且包含边界的起始行。
    pub start_line: usize,
    /// One-based inclusive end line.
    /// 从一开始且包含边界的结束行。
    pub end_line: usize,
    /// Unicode-scalar offset within the first line.
    /// 首行内的 Unicode 标量偏移。
    pub start_char_offset: usize,
    /// Stable chunking strategy.
    /// 稳定切块策略。
    pub kind: String,
    /// Optional bounded source preview.
    /// 可选有界源码预览。
    pub preview: Option<String>,
    /// Whether a compatible content vector exists.
    /// 是否存在兼容内容向量。
    pub vector_ready: bool,
}

/// Detailed management-safe state for one Blob.
/// 单个 Blob 的详细管理安全状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceBlobDetailResponse {
    /// Stable Blob identity.
    /// 稳定 Blob 身份。
    pub blob_name: String,
    /// Canonical source path.
    /// 规范源码路径。
    pub path: String,
    /// Workspace generation that produced the Blob.
    /// 生成此 Blob 的工作区代数。
    pub generation: u64,
    /// Structural analyzer identity.
    /// 结构分析器身份。
    pub structure_analyzer_id: Option<String>,
    /// Primary analyzer failure that caused recursive fallback.
    /// 导致递归回退的主分析器失败原因。
    pub structure_fallback_reason: Option<String>,
    /// Persisted chunks in stable source order.
    /// 按稳定源码顺序排列的持久化代码块。
    pub chunks: Vec<VceChunkSummary>,
    /// Path semantic channel state.
    /// 路径语义通道状态。
    pub path_channel: VceChannelStateSummary,
    /// Exact symbol channel state.
    /// 精确符号通道状态。
    pub symbol_channel: VceChannelStateSummary,
}

/// Project-scoped paged operation or integrity request.
/// 项目作用域分页操作或完整性请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VcePagedProjectRequest {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Bounded page selection.
    /// 有界分页选择。
    pub page: VcePageRequest,
}

/// One durable VCE operation shown by the jobs panel.
/// 作业面板显示的一项持久 VCE 操作。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceOperationSummary {
    /// Stable operation identity.
    /// 稳定操作身份。
    pub operation_id: String,
    /// Stable operation kind.
    /// 稳定操作种类。
    pub kind: String,
    /// Stable lifecycle phase.
    /// 稳定生命周期阶段。
    pub phase: String,
    /// Workspace generation being processed.
    /// 正在处理的工作区代数。
    pub generation: u64,
    /// Optional refresh mode.
    /// 可选刷新模式。
    pub refresh_mode: Option<String>,
    /// Optional refresh reason.
    /// 可选刷新原因。
    pub refresh_reason: Option<String>,
    /// Completed work units.
    /// 已完成工作单元。
    pub completed_units: u64,
    /// Total planned work units.
    /// 总计划工作单元。
    pub total_units: u64,
    /// Host retry attempt count.
    /// 宿主重试次数。
    pub attempt: u32,
    /// Whether another event was coalesced.
    /// 是否合并了其他事件。
    pub coalesced: bool,
    /// Optional terminal error.
    /// 可选终止错误。
    pub error: Option<String>,
}

/// One integrity issue reported without storage implementation details.
/// 不含存储实现细节的一项完整性问题。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceIntegrityIssueSummary {
    /// Stable issue identity.
    /// 稳定问题身份。
    pub issue_id: String,
    /// Stable issue kind.
    /// 稳定问题种类。
    pub kind: String,
    /// Optional affected path.
    /// 可选受影响路径。
    pub path: Option<String>,
    /// Optional affected Blob identity.
    /// 可选受影响 Blob 身份。
    pub blob_name: Option<String>,
    /// Management-safe diagnostic.
    /// 管理安全诊断。
    pub message: String,
}

/// Read-only maintenance-plan request.
/// 只读维护计划请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceMaintenancePlanRequest {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Maintenance mode: repair, rebuild, or cleanup_staging.
    /// 维护模式：repair、rebuild 或 cleanup_staging。
    pub mode: String,
}

/// One explicitly confirmable maintenance action.
/// 一项可显式确认的维护操作。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceMaintenanceActionSummary {
    /// Stable idempotent action identity.
    /// 稳定幂等操作身份。
    pub action_id: String,
    /// Human-readable action description.
    /// 人类可读操作说明。
    pub description: String,
    /// Affected Blob identities.
    /// 受影响 Blob 身份。
    pub blob_names: Vec<String>,
    /// Whether execution removes persisted derived data.
    /// 执行是否会删除持久化派生数据。
    pub destructive: bool,
}

/// Read-only maintenance plan returned before any mutation.
/// 在任何修改前返回的只读维护计划。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceMaintenancePlanResponse {
    /// Stable maintenance mode.
    /// 稳定维护模式。
    pub mode: String,
    /// Explicit actions in deterministic order.
    /// 按确定性顺序排列的显式操作。
    pub actions: Vec<VceMaintenanceActionSummary>,
    /// Number of affected Blobs.
    /// 受影响 Blob 数量。
    pub affected_blob_count: u64,
}

/// Guarded execution request copied from one current maintenance plan.
/// 从一份当前维护计划复制的受保护执行请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceMaintenanceExecutionRequest {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Maintenance mode copied from the plan.
    /// 从计划复制的维护模式。
    pub mode: String,
    /// Exact settings revision required by this command.
    /// 此命令要求的精确设置修订号。
    pub expected_settings_revision: u64,
    /// Exact published checkpoint required by this destructive command.
    /// 此破坏性命令要求的精确已发布检查点。
    pub expected_checkpoint_version: u64,
    /// Exact action identities copied from the current read-only plan.
    /// 从当前只读计划复制的精确操作身份。
    pub action_ids: Vec<String>,
}

/// Guarded request targeting one durable or currently running operation.
/// 面向一项持久或当前运行操作的受保护请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceOperationMutationRequest {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Stable operation identity shown by the management surface.
    /// 管理界面显示的稳定操作身份。
    pub operation_id: String,
    /// Exact settings revision required by this command.
    /// 此命令要求的精确设置修订号。
    pub expected_settings_revision: u64,
    /// Exact published checkpoint visible when the command was requested.
    /// 请求命令时可见的精确已发布检查点。
    #[serde(default)]
    pub expected_checkpoint_version: Option<u64>,
}

/// Guarded mutation request for refresh, retry, cancel, rebuild, or maintenance execution.
/// 刷新、重试、取消、重建或维护执行使用的受保护修改请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectMutationRequest {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Exact settings revision required by this command.
    /// 此命令要求的精确设置修订号。
    pub expected_settings_revision: u64,
    /// Exact published checkpoint required by destructive commands.
    /// 破坏性命令要求的精确已发布检查点。
    #[serde(default)]
    pub expected_checkpoint_version: Option<u64>,
    /// Optional action identities copied from a read-only maintenance plan.
    /// 从只读维护计划复制的可选操作身份。
    #[serde(default)]
    pub action_ids: Vec<String>,
}

/// One structured code hit returned to the model-side retrieval tool.
/// 返回给模型侧检索工具的一项结构化代码命中。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceRetrievalHit {
    /// Stable occurrence identity.
    /// 稳定出现位置身份。
    pub id: String,
    /// Source Blob identity.
    /// 源 Blob 身份。
    pub blob_name: String,
    /// Canonical project-relative path.
    /// 规范项目相对路径。
    pub path: String,
    /// One-based inclusive start line.
    /// 从一开始且包含边界的起始行。
    pub start_line: usize,
    /// One-based inclusive end line.
    /// 从一开始且包含边界的结束行。
    pub end_line: usize,
    /// Verbatim bounded source content.
    /// 原样有界源码内容。
    pub content: String,
    /// Final ranking score.
    /// 最终排序分数。
    pub score: f32,
    /// Stable retrieval source channel.
    /// 稳定检索来源通道。
    pub source: String,
    /// Stable semantic hit kind.
    /// 稳定语义命中种类。
    pub kind: String,
}

/// Model-facing result of one project-scoped semantic retrieval.
/// 一次项目作用域语义检索的模型侧结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceRetrievalResponse {
    /// Exact project selected by the host execution context.
    /// 宿主执行上下文选择的精确项目。
    pub project_id: String,
    /// Stable retrieval request identity.
    /// 稳定检索请求身份。
    pub request_id: String,
    /// Settings revision represented by the runtime.
    /// 运行时表示的设置修订号。
    pub settings_revision: u64,
    /// Published checkpoint searched by this immutable engine snapshot.
    /// 此不可变引擎快照检索的已发布检查点。
    pub checkpoint_version: u64,
    /// Classified query intent.
    /// 分类后的查询意图。
    pub intent: String,
    /// Query variants actually searched.
    /// 实际搜索的查询变体。
    pub queries: Vec<String>,
    /// Final structured selected hits.
    /// 最终结构化选中命中。
    pub hits: Vec<VceRetrievalHit>,
    /// Line-numbered bounded code context.
    /// 带行号的有界代码上下文。
    pub formatted_context: String,
    /// Optional stages bypassed safely.
    /// 安全绕过的可选阶段。
    pub degraded_features: Vec<String>,
    /// Detailed bounded degradation diagnostics.
    /// 详细有界降级诊断。
    pub degradations: Vec<String>,
}

/// Revisioned project event payload used by WebUI reconnect and invalidation logic.
/// WebUI 重连与失效逻辑使用的带修订项目事件载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectChangedEvent {
    /// Exact workbench project identity.
    /// 精确工作台项目身份。
    pub project_id: String,
    /// Monotonic project settings revision.
    /// 单调项目设置修订号。
    pub settings_revision: u64,
    /// Current lifecycle state.
    /// 当前生命周期状态。
    pub lifecycle: VceProjectLifecycleState,
    /// Current published checkpoint version.
    /// 当前已发布检查点版本。
    pub checkpoint_version: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies settings update JSON excludes host-owned revision and migration fields.
    /// 验证设置更新 JSON 不包含宿主持有的修订与迁移字段。
    #[test]
    fn settings_input_has_no_host_owned_fields() {
        let value = serde_json::to_value(VceProjectSettingsInput {
            enabled: true,
            max_file_bytes: 1_048_576,
            extra_ignore_patterns: vec![String::from("generated/**")],
            allow_query_rewrite: false,
            allow_rerank: true,
            embedding_max_concurrency: 4,
            query_max_concurrency: 3,
            max_results: 10,
            max_context_chars: 32_000,
        })
        .expect("serialize VCE settings input");

        assert!(value.get("revision").is_none());
        assert!(value.get("migratedFromCodebaseMemory").is_none());
    }

    /// Verifies browser-facing lifecycle values remain stable snake-case identifiers.
    /// 验证浏览器侧生命周期值保持稳定蛇形标识。
    #[test]
    fn lifecycle_serialization_is_stable() {
        assert_eq!(
            serde_json::to_value(VceProjectLifecycleState::BlockedConfiguration)
                .expect("serialize VCE lifecycle"),
            json!("blocked_configuration")
        );
    }
}
