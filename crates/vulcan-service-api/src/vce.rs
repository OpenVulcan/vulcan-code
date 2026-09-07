use super::*;

/// Event emitted when one project's code-semantic lifecycle or checkpoint changes.
/// 单个项目的代码语义生命周期或检查点变化时发送的事件。
pub const VCE_PROJECT_CHANGED_EVENT: &str = "vce.project.changed";

/// Durable job receipt notification shared by all conversations in the owning project.
/// 持久作业回执通知，由所属项目全部会话共享。
pub const VCE_JOB_CHANGED_EVENT: &str = "vce.job.changed";

/// Explicit indexing commands accepted without implicit rebuild or compatibility approval.
/// 不包含隐式重建或兼容批准的显式索引命令。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VceJobAction {
    /// Human-authorized first index initialization.
    /// 人为授权首次索引初始化。
    Initialize,
    /// Human-authorized synchronization of an already initialized index.
    /// 人为授权同步已初始化索引。
    Synchronize,
}

/// Durable job categories, including independently confirmed maintenance workflows.
/// 持久作业分类，包括独立确认的维护流程。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VceJobKindSummary {
    /// Initialize an index for the first time.
    /// 首次初始化索引。
    Initialize,
    /// Synchronize source changes.
    /// 同步源码变化。
    Synchronize,
    /// Rebuild an explicitly confirmed index contract.
    /// 重建已明确确认的索引契约。
    Rebuild,
}

/// Job execution state; it is never the authority for query eligibility.
/// 作业执行状态；绝不作为查询资格的权威值。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VceJobStateSummary {
    /// Receipt persisted before resource admission.
    /// 资源准入前已持久化回执。
    Queued,
    /// The admitted worker is executing.
    /// 已准入工作线程正在执行。
    Running,
    /// Cancellation accepted; bounded work and cleanup are settling.
    /// 取消已受理；有界处理与清理正在收敛。
    Cancelling,
    /// Work and authoritative checkpoint persistence succeeded.
    /// 工作与权威检查点持久化均成功。
    Succeeded,
    /// Execution failed with an explicit diagnostic.
    /// 执行失败且带明确诊断。
    Failed,
    /// Cancellation and unpublished staging cleanup completed.
    /// 取消及未发布暂存清理已完成。
    Cancelled,
    /// A previous process exited without its terminal receipt.
    /// 先前进程退出时未写入终态回执。
    Interrupted,
}

/// Real phase metadata without invented global progress percentages.
/// 真实阶段元数据，不包含虚构整体进度百分比。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceJobProgressSummary {
    /// Exact core phase: scanning, reconciling, chunking, embedding, writing, committing, complete.
    /// 精确核心阶段：扫描、核对、切块、向量化、写入、提交、完成。
    pub phase: String,
    /// Exact child operation identity that scopes phase counters.
    /// 限定阶段计数的精确子操作身份。
    pub operation_id: String,
    /// Exact unit: files, blobs, batches, or none.
    /// 精确单位：文件、块对象、批次或无单位。
    pub unit: String,
    /// Confirmed completed units.
    /// 已确认完成的单位数。
    pub completed: u64,
    /// Known total; absent means indeterminate, not zero.
    /// 已知总量；缺失表示未确定，而不是零。
    pub total: Option<u64>,
    /// Optional source-relative path, never source content or credentials.
    /// 可选源码相对路径，绝不含源码正文或凭据。
    pub current_path: Option<String>,
}

/// Management-safe stable job receipt returned immediately after durable admission.
/// 持久准入后立即返回的管理安全稳定作业回执。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceJobSummary {
    /// Prior durable job explicitly retried; unrelated later synchronizations have no retry origin.
    /// 明确重试的先前持久作业；无关的后续同步没有重试来源。
    #[serde(default)]
    pub retry_of_job_id: Option<String>,
    /// Exact owning workbench project.
    /// 精确所属工作台项目。
    pub project_id: String,
    /// Stable identity used for querying and cancellation.
    /// 用于查询与取消的稳定身份。
    pub job_id: String,
    /// Explicit operation category.
    /// 明确操作分类。
    pub kind: VceJobKindSummary,
    /// Durable execution state.
    /// 持久执行状态。
    pub state: VceJobStateSummary,
    /// Monotonic sequence allowing clients to discard reordered events.
    /// 允许客户端丢弃乱序事件的单调序号。
    pub sequence: u64,
    /// Admission wall-clock milliseconds for display.
    /// 用于显示的准入墙上时钟毫秒数。
    pub created_at_unix_ms: u64,
    /// Latest acknowledged receipt timestamp.
    /// 最近已确认回执时间戳。
    pub updated_at_unix_ms: u64,
    /// Prior checkpoint that remains queryable during ordinary updates.
    /// 普通更新期间仍可查询的先前检查点。
    pub source_checkpoint_version: Option<u64>,
    /// New authoritative checkpoint present only after success.
    /// 仅成功后出现的新权威检查点。
    pub result_checkpoint_version: Option<u64>,
    /// Real phase observation, absent while still queued.
    /// 真实阶段观察，仍在排队时缺失。
    pub progress: Option<VceJobProgressSummary>,
    /// Whether one subsequent manual synchronization was coalesced.
    /// 是否合并了一个后续手动同步。
    pub followup_requested: bool,
    /// Bounded execution or cleanup diagnostic.
    /// 有界执行或清理诊断。
    pub error: Option<String>,
}

/// Exact human job command with optimistic guards and stable retry identity.
/// 带乐观保护与稳定重试身份的精确人为作业命令。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VceJobSubmitRequest {
    /// Optional exact prior durable job to retry after current-authority validation.
    /// 当前权威校验后要重试的可选精确先前持久作业。
    #[serde(default)]
    pub retry_of_job_id: Option<String>,
    /// Owning workbench project selected by the user.
    /// 用户选择的所属工作台项目。
    pub project_id: String,
    /// Explicit first initialization or synchronization action.
    /// 明确首次初始化或同步操作。
    pub action: VceJobAction,
    /// Stable client request key reused on transport retry, not on a later new click.
    /// 传输重试时复用的稳定客户端请求键，而非后续新点击的键。
    pub idempotency_key: String,
    /// Exact observed project settings revision.
    /// 精确观察到的项目设置修订。
    pub expected_settings_revision: u64,
    /// Exact observed checkpoint; explicit null is required before initialization.
    /// 精确观察到的检查点；初始化前明确为空。
    pub expected_checkpoint_version: Option<u64>,
}

/// Read the latest project job or one exact stable identity, even when models are unavailable.
/// 读取项目最近作业或精确稳定身份，即使模型不可用也可读取。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VceJobQueryRequest {
    /// Exact owning project.
    /// 精确所属项目。
    pub project_id: String,
    /// Exact job; absent selects the latest admitted receipt.
    /// 精确作业；缺失时选择最近准入回执。
    pub job_id: Option<String>,
}

/// Cancel only the exact displayed job; project configuration cannot retarget cancellation.
/// 仅取消精确展示作业；项目配置不能重新指定取消对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VceJobCancelRequest {
    /// Exact owning project.
    /// 精确所属项目。
    pub project_id: String,
    /// Exact stable job identity, never a child-operation prefix.
    /// 精确稳定作业身份，绝非子操作前缀。
    pub job_id: String,
}

/// Exact project-level update policy accepted by settings and rendered by clients.
/// 设置接受且客户端展示的精确项目级更新策略。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VceIndexUpdateMode {
    /// Only explicit human sync requests schedule updates.
    /// 仅明确的人为同步请求调度更新。
    Manual,
    /// Authorized file events may schedule incremental updates after initialization.
    /// 已初始化后，获授权文件事件可调度增量更新。
    Automatic,
}

/// Persisted project-owned settings for the native code-semantic runtime.
/// 原生代码语义运行时使用的项目自有持久化设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectSettingsSummary {
    /// Whether semantic indexing is enabled for this project.
    /// 此项目是否启用语义索引。
    pub enabled: bool,
    /// Persisted project-wide update mode shared by every conversation.
    /// 所有会话共享的持久化项目级更新模式。
    pub update_mode: VceIndexUpdateMode,
    /// Minimum automatic update admission interval in seconds.
    /// 自动更新受理的最低间隔秒数。
    pub auto_update_min_interval_seconds: u32,
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
    /// Exact project update policy; not a conversation-level preference.
    /// 精确项目更新策略，不是会话级偏好。
    pub update_mode: VceIndexUpdateMode,
    /// Automatic update interval in seconds; manual sync bypasses this interval.
    /// 自动更新间隔秒数；手动同步绕过此间隔。
    pub auto_update_min_interval_seconds: u32,
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
    /// The enabled project has no initialized committed index.
    /// 已启用项目尚无已初始化的已提交索引。
    Uninitialized,
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
    /// Legacy compatibility value retained for older clients; the current host no longer emits it.
    /// 为旧客户端保留的兼容值；当前宿主不再发送该状态。
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
    /// Whether required query rewriting is available.
    /// 必需查询改写是否可用。
    pub query_rewrite_available: bool,
    /// Whether required reranking is available.
    /// 必需重排是否可用。
    pub rerank_available: bool,
}

/// Readiness of one required VCE public-AI route.
/// 一条 VCE 必需公共 AI 路由的就绪状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceConfigurationCapabilityStatus {
    /// Whether an exact route is persisted for this capability.
    /// 是否已为此能力持久化一条精确路由。
    pub configured: bool,
    /// Whether the persisted route resolves to an enabled compatible model now.
    /// 已持久化路由当前是否能解析到已启用且兼容的模型。
    pub available: bool,
    /// Bounded diagnostic explaining why the route is unavailable.
    /// 解释路由为何不可用的有界诊断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
}

/// Fixed reasoning policy used by VCE machine retrieval stages.
/// VCE 机器检索阶段使用的固定推理策略。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VceReasoningPolicy {
    /// Query rewriting must execute with model reasoning disabled.
    /// 查询改写必须在关闭模型推理的状态下执行。
    FixedOff,
}

/// Authoritative global prerequisite state for every VCE project and tool entry.
/// 每个 VCE 项目与工具入口共同使用的权威全局前置状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceGlobalConfigurationStatus {
    /// Whether every required route is configured, current, and contract-compatible.
    /// 每条必需路由是否均已配置、当前有效且契约兼容。
    pub ready: bool,
    /// Stable public-AI consumer profile owned by VCE.
    /// VCE 持有的稳定公共 AI 消费者配置档。
    pub consumer_profile_id: String,
    /// Stable LLM purpose used only for query rewriting.
    /// 仅用于查询改写的稳定 LLM 用途。
    pub query_rewrite_purpose_id: String,
    /// Non-configurable reasoning policy enforced by the host.
    /// 由宿主强制执行且不可配置的推理策略。
    pub reasoning_policy: VceReasoningPolicy,
    /// Required document-and-query embedding route status.
    /// 必需的文档与查询向量路由状态。
    pub embedding: VceConfigurationCapabilityStatus,
    /// Required no-reasoning LLM query-rewrite route status.
    /// 必需的无推理 LLM 查询改写路由状态。
    pub query_rewrite: VceConfigurationCapabilityStatus,
    /// Required non-truncating rerank route status.
    /// 必需的不截断重排路由状态。
    pub rerank: VceConfigurationCapabilityStatus,
    /// Ordered bounded diagnostics for all unavailable prerequisites.
    /// 所有不可用前置条件的有序有界诊断。
    pub issues: Vec<String>,
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
    /// Opaque published-index identity required by subsequent content management reads.
    /// 后续内容管理读取必需的不透明已发布索引身份。
    pub read_identity: Option<String>,
    /// Authoritative global configuration prerequisite state.
    /// 权威全局配置前置状态。
    pub configuration: VceGlobalConfigurationStatus,
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

/// Source observation evidence, independent from query admission and successful update time.
/// 源码观察证据，独立于查询准入及成功更新时间。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceSourceFreshnessSummary {
    /// Exact live observation exists; does not alone prove the index is current.
    /// 存在精确存活观察；单凭此项不能证明索引最新。
    pub watching: bool,
    /// Delivered changes remain outside the last acknowledged scan boundary.
    /// 已投递变化仍处于最近确认扫描边界之外。
    pub pending_changes: bool,
    /// Continuous evidence is absent or broken; the next admitted update needs a full manifest scan.
    /// 连续证据缺失或中断；下一次准入更新需要完整清单扫描。
    pub reconciliation_required: bool,
}

/// Lightweight project-wide activity for composer controls and the floating status panel.
/// 输入区控件与浮动状态面板使用的轻量项目级活动。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VceProjectActivityResponse {
    /// Authoritative global model availability, independent from project enablement.
    /// 权威全局模型可用性，独立于项目启用。
    pub configuration: VceGlobalConfigurationStatus,
    /// Exact persisted project settings and optimistic revision.
    /// 精确持久项目设置与乐观修订。
    pub settings: VceProjectSettingsSummary,
    /// Committed runtime metadata without taking a running query's engine lock.
    /// 已提交运行时元数据，不获取执行中查询的引擎锁。
    pub status: VceProjectStatusSummary,
    /// Current authoritative tool/query admission; indexing activity does not veto a valid checkpoint.
    /// 当前权威工具及查询准入；索引活动不否决有效检查点。
    pub query_available: bool,
    /// Last acknowledged successful content update; unknown for legacy indexes without job history.
    /// 最近确认成功内容更新时间；没有作业历史的旧索引为未知。
    pub last_successful_update_unix_ms: Option<u64>,
    /// Read-only source evidence; never revokes an existing compatible checkpoint.
    /// 只读源码证据；绝不撤销既有兼容检查点。
    pub source_freshness: VceSourceFreshnessSummary,
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
    /// Exact read identity obtained from the displayed management overview.
    /// 从当前显示的管理概览取得的精确读取身份。
    pub expected_read_identity: String,
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
    /// Exact read identity of the file list from which this detail was selected.
    /// 选择此详情时所在文件列表的精确读取身份。
    pub expected_read_identity: String,
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
    /// Required for checkpoint integrity; absent for live operation history.
    /// 检查点完整性读取必需；实时操作历史不携带此值。
    pub expected_read_identity: Option<String>,
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
    /// Exact embedding identity shown by this preview, retained through human confirmation.
    /// 此预览展示的精确向量身份，保留到人为确认。
    pub embedding_fingerprint: String,
    /// Explicit target dimension; no provider-default dimension is inferred.
    /// 明确目标维度；不推断供应商默认维度。
    pub embedding_dimension: u32,
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
    /// Embedding identity copied from the confirmed preview, never replaced with the latest route.
    /// 从已确认预览复制的向量身份，绝不替换为最新路由。
    pub expected_embedding_fingerprint: String,
    /// Stable identity of this exact confirmed submission, retained across transport retries.
    /// 此精确确认提交的稳定身份，在传输重试期间保留。
    pub idempotency_key: String,
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

/// Maintenance either admits asynchronous indexing or completes bounded cleanup without indexing.
/// 维护要么准入异步索引，要么完成不执行索引的有界清理。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VceMaintenanceExecutionResponse {
    /// Durable receipt for confirmed rebuild or repair; completion is observed through normal job queries.
    /// 已确认重建或修复的持久回执；完成状态通过普通作业查询观察。
    Job {
        /// Exact admitted project job, boxed without changing its JSON representation.
        /// 精确准入的项目作业，装箱不改变其 JSON 表示。
        job: Box<VceJobSummary>,
    },
    /// Cleanup-only result with no source scanning or model invocation.
    /// 仅清理结果，不扫描源码、不调用模型。
    Cleaned {
        /// Authoritative overview after cleanup; indirection keeps every response variant bounded on the stack.
        /// 清理后的权威概览；间接存储使各响应分支的栈占用保持有界。
        overview: Box<VceProjectOverviewResponse>,
    },
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
    /// Zero-based Unicode-scalar offset within the first source line, preserved from the core hit.
    /// 首行内从零开始的 Unicode 标量偏移，原样保留核心命中的值。
    pub start_char_offset: usize,
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

    /// Require explicit content-read identity on the wire while keeping job history independent.
    /// 在线协议中要求明确内容读取身份，同时保持作业历史独立。
    /// No parameters or I/O; checks required fields and exact round-trip identity.
    /// 无参数和输入输出；检查必填字段与精确往返身份。
    #[test]
    fn content_management_requests_preserve_required_read_identity() {
        // A file page cannot silently default to the latest publication.
        // 文件分页不能静默默认读取最新发布。
        let missing = json!({"projectId": "project", "page": {"limit": 25}});
        assert!(serde_json::from_value::<VceFileListRequest>(missing).is_err());
        assert!(
            serde_json::from_value::<VceBlobDetailRequest>(json!({
                "projectId": "project", "blobName": "blob"
            }))
            .is_err()
        );
        // Detail identity survives deserialization and serialization without deriving a new value.
        // 详情身份经过反序列化与序列化保持不变，不派生新值。
        let wire =
            json!({"projectId": "project", "blobName": "blob", "expectedReadIdentity": "space:7"});
        let detail: VceBlobDetailRequest =
            serde_json::from_value(wire.clone()).expect("decode exact identity");
        assert_eq!(
            serde_json::to_value(detail).expect("encode exact identity"),
            wire
        );
        // Shared paging permits absence only because live operation history has no content snapshot.
        // 共享分页允许空值，仅因为实时操作历史不具备内容快照。
        let operations: VcePagedProjectRequest = serde_json::from_value(json!({
            "projectId": "project", "page": {"limit": 25}
        }))
        .expect("decode live history");
        assert!(operations.expected_read_identity.is_none());
    }

    /// Verifies settings update JSON excludes host-owned revision and migration fields.
    /// 验证设置更新 JSON 不包含宿主持有的修订与迁移字段。
    #[test]
    fn settings_input_has_no_host_owned_fields() {
        let value = serde_json::to_value(VceProjectSettingsInput {
            enabled: true,
            update_mode: VceIndexUpdateMode::Manual,
            auto_update_min_interval_seconds: 300,
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

    /// Verify boxed maintenance results preserve the exact browser JSON and bounded enum layout.
    /// 验证装箱维护结果保留精确浏览器 JSON 及有界枚举布局。
    /// No parameters. Asserts round-trip fields and size without creating any runtime or storage.
    /// 无参数。不创建任何运行时或存储，断言往返字段及大小。
    #[test]
    fn maintenance_response_boxing_preserves_wire_fields() {
        // wire is the existing tagged response, not a new wrapper around its job field.
        // wire 为既有带标签响应，不为作业字段新增包装层。
        let wire = json!({
            "kind": "job",
            "job": {
                "retryOfJobId": null, "projectId": "project", "jobId": "job",
                "kind": "rebuild", "state": "queued", "sequence": 1,
                "createdAtUnixMs": 10, "updatedAtUnixMs": 10,
                "sourceCheckpointVersion": 1, "resultCheckpointVersion": null,
                "progress": null, "followupRequested": false, "error": null
            }
        });
        // response decodes exactly the fields previously consumed by the browser.
        // response 精确解码此前由浏览器消费的字段。
        let response: VceMaintenanceExecutionResponse =
            serde_json::from_value(wire.clone()).expect("decode maintenance response");
        assert!(matches!(
            response,
            VceMaintenanceExecutionResponse::Job { .. }
        ));
        assert_eq!(
            serde_json::to_value(response).expect("encode maintenance response"),
            wire
        );
        assert!(
            std::mem::size_of::<VceMaintenanceExecutionResponse>()
                <= 3 * std::mem::size_of::<usize>()
        );
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

    /// Verifies the non-configurable VCE reasoning policy keeps its stable wire value.
    /// 验证不可配置的 VCE 推理策略保持稳定线协议值。
    #[test]
    fn fixed_off_reasoning_policy_serialization_is_stable() {
        assert_eq!(
            serde_json::to_value(VceReasoningPolicy::FixedOff)
                .expect("serialize VCE reasoning policy"),
            json!("fixed_off")
        );
    }
}
